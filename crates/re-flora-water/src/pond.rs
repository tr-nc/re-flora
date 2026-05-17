use glam::{IVec3, Mat3, UVec3, Vec3};

use super::collider::{WaterBoxCollider, WaterTerrainColliderChunk, WaterTerrainColliderSet};
use std::sync::Arc;

const DEFAULT_GRID_DIM: UVec3 = UVec3::new(64, 32, 64);
const DEFAULT_PARTICLE_COUNT: usize = 0;
const DEFAULT_PARTICLE_VOLUME_REFERENCE_COUNT: usize = 4_096;
const DEBUG_SPAWN_HYDROSTATIC_J_MAX_DEPTH_CELLS: f32 = 4.0;
const INITIAL_PARTICLE_CHUNK_MIN_WS: Vec3 = Vec3::new(1.0, 0.0, 1.0);
const INITIAL_PARTICLE_CHUNK_MAX_WS: Vec3 = Vec3::new(2.0, 1.0, 2.0);
// Keep particle rest volume tied to the original one-chunk pond, even though
// the containing box is now wider and the default starts empty. This preserves
// the old 4096-particle density when water is added later by debug spawn.
const DEFAULT_INITIAL_PARTICLE_VOLUME_FRACTION: f32 = 0.1;

pub(crate) const WATER_GRID_BOUNDARY_X_MIN: u8 = 1 << 0;
pub(crate) const WATER_GRID_BOUNDARY_X_MAX: u8 = 1 << 1;
pub(crate) const WATER_GRID_BOUNDARY_Y_MIN: u8 = 1 << 2;
pub(crate) const WATER_GRID_BOUNDARY_Y_MAX: u8 = 1 << 3;
pub(crate) const WATER_GRID_BOUNDARY_Z_MIN: u8 = 1 << 4;
pub(crate) const WATER_GRID_BOUNDARY_Z_MAX: u8 = 1 << 5;

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
        let collider = WaterBoxCollider::default();
        Self {
            collider,
            grid_dim: DEFAULT_GRID_DIM,
            particle_count: DEFAULT_PARTICLE_COUNT,
            substep_dt: 1.0 / 240.0,
            particle_mass: 1.0,
            particle_volume: default_particle_volume(DEFAULT_PARTICLE_COUNT),
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
        self.particle_count = particle_count;
        self.particle_volume = default_particle_volume(particle_count);
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

    pub fn with_collider_bounds(mut self, min_ws: Vec3, max_ws: Vec3) -> Self {
        assert!(max_ws.cmpgt(min_ws).all());
        self.collider = WaterBoxCollider::new(min_ws, max_ws);
        self
    }
}

