use glam::{Mat3, UVec3, Vec3};

use super::collider::{WaterBoxCollider, WaterTerrainColliderChunk, WaterTerrainColliderSet};
use std::sync::Arc;

const DEFAULT_GRID_DIM: UVec3 = UVec3::new(32, 32, 32);
const DEFAULT_PARTICLE_COUNT: usize = 4_096;
// Particles are initially seeded into a compact pond volume, not the whole
// chunk-sized simulation box. Keeping the rest volume below the full box volume
// avoids pressure-driven expansion into terrain solids and artificial walls.
const DEFAULT_PARTICLE_FILL_VOLUME_FRACTION: f32 = 0.1;

#[derive(Clone, Debug)]
pub struct PondWaterConfig {
    pub collider: WaterBoxCollider,
    pub grid_dim: UVec3,
    pub particle_count: usize,
    pub substep_dt: f32,
    pub particle_mass: f32,
    pub particle_volume: f32,
    pub gravity: Vec3,
    pub stiffness: f32,
    pub gamma: f32,
    pub j_min: f32,
    pub terrain_collision_margin_cells: f32,
    pub linear_damping_per_sec: f32,
    pub wall_padding_cells: f32,
    pub wall_damping: f32,
}

impl Default for PondWaterConfig {
    fn default() -> Self {
        Self {
            collider: WaterBoxCollider::default(),
            grid_dim: DEFAULT_GRID_DIM,
            particle_count: DEFAULT_PARTICLE_COUNT,
            substep_dt: 1.0 / 240.0,
            particle_mass: 1.0,
            particle_volume: DEFAULT_PARTICLE_FILL_VOLUME_FRACTION / DEFAULT_PARTICLE_COUNT as f32,
            gravity: Vec3::new(0.0, -9.8, 0.0),
            stiffness: 10_000.0,
            gamma: 7.0,
            j_min: 0.1,
            terrain_collision_margin_cells: 0.5,
            linear_damping_per_sec: 0.0,
            wall_padding_cells: 2.0,
            wall_damping: 0.0,
        }
    }
}

impl PondWaterConfig {
    pub fn with_particle_count(mut self, particle_count: usize) -> Self {
        assert!(particle_count > 0);
        self.particle_count = particle_count;
        self.particle_volume = DEFAULT_PARTICLE_FILL_VOLUME_FRACTION / particle_count as f32;
        self
    }

    pub fn with_cubic_grid_dim(mut self, grid_dim: u32) -> Self {
        assert!(grid_dim >= 4);
        self.grid_dim = UVec3::splat(grid_dim);
        self
    }

    pub fn with_substep_hz(mut self, substep_hz: f32) -> Self {
        assert!(substep_hz > 0.0 && substep_hz.is_finite());
        self.substep_dt = substep_hz.recip();
        self
    }

    pub fn with_terrain_collision_margin_cells(mut self, margin_cells: f32) -> Self {
        assert!(margin_cells >= 0.0 && margin_cells.is_finite());
        self.terrain_collision_margin_cells = margin_cells;
        self
    }

