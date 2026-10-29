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
const CAPACITY: usize =
    256 + crate::particles::fireflies::CAPACITY + crate::particles::bees::CAPACITY;
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
    flower_attachment: [u32; 4],
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
    hosts: Vec<Option<crate::particles::flower_visits::FlowerId>>,
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
        self.hosts.clear();
        self.draw_order.clear();
        let mode = (settings.resolution.clamp(8, MAX_RESOLUTION), settings.fps);
        if self.previous_mode != Some(mode) {
            log::info!("[BUTTERFLY_MODEL] resolution={} presentation_fps={} poses=published leaves=voxel_particles", mode.0, mode.1);
            self.previous_mode = Some(mode);
        }
        let source = crate::model_assets::butterfly();
        let mut candidates: Vec<_> = snapshots
            .iter()
            .filter(|s| s.kind.is_model() && s.color.w > 0. && s.size > 0.)
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
            if s.kind == ParticleRenderKind::Bee {
                let body = s.butterfly_body_pose;
                let phase = body.and_then(|p| p.phase).unwrap_or(
                    (s.animation_sample_time.unwrap() * 11. + s.animation_phase_offset)
                        .rem_euclid(1.),
                );
                let source = super::model_geometry::BEE_SOURCE_BASE
                    + (phase * super::model_geometry::BEE_POSES as f32) as u32
                        % super::model_geometry::BEE_POSES;
                let facing = body.map_or_else(
                    || crate::particles::butterfly_wingbeat::flight_orientation(s.velocity, 0.),
                    |p| p.orientation,
                );
                self.hosts.push(body.and_then(|p| p.host));
                self.instances.push(Instance {
                    position_size: s.position_ws.extend(s.size).to_array(),
                    color: s.color.to_array(),
                    metadata: [source, 0, settings.resolution.clamp(8, MAX_RESOLUTION), 8],
                    lighting: [0.15, 0., 0., 0.],
                    tile: [0; 4],
                    view_orientation: facing.to_array(),
                    flower_attachment: body
                        .map_or([0; 4], |p| [0, 0, p.yaw.to_bits(), p.clearance.to_bits()]),
                });
                continue;
            }
            if s.kind == ParticleRenderKind::Firefly {
                let time = s.animation_sample_time.unwrap();
                let phase = (time * 9. + s.animation_phase_offset).rem_euclid(1.);
                self.hosts.push(None);
                self.instances.push(Instance {
                    position_size: s.position_ws.extend(s.size).to_array(),
                    color: s.color.to_array(),
                    metadata: [
                        super::model_geometry::FIREFLY_SOURCE_BASE
                            + (phase * super::model_geometry::FIREFLY_POSES as f32) as u32,
                        0,
                        settings.resolution.clamp(8, MAX_RESOLUTION),
                        4,
                    ],
                    lighting: [
                        0.4,
                        crate::particles::fireflies::flash(time, s.animation_phase_offset)
                            * s.color.w,
                        0.,
                        0.,
                    ],
                    tile: [0; 4],
                    view_orientation: crate::particles::butterfly_wingbeat::flight_orientation(
                        s.velocity, 0.,
                    )
                    .to_array(),
                    flower_attachment: [0; 4],
                });
                continue;
            }
            let original_phase = (s.animation_sample_time.unwrap()
                * crate::particles::butterfly_wingbeat::FLIGHT_WINGBEATS_HZ
                + s.animation_phase_offset)
                .rem_euclid(1.);
            let coupling = s.butterfly_wingbeat;
            let blend = coupling.map_or(0., |p| p.blend);
            let phase = coupling.map_or(original_phase, |p| {
                if blend == 1. {
                    p.phase
                } else {
                    original_phase + ((p.phase - original_phase + 0.5).rem_euclid(1.) - 0.5) * blend
                }
            });
            let body = s.butterfly_body_pose;
            let phase = body.and_then(|p| p.phase).unwrap_or(phase);
            let transforms = source.transforms(phase, 0);
            let root_motion = transforms[source.node("Flight pose")].w_axis.truncate();
            let bank = coupling.map_or(0., |p| p.bank * blend);
            let facing = body.map_or_else(
                || crate::particles::butterfly_wingbeat::flight_orientation(s.velocity, bank),
                |p| p.orientation,
            );
            let rgb = ButterflyPalettePreset::from_index(s.palette_index).base_color_srgb();
            self.hosts.push(body.and_then(|p| p.host));
            self.instances.push(Instance {
                position_size: (s.position_ws
                    + facing
                        * root_motion
                        * (if body.is_some() { 0. } else { 1. - blend }
                            * s.size
                            * (1.53125 / 3.4)))
                    .extend(s.size)
                    .to_array(),
                color: [
                    rgb[0] as f32 / 255.,
                    rgb[1] as f32 / 255.,
                    rgb[2] as f32 / 255.,
                    s.color.w,
                ],
                metadata: [
                    if body.is_some_and(|p| p.host.is_some()) {
                        super::model_geometry::BUTTERFLY_REST_SOURCE_BASE
                            + (phase * (super::model_geometry::REST_FRAMES - 1) as f32).round()
                                as u32
                    } else {
                        super::model_geometry::BUTTERFLY_SOURCE_BASE
                            + super::model_geometry::animation_frame(phase)
                    },
                    0,
                    settings.resolution.clamp(8, MAX_RESOLUTION),
                    0,
                ],
                lighting: [settings.transmission.clamp(0., 1.), 0., 0., 0.],
                tile: [0; 4],
                view_orientation: facing.to_array(),
                flower_attachment: body
                    .map_or([0; 4], |p| [0, 0, p.yaw.to_bits(), p.clearance.to_bits()]),
            });
        }
        self.draw_order.extend(0..self.count());
        Ok(())
    }
    pub(super) fn flower_hosts(
        &self,
    ) -> std::collections::HashSet<crate::particles::flower_visits::FlowerId> {
        self.hosts.iter().flatten().copied().collect()
    }
    pub(super) fn bind_flower_hosts(
        &mut self,
        indices: &std::collections::HashMap<crate::particles::flower_visits::FlowerId, u32>,
    ) {
        for (instance, host) in self.instances.iter_mut().zip(&self.hosts) {
            if let Some(id) = host {
                if let Some(slot) = indices.get(id) {
                    instance.flower_attachment[0] = *slot;
                    instance.flower_attachment[1] = 1;
                } else {
                    instance.position_size[3] = 0.;
                }
            }
        }
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
            butterfly_body_pose: None,
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
    fn attached_pose_uses_fixed_wings_and_stable_host_mapping() {
        use crate::particles::flower_visits::{ButterflyBodyPose, FlowerId};
        let id = FlowerId {
            epoch: 3,
            plant: 7,
            head: 2,
        };
        let mut s = snapshot(ParticleRenderKind::Butterfly);
        s.butterfly_body_pose = Some(ButterflyBodyPose {
            orientation: Quat::IDENTITY,
            phase: Some(0.5),
            host: Some(id),
            yaw: 0.2,
            clearance: 0.1,
        });
        let mut r = ButterflyMeshRenderer::default();
        r.prepare_frame_models(&[s], settings(), Vec3::ZERO)
            .unwrap();
        assert_eq!(
            r.instances[0].position_size,
            s.position_ws.extend(s.size).to_array()
        );
        let phase = r.instances[0].metadata[0];
        r.bind_flower_hosts(&[(id, 5)].into());
        assert_eq!(&r.instances[0].flower_attachment[..2], &[5, 1]);
        s.animation_sample_time = Some(9.7);
        r.prepare_frame_models(&[s], settings(), Vec3::ZERO)
            .unwrap();
        assert_eq!(r.instances[0].metadata[0], phase);
        r.bind_flower_hosts(&Default::default());
        assert_eq!(
            r.instances[0].position_size[3], 0.,
            "missing/replaced host must not hover"
        );
    }

    #[test]
    fn foraging_model_stream_faces_published_motion_at_low_and_high_display_rates() {
        use crate::particles::{
            flower_visits::{FlowerId, FlowerTarget},
            ButterflyEmitter, ButterflyEmitterDesc, ButterflyFlightVariant, ButterflyFrame,
            ButterflySpawnSource, ParticleForces, ParticleSystem,
        };
        for fps in [8, 60] {
            for coupled in [false, true] {
                let mut desc = ButterflyEmitterDesc {
                    flight_variant: ButterflyFlightVariant::Darting,
                    ..Default::default()
                };
                desc.flight_tuning.speed = 0.25;
                desc.flight_tuning.vertical_strength = 4.;
                desc.flight_tuning.wingbeat_coupling = coupled;
                desc.foraging.dwell_seconds = 0.5;
                desc.lifetime_min = 100.;
                desc.lifetime_max = 100.;
                let mut emitter = ButterflyEmitter::new(17, &desc);
                let mut system = ParticleSystem::new(2);
                emitter
                    .spawn_at(&mut system, ButterflySpawnSource::tree_leaf(Vec3::ONE))
                    .unwrap();
                let mut flowers = [0, 1].map(|i| FlowerTarget {
                    id: FlowerId {
                        epoch: 1,
                        plant: i + 1,
                        head: 0,
                    },
                    species: i as u32,
                    position: Vec3::ONE + Vec3::X * i as f32 * 0.1,
                    normal: Vec3::Y,
                    tangent: Vec3::Z,
                    radius: 0.01,
                    velocity: Vec3::ZERO,
                    observed_at: 0.,
                });
                let mut renderer = ButterflyMeshRenderer::default();
                let mut snapshots = Vec::new();
                let mut checked = 0;
                for tick in 1..=1200 {
                    let dt = 1. / 120.;
                    let time = tick as f32 * dt;
                    for f in &mut flowers {
                        f.observed_at = time;
                    }
                    emitter.set_flowers(&flowers);
                    emitter.synchronize_animation_clock(time, dt);
                    system.update(dt, ParticleForces::default());
                    emitter.advance_guided_flight(
                        &mut system,
                        dt,
                        Vec3::splat(2.),
                        &crate::wind_field::WindFieldFrame::uniform(glam::Vec2::X * 0.3),
                        |_, _| None,
                    );
                    system.write_snapshots_for_frame(&mut snapshots, ButterflyFrame::at(time, fps));
                    let s = snapshots[0];
                    renderer
                        .prepare_frame_models(
                            &snapshots,
                            ButterflyMeshSettings { fps, ..settings() },
                            Vec3::ZERO,
                        )
                        .unwrap();
                    renderer
                        .publish_mesh_frame(|bytes| {
                            if !bytes.is_empty()
                                && s.velocity.length_squared() > 1e-10
                                && s.butterfly_body_pose.is_none_or(|p| p.host.is_none())
                            {
                                let instance: Instance = bytemuck::pod_read_unaligned(
                                    &bytes[..std::mem::size_of::<Instance>()],
                                );
                                let forward =
                                    Quat::from_array(instance.view_orientation) * Vec3::NEG_Z;
                                assert!(
                                    forward.dot(s.velocity.normalize()) > 0.99999,
                                    "tick={tick} fps={fps} coupling={coupled}"
                                );
                                checked += 1;
                            }
                            Ok(())
                        })
                        .unwrap();
                }
                assert!(checked > 100);
            }
        }
    }

    #[test]
    fn bees_use_their_own_cached_sources() {
        let mut renderer = ButterflyMeshRenderer::default();
        renderer
            .prepare_frame_models(&[snapshot(ParticleRenderKind::Bee)], settings(), Vec3::ZERO)
            .unwrap();
        let i = renderer.instances[0];
        assert_eq!(i.metadata[3], 8);
        assert!((super::super::model_geometry::BEE_SOURCE_BASE
            ..super::super::model_geometry::FLOWER_SOURCE_BASE)
            .contains(&i.metadata[0]));
    }
    #[test]
    fn firefly_uses_its_own_cached_source_and_blink_material() {
        let mut s = snapshot(ParticleRenderKind::Firefly);
        s.animation_sample_time = Some(0.384);
        s.animation_phase_offset = 0.;
        let mut renderer = ButterflyMeshRenderer::default();
        renderer
            .prepare_frame_models(&[s], settings(), Vec3::ZERO)
            .unwrap();
        let instance = renderer.instances[0];
        assert_eq!(instance.metadata[3], 4);
        assert!((super::super::model_geometry::FIREFLY_SOURCE_BASE
            ..super::super::model_geometry::FIREFLY_SOURCE_BASE
                + super::super::model_geometry::FIREFLY_POSES)
            .contains(&instance.metadata[0]));
        assert!(instance.lighting[1] > 0.99);
        assert_eq!(
            instance.position_size,
            s.position_ws.extend(s.size).to_array()
        );
        assert!(renderer.flower_hosts().is_empty());
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
        assert_eq!(std::mem::size_of::<Instance>(), 112);
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
