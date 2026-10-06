//! Published butterfly poses for native and pre-cached model rendering.
//! Falling leaves are ordinary voxel particles, never model instances.
use super::ButterflyPalettePreset;
use crate::{
    particles::{ParticleRenderKind, ParticleSnapshot},
    resource::Resource,
};
use anyhow::{ensure, Result};
use bytemuck::{Pod, Zeroable};
#[cfg(test)]
use glam::Quat;
use glam::Vec3;
use re_flora_vkn::{vk, Allocator, Buffer, BufferUsage, Device, MemoryLocation};
use resource_container_derive::ResourceContainer;
const CAPACITY: usize = 256;
pub const MAX_RESOLUTION: u32 = 64;
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
    metadata: [u32; 4],
    lighting: [f32; 4],
    tile: [u32; 4],
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
        let buffer = |bytes, location| {
            Resource::new(Buffer::new_sized(
                device.clone(),
                allocator.clone(),
                BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
                location,
                bytes,
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
            butterfly_mesh_instances: buffer(
                std::mem::size_of::<Instance>() as u64,
                MemoryLocation::CpuToGpu,
            ),
            model_pixel_tiles: buffer(16, MemoryLocation::GpuOnly),
            model_object_samples: buffer(16, MemoryLocation::GpuOnly),
            model_object_view_samples: buffer(16, MemoryLocation::GpuOnly),
            draw_indices: Resource::new(draw_indices),
        }
    }
}
#[derive(Default)]
pub(super) struct ButterflyMeshRenderer {
    instances: Vec<Instance>,
    draw_order: Vec<u32>,
    previous_mode: Option<(u32, u32)>,
}
impl ButterflyMeshRenderer {
    pub fn count(&self) -> u32 {
        self.instances.len() as u32
    }
    pub fn prepare_frame_models(
        &mut self,
        snapshots: &[ParticleSnapshot],
        settings: ButterflyMeshSettings,
        camera: Vec3,
    ) -> Result<()> {
        self.instances.clear();
        self.draw_order.clear();
        let mode = (settings.resolution.clamp(8, MAX_RESOLUTION), settings.fps);
        if self.previous_mode != Some(mode) {
            log::info!("[BUTTERFLY_MODEL] resolution={} presentation_fps={} poses=published leaves=voxel_particles", mode.0, mode.1);
            self.previous_mode = Some(mode);
        }
        let source = crate::model_assets::butterfly();
        let mut candidates: Vec<_> = snapshots
            .iter()
            .filter(|s| s.kind == ParticleRenderKind::Butterfly && s.color.w > 0. && s.size > 0.)
            .collect();
        ensure!(candidates.len() <= CAPACITY, "butterfly capacity exceeded");
        ensure!(
            candidates.iter().all(|s| s
                .animation_sample_time
                .is_some_and(|t| t.is_finite() && t >= 0.)),
            "butterfly snapshot missing a valid shared presentation timestamp"
        );
        candidates.sort_by(|a, b| {
            b.position_ws
                .distance_squared(camera)
                .total_cmp(&a.position_ws.distance_squared(camera))
        });
        for s in candidates {
            let original_phase =
                (s.animation_sample_time.unwrap() + s.animation_phase_offset).rem_euclid(1.);
            let coupling = s.butterfly_wingbeat;
            let blend = coupling.map_or(0., |p| p.blend);
            let phase = coupling.map_or(original_phase, |p| {
                if blend == 1. {
                    p.phase
                } else {
                    original_phase + ((p.phase - original_phase + 0.5).rem_euclid(1.) - 0.5) * blend
                }
            });
            let transforms = source.transforms(phase, 0);
            let root_motion = transforms[source.node("Flight pose")].w_axis.truncate();
            let bank = coupling.map_or(0., |p| p.bank * blend);
            let facing = crate::particles::butterfly_wingbeat::flight_orientation(s.velocity, bank);
            let rgb = ButterflyPalettePreset::from_index(s.palette_index).base_color_srgb();
            self.instances.push(Instance {
                position_size: (s.position_ws
                    + facing * root_motion * ((1. - blend) * s.size * (1.53125 / 3.4)))
                    .extend(s.size)
                    .to_array(),
                color: [
                    rgb[0] as f32 / 255.,
                    rgb[1] as f32 / 255.,
                    rgb[2] as f32 / 255.,
                    s.color.w,
                ],
                metadata: [
                    super::model_geometry::BUTTERFLY_SOURCE_BASE
                        + super::model_geometry::animation_frame(phase),
                    0,
                    settings.resolution.clamp(8, MAX_RESOLUTION),
                    0,
                ],
                lighting: [settings.transmission.clamp(0., 1.), 0., 0., 0.],
                tile: [0; 4],
                view_orientation: facing.to_array(),
            });
        }
        self.draw_order.extend(0..self.count());
        Ok(())
    }
    pub fn publish_mesh_frame(&self, publish: impl FnOnce(&[u8]) -> Result<()>) -> Result<()> {
        let ordered: Vec<_> = self
            .draw_order
            .iter()
            .map(|&i| self.instances[i as usize])
            .collect();
        publish(bytemuck::cast_slice(&ordered))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot(kind: ParticleRenderKind) -> ParticleSnapshot {
        ParticleSnapshot {
            position_ws: Vec3::ONE,
            velocity: Vec3::NEG_Z,
            color: glam::Vec4::ONE,
            size: 0.03,
            kind,
            palette_index: 0,
            animation_phase_offset: 0.,
            animation_sample_time: Some(0.137),
            butterfly_wingbeat: None,
            leaf_orientation: Some(Quat::IDENTITY),
            leaf_shape_seed: Some(17),
            leaf_geometry: Some(Quat::IDENTITY),
        }
    }
    fn settings() -> ButterflyMeshSettings {
        ButterflyMeshSettings {
            resolution: 16,
            fps: 8,
            transmission: 0.8,
        }
    }
    #[test]
    fn leaves_never_enter_the_model_stream() {
        let mut r = ButterflyMeshRenderer::default();
        r.prepare_frame_models(
            &[
                snapshot(ParticleRenderKind::Leaf),
                snapshot(ParticleRenderKind::Butterfly),
            ],
            settings(),
            Vec3::ZERO,
        )
        .unwrap();
        assert_eq!(r.count(), 1);
        assert_eq!(std::mem::size_of::<Instance>(), 96);
        assert_eq!(r.instances[0].metadata[3], 0);
        r.prepare_frame_models(&[], settings(), Vec3::ZERO).unwrap();
        assert_eq!(r.count(), 0);
    }
    #[test]
    fn coupled_pose_preserves_publication_and_has_no_duplicate_bob() {
        let mut s = snapshot(ParticleRenderKind::Butterfly);
        let orientation = Quat::from_rotation_z(0.2);
        s.butterfly_wingbeat = Some(crate::particles::ButterflyWingbeatPose {
            phase: 0.413,
            blend: 1.,
            bank: 0.2,
        });
        let mut r = ButterflyMeshRenderer::default();
        r.prepare_frame_models(&[s], settings(), Vec3::ZERO)
            .unwrap();
        let first = bytemuck::cast_slice::<Instance, u8>(&r.instances).to_vec();
        assert_eq!(
            r.instances[0].position_size,
            s.position_ws.extend(s.size).to_array()
        );
        assert_eq!(r.instances[0].view_orientation, orientation.to_array());
        s.animation_sample_time = Some(123.731);
        s.velocity = Vec3::Y;
        r.prepare_frame_models(&[s], settings(), Vec3::ZERO)
            .unwrap();
        assert_ne!(first, bytemuck::cast_slice::<Instance, u8>(&r.instances));
        let forward = Quat::from_array(r.instances[0].view_orientation) * Vec3::NEG_Z;
        assert!(forward.dot(s.velocity.normalize()) > 0.9999);
    }
    #[test]
    fn invalid_publication_does_not_retain_previous_instances() {
        let mut r = ButterflyMeshRenderer::default();
        let mut s = snapshot(ParticleRenderKind::Butterfly);
        r.prepare_frame_models(&[s], settings(), Vec3::ZERO)
            .unwrap();
        s.animation_sample_time = None;
        assert!(r
            .prepare_frame_models(&[s], settings(), Vec3::ZERO)
            .is_err());
        assert_eq!(r.count(), 0);
        assert!(r
            .prepare_frame_models(
                &vec![snapshot(ParticleRenderKind::Butterfly); CAPACITY + 1],
                settings(),
                Vec3::ZERO
            )
            .is_err());
    }
}
