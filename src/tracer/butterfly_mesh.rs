//! Butterfly/leaf pose and material inputs for the shared model pixel generator.
//! Both generate one sample per tile texel, then display through the same lookup
//! shader as apples. Visible tiles are compact, resolution-sized and batched;
//! no maximum-resolution allocation is reserved for inactive particle slots.
use super::model_pixel_repair::{self, Node as RepairNode};
use anyhow::{ensure, Result};
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Quat, Vec3};
use re_flora_vkn::{vk, Allocator, Buffer, BufferUsage, Device, MemoryLocation};
use resource_container_derive::ResourceContainer;
use std::sync::{Arc, OnceLock};

use super::ButterflyPalettePreset;
mod validation;
use crate::{
    particles::{ParticleRenderKind, ParticleSnapshot},
    resource::Resource,
};

// The current eight-chunk garden allows 16 live butterflies. Reserve room for
// larger gardens without allocating PARTICLE_CAPACITY * 64² (a gigabyte).
const CAPACITY: usize = 256;
const MAX_TRIANGLES: usize = 256;
pub const MAX_RESOLUTION: u32 = 64;
const MODEL_CAPACITY: usize = crate::particles::PARTICLE_CAPACITY + CAPACITY;
const LEAF_MODEL_FLAG: u32 = 2;

// Match the GPU frame construction and operation order. Quaternion-vector
// multiplication is algebraically equivalent but not numerically identical for
// subpixel geometry far from the origin.
fn model_pose_axes(rotation: [f32; 4]) -> [Vec3; 3] {
    let q = Vec3::from_slice(&rotation);
    [Vec3::X, Vec3::Y, Vec3::Z].map(|v| v + 2. * q.cross(q.cross(v) + rotation[3] * v))
}
fn model_local_vector(axes: [Vec3; 3], v: Vec3) -> Vec3 {
    Vec3::new(v.dot(axes[0]), v.dot(axes[1]), v.dot(axes[2]))
}
fn leaf_variant_index(seed: u32) -> usize {
    seed as usize % crate::model_assets::LEAF_VARIANT_COUNT
}

// Upload one small authored shape bank, never a unique mesh or tile per leaf.
// All variants are generated from the same leafGeometry recipe as leaf.glb.
fn leaf_variant_triangles() -> &'static [Triangle] {
    static TRIANGLES: OnceLock<Vec<Triangle>> = OnceLock::new();
    TRIANGLES.get_or_init(|| {
        let model = crate::model_assets::leaf_variants();
        let transforms = model.transforms(0., 0);
        model
            .triangles
            .iter()
            .map(|triangle| {
                let transform = transforms[triangle.node];
                let normal_transform = transform.inverse().transpose();
                let p = triangle.positions.map(|p| transform.transform_point3(p));
                let uv = triangle.uvs;
                Triangle {
                    a: p[0].extend(0.).to_array(),
                    e1: (p[1] - p[0]).extend(0.).to_array(),
                    e2: (p[2] - p[0]).extend(0.).to_array(),
                    normals: triangle.normals.map(|n| {
                        normal_transform
                            .transform_vector3(n)
                            .normalize()
                            .extend(0.)
                            .to_array()
                    }),
                    uv01: [uv[0].x, uv[0].y, uv[1].x, uv[1].y],
                    uv2: [uv[2].x, uv[2].y, 0., 0.],
                }
            })
            .collect()
    })
}
pub(super) fn native_review() -> bool {
    std::env::var_os("RE_FLORA_LEAF_MODEL_REVIEW").is_some()
        || std::env::var_os("RE_FLORA_BUTTERFLY_MESH_REVIEW").is_some()
}

#[derive(Clone, Copy, Debug)]
pub struct LeafModelSettings {
    pub enabled: bool,
    pub resolution: u32,
    pub size_scale: f32,
}
impl Default for LeafModelSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            resolution: 16,
            size_scale: 1.,
        }
    }
}
impl LeafModelSettings {
    fn display_scale(self) -> f32 {
        if self.size_scale.is_finite() {
            self.size_scale.clamp(0.25, 4.)
        } else {
            1.
        }
    }

    /// Visual size only: never feed this back into LeafFlight's aerodynamic size.
    /// Both A/B paths scale detached leaves, not butterflies or leaf-colored debris.
    pub fn render_size(self, snapshot: &ParticleSnapshot) -> f32 {
        if snapshot.kind == ParticleRenderKind::Leaf && snapshot.leaf_orientation.is_some() {
            snapshot.size * self.display_scale()
        } else {
            snapshot.size
        }
    }

