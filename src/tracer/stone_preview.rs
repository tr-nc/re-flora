//! Native dual adapters for one immutable Stone Model. Every submitted frame
//! retains its source/volume generation until that acquired frame slot's fence.
//! No terrain writes, standalone pixel grid, model_mesh.slang changes or dithering.
mod voxel;
use crate::stone_models::{generate, voxel as plan, StoneMesh, StoneSpec};
use anyhow::{ensure, Result};
use bytemuck::{Pod, Zeroable};
use glam::{Quat, Vec3};
use re_flora_vkn::{
    vk, Allocator, Buffer, BufferUsage, BufferUse, CommandBuffer, ComputePipeline,
    DescriptorResource, Device, Extent3D, GraphicsPipeline, MemoryLocation,
    PreparedDrawDescriptors, PushConstantInfo, VulkanContext,
};
use std::sync::Arc;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Triangle {
    a: [f32; 4],
    e1: [f32; 4],
    e2: [f32; 4],
    normal: [f32; 4],
    color: [f32; 4],
}
fn triangles(mesh: &StoneMesh) -> Vec<Triangle> {
    mesh.triangles()
        .enumerate()
        .map(|(i, t)| Triangle {
            a: t[0].extend(0.).to_array(),
            e1: (t[1] - t[0]).extend(0.).to_array(),
            e2: (t[2] - t[0]).extend(0.).to_array(),
            normal: mesh.normals[i].extend(0.).to_array(),
            color: mesh.colors[i].extend(1.).to_array(),
        })
        .collect()
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Pose {
    translation: [f32; 4],
    rotation: [f32; 4],
    chunk_origin: [f32; 4],
    metadata: [u32; 4],
}
impl Pose {
    fn new(request: Request, offsets: [u32; 2]) -> Self {
        Self {
            translation: request.base.extend(0.).to_array(),
            rotation: Quat::from_rotation_y(request.yaw.to_radians()).to_array(),
            chunk_origin: plan::CHUNK_ORIGIN.extend(plan::CHUNK_SIZE).to_array(),
            metadata: [
                offsets[0],
                offsets[1],
                voxel::material(request.spec.kind),
                0,
            ],
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Request {
    pub spec: StoneSpec,
    pub direct: bool,
    pub base: Vec3,
    pub yaw: f32,
}
struct Source {
    model: StoneMesh,
    triangles: Buffer,
    indices: Buffer,
}
impl Source {
    fn new(device: &Device, allocator: &Allocator, spec: StoneSpec) -> Result<Self> {
        let model = generate(spec)?;
        if std::env::var_os("RE_FLORA_STONE_REVIEW").is_some() {
            let directory = re_flora_vkn::project_root().join("target/stone-native");
            std::fs::create_dir_all(&directory)?;
            std::fs::write(
                directory.join(format!("{:?}-{}.obj", spec.kind, spec.seed)),
                model.obj(),
            )?;
        }
        let data = triangles(&model);
        let indices: Vec<u32> = (0..data.len() as u32 * 3).collect();
        let upload = |bytes: &[u8], usage| -> Result<Buffer> {
            let buffer = Buffer::try_new_sized(
                device.clone(),
                allocator.clone(),
                BufferUsage::from_flags(usage),
                MemoryLocation::CpuToGpu,
                bytes.len() as u64,
            )?;
            buffer.fill_range_with_raw_u8(0, bytes)?;
            Ok(buffer)
        };
        Ok(Self {
            triangles: upload(
                bytemuck::cast_slice(&data),
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )?,
            indices: upload(
                bytemuck::cast_slice(&indices),
                vk::BufferUsageFlags::INDEX_BUFFER,
            )?,
            model,
        })
    }
}
#[derive(Default)]
struct Frame {
    source: Option<Arc<Source>>,
    volume: Option<Arc<voxel::Volume>>,
    request: Option<Request>,
}
pub(super) struct PreparedDraw {
    pipeline: GraphicsPipeline,
    descriptors: PreparedDrawDescriptors,
    source: Arc<Source>,
    push: PushConstantInfo,
}
impl PreparedDraw {
    pub fn record(&self, cmd: &CommandBuffer) {
        cmd.bind_index_buffer_u32(&self.source.indices);
        self.pipeline.record_indexed_with_prepared_descriptors(
            cmd,
            &self.descriptors,
            self.source.model.indices.len() as u32,
            1,
            0,
            0,
            0,
            Some(&self.push),
        );
    }
}
pub(super) struct Renderer {
    context: VulkanContext,
    allocator: Allocator,
    request: Option<Request>,
    previous: Option<Request>,
    source: Option<Arc<Source>>,
    volume: Option<Arc<voxel::Volume>>,
    frames: Vec<Frame>,
    slot: usize,
}
impl Renderer {
    pub fn new(context: VulkanContext, allocator: Allocator) -> Self {
        Self {
            context,
            allocator,
            request: None,
            previous: None,
            source: None,
            volume: None,
            frames: Vec::new(),
            slot: 0,
        }
    }
    pub fn request(&mut self, request: Option<Request>) -> Result<()> {
        if let Some(r) = request {
            ensure!(
                r.base.is_finite() && r.yaw.is_finite(),
                "nonfinite stone pose"
            );
        }
        self.request = request.map(|r| Request {
            spec: r.spec.sanitized(),
            ..r
        });
        Ok(())
    }
    /// Only the acquired slot is released. Seed, type, path, enable and shader
    /// descriptor/extent changes cannot invalidate other in-flight generations.
    pub fn begin_frame(&mut self, slot: usize) -> Result<()> {
        self.frames
            .resize_with(self.frames.len().max(slot + 1), Frame::default);
        self.slot = slot;
        self.frames[slot] = Frame::default();
        let Some(request) = self.request else {
            if self.previous.take().is_some() {
                log::info!("[STONE_PREVIEW] enabled=false garden_atlas_writes=0");
            }
            return Ok(());
        };
        if self
            .source
            .as_ref()
            .is_none_or(|s| s.model.spec != request.spec)
        {
            self.source = Some(Arc::new(Source::new(
                self.context.device(),
                &self.allocator,
                request.spec,
            )?));
            self.volume = None;
        }
        if !request.direct && self.volume.is_none() {
            self.volume = Some(Arc::new(voxel::Volume::new(
                &self.context,
                &self.allocator,
                &self.source.as_ref().unwrap().model,
            )?));
        }
        self.frames[slot] = Frame {
            source: self.source.clone(),
            volume: if request.direct {
                None
            } else {
                self.volume.clone()
            },
            request: Some(request),
        };
        if self.previous != Some(request) {
            let model = &self.source.as_ref().unwrap().model;
            log::info!("[STONE_PREVIEW] enabled=true path={} type={:?} seed={} source={:016x} triangles={} bounds={:?}..{:?} pivot=bottom-local base={:?} yaw={} frame_slot={} native_ui=true scene_grid=unchanged garden_atlas_writes=0",if request.direct {"direct-triangle"} else {"voxel-contree"},request.spec.kind,request.spec.seed,model.fingerprint(),model.indices.len()/3,model.min,model.max,request.base,request.yaw,slot);
        }
        self.previous = Some(request);
        Ok(())
    }
    pub fn has_direct(&self) -> bool {
        self.request.is_some_and(|r| r.direct)
    }
    pub fn record_volume(
        &self,
        cmd: &CommandBuffer,
        pipeline: &ComputePipeline,
        extent: Extent3D,
    ) -> Result<()> {
        let frame = &self.frames[self.slot];
        let Some(request) = frame.request.filter(|r| !r.direct) else {
            return Ok(());
        };
        let volume = frame.volume.as_ref().unwrap();
        let pose = Pose::new(request, volume.offsets);
        pipeline.record_with_descriptors(
            cmd,
            &[
                ("stone_nodes", DescriptorResource::Buffer(&volume.nodes)),
                ("stone_leaves", DescriptorResource::Buffer(&volume.leaves)),
            ],
            extent,
            Some(bytemuck::bytes_of(&pose)),
        )
    }
    pub fn prepare_direct(
        &self,
        cmd: &CommandBuffer,
        pipeline: &GraphicsPipeline,
        view_bank: (&str, DescriptorResource<'_>),
    ) -> Result<Option<PreparedDraw>> {
        let frame = &self.frames[self.slot];
        let Some(request) = frame.request.filter(|r| r.direct) else {
            return Ok(None);
        };
        let source = frame.source.as_ref().unwrap().clone();
        cmd.use_buffer(&source.triangles, BufferUse::HostWrite);
        cmd.use_buffer(&source.indices, BufferUse::HostWrite);
        cmd.use_buffer(&source.indices, BufferUse::IndexRead);
        pipeline.prepare_descriptor_resources(cmd);
        let descriptors = pipeline.prepare_draw_descriptors(
            cmd,
            &[
                (
                    "stone_triangles",
                    DescriptorResource::Buffer(&source.triangles),
                ),
                view_bank,
            ],
        )?;
        let pose = Pose::new(request, [0; 2]);
        Ok(Some(PreparedDraw {
            pipeline: pipeline.clone(),
            descriptors,
            source,
            push: PushConstantInfo {
                shader_stage: vk::ShaderStageFlags::VERTEX,
                push_constants: bytemuck::bytes_of(&pose).to_vec(),
            },
        }))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::stone_models::StoneKind;
    #[test]
    fn direct_stones_use_the_shared_view_bank_and_rigid_normal_frame() {
        let shader = include_str!("../../shader/slang/stone_preview.vert.slang");
        assert!(shader.contains("modelMeshViewFrame(stonePhysicalFrame(stone_pose), float3(0))"));
        assert!(shader.contains("position = modelViewWorldPoint(rendered, local)"));
        assert!(shader.contains("modelViewWorldVector(rendered, t.normal.xyz)"));
        assert!(!shader.contains("model_view_quantization_enabled"));
        assert!(!shader.contains("stoneWorldPoint(local, stone_pose)"));
        assert!(!shader.contains("SV_Depth"));
        let tracer = include_str!("mod.rs");
        let direct = tracer.split("let prepared_stone =").nth(1).unwrap();
        assert!(direct
            .split("let prepared_dynamic_pixels")
            .next()
            .unwrap()
            .contains("self.model_mesh_frame.view_bank_binding()"));
    }

    #[test]
    fn direct_triangle_adapter_preserves_the_same_source_positions_normals_and_colors() {
        assert_eq!(std::mem::size_of::<Triangle>(), 80);
        assert_eq!(std::mem::size_of::<Pose>(), 64);
        for kind in [StoneKind::Slab, StoneKind::Rock] {
            for seed in 0..8 {
                let mesh = generate(StoneSpec::new(kind, seed)).unwrap();
                let data = triangles(&mesh);
                for (i, (original, t)) in mesh.triangles().zip(data).enumerate() {
                    let a = Vec3::from_slice(&t.a);
                    assert_eq!(a, original[0]);
                    assert!((a + Vec3::from_slice(&t.e1)).abs_diff_eq(original[1], 1e-7));
                    assert!((a + Vec3::from_slice(&t.e2)).abs_diff_eq(original[2], 1e-7));
                    assert_eq!(Vec3::from_slice(&t.normal), mesh.normals[i]);
                    assert_eq!(Vec3::from_slice(&t.color), mesh.colors[i]);
                }
            }
        }
    }
}
