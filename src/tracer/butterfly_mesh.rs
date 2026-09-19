//! Fixed per-animal pixel tiles, independent of world distance or flight mode.
//! The small approved mesh is traced once per tile texel, not per screen pixel.
use anyhow::{ensure, Result};
use bytemuck::{Pod, Zeroable};
use glam::{Quat, Vec3};
use re_flora_vkn::{vk, Allocator, Buffer, BufferUsage, Device, MemoryLocation};
use resource_container_derive::ResourceContainer;
use serde::Deserialize;

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

#[derive(Clone, Copy, Debug)]
pub struct ButterflyMeshSettings {
    pub resolution: u32,
    pub fps: u32,
    pub self_shadows: bool,
    pub transmission: f32,
    pub time_seconds: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Instance {
    position_size: [f32; 4],
    color: [f32; 4],
    // triangle start/count, tile resolution, self-shadow enabled
    metadata: [u32; 4],
    lighting: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Triangle {
    a: [f32; 4],
    e1: [f32; 4],
    e2: [f32; 4],
}

#[derive(ResourceContainer)]
pub struct ButterflyMeshResources {
    pub butterfly_mesh_instances: Resource<Buffer>,
    pub butterfly_mesh_triangles: Resource<Buffer>,
    pub butterfly_pixel_tiles: Resource<Buffer>,
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
            BufferUsage::from_flags(vk::BufferUsageFlags::VERTEX_BUFFER),
            MemoryLocation::CpuToGpu,
            (CAPACITY * 4) as u64,
        );
        draw_indices
            .fill(&(0..CAPACITY as u32).collect::<Vec<_>>())
            .unwrap();
        Self {
            draw_indices: Resource::new(draw_indices),
            butterfly_mesh_instances: buffer(
                CAPACITY * std::mem::size_of::<Instance>(),
                MemoryLocation::CpuToGpu,
            ),
            butterfly_mesh_triangles: buffer(
                CAPACITY * MAX_TRIANGLES * std::mem::size_of::<Triangle>(),
                MemoryLocation::CpuToGpu,
            ),
            butterfly_pixel_tiles: Resource::new(Buffer::new_sized(
                device.clone(),
                allocator.clone(),
                BufferUsage::from_flags(
                    vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC,
                ),
                MemoryLocation::GpuOnly,
                (CAPACITY * MAX_RESOLUTION as usize * MAX_RESOLUTION as usize * 16) as u64,
            )),
        }
    }
}

#[derive(Deserialize)]
struct RestTriangle {
    side: f32,
    positions: [[f32; 3]; 3],
}
#[derive(Deserialize)]
struct Mesh {
    source_fps: usize,
    keys: Vec<[f32; 3]>,
    triangles: Vec<RestTriangle>,
}
impl Mesh {
    fn load() -> Self {
        let mesh: Self =
            serde_json::from_str(include_str!("../../assets/butterfly/wing-mesh.json"))
                .expect("validated, embedded butterfly source");
        assert_eq!(mesh.keys.len(), mesh.source_fps + 1);
        assert!(mesh.triangles.len() <= MAX_TRIANGLES);
        mesh
    }
    fn pose(&self, phase: f32, fps: u32) -> [f32; 3] {
        let fps = fps.clamp(2, 60) as f32;
        let sampled = (phase.rem_euclid(1.0) * fps).floor() / fps;
        let key = sampled * self.source_fps as f32;
        let i = (key.floor() as usize).min(self.source_fps - 1);
        std::array::from_fn(|c| {
            self.keys[i][c] + (self.keys[i + 1][c] - self.keys[i][c]) * key.fract()
        })
    }
}

