//! Butterfly/leaf pose and material inputs for the shared model pixel generator.
//! Both generate one sample per tile texel, then display through the same lookup
//! shader as apples. Visible tiles are compact, resolution-sized and batched;
//! no maximum-resolution allocation is reserved for inactive particle slots.
use anyhow::{ensure, Result};
use bytemuck::{Pod, Zeroable};
use glam::{Quat, Vec3};
use re_flora_vkn::{vk, Allocator, Buffer, BufferUsage, Device, MemoryLocation};
use resource_container_derive::ResourceContainer;

use super::ButterflyPalettePreset;
use crate::{
    particles::{ParticleRenderKind, ParticleSnapshot},
    resource::Resource,
};

// The current eight-chunk garden allows 16 live butterflies. Reserve room for
// larger gardens without allocating PARTICLE_CAPACITY * 64² (a gigabyte).
const CAPACITY: usize = 256;
pub const MAX_RESOLUTION: u32 = 64;
const LEAF_MODEL_FLAG: u32 = 2;

fn leaf_variant_index(seed: u32) -> usize {
    seed as usize % crate::model_assets::LEAF_VARIANT_COUNT
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
    /// Source voxels keep their exact size unless the model appearance is selected.
    pub fn render_size(self, snapshot: &ParticleSnapshot) -> f32 {
        if snapshot.kind == ParticleRenderKind::Leaf
            && snapshot.leaf_orientation.is_some()
            && (snapshot.leaf_geometry.is_none() || self.enabled)
        {
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
    pub transmission: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Instance {
    position_size: [f32; 4],
    color: [f32; 4],
    // immutable source, reserved, pixel resolution, flags (bit 1: leaf)
    metadata: [u32; 4],
    lighting: [f32; 4],
    tile: [u32; 4], // tile offset, visible, reserved, reserved
    view_orientation: [f32; 4],
}
#[derive(ResourceContainer)]
pub struct ButterflyMeshResources {
    pub butterfly_mesh_instances: Resource<Buffer>,
    pub model_pixel_tiles: Resource<Buffer>,
    pub model_object_samples: Resource<Buffer>,
    pub model_object_view_samples: Resource<Buffer>,
    pub draw_indices: Resource<Buffer>,
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
            4,
        );
        draw_indices.fill(&[0u32]).unwrap();
        Self {
            draw_indices: Resource::new(draw_indices),
            butterfly_mesh_instances: buffer(
                std::mem::size_of::<Instance>(),
                MemoryLocation::CpuToGpu,
            ),
            model_pixel_tiles: buffer(16, MemoryLocation::GpuOnly),
            model_object_samples: buffer(16, MemoryLocation::GpuOnly),
            model_object_view_samples: buffer(16, MemoryLocation::GpuOnly),
        }
    }
}

#[cfg(test)]
struct RestTriangle {
    side: f32,
    positions: [[f32; 3]; 3],
}
struct Mesh {
    source: &'static crate::model_assets::Model,
    #[cfg(test)]
    triangles: Vec<RestTriangle>,
}
impl Mesh {
    fn load() -> Self {
        let source = crate::model_assets::butterfly();
        #[cfg(test)]
        let left = source.node("L continuous fore-hind wing");
        #[cfg(test)]
        let right = source.node("R continuous fore-hind wing");
        #[cfg(test)]
        let triangles = source
            .triangles
            .iter()
            .map(|t| {
                assert!(t.node == left || t.node == right);
                RestTriangle {
                    side: if t.node == left { -1. } else { 1. },
                    positions: t.positions.map(|p| p.to_array()),
                }
            })
            .collect::<Vec<_>>();
        Self {
            source,
            #[cfg(test)]
            triangles,
        }
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
    pub resolution: u32,
    tile_count: u32,
    #[cfg(test)]
    pub tile_layout: super::model_pixel_tiles::ParticleTiles,
    pub dispatch_resolution: u32,
    previous_leaf_mode: Option<(bool, u32, u32)>,
    previous_mode: Option<(u32, u32, u32)>,
}
impl Default for ButterflyMeshRenderer {
    fn default() -> Self {
        Self {
            mesh: Mesh::load(),
            instances: Vec::new(),
            draw_order: Vec::new(),
            resolution: 22,
            tile_count: 0,
            #[cfg(test)]
            tile_layout: super::model_pixel_tiles::ParticleTiles::default(),
            dispatch_resolution: 22,
            previous_leaf_mode: None,
            previous_mode: None,
        }
    }
}
impl ButterflyMeshRenderer {
    pub fn count(&self) -> u32 {
        self.instances.len() as u32
    }

    pub(super) fn model_counts(&self) -> (u32, u32) {
        let leaves = self
            .instances
            .iter()
            .filter(|i| i.metadata[3] & LEAF_MODEL_FLAG != 0)
            .count() as u32;
        (self.count() - leaves, leaves)
    }
    fn prepare(
        &mut self,
        snapshots: &[ParticleSnapshot],
        settings: ButterflyMeshSettings,
        camera_position: Vec3,
    ) -> Result<()> {
        self.instances.clear();
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
            let rgb = ButterflyPalettePreset::from_index(snapshot.palette_index).base_color_srgb();
            self.instances.push(Instance {
                // Quantize articulation only; published rigid root motion stays continuous.
                position_size: (snapshot.position_ws
                    + facing * root_motion * ((1. - blend) * scale))
                    .extend(snapshot.size)
                    .to_array(),
                metadata: [
                    super::model_geometry::BUTTERFLY_SOURCE_BASE
                        + super::model_geometry::animation_frame(phase),
                    0,
                    self.resolution,
                    0,
                ],
                color: [
                    rgb[0] as f32 / 255.,
                    rgb[1] as f32 / 255.,
                    rgb[2] as f32 / 255.,
                    snapshot.color.w,
                ],
                lighting: [settings.transmission.clamp(0., 1.), 0., 0., 0.],
                view_orientation: facing.to_array(),
                tile: [0; 4],
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
        self.tile_count = 0;
        self.prepare(snapshots, butterflies, camera_position)?;
        self.tile_count = self.count();
        self.dispatch_resolution = self.resolution;
        if !leaves.enabled {
            return Ok(());
        }
        let candidates: Vec<_> = snapshots
            .iter()
            .filter(|s| leaves.uses_model(s) && s.size > 0. && s.color.w > 0.)
            .collect();
        ensure!(
            candidates.iter().all(|s| s
                .leaf_orientation
                .is_some_and(|q| q.is_finite() && q.is_normalized())
                && s.leaf_shape_seed.is_some()),
            "invalid published leaf orientation or shape seed"
        );
        let resolution = leaves.resolution.clamp(8, MAX_RESOLUTION);
        for snapshot in candidates {
            let shape = leaf_variant_index(snapshot.leaf_shape_seed.unwrap());
            self.instances.push(Instance {
                position_size: snapshot
                    .position_ws
                    .extend(leaves.render_size(snapshot))
                    .to_array(),
                color: snapshot.color.to_array(),
                metadata: [shape as u32, 0, resolution, LEAF_MODEL_FLAG],
                // No resampling, local animation, velocity-facing override or reset.
                lighting: snapshot.leaf_orientation.unwrap().to_array(),
                view_orientation: snapshot.leaf_orientation.unwrap().to_array(),
                tile: [0; 4],
            });
        }
        Ok(())
    }

    /// Ordinary geometry uses published simulation poses in back-to-front order.
    /// No tile packing, visibility samples or model-local pixel allocation.
    pub fn publish_mesh_frame(&self, publish: impl FnOnce(&[u8]) -> Result<()>) -> Result<()> {
        let ordered = self
            .draw_order
            .iter()
            .map(|&index| self.instances[index as usize])
            .collect::<Vec<_>>();
        publish(bytemuck::cast_slice(&ordered))
    }

    #[cfg(test)]
    fn publish_pixel_frame(
        &mut self,
        publish: impl FnOnce(&[Instance], &[u32]) -> Result<()>,
    ) -> Result<()> {
        self.tile_layout = super::model_pixel_tiles::ParticleTiles::pack(
            &self
                .instances
                .iter()
                .map(|i| (i.metadata[2], i.tile[1] != 0))
                .collect::<Vec<_>>(),
            &self.draw_order,
        );
        for (instance, &offset) in self.instances.iter_mut().zip(self.tile_layout.offsets()) {
            instance.tile[0] = offset;
        }
        if !self.instances.is_empty() {
            // One complete publication, only after sorted offsets and visibility
            // are final. CPU pose preparation must never upload its zeroed tile
            // metadata over an in-flight frame (a3661975).
            publish(&self.instances, &self.draw_order)?;
        }
        Ok(())
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
                let ids: Vec<_> = self.instances[self.tile_count as usize..]
                    .iter()
                    .map(|instance| instance.metadata[0] as usize)
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
            settings.transmission.clamp(0., 1.).to_bits(),
        );
        if self.previous_mode != Some(mode) {
            log::info!("[BUTTERFLY-MESH] tile={}x{} fps={} triangles_per_animal={} active={} capacity={CAPACITY} sun=game depth=per_texel", self.resolution,self.resolution,settings.fps,self.mesh.source.triangles.len(),self.tile_count);
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
    fn canonical_animation_keeps_published_motion_and_avoids_instance_geometry() {
        let mut renderer = ButterflyMeshRenderer::default();
        let settings = ButterflyMeshSettings {
            resolution: 16,
            fps: 22,
            transmission: 0.8,
        };
        for blend in [0., 0.35, 1.] {
            let pose = crate::particles::ButterflyWingbeatPose {
                phase: 0.413,
                blend,
                orientation: Quat::from_rotation_y(0.7),
            };
            let snapshot = ParticleSnapshot {
                position_ws: Vec3::ONE,
                velocity: Vec3::NEG_Z,
                color: glam::Vec4::ONE,
                size: 0.03,
                kind: ParticleRenderKind::Butterfly,
                palette_index: 0,
                animation_phase_offset: 0.,
                animation_sample_time: Some(0.137),
                butterfly_wingbeat: Some(pose),
                leaf_orientation: None,
                leaf_shape_seed: None,
                leaf_geometry: None,
            };
            renderer.prepare(&[snapshot], settings, Vec3::ZERO).unwrap();
            let instance = renderer.instances[0];
            let phase = 0.137 + (pose.phase - 0.137) * blend;
            let facing = Quat::IDENTITY.slerp(pose.orientation, blend);
            let transforms = renderer.mesh.source.transforms(phase, 0);
            let root = transforms[renderer.mesh.source.node("Flight pose")]
                .w_axis
                .truncate();
            let expected = snapshot.position_ws
                + facing * root * ((1. - blend) * snapshot.size * (1.53125 / 3.4));
            assert!((Vec3::from_slice(&instance.position_size) - expected).length() < 1e-7);
            assert!(
                (Quat::from_array(instance.view_orientation)
                    .dot(facing)
                    .abs()
                    - 1.)
                    .abs()
                    < 1e-6
            );
            assert_eq!(
                instance.metadata[0],
                super::super::model_geometry::BUTTERFLY_SOURCE_BASE
                    + super::super::model_geometry::animation_frame(phase)
            );
        }
    }

    #[test]
    fn publication_contains_complete_sorted_offsets_and_skips_empty_uploads() {
        let mut renderer = ButterflyMeshRenderer {
            instances: (0..1100)
                .map(|_| Instance {
                    position_size: [0.; 4],
                    color: [1.; 4],
                    metadata: [0, 0, 64, LEAF_MODEL_FLAG],
                    lighting: [0., 0., 0., 1.],
                    view_orientation: [0., 0., 0., 1.],
                    tile: [0, 1, 0, 0],
                })
                .collect(),
            draw_order: (0..1100u32).rev().collect(),
            ..Default::default()
        };
        let mut publications = 0;
        renderer
            .publish_pixel_frame(|instances, order| {
                publications += 1;
                assert_eq!(order, &(0..1100u32).rev().collect::<Vec<_>>());
                for (draw, &index) in order.iter().enumerate() {
                    assert_eq!(
                        instances[index as usize].tile,
                        [(draw as u32 % 1024) * 4096, 1, 0, 0]
                    );
                }
                Ok(())
            })
            .unwrap();
        assert_eq!(publications, 1);
        renderer.instances.clear();
        renderer.draw_order.clear();
        renderer
            .publish_pixel_frame(|_, _| panic!("empty frame must not upload stale models"))
            .unwrap();
        assert!(renderer.tile_layout.offsets().is_empty());
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
                        leaf_variant_index(snapshot.leaf_shape_seed.unwrap()) as u32,
                        0,
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
                    leaf_variant_index(original.leaf_shape_seed.unwrap()) as u32,
                    0,
                    16,
                    LEAF_MODEL_FLAG
                ]
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
        other.leaf_orientation = Some(glam::Quat::from_rotation_x(0.3));
        renderer
            .prepare_models(&[original, other], butterfly, leaves, Vec3::Z)
            .unwrap();
        assert_eq!(renderer.instances[1].metadata[0] as usize, second);
    }

    #[test]
    fn full_particle_capacity_publishes_only_shared_source_keys() {
        let mut system = crate::particles::ParticleSystem::new(1);
        system
            .spawn(crate::particles::ParticleSpawn::default())
            .unwrap();
        let mut source = Vec::new();
        system.write_snapshots(&mut source);
        // The renderer consumes demand, not the simulation's initial reservation.
        let snapshots = vec![source[0]; 32_769];
        let mut renderer = ButterflyMeshRenderer::default();
        renderer
            .prepare_models(
                &snapshots,
                ButterflyMeshSettings {
                    resolution: 16,
                    fps: 8,
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
        assert!(renderer.instances.iter().all(|i| i.metadata[0] < 64));
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
            leaf_geometry: None,
        };
        let settings = ButterflyMeshSettings {
            resolution: 16,
            fps: 8,
            transmission: 0.8,
        };
        let mut renderer = ButterflyMeshRenderer::default();
        renderer.prepare(&[snapshot], settings, Vec3::ZERO).unwrap();
        let geometry = bytemuck::cast_slice::<Instance, u8>(&renderer.instances).to_vec();
        let [_, _, bob] = renderer.mesh.pose(pose.phase);
        assert!(
            bob.abs() > 0.01,
            "test a source pose with visible authored bob"
        );
        assert_eq!(
            renderer.instances[0].position_size,
            snapshot.position_ws.extend(snapshot.size).to_array()
        );
        assert_eq!(
            renderer.instances[0].view_orientation,
            pose.orientation.to_array()
        );
        // The publication timestamp remains required, but cannot independently
        // animate a coupled pose. Nor may ground-relative velocity override yaw.
        snapshot.animation_sample_time = Some(123.731);
        snapshot.velocity = -Vec3::Y;
        renderer.prepare(&[snapshot], settings, Vec3::ZERO).unwrap();
        assert_eq!(
            geometry,
            bytemuck::cast_slice::<Instance, u8>(&renderer.instances)
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
            leaf_geometry: None,
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
            transmission: 0.,
        };
        for n in 8..=64 {
            settings.resolution = n;
            renderer.prepare(&snapshots, settings, Vec3::ZERO).unwrap();
            assert_eq!(renderer.count(), 2);
            assert!(renderer.instances.iter().all(|i| i.metadata[2] == n));
            assert_eq!(renderer.instances[0].position_size[2], -2.);
            assert_eq!(renderer.instances[0].color, renderer.instances[1].color);
        }
        for (requested, expected) in [(-1., 0.), (0., 0.), (0.5, 0.5), (1., 1.), (2., 1.)] {
            settings.transmission = requested;
            renderer.prepare(&snapshots, settings, Vec3::ZERO).unwrap();
            assert!(renderer.instances.iter().all(|i| i.lighting[0] == expected));
            assert!(renderer.instances.iter().all(|i| i.color[3] == 1.));
        }
        renderer.prepare(&[], settings, Vec3::ZERO).unwrap();
        assert_eq!(renderer.count(), 0);
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
                leaf_geometry: None,
            };
            renderer
                .prepare(
                    &[snapshot],
                    ButterflyMeshSettings {
                        resolution: 16,
                        fps: 8,
                        transmission: 0.,
                    },
                    Vec3::ZERO,
                )
                .unwrap();
            let geometry = bytemuck::cast_slice::<Instance, u8>(&renderer.instances).to_vec();
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
                            transmission: 0.,
                        },
                        Vec3::ZERO,
                    )
                    .unwrap();
                assert_eq!(renderer.count(), 1);
                let geometry = bytemuck::cast_slice::<Instance, u8>(&renderer.instances).to_vec();
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
    fn source_leaf_appearance_switch_preserves_pose_and_physical_size() {
        use crate::particles::{ParticleSpawn, ParticleSystem};
        let mut system = ParticleSystem::new(1);
        system.spawn(ParticleSpawn::default()).unwrap();
        let mut snapshots = Vec::new();
        system.write_snapshots(&mut snapshots);
        let mut source = snapshots[0];
        source.kind = ParticleRenderKind::Leaf;
        source.leaf_orientation = Some(Quat::from_rotation_y(0.3));
        source.leaf_geometry = Some(Quat::from_rotation_x(0.5));
        let original = source;
        let mut settings = LeafModelSettings {
            enabled: false,
            resolution: 22,
            size_scale: 4.0,
        };
        assert!(!settings.uses_model(&source));
        assert_eq!(settings.render_size(&source), source.size);
        settings.enabled = true;
        assert!(settings.uses_model(&source));
        assert_eq!(settings.render_size(&source), source.size * 4.0);
        assert_eq!(source.size, original.size);
        assert_eq!(source.leaf_geometry, original.leaf_geometry);
        assert_eq!(source.leaf_orientation, original.leaf_orientation);
        for kind in [
            ParticleRenderKind::Butterfly,
            ParticleRenderKind::WaterDroplet,
            ParticleRenderKind::TerrainVoxel,
        ] {
            source.kind = kind;
            assert!(!settings.uses_model(&source));
            assert_eq!(settings.render_size(&source), source.size);
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