    pub fn with_linear_damping_per_sec(mut self, damping_per_sec: f32) -> Self {
        assert!(damping_per_sec >= 0.0 && damping_per_sec.is_finite());
        self.linear_damping_per_sec = damping_per_sec;
        self
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WaterParticle {
    pub x: Vec3,
    pub v: Vec3,
    pub c: Mat3,
    pub j: f32,
}

impl WaterParticle {
    fn new(x: Vec3) -> Self {
        Self {
            x,
            v: Vec3::ZERO,
            c: Mat3::ZERO,
            j: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WaterGridNode {
    pub v: Vec3,
    pub mass: f32,
    pub solid: bool,
    pub normal: Vec3,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct WaterTerrainGridSample {
    pub sdf: f32,
    pub normal: Vec3,
    pub near_surface: bool,
    pub has_sdf: bool,
}

impl Default for WaterTerrainGridSample {
    fn default() -> Self {
        Self {
            sdf: f32::INFINITY,
            normal: Vec3::ZERO,
            near_surface: false,
            has_sdf: false,
        }
    }
}

impl Default for WaterGridNode {
    fn default() -> Self {
        Self {
            v: Vec3::ZERO,
            mass: 0.0,
            solid: false,
            normal: Vec3::ZERO,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct WaterPerfStats {
    pub substeps: u32,
    pub repair_seconds: f64,
    pub clear_seconds: f64,
    pub p2g_seconds: f64,
    pub grid_seconds: f64,
    pub g2p_seconds: f64,
    pub g2p_gather_seconds: f64,
    pub g2p_box_seconds: f64,
    pub g2p_terrain_seconds: f64,
    pub g2p_repair_seconds: f64,
    pub total_seconds: f64,
    pub active_node_visits: u64,
    pub g2p_terrain_cache_skips: u64,
    pub g2p_terrain_cache_projections: u64,
    pub g2p_terrain_exact_fallbacks: u64,
    pub g2p_terrain_exact_checks: u64,
    pub g2p_terrain_exact_corrections: u64,
    pub g2p_terrain_shadow_samples: u64,
    pub g2p_terrain_shadow_false_skips: u64,
    pub g2p_terrain_shadow_sdf_abs_error_sum: f64,
    pub g2p_terrain_shadow_sdf_abs_error_max: f32,
}

impl WaterPerfStats {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

pub struct PondWaterSim {
    pub config: PondWaterConfig,
    pub(crate) terrain: Option<WaterTerrainColliderSet>,
    pub origin_ws: Vec3,
    pub extent_ws: Vec3,
    pub grid_dim: UVec3,
    pub dx: f32,
    pub inv_dx: f32,
    pub particles: Vec<WaterParticle>,
    pub grid: Vec<WaterGridNode>,
    pub(crate) terrain_grid: Vec<WaterTerrainGridSample>,
    pub accumulator: f32,
    pub perf_stats: WaterPerfStats,
    pub perf_report_seconds: f32,
}

impl PondWaterSim {
    pub fn new(config: PondWaterConfig) -> Self {
        assert!(config.grid_dim.x >= 4 && config.grid_dim.y >= 4 && config.grid_dim.z >= 4);
        assert!(config.particle_count > 0);

        let origin_ws = config.collider.min_ws;
        let extent_ws = config.collider.extent();
        assert!(extent_ws.min_element() > 0.0);

        let dx = (extent_ws.x / config.grid_dim.x as f32)
            .min(extent_ws.y / config.grid_dim.y as f32)
            .min(extent_ws.z / config.grid_dim.z as f32);
        let inv_dx = dx.recip();

        let grid_len = (config.grid_dim.x as usize)
            .saturating_mul(config.grid_dim.y as usize)
            .saturating_mul(config.grid_dim.z as usize);
        let mut sim = Self {
            config,
            terrain: None,
            origin_ws,
            extent_ws,
            grid_dim: DEFAULT_GRID_DIM,
            dx,
            inv_dx,
            particles: Vec::new(),
            grid: vec![WaterGridNode::default(); grid_len],
            terrain_grid: vec![WaterTerrainGridSample::default(); grid_len],
            accumulator: 0.0,
            perf_stats: WaterPerfStats::default(),
            perf_report_seconds: 0.0,
        };
        sim.grid_dim = sim.config.grid_dim;
        sim.seed_particles();
        sim
    }

    pub fn fixed_test_box() -> Self {
        Self::new(PondWaterConfig::default())
    }

    pub fn set_terrain_collider_set(&mut self, collider_set: WaterTerrainColliderSet) {
        collider_set.validate();
        self.terrain = Some(collider_set);
        self.rebuild_terrain_grid_cache();
        self.stabilize_after_terrain_change();
    }

    pub fn upsert_terrain_collider_chunk(
        &mut self,
        chunk: WaterTerrainColliderChunk,
        stabilize_particles: bool,
    ) {
        self.terrain
            .get_or_insert_with(WaterTerrainColliderSet::new)
            .insert_chunk(Arc::new(chunk));
        self.rebuild_terrain_grid_cache();
        if stabilize_particles {
            self.stabilize_after_terrain_change();
        }
    }

    pub fn clear_terrain_collider_set(&mut self) {
        self.terrain = None;
        self.rebuild_terrain_grid_cache();
        self.stabilize_after_terrain_change();
    }

    fn stabilize_after_terrain_change(&mut self) {
        self.accumulator = 0.0;
        let terrain = self.terrain.as_ref();
        let collision_margin = self.terrain_collision_margin();
        let max_correction = self.dx * self.config.wall_padding_cells.max(1.0);
        let particle_padding = self.dx * self.config.wall_padding_cells.max(1.0);
        let bounds = self.config.collider;

        for particle in &mut self.particles {
            particle.v = Vec3::ZERO;
            particle.c = Mat3::ZERO;
            particle.j = 1.0;

            if let Some(terrain) = terrain {
                for _ in 0..8 {
                    let Some((sdf, normal)) = terrain.sample_sdf_and_normal_ws(particle.x) else {
                        break;
                    };
                    let correction = collision_margin - sdf;
                    if correction <= 1.0e-5 {
                        break;
                    }
                    particle.x += normal * correction.min(max_correction);
                    particle.x = bounds.clamp_point(particle.x, particle_padding);
                }
            }
        }
    }

    pub fn terrain_collider_set(&self) -> Option<&WaterTerrainColliderSet> {
        self.terrain.as_ref()
    }

    pub(crate) fn terrain_collision_margin(&self) -> f32 {
        self.dx * self.config.terrain_collision_margin_cells.max(0.0)
    }

    pub fn spawn_debug_particles_at_surface(
        &mut self,
        surface_point_ws: Vec3,
        count: usize,
        radius_ws: f32,
    ) -> usize {
        if count == 0
            || radius_ws <= 0.0
            || !radius_ws.is_finite()
            || !surface_point_ws.is_finite()
            || !self.config.collider.contains(surface_point_ws)
        {
            return 0;
        }

        let terrain = self.terrain.as_ref();
        let collision_margin = self.terrain_collision_margin();
        let bounds = self.config.collider;
        let padding = (self.dx * self.config.wall_padding_cells.max(1.0))
            .min(self.extent_ws.min_element() * 0.25);
        let safe_min = bounds.min_ws + Vec3::splat(padding);
        let safe_max = bounds.max_ws - Vec3::splat(padding);
        if safe_min.cmpge(safe_max).any() {
            return 0;
        }

        // A dense random blob injected inside the existing pond creates an
        // immediate pressure impulse. Spread debug water across a shallow disk
        // and place it above the local water surface so it settles in instead
        // of appearing already compressed inside the fluid volume.
        let max_horizontal_radius = ((safe_max.x - safe_min.x) * 0.5)
            .min((safe_max.z - safe_min.z) * 0.5);
        let horizontal_radius = radius_ws.max(self.dx * 0.5).min(max_horizontal_radius);
        let vertical_radius = (radius_ws * 0.2)
            .max(self.dx * 0.25)
            .min((safe_max.y - safe_min.y) * 0.25);
        if horizontal_radius <= 0.0 || vertical_radius <= 0.0 {
            return 0;
        }

        let spawn_x = surface_point_ws.x.clamp(
            safe_min.x + horizontal_radius,
            safe_max.x - horizontal_radius,
        );
        let spawn_z = surface_point_ws.z.clamp(
            safe_min.z + horizontal_radius,
            safe_max.z - horizontal_radius,
        );
        let (local_surface_y, local_average_j) =
            self.local_water_surface_y_and_average_j(spawn_x, spawn_z, horizontal_radius * 1.5);
        let base_y = surface_point_ws.y.max(local_surface_y.unwrap_or(surface_point_ws.y));
        let spawn_y = (base_y + self.dx * 1.5 + vertical_radius)
            .clamp(safe_min.y + vertical_radius, safe_max.y - vertical_radius);
        let spawn_center = Vec3::new(spawn_x, spawn_y, spawn_z);
        let spawn_j = local_average_j
            .unwrap_or(1.0)
            .clamp(self.config.j_min.max(1.0e-4), 8.0);

        let mut spawned = 0usize;
        self.particles.reserve(count);

        const GOLDEN_ANGLE: f32 = 2.399_963_1;
        let inv_count = (count as f32).recip();
        for i in 0..count {
            let i_u32 = i as u32;
            let t = (i as f32 + 0.5) * inv_count;
            let angle = i as f32 * GOLDEN_ANGLE;
            let radial = horizontal_radius * t.sqrt();
            let jitter = Vec3::new(
                radial * angle.cos(),
                (hash_unit(31, i_u32, 97) - 0.5) * 2.0 * vertical_radius,
                radial * angle.sin(),
            );
            let mut pos = spawn_center + jitter;

            if let Some(terrain) = terrain {
                for _ in 0..4 {
                    let Some((sdf, normal)) = terrain.sample_sdf_and_normal_ws(pos) else {
                        break;
                    };
                    let correction = collision_margin - sdf;
                    if correction <= 1.0e-5 {
                        break;
                    }
                    pos += normal * (correction + self.dx * 0.1);
                }
            }

            let mut particle = WaterParticle::new(bounds.clamp_point(pos, padding));
            particle.j = spawn_j;
            self.particles.push(particle);
            spawned += 1;
        }

        if spawned > 0 {
            self.accumulator = 0.0;
        }

        spawned
    }

    fn local_water_surface_y_and_average_j(
        &self,
        center_x: f32,
        center_z: f32,
        radius_ws: f32,
    ) -> (Option<f32>, Option<f32>) {
        if radius_ws <= 0.0 || !radius_ws.is_finite() {
            return (None, None);
        }

        let radius_sq = radius_ws * radius_ws;
        let mut max_y = f32::NEG_INFINITY;
        let mut sum_j = 0.0;
        let mut count = 0usize;
        for particle in &self.particles {
            let dx = particle.x.x - center_x;
            let dz = particle.x.z - center_z;
            if dx * dx + dz * dz <= radius_sq {
                max_y = max_y.max(particle.x.y);
                if particle.j.is_finite() {
                    sum_j += particle.j;
                    count += 1;
                }
            }
        }

        (
            max_y.is_finite().then_some(max_y),
            (count > 0).then_some(sum_j / count as f32),
        )
    }

    fn seed_particles(&mut self) {
        self.particles.clear();
        self.particles.reserve(self.config.particle_count);

        let side = (self.config.particle_count as f32).cbrt().ceil() as u32;
        let spacing = self.extent_ws / Vec3::splat(side as f32);
        let min = self.origin_ws + self.extent_ws * Vec3::new(0.15, 0.08, 0.15);
        let max = self.origin_ws + self.extent_ws * Vec3::new(0.85, 0.58, 0.85);
        let fill_extent = max - min;

        'z: for z in 0..side {
            for y in 0..side {
                for x in 0..side {
                    if self.particles.len() >= self.config.particle_count {
                        break 'z;
                    }

                    let t = (Vec3::new(x as f32, y as f32, z as f32) + Vec3::splat(0.5))
                        / Vec3::splat(side as f32);
                    let jitter = Vec3::new(
                        hash_unit(x, y, z) - 0.5,
                        hash_unit(y, z, x) - 0.5,
                        hash_unit(z, x, y) - 0.5,
                    ) * spacing
                        * 0.15;
                    let pos = min + fill_extent * t + jitter;
                    self.particles.push(WaterParticle::new(
                        self.config.collider.clamp_point(pos, self.dx),
                    ));
                }
            }
        }
    }
}

fn hash_unit(x: u32, y: u32, z: u32) -> f32 {
    let mut n =
        x.wrapping_mul(73_856_093) ^ y.wrapping_mul(19_349_663) ^ z.wrapping_mul(83_492_791);
    n ^= n >> 13;
    n = n.wrapping_mul(1_274_126_177);
    ((n & 0x00ff_ffff) as f32) / 0x0100_0000 as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_test_box_seeds_particles_inside_bounds() {
        let sim = PondWaterSim::fixed_test_box();
        assert_eq!(sim.particles.len(), DEFAULT_PARTICLE_COUNT);
        assert_eq!(sim.grid_dim, DEFAULT_GRID_DIM);
        for particle in &sim.particles {
            assert!(
                sim.config.collider.contains(particle.x),
                "particle escaped: {:?}",
                particle.x
            );
        }
    }

    #[test]
    fn terrain_collider_state_round_trips() {
        let mut sim = PondWaterSim::fixed_test_box();
        assert!(sim.terrain_collider_set().is_none());

        sim.set_terrain_collider_set(WaterTerrainColliderSet::from_chunk(test_chunk(
            glam::IVec3::new(1, 0, 1),
        )));
        assert!(sim.terrain_collider_set().is_some());

        sim.clear_terrain_collider_set();
        assert!(sim.terrain_collider_set().is_none());
    }

    #[test]
    fn upserting_chunk_without_stabilization_preserves_particle_state() {
        let mut sim = PondWaterSim::fixed_test_box();
        sim.particles[0].v = Vec3::new(1.0, 2.0, 3.0);
        sim.particles[0].j = 1.25;

        sim.upsert_terrain_collider_chunk(test_chunk(glam::IVec3::new(2, 0, 1)), false);

        assert_eq!(sim.particles[0].v, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(sim.particles[0].j, 1.25);
    }

    #[test]
    fn debug_spawn_adds_particles_inside_current_bounds() {
        let mut sim = PondWaterSim::fixed_test_box();
        let initial_len = sim.particles.len();

        let spawned = sim.spawn_debug_particles_at_surface(Vec3::new(1.5, 0.5, 1.5), 12, 0.05);

        assert_eq!(spawned, 12);
        assert_eq!(sim.particles.len(), initial_len + 12);
        for particle in sim.particles.iter().skip(initial_len) {
            assert!(sim.config.collider.contains(particle.x));
        }
    }

    #[test]
    fn debug_spawn_rejects_points_outside_current_bounds() {
        let mut sim = PondWaterSim::fixed_test_box();
        let initial_len = sim.particles.len();

        let spawned = sim.spawn_debug_particles_at_surface(Vec3::new(0.5, 0.5, 1.5), 12, 0.05);

        assert_eq!(spawned, 0);
        assert_eq!(sim.particles.len(), initial_len);
    }

    #[test]
    fn debug_spawn_uses_local_water_surface_height() {
        let mut sim = PondWaterSim::fixed_test_box();
        let initial_len = sim.particles.len();
        let spawn_x = 1.5;
        let spawn_z = 1.5;
        let before_surface_y = max_particle_y_near(&sim, spawn_x, spawn_z, 0.18);

        let spawned = sim.spawn_debug_particles_at_surface(Vec3::new(spawn_x, 0.1, spawn_z), 16, 0.08);

        assert_eq!(spawned, 16);
        let spawned_min_y = sim.particles[initial_len..]
            .iter()
            .map(|particle| particle.x.y)
            .fold(f32::INFINITY, f32::min);
        assert!(
            spawned_min_y >= before_surface_y + sim.dx - 1.0e-5,
            "spawned_min_y={spawned_min_y} before_surface_y={before_surface_y} dx={}",
            sim.dx,
        );
    }

    fn max_particle_y_near(sim: &PondWaterSim, center_x: f32, center_z: f32, radius: f32) -> f32 {
        let radius_sq = radius * radius;
        sim.particles
            .iter()
            .filter(|particle| {
                let dx = particle.x.x - center_x;
                let dz = particle.x.z - center_z;
                dx * dx + dz * dz <= radius_sq
            })
            .map(|particle| particle.x.y)
            .fold(f32::NEG_INFINITY, f32::max)
    }

    fn test_chunk(chunk_id: glam::IVec3) -> crate::WaterTerrainColliderChunk {
        crate::WaterTerrainColliderChunk {
            chunk_id,
            dim: glam::UVec3::splat(2),
            sdf_ws: vec![1.0; 8],
            revision: 0,
        }
    }
}