pub(super) struct ButterflyMeshRenderer {
    mesh: Mesh,
    instances: Vec<Instance>,
    triangles: Vec<Triangle>,
    pub resolution: u32,
    previous_mode: Option<(u32, u32, bool, u32)>,
    validated_mode: Option<(u32, u32, bool, u32)>,
}
impl Default for ButterflyMeshRenderer {
    fn default() -> Self {
        Self {
            mesh: Mesh::load(),
            instances: Vec::new(),
            triangles: Vec::new(),
            resolution: 22,
            previous_mode: None,
            validated_mode: None,
        }
    }
}
impl ButterflyMeshRenderer {
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
        // Lifetime fading is object-level alpha, not transparent wing material.
        // Back-to-front submission preserves overlapping fading silhouettes.
        candidates.sort_by(|a, b| {
            b.position_ws
                .distance_squared(camera_position)
                .total_cmp(&a.position_ws.distance_squared(camera_position))
        });
        for snapshot in candidates {
            let [wing, pitch, bob] = self.mesh.pose(
                settings.time_seconds + snapshot.animation_phase_offset,
                settings.fps,
            );
            let velocity = snapshot.velocity;
            let speed = velocity.x.hypot(velocity.z);
            let yaw = if speed > 0.0001 {
                (-velocity.x).atan2(-velocity.z)
            } else {
                0.0
            };
            let flight_pitch = velocity.y.atan2(speed.max(0.001)).clamp(-0.4, 0.4);
            let facing = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(flight_pitch);
            // Keep the same nominal billboard footprint as the old particles.
            // 3.4 is the approved browser's fixed framing span, not a fitted
            // per-frame silhouette: flapping must not pump the pixel scale.
            let scale = snapshot.size * (1.53125 / 3.4);
            let start = self.triangles.len() as u32;
            for triangle in &self.mesh.triangles {
                let rotation =
                    Quat::from_rotation_x(pitch) * Quat::from_rotation_z(wing * triangle.side);
                let p = triangle.positions.map(|p| {
                    snapshot.position_ws
                        + facing * (rotation * Vec3::from(p) + Vec3::Y * bob) * scale
                });
                self.triangles.push(Triangle {
                    a: p[0].extend(0.).to_array(),
                    e1: (p[1] - p[0]).extend(triangle.side).to_array(),
                    e2: (p[2] - p[0]).extend(0.).to_array(),
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

    pub fn upload(
        &mut self,
        resources: &ButterflyMeshResources,
        snapshots: &[ParticleSnapshot],
        settings: ButterflyMeshSettings,
        camera_position: Vec3,
    ) -> Result<()> {
        self.prepare(snapshots, settings, camera_position)?;
        if !self.instances.is_empty() {
            resources.butterfly_mesh_instances.fill(&self.instances)?;
            resources.butterfly_mesh_triangles.fill(&self.triangles)?;
        }
        let mode = (
            self.resolution,
            settings.fps,
            settings.self_shadows,
            settings.transmission.clamp(0., 1.).to_bits(),
        );
        if self.previous_mode != Some(mode) {
            log::info!("[BUTTERFLY-MESH] tile={}x{} fps={} self_shadows={} triangles_per_animal={} active={} capacity={CAPACITY} sun=game depth=per_texel", self.resolution,self.resolution,settings.fps,settings.self_shadows,self.mesh.triangles.len(),self.count());
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
    fn approved_source_is_closed_wing_geometry_with_looping_keys() {
        let mesh = Mesh::load();
        assert_eq!(mesh.triangles.len(), 156);
        for (a, b) in mesh.keys[0].iter().zip(mesh.keys.last().unwrap()) {
            assert!((a - b).abs() < 1e-6);
        }
        for t in &mesh.triangles {
            assert!(t.side == 1.0 || t.side == -1.0);
            let p = t.positions.map(Vec3::from);
            assert!((p[1] - p[0]).cross(p[2] - p[0]).length_squared() > 1e-12);
            for frame in 0..60 {
                let [wing, pitch, bob] = mesh.pose(frame as f32 / 60., 60);
                let rot = Quat::from_rotation_x(pitch) * Quat::from_rotation_z(wing * t.side);
                for v in p {
                    assert!((rot * v + Vec3::Y * bob).length() < 1.7);
                }
            }
        }
        assert_eq!(std::mem::size_of::<Instance>(), 64);
        assert_eq!(std::mem::size_of::<Triangle>(), 48);
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
            leaf_orientation: None,
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
            time_seconds: 0.,
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
    }

    #[test]
    fn pose_sampling_holds_and_loops_at_every_supported_fps() {
        let mesh = Mesh::load();
        for fps in 2..=60 {
            assert_eq!(mesh.pose(0., fps), mesh.pose(1., fps));
            assert_eq!(mesh.pose(0., fps), mesh.pose(0.9 / fps as f32, fps));
            assert_ne!(mesh.pose(0., fps), mesh.pose(1.01 / fps as f32, fps));
        }
    }
}
