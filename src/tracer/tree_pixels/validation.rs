//! Opt-in native temporal regression using the production tree pixel shader.
//! Replay-owned geometry/camera/light buffers keep wind, DDGI and frame pacing
//! out of the measurement; no game state or saved configuration is modified.
use super::*;
use crate::generated::gpu_structs::{CameraInfo, VoxelColors};
use crate::resource::{ResourceContainer, ResourceLookup};
use bytemuck::Zeroable;
use re_flora_vkn::{ComputePipeline, ShaderModule};
use std::collections::HashMap;

struct Inputs(HashMap<&'static str, Buffer>);
impl ResourceContainer for Inputs {
    fn resolve_resource(&self, name: &str) -> ResourceLookup<'_> {
        self.0.get(name).map_or(ResourceLookup::Missing, |buffer| {
            ResourceLookup::Unique(DescriptorResource::Buffer(buffer))
        })
    }
}

impl Tracer {
    pub fn validate_tree_pixel_camera_motion(&self) -> Result<()> {
        let context = &self.vulkan_ctx;
        let device = context.device();
        let alloc = self.allocator.clone();
        let storage = |bytes: &[u8]| -> Result<Buffer> {
            let buffer = Buffer::new_sized(
                device.clone(),
                alloc.clone(),
                BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
                MemoryLocation::CpuToGpu,
                bytes.len() as u64,
            );
            buffer.fill_range_with_raw_u8(0, bytes)?;
            Ok(buffer)
        };
        let points = [
            Vec3::new(-0.8, -0.8, 0.),
            Vec3::new(0.8, -0.8, 0.),
            Vec3::new(-0.8, 0.8, 0.),
            Vec3::new(0.8, 0.8, 0.),
        ];
        let scene = tree_scene::TreeScene::new(&[0, 1, 2, 2, 1, 3], &points)?;
        let surface: Vec<[f32; 4]> = points
            .iter()
            .flat_map(|p| [p.extend(0.).to_array(), [0., 0., 1., 0.]])
            .collect();
        let camera = Buffer::new_uniform::<CameraInfo>(device.clone(), alloc.clone());
        let palette = Buffer::new_uniform::<VoxelColors>(device.clone(), alloc.clone());
        palette.fill_uniform(&VoxelColors {
            cherry_wood_color: [1.; 3],
            ..VoxelColors::zeroed()
        })?;
        // Reserve the largest replay grid once; each dispatch/readback uses
        // only its actual extent. No production frame or allocator is changed.
        let bytes = 256 * 256 * 16;
        let tile = Buffer::new_sized(
            device.clone(),
            alloc.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC,
            ),
            MemoryLocation::GpuOnly,
            bytes,
        );
        let readback = Buffer::new_sized(
            device.clone(),
            alloc.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::TRANSFER_DST),
            MemoryLocation::GpuToCpu,
            bytes,
        );
        let inputs = Inputs(HashMap::from([
            ("camera_info", camera),
            ("voxel_colors", palette),
            ("tree_pixel_tiles", tile),
            (
                "tree_attachment_keys",
                storage(bytemuck::bytes_of(&[0u32; 4]))?,
            ),
            (
                "tree_attachment_poses",
                storage(bytemuck::bytes_of(&[0f32; 8]))?,
            ),
            (
                "tree_scene_info",
                storage(bytemuck::bytes_of(&[1u32, scene.nodes.len() as u32, 0, 4]))?,
            ),
            (
                "tree_scene_nodes",
                storage(bytemuck::cast_slice(&scene.nodes))?,
            ),
            (
                "tree_scene_primitives",
                storage(bytemuck::cast_slice(&scene.primitives))?,
            ),
            (
                "tree_scene_vertices",
                storage(bytemuck::cast_slice(&surface))?,
            ),
            (
                "raster_tree_light_cache",
                storage(bytemuck::cast_slice(&[
                    [0f32; 4],
                    [2., 2., 2., 0.],
                    [1., 1., 1., 0.],
                    [3., 3., 3., 0.],
                ]))?,
            ),
        ]));
        let pool = DescriptorPool::new(device)?;
        let shader = ShaderModule::from_precompiled(device, "shader/trees/tree_pixel.comp", "main")
            .map_err(anyhow::Error::msg)?;
        let pipeline = ComputePipeline::new(device, &shader, &pool, &[&inputs]);
        let sample = |angle: f32, grid: PixelGrid, motion: &str| -> Result<Vec<[f32; 4]>> {
            let band = grid.bands()[0];
            let bytes = grid.width as u64 * grid.height as u64 * 16;
            let (eye, front) = match motion {
                "yaw" => (
                    Vec3::new(0., 0., 2.),
                    glam::Quat::from_rotation_y(angle) * -Vec3::Z,
                ),
                "pitch" => (
                    Vec3::new(0., 0., 2.),
                    glam::Quat::from_rotation_x(angle) * -Vec3::Z,
                ),
                "translation" => (Vec3::new(angle, 0., 2.), -Vec3::Z),
                _ => unreachable!(),
            };
            let view = Mat4::look_at_rh(eye, eye + front, Vec3::Y);
            let proj = crate::gameplay::Camera::calculate_proj_mat(60., 1., 0.01, 10.);
            let vp = proj * view;
            inputs.0["camera_info"].fill_uniform(&CameraInfo {
                pos: eye.extend(1.).to_array(),
                view_mat: view.to_cols_array_2d(),
                view_mat_inv: view.inverse().to_cols_array_2d(),
                proj_mat: proj.to_cols_array_2d(),
                proj_mat_inv: proj.inverse().to_cols_array_2d(),
                view_proj_mat: vp.to_cols_array_2d(),
                view_proj_mat_inv: vp.inverse().to_cols_array_2d(),
            })?;
            execute_one_time_gpu_job(
                device,
                context.command_pool(),
                &context.get_general_queue(),
                |cmd| {
                    pipeline.record(
                        cmd,
                        Extent3D::new(grid.width, grid.height, 1),
                        Some(bytemuck::bytes_of(&grid.push(band))),
                    );
                    inputs.0["tree_pixel_tiles"].record_copy_to_buffer(cmd, &readback, bytes, 0, 0);
                    cmd.use_buffer(&readback, BufferUse::HostRead);
                },
            );
            Ok(readback
                .read_back_range(0, bytes)?
                .chunks_exact(16)
                .map(bytemuck::pod_read_unaligned)
                .collect())
        };
        for (geometry, transform) in [
            ("front", Mat4::IDENTITY),
            ("oblique", Mat4::from_rotation_y(0.9)),
            ("split_quad", Mat4::from_rotation_y(0.9)),
            (
                "near_clipped",
                Mat4::from_translation(Vec3::new(0., 0., 1.94)) * Mat4::from_rotation_y(1.0),
            ),
        ] {
            let moved = points.map(|p| transform.transform_point3(p));
            let indices: &[u32] = if geometry == "split_quad" {
                &[0, 1, 2, 2, 1, 3]
            } else {
                &[0, 1, 2]
            };
            let scene = tree_scene::TreeScene::new(indices, &moved)?;
            inputs.0["tree_scene_info"].fill_range_with_raw_u8(
                0,
                bytemuck::bytes_of(&[1u32, scene.nodes.len() as u32, 0, 4]),
            )?;
            inputs.0["tree_scene_primitives"]
                .fill_range_with_raw_u8(0, bytemuck::cast_slice(&scene.primitives))?;
            let surface: Vec<[f32; 4]> = moved
                .iter()
                .flat_map(|p| [p.extend(0.).to_array(), [0., 0., 1., 0.]])
                .collect();
            inputs.0["tree_scene_nodes"]
                .fill_range_with_raw_u8(0, bytemuck::cast_slice(&scene.nodes))?;
            inputs.0["tree_scene_vertices"]
                .fill_range_with_raw_u8(0, bytemuck::cast_slice(&surface))?;
            for size in [1, 4, 16] {
                let grid = PixelGrid::new([256, 256], size)?;
                for motion in ["yaw", "pitch", "translation"] {
                    let initial = sample(-0.006, grid, motion)?;
                    anyhow::ensure!(
                        initial == sample(-0.006, grid, motion)?,
                        "stationary tree pixel shader is nondeterministic"
                    );
                    let mut previous = initial;
                    let mut worst = 0f32;
                    let mut worst_pixel = 0usize;
                    let mut comparisons = 0;
                    let mut coverage_changes = 0;
                    let mut total_change = 0f64;
                    for step in 1..=120 {
                        let next = sample(-0.006 + step as f32 * 0.0001, grid, motion)?;
                        for (pixel, (a, b)) in previous.iter().zip(&next).enumerate() {
                            anyhow::ensure!(
                                b.iter().all(|x| x.is_finite()) && (0.0..=1.0).contains(&b[3]),
                                "invalid motion color/depth"
                            );
                            if (a[3] < 1.) != (b[3] < 1.) {
                                coverage_changes += 1;
                            }
                            if a[3] < 1. && b[3] < 1. {
                                comparisons += 1;
                                let delta = (a[0] - b[0]).abs();
                                total_change += delta as f64;
                                if delta > worst {
                                    worst = delta;
                                    worst_pixel = pixel;
                                }
                            }
                        }
                        previous = next;
                    }
                    if geometry == "split_quad" {
                        // The same planar surface must not change appearance
                        // when its internal diagonal (and BVH order) changes.
                        let alternate = tree_scene::TreeScene::new(&[0, 1, 3, 0, 3, 2], &moved)?;
                        for angle in [-0.006, 0., 0.006] {
                            let original = sample(angle, grid, motion)?;
                            inputs.0["tree_scene_nodes"].fill_range_with_raw_u8(
                                0,
                                bytemuck::cast_slice(&alternate.nodes),
                            )?;
                            inputs.0["tree_scene_primitives"].fill_range_with_raw_u8(
                                0,
                                bytemuck::cast_slice(&alternate.primitives),
                            )?;
                            let retriangulated = sample(angle, grid, motion)?;
                            inputs.0["tree_scene_nodes"]
                                .fill_range_with_raw_u8(0, bytemuck::cast_slice(&scene.nodes))?;
                            inputs.0["tree_scene_primitives"].fill_range_with_raw_u8(
                                0,
                                bytemuck::cast_slice(&scene.primitives),
                            )?;
                            for (pixel, (a, b)) in original.iter().zip(&retriangulated).enumerate()
                            {
                                anyhow::ensure!((a[3]<1.)==(b[3]<1.) && (a[0]-b[0]).abs()<0.0001 && (a[3]-b[3]).abs()<0.00001,
                                    "coverage depends on internal triangulation: size={size} motion={motion} angle={angle} pixel={pixel} original={a:?} alternate={b:?}");
                            }
                        }
                    }
                    log::info!("[TREE][MOTION_VALIDATE] geometry={geometry} size={size} motion={motion} repeat=identical frames=121 step=0.0001 comparisons={comparisons} max_stable_coverage_color_step={worst} pixel={worst_pixel} coverage_changes={coverage_changes}");
                    anyhow::ensure!(
                        comparisons > 1000 && total_change > 0.01,
                        "motion fixture missed the surface or ignored the current camera"
                    );
                    anyhow::ensure!(worst<0.01,"camera motion makes a continuously covered wood cell flash: geometry={geometry} size={size} motion={motion} delta={worst} pixel={worst_pixel}");
                }
            }
        }
        log::info!("[TREE][MOTION_VALIDATE] PASS cases=36 current_camera=true continuous_edge_material=true triangulation=true clipping=true sizes=1,4,16");
        Ok(())
    }
}