    pub fn uses_model(self, snapshot: &ParticleSnapshot) -> bool {
        self.enabled
            && snapshot.kind == ParticleRenderKind::Leaf
            && snapshot.leaf_orientation.is_some()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ButterflyMeshSettings {
    pub resolution: u32,
    pub fps: u32,
    pub self_shadows: bool,
    pub transmission: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Instance {
    position_size: [f32; 4],
    color: [f32; 4],
    // triangle start/count, pixel resolution, flags (bit 0: self-shadow, bit 1: leaf)
    metadata: [u32; 4],
    lighting: [f32; 4],
    repair: [u32; 4], // sparse expression offset/count; no user-selectable mode
    view_orientation: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Triangle {
    a: [f32; 4],
    e1: [f32; 4],
    e2: [f32; 4],
    normals: [[f32; 4]; 3],
    uv01: [f32; 4],
    uv2: [f32; 4],
}

#[derive(ResourceContainer)]
pub struct ButterflyMeshResources {
    pub butterfly_mesh_instances: Resource<Buffer>,
    pub butterfly_mesh_triangles: Resource<Buffer>,
    pub butterfly_pixel_tiles: Resource<Buffer>,
    pub model_pixel_tiles: Resource<Buffer>,
    pub model_object_samples: Resource<Buffer>,
    pub draw_indices: Resource<Buffer>,
    pub model_view_azimuths: Resource<Buffer>,
    pub particle_model_repairs: Resource<Buffer>,
    pub particle_model_repair_output: Resource<Buffer>,
}
impl ButterflyMeshResources {
    pub fn new(device: Device, allocator: Allocator) -> Self {
        let buffer = |bytes: usize, location| {
            Resource::new(Buffer::new_sized(
                device.clone(),
                allocator.clone(),
                BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
                location,
                bytes as u64,
            ))
        };
        let draw_indices = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::VERTEX_BUFFER | vk::BufferUsageFlags::STORAGE_BUFFER,
            ),
            MemoryLocation::CpuToGpu,
            (MODEL_CAPACITY * 4) as u64,
        );
        draw_indices
            .fill(&(0..MODEL_CAPACITY as u32).collect::<Vec<_>>())
            .unwrap();
        let model_view_azimuths = buffer(
            super::model_pixel_views::MAX_VIEWS as usize * 16,
            MemoryLocation::CpuToGpu,
        );
        model_view_azimuths
            .fill(&super::model_pixel_views::azimuths())
            .expect("model view directions");
        Self {
            model_view_azimuths,
            draw_indices: Resource::new(draw_indices),
            butterfly_mesh_instances: buffer(
                MODEL_CAPACITY * std::mem::size_of::<Instance>(),
                MemoryLocation::CpuToGpu,
            ),
            butterfly_mesh_triangles: buffer(
                ((CAPACITY + 1) * MAX_TRIANGLES) * std::mem::size_of::<Triangle>(),
                MemoryLocation::CpuToGpu,
            ),
            particle_model_repairs: buffer(
                std::mem::size_of::<RepairNode>(),
                MemoryLocation::CpuToGpu,
            ),
            particle_model_repair_output: buffer(
                std::mem::size_of::<RepairNode>(),
                MemoryLocation::CpuToGpu,
            ),
            model_pixel_tiles: buffer(16, MemoryLocation::GpuOnly),
            model_object_samples: buffer(16, MemoryLocation::GpuOnly),
            butterfly_pixel_tiles: Resource::new(Buffer::new_sized(
                device.clone(),
                allocator.clone(),
                BufferUsage::from_flags(
                    vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC,
                ),
                MemoryLocation::GpuOnly,
                (CAPACITY
                    * if native_review() { 4 } else { 1 }
                    * MAX_RESOLUTION as usize
                    * MAX_RESOLUTION as usize
                    * 16) as u64,
            )),
        }
    }
}

struct RestTriangle {
    node: usize,
    side: f32,
    positions: [[f32; 3]; 3],
}
struct Mesh {
    source: &'static crate::model_assets::Model,
    triangles: Vec<RestTriangle>,
}
impl Mesh {
    fn load() -> Self {
        let source = crate::model_assets::butterfly();
        let left = source.node("L continuous fore-hind wing");
        let right = source.node("R continuous fore-hind wing");
        let triangles = source
            .triangles
            .iter()
            .map(|t| {
                assert!(t.node == left || t.node == right);
                RestTriangle {
                    node: t.node,
                    side: if t.node == left { -1. } else { 1. },
                    positions: t.positions.map(|p| p.to_array()),
                }
            })
            .collect::<Vec<_>>();
        assert!(triangles.len() <= MAX_TRIANGLES);
        Self { source, triangles }
    }
    #[cfg(test)]
    fn pose(&self, phase: f32) -> [f32; 3] {
        // Publication already sampled the shared clock. The per-animal offset
        // changes wing phase, never the moment at which a pose is published.
        crate::particles::butterfly_wingbeat::wing_pose(phase)
    }
}

pub(super) struct ButterflyMeshRenderer {
    mesh: Mesh,
    instances: Vec<Instance>,
    draw_order: Vec<u32>,
    pub repair_nodes: Vec<RepairNode>,
    reference_tiles: bool,
    validation_calls: u64,
    repair_frames: Vec<Option<(Arc<Buffer>, usize)>>,
    repair_key: Vec<u8>,
    repair_key_scratch: Vec<u8>,
    repair_ranges: Vec<[u32; 4]>,
    pub repair_added: usize,
    repair_before: usize,
    repair_after: usize,
    triangles: Vec<Triangle>,
    pub resolution: u32,
    tile_count: u32,
    pub compute_count: u32,
    pub tile_batches: Vec<super::model_pixel_tiles::TileBatch>,
    pub dispatch_resolution: u32,
    previous_leaf_mode: Option<(bool, u32, u32)>,
    validated_leaf_mode: Option<(bool, u32, u32)>,
    previous_mode: Option<(u32, u32, bool, u32)>,
    validated_mode: Option<(u32, u32, bool, u32)>,
}
impl Default for ButterflyMeshRenderer {
    fn default() -> Self {
        Self {
            mesh: Mesh::load(),
            instances: Vec::new(),
            draw_order: Vec::new(),
            repair_nodes: Vec::new(),
            reference_tiles: native_review(),
            validation_calls: 0,
            repair_frames: Vec::new(),
            repair_key: Vec::new(),
            repair_key_scratch: Vec::new(),
            repair_ranges: Vec::new(),
            repair_added: 0,
            repair_before: 0,
            repair_after: 0,
            triangles: Vec::new(),
            resolution: 22,
            tile_count: 0,
            compute_count: 0,
            tile_batches: Vec::new(),
            dispatch_resolution: 22,
            previous_leaf_mode: None,
            validated_leaf_mode: None,
            previous_mode: None,
            validated_mode: None,
        }
    }
}
impl ButterflyMeshRenderer {
    /// Bounded diagnostic reference storage, never a selectable rendering mode.
    pub fn reference_tile_offset(&self) -> Option<u32> {
        self.reference_tiles.then_some(CAPACITY as u32)
    }
    pub fn count(&self) -> u32 {
        self.instances.len() as u32
    }
    fn prepare(
        &mut self,
        snapshots: &[ParticleSnapshot],
        settings: ButterflyMeshSettings,
        camera_position: Vec3,
    ) -> Result<()> {
        self.instances.clear();
        self.triangles.clear();
        self.resolution = settings.resolution.clamp(8, MAX_RESOLUTION);
        let mut candidates: Vec<_> = snapshots
            .iter()
            .filter(|s| s.kind == ParticleRenderKind::Butterfly && s.color.w > 0.0 && s.size > 0.0)
            .collect();
        ensure!(
            candidates.len() <= CAPACITY,
            "butterfly tile capacity exceeded: {} > {CAPACITY}",
            candidates.len()
        );
        ensure!(
            candidates.iter().all(|s| s
                .animation_sample_time
                .is_some_and(|t| t.is_finite() && t >= 0.)),
            "butterfly snapshot missing a valid shared presentation timestamp"
        );
        // Lifetime fading is object-level alpha, not transparent wing material.
        // Back-to-front submission preserves overlapping fading silhouettes.
        candidates.sort_by(|a, b| {
            b.position_ws
                .distance_squared(camera_position)
                .total_cmp(&a.position_ws.distance_squared(camera_position))
        });
        for snapshot in candidates {
            let sampled_time = snapshot
                .animation_sample_time
                .expect("validated presentation timestamp");
            let original_phase = (sampled_time + snapshot.animation_phase_offset).rem_euclid(1.);
            let coupling = snapshot.butterfly_wingbeat;
            let blend = coupling.map_or(0., |p| p.blend);
            let phase = coupling.map_or(original_phase, |p| {
                if blend == 1. {
                    p.phase
                } else {
                    original_phase + ((p.phase - original_phase + 0.5).rem_euclid(1.) - 0.5) * blend
                }
            });
            let transforms = self.mesh.source.transforms(phase, 0);
            // Only authored root displacement is suppressed by flight coupling;
            // articulated geometry and rotations still come directly from the GLB.
            let root_motion = transforms[self.mesh.source.node("Flight pose")]
                .w_axis
                .truncate();
            let velocity = snapshot.velocity;
            let speed = velocity.x.hypot(velocity.z);
            let yaw = if speed > 0.0001 {
                (-velocity.x).atan2(-velocity.z)
            } else {
                0.0
            };
            let flight_pitch = velocity.y.atan2(speed.max(0.001)).clamp(-0.4, 0.4);
            let original_facing = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(flight_pitch);
            let facing = coupling.map_or(original_facing, |p| {
                if blend == 1. {
                    p.orientation
                } else {
                    original_facing.slerp(p.orientation, blend)
                }
            });
            // Keep the same nominal billboard footprint as the old particles.
            // 3.4 is the approved browser's fixed framing span, not a fitted
            // per-frame silhouette: flapping must not pump the pixel scale.
            let scale = snapshot.size * (1.53125 / 3.4);
            let start = self.triangles.len() as u32;
            for triangle in &self.mesh.triangles {
                let transform = transforms[triangle.node];
                let p = triangle.positions.map(|p| {
                    snapshot.position_ws
                        + facing
                            * (transform.transform_point3(Vec3::from(p)) - root_motion * blend)
                            * scale
                });
                self.triangles.push(Triangle {
                    a: p[0].extend(0.).to_array(),
                    e1: (p[1] - p[0]).extend(triangle.side).to_array(),
                    e2: (p[2] - p[0]).extend(0.).to_array(),
                    ..Triangle::zeroed()
                });
            }
            let rgb = ButterflyPalettePreset::from_index(snapshot.palette_index).base_color_srgb();
            self.instances.push(Instance {
                position_size: snapshot.position_ws.extend(snapshot.size).to_array(),
                color: [
                    rgb[0] as f32 / 255.,
                    rgb[1] as f32 / 255.,
                    rgb[2] as f32 / 255.,
                    snapshot.color.w,
                ],
                lighting: [settings.transmission.clamp(0., 1.), 0., 0., 0.],
                view_orientation: facing.to_array(),
                repair: [0; 4],
                metadata: [
                    start,
                    self.mesh.triangles.len() as u32,
                    self.resolution,
                    u32::from(settings.self_shadows),
                ],
            });
        }
        Ok(())
    }

    fn prepare_models(
        &mut self,
        snapshots: &[ParticleSnapshot],
        butterflies: ButterflyMeshSettings,
        leaves: LeafModelSettings,
        camera_position: Vec3,
    ) -> Result<()> {
        self.compute_count = 0;
        self.tile_count = 0;
        self.prepare(snapshots, butterflies, camera_position)?;
        self.tile_count = self.count();
        self.compute_count = self.tile_count;
        self.dispatch_resolution = self.resolution;
        if !leaves.enabled {
            return Ok(());
        }
        let candidates: Vec<_> = snapshots
            .iter()
            .filter(|s| leaves.uses_model(s) && s.size > 0. && s.color.w > 0.)
            .collect();
        ensure!(
            candidates.len() <= crate::particles::PARTICLE_CAPACITY,
            "falling leaf model capacity exceeded"
        );
        ensure!(
            candidates.iter().all(|s| s
                .leaf_orientation
                .is_some_and(|q| q.is_finite() && q.is_normalized())
                && s.leaf_shape_seed.is_some()),
            "invalid published leaf orientation or shape seed"
        );
        let triangles_per_leaf = crate::model_assets::leaf().triangles.len();
        ensure!(
            triangles_per_leaf <= MAX_TRIANGLES,
            "shared leaf exceeds triangle budget"
        );
        let first = self.triangles.len() as u32;
        if !candidates.is_empty() {
            let shapes = leaf_variant_triangles();
            ensure!(
                shapes.len() == triangles_per_leaf * crate::model_assets::LEAF_VARIANT_COUNT,
                "leaf shape bank does not match the approved topology"
            );
            self.triangles.extend_from_slice(shapes);
        }
        let resolution = leaves.resolution.clamp(8, MAX_RESOLUTION);
        for snapshot in candidates {
            let shape = leaf_variant_index(snapshot.leaf_shape_seed.unwrap());
            self.instances.push(Instance {
                position_size: snapshot
                    .position_ws
                    .extend(leaves.render_size(snapshot))
                    .to_array(),
                color: snapshot.color.to_array(),
                metadata: [
                    first + (shape * triangles_per_leaf) as u32,
                    triangles_per_leaf as u32,
                    resolution,
                    LEAF_MODEL_FLAG,
                ],
                // No resampling, local animation, velocity-facing override or reset.
                lighting: snapshot.leaf_orientation.unwrap().to_array(),
                view_orientation: snapshot.leaf_orientation.unwrap().to_array(),
                repair: [0; 4],
            });
        }
        if std::env::var_os("RE_FLORA_LEAF_MODEL_REVIEW").is_some() {
            // Diagnostic-only readback of the same shader function used by the
            // production fragment path, bounded by the existing tile allocation.
            ensure!(
                self.instances.len() <= CAPACITY,
                "leaf review tile capacity exceeded"
            );
            self.compute_count = self.count();
            self.dispatch_resolution = self.resolution.max(resolution);
        }
        Ok(())
    }

    /// Called after this frame's camera is final, before any model draw/dispatch.
    /// Exact input comparison avoids rebuilding held poses, but GPU lighting is
    /// evaluated afresh even when the geometry plan is reused.
    pub fn prepare_repair_frame(
        &mut self,
        view: Mat4,
        projection: Mat4,
        resources: &ButterflyMeshResources,
        device: Device,
        allocator: Allocator,
        frame_slot: usize,
    ) -> Result<()> {
        self.repair_key_scratch.clear();
        self.repair_key_scratch
            .extend_from_slice(bytemuck::bytes_of(&[
                self.instances.len() as u64,
                self.triangles.len() as u64,
            ]));
        self.repair_key_scratch
            .extend_from_slice(bytemuck::bytes_of(&view));
        self.repair_key_scratch
            .extend_from_slice(bytemuck::bytes_of(&projection));
        for instance in &self.instances {
            self.repair_key_scratch
                .extend_from_slice(bytemuck::bytes_of(&instance.position_size));
            self.repair_key_scratch
                .extend_from_slice(bytemuck::bytes_of(&instance.metadata));
            self.repair_key_scratch
                .extend_from_slice(bytemuck::bytes_of(&instance.lighting));
        }
        self.repair_key_scratch
            .extend_from_slice(bytemuck::cast_slice(&self.triangles));
        if native_review() && self.repair_key != self.repair_key_scratch {
            std::mem::swap(&mut self.repair_key, &mut self.repair_key_scratch);
            self.repair_nodes.clear();
            self.repair_ranges.clear();
            self.repair_added = 0;
            self.repair_before = 0;
            self.repair_after = 0;
            for instance in &self.instances {
                let leaf = instance.metadata[3] & LEAF_MODEL_FLAG != 0;
                let scale = instance.position_size[3] * (1.53125 / 3.4);
                let center = Vec3::from_slice(&instance.position_size);
                let bounds = model_pixel_repair::tile_bounds(
                    center,
                    instance.position_size[3],
                    view,
                    projection,
                );
                if bounds.x > 1. || bounds.y > 1. || bounds.z < -1. || bounds.w < -1. {
                    self.repair_ranges.push([0; 4]);
                    continue;
                }
                let axes = model_pose_axes(instance.lighting);
                let start = instance.metadata[0] as usize;
                let end = start + instance.metadata[1] as usize;
                let triangles: Vec<_> = self.triangles[start..end]
                    .iter()
                    .map(|t| {
                        let a = Vec3::from_slice(&t.a);
                        let p = [a, a + Vec3::from_slice(&t.e1), a + Vec3::from_slice(&t.e2)];
                        (
                            if leaf || t.e1[3] < 0. { 1 } else { 2 },
                            if leaf {
                                p.map(|p| {
                                    center + scale * (axes[0] * p.x + axes[1] * p.y + axes[2] * p.z)
                                })
                            } else {
                                p
                            },
                        )
                    })
                    .collect();
                let vp = projection * view;
                let inverse = vp.inverse();
                let center_distance = |index: usize, x: usize, y: usize| {
                    let uv = glam::Vec2::new(x as f32 + 0.5, y as f32 + 0.5)
                        / instance.metadata[2] as f32;
                    let ndc = glam::Vec2::new(bounds.x, bounds.y)
                        + glam::Vec2::new(bounds.z - bounds.x, bounds.w - bounds.y) * uv;
                    let near = inverse * glam::Vec4::new(ndc.x, ndc.y, 0., 1.);
                    let far = inverse * glam::Vec4::new(ndc.x, ndc.y, 1., 1.);
                    let origin = near.truncate() / near.w;
                    let direction = (far.truncate() / far.w - origin).normalize();
                    let (local_origin, local_direction) = if leaf {
                        (
                            model_local_vector(axes, origin - center) / scale,
                            model_local_vector(axes, direction),
                        )
                    } else {
                        (origin, direction)
                    };
                    let t = &self.triangles[start + index];
                    let distance = model_pixel_repair::ray_triangle(
                        local_origin,
                        local_direction,
                        Vec3::from_slice(&t.a),
                        Vec3::from_slice(&t.e1),
                        Vec3::from_slice(&t.e2),
                    )?;
                    let clip = vp
                        * (origin + direction * distance * if leaf { scale } else { 1. })
                            .extend(1.);
                    (0.0..1.0).contains(&(clip.z / clip.w)).then_some(distance)
                };
                let (owners, groups) = model_pixel_repair::project(
                    &triangles,
                    vp,
                    bounds,
                    instance.metadata[2] as usize,
                    center_distance,
                );
                let plan =
                    model_pixel_repair::plan(&owners, &groups, instance.metadata[2] as usize);
                self.repair_ranges.push([
                    self.repair_nodes.len() as u32,
                    plan.nodes.len() as u32,
                    0,
                    0,
                ]);
                self.repair_added += plan.added;
                self.repair_before += plan.before;
                self.repair_after += plan.after;
                self.repair_nodes.extend(plan.nodes);
            }
            if native_review() {
                log::info!("[MODEL_COVERAGE_REVIEW] instances={} added={} original_components={} final_components={} nodes={}",
                    self.instances.len(), self.repair_added, self.repair_before, self.repair_after, self.repair_nodes.len());
            }
        }
        for (instance, range) in self.instances.iter_mut().zip(&self.repair_ranges) {
            instance.repair = *range;
        }
        // Pack only visible tiles, at their own resolution, instead of reserving
        // PARTICLE_CAPACITY * 64². The fragment path never samples geometry.
        for instance in &mut self.instances {
            let bounds = model_pixel_repair::tile_bounds(
                Vec3::from_slice(&instance.position_size),
                instance.position_size[3],
                view,
                projection,
            );
            let visible = !(bounds.x > 1. || bounds.y > 1. || bounds.z < -1. || bounds.w < -1.);
            instance.repair[3] = u32::from(visible);
        }
        self.assign_pixel_tiles();
        if !self.instances.is_empty() {
            // Publish only the complete frame: never overwrite in-flight tile
            // offsets/visibility with prepare_models' zero-initialized metadata.
            resources.butterfly_mesh_instances.fill(&self.instances)?;
            resources.butterfly_mesh_triangles.fill(&self.triangles)?;
            resources.draw_indices.fill(&self.draw_order)?;
        }
        self.compute_count = self.count();
        self.dispatch_resolution = self
            .instances
            .iter()
            .map(|i| i.metadata[2])
            .max()
            .unwrap_or(8);
        while self.repair_frames.len() <= frame_slot {
            self.repair_frames.push(None);
        }
        let required = self
            .repair_nodes
            .len()
            .max(1)
            .checked_next_power_of_two()
            .ok_or_else(|| anyhow::anyhow!("repair buffer capacity overflow"))?;
        // Vulkan guarantees at least 128 MiB per storage-buffer binding. Never
        // silently drop repairs or bind an out-of-range allocation on overflow.
        ensure!(
            required <= 128 * 1024 * 1024 / std::mem::size_of::<RepairNode>(),
            "sparse repair plan exceeds portable storage-buffer range"
        );
        if self.repair_frames[frame_slot]
            .as_ref()
            .is_none_or(|(_, capacity)| *capacity < required)
        {
            let buffer = Buffer::new_sized(
                device,
                allocator,
                BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
                MemoryLocation::CpuToGpu,
                (required * std::mem::size_of::<RepairNode>()) as u64,
            );
            self.repair_frames[frame_slot] = Some((Arc::new(buffer), required));
        }
        if !self.repair_nodes.is_empty() {
            self.repair_frames[frame_slot]
                .as_ref()
                .unwrap()
                .0
                .fill(&self.repair_nodes)?;
        }
        Ok(())
    }

    fn assign_pixel_tiles(&mut self) {
        let (offsets, batches) =
            super::model_pixel_tiles::pack_tiles(self.draw_order.iter().map(|&index| {
                let i = &self.instances[index as usize];
                (i.metadata[2], i.repair[3] != 0)
            }));
        self.tile_batches = batches;
        for (&index, offset) in self.draw_order.iter().zip(offsets) {
            self.instances[index as usize].repair[2] = offset;
        }
    }
    pub fn tile_compute_mode(&self) -> u32 {
        if native_review() {
            1
        } else {
            3
        }
    }

    pub fn repair_buffer(&self, frame_slot: usize) -> Arc<Buffer> {
        self.repair_frames[frame_slot]
            .as_ref()
            .expect("prepared model frame")
            .0
            .clone()
    }

    pub fn prepare_frame_models(
        &mut self,
        snapshots: &[ParticleSnapshot],
        settings: ButterflyMeshSettings,
        leaves: LeafModelSettings,
        camera_position: Vec3,
    ) -> Result<()> {
        self.prepare_models(snapshots, settings, leaves, camera_position)?;
        self.draw_order.clear();
        if !self.instances.is_empty() {
            let mut order: Vec<u32> = (0..self.count()).collect();
            order.sort_by(|a, b| {
                let distance = |i: u32| {
                    Vec3::from_slice(&self.instances[i as usize].position_size)
                        .distance_squared(camera_position)
                };
                distance(*b).total_cmp(&distance(*a))
            });
            self.draw_order = order;
        }
        let leaf_mode = (
            leaves.enabled,
            leaves.resolution.clamp(8, MAX_RESOLUTION),
            leaves.display_scale().to_bits(),
        );
        if self.previous_leaf_mode != Some(leaf_mode) {
            log::info!("[LEAF-MODEL] mode={} pixels={}x{} active={} source={} source_crc32={:08x} orientation=published_simulation geometry=32_triangles render_scale={}", if leaves.enabled { "B" } else { "A" }, leaf_mode.1, leaf_mode.1, self.count() - self.tile_count,
                if leaves.enabled { "assets/models/leaf-variants.glb" } else { "assets/models/leaf.glb" },
                crc32fast::hash(if leaves.enabled { crate::model_assets::LEAF_VARIANTS_BYTES } else { crate::model_assets::LEAF_BYTES }), leaves.display_scale());
            if leaves.enabled && native_review() {
                let first = self.tile_count as usize * self.mesh.triangles.len();
                let ids: Vec<_> = self.instances[self.tile_count as usize..]
                    .iter()
                    .map(|instance| {
                        (instance.metadata[0] as usize - first)
                            / crate::model_assets::leaf().triangles.len()
                    })
                    .collect();
                let mut unique = ids.clone();
                unique.sort_unstable();
                unique.dedup();
                log::info!(
                    "[LEAF-SHAPE-REVIEW] active={} unique={} ids={ids:?}",
                    ids.len(),
                    unique.len()
                );
            }
            self.previous_leaf_mode = Some(leaf_mode);
        }
        let mode = (
            self.resolution,
            settings.fps,
            settings.self_shadows,
            settings.transmission.clamp(0., 1.).to_bits(),
        );
        if self.previous_mode != Some(mode) {
            log::info!("[BUTTERFLY-MESH] tile={}x{} fps={} self_shadows={} triangles_per_animal={} active={} capacity={CAPACITY} sun=game depth=per_texel", self.resolution,self.resolution,settings.fps,settings.self_shadows,self.mesh.triangles.len(),self.tile_count);
            log::info!(
                "[BUTTERFLY-MESH] transmission={}",
                settings.transmission.clamp(0., 1.)
            );
            self.previous_mode = Some(mode);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sorted_draw_batches_keep_tile_offsets_with_their_instances() {
        let mut renderer = ButterflyMeshRenderer::default();
        renderer.instances = (0..1100)
            .map(|_| Instance {
                position_size: [0.; 4],
                color: [1.; 4],
                metadata: [0, 32, 64, LEAF_MODEL_FLAG],
                lighting: [0., 0., 0., 1.],
                view_orientation: [0., 0., 0., 1.],
                repair: [0, 0, 0, 1],
            })
            .collect();
        renderer.draw_order = (0..1100u32).rev().collect();
        renderer.assign_pixel_tiles();
        assert_eq!(renderer.tile_batches.len(), 2);
        for batch in &renderer.tile_batches {
            for (tile, &instance) in renderer.draw_order
                [batch.first as usize..(batch.first + batch.count) as usize]
                .iter()
                .enumerate()
            {
                assert_eq!(
                    renderer.instances[instance as usize].repair[2],
                    tile as u32 * 64 * 64
                );
            }
        }
        assert_eq!(renderer.instances[0].repair[2], 75 * 4096);
        assert_eq!(renderer.instances[1099].repair[2], 0);
    }

    #[test]
    fn declared_leaf_pixel_control_matches_butterfly_range_but_is_independent() {
        let config: toml::Value = toml::from_str(include_str!("../../config/gui.toml")).unwrap();
        let params: Vec<_> = config["section"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|s| s.get("param").and_then(toml::Value::as_array))
            .flatten()
            .collect();
        let data = |id: &str| {
            &params
                .iter()
                .find(|p| p["id"].as_str() == Some(id))
                .unwrap()["data"]
        };
        for key in ["min", "max"] {
            assert_eq!(
                data("falling_leaf_pixel_resolution")[key],
                data("butterfly_pixel_resolution")[key]
            );
        }
        // Saved live values may legitimately diverge after a user edits either
        // slider. Only the initial programmatic default is fixed at 16.
        assert_eq!(LeafModelSettings::default().resolution, 16);
        assert!(!LeafModelSettings::default().enabled);
    }

    #[test]
    fn leaf_ab_consumes_existing_pose_without_retiming_or_changing_the_particle() {
        use crate::particles::{ParticleForces, ParticleSpawn, ParticleSystem};
        let mut system = ParticleSystem::new(1);
        system
            .spawn(ParticleSpawn {
                position: Vec3::new(0., 2., 0.),
                lifetime: 30.,
                ..ParticleSpawn::default()
            })
            .unwrap();
        let mut snapshots = Vec::new();
        let mut renderer = ButterflyMeshRenderer::default();
        let mut butterfly = ButterflyMeshSettings {
            resolution: 8,
            fps: 2,
            self_shadows: true,
            transmission: 0.9,
        };
        for _ in 0..120 {
            system.update(1. / 120., ParticleForces::default());
            system.write_snapshots(&mut snapshots);
            let snapshot = snapshots[0];
            for resolution in [8, 16, 64] {
                let enabled = LeafModelSettings {
                    enabled: true,
                    resolution,
                    ..LeafModelSettings::default()
                };
                renderer
                    .prepare_models(&snapshots, butterfly, enabled, Vec3::Z)
                    .unwrap();
                assert_eq!(renderer.count(), 1);
                assert_eq!(renderer.tile_count, 0);
                assert_eq!(renderer.triangles.len(), 64 * 32);
                let gpu = renderer.instances[0];
                assert_eq!(gpu.lighting, snapshot.leaf_orientation.unwrap().to_array());
                assert_eq!(
                    gpu.position_size,
                    snapshot.position_ws.extend(snapshot.size).to_array()
                );
                assert_eq!(gpu.color, snapshot.color.to_array());
                assert_eq!(
                    gpu.metadata,
                    [
                        (leaf_variant_index(snapshot.leaf_shape_seed.unwrap()) * 32) as u32,
                        32,
                        resolution,
                        LEAF_MODEL_FLAG
                    ]
                );
                butterfly.fps = 60;
                renderer
                    .prepare_models(&snapshots, butterfly, enabled, Vec3::Z)
                    .unwrap();
                assert_eq!(
                    bytemuck::bytes_of(&gpu),
                    bytemuck::bytes_of(&renderer.instances[0])
                );
                renderer
                    .prepare_models(&snapshots, butterfly, LeafModelSettings::default(), Vec3::Z)
                    .unwrap();
                assert_eq!(renderer.count(), 0);
                assert_eq!(snapshots[0].leaf_orientation, snapshot.leaf_orientation);
                assert_eq!(snapshots[0].position_ws, snapshot.position_ws);
            }
        }
    }

    #[test]
    fn display_scale_changes_only_detached_leaf_render_size_in_both_modes() {
        let mut system = crate::particles::ParticleSystem::new(1);
        system
            .spawn(crate::particles::ParticleSpawn::default())
            .unwrap();
        let mut snapshots = Vec::new();
        system.write_snapshots(&mut snapshots);
        let original = snapshots[0];
        let butterfly = ButterflyMeshSettings {
            resolution: 16,
            fps: 8,
            self_shadows: true,
            transmission: 0.9,
        };
        let mut renderer = ButterflyMeshRenderer::default();
        for scale in [0.25, 1., 2., 4.] {
            let mut settings = LeafModelSettings {
                size_scale: scale,
                ..LeafModelSettings::default()
            };
            // The sprite path uses precisely this helper, without editing snapshots.
            assert_eq!(settings.render_size(&original), original.size * scale);
            settings.enabled = true;
            renderer
                .prepare_models(&snapshots, butterfly, settings, Vec3::Z)
                .unwrap();
            let instance = renderer.instances[0];
            assert_eq!(
                instance.position_size,
                original
                    .position_ws
                    .extend(original.size * scale)
                    .to_array()
            );
            assert_eq!(
                instance.lighting,
                original.leaf_orientation.unwrap().to_array()
            );
            assert_eq!(instance.color, original.color.to_array());
            assert_eq!(
                instance.metadata,
                [
                    (leaf_variant_index(original.leaf_shape_seed.unwrap()) * 32) as u32,
                    32,
                    16,
                    LEAF_MODEL_FLAG
                ]
            );
            assert_eq!(
                renderer.triangles[0].a,
                crate::model_assets::leaf().triangles[0].positions[0]
                    .extend(0.)
                    .to_array()
            );
            for kind in [
                ParticleRenderKind::Butterfly,
                ParticleRenderKind::WaterDroplet,
                ParticleRenderKind::TerrainVoxel,
            ] {
                let mut other = original;
                other.kind = kind;
                assert_eq!(settings.render_size(&other), original.size);
            }
            let mut debris = original;
            debris.leaf_orientation = None;
            assert_eq!(settings.render_size(&debris), original.size);
        }
        for (input, expected) in [(f32::NAN, 1.), (f32::INFINITY, 1.), (-1., 0.25), (100., 4.)] {
            assert_eq!(
                LeafModelSettings {
                    size_scale: input,
                    ..LeafModelSettings::default()
                }
                .render_size(&original),
                original.size * expected
            );
        }
        system.write_snapshots(&mut snapshots);
        assert_eq!(snapshots[0].size, original.size);
        assert_eq!(snapshots[0].position_ws, original.position_ws);
        assert_eq!(snapshots[0].velocity, original.velocity);
        assert_eq!(snapshots[0].leaf_orientation, original.leaf_orientation);
    }

    #[test]
    fn falling_leaves_choose_independent_authored_shapes_without_per_leaf_meshes() {
        let mut system = crate::particles::ParticleSystem::new(1);
        system
            .spawn(crate::particles::ParticleSpawn::default())
            .unwrap();
        let mut snapshots = Vec::new();
        system.write_snapshots(&mut snapshots);
        let original = snapshots[0];
        let mut other = original;
        other.leaf_shape_seed = Some(original.leaf_shape_seed.unwrap().wrapping_add(1));
        let mut renderer = ButterflyMeshRenderer::default();
        let butterfly = ButterflyMeshSettings {
            resolution: 16,
            fps: 8,
            self_shadows: true,
            transmission: 0.,
        };
        let leaves = LeafModelSettings {
            enabled: true,
            ..LeafModelSettings::default()
        };
        renderer
            .prepare_models(&[original, other], butterfly, leaves, Vec3::Z)
            .unwrap();
        let first = renderer.instances[0].metadata[0] as usize;
        let second = renderer.instances[1].metadata[0] as usize;
        assert_ne!(first, second);
        assert_eq!(
            renderer.triangles.len(),
            crate::model_assets::LEAF_VARIANT_COUNT * 32
        );
        assert!(renderer.triangles[first..first + 32]
            .iter()
            .zip(&renderer.triangles[second..second + 32])
            .any(|(a, b)| a.a != b.a));
        let triangle_bytes = bytemuck::cast_slice::<Triangle, u8>(&renderer.triangles).to_vec();
        other.leaf_orientation = Some(glam::Quat::from_rotation_x(0.3));
        renderer
            .prepare_models(&[original, other], butterfly, leaves, Vec3::Z)
            .unwrap();
        assert_eq!(renderer.instances[1].metadata[0] as usize, second);
        assert_eq!(
            bytemuck::cast_slice::<Triangle, u8>(&renderer.triangles),
            triangle_bytes
        );
    }

    #[test]
    fn leaf_shape_bank_is_uploaded_once_even_at_full_particle_capacity() {
        let mut system = crate::particles::ParticleSystem::new(1);
        system
            .spawn(crate::particles::ParticleSpawn::default())
            .unwrap();
        let mut source = Vec::new();
        system.write_snapshots(&mut source);
        let snapshots = vec![source[0]; crate::particles::PARTICLE_CAPACITY];
        let mut renderer = ButterflyMeshRenderer::default();
        renderer
            .prepare_models(
                &snapshots,
                ButterflyMeshSettings {
                    resolution: 16,
                    fps: 8,
                    self_shadows: true,
                    transmission: 0.9,
                },
                LeafModelSettings {
                    enabled: true,
                    resolution: 64,
                    ..LeafModelSettings::default()
                },
                Vec3::Z,
            )
            .unwrap();
        assert_eq!(renderer.count() as usize, snapshots.len());
        assert_eq!(renderer.triangles.len(), 64 * 32);
        assert_eq!(
            renderer.compute_count, 0,
            "ordinary leaves must not allocate or dispatch per-particle tiles"
        );
        let mut no_pose = source[0];
        no_pose.leaf_orientation = None;
        assert!(!LeafModelSettings {
            enabled: true,
            resolution: 16,
            ..LeafModelSettings::default()
        }
        .uses_model(&no_pose));
    }

    #[test]
    fn approved_source_is_closed_wing_geometry_with_looping_keys() {
        let mesh = Mesh::load();
        assert_eq!(mesh.triangles.len(), 156);
        assert_eq!(mesh.source.transforms(0., 0), mesh.source.transforms(1., 0));
        for t in &mesh.triangles {
            assert!(t.side == 1.0 || t.side == -1.0);
            let p = t.positions.map(Vec3::from);
            assert!((p[1] - p[0]).cross(p[2] - p[0]).length_squared() > 1e-12);
            for frame in 0..60 {
                let [wing, pitch, bob] = mesh.pose(frame as f32 / 60.);
                let rot = Quat::from_rotation_x(pitch) * Quat::from_rotation_z(wing * t.side);
                for v in p {
                    assert!((rot * v + Vec3::Y * bob).length() < 1.7);
                }
            }
        }
        assert_eq!(std::mem::size_of::<Instance>(), 96);
        assert_eq!(std::mem::size_of::<Triangle>(), 128);
    }
    #[test]
    fn coupled_mesh_uses_published_phase_attitude_and_no_duplicate_bob() {
        let pose = crate::particles::ButterflyWingbeatPose {
            phase: 0.4,
            blend: 1.,
            orientation: Quat::from_rotation_y(0.2) * Quat::from_rotation_z(0.3),
        };
        let mut snapshot = ParticleSnapshot {
            position_ws: Vec3::ONE,
            velocity: Vec3::X,
            color: glam::Vec4::ONE,
            size: 0.03,
            kind: ParticleRenderKind::Butterfly,
            palette_index: 0,
            animation_phase_offset: 0.25,
            animation_sample_time: Some(0.),
            butterfly_wingbeat: Some(pose),
            leaf_orientation: None,
            leaf_shape_seed: None,
        };
        let settings = ButterflyMeshSettings {
            resolution: 16,
            fps: 8,
            self_shadows: true,
            transmission: 0.8,
        };
        let mut renderer = ButterflyMeshRenderer::default();
        renderer.prepare(&[snapshot], settings, Vec3::ZERO).unwrap();
        let geometry = bytemuck::cast_slice::<Triangle, u8>(&renderer.triangles).to_vec();
        let [wing, pitch, bob] = renderer.mesh.pose(pose.phase);
        assert!(
            bob.abs() > 0.01,
            "test a source pose with visible authored bob"
        );
        for (source, result) in renderer.mesh.triangles.iter().zip(&renderer.triangles) {
            let rotation = Quat::from_rotation_x(pitch) * Quat::from_rotation_z(wing * source.side);
            let expected = snapshot.position_ws
                + pose.orientation
                    * (rotation * Vec3::from(source.positions[0]))
                    * (snapshot.size * (1.53125 / 3.4));
            assert!(Vec3::from_slice(&result.a).distance(expected) < 1e-6);
        }
        // The publication timestamp remains required, but cannot independently
        // animate a coupled pose. Nor may ground-relative velocity override yaw.
        snapshot.animation_sample_time = Some(123.731);
        snapshot.velocity = -Vec3::Y;
        renderer.prepare(&[snapshot], settings, Vec3::ZERO).unwrap();
        assert_eq!(
            geometry,
            bytemuck::cast_slice::<Triangle, u8>(&renderer.triangles)
        );
    }

    #[test]
    fn tile_resolution_is_global_and_empty_frames_do_not_retain_instances() {
        let snapshot = |distance: f32, kind| ParticleSnapshot {
            position_ws: Vec3::new(0., 0., -distance),
            velocity: Vec3::Z,
            color: glam::Vec4::ONE,
            size: 0.03,
            kind,
            palette_index: 5,
            animation_phase_offset: 0.25,
            animation_sample_time: Some(0.),
            butterfly_wingbeat: None,
            leaf_orientation: None,
            leaf_shape_seed: None,
        };
        let snapshots = [
            snapshot(0.1, ParticleRenderKind::Butterfly),
            snapshot(2., ParticleRenderKind::Butterfly),
            snapshot(0.5, ParticleRenderKind::Leaf),
        ];
        let mut renderer = ButterflyMeshRenderer::default();
        let mut settings = ButterflyMeshSettings {
            resolution: 22,
            fps: 60,
            self_shadows: true,
            transmission: 0.,
        };
        for n in 8..=64 {
            settings.resolution = n;
            renderer.prepare(&snapshots, settings, Vec3::ZERO).unwrap();
            assert_eq!(renderer.count(), 2);
            assert!(renderer.instances.iter().all(|i| i.metadata[2] == n));
            assert_eq!(renderer.instances[0].position_size[2], -2.);
            assert_eq!(renderer.instances[0].color, renderer.instances[1].color);
            assert_eq!(renderer.triangles.len(), 312);
            assert!(renderer.triangles.iter().all(|t| t.e1[3].abs() == 1.));
        }
        for (requested, expected) in [(-1., 0.), (0., 0.), (0.5, 0.5), (1., 1.), (2., 1.)] {
            settings.transmission = requested;
            renderer.prepare(&snapshots, settings, Vec3::ZERO).unwrap();
            assert!(renderer.instances.iter().all(|i| i.lighting[0] == expected));
            assert!(renderer.instances.iter().all(|i| i.color[3] == 1.));
        }
        renderer.prepare(&[], settings, Vec3::ZERO).unwrap();
        assert_eq!(renderer.count(), 0);
        assert!(renderer.triangles.is_empty());
        assert!(renderer
            .prepare(&vec![snapshots[0]; CAPACITY + 1], settings, Vec3::ZERO)
            .is_err());
        assert_eq!(renderer.count(), 0);
        renderer.prepare(&snapshots, settings, Vec3::ZERO).unwrap();
        assert_eq!(renderer.count(), 2);
        let mut unsampled = snapshots[0];
        unsampled.animation_sample_time = None;
        assert!(renderer
            .prepare(&[snapshots[0], unsampled], settings, Vec3::ZERO)
            .is_err());
        assert_eq!(renderer.count(), 0);
        assert!(renderer.triangles.is_empty());
    }

    #[test]
    fn movement_and_wings_hold_on_the_same_tick_despite_individual_phase() {
        let mut renderer = ButterflyMeshRenderer::default();
        let mut previous = None;
        for frame in 0..120 {
            let time = frame as f32 / 120.;
            let tick = (time * 8.).floor() as u32;
            let snapshot = ParticleSnapshot {
                position_ws: Vec3::new(tick as f32 * 0.01, 0., -0.5),
                velocity: Vec3::Z,
                color: glam::Vec4::ONE,
                size: 0.03,
                kind: ParticleRenderKind::Butterfly,
                palette_index: 0,
                animation_phase_offset: 0.31,
                butterfly_wingbeat: None,
                animation_sample_time: Some(
                    crate::particles::ButterflyFrame::at(time, 8).time_seconds(),
                ),
                leaf_orientation: None,
                leaf_shape_seed: None,
            };
            renderer
                .prepare(
                    &[snapshot],
                    ButterflyMeshSettings {
                        resolution: 16,
                        fps: 8,
                        self_shadows: true,
                        transmission: 0.,
                    },
                    Vec3::ZERO,
                )
                .unwrap();
            let geometry = bytemuck::cast_slice::<Triangle, u8>(&renderer.triangles).to_vec();
            if let Some((previous_tick, ref previous_geometry)) = previous {
                if previous_tick == tick {
                    assert_eq!(
                        &geometry, previous_geometry,
                        "wings advanced while position held at frame {frame}"
                    );
                }
            }
            previous = Some((tick, geometry));
        }
    }

    #[test]
    fn production_snapshots_publish_position_heading_and_wings_atomically() {
        use crate::particles::{ButterflyFrame, MotionMode, ParticleSpawn, ParticleSystem};
        for motion_mode in [MotionMode::Free, MotionMode::GuidedFlight] {
            let mut system = ParticleSystem::new(1);
            let handle = system
                .spawn(ParticleSpawn {
                    render_kind: ParticleRenderKind::Butterfly,
                    motion_mode,
                    position: Vec3::new(0., 0., -0.5),
                    lifetime: 30.,
                    ..Default::default()
                })
                .unwrap();
            system.set_butterfly_guided_flight(handle, true);
            system.advance_guided_flight(handle, Vec3::ZERO, 0.5);
            system.set_butterfly_guided_flight(handle, motion_mode == MotionMode::GuidedFlight);
            let mut snapshots = Vec::new();
            let mut renderer = ButterflyMeshRenderer::default();
            let mut previous = None;
            for step in 0..120 {
                let time = step as f32 / 120.;
                let frame = ButterflyFrame::at(time, 8);
                let physical = Vec3::new(time * 0.1, 0., -0.5);
                system.set_position(handle, physical);
                system.set_velocity(handle, Vec3::new(time.sin(), 0., time.cos()));
                system.write_snapshots_for_frame(&mut snapshots, frame);
                assert_eq!(system.position(handle), Some(physical));
                assert_eq!(
                    snapshots[0].animation_sample_time,
                    Some(frame.time_seconds())
                );
                renderer
                    .prepare(
                        &snapshots,
                        ButterflyMeshSettings {
                            resolution: 16,
                            fps: 8,
                            self_shadows: true,
                            transmission: 0.,
                        },
                        Vec3::ZERO,
                    )
                    .unwrap();
                assert_eq!(renderer.count(), 1);
                let geometry = bytemuck::cast_slice::<Triangle, u8>(&renderer.triangles).to_vec();
                if let Some((old_frame, ref old_geometry)) = previous {
                    if old_frame == frame {
                        assert_eq!(&geometry, old_geometry);
                    } else {
                        assert_eq!(snapshots[0].position_ws, physical);
                    }
                }
                previous = Some((frame, geometry));
            }
            // A reused slot must never inherit the previous animal's held pose.
            system.despawn(handle);
            system
                .spawn(ParticleSpawn {
                    render_kind: ParticleRenderKind::Butterfly,
                    position: Vec3::Y,
                    ..Default::default()
                })
                .unwrap();
            system.write_snapshots_for_frame(&mut snapshots, ButterflyFrame::at(119. / 120., 8));
            assert_eq!(snapshots[0].position_ws, Vec3::Y);
        }
    }

    #[test]
    fn pose_sampling_holds_and_loops_at_every_supported_fps() {
        let mesh = Mesh::load();
        for fps in 2..=60 {
            let pose =
                |time| mesh.pose(crate::particles::ButterflyFrame::at(time, fps).time_seconds());
            assert_eq!(pose(0.), pose(1.));
            assert_eq!(pose(0.), pose(0.9 / fps as f32));
            assert_ne!(pose(0.), pose(1.01 / fps as f32));
        }
    }
}