fn default_particle_volume(particle_count: usize) -> f32 {
    let volume_particle_count = if particle_count == 0 {
        DEFAULT_PARTICLE_VOLUME_REFERENCE_COUNT
    } else {
        particle_count
    };
    (INITIAL_PARTICLE_CHUNK_MAX_WS - INITIAL_PARTICLE_CHUNK_MIN_WS).element_product()
        * DEFAULT_INITIAL_PARTICLE_VOLUME_FRACTION
        / volume_particle_count as f32
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DebugWaterSpawnSkipReason {
    InvalidInput,
    OutsideCurrentBounds,
    TooCloseToBoundary { min_ws: Vec3, max_ws: Vec3 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DebugWaterSpawnResult {
    Spawned { count: usize, total_particles: usize },
    Skipped(DebugWaterSpawnSkipReason),
}

impl DebugWaterSpawnResult {
    pub fn spawned_count(self) -> usize {
        match self {
            Self::Spawned { count, .. } => count,
            Self::Skipped(_) => 0,
        }
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
    pub(crate) touched_grid_nodes: Vec<usize>,
    pub(crate) grid_boundary_flags: Vec<u8>,
    pub(crate) terrain_grid: Vec<WaterTerrainGridSample>,
    pub accumulator: f32,
    pub perf_stats: WaterPerfStats,
    pub perf_report_seconds: f32,
    pub diagnostic_stats: WaterPerfStats,
    pub diagnostic_report_seconds: f32,
    pub sim_time_seconds: f32,
    pub last_terrain_contact_particles: usize,
}

impl PondWaterSim {
    pub fn new(config: PondWaterConfig) -> Self {
        assert!(config.grid_dim.x >= 4 && config.grid_dim.y >= 4 && config.grid_dim.z >= 4);

        let origin_ws = config.collider.min_ws;
        let extent_ws = config.collider.extent();
        assert!(extent_ws.min_element() > 0.0);

        let dx = (extent_ws.x / config.grid_dim.x as f32)
            .max(extent_ws.y / config.grid_dim.y as f32)
            .max(extent_ws.z / config.grid_dim.z as f32);
        let inv_dx = dx.recip();

        let grid_len = (config.grid_dim.x as usize)
            .saturating_mul(config.grid_dim.y as usize)
            .saturating_mul(config.grid_dim.z as usize);
        let touched_grid_capacity = config.particle_count.saturating_mul(27).min(grid_len);
        let grid_boundary_flags = water_grid_boundary_flags(config.grid_dim, config.wall_padding_cells);
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
            touched_grid_nodes: Vec::with_capacity(touched_grid_capacity),
            grid_boundary_flags,
            terrain_grid: vec![WaterTerrainGridSample::default(); grid_len],
            accumulator: 0.0,
            perf_stats: WaterPerfStats::default(),
            perf_report_seconds: 0.0,
            diagnostic_stats: WaterPerfStats::default(),
            diagnostic_report_seconds: 0.0,
            sim_time_seconds: 0.0,
            last_terrain_contact_particles: 0,
        };
        sim.grid_dim = sim.config.grid_dim;
        sim.seed_particles();
        log::info!(
            "[WATER][INIT] seeded {} initial particles bounds {:?}..{:?} grid {:?} dx {:.5} particle_volume {:.6} rest_density {:.1} initial_area {:?}..{:?}",
            sim.particles.len(),
            sim.config.collider.min_ws,
            sim.config.collider.max_ws,
            sim.grid_dim,
            sim.dx,
            sim.config.particle_volume,
            sim.config.particle_mass / sim.config.particle_volume,
            INITIAL_PARTICLE_CHUNK_MIN_WS,
            INITIAL_PARTICLE_CHUNK_MAX_WS,
        );
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
        let chunk_id = chunk.chunk_id;
        self.upsert_terrain_collider_chunk_deferred(chunk);
        self.rebuild_terrain_grid_cache_for_chunk(chunk_id);
        if stabilize_particles {
            self.stabilize_after_terrain_chunk_change(chunk_id);
        }
    }

    pub fn upsert_terrain_collider_chunk_deferred(&mut self, chunk: WaterTerrainColliderChunk) {
        self.terrain
            .get_or_insert_with(WaterTerrainColliderSet::new)
            .insert_chunk(Arc::new(chunk));
    }

    pub fn finish_terrain_collider_chunk_batch(&mut self, stabilize_particles: bool) {
        self.rebuild_terrain_grid_cache();
        if stabilize_particles {
            self.stabilize_after_terrain_change();
        }
    }

    pub fn remove_terrain_collider_chunk(
        &mut self,
        chunk_id: glam::IVec3,
        stabilize_particles: bool,
    ) -> bool {
        let Some(terrain) = self.terrain.as_mut() else {
            return false;
        };
        if terrain.remove_chunk(chunk_id).is_none() {
            return false;
        }
        if terrain.is_empty() {
            self.terrain = None;
        }
        self.rebuild_terrain_grid_cache_for_chunk(chunk_id);
        if stabilize_particles {
            self.stabilize_after_terrain_chunk_change(chunk_id);
        }
        true
    }

    pub fn clear_terrain_collider_set(&mut self) {
        self.terrain = None;
        self.rebuild_terrain_grid_cache();
        self.stabilize_after_terrain_change();
    }

    fn stabilize_after_terrain_change(&mut self) {
        self.stabilize_after_terrain_change_in_scope(None);
    }

    fn stabilize_after_terrain_chunk_change(&mut self, chunk_id: IVec3) {
        let halo = self.terrain_chunk_stabilization_halo();
        let min_ws = chunk_id.as_vec3() - Vec3::splat(halo);
        let max_ws = chunk_id.as_vec3() + Vec3::ONE + Vec3::splat(halo);
        self.stabilize_after_terrain_change_in_scope(Some(TerrainStabilizationScope {
            chunk_id,
            min_ws,
            max_ws,
            halo,
        }));
    }

    fn terrain_chunk_stabilization_halo(&self) -> f32 {
        self.dx
            * (self.config.wall_padding_cells.max(1.0)
                + self.config.terrain_collision_margin_cells.max(0.0)
                + 1.0)
    }

    fn stabilize_after_terrain_change_in_scope(&mut self, scope: Option<TerrainStabilizationScope>) {
        self.accumulator = 0.0;
        self.diagnostic_stats.reset();
        self.diagnostic_report_seconds = 0.0;
        self.last_terrain_contact_particles = 0;

        let terrain = self.terrain.as_ref();
        let collision_margin = self.terrain_collision_margin();
        let max_correction = self.dx * self.config.wall_padding_cells.max(1.0);
        let particle_padding = self.dx * self.config.wall_padding_cells.max(1.0);
        let bounds = self.config.collider;
        let mut scoped_particles = 0usize;
        let mut sampled_particles = 0usize;
        let mut corrected_particles = 0usize;
        let mut clamped_corrections = 0usize;
        let mut total_particle_correction = 0.0f32;
        let mut max_particle_correction = 0.0f32;
        let mut min_sdf_before = f32::INFINITY;
        let mut min_sdf_after = f32::INFINITY;

        for particle in &mut self.particles {
            if let Some(scope) = scope {
                if !scope.contains(particle.x) {
                    continue;
                }
            }
            scoped_particles += 1;

            particle.v = Vec3::ZERO;
            particle.c = Mat3::ZERO;
            particle.j = 1.0;

            let mut particle_correction = 0.0f32;
            if let Some(terrain) = terrain {
                for iteration in 0..8 {
                    let Some((sdf, normal)) = terrain.sample_sdf_and_normal_ws(particle.x) else {
                        break;
                    };
                    if iteration == 0 {
                        sampled_particles += 1;
                        min_sdf_before = min_sdf_before.min(sdf);
                    }
                    let correction = collision_margin - sdf;
                    if correction <= 1.0e-5 {
                        break;
                    }

                    let before = particle.x;
                    particle.x += normal * correction.min(max_correction);
                    let unclamped = particle.x;
                    particle.x = bounds.clamp_point(particle.x, particle_padding);
                    if particle.x.distance_squared(unclamped) > 1.0e-10 {
                        clamped_corrections += 1;
                    }
                    particle_correction += particle.x.distance(before);
                }

                if let Some(sdf) = terrain.sample_sdf_ws(particle.x) {
                    min_sdf_after = min_sdf_after.min(sdf);
                }
            }

            if particle_correction > 0.0 {
                corrected_particles += 1;
                total_particle_correction += particle_correction;
                max_particle_correction = max_particle_correction.max(particle_correction);
            }
        }

        if terrain.is_some() {
            if let Some(scope) = scope {
                log::info!(
                    "[WATER][TERRAIN_STABILIZE] scope=chunk chunk={:?} halo={:.5} particles={} scoped={} sampled={} corrected={} clamped={} total_correction={:.4} avg_corrected={:.4} max_corrected={:.4} margin={:.5} max_step={:.5} min_sdf_before={:.5} min_sdf_after={:.5}",
                    scope.chunk_id,
                    scope.halo,
                    self.particles.len(),
                    scoped_particles,
                    sampled_particles,
                    corrected_particles,
                    clamped_corrections,
                    total_particle_correction,
                    if corrected_particles > 0 {
                        total_particle_correction / corrected_particles as f32
                    } else {
                        0.0
                    },
                    max_particle_correction,
                    collision_margin,
                    max_correction,
                    if min_sdf_before.is_finite() { min_sdf_before } else { f32::NAN },
                    if min_sdf_after.is_finite() { min_sdf_after } else { f32::NAN },
                );
            } else {
                log::info!(
                    "[WATER][TERRAIN_STABILIZE] particles={} sampled={} corrected={} clamped={} total_correction={:.4} avg_corrected={:.4} max_corrected={:.4} margin={:.5} max_step={:.5} min_sdf_before={:.5} min_sdf_after={:.5}",
                    self.particles.len(),
                    sampled_particles,
                    corrected_particles,
                    clamped_corrections,
                    total_particle_correction,
                    if corrected_particles > 0 {
                        total_particle_correction / corrected_particles as f32
                    } else {
                        0.0
                    },
                    max_particle_correction,
                    collision_margin,
                    max_correction,
                    if min_sdf_before.is_finite() { min_sdf_before } else { f32::NAN },
                    if min_sdf_after.is_finite() { min_sdf_after } else { f32::NAN },
                );
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
    ) -> DebugWaterSpawnResult {
        if count == 0
            || radius_ws <= 0.0
            || !radius_ws.is_finite()
            || !surface_point_ws.is_finite()
        {
            return DebugWaterSpawnResult::Skipped(DebugWaterSpawnSkipReason::InvalidInput);
        }

        let bounds = self.config.collider;
        if !bounds.contains(surface_point_ws) {
            return DebugWaterSpawnResult::Skipped(
                DebugWaterSpawnSkipReason::OutsideCurrentBounds,
            );
        }

        let terrain = self.terrain.as_ref();
        let collision_margin = self.terrain_collision_margin();
        let padding = (self.dx * self.config.wall_padding_cells.max(1.0))
            .min(self.extent_ws.min_element() * 0.25);
        let safe_min = bounds.min_ws + Vec3::splat(padding);
        let safe_max = bounds.max_ws - Vec3::splat(padding);
        if safe_min.cmpge(safe_max).any() {
            return DebugWaterSpawnResult::Skipped(DebugWaterSpawnSkipReason::InvalidInput);
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
            return DebugWaterSpawnResult::Skipped(DebugWaterSpawnSkipReason::InvalidInput);
        }

        let accepted_min = Vec3::new(
            safe_min.x + horizontal_radius,
            bounds.min_ws.y,
            safe_min.z + horizontal_radius,
        );
        let accepted_max = Vec3::new(
            safe_max.x - horizontal_radius,
            bounds.max_ws.y,
            safe_max.z - horizontal_radius,
        );
        if surface_point_ws.x < accepted_min.x
            || surface_point_ws.x > accepted_max.x
            || surface_point_ws.z < accepted_min.z
            || surface_point_ws.z > accepted_max.z
        {
            return DebugWaterSpawnResult::Skipped(DebugWaterSpawnSkipReason::TooCloseToBoundary {
                min_ws: accepted_min,
                max_ws: accepted_max,
            });
        }

        let spawn_x = surface_point_ws.x.clamp(accepted_min.x, accepted_max.x);
        let spawn_z = surface_point_ws.z.clamp(accepted_min.z, accepted_max.z);
        let local_surface_y = self.local_water_surface_y(spawn_x, spawn_z, horizontal_radius * 1.5);
        let base_y = surface_point_ws.y.max(local_surface_y.unwrap_or(surface_point_ws.y));
        let spawn_y = (base_y + self.dx * 1.5 + vertical_radius)
            .clamp(safe_min.y + vertical_radius, safe_max.y - vertical_radius);
        let spawn_center = Vec3::new(spawn_x, spawn_y, spawn_z);

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
            particle.j = self.debug_spawn_initial_j(particle.x.y, local_surface_y);
            self.particles.push(particle);
            spawned += 1;
        }

        if spawned > 0 {
            self.accumulator = 0.0;
        }

        DebugWaterSpawnResult::Spawned {
            count: spawned,
            total_particles: self.particles.len(),
        }
    }

    fn local_water_surface_y(&self, center_x: f32, center_z: f32, radius_ws: f32) -> Option<f32> {
        if radius_ws <= 0.0 || !radius_ws.is_finite() {
            return None;
        }

        let radius_sq = radius_ws * radius_ws;
        let mut max_y = f32::NEG_INFINITY;
        for particle in &self.particles {
            let dx = particle.x.x - center_x;
            let dz = particle.x.z - center_z;
            if dx * dx + dz * dz <= radius_sq {
                max_y = max_y.max(particle.x.y);
            }
        }

        max_y.is_finite().then_some(max_y)
    }

    fn debug_spawn_initial_j(&self, particle_y: f32, local_surface_y: Option<f32>) -> f32 {
        let Some(surface_y) = local_surface_y else {
            return 1.0;
        };
        if !particle_y.is_finite() || !surface_y.is_finite() {
            return 1.0;
        }

        // Debug-spawned droplets start at atmospheric pressure when they are on
        // or above the free surface. If clamping/collision correction places one
        // just under the local surface, use a bounded hydrostatic estimate
        // instead of inheriting a numerically compressed neighbor J.
        let max_depth = self.dx * DEBUG_SPAWN_HYDROSTATIC_J_MAX_DEPTH_CELLS;
        let depth_ws = (surface_y - particle_y).max(0.0).min(max_depth);
        if depth_ws <= 0.0 {
            return 1.0;
        }

        let rest_density = self.config.particle_mass / self.config.particle_volume;
        let gravity = self.config.gravity.length();
        let stiffness = self.config.stiffness;
        let gamma = self.config.gamma;
        if !rest_density.is_finite()
            || rest_density <= 0.0
            || !gravity.is_finite()
            || gravity <= 0.0
            || !stiffness.is_finite()
            || stiffness <= 0.0
            || !gamma.is_finite()
            || gamma <= 0.0
        {
            return 1.0;
        }

        let pressure = rest_density * gravity * depth_ws;
        let min_j = self.config.j_min.max(1.0e-4).min(1.0);
        (1.0 + pressure / stiffness)
            .powf(-gamma.recip())
            .clamp(min_j, 1.0)
    }

    fn seed_particles(&mut self) {
        self.particles.clear();
        self.particles.reserve(self.config.particle_count);
        if self.config.particle_count == 0 {
            return;
        }

        let padding = self.dx * self.config.wall_padding_cells.max(1.0);
        let bounds = self.config.collider;
        let safe_min = bounds.min_ws + Vec3::splat(padding);
        let safe_max = bounds.max_ws - Vec3::splat(padding);
        if safe_min.cmpge(safe_max).any() {
            return;
        }

        let plane_min = Vec3::new(
            INITIAL_PARTICLE_CHUNK_MIN_WS.x.max(safe_min.x),
            INITIAL_PARTICLE_CHUNK_MAX_WS.y.clamp(safe_min.y, safe_max.y),
            INITIAL_PARTICLE_CHUNK_MIN_WS.z.max(safe_min.z),
        );
        let plane_max = Vec3::new(
            INITIAL_PARTICLE_CHUNK_MAX_WS.x.min(safe_max.x),
            plane_min.y,
            INITIAL_PARTICLE_CHUNK_MAX_WS.z.min(safe_max.z),
        );
        if plane_min.x >= plane_max.x || plane_min.z >= plane_max.z {
            return;
        }

        let side = (self.config.particle_count as f32).sqrt().ceil() as u32;

        'rows: for z in 0..side {
            for x in 0..side {
                if self.particles.len() >= self.config.particle_count {
                    break 'rows;
                }

                let jitter_x = hash_unit(x, z, 17) - 0.5;
                let jitter_z = hash_unit(z, x, 29) - 0.5;
                let tx = (x as f32 + 0.5 + jitter_x * 0.8) / side as f32;
                let tz = (z as f32 + 0.5 + jitter_z * 0.8) / side as f32;
                let pos = Vec3::new(
                    plane_min.x + (plane_max.x - plane_min.x) * tx.clamp(0.0, 1.0),
                    plane_min.y,
                    plane_min.z + (plane_max.z - plane_min.z) * tz.clamp(0.0, 1.0),
                );
                self.particles.push(WaterParticle::new(
                    self.config.collider.clamp_point(pos, padding),
                ));
            }
        }

        debug_assert!(self.particles.iter().all(|particle| {
            particle.x.x >= INITIAL_PARTICLE_CHUNK_MIN_WS.x
                && particle.x.x <= INITIAL_PARTICLE_CHUNK_MAX_WS.x
                && particle.x.z >= INITIAL_PARTICLE_CHUNK_MIN_WS.z
                && particle.x.z <= INITIAL_PARTICLE_CHUNK_MAX_WS.z
        }));
    }
}

#[derive(Clone, Copy, Debug)]
struct TerrainStabilizationScope {
    chunk_id: IVec3,
    min_ws: Vec3,
    max_ws: Vec3,
    halo: f32,
}

impl TerrainStabilizationScope {
    fn contains(self, point_ws: Vec3) -> bool {
        !point_ws.is_finite()
            || (point_ws.cmpge(self.min_ws).all() && point_ws.cmple(self.max_ws).all())
    }
}

fn water_grid_boundary_flags(grid_dim: UVec3, wall_padding_cells: f32) -> Vec<u8> {
    let grid_len = (grid_dim.x as usize)
        .saturating_mul(grid_dim.y as usize)
        .saturating_mul(grid_dim.z as usize);
    let mut flags = vec![0u8; grid_len];
    let wall_cells = wall_padding_cells.max(1.0);

    for z in 0..grid_dim.z {
        for y in 0..grid_dim.y {
            for x in 0..grid_dim.x {
                let mut node_flags = 0u8;
                if x as f32 <= wall_cells {
                    node_flags |= WATER_GRID_BOUNDARY_X_MIN;
                }
                if (grid_dim.x - 1 - x) as f32 <= wall_cells {
                    node_flags |= WATER_GRID_BOUNDARY_X_MAX;
                }
                if y as f32 <= wall_cells {
                    node_flags |= WATER_GRID_BOUNDARY_Y_MIN;
                }
                if (grid_dim.y - 1 - y) as f32 <= wall_cells {
                    node_flags |= WATER_GRID_BOUNDARY_Y_MAX;
                }
                if z as f32 <= wall_cells {
                    node_flags |= WATER_GRID_BOUNDARY_Z_MIN;
                }
                if (grid_dim.z - 1 - z) as f32 <= wall_cells {
                    node_flags |= WATER_GRID_BOUNDARY_Z_MAX;
                }

                let idx = ((z as usize * grid_dim.y as usize + y as usize) * grid_dim.x as usize)
                    + x as usize;
                flags[idx] = node_flags;
            }
        }
    }

    flags
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
    fn fixed_test_box_starts_without_initial_particles() {
        let sim = PondWaterSim::fixed_test_box();
        assert_eq!(sim.config.collider.min_ws, Vec3::ZERO);
        assert_eq!(sim.config.collider.max_ws, Vec3::new(2.0, 1.0, 2.0));
        assert_eq!(sim.config.particle_count, DEFAULT_PARTICLE_COUNT);
        assert_eq!(sim.particles.len(), 0);
        assert_eq!(sim.grid_dim, DEFAULT_GRID_DIM);
        assert!((sim.dx - 1.0 / 32.0).abs() < 1.0e-6);
        assert_eq!(
            sim.config.particle_volume,
            default_particle_volume(DEFAULT_PARTICLE_VOLUME_REFERENCE_COUNT)
        );
    }

    #[test]
    fn explicit_particle_count_seeds_particles_on_original_chunk_surface() {
        let sim = PondWaterSim::new(PondWaterConfig::default().with_particle_count(128));
        assert_eq!(sim.particles.len(), 128);
        let seed_y = sim.config.collider.max_ws.y - sim.dx * sim.config.wall_padding_cells.max(1.0);
        for particle in &sim.particles {
            assert!(
                sim.config.collider.contains(particle.x),
                "particle escaped: {:?}",
                particle.x
            );
            assert!(
                particle.x.x >= INITIAL_PARTICLE_CHUNK_MIN_WS.x
                    && particle.x.x <= INITIAL_PARTICLE_CHUNK_MAX_WS.x,
                "particle seeded outside original x chunk: {:?}",
                particle.x,
            );
            assert!(
                particle.x.z >= INITIAL_PARTICLE_CHUNK_MIN_WS.z
                    && particle.x.z <= INITIAL_PARTICLE_CHUNK_MAX_WS.z,
                "particle seeded outside original z chunk: {:?}",
                particle.x,
            );
            assert!(
                (particle.x.y - seed_y).abs() < 1.0e-6,
                "particle not on initial surface plane: {:?}, expected y {seed_y}",
                particle.x,
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
        let mut sim = PondWaterSim::new(PondWaterConfig::default().with_particle_count(1));
        sim.particles[0].v = Vec3::new(1.0, 2.0, 3.0);
        sim.particles[0].j = 1.25;

        sim.upsert_terrain_collider_chunk(test_chunk(glam::IVec3::new(2, 0, 1)), false);

        assert_eq!(sim.particles[0].v, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(sim.particles[0].j, 1.25);
    }

    #[test]
    fn removing_last_terrain_chunk_clears_collider_set() {
        let mut sim = PondWaterSim::fixed_test_box();
        let chunk_id = glam::IVec3::new(1, 0, 1);
        sim.upsert_terrain_collider_chunk(test_chunk(chunk_id), false);

        assert!(sim.remove_terrain_collider_chunk(chunk_id, false));

        assert!(sim.terrain_collider_set().is_none());
        assert!(!sim.remove_terrain_collider_chunk(chunk_id, false));
    }

    #[test]
    fn terrain_chunk_upsert_and_remove_update_cached_grid_region() {
        let mut sim = PondWaterSim::fixed_test_box();
        let chunk_id = glam::IVec3::new(1, 0, 1);
        let inside_idx = terrain_grid_idx(sim.grid_dim, glam::UVec3::new(40, 8, 40));
        let outside_idx = terrain_grid_idx(sim.grid_dim, glam::UVec3::new(8, 8, 8));

        assert!(!sim.terrain_grid[inside_idx].has_sdf);
        assert!(!sim.terrain_grid[outside_idx].has_sdf);

        sim.upsert_terrain_collider_chunk(test_chunk(chunk_id), false);

        assert!(sim.terrain_grid[inside_idx].has_sdf);
        assert!(!sim.terrain_grid[outside_idx].has_sdf);

        assert!(sim.remove_terrain_collider_chunk(chunk_id, false));

        assert!(!sim.terrain_grid[inside_idx].has_sdf);
        assert!(!sim.terrain_grid[outside_idx].has_sdf);
    }

    #[test]
    fn chunk_terrain_stabilization_preserves_far_particles() {
        let mut sim = PondWaterSim::fixed_test_box();
        sim.particles = vec![
            WaterParticle {
                x: Vec3::new(1.5, 0.5, 1.5),
                v: Vec3::new(1.0, 2.0, 3.0),
                c: Mat3::from_diagonal(Vec3::splat(2.0)),
                j: 1.25,
            },
            WaterParticle {
                x: Vec3::new(0.25, 0.5, 0.25),
                v: Vec3::new(4.0, 5.0, 6.0),
                c: Mat3::from_diagonal(Vec3::splat(3.0)),
                j: 1.5,
            },
        ];

        sim.upsert_terrain_collider_chunk(test_chunk(glam::IVec3::new(1, 0, 1)), true);

        assert_eq!(sim.particles[0].v, Vec3::ZERO);
        assert_eq!(sim.particles[0].c, Mat3::ZERO);
        assert_eq!(sim.particles[0].j, 1.0);
        assert_eq!(sim.particles[1].v, Vec3::new(4.0, 5.0, 6.0));
        assert_eq!(sim.particles[1].c, Mat3::from_diagonal(Vec3::splat(3.0)));
        assert_eq!(sim.particles[1].j, 1.5);
    }

    #[test]
    fn debug_spawn_adds_particles_inside_current_bounds() {
        let mut sim = PondWaterSim::fixed_test_box();
        let initial_len = sim.particles.len();

        let result = sim.spawn_debug_particles_at_surface(Vec3::new(1.5, 0.5, 1.5), 12, 0.05);

        assert_eq!(result.spawned_count(), 12);
        assert_eq!(sim.particles.len(), initial_len + 12);
        for particle in sim.particles.iter().skip(initial_len) {
            assert!(sim.config.collider.contains(particle.x));
        }
    }

    #[test]
    fn debug_spawn_rejects_points_outside_current_bounds() {
        let mut sim = PondWaterSim::fixed_test_box();
        let initial_len = sim.particles.len();

        let result = sim.spawn_debug_particles_at_surface(Vec3::new(-0.5, 0.5, 1.5), 12, 0.05);

        assert_eq!(
            result,
            DebugWaterSpawnResult::Skipped(DebugWaterSpawnSkipReason::OutsideCurrentBounds)
        );
        assert_eq!(sim.particles.len(), initial_len);
    }

    #[test]
    fn debug_spawn_rejects_points_too_close_to_boundary() {
        let mut sim = PondWaterSim::fixed_test_box();
        let initial_len = sim.particles.len();

        let result = sim.spawn_debug_particles_at_surface(Vec3::new(0.05, 0.5, 1.5), 12, 0.12);

        assert!(matches!(
            result,
            DebugWaterSpawnResult::Skipped(DebugWaterSpawnSkipReason::TooCloseToBoundary { .. })
        ));
        assert_eq!(sim.particles.len(), initial_len);
    }

    #[test]
    fn debug_spawn_allows_growth_past_initial_particle_count() {
        let mut sim = PondWaterSim::new(PondWaterConfig::default().with_particle_count(4));
        let old_cap = sim.config.particle_count * 2;
        let filler = sim.particles[0];
        sim.particles.resize(old_cap, filler);

        let result = sim.spawn_debug_particles_at_surface(Vec3::new(1.5, 0.5, 1.5), 12, 0.05);

        assert_eq!(result.spawned_count(), 12);
        assert_eq!(sim.particles.len(), old_cap + 12);
    }

    #[test]
    fn debug_spawn_does_not_inherit_compressed_local_j_at_free_surface() {
        let mut sim = PondWaterSim::new(PondWaterConfig::default().with_particle_count(16));
        let initial_len = sim.particles.len();
        for particle in &mut sim.particles {
            particle.x.y = 0.3;
            particle.j = 0.25;
        }

        let result = sim.spawn_debug_particles_at_surface(Vec3::new(1.5, 0.6, 1.5), 12, 0.05);

        assert_eq!(result.spawned_count(), 12);
        assert!(sim.particles[initial_len..]
            .iter()
            .all(|particle| particle.j == 1.0));
    }

    #[test]
    fn debug_spawn_initial_j_uses_bounded_hydrostatic_depth_below_surface() {
        let sim = PondWaterSim::fixed_test_box();
        let surface_y = 0.5;

        assert_eq!(sim.debug_spawn_initial_j(surface_y + sim.dx, Some(surface_y)), 1.0);

        let below_surface_j = sim.debug_spawn_initial_j(surface_y - sim.dx, Some(surface_y));
        assert!(
            below_surface_j < 1.0 && below_surface_j >= sim.config.j_min,
            "below_surface_j={below_surface_j}"
        );
    }

    #[test]
    fn debug_spawn_uses_local_water_surface_height() {
        let mut sim = PondWaterSim::new(PondWaterConfig::default().with_particle_count(16));
        let initial_len = sim.particles.len();
        let spawn_x = 1.5;
        let spawn_z = 1.5;
        for particle in &mut sim.particles {
            particle.x.y = 0.3;
        }
        let before_surface_y = max_particle_y_near(&sim, spawn_x, spawn_z, 0.18);

        let result =
            sim.spawn_debug_particles_at_surface(Vec3::new(spawn_x, 0.1, spawn_z), 16, 0.08);

        assert_eq!(result.spawned_count(), 16);
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

    fn terrain_grid_idx(grid_dim: glam::UVec3, node: glam::UVec3) -> usize {
        ((node.z as usize * grid_dim.y as usize + node.y as usize) * grid_dim.x as usize)
            + node.x as usize
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
