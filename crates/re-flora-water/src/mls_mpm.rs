use glam::{IVec3, Mat3, UVec3, Vec3};
use std::time::Instant;

use super::{
    collider::{WaterBoxCollider, WaterTerrainColliderSet},
    pond::{
        PondWaterConfig, PondWaterSim, WaterGridNode, WaterTerrainGridSample, WATER_GRID_BOUNDARY_X_MAX,
        WATER_GRID_BOUNDARY_X_MIN, WATER_GRID_BOUNDARY_Y_MAX, WATER_GRID_BOUNDARY_Y_MIN,
        WATER_GRID_BOUNDARY_Z_MAX, WATER_GRID_BOUNDARY_Z_MIN,
    },
};

const MAX_SUBSTEPS_PER_UPDATE: usize = 8;
const ACTIVE_MASS_EPSILON: f32 = 1.0e-8;
const MIN_FLUID_DENSITY: f32 = 1.0e-8;
const MAX_J: f32 = 8.0;
const NO_TENSION_MAX_J: f32 = 1.0;
#[cfg(test)]
const MAX_J_LOG_STEP_PER_SUBSTEP: f32 = 0.10;
// Blend a small MLS grid-density estimate into the deformation-history J each
// substep. Pure velocity-gradient J can relax to rest volume after wall/terrain
// collision projection even when particles are visibly overpacked; this feedback
// re-anchors pressure to the configured marker volume without a neighbor solve.
#[cfg(test)]
const DENSITY_J_FEEDBACK_PER_SECOND: f32 = 12.64;
// Ignore tiny density-estimate compression. The MLS kernel/marker discretization
// constantly produces sub-percent local density noise; feeding that straight into
// pressure keeps quiet puddles breathing forever.
const DENSITY_J_FEEDBACK_DEADBAND: f32 = 0.02;
// APIC's affine velocity mode preserves sub-cell motion very well, which is good
// for lively splashes but leaves collision/pressure ringing in settled water.
// Mildly damp only the affine part; particle velocity keeps the configured water
// linear damping path.
const APIC_AFFINE_DAMPING_PER_SECOND: f32 = 1.5;
const MAX_PARTICLE_SPEED: f32 = 20.0;
const MAX_PARTICLE_CFL_CELLS_PER_SUBSTEP: f32 = 0.5;
const MAX_AFFINE_COMPONENT: f32 = 100.0;
const TERRAIN_GRID_SKIP_GUARD_CELLS: f32 = 0.25;
const TERRAIN_GRID_PROJECTION_GUARD_CELLS: f32 = 0.10;
// Fill missing density-kernel support when terrain SDF occupies part of a
// particle's pressure stencil. This is an mDBC/Ghost-SPH style density sample:
// solid-side stencil weight contributes virtual density extrapolated from the
// mirrored fluid side, with a hydrostatic pressure offset. It is used only for
// EOS pressure/stress; it does not add real grid mass or alter velocity
// normalization.
const TERRAIN_DENSITY_MIN_FLUID_FRACTION: f32 = 0.50;
const TERRAIN_DENSITY_MAX_CORRECTION_FACTOR: f32 = 2.0;
const TERRAIN_DENSITY_OCCUPANCY_TRANSITION_CELLS: f32 = 1.0;
const TERRAIN_DENSITY_MIN_SOLID_WEIGHT: f32 = 1.0e-5;
const TERRAIN_GHOST_MIRROR_MIN_DISTANCE_CELLS: f32 = 0.25;

#[derive(Debug)]
pub struct WaterTerrainCacheBuildRequest {
    chunk_id: IVec3,
    terrain_chunk_count: usize,
    terrain: Option<WaterTerrainColliderSet>,
    origin_ws: Vec3,
    grid_dim: UVec3,
    dx: f32,
    min_node: UVec3,
    max_node_exclusive: UVec3,
    near_surface_band: f32,
}

impl WaterTerrainCacheBuildRequest {
    pub fn for_config_and_terrain(
        config: &PondWaterConfig,
        terrain: Option<WaterTerrainColliderSet>,
        chunk_id: IVec3,
    ) -> Option<Self> {
        let origin_ws = config.collider.min_ws;
        let extent_ws = config.collider.extent();
        if !extent_ws.is_finite() || extent_ws.min_element() <= 0.0 {
            return None;
        }

        let dx = (extent_ws.x / config.grid_dim.x as f32)
            .max(extent_ws.y / config.grid_dim.y as f32)
            .max(extent_ws.z / config.grid_dim.z as f32);
        if dx <= 0.0 || !dx.is_finite() {
            return None;
        }

        let inv_dx = dx.recip();
        let (min_node, max_node_exclusive) =
            terrain_grid_cache_range_for_chunk_parts(origin_ws, inv_dx, config.grid_dim, chunk_id)?;
        let terrain_chunk_count = terrain.as_ref().map_or(0, |terrain| terrain.chunks.len());
        let near_surface_band = dx * (config.terrain_collision_margin_cells.max(0.0) + 2.0);
        Some(Self {
            chunk_id,
            terrain_chunk_count,
            terrain,
            origin_ws,
            grid_dim: config.grid_dim,
            dx,
            min_node,
            max_node_exclusive,
            near_surface_band,
        })
    }

    pub fn chunk_id(&self) -> IVec3 {
        self.chunk_id
    }

    pub fn terrain_chunk_count(&self) -> usize {
        self.terrain_chunk_count
    }

    pub fn grid_dim(&self) -> UVec3 {
        self.grid_dim
    }

    pub fn min_node(&self) -> UVec3 {
        self.min_node
    }

    pub fn max_node_exclusive(&self) -> UVec3 {
        self.max_node_exclusive
    }

    pub fn near_surface_band(&self) -> f32 {
        self.near_surface_band
    }

    pub fn dx(&self) -> f32 {
        self.dx
    }

    pub fn node_count(&self) -> usize {
        terrain_cache_range_node_count(self.min_node, self.max_node_exclusive)
    }
}

#[derive(Debug)]
pub struct WaterTerrainCachePatch {
    chunk_id: IVec3,
    terrain_chunk_count: usize,
    grid_dim: UVec3,
    dx: f32,
    min_node: UVec3,
    max_node_exclusive: UVec3,
    near_surface_band: f32,
    samples: Vec<WaterTerrainGridSample>,
    stats: WaterTerrainCacheRebuildStats,
    build_ms: f32,
}

impl WaterTerrainCachePatch {
    pub fn chunk_id(&self) -> IVec3 {
        self.chunk_id
    }

    pub fn terrain_chunk_count(&self) -> usize {
        self.terrain_chunk_count
    }

    pub fn grid_dim(&self) -> UVec3 {
        self.grid_dim
    }

    pub fn min_node(&self) -> UVec3 {
        self.min_node
    }

    pub fn max_node_exclusive(&self) -> UVec3 {
        self.max_node_exclusive
    }

    pub fn near_surface_band(&self) -> f32 {
        self.near_surface_band
    }

    pub fn dx(&self) -> f32 {
        self.dx
    }

    pub fn stats(&self) -> WaterTerrainCacheRebuildStats {
        self.stats
    }

    pub fn build_ms(&self) -> f32 {
        self.build_ms
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WaterTerrainCacheRebuildStats {
    pub node_count: usize,
    pub has_sdf_count: usize,
    pub near_surface_count: usize,
    pub normal_count: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WaterTerrainCacheApplyReport {
    pub chunk_id: IVec3,
    pub terrain_chunk_count: usize,
    pub grid_dim: UVec3,
    pub min_node: UVec3,
    pub max_node_exclusive: UVec3,
    pub node_count: usize,
    pub has_sdf_count: usize,
    pub near_surface_count: usize,
    pub normal_count: usize,
    pub near_surface_band: f32,
    pub dx: f32,
    pub build_ms: f32,
    pub apply_ms: f32,
}

pub fn build_terrain_grid_cache_patch(
    request: WaterTerrainCacheBuildRequest,
) -> WaterTerrainCachePatch {
    let build_start = Instant::now();
    let mut samples = Vec::with_capacity(request.node_count());
    let stats = build_terrain_grid_cache_samples(
        request.terrain.as_ref(),
        request.origin_ws,
        request.dx,
        request.min_node,
        request.max_node_exclusive,
        request.near_surface_band,
        |sample| samples.push(sample),
    );

    WaterTerrainCachePatch {
        chunk_id: request.chunk_id,
        terrain_chunk_count: request.terrain_chunk_count,
        grid_dim: request.grid_dim,
        dx: request.dx,
        min_node: request.min_node,
        max_node_exclusive: request.max_node_exclusive,
        near_surface_band: request.near_surface_band,
        samples,
        stats,
        build_ms: build_start.elapsed().as_secs_f32() * 1000.0,
    }
}

fn build_terrain_grid_cache_samples(
    terrain: Option<&WaterTerrainColliderSet>,
    origin_ws: Vec3,
    dx: f32,
    min_node: UVec3,
    max_node_exclusive: UVec3,
    near_surface_band: f32,
    mut push_sample: impl FnMut(WaterTerrainGridSample),
) -> WaterTerrainCacheRebuildStats {
    let mut stats = WaterTerrainCacheRebuildStats::default();

    for z in min_node.z..max_node_exclusive.z {
        for y in min_node.y..max_node_exclusive.y {
            for x in min_node.x..max_node_exclusive.x {
                let mut sample = WaterTerrainGridSample::default();
                if let Some(terrain) = terrain {
                    let node_world = origin_ws + Vec3::new(x as f32, y as f32, z as f32) * dx;
                    if let Some(sdf) = terrain.sample_sdf_ws(node_world) {
                        sample.sdf = sdf;
                        sample.has_sdf = true;
                        stats.has_sdf_count += 1;
                        sample.near_surface = sdf <= near_surface_band;
                        if sample.near_surface {
                            stats.near_surface_count += 1;
                            sample.normal = terrain.sample_normal_ws(node_world).unwrap_or(Vec3::Y);
                            stats.normal_count += 1;
                        }
                    }
                }
                push_sample(sample);
                stats.node_count += 1;
            }
        }
    }

    stats
}

fn terrain_cache_range_node_count(min_node: UVec3, max_node_exclusive: UVec3) -> usize {
    let extent = max_node_exclusive.saturating_sub(min_node);
    (extent.x as usize)
        .saturating_mul(extent.y as usize)
        .saturating_mul(extent.z as usize)
}

// A particle can end a substep deeper than one capped correction can resolve.
// Iterate bounded SDF corrections so the next P2G pass does not deposit mass
// from inside terrain.
const TERRAIN_PARTICLE_COLLISION_ITERATIONS: usize = 8;

impl PondWaterSim {
    /// Advance the pond by fixed MLS-MPM substeps.
    pub fn update(&mut self, dt: f32, perf_logging: bool) {
        self.update_with_max_substeps(dt, perf_logging, MAX_SUBSTEPS_PER_UPDATE);
    }

    pub fn update_with_max_substeps(
        &mut self,
        dt: f32,
        perf_logging: bool,
        max_substeps: usize,
    ) {
        if dt <= 0.0 || !dt.is_finite() {
            return;
        }

        if self.particles.is_empty() {
            self.clear_grid();
            self.accumulator = 0.0;
            self.perf_report_seconds = 0.0;
            self.perf_stats.reset();
            self.diagnostic_report_seconds = 0.0;
            self.diagnostic_stats.reset();
            self.last_terrain_contact_particles = 0;
            return;
        }

        let max_substeps = max_substeps.min(MAX_SUBSTEPS_PER_UPDATE);
        if max_substeps == 0 {
            self.accumulator = 0.0;
            return;
        }

        self.accumulator += dt.min(0.25);
        let substep_dt = self.config.substep_dt;
        let mut ran_substeps = 0usize;
        for _ in 0..max_substeps {
            if self.accumulator < substep_dt {
                break;
            }
            self.substep_timed(substep_dt, perf_logging);
            self.accumulator -= substep_dt;
            ran_substeps += 1;
        }

        // Avoid a long catch-up spiral if a frame stalls while the sim is enabled.
        let max_remainder = substep_dt * max_substeps as f32;
        self.accumulator = self.accumulator.min(max_remainder);

        if ran_substeps > 0 {
            self.sim_time_seconds += substep_dt * ran_substeps as f32;
            self.log_diagnostics_after_update(dt, ran_substeps);
        }

        if perf_logging {
            self.perf_report_seconds += dt;
            if self.perf_report_seconds >= 1.0 {
                self.log_perf_report();
            }
        } else {
            self.perf_report_seconds = 0.0;
            self.perf_stats.reset();
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn substep(&mut self, dt: f32) {
        self.substep_timed(dt, false);
    }

    fn substep_timed(&mut self, dt: f32, perf_logging: bool) {
        if dt <= 0.0 || !dt.is_finite() {
            return;
        }

        if !perf_logging {
            self.clear_grid();
            self.particle_to_grid(dt);
            let active_nodes = self.update_grid(dt);
            let g2p_breakdown = self.grid_to_particle(dt);
            self.record_diagnostic_substep(active_nodes, g2p_breakdown);
            return;
        }

        let total_start = Instant::now();
        let repair_seconds = 0.0;

        let clear_start = Instant::now();
        self.clear_grid();
        let clear_seconds = clear_start.elapsed().as_secs_f64();

        let p2g_start = Instant::now();
        self.particle_to_grid(dt);
        let p2g_seconds = p2g_start.elapsed().as_secs_f64();

        let grid_update_start = Instant::now();
        let active_nodes = self.update_grid(dt);
        let grid_update_seconds = grid_update_start.elapsed().as_secs_f64();

        let grid_seconds = grid_update_seconds;

        let g2p_breakdown = self.grid_to_particle_timed(dt);

        let diagnostics_start = Instant::now();
        self.record_diagnostic_substep(active_nodes, g2p_breakdown);
        let diagnostics_seconds = diagnostics_start.elapsed().as_secs_f64();

        self.perf_stats.substeps += 1;
        self.perf_stats.repair_seconds += repair_seconds;
        self.perf_stats.clear_seconds += clear_seconds;
        self.perf_stats.p2g_seconds += p2g_seconds;
        self.perf_stats.grid_seconds += grid_seconds;
        self.perf_stats.grid_update_seconds += grid_update_seconds;
        self.perf_stats.g2p_seconds += g2p_breakdown.total_seconds;
        self.perf_stats.diagnostics_seconds += diagnostics_seconds;
        self.perf_stats.g2p_gather_seconds += g2p_breakdown.gather_seconds;
        self.perf_stats.g2p_box_seconds += g2p_breakdown.box_seconds;
        self.perf_stats.g2p_terrain_seconds += g2p_breakdown.terrain_seconds;
        self.perf_stats.g2p_repair_seconds += g2p_breakdown.repair_seconds;
        self.perf_stats.total_seconds += total_start.elapsed().as_secs_f64();
        self.perf_stats.active_node_visits += active_nodes as u64;
        self.perf_stats.g2p_terrain_cache_skips += g2p_breakdown.terrain_cache_skips;
        self.perf_stats.g2p_terrain_cache_projections += g2p_breakdown.terrain_cache_projections;
        self.perf_stats.g2p_terrain_exact_fallbacks += g2p_breakdown.terrain_exact_fallbacks;
        self.perf_stats.g2p_terrain_exact_checks += g2p_breakdown.terrain_exact_checks;
        self.perf_stats.g2p_terrain_exact_corrections += g2p_breakdown.terrain_exact_corrections;
    }

    fn record_diagnostic_substep(
        &mut self,
        active_nodes: usize,
        g2p_breakdown: WaterG2pBreakdown,
    ) {
        self.diagnostic_stats.substeps += 1;
        self.diagnostic_stats.active_node_visits += active_nodes as u64;
        self.diagnostic_stats.g2p_terrain_cache_skips += g2p_breakdown.terrain_cache_skips;
        self.diagnostic_stats.g2p_terrain_cache_projections +=
            g2p_breakdown.terrain_cache_projections;
        self.diagnostic_stats.g2p_terrain_exact_fallbacks +=
            g2p_breakdown.terrain_exact_fallbacks;
        self.diagnostic_stats.g2p_terrain_exact_checks += g2p_breakdown.terrain_exact_checks;
        self.diagnostic_stats.g2p_terrain_exact_corrections +=
            g2p_breakdown.terrain_exact_corrections;
    }

    fn log_diagnostics_after_update(&mut self, frame_dt: f32, ran_substeps: usize) {
        const REPORT_INTERVAL_SECONDS: f32 = 0.25;
        const ANOMALY_REPORT_INTERVAL_SECONDS: f32 = 0.10;
        const SPEED_LIMIT_WARN_FRACTION: f32 = 0.98;
        const TERRAIN_PENETRATION_WARN_CELLS: f32 = 1.0;
        const BOUNDARY_PIN_WARN_FRACTION: f32 = 0.25;

        self.diagnostic_report_seconds += frame_dt;

        let padding = self.dx * self.config.wall_padding_cells.max(1.0);
        let speed_limit = max_particle_speed_for_substep(self.dx, self.config.substep_dt);
        let terrain_collision_margin = self.terrain_collision_margin();
        let eos_j_min = Some(self.config.j_min);
        let interval_due = self.diagnostic_report_seconds >= REPORT_INTERVAL_SECONDS;
        let terrain_activity_since_report = self.diagnostic_stats.g2p_terrain_cache_projections > 0
            || self.diagnostic_stats.g2p_terrain_exact_corrections > 0;
        let early_terrain_contact =
            terrain_activity_since_report && self.last_terrain_contact_particles == 0;

        if !interval_due && !early_terrain_contact {
            let cheap_particle_stats = water_particle_debug_stats(
                &self.particles,
                None,
                self.config.collider,
                padding,
                speed_limit,
                eos_j_min,
                terrain_collision_margin,
            );
            let speed_saturated = cheap_particle_stats.max_speed
                >= speed_limit * SPEED_LIMIT_WARN_FRACTION
                || cheap_particle_stats.speed_limited_particles > self.particles.len() / 8;
            let boundary_pinned = cheap_particle_stats.floor_pinned_particles
                + cheap_particle_stats.wall_pinned_particles
                > (self.particles.len() as f32 * BOUNDARY_PIN_WARN_FRACTION) as usize;
            let cheap_anomalous = cheap_particle_stats.non_finite_particles > 0
                || cheap_particle_stats.out_of_bounds_particles > 0
                || speed_saturated
                || boundary_pinned;
            if !(cheap_anomalous
                && self.diagnostic_report_seconds >= ANOMALY_REPORT_INTERVAL_SECONDS)
            {
                return;
            }
        }

        let particle_stats = water_particle_debug_stats(
            &self.particles,
            self.terrain.as_ref(),
            self.config.collider,
            padding,
            speed_limit,
            eos_j_min,
            terrain_collision_margin,
        );

        let newly_contacting = (particle_stats.terrain_contact_particles > 0 || early_terrain_contact)
            && self.last_terrain_contact_particles == 0;
        let speed_saturated = particle_stats.max_speed >= speed_limit * SPEED_LIMIT_WARN_FRACTION
            || particle_stats.speed_limited_particles > self.particles.len() / 8;
        let deep_terrain_penetration = particle_stats.max_terrain_penetration
            > self.dx * TERRAIN_PENETRATION_WARN_CELLS;
        let boundary_pinned = particle_stats.floor_pinned_particles
            + particle_stats.wall_pinned_particles
            > (self.particles.len() as f32 * BOUNDARY_PIN_WARN_FRACTION) as usize;
        let anomalous = particle_stats.non_finite_particles > 0
            || particle_stats.out_of_bounds_particles > 0
            || speed_saturated
            || deep_terrain_penetration
            || boundary_pinned;

        let should_report = interval_due
            || newly_contacting
            || (anomalous && self.diagnostic_report_seconds >= ANOMALY_REPORT_INTERVAL_SECONDS);
        if !should_report {
            self.last_terrain_contact_particles = particle_stats.terrain_contact_particles;
            return;
        }

        let diag = self.diagnostic_stats;
        let diag_substeps = diag.substeps.max(1) as f64;
        let terrain_projections_per_substep = diag.g2p_terrain_cache_projections as f64 / diag_substeps;
        let terrain_exact_corrections_per_substep =
            diag.g2p_terrain_exact_corrections as f64 / diag_substeps;
        let terrain_exact_checks_per_substep = diag.g2p_terrain_exact_checks as f64 / diag_substeps;
        let active_nodes_per_substep = diag.active_node_visits as f64 / diag_substeps;
        let density_corrections_per_substep =
            diag.p2g_density_correction_particles as f64 / diag_substeps;
        let density_correction_factor_avg = if diag.p2g_density_correction_particles > 0 {
            diag.p2g_density_correction_factor_sum
                / diag.p2g_density_correction_particles as f64
        } else {
            1.0
        };

        let message = format!(
            "[WATER][DIAG] t={:.3}s frame_dt={:.4}s ran_substeps={} diag_substeps={} particles={} finite={} pos_x={:.3}..{:.3} pos_y={:.3}..{:.3} avg_y={:.3} pos_z={:.3}..{:.3} speed_avg={:.3} speed_max={:.3}/{:.3} speed_limited={} j={:.3}..{:.3} j_min_clamped={} j_max_clamped={} affine_max={:.2} terrain_contact={} terrain_penetrating={} terrain_no_sdf={} terrain_sdf_min={:.5} terrain_penetration_max={:.5} p2g_density_corr/substep={:.1} p2g_density_corr_factor_avg={:.3} p2g_density_corr_factor_max={:.3} g2p_cache_proj/substep={:.1} g2p_exact_checks/substep={:.1} g2p_exact_corr/substep={:.1} active_nodes/substep={:.0} floor_pinned={} ceil_pinned={} wall_pinned={} out_of_bounds={} non_finite={} fastest_idx={} fastest_pos={:?} fastest_v={:?} fastest_j={:.3} fastest_sdf={:.5}",
            self.sim_time_seconds,
            frame_dt,
            ran_substeps,
            diag.substeps,
            self.particles.len(),
            particle_stats.finite_particles,
            particle_stats.min_ws.x,
            particle_stats.max_ws.x,
            particle_stats.min_ws.y,
            particle_stats.max_ws.y,
            particle_stats.avg_ws.y,
            particle_stats.min_ws.z,
            particle_stats.max_ws.z,
            particle_stats.avg_speed,
            particle_stats.max_speed,
            speed_limit,
            particle_stats.speed_limited_particles,
            particle_stats.min_j,
            particle_stats.max_j,
            particle_stats.j_min_clamped_particles,
            particle_stats.j_max_clamped_particles,
            particle_stats.max_abs_affine,
            particle_stats.terrain_contact_particles,
            particle_stats.terrain_penetrating,
            particle_stats.no_terrain_sdf,
            particle_stats.min_terrain_sdf.unwrap_or(f32::NAN),
            particle_stats.max_terrain_penetration,
            density_corrections_per_substep,
            density_correction_factor_avg,
            diag.p2g_density_correction_factor_max,
            terrain_projections_per_substep,
            terrain_exact_checks_per_substep,
            terrain_exact_corrections_per_substep,
            active_nodes_per_substep,
            particle_stats.floor_pinned_particles,
            particle_stats.ceiling_pinned_particles,
            particle_stats.wall_pinned_particles,
            particle_stats.out_of_bounds_particles,
            particle_stats.non_finite_particles,
            particle_stats.max_speed_index,
            particle_stats.max_speed_position,
            particle_stats.max_speed_velocity,
            particle_stats.max_speed_j,
            particle_stats.max_speed_terrain_sdf.unwrap_or(f32::NAN),
        );
        if anomalous {
            log::warn!("{message}");
        } else {
            log::info!("{message}");
        }

        self.last_terrain_contact_particles = particle_stats.terrain_contact_particles;
        self.diagnostic_report_seconds = 0.0;
        self.diagnostic_stats.reset();
    }

    fn log_perf_report(&mut self) {
        let mut stats = self.perf_stats;
        if stats.substeps == 0 {
            return;
        }

        let shadow_measure_start = Instant::now();
        let shadow_stats = self.measure_terrain_shadow_samples();
        let shadow_measure_seconds = shadow_measure_start.elapsed().as_secs_f64();
        stats.g2p_terrain_shadow_samples += shadow_stats.samples;
        stats.g2p_terrain_shadow_false_skips += shadow_stats.false_skips;
        stats.g2p_terrain_shadow_sdf_abs_error_sum += shadow_stats.sdf_abs_error_sum;
        stats.g2p_terrain_shadow_sdf_abs_error_max = stats
            .g2p_terrain_shadow_sdf_abs_error_max
            .max(shadow_stats.sdf_abs_error_max);

        let substeps = stats.substeps as f64;
        let recorded_seconds = stats.repair_seconds
            + stats.clear_seconds
            + stats.p2g_seconds
            + stats.grid_seconds
            + stats.g2p_seconds
            + stats.diagnostics_seconds;
        let residual_seconds = (stats.total_seconds - recorded_seconds).max(0.0);
        let grid_nodes = self.grid.len();
        let particle_padding = self.dx * self.config.wall_padding_cells.max(1.0);
        let max_particle_speed = max_particle_speed_for_substep(self.dx, self.config.substep_dt);
        let particle_stats = water_particle_debug_stats(
            &self.particles,
            self.terrain.as_ref(),
            self.config.collider,
            particle_padding,
            max_particle_speed,
            Some(self.config.j_min),
            self.terrain_collision_margin(),
        );
        let density_correction_factor_avg = if stats.p2g_density_correction_particles > 0 {
            stats.p2g_density_correction_factor_sum
                / stats.p2g_density_correction_particles as f64
        } else {
            1.0
        };
        log::info!(
            "[PERF][WATER] particles {} grid {:?} nodes {} substeps {} total {:.2}ms avg {:.3}ms/substep repair {:.2}ms clear {:.2}ms p2g {:.2}ms grid {:.2}ms grid_update {:.2}ms g2p {:.2}ms g2p_gather {:.2}ms g2p_box {:.2}ms g2p_terrain {:.2}ms g2p_repair {:.2}ms diagnostics {:.2}ms residual {:.2}ms shadow_measure {:.2}ms p2g_density_corr/substep {:.1} p2g_density_corr_factor_avg {:.3} p2g_density_corr_factor_max {:.3} terrain_cache_skips/substep {:.0} terrain_cache_projections/substep {:.0} terrain_exact_fallbacks/substep {:.0} terrain_exact_checks/substep {:.0} terrain_exact_corrections/substep {:.0} terrain_shadow_samples/substep {:.1} terrain_shadow_false_skips {} terrain_shadow_sdf_err_avg {:.5} terrain_shadow_sdf_err_max {:.5} active_nodes/substep {:.0} particle_y {:.3}..{:.3} avg {:.3} terrain_sdf_min {:.4} penetrating {} no_sdf {}",
            self.particles.len(),
            self.grid_dim,
            grid_nodes,
            stats.substeps,
            stats.total_seconds * 1000.0,
            stats.total_seconds * 1000.0 / substeps,
            stats.repair_seconds * 1000.0,
            stats.clear_seconds * 1000.0,
            stats.p2g_seconds * 1000.0,
            stats.grid_seconds * 1000.0,
            stats.grid_update_seconds * 1000.0,
            stats.g2p_seconds * 1000.0,
            stats.g2p_gather_seconds * 1000.0,
            stats.g2p_box_seconds * 1000.0,
            stats.g2p_terrain_seconds * 1000.0,
            stats.g2p_repair_seconds * 1000.0,
            stats.diagnostics_seconds * 1000.0,
            residual_seconds * 1000.0,
            shadow_measure_seconds * 1000.0,
            stats.p2g_density_correction_particles as f64 / substeps,
            density_correction_factor_avg,
            stats.p2g_density_correction_factor_max,
            stats.g2p_terrain_cache_skips as f64 / substeps,
            stats.g2p_terrain_cache_projections as f64 / substeps,
            stats.g2p_terrain_exact_fallbacks as f64 / substeps,
            stats.g2p_terrain_exact_checks as f64 / substeps,
            stats.g2p_terrain_exact_corrections as f64 / substeps,
            stats.g2p_terrain_shadow_samples as f64 / substeps,
            stats.g2p_terrain_shadow_false_skips,
            if stats.g2p_terrain_shadow_samples > 0 {
                stats.g2p_terrain_shadow_sdf_abs_error_sum
                    / stats.g2p_terrain_shadow_samples as f64
            } else {
                f64::NAN
            },
            stats.g2p_terrain_shadow_sdf_abs_error_max,
            stats.active_node_visits as f64 / substeps,
            particle_stats.min_ws.y,
            particle_stats.max_ws.y,
            particle_stats.avg_ws.y,
            particle_stats.min_terrain_sdf.unwrap_or(f32::NAN),
            particle_stats.terrain_penetrating,
            particle_stats.no_terrain_sdf,
        );

        self.perf_stats.reset();
        self.perf_report_seconds = 0.0;
    }

    fn measure_terrain_shadow_samples(&self) -> TerrainShadowSampleStats {
        // Keep expensive exact-SDF cache validation out of the measured G2P hot loop.
        // Sampling once per perf report still catches cache drift without charging every substep.
        let mut stats = TerrainShadowSampleStats::default();
        let Some(terrain) = self.terrain.as_ref() else {
            return stats;
        };

        let terrain_collision_margin = self.terrain_collision_margin();
        for (particle_idx, particle) in self.particles.iter().enumerate() {
            if !should_shadow_sample_terrain(particle_idx) {
                continue;
            }

            let local_pos = particle.x - self.origin_ws;
            match terrain_grid_particle_query(
                local_pos,
                self.inv_dx,
                self.dx,
                self.grid_dim,
                &self.terrain_grid,
                terrain_collision_margin,
            ) {
                TerrainGridParticleQuery::Skip { sdf } => {
                    stats.samples += 1;
                    if let Some(exact_sdf) = terrain.sample_sdf_ws(particle.x) {
                        let abs_error = (sdf - exact_sdf).abs();
                        stats.sdf_abs_error_sum += abs_error as f64;
                        stats.sdf_abs_error_max = stats.sdf_abs_error_max.max(abs_error);
                        if exact_sdf <= terrain_collision_margin {
                            stats.false_skips += 1;
                        }
                    }
                }
                TerrainGridParticleQuery::CachedProjection { cached_sdf, .. } => {
                    stats.samples += 1;
                    if let Some(exact_sdf) = terrain.sample_sdf_ws(particle.x) {
                        let abs_error = (cached_sdf - exact_sdf).abs();
                        stats.sdf_abs_error_sum += abs_error as f64;
                        stats.sdf_abs_error_max = stats.sdf_abs_error_max.max(abs_error);
                    }
                }
                TerrainGridParticleQuery::ExactFallback => {}
            }
        }

        stats
    }

    pub(crate) fn rebuild_terrain_grid_cache(&mut self) {
        let rebuild_start = Instant::now();
        let near_surface_band = self.terrain_grid_near_surface_band();
        let terrain_chunk_count = self.terrain.as_ref().map_or(0, |terrain| terrain.chunks.len());
        let grid_dim = self.grid_dim;
        let node_count = self.grid.len();

        self.ensure_terrain_grid_cache_len();
        let stats = self.rebuild_terrain_grid_cache_range(UVec3::ZERO, grid_dim, near_surface_band);

        log::info!(
            "[WATER][TERRAIN_CACHE] rebuilt grid cache chunks={} grid {:?} nodes={} has_sdf={} near_surface={} normals={} band={:.5} dx={:.5} total_ms={:.2}",
            terrain_chunk_count,
            grid_dim,
            node_count,
            stats.has_sdf_count,
            stats.near_surface_count,
            stats.normal_count,
            near_surface_band,
            self.dx,
            rebuild_start.elapsed().as_secs_f32() * 1000.0,
        );
    }

    pub fn terrain_grid_cache_build_request_for_chunk(
        &self,
        chunk_id: IVec3,
    ) -> Option<WaterTerrainCacheBuildRequest> {
        let (min_node, max_node_exclusive) = self.terrain_grid_cache_range_for_chunk(chunk_id)?;
        Some(WaterTerrainCacheBuildRequest {
            chunk_id,
            terrain_chunk_count: self.terrain.as_ref().map_or(0, |terrain| terrain.chunks.len()),
            terrain: self.terrain.clone(),
            origin_ws: self.origin_ws,
            grid_dim: self.grid_dim,
            dx: self.dx,
            min_node,
            max_node_exclusive,
            near_surface_band: self.terrain_grid_near_surface_band(),
        })
    }

    pub fn apply_terrain_grid_cache_patch(
        &mut self,
        patch: WaterTerrainCachePatch,
    ) -> Option<WaterTerrainCacheApplyReport> {
        let apply_start = Instant::now();
        if patch.grid_dim != self.grid_dim {
            log::warn!(
                "[WATER][TERRAIN_CACHE] discarded worker grid cache patch chunk={:?} grid {:?} current_grid {:?}: grid changed",
                patch.chunk_id,
                patch.grid_dim,
                self.grid_dim,
            );
            return None;
        }

        self.ensure_terrain_grid_cache_len();
        let expected_samples = terrain_cache_range_node_count(
            patch.min_node,
            patch.max_node_exclusive,
        );
        if patch.samples.len() != expected_samples {
            log::warn!(
                "[WATER][TERRAIN_CACHE] discarded worker grid cache patch chunk={:?} range {:?}..{:?}: sample count {} expected {}",
                patch.chunk_id,
                patch.min_node,
                patch.max_node_exclusive,
                patch.samples.len(),
                expected_samples,
            );
            return None;
        }

        let mut sample_idx = 0usize;
        for z in patch.min_node.z..patch.max_node_exclusive.z {
            for y in patch.min_node.y..patch.max_node_exclusive.y {
                for x in patch.min_node.x..patch.max_node_exclusive.x {
                    let idx = grid_index_dims(self.grid_dim, x, y, z);
                    self.terrain_grid[idx] = patch.samples[sample_idx];
                    sample_idx += 1;
                }
            }
        }

        Some(WaterTerrainCacheApplyReport {
            chunk_id: patch.chunk_id,
            terrain_chunk_count: patch.terrain_chunk_count,
            grid_dim: patch.grid_dim,
            min_node: patch.min_node,
            max_node_exclusive: patch.max_node_exclusive,
            node_count: patch.stats.node_count,
            has_sdf_count: patch.stats.has_sdf_count,
            near_surface_count: patch.stats.near_surface_count,
            normal_count: patch.stats.normal_count,
            near_surface_band: patch.near_surface_band,
            dx: patch.dx,
            build_ms: patch.build_ms,
            apply_ms: apply_start.elapsed().as_secs_f32() * 1000.0,
        })
    }

    pub fn rebuild_terrain_grid_cache_for_chunk(&mut self, chunk_id: IVec3) {
        let rebuild_start = Instant::now();
        let near_surface_band = self.terrain_grid_near_surface_band();
        let terrain_chunk_count = self.terrain.as_ref().map_or(0, |terrain| terrain.chunks.len());
        let grid_dim = self.grid_dim;

        self.ensure_terrain_grid_cache_len();
        let Some((min_node, max_node_exclusive)) = self.terrain_grid_cache_range_for_chunk(chunk_id)
        else {
            log::info!(
                "[WATER][TERRAIN_CACHE] skipped grid cache region chunk={:?} chunks={} grid {:?} outside=true total_ms={:.2}",
                chunk_id,
                terrain_chunk_count,
                grid_dim,
                rebuild_start.elapsed().as_secs_f32() * 1000.0,
            );
            return;
        };

        let stats = self.rebuild_terrain_grid_cache_range(
            min_node,
            max_node_exclusive,
            near_surface_band,
        );

        log::info!(
            "[WATER][TERRAIN_CACHE] rebuilt grid cache region chunk={:?} chunks={} grid {:?} range {:?}..{:?} nodes={} has_sdf={} near_surface={} normals={} band={:.5} dx={:.5} total_ms={:.2}",
            chunk_id,
            terrain_chunk_count,
            grid_dim,
            min_node,
            max_node_exclusive,
            stats.node_count,
            stats.has_sdf_count,
            stats.near_surface_count,
            stats.normal_count,
            near_surface_band,
            self.dx,
            rebuild_start.elapsed().as_secs_f32() * 1000.0,
        );
    }

    pub fn invalidate_terrain_grid_cache_for_chunk(&mut self, chunk_id: IVec3) {
        let invalidate_start = Instant::now();
        let terrain_chunk_count = self.terrain.as_ref().map_or(0, |terrain| terrain.chunks.len());
        let grid_dim = self.grid_dim;

        self.ensure_terrain_grid_cache_len();
        let Some((min_node, max_node_exclusive)) = self.terrain_grid_cache_range_for_chunk(chunk_id)
        else {
            log::debug!(
                "[WATER][TERRAIN_CACHE] skipped invalidating grid cache region chunk={:?} chunks={} grid {:?} outside=true total_ms={:.2}",
                chunk_id,
                terrain_chunk_count,
                grid_dim,
                invalidate_start.elapsed().as_secs_f32() * 1000.0,
            );
            return;
        };

        let mut node_count = 0usize;
        for z in min_node.z..max_node_exclusive.z {
            for y in min_node.y..max_node_exclusive.y {
                for x in min_node.x..max_node_exclusive.x {
                    let idx = grid_index_dims(grid_dim, x, y, z);
                    self.terrain_grid[idx] = WaterTerrainGridSample::default();
                    node_count += 1;
                }
            }
        }

        log::debug!(
            "[WATER][TERRAIN_CACHE] invalidated grid cache region chunk={:?} chunks={} grid {:?} range {:?}..{:?} nodes={} total_ms={:.2}",
            chunk_id,
            terrain_chunk_count,
            grid_dim,
            min_node,
            max_node_exclusive,
            node_count,
            invalidate_start.elapsed().as_secs_f32() * 1000.0,
        );
    }

    fn ensure_terrain_grid_cache_len(&mut self) {
        let node_count = self.grid.len();
        if self.terrain_grid.len() != node_count {
            self.terrain_grid = vec![WaterTerrainGridSample::default(); node_count];
        }
    }

    fn terrain_grid_near_surface_band(&self) -> f32 {
        // Cache a conservative narrow band around terrain. Hot loops use this
        // cheap water-grid SDF to skip exact collider queries for particles and
        // grid nodes that are clearly away from solids.
        self.terrain_collision_margin() + self.dx * 2.0
    }

    fn terrain_grid_cache_range_for_chunk(&self, chunk_id: IVec3) -> Option<(UVec3, UVec3)> {
        terrain_grid_cache_range_for_chunk_parts(
            self.origin_ws,
            self.inv_dx,
            self.grid_dim,
            chunk_id,
        )
    }

    fn rebuild_terrain_grid_cache_range(
        &mut self,
        min_node: UVec3,
        max_node_exclusive: UVec3,
        near_surface_band: f32,
    ) -> WaterTerrainCacheRebuildStats {
        let terrain = self.terrain.as_ref();
        let origin_ws = self.origin_ws;
        let grid_dim = self.grid_dim;
        let dx = self.dx;
        let mut stats = WaterTerrainCacheRebuildStats::default();

        for z in min_node.z..max_node_exclusive.z {
            for y in min_node.y..max_node_exclusive.y {
                for x in min_node.x..max_node_exclusive.x {
                    let idx = grid_index_dims(grid_dim, x, y, z);
                    let mut sample = WaterTerrainGridSample::default();
                    if let Some(terrain) = terrain {
                        let node_world = origin_ws + Vec3::new(x as f32, y as f32, z as f32) * dx;
                        if let Some(sdf) = terrain.sample_sdf_ws(node_world) {
                            sample.sdf = sdf;
                            sample.has_sdf = true;
                            stats.has_sdf_count += 1;
                            sample.near_surface = sdf <= near_surface_band;
                            if sample.near_surface {
                                stats.near_surface_count += 1;
                                sample.normal =
                                    terrain.sample_normal_ws(node_world).unwrap_or(Vec3::Y);
                                stats.normal_count += 1;
                            }
                        }
                    }
                    self.terrain_grid[idx] = sample;
                    stats.node_count += 1;
                }
            }
        }

        stats
    }

    fn clear_grid(&mut self) {
        for node_idx in self.touched_grid_nodes.drain(..) {
            if let Some(node) = self.grid.get_mut(node_idx) {
                node.v = Vec3::ZERO;
                node.mass = 0.0;
                node.solid = false;
                node.normal = Vec3::ZERO;
            }
        }
    }

    fn particle_to_grid(&mut self, dt: f32) {
        self.particle_to_grid_mass_momentum();
        self.particle_to_grid_fluid_stress(dt);
    }

    fn particle_to_grid_mass_momentum(&mut self) {
        let origin_ws = self.origin_ws;
        let grid_dim = self.grid_dim;
        let dx = self.dx;
        let inv_dx = self.inv_dx;
        let mass = self.config.particle_mass;
        let y_stride = grid_dim.x as usize;
        let z_stride = y_stride * grid_dim.y as usize;

        for particle in &self.particles {
            let local_pos = particle.x - origin_ws;
            let grid_pos = local_pos * inv_dx;
            let base = base_coord(grid_pos);
            let fx = grid_pos - base.as_vec3();
            let weights = quadratic_weights(fx);
            let wx = [weights[0].x, weights[1].x, weights[2].x];
            let wy = [weights[0].y, weights[1].y, weights[2].y];
            let wz = [weights[0].z, weights[1].z, weights[2].z];

            let affine = particle.c * mass;
            let momentum = particle.v * mass;

            // Most particles are kept away from grid boundaries by wall padding;
            // use linear strides for fully interior stencils to avoid per-node
            // bounds checks and 3D->1D index recomputation in the P2G hot loop.
            if particle_stencil_interior(base, grid_dim) {
                let base_idx =
                    grid_index_dims(grid_dim, base.x as u32, base.y as u32, base.z as u32);
                let base_dpos = base.as_vec3() * dx - local_pos;
                for (oz, wz) in wz.iter().copied().enumerate() {
                    let node_z_offset = oz * z_stride;
                    let dpos_z = base_dpos.z + oz as f32 * dx;
                    for (oy, wy) in wy.iter().copied().enumerate() {
                        let node_y_offset = oy * y_stride;
                        let dpos_y = base_dpos.y + oy as f32 * dx;
                        let wyz = wy * wz;
                        for (ox, wx) in wx.iter().copied().enumerate() {
                            let weight = wx * wyz;
                            if weight <= 0.0 {
                                continue;
                            }

                            let node_idx = base_idx + ox + node_y_offset + node_z_offset;
                            debug_assert!(node_idx < self.grid.len());
                            // SAFETY: `particle_stencil_interior` guarantees all 27 stencil
                            // nodes are inside `grid_dim`, and the grid storage is sized from
                            // that same domain.
                            let grid_node = unsafe { self.grid.get_unchecked_mut(node_idx) };
                            if grid_node.mass <= 0.0 {
                                self.touched_grid_nodes.push(node_idx);
                            }
                            grid_node.mass += weight * mass;
                            let dpos = Vec3::new(base_dpos.x + ox as f32 * dx, dpos_y, dpos_z);
                            grid_node.v += weight * (momentum + affine * dpos);
                        }
                    }
                }
            } else {
                for oz in 0..3 {
                    for oy in 0..3 {
                        for ox in 0..3 {
                            let node = base + IVec3::new(ox, oy, oz);
                            if !in_grid(node, grid_dim) {
                                continue;
                            }

                            let weight = wx[ox as usize] * wy[oy as usize] * wz[oz as usize];
                            if weight <= 0.0 {
                                continue;
                            }

                            let node_idx = grid_index_dims(
                                grid_dim,
                                node.x as u32,
                                node.y as u32,
                                node.z as u32,
                            );
                            let grid_node = &mut self.grid[node_idx];
                            if grid_node.mass <= 0.0 {
                                self.touched_grid_nodes.push(node_idx);
                            }
                            grid_node.mass += weight * mass;
                            let node_local = node.as_vec3() * dx;
                            let dpos = node_local - local_pos;
                            grid_node.v += weight * (momentum + affine * dpos);
                        }
                    }
                }
            }
        }
    }

    fn particle_to_grid_fluid_stress(&mut self, dt: f32) {
        if dt <= 0.0 || !dt.is_finite() {
            return;
        }

        let origin_ws = self.origin_ws;
        let grid_dim = self.grid_dim;
        let dx = self.dx;
        let inv_dx = self.inv_dx;
        let inv_cell_volume = inv_dx * inv_dx * inv_dx;
        let d_inv = 4.0 * inv_dx * inv_dx;
        let mass = self.config.particle_mass;
        let rest_density = self.config.particle_mass / self.config.particle_volume;
        let stiffness = self.config.stiffness;
        let gamma = self.config.gamma;
        let dynamic_viscosity = self.config.dynamic_viscosity;
        let pressure_floor = self.config.pressure_floor;
        let terrain_grid = &self.terrain_grid;
        let y_stride = grid_dim.x as usize;
        let z_stride = y_stride * grid_dim.y as usize;
        let mut density_correction_particles = 0u64;
        let mut density_correction_factor_sum = 0.0f64;
        let mut density_correction_factor_max = 1.0f32;

        for particle in &self.particles {
            let local_pos = particle.x - origin_ws;
            let grid_pos = local_pos * inv_dx;
            let base = base_coord(grid_pos);
            let fx = grid_pos - base.as_vec3();
            let weights = quadratic_weights(fx);
            let wx = [weights[0].x, weights[1].x, weights[2].x];
            let wy = [weights[0].y, weights[1].y, weights[2].y];
            let wz = [weights[0].z, weights[1].z, weights[2].z];

            let Some(raw_density) = particle_density_from_grid(
                &self.grid,
                grid_dim,
                base,
                wx,
                wy,
                wz,
                inv_cell_volume,
            ) else {
                continue;
            };
            let density_sample = terrain_boundary_density_correction(
                raw_density,
                &self.grid,
                terrain_grid,
                grid_dim,
                base,
                wx,
                wy,
                wz,
                dx,
                inv_dx,
                inv_cell_volume,
                rest_density,
                stiffness,
                gamma,
                pressure_floor,
                self.config.gravity,
            );
            if density_sample.correction_factor > 1.0 + f32::EPSILON {
                density_correction_particles += 1;
                density_correction_factor_sum += density_sample.correction_factor as f64;
                density_correction_factor_max =
                    density_correction_factor_max.max(density_sample.correction_factor);
            }
            let density = density_sample.density;
            let volume = mass / density;
            if !volume.is_finite() || volume <= 0.0 {
                continue;
            }

            let pressure = fluid_eos_pressure(
                stiffness,
                gamma,
                density,
                rest_density,
                pressure_floor,
            );
            let stress = fluid_stress(pressure, dynamic_viscosity, particle.c);
            if !mat3_is_finite(stress) {
                continue;
            }
            let stress_affine = stress * (-dt * volume * d_inv);

            if particle_stencil_interior(base, grid_dim) {
                let base_idx =
                    grid_index_dims(grid_dim, base.x as u32, base.y as u32, base.z as u32);
                let base_dpos = base.as_vec3() * dx - local_pos;
                for (oz, wz) in wz.iter().copied().enumerate() {
                    let node_z_offset = oz * z_stride;
                    let dpos_z = base_dpos.z + oz as f32 * dx;
                    for (oy, wy) in wy.iter().copied().enumerate() {
                        let node_y_offset = oy * y_stride;
                        let dpos_y = base_dpos.y + oy as f32 * dx;
                        let wyz = wy * wz;
                        for (ox, wx) in wx.iter().copied().enumerate() {
                            let weight = wx * wyz;
                            if weight <= 0.0 {
                                continue;
                            }

                            let node_idx = base_idx + ox + node_y_offset + node_z_offset;
                            debug_assert!(node_idx < self.grid.len());
                            // SAFETY: `particle_stencil_interior` guarantees all 27 stencil
                            // nodes are inside `grid_dim`, and the grid storage is sized from
                            // that same domain.
                            let grid_node = unsafe { self.grid.get_unchecked_mut(node_idx) };
                            let dpos = Vec3::new(base_dpos.x + ox as f32 * dx, dpos_y, dpos_z);
                            grid_node.v += weight * (stress_affine * dpos);
                        }
                    }
                }
            } else {
                for oz in 0..3 {
                    for oy in 0..3 {
                        for ox in 0..3 {
                            let node = base + IVec3::new(ox, oy, oz);
                            if !in_grid(node, grid_dim) {
                                continue;
                            }

                            let weight = wx[ox as usize] * wy[oy as usize] * wz[oz as usize];
                            if weight <= 0.0 {
                                continue;
                            }

                            let node_idx = grid_index_dims(
                                grid_dim,
                                node.x as u32,
                                node.y as u32,
                                node.z as u32,
                            );
                            let node_local = node.as_vec3() * dx;
                            let dpos = node_local - local_pos;
                            self.grid[node_idx].v += weight * (stress_affine * dpos);
                        }
                    }
                }
            }
        }

        self.diagnostic_stats.p2g_density_correction_particles += density_correction_particles;
        self.diagnostic_stats.p2g_density_correction_factor_sum += density_correction_factor_sum;
        self.diagnostic_stats.p2g_density_correction_factor_max = self
            .diagnostic_stats
            .p2g_density_correction_factor_max
            .max(density_correction_factor_max);
        self.perf_stats.p2g_density_correction_particles += density_correction_particles;
        self.perf_stats.p2g_density_correction_factor_sum += density_correction_factor_sum;
        self.perf_stats.p2g_density_correction_factor_max = self
            .perf_stats
            .p2g_density_correction_factor_max
            .max(density_correction_factor_max);
    }

    fn update_grid(&mut self, dt: f32) -> usize {
        let gravity = self.config.gravity;
        let linear_damping = velocity_damping_factor(self.config.linear_damping_per_sec, dt);
        let terrain_collision_margin = self.terrain_collision_margin();
        let terrain_tangent_damping_per_sec = self.config.terrain_tangent_damping_per_sec;
        let wall_damping = self.config.wall_damping.clamp(0.0, 1.0);
        let terrain_grid = &self.terrain_grid;
        let grid_boundary_flags = &self.grid_boundary_flags;

        let mut active_nodes = 0usize;

        for &idx in &self.touched_grid_nodes {
            let node = &mut self.grid[idx];
            if node.mass <= ACTIVE_MASS_EPSILON {
                continue;
            }

            let boundary_flags = grid_boundary_flags[idx];

            active_nodes += 1;
            node.v /= node.mass;
            node.v += gravity * dt;
            node.v *= linear_damping;

            project_grid_node_collisions(
                node,
                boundary_flags,
                terrain_grid[idx],
                terrain_collision_margin,
                terrain_tangent_damping_per_sec,
                wall_damping,
                dt,
            );
        }

        active_nodes
    }

    fn grid_to_particle(&mut self, dt: f32) -> WaterG2pBreakdown {
        self.grid_to_particle_impl::<false>(dt)
    }

    fn grid_to_particle_timed(&mut self, dt: f32) -> WaterG2pBreakdown {
        self.grid_to_particle_impl::<true>(dt)
    }

    fn grid_to_particle_impl<const COLLECT_BREAKDOWN: bool>(&mut self, dt: f32) -> WaterG2pBreakdown {
        let total_start = COLLECT_BREAKDOWN.then(Instant::now);
        let mut gather_seconds = 0.0;
        let mut box_seconds = 0.0;
        let mut terrain_seconds = 0.0;
        let mut repair_seconds = 0.0;
        let mut terrain_cache_skips = 0u64;
        let mut terrain_cache_projections = 0u64;
        let mut terrain_exact_fallbacks = 0u64;
        let mut terrain_exact_checks = 0u64;
        let mut terrain_exact_corrections = 0u64;
        let origin_ws = self.origin_ws;
        let grid_dim = self.grid_dim;
        let dx = self.dx;
        let inv_dx = self.inv_dx;
        let y_stride = grid_dim.x as usize;
        let z_stride = y_stride * grid_dim.y as usize;
        let c_scale = 4.0 * inv_dx * inv_dx;
        let inv_cell_volume = inv_dx * inv_dx * inv_dx;
        let rest_density = self.config.particle_mass / self.config.particle_volume;
        let affine_damping = affine_damping_factor(dt);
        let j_min = self.config.j_min;
        let bounds = self.config.collider;
        let particle_padding = self.dx * self.config.wall_padding_cells.max(1.0);
        let particle_min_padding = Vec3::splat(particle_padding);
        let particle_max_padding = Vec3::splat(particle_padding);
        let max_particle_speed = max_particle_speed_for_substep(dx, dt);
        let grid = &self.grid;
        let terrain_grid = &self.terrain_grid;
        let terrain = self.terrain.as_ref();
        let terrain_collision_margin = self.terrain_collision_margin();
        let terrain_max_correction = particle_padding;

        for particle in &mut self.particles {
            let gather_start = COLLECT_BREAKDOWN.then(Instant::now);
            let local_pos = particle.x - origin_ws;
            let grid_pos = local_pos * inv_dx;
            let base = base_coord(grid_pos);
            let fx = grid_pos - base.as_vec3();
            let weights = quadratic_weights(fx);
            let wx = [weights[0].x, weights[1].x, weights[2].x];
            let wy = [weights[0].y, weights[1].y, weights[2].y];
            let wz = [weights[0].z, weights[1].z, weights[2].z];

            let mut new_v = Vec3::ZERO;
            let mut new_density_mass = 0.0f32;
            let mut new_c_x = Vec3::ZERO;
            let mut new_c_y = Vec3::ZERO;
            let mut new_c_z = Vec3::ZERO;
            if particle_stencil_interior(base, grid_dim) {
                let base_idx =
                    grid_index_dims(grid_dim, base.x as u32, base.y as u32, base.z as u32);
                let base_dpos = base.as_vec3() * dx - local_pos;
                for (oz, wz) in wz.iter().copied().enumerate() {
                    let node_z_offset = oz * z_stride;
                    let dpos_z = base_dpos.z + oz as f32 * dx;
                    for (oy, wy) in wy.iter().copied().enumerate() {
                        let node_y_offset = oy * y_stride;
                        let dpos_y = base_dpos.y + oy as f32 * dx;
                        let wyz = wy * wz;
                        for (ox, wx) in wx.iter().copied().enumerate() {
                            let weight = wx * wyz;
                            let node_idx = base_idx + ox + node_y_offset + node_z_offset;
                            debug_assert!(node_idx < grid.len());
                            // SAFETY: `particle_stencil_interior` guarantees all 27 stencil
                            // nodes are inside `grid_dim`, and the grid storage is sized from
                            // that same domain.
                            let grid_node = unsafe { grid.get_unchecked(node_idx) };
                            let grid_v = grid_node.v;
                            new_density_mass += weight * grid_node.mass;
                            let weighted_v = grid_v * weight;
                            new_v += weighted_v;
                            let dpos = Vec3::new(base_dpos.x + ox as f32 * dx, dpos_y, dpos_z);
                            new_c_x += weighted_v * dpos.x;
                            new_c_y += weighted_v * dpos.y;
                            new_c_z += weighted_v * dpos.z;
                        }
                    }
                }
            } else {
                for oz in 0..3 {
                    for oy in 0..3 {
                        for ox in 0..3 {
                            let node = base + IVec3::new(ox, oy, oz);
                            if !in_grid(node, grid_dim) {
                                continue;
                            }

                            let weight = wx[ox as usize] * wy[oy as usize] * wz[oz as usize];
                            if weight <= 0.0 {
                                continue;
                            }

                            let node_idx = grid_index_dims(
                                grid_dim,
                                node.x as u32,
                                node.y as u32,
                                node.z as u32,
                            );
                            let grid_node = &grid[node_idx];
                            new_density_mass += weight * grid_node.mass;
                            let weighted_v = grid_node.v * weight;
                            new_v += weighted_v;
                            let node_local = node.as_vec3() * dx;
                            let dpos = node_local - local_pos;
                            new_c_x += weighted_v * dpos.x;
                            new_c_y += weighted_v * dpos.y;
                            new_c_z += weighted_v * dpos.z;
                        }
                    }
                }
            }
            let new_c = Mat3::from_cols(new_c_x * c_scale, new_c_y * c_scale, new_c_z * c_scale)
                * affine_damping;
            if let Some(gather_start) = gather_start {
                gather_seconds += gather_start.elapsed().as_secs_f64();
            }

            particle.v = clamp_vec3_length(new_v, max_particle_speed);
            particle.c = clamp_mat3_components(new_c, MAX_AFFINE_COMPONENT);
            particle.j = grid_density_no_tension_j(
                new_density_mass,
                inv_cell_volume,
                rest_density,
                j_min,
            )
            .unwrap_or(NO_TENSION_MAX_J);
            particle.x += particle.v * dt;

            let box_start = COLLECT_BREAKDOWN.then(Instant::now);
            collide_particle_with_box_with_padding(
                particle,
                bounds.min_ws,
                bounds.max_ws,
                particle_min_padding,
                particle_max_padding,
            );
            if let Some(box_start) = box_start {
                box_seconds += box_start.elapsed().as_secs_f64();
            }

            if particle.x.is_finite() {
                if let Some(terrain) = terrain {
                    let local_pos = particle.x - origin_ws;
                    let terrain_start = COLLECT_BREAKDOWN.then(Instant::now);
                    match terrain_grid_particle_query(
                        local_pos,
                        inv_dx,
                        dx,
                        grid_dim,
                        terrain_grid,
                        terrain_collision_margin,
                    ) {
                        TerrainGridParticleQuery::Skip { .. } => {
                            terrain_cache_skips += 1;
                        }
                        TerrainGridParticleQuery::CachedProjection { sdf, normal, .. } => {
                            terrain_cache_projections += 1;
                            project_particle_with_cached_terrain(
                                particle,
                                sdf,
                                normal,
                                terrain_collision_margin,
                                terrain_max_correction,
                                bounds.min_ws,
                                bounds.max_ws,
                                particle_min_padding,
                                particle_max_padding,
                            );
                        }
                        TerrainGridParticleQuery::ExactFallback => {
                            terrain_exact_fallbacks += 1;
                            terrain_exact_checks += 1;
                            if collide_particle_with_terrain_iterative(
                                particle,
                                terrain,
                                terrain_collision_margin,
                                terrain_max_correction,
                                TERRAIN_PARTICLE_COLLISION_ITERATIONS,
                                bounds.min_ws,
                                bounds.max_ws,
                                particle_min_padding,
                                particle_max_padding,
                            ) {
                                terrain_exact_corrections += 1;
                            }
                        }
                    }
                    if let Some(terrain_start) = terrain_start {
                        terrain_seconds += terrain_start.elapsed().as_secs_f64();
                    }
                }
            }

            let repair_start = COLLECT_BREAKDOWN.then(Instant::now);
            repair_particle_state_after_g2p_with_padding(
                particle,
                bounds.min_ws,
                bounds.max_ws,
                particle_min_padding,
                particle_max_padding,
                max_particle_speed,
                j_min,
            );
            if let Some(repair_start) = repair_start {
                repair_seconds += repair_start.elapsed().as_secs_f64();
            }
        }

        WaterG2pBreakdown {
            total_seconds: total_start
                .map(|start| start.elapsed().as_secs_f64())
                .unwrap_or(0.0),
            gather_seconds,
            box_seconds,
            terrain_seconds,
            repair_seconds,
            terrain_cache_skips,
            terrain_cache_projections,
            terrain_exact_fallbacks,
            terrain_exact_checks,
            terrain_exact_corrections,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct TerrainShadowSampleStats {
    samples: u64,
    false_skips: u64,
    sdf_abs_error_sum: f64,
    sdf_abs_error_max: f32,
}

#[derive(Clone, Copy, Debug, Default)]
struct WaterG2pBreakdown {
    total_seconds: f64,
    gather_seconds: f64,
    box_seconds: f64,
    terrain_seconds: f64,
    repair_seconds: f64,
    terrain_cache_skips: u64,
    terrain_cache_projections: u64,
    terrain_exact_fallbacks: u64,
    terrain_exact_checks: u64,
    terrain_exact_corrections: u64,
}

fn base_coord(grid_pos: Vec3) -> IVec3 {
    let base = (grid_pos - Vec3::splat(0.5)).floor();
    IVec3::new(base.x as i32, base.y as i32, base.z as i32)
}

fn quadratic_weights(fx: Vec3) -> [Vec3; 3] {
    let w0 = Vec3::splat(1.5) - fx;
    let w1 = fx - Vec3::ONE;
    let w2 = fx - Vec3::splat(0.5);
    [
        0.5 * w0 * w0,
        Vec3::splat(0.75) - w1 * w1,
        0.5 * w2 * w2,
    ]
}

fn particle_density_from_grid(
    grid: &[super::pond::WaterGridNode],
    grid_dim: glam::UVec3,
    base: IVec3,
    wx: [f32; 3],
    wy: [f32; 3],
    wz: [f32; 3],
    inv_cell_volume: f32,
) -> Option<f32> {
    if grid.is_empty() || inv_cell_volume <= 0.0 || !inv_cell_volume.is_finite() {
        return None;
    }

    let mut gathered_mass = 0.0f32;
    for oz in 0..3 {
        for oy in 0..3 {
            for ox in 0..3 {
                let node = base + IVec3::new(ox, oy, oz);
                if !in_grid(node, grid_dim) {
                    continue;
                }

                let weight = wx[ox as usize] * wy[oy as usize] * wz[oz as usize];
                if weight <= 0.0 {
                    continue;
                }

                let node_idx = grid_index_dims(
                    grid_dim,
                    node.x as u32,
                    node.y as u32,
                    node.z as u32,
                );
                gathered_mass += grid[node_idx].mass * weight;
            }
        }
    }

    let density = gathered_mass * inv_cell_volume;
    (density.is_finite() && density > MIN_FLUID_DENSITY).then_some(density)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TerrainBoundaryDensitySample {
    density: f32,
    correction_factor: f32,
    fluid_fraction: f32,
    solid_weight: f32,
}

fn terrain_boundary_density_correction(
    raw_density: f32,
    grid: &[WaterGridNode],
    terrain_grid: &[WaterTerrainGridSample],
    grid_dim: UVec3,
    base: IVec3,
    wx: [f32; 3],
    wy: [f32; 3],
    wz: [f32; 3],
    dx: f32,
    inv_dx: f32,
    inv_cell_volume: f32,
    rest_density: f32,
    stiffness: f32,
    gamma: f32,
    pressure_floor: f32,
    gravity: Vec3,
) -> TerrainBoundaryDensitySample {
    if !raw_density.is_finite() || raw_density <= 0.0 {
        return TerrainBoundaryDensitySample {
            density: raw_density,
            correction_factor: 1.0,
            fluid_fraction: 1.0,
            solid_weight: 0.0,
        };
    }

    let ghost = terrain_ghost_density_contribution(
        raw_density,
        grid,
        terrain_grid,
        grid_dim,
        base,
        wx,
        wy,
        wz,
        dx,
        inv_dx,
        inv_cell_volume,
        rest_density,
        stiffness,
        gamma,
        pressure_floor,
        gravity,
    );
    let solid_weight = ghost.solid_weight;
    if solid_weight <= TERRAIN_DENSITY_MIN_SOLID_WEIGHT {
        return TerrainBoundaryDensitySample {
            density: raw_density,
            correction_factor: 1.0,
            fluid_fraction: 1.0,
            solid_weight,
        };
    }

    let min_fluid_fraction = TERRAIN_DENSITY_MIN_FLUID_FRACTION.clamp(1.0e-3, 1.0);
    let fluid_fraction = (1.0 - solid_weight).clamp(min_fluid_fraction, 1.0);
    let max_correction_factor = TERRAIN_DENSITY_MAX_CORRECTION_FACTOR.max(1.0);
    let max_density = raw_density * max_correction_factor;
    let fallback_density = raw_density
        * fluid_fraction
            .recip()
            .min(max_correction_factor);
    let ghost_density = raw_density + ghost.weighted_density;
    let density = if ghost_density.is_finite() && ghost_density > raw_density {
        ghost_density.min(max_density).max(raw_density)
    } else {
        fallback_density
    };
    let density = if density.is_finite() && density > 0.0 {
        density
    } else {
        raw_density
    };
    TerrainBoundaryDensitySample {
        density,
        correction_factor: (density / raw_density).max(1.0),
        fluid_fraction,
        solid_weight,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct TerrainGhostDensityContribution {
    solid_weight: f32,
    weighted_density: f32,
}

fn terrain_ghost_density_contribution(
    raw_density: f32,
    grid: &[WaterGridNode],
    terrain_grid: &[WaterTerrainGridSample],
    grid_dim: UVec3,
    base: IVec3,
    wx: [f32; 3],
    wy: [f32; 3],
    wz: [f32; 3],
    dx: f32,
    inv_dx: f32,
    inv_cell_volume: f32,
    rest_density: f32,
    stiffness: f32,
    gamma: f32,
    pressure_floor: f32,
    gravity: Vec3,
) -> TerrainGhostDensityContribution {
    if terrain_grid.is_empty() || dx <= 0.0 || !dx.is_finite() {
        return TerrainGhostDensityContribution::default();
    }

    let mut solid_weight = 0.0f32;
    let mut weighted_density = 0.0f32;
    for oz in 0..3 {
        for oy in 0..3 {
            for ox in 0..3 {
                let node = base + IVec3::new(ox, oy, oz);
                if !in_grid(node, grid_dim) {
                    continue;
                }

                let weight = wx[ox as usize] * wy[oy as usize] * wz[oz as usize];
                if weight <= 0.0 {
                    continue;
                }

                let node_idx = grid_index_dims(
                    grid_dim,
                    node.x as u32,
                    node.y as u32,
                    node.z as u32,
                );
                let Some(sample) = terrain_grid.get(node_idx) else {
                    continue;
                };
                if !sample.has_sdf {
                    continue;
                }

                let occupancy = terrain_solid_occupancy_from_sdf(sample.sdf, dx);
                if occupancy <= 0.0 {
                    continue;
                }
                solid_weight += weight * occupancy;

                let Some(normal) = terrain_sample_normal(*sample) else {
                    continue;
                };
                let node_local = node.as_vec3() * dx;
                let surface_local = node_local - sample.sdf * normal;
                let mirror_distance = (-sample.sdf)
                    .max(dx * TERRAIN_GHOST_MIRROR_MIN_DISTANCE_CELLS.max(0.0));
                let ghost_local = surface_local - mirror_distance * normal;
                let mirror_local = surface_local + mirror_distance * normal;
                let mirror_density = fluid_density_at_local_position(
                    grid,
                    grid_dim,
                    mirror_local,
                    inv_dx,
                    inv_cell_volume,
                )
                .unwrap_or_else(|| raw_density.max(rest_density));
                if !mirror_density.is_finite() || mirror_density <= 0.0 {
                    continue;
                }

                let mirror_pressure = fluid_eos_pressure(
                    stiffness,
                    gamma,
                    mirror_density,
                    rest_density,
                    pressure_floor,
                )
                .max(0.0);
                let hydrostatic_delta = rest_density * gravity.dot(ghost_local - mirror_local);
                let ghost_pressure = (mirror_pressure + hydrostatic_delta).max(0.0);
                let ghost_density = fluid_density_from_eos_pressure(
                    stiffness,
                    gamma,
                    ghost_pressure,
                    rest_density,
                )
                .unwrap_or(mirror_density);
                if ghost_density.is_finite() && ghost_density > MIN_FLUID_DENSITY {
                    weighted_density += weight * occupancy * ghost_density;
                }
            }
        }
    }

    TerrainGhostDensityContribution {
        solid_weight: solid_weight.clamp(0.0, 1.0),
        weighted_density: weighted_density.max(0.0),
    }
}

fn fluid_density_at_local_position(
    grid: &[WaterGridNode],
    grid_dim: UVec3,
    local_pos: Vec3,
    inv_dx: f32,
    inv_cell_volume: f32,
) -> Option<f32> {
    if !local_pos.is_finite() || inv_dx <= 0.0 || !inv_dx.is_finite() {
        return None;
    }

    let grid_pos = local_pos * inv_dx;
    let base = grid_pos.floor().as_ivec3();
    let frac = grid_pos - base.as_vec3();
    let wx = [1.0 - frac.x, frac.x];
    let wy = [1.0 - frac.y, frac.y];
    let wz = [1.0 - frac.z, frac.z];
    let mut gathered_mass = 0.0f32;
    for oz in 0..2 {
        for oy in 0..2 {
            for ox in 0..2 {
                let node = base + IVec3::new(ox, oy, oz);
                if !in_grid(node, grid_dim) {
                    continue;
                }

                let weight = wx[ox as usize] * wy[oy as usize] * wz[oz as usize];
                if weight <= 0.0 {
                    continue;
                }

                let node_idx = grid_index_dims(
                    grid_dim,
                    node.x as u32,
                    node.y as u32,
                    node.z as u32,
                );
                gathered_mass += grid[node_idx].mass * weight;
            }
        }
    }

    let density = gathered_mass * inv_cell_volume;
    (density.is_finite() && density > MIN_FLUID_DENSITY).then_some(density)
}

fn terrain_sample_normal(sample: WaterTerrainGridSample) -> Option<Vec3> {
    if !sample.normal.is_finite() {
        return None;
    }
    let len2 = sample.normal.length_squared();
    (len2 > 1.0e-8 && len2.is_finite()).then_some(sample.normal / len2.sqrt())
}

#[cfg(test)]
fn terrain_solid_kernel_weight(
    terrain_grid: &[WaterTerrainGridSample],
    grid_dim: glam::UVec3,
    base: IVec3,
    wx: [f32; 3],
    wy: [f32; 3],
    wz: [f32; 3],
    dx: f32,
) -> f32 {
    if terrain_grid.is_empty() || dx <= 0.0 || !dx.is_finite() {
        return 0.0;
    }

    let mut solid_weight = 0.0f32;
    for oz in 0..3 {
        for oy in 0..3 {
            for ox in 0..3 {
                let node = base + IVec3::new(ox, oy, oz);
                if !in_grid(node, grid_dim) {
                    continue;
                }

                let weight = wx[ox as usize] * wy[oy as usize] * wz[oz as usize];
                if weight <= 0.0 {
                    continue;
                }

                let node_idx = grid_index_dims(
                    grid_dim,
                    node.x as u32,
                    node.y as u32,
                    node.z as u32,
                );
                let Some(sample) = terrain_grid.get(node_idx) else {
                    continue;
                };
                if !sample.has_sdf {
                    continue;
                }

                solid_weight += weight * terrain_solid_occupancy_from_sdf(sample.sdf, dx);
            }
        }
    }

    solid_weight.clamp(0.0, 1.0)
}

fn terrain_solid_occupancy_from_sdf(sdf: f32, dx: f32) -> f32 {
    if !sdf.is_finite() || dx <= 0.0 || !dx.is_finite() {
        return 0.0;
    }

    let transition_width = dx * TERRAIN_DENSITY_OCCUPANCY_TRANSITION_CELLS.max(1.0e-3);
    (0.5 - sdf / transition_width).clamp(0.0, 1.0)
}

fn fluid_density_from_eos_pressure(
    stiffness: f32,
    gamma: f32,
    pressure: f32,
    rest_density: f32,
) -> Option<f32> {
    if !stiffness.is_finite()
        || stiffness <= 0.0
        || !gamma.is_finite()
        || gamma <= 0.0
        || !pressure.is_finite()
        || !rest_density.is_finite()
        || rest_density <= 0.0
    {
        return None;
    }

    let density_ratio = (1.0 + pressure.max(0.0) / stiffness).powf(gamma.recip());
    let density = rest_density * density_ratio;
    (density.is_finite() && density > MIN_FLUID_DENSITY).then_some(density)
}

fn fluid_eos_pressure(
    stiffness: f32,
    gamma: f32,
    density: f32,
    rest_density: f32,
    pressure_floor: f32,
) -> f32 {
    if !stiffness.is_finite()
        || stiffness <= 0.0
        || !gamma.is_finite()
        || gamma <= 0.0
        || !density.is_finite()
        || density <= 0.0
        || !rest_density.is_finite()
        || rest_density <= 0.0
        || !pressure_floor.is_finite()
    {
        return 0.0;
    }

    let density_ratio = density / rest_density;
    let pressure = if (gamma - 4.0).abs() <= f32::EPSILON {
        let ratio2 = density_ratio * density_ratio;
        stiffness * (ratio2 * ratio2 - 1.0)
    } else {
        stiffness * (density_ratio.powf(gamma) - 1.0)
    };
    pressure.max(pressure_floor)
}

fn fluid_stress(pressure: f32, dynamic_viscosity: f32, velocity_gradient: Mat3) -> Mat3 {
    let pressure = if pressure.is_finite() { pressure } else { 0.0 };
    let dynamic_viscosity = if dynamic_viscosity.is_finite() {
        dynamic_viscosity.max(0.0)
    } else {
        0.0
    };
    let strain_rate = velocity_gradient + velocity_gradient.transpose();
    Mat3::from_diagonal(Vec3::splat(-pressure)) + strain_rate * dynamic_viscosity
}

fn in_grid(node: IVec3, grid_dim: glam::UVec3) -> bool {
    node.x >= 0
        && node.y >= 0
        && node.z >= 0
        && node.x < grid_dim.x as i32
        && node.y < grid_dim.y as i32
        && node.z < grid_dim.z as i32
}

fn particle_stencil_interior(base: IVec3, grid_dim: glam::UVec3) -> bool {
    base.x >= 0
        && base.y >= 0
        && base.z >= 0
        && base.x < grid_dim.x as i32 - 2
        && base.y < grid_dim.y as i32 - 2
        && base.z < grid_dim.z as i32 - 2
}

fn project_grid_node_collisions(
    node: &mut super::pond::WaterGridNode,
    boundary_flags: u8,
    terrain_sample: WaterTerrainGridSample,
    terrain_collision_margin: f32,
    terrain_tangent_damping_per_sec: f32,
    wall_damping: f32,
    dt: f32,
) {
    let mut normal = Vec3::ZERO;
    // The cached normal band is wider than the actual contact band so G2P can
    // reuse normals near terrain. Grid velocity collision must stay tight;
    // projecting every near-band node makes water hover.
    if terrain_sample.has_sdf
        && terrain_sample.sdf <= terrain_collision_margin
        && terrain_sample.normal.length_squared() > 0.0
    {
        node.v = project_velocity_away_from_surface(node.v, terrain_sample.normal);
        node.v = damp_velocity_tangent_to_surface(
            node.v,
            terrain_sample.normal,
            terrain_tangent_damping_factor(
                terrain_tangent_damping_per_sec,
                terrain_sample.sdf,
                terrain_collision_margin,
                dt,
            ),
        );
        normal += terrain_sample.normal;
    }

    if boundary_flags & WATER_GRID_BOUNDARY_X_MIN != 0 && node.v.x < 0.0 {
        node.v.x *= -wall_damping;
        normal += Vec3::X;
    }
    if boundary_flags & WATER_GRID_BOUNDARY_X_MAX != 0 && node.v.x > 0.0 {
        node.v.x *= -wall_damping;
        normal -= Vec3::X;
    }
    if boundary_flags & WATER_GRID_BOUNDARY_Y_MIN != 0 && node.v.y < 0.0 {
        node.v.y *= -wall_damping;
        normal += Vec3::Y;
    }
    if boundary_flags & WATER_GRID_BOUNDARY_Y_MAX != 0 && node.v.y > 0.0 {
        node.v.y *= -wall_damping;
        normal -= Vec3::Y;
    }
    if boundary_flags & WATER_GRID_BOUNDARY_Z_MIN != 0 && node.v.z < 0.0 {
        node.v.z *= -wall_damping;
        normal += Vec3::Z;
    }
    if boundary_flags & WATER_GRID_BOUNDARY_Z_MAX != 0 && node.v.z > 0.0 {
        node.v.z *= -wall_damping;
        normal -= Vec3::Z;
    }

    if normal.length_squared() > 0.0 {
        node.solid = true;
        node.normal = normal.normalize_or_zero();
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum TerrainGridParticleQuery {
    Skip { sdf: f32 },
    CachedProjection {
        sdf: f32,
        normal: Vec3,
        cached_sdf: f32,
    },
    ExactFallback,
}

fn terrain_grid_particle_query(
    local_pos: Vec3,
    inv_dx: f32,
    dx: f32,
    grid_dim: glam::UVec3,
    terrain_grid: &[WaterTerrainGridSample],
    collision_margin: f32,
) -> TerrainGridParticleQuery {
    if terrain_grid.is_empty()
        || !local_pos.is_finite()
        || inv_dx <= 0.0
        || !inv_dx.is_finite()
        || dx <= 0.0
        || !dx.is_finite()
    {
        return TerrainGridParticleQuery::ExactFallback;
    }

    let grid_pos = local_pos * inv_dx;
    if !grid_pos.is_finite() {
        return TerrainGridParticleQuery::ExactFallback;
    }

    let base = grid_pos.floor().as_ivec3();
    let next = base + IVec3::ONE;
    if !in_grid(base, grid_dim) || !in_grid(next, grid_dim) {
        return TerrainGridParticleQuery::ExactFallback;
    }

    let f = (grid_pos - base.as_vec3()).clamp(Vec3::ZERO, Vec3::ONE);
    let mut corner_sdf = [[[0.0f32; 2]; 2]; 2];
    for oz in 0..=1 {
        for oy in 0..=1 {
            for ox in 0..=1 {
                let node = base + IVec3::new(ox, oy, oz);
                let idx = grid_index_dims(grid_dim, node.x as u32, node.y as u32, node.z as u32);
                let Some(sample) = terrain_grid.get(idx) else {
                    return TerrainGridParticleQuery::ExactFallback;
                };
                if !sample.has_sdf {
                    return TerrainGridParticleQuery::ExactFallback;
                }
                corner_sdf[oz as usize][oy as usize][ox as usize] = sample.sdf;
            }
        }
    }

    let sdf = trilinear_sdf(corner_sdf, f);
    let collision_margin = collision_margin.max(0.0);
    let skip_guard = dx * TERRAIN_GRID_SKIP_GUARD_CELLS;
    if sdf > collision_margin + skip_guard {
        return TerrainGridParticleQuery::Skip { sdf };
    }

    let normal = trilinear_sdf_gradient(corner_sdf, f).normalize_or_zero();
    if normal.is_finite() && normal.length_squared() > 0.0 {
        let projection_sdf = if sdf <= collision_margin {
            sdf
        } else {
            // In the interpolation-uncertainty band just outside the contact
            // margin, apply a small conservative cached correction instead of
            // falling back to the exact collider for every near-surface particle.
            sdf - dx * TERRAIN_GRID_PROJECTION_GUARD_CELLS
        };
        return TerrainGridParticleQuery::CachedProjection {
            sdf: projection_sdf,
            normal,
            cached_sdf: sdf,
        };
    }

    TerrainGridParticleQuery::ExactFallback
}

fn should_shadow_sample_terrain(_particle_idx: usize) -> bool {
    true
}

fn trilinear_sdf(c: [[[f32; 2]; 2]; 2], f: Vec3) -> f32 {
    let x00 = lerp(c[0][0][0], c[0][0][1], f.x);
    let x10 = lerp(c[0][1][0], c[0][1][1], f.x);
    let x01 = lerp(c[1][0][0], c[1][0][1], f.x);
    let x11 = lerp(c[1][1][0], c[1][1][1], f.x);
    let y0 = lerp(x00, x10, f.y);
    let y1 = lerp(x01, x11, f.y);
    lerp(y0, y1, f.z)
}

fn trilinear_sdf_gradient(c: [[[f32; 2]; 2]; 2], f: Vec3) -> Vec3 {
    let dx00 = c[0][0][1] - c[0][0][0];
    let dx10 = c[0][1][1] - c[0][1][0];
    let dx01 = c[1][0][1] - c[1][0][0];
    let dx11 = c[1][1][1] - c[1][1][0];
    let dy00 = c[0][1][0] - c[0][0][0];
    let dy10 = c[0][1][1] - c[0][0][1];
    let dy01 = c[1][1][0] - c[1][0][0];
    let dy11 = c[1][1][1] - c[1][0][1];
    let dz00 = c[1][0][0] - c[0][0][0];
    let dz10 = c[1][0][1] - c[0][0][1];
    let dz01 = c[1][1][0] - c[0][1][0];
    let dz11 = c[1][1][1] - c[0][1][1];

    let grad_x = lerp(lerp(dx00, dx10, f.y), lerp(dx01, dx11, f.y), f.z);
    let grad_y = lerp(lerp(dy00, dy10, f.x), lerp(dy01, dy11, f.x), f.z);
    let grad_z = lerp(lerp(dz00, dz10, f.x), lerp(dz01, dz11, f.x), f.y);
    Vec3::new(grad_x, grad_y, grad_z)
}

#[allow(clippy::too_many_arguments)]
fn project_particle_with_cached_terrain(
    particle: &mut super::pond::WaterParticle,
    sdf: f32,
    terrain_normal: Vec3,
    collision_margin: f32,
    max_correction: f32,
    box_min_ws: Vec3,
    box_max_ws: Vec3,
    box_min_padding: Vec3,
    box_max_padding: Vec3,
) {
    let correction = collision_margin.max(0.0) - sdf;
    if correction <= 0.0 {
        return;
    }

    particle.x += terrain_normal * correction.min(max_correction.max(0.0));
    particle.v = project_velocity_away_from_surface(particle.v, terrain_normal);
    collide_particle_with_box_with_padding(
        particle,
        box_min_ws,
        box_max_ws,
        box_min_padding,
        box_max_padding,
    );
}

fn terrain_grid_cache_range_for_chunk_parts(
    origin_ws: Vec3,
    inv_dx: f32,
    grid_dim: UVec3,
    chunk_id: IVec3,
) -> Option<(UVec3, UVec3)> {
    const HALO_CELLS: i32 = 1;

    if !origin_ws.is_finite() || inv_dx <= 0.0 || !inv_dx.is_finite() {
        return None;
    }

    let min_ws = chunk_id.as_vec3();
    let max_ws = min_ws + Vec3::ONE;
    let min_grid = ((min_ws - origin_ws) * inv_dx).floor().as_ivec3()
        - IVec3::splat(HALO_CELLS);
    let max_grid = ((max_ws - origin_ws) * inv_dx).ceil().as_ivec3()
        + IVec3::splat(HALO_CELLS + 1);
    let grid_dim_i = grid_dim.as_ivec3();
    let min_node = min_grid.max(IVec3::ZERO).min(grid_dim_i);
    let max_node_exclusive = max_grid.max(IVec3::ZERO).min(grid_dim_i);
    if min_node.cmpge(max_node_exclusive).any() {
        return None;
    }
    Some((min_node.as_uvec3(), max_node_exclusive.as_uvec3()))
}

fn grid_index_dims(grid_dim: glam::UVec3, x: u32, y: u32, z: u32) -> usize {
    ((z as usize * grid_dim.y as usize + y as usize) * grid_dim.x as usize) + x as usize
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[cfg(test)]
fn eos_pressure(stiffness: f32, gamma: f32, j: f32, j_min: f32) -> f32 {
    if !stiffness.is_finite()
        || stiffness <= 0.0
        || !gamma.is_finite()
        || gamma <= 0.0
        || !j.is_finite()
    {
        return 0.0;
    }

    // Free-surface weakly-compressible water should resist compression but
    // not generate tensile attraction when a marker's volume estimate is
    // expanded. Negative EOS pressure pulls sparse surface particles into
    // clumps and then lets the pile creep down to j_min; clamp it to zero
    // like a Tait water EOS with no tensile strength.
    let clamped_j = j.max(j_min.max(1.0e-6));
    if clamped_j >= 1.0 {
        return 0.0;
    }

    let compression = if gamma == 7.0 {
        let inv_j = clamped_j.recip();
        let inv_j2 = inv_j * inv_j;
        let inv_j4 = inv_j2 * inv_j2;
        inv_j4 * inv_j2 * inv_j - 1.0
    } else {
        clamped_j.powf(-gamma) - 1.0
    };
    (stiffness * compression).max(0.0)
}

fn grid_density_no_tension_j(
    gathered_mass: f32,
    inv_cell_volume: f32,
    rest_density: f32,
    j_min: f32,
) -> Option<f32> {
    if !gathered_mass.is_finite()
        || gathered_mass <= 0.0
        || !inv_cell_volume.is_finite()
        || inv_cell_volume <= 0.0
        || !rest_density.is_finite()
        || rest_density <= 0.0
    {
        return None;
    }

    let density = gathered_mass * inv_cell_volume;
    if !density.is_finite() || density <= 0.0 {
        return None;
    }

    let density_j = rest_density / density;
    if density_j >= NO_TENSION_MAX_J - DENSITY_J_FEEDBACK_DEADBAND.max(0.0) {
        return Some(NO_TENSION_MAX_J);
    }

    Some(clamp_no_tension_j(density_j, j_min))
}

#[cfg(test)]
fn density_j_feedback_blend(dt: f32) -> f32 {
    if !dt.is_finite() || dt <= 0.0 || DENSITY_J_FEEDBACK_PER_SECOND <= 0.0 {
        return 0.0;
    }

    (1.0 - (-DENSITY_J_FEEDBACK_PER_SECOND * dt).exp()).clamp(0.0, 1.0)
}

#[cfg(test)]
fn blend_no_tension_j(kinematic_j: f32, density_j: f32, blend: f32, j_min: f32) -> f32 {
    let kinematic_j = clamp_no_tension_j(kinematic_j, j_min);
    let density_j = clamp_no_tension_j(density_j, j_min);
    let blend = blend.clamp(0.0, 1.0);
    clamp_no_tension_j(lerp(kinematic_j, density_j, blend), j_min)
}

fn affine_damping_factor(dt: f32) -> f32 {
    if !dt.is_finite() || dt <= 0.0 || APIC_AFFINE_DAMPING_PER_SECOND <= 0.0 {
        return 1.0;
    }

    (-APIC_AFFINE_DAMPING_PER_SECOND * dt)
        .exp()
        .clamp(0.0, 1.0)
}

#[cfg(test)]
fn integrate_no_tension_j(j: f32, trace_c: f32, dt: f32, j_min: f32) -> f32 {
    let j = clamp_no_tension_j(j, j_min);
    if !trace_c.is_finite() || !dt.is_finite() || dt <= 0.0 {
        return j;
    }

    // J is a volume-ratio history variable, while the current EOS has no
    // tensile branch for J > 1.  Let compression/relaxation update J
    // multiplicatively in log-space, cap the per-substep change so a clamped
    // APIC affine cannot launch J to an extreme value in one frame, and keep
    // expanded free-surface markers at the rest volume instead of preserving a
    // permanent no-pressure J > 1 history.
    let log_step = (dt * trace_c).clamp(-MAX_J_LOG_STEP_PER_SUBSTEP, MAX_J_LOG_STEP_PER_SUBSTEP);
    clamp_no_tension_j(j * log_step.exp(), j_min)
}

fn clamp_no_tension_j(j: f32, j_min: f32) -> f32 {
    let min_j = j_min.clamp(1.0e-6, NO_TENSION_MAX_J);
    if !j.is_finite() {
        return NO_TENSION_MAX_J;
    }

    j.clamp(min_j, NO_TENSION_MAX_J)
}

fn damp_velocity_tangent_to_surface(velocity: Vec3, normal: Vec3, tangent_damping: f32) -> Vec3 {
    if !velocity.is_finite() {
        return Vec3::ZERO;
    }
    if !normal.is_finite() {
        return velocity;
    }

    let normal = normal.normalize_or_zero();
    if normal.length_squared() <= f32::EPSILON {
        return velocity;
    }

    let tangent_damping = tangent_damping.clamp(0.0, 1.0);
    let normal_speed = velocity.dot(normal);
    let normal_v = normal * normal_speed;
    let tangent_v = velocity - normal_v;
    normal_v + tangent_v * tangent_damping
}

fn terrain_tangent_damping_factor(
    damping_per_sec: f32,
    terrain_sdf: f32,
    collision_margin: f32,
    dt: f32,
) -> f32 {
    if damping_per_sec <= 0.0
        || !damping_per_sec.is_finite()
        || !terrain_sdf.is_finite()
        || dt <= 0.0
        || !dt.is_finite()
    {
        return 1.0;
    }

    let collision_margin = collision_margin.max(0.0);
    let contact_weight = if collision_margin > f32::EPSILON {
        ((collision_margin - terrain_sdf) / collision_margin).clamp(0.0, 1.0)
    } else if terrain_sdf <= 0.0 {
        1.0
    } else {
        0.0
    };

    if contact_weight <= 0.0 {
        return 1.0;
    }

    (-damping_per_sec * contact_weight * dt)
        .exp()
        .clamp(0.0, 1.0)
}

fn project_velocity_away_from_surface(velocity: Vec3, normal: Vec3) -> Vec3 {
    if !velocity.is_finite() {
        return Vec3::ZERO;
    }
    if !normal.is_finite() {
        return velocity;
    }

    let normal = normal.normalize_or_zero();
    if normal.length_squared() <= f32::EPSILON {
        return velocity;
    }

    let inward_speed = velocity.dot(normal);
    if inward_speed < 0.0 {
        velocity - normal * inward_speed
    } else {
        velocity
    }
}

fn repair_particle_state_with_padding(
    particle: &mut super::pond::WaterParticle,
    min_ws: Vec3,
    max_ws: Vec3,
    min_padding: Vec3,
    max_padding: Vec3,
    max_speed: f32,
    j_min: f32,
) {
    repair_particle_position_velocity_with_padding(
        particle,
        min_ws,
        max_ws,
        min_padding,
        max_padding,
        max_speed,
    );

    if !mat3_is_finite(particle.c) {
        particle.c = Mat3::ZERO;
    }
    particle.c = clamp_mat3_components(particle.c, MAX_AFFINE_COMPONENT);
    particle.j = clamp_no_tension_j(particle.j, j_min);
}

fn repair_particle_state_after_g2p_with_padding(
    particle: &mut super::pond::WaterParticle,
    min_ws: Vec3,
    max_ws: Vec3,
    min_padding: Vec3,
    max_padding: Vec3,
    max_speed: f32,
    j_min: f32,
) {
    let min = min_ws + min_padding;
    let max = max_ws - max_padding;
    if particle.x.is_finite()
        && particle.v.is_finite()
        && mat3_is_finite(particle.c)
        && particle.j.is_finite()
        && particle.j >= j_min
        && particle.j <= NO_TENSION_MAX_J
        && !particle.x.cmplt(min).any()
        && !particle.x.cmpgt(max).any()
    {
        return;
    }

    repair_particle_state_with_padding(
        particle,
        min_ws,
        max_ws,
        min_padding,
        max_padding,
        max_speed,
        j_min,
    );
}

fn repair_particle_position_velocity_with_padding(
    particle: &mut super::pond::WaterParticle,
    min_ws: Vec3,
    max_ws: Vec3,
    min_padding: Vec3,
    max_padding: Vec3,
    max_speed: f32,
) {
    let min = min_ws + min_padding;
    let max = max_ws - max_padding;
    let fallback = (min + max) * 0.5;
    particle.x = finite_or(particle.x, fallback).clamp(min, max);

    if !particle.v.is_finite() {
        particle.v = Vec3::ZERO;
    }
    particle.v = clamp_vec3_length(particle.v, max_speed);
}



fn max_particle_speed_for_substep(dx: f32, dt: f32) -> f32 {
    if dx > 0.0 && dt > 0.0 && dx.is_finite() && dt.is_finite() {
        MAX_PARTICLE_SPEED.min(MAX_PARTICLE_CFL_CELLS_PER_SUBSTEP * dx / dt)
    } else {
        MAX_PARTICLE_SPEED
    }
}

fn velocity_damping_factor(linear_damping_per_sec: f32, dt: f32) -> f32 {
    if linear_damping_per_sec <= 0.0
        || !linear_damping_per_sec.is_finite()
        || dt <= 0.0
        || !dt.is_finite()
    {
        return 1.0;
    }

    (-linear_damping_per_sec * dt).exp().clamp(0.0, 1.0)
}

fn finite_or(value: Vec3, fallback: Vec3) -> Vec3 {
    Vec3::new(
        if value.x.is_finite() {
            value.x
        } else {
            fallback.x
        },
        if value.y.is_finite() {
            value.y
        } else {
            fallback.y
        },
        if value.z.is_finite() {
            value.z
        } else {
            fallback.z
        },
    )
}

fn clamp_vec3_length(value: Vec3, max_length: f32) -> Vec3 {
    let max_length = max_length.max(0.0);
    let length_squared = value.length_squared();
    if length_squared > max_length * max_length {
        value * (max_length / length_squared.sqrt())
    } else {
        value
    }
}

fn clamp_mat3_components(value: Mat3, max_abs_component: f32) -> Mat3 {
    let limit = Vec3::splat(max_abs_component.max(0.0));
    Mat3::from_cols(
        value.x_axis.clamp(-limit, limit),
        value.y_axis.clamp(-limit, limit),
        value.z_axis.clamp(-limit, limit),
    )
}

fn mat3_is_finite(value: Mat3) -> bool {
    value.x_axis.is_finite() && value.y_axis.is_finite() && value.z_axis.is_finite()
}

fn collide_particle_with_box_with_padding(
    particle: &mut super::pond::WaterParticle,
    min_ws: Vec3,
    max_ws: Vec3,
    min_padding: Vec3,
    max_padding: Vec3,
) {
    let min = min_ws + min_padding;
    let max = max_ws - max_padding;

    if particle.x.x < min.x {
        particle.x.x = min.x;
        particle.v.x = particle.v.x.max(0.0);
    } else if particle.x.x > max.x {
        particle.x.x = max.x;
        particle.v.x = particle.v.x.min(0.0);
    }

    if particle.x.y < min.y {
        particle.x.y = min.y;
        particle.v.y = particle.v.y.max(0.0);
    } else if particle.x.y > max.y {
        particle.x.y = max.y;
        particle.v.y = particle.v.y.min(0.0);
    }

    if particle.x.z < min.z {
        particle.x.z = min.z;
        particle.v.z = particle.v.z.max(0.0);
    } else if particle.x.z > max.z {
        particle.x.z = max.z;
        particle.v.z = particle.v.z.min(0.0);
    }
}

fn collide_particle_with_terrain(
    particle: &mut super::pond::WaterParticle,
    terrain: &WaterTerrainColliderSet,
    collision_margin: f32,
    max_correction: f32,
) -> bool {
    let Some(sdf) = terrain.sample_sdf_ws(particle.x) else {
        return false;
    };

    let correction = collision_margin.max(0.0) - sdf;
    if correction <= 0.0 {
        return false;
    }

    let terrain_normal = terrain.sample_normal_ws(particle.x).unwrap_or(Vec3::Y);
    particle.x += terrain_normal * correction.min(max_correction.max(0.0));
    particle.v = project_velocity_away_from_surface(particle.v, terrain_normal);
    true
}

#[allow(clippy::too_many_arguments)]
fn collide_particle_with_terrain_iterative(
    particle: &mut super::pond::WaterParticle,
    terrain: &WaterTerrainColliderSet,
    collision_margin: f32,
    max_correction: f32,
    iterations: usize,
    box_min_ws: Vec3,
    box_max_ws: Vec3,
    box_min_padding: Vec3,
    box_max_padding: Vec3,
) -> bool {
    let mut corrected = false;
    for _ in 0..iterations {
        let before = particle.x;
        if !collide_particle_with_terrain(particle, terrain, collision_margin, max_correction) {
            return corrected;
        }
        corrected = true;
        collide_particle_with_box_with_padding(
            particle,
            box_min_ws,
            box_max_ws,
            box_min_padding,
            box_max_padding,
        );
        if particle.x.distance_squared(before) <= 1.0e-10 {
            return corrected;
        }
    }
    corrected
}

#[derive(Clone, Copy, Debug)]
struct WaterParticleDebugStats {
    finite_particles: usize,
    min_ws: Vec3,
    max_ws: Vec3,
    avg_ws: Vec3,
    avg_speed: f32,
    max_speed: f32,
    speed_limited_particles: usize,
    max_speed_index: usize,
    max_speed_position: Vec3,
    max_speed_velocity: Vec3,
    max_speed_j: f32,
    max_speed_terrain_sdf: Option<f32>,
    min_j: f32,
    max_j: f32,
    j_min_clamped_particles: usize,
    j_max_clamped_particles: usize,
    max_abs_affine: f32,
    min_terrain_sdf: Option<f32>,
    max_terrain_penetration: f32,
    terrain_contact_particles: usize,
    terrain_penetrating: usize,
    no_terrain_sdf: usize,
    floor_pinned_particles: usize,
    ceiling_pinned_particles: usize,
    wall_pinned_particles: usize,
    out_of_bounds_particles: usize,
    non_finite_particles: usize,
}

fn water_particle_debug_stats(
    particles: &[super::pond::WaterParticle],
    terrain: Option<&WaterTerrainColliderSet>,
    bounds: WaterBoxCollider,
    padding: f32,
    speed_limit: f32,
    eos_j_min: Option<f32>,
    terrain_collision_margin: f32,
) -> WaterParticleDebugStats {
    if particles.is_empty() {
        return WaterParticleDebugStats {
            finite_particles: 0,
            min_ws: Vec3::splat(f32::NAN),
            max_ws: Vec3::splat(f32::NAN),
            avg_ws: Vec3::splat(f32::NAN),
            avg_speed: f32::NAN,
            max_speed: f32::NAN,
            speed_limited_particles: 0,
            max_speed_index: 0,
            max_speed_position: Vec3::splat(f32::NAN),
            max_speed_velocity: Vec3::splat(f32::NAN),
            max_speed_j: f32::NAN,
            max_speed_terrain_sdf: None,
            min_j: f32::NAN,
            max_j: f32::NAN,
            j_min_clamped_particles: 0,
            j_max_clamped_particles: 0,
            max_abs_affine: f32::NAN,
            min_terrain_sdf: None,
            max_terrain_penetration: 0.0,
            terrain_contact_particles: 0,
            terrain_penetrating: 0,
            no_terrain_sdf: 0,
            floor_pinned_particles: 0,
            ceiling_pinned_particles: 0,
            wall_pinned_particles: 0,
            out_of_bounds_particles: 0,
            non_finite_particles: 0,
        };
    }

    let padded_min = bounds.min_ws + Vec3::splat(padding);
    let padded_max = bounds.max_ws - Vec3::splat(padding);
    let boundary_epsilon = (padding * 0.1).max(1.0e-4);
    let speed_limit_threshold = speed_limit * 0.98;
    let track_j_min = eos_j_min.is_some();
    let j_min_threshold = eos_j_min.unwrap_or(1.0) * 1.001;
    let j_max_threshold = MAX_J * 0.999;

    let mut finite_particles = 0usize;
    let mut non_finite_particles = 0usize;
    let mut min_ws = Vec3::splat(f32::INFINITY);
    let mut max_ws = Vec3::splat(f32::NEG_INFINITY);
    let mut sum_ws = Vec3::ZERO;
    let mut sum_speed = 0.0f32;
    let mut max_speed = 0.0f32;
    let mut speed_limited_particles = 0usize;
    let mut max_speed_index = 0usize;
    let mut max_speed_position = Vec3::ZERO;
    let mut max_speed_velocity = Vec3::ZERO;
    let mut max_speed_j = 1.0f32;
    let mut max_speed_terrain_sdf = None;
    let mut min_j = if track_j_min { f32::INFINITY } else { 1.0 };
    let mut max_j = if track_j_min { f32::NEG_INFINITY } else { 1.0 };
    let mut j_min_clamped_particles = 0usize;
    let mut j_max_clamped_particles = 0usize;
    let mut max_abs_affine = 0.0f32;
    let mut min_terrain_sdf = f32::INFINITY;
    let mut max_terrain_penetration = 0.0f32;
    let mut terrain_contact_particles = 0usize;
    let mut terrain_penetrating = 0usize;
    let mut no_terrain_sdf = 0usize;
    let mut floor_pinned_particles = 0usize;
    let mut ceiling_pinned_particles = 0usize;
    let mut wall_pinned_particles = 0usize;
    let mut out_of_bounds_particles = 0usize;

    for (particle_idx, particle) in particles.iter().enumerate() {
        if !particle.x.is_finite()
            || !particle.v.is_finite()
            || (track_j_min && !particle.j.is_finite())
            || !mat3_is_finite(particle.c)
        {
            non_finite_particles += 1;
            continue;
        }

        finite_particles += 1;
        min_ws = min_ws.min(particle.x);
        max_ws = max_ws.max(particle.x);
        sum_ws += particle.x;

        if !bounds.contains(particle.x) {
            out_of_bounds_particles += 1;
        }
        if particle.x.y <= padded_min.y + boundary_epsilon {
            floor_pinned_particles += 1;
        }
        if particle.x.y >= padded_max.y - boundary_epsilon {
            ceiling_pinned_particles += 1;
        }
        if particle.x.x <= padded_min.x + boundary_epsilon
            || particle.x.x >= padded_max.x - boundary_epsilon
            || particle.x.z <= padded_min.z + boundary_epsilon
            || particle.x.z >= padded_max.z - boundary_epsilon
        {
            wall_pinned_particles += 1;
        }

        let speed = particle.v.length();
        sum_speed += speed;
        if speed >= speed_limit_threshold {
            speed_limited_particles += 1;
        }

        let terrain_sdf = terrain.and_then(|terrain| terrain.sample_sdf_ws(particle.x));
        if let Some(sdf) = terrain_sdf {
            min_terrain_sdf = min_terrain_sdf.min(sdf);
            if sdf <= terrain_collision_margin {
                terrain_contact_particles += 1;
            }
            if sdf < 0.0 {
                terrain_penetrating += 1;
                max_terrain_penetration = max_terrain_penetration.max(-sdf);
            }
        } else if terrain.is_some() {
            no_terrain_sdf += 1;
        }

        if speed > max_speed {
            max_speed = speed;
            max_speed_index = particle_idx;
            max_speed_position = particle.x;
            max_speed_velocity = particle.v;
            max_speed_j = if track_j_min { particle.j } else { 1.0 };
            max_speed_terrain_sdf = terrain_sdf;
        }

        if track_j_min {
            min_j = min_j.min(particle.j);
            max_j = max_j.max(particle.j);
            if particle.j <= j_min_threshold {
                j_min_clamped_particles += 1;
            }
            if particle.j >= j_max_threshold {
                j_max_clamped_particles += 1;
            }
        }
        max_abs_affine = max_abs_affine
            .max(particle.c.x_axis.abs().max_element())
            .max(particle.c.y_axis.abs().max_element())
            .max(particle.c.z_axis.abs().max_element());
    }

    if finite_particles == 0 {
        return WaterParticleDebugStats {
            finite_particles,
            min_ws: Vec3::splat(f32::NAN),
            max_ws: Vec3::splat(f32::NAN),
            avg_ws: Vec3::splat(f32::NAN),
            avg_speed: f32::NAN,
            max_speed: f32::NAN,
            speed_limited_particles,
            max_speed_index,
            max_speed_position: Vec3::splat(f32::NAN),
            max_speed_velocity: Vec3::splat(f32::NAN),
            max_speed_j: f32::NAN,
            max_speed_terrain_sdf: None,
            min_j: f32::NAN,
            max_j: f32::NAN,
            j_min_clamped_particles,
            j_max_clamped_particles,
            max_abs_affine: f32::NAN,
            min_terrain_sdf: None,
            max_terrain_penetration,
            terrain_contact_particles,
            terrain_penetrating,
            no_terrain_sdf,
            floor_pinned_particles,
            ceiling_pinned_particles,
            wall_pinned_particles,
            out_of_bounds_particles,
            non_finite_particles,
        };
    }

    WaterParticleDebugStats {
        finite_particles,
        min_ws,
        max_ws,
        avg_ws: sum_ws / finite_particles as f32,
        avg_speed: sum_speed / finite_particles as f32,
        max_speed,
        speed_limited_particles,
        max_speed_index,
        max_speed_position,
        max_speed_velocity,
        max_speed_j,
        max_speed_terrain_sdf,
        min_j,
        max_j,
        j_min_clamped_particles,
        j_max_clamped_particles,
        max_abs_affine,
        min_terrain_sdf: min_terrain_sdf.is_finite().then_some(min_terrain_sdf),
        max_terrain_penetration,
        terrain_contact_particles,
        terrain_penetrating,
        no_terrain_sdf,
        floor_pinned_particles,
        ceiling_pinned_particles,
        wall_pinned_particles,
        out_of_bounds_particles,
        non_finite_particles,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        affine_damping_factor, blend_no_tension_j, collide_particle_with_terrain,
        collide_particle_with_terrain_iterative, damp_velocity_tangent_to_surface,
        density_j_feedback_blend, eos_pressure, fluid_eos_pressure, fluid_stress,
        grid_density_no_tension_j, integrate_no_tension_j, project_velocity_away_from_surface,
        terrain_boundary_density_correction, terrain_grid_particle_query,
        terrain_solid_kernel_weight, terrain_solid_occupancy_from_sdf,
        terrain_tangent_damping_factor, TerrainGridParticleQuery, WaterGridNode,
        WaterTerrainGridSample,
        ACTIVE_MASS_EPSILON,
    };
    use crate::{PondWaterConfig, PondWaterSim, WaterTerrainColliderChunk, WaterTerrainColliderSet};
    use glam::{IVec3, Mat3, UVec3, Vec3};
    use std::sync::Arc;

    #[test]
    fn fixed_box_substeps_keep_particles_finite_and_bounded() {
        let mut sim = test_sim_with_particles();
        for _ in 0..120 {
            sim.substep(sim.config.substep_dt);
        }

        assert_particles_finite_and_bounded(&sim);
    }

    #[test]
    fn eos_pressure_has_no_tensile_branch() {
        let compressed = eos_pressure(10_000.0, 7.0, 0.8, 0.55);
        assert!(compressed > 0.0, "compressed={compressed}");
        assert_eq!(eos_pressure(10_000.0, 7.0, 1.0, 0.55), 0.0);
        assert_eq!(eos_pressure(10_000.0, 7.0, 1.2, 0.55), 0.0);
    }

    #[test]
    fn fluid_eos_pressure_uses_density_ratio_and_floor() {
        let compressed = fluid_eos_pressure(10.0, 4.0, 8.0, 4.0, -0.1);
        assert!((compressed - 150.0).abs() <= 1.0e-5, "compressed={compressed}");
        assert_eq!(fluid_eos_pressure(10.0, 4.0, 4.0, 4.0, -0.1), 0.0);
        assert_eq!(fluid_eos_pressure(10.0, 4.0, 2.0, 4.0, -0.1), -0.1);
        assert_eq!(fluid_eos_pressure(10.0, 4.0, 2.0, 4.0, 0.0), 0.0);
    }

    #[test]
    fn terrain_sdf_occupancy_smooths_over_one_cell() {
        assert_eq!(terrain_solid_occupancy_from_sdf(0.6, 1.0), 0.0);
        assert_eq!(terrain_solid_occupancy_from_sdf(0.5, 1.0), 0.0);
        assert!((terrain_solid_occupancy_from_sdf(0.0, 1.0) - 0.5).abs() <= 1.0e-6);
        assert_eq!(terrain_solid_occupancy_from_sdf(-0.5, 1.0), 1.0);
        assert_eq!(terrain_solid_occupancy_from_sdf(-0.6, 1.0), 1.0);
        assert_eq!(terrain_solid_occupancy_from_sdf(0.0, 0.0), 0.0);
    }

    #[test]
    fn terrain_density_correction_skips_when_no_sdf_support_exists() {
        let grid_dim = UVec3::splat(3);
        let terrain_grid = vec![WaterTerrainGridSample::default(); 27];
        let grid = vec![WaterGridNode::default(); 27];
        let weights = [0.25, 0.5, 0.25];
        let corrected = terrain_density_correction_for_test(4.0, &grid, &terrain_grid, grid_dim, weights, 4.0);

        assert_eq!(corrected.density, 4.0);
        assert_eq!(corrected.correction_factor, 1.0);
        assert_eq!(corrected.solid_weight, 0.0);
    }

    #[test]
    fn terrain_density_correction_fills_planar_half_space_support() {
        let grid_dim = UVec3::splat(3);
        let terrain_grid = terrain_grid_from_sdf(grid_dim, |node| node.y as f32 - 1.0);
        let weights = [0.25, 0.5, 0.25];
        let solid_weight = terrain_solid_kernel_weight(
            &terrain_grid,
            grid_dim,
            IVec3::ZERO,
            weights,
            weights,
            weights,
            1.0,
        );
        assert!((solid_weight - 0.5).abs() <= 1.0e-6, "solid_weight={solid_weight}");

        let grid = vec![WaterGridNode::default(); 27];
        let corrected = terrain_density_correction_for_test(2.0, &grid, &terrain_grid, grid_dim, weights, 4.0);

        assert!((corrected.fluid_fraction - 0.5).abs() <= 1.0e-6);
        assert!((corrected.correction_factor - 2.0).abs() <= 1.0e-6);
        assert!((corrected.density - 4.0).abs() <= 1.0e-6);
    }

    #[test]
    fn terrain_density_correction_adds_hydrostatic_ghost_pressure() {
        let grid_dim = UVec3::splat(3);
        let terrain_grid = terrain_grid_from_sdf(grid_dim, |node| node.y as f32 - 1.0);
        let grid = vec![WaterGridNode::default(); 27];
        let weights = [0.25, 0.5, 0.25];
        let no_gravity = terrain_density_correction_for_test_with_gravity(
            4.0,
            &grid,
            &terrain_grid,
            grid_dim,
            weights,
            4.0,
            Vec3::ZERO,
        );
        let with_gravity = terrain_density_correction_for_test_with_gravity(
            4.0,
            &grid,
            &terrain_grid,
            grid_dim,
            weights,
            4.0,
            Vec3::new(0.0, -9.8, 0.0),
        );

        assert!(with_gravity.density > no_gravity.density);
        assert!(with_gravity.correction_factor <= 2.0);
    }

    #[test]
    fn terrain_density_correction_is_bounded_for_deep_solid_overlap() {
        let grid_dim = UVec3::splat(3);
        let terrain_grid = terrain_grid_from_sdf(grid_dim, |_node| -1.0);
        let weights = [0.25, 0.5, 0.25];
        let grid = vec![WaterGridNode::default(); 27];
        let corrected = terrain_density_correction_for_test(4.0, &grid, &terrain_grid, grid_dim, weights, 4.0);

        assert_eq!(corrected.fluid_fraction, 0.5);
        assert_eq!(corrected.correction_factor, 2.0);
        assert_eq!(corrected.density, 8.0);
    }

    #[test]
    fn fluid_stress_combines_pressure_and_viscosity() {
        let velocity_gradient = Mat3::from_cols(
            Vec3::new(1.0, 2.0, 0.0),
            Vec3::new(3.0, 4.0, 0.0),
            Vec3::new(0.0, 0.0, 5.0),
        );
        let stress = fluid_stress(7.0, 0.5, velocity_gradient);

        assert!((stress.x_axis.x - -6.0).abs() <= 1.0e-6);
        assert!((stress.y_axis.y - -3.0).abs() <= 1.0e-6);
        assert!((stress.z_axis.z - -2.0).abs() <= 1.0e-6);
        assert!((stress.x_axis.y - 2.5).abs() <= 1.0e-6);
        assert!((stress.y_axis.x - 2.5).abs() <= 1.0e-6);
    }

    #[test]
    fn fluid_box_prototype_substeps_keep_particles_finite_and_bounded() {
        let mut sim = PondWaterSim::fluid_box_prototype();
        for _ in 0..60 {
            sim.substep(sim.config.substep_dt);
        }

        assert_particles_finite_and_bounded(&sim);
        assert!(sim.grid.iter().all(|node| node.mass >= 0.0 && node.v.is_finite()));
    }

    #[test]
    fn no_tension_j_update_is_bounded_and_does_not_store_expansion() {
        let expanded = integrate_no_tension_j(1.0, 100.0, 1.0 / 60.0, 0.55);
        assert_eq!(expanded, 1.0);

        let relaxed = integrate_no_tension_j(0.8, 100.0, 1.0 / 60.0, 0.55);
        assert!(relaxed > 0.8 && relaxed <= 1.0, "relaxed={relaxed}");

        let compressed = integrate_no_tension_j(1.0, -100.0, 1.0 / 60.0, 0.55);
        assert!(compressed > 0.90 && compressed < 1.0, "compressed={compressed}");

        let clamped = integrate_no_tension_j(0.56, -100.0, 1.0, 0.55);
        assert_eq!(clamped, 0.55);
    }

    #[test]
    fn grid_density_j_estimate_respects_rest_density() {
        let rest_density = 1_000.0;
        let inv_cell_volume = 8.0;
        let rest_gathered_mass = rest_density / inv_cell_volume;

        assert_eq!(
            grid_density_no_tension_j(rest_gathered_mass, inv_cell_volume, rest_density, 0.2),
            Some(1.0)
        );

        let compressed = grid_density_no_tension_j(
            rest_gathered_mass * 2.0,
            inv_cell_volume,
            rest_density,
            0.2,
        )
        .unwrap();
        assert!((compressed - 0.5).abs() <= 1.0e-6, "compressed={compressed}");

        assert_eq!(
            grid_density_no_tension_j(rest_gathered_mass * 1.01, inv_cell_volume, rest_density, 0.2),
            Some(1.0)
        );
        assert_eq!(
            grid_density_no_tension_j(rest_gathered_mass * 0.5, inv_cell_volume, rest_density, 0.2),
            Some(1.0)
        );
        assert_eq!(grid_density_no_tension_j(0.0, inv_cell_volume, rest_density, 0.2), None);
    }

    #[test]
    fn density_j_feedback_blends_toward_grid_density() {
        let blend_120_hz = density_j_feedback_blend(1.0 / 120.0);
        assert!(
            blend_120_hz > 0.09 && blend_120_hz < 0.11,
            "blend_120_hz={blend_120_hz}"
        );
        assert_eq!(density_j_feedback_blend(0.0), 0.0);

        let blended = blend_no_tension_j(1.0, 0.5, 0.1, 0.2);
        assert!((blended - 0.95).abs() <= 1.0e-6, "blended={blended}");
    }

    #[test]
    fn affine_damping_is_mild_per_substep() {
        let damping_120_hz = affine_damping_factor(1.0 / 120.0);
        assert!(
            damping_120_hz > 0.98 && damping_120_hz < 0.99,
            "damping_120_hz={damping_120_hz}"
        );
        assert_eq!(affine_damping_factor(0.0), 1.0);
    }

    #[test]
    fn density_feedback_keeps_settled_puddle_from_collapsing_below_marker_volume() {
        let mut sim = PondWaterSim::new(PondWaterConfig::default().with_particle_count(4_096));
        for _ in 0..240 {
            sim.substep(sim.config.substep_dt);
        }

        let (min_ws, max_ws) = particle_bounds(&sim);
        let height = max_ws.y - min_ws.y;
        let padding = sim.dx * sim.config.wall_padding_cells.max(1.0);
        let usable_x = sim.config.collider.max_ws.x - sim.config.collider.min_ws.x - padding * 2.0;
        let usable_z = sim.config.collider.max_ws.z - sim.config.collider.min_ws.z - padding * 2.0;
        let rest_height = sim.particles.len() as f32 * sim.config.particle_volume / (usable_x * usable_z);

        assert!(
            height >= rest_height * 0.5,
            "settled height {height} lost too much marker volume; rest_height={rest_height} bounds={min_ws:?}..{max_ws:?}"
        );
        assert_particles_finite_and_bounded(&sim);
    }

    #[test]
    fn empty_update_idles_without_accumulating_substeps() {
        let mut sim = PondWaterSim::fixed_test_box();
        assert!(sim.particles.is_empty());

        sim.accumulator = sim.config.substep_dt * 4.0;
        sim.perf_report_seconds = 0.5;
        sim.perf_stats.substeps = 3;
        sim.diagnostic_report_seconds = 0.5;
        sim.diagnostic_stats.substeps = 3;
        sim.last_terrain_contact_particles = 2;

        sim.update(1.0, true);

        assert_eq!(sim.accumulator, 0.0);
        assert_eq!(sim.perf_report_seconds, 0.0);
        assert_eq!(sim.perf_stats.substeps, 0);
        assert_eq!(sim.diagnostic_report_seconds, 0.0);
        assert_eq!(sim.diagnostic_stats.substeps, 0);
        assert_eq!(sim.last_terrain_contact_particles, 0);
        assert_eq!(sim.sim_time_seconds, 0.0);
    }

    #[test]
    fn p2g_tracks_unique_touched_grid_nodes_and_sparse_clear_resets_them() {
        let mut sim = test_sim_with_particles();

        sim.clear_grid();
        sim.particle_to_grid(sim.config.substep_dt);

        let touched_len = sim.touched_grid_nodes.len();
        assert!(touched_len > 0);
        assert!(touched_len < sim.grid.len());

        let mut unique_nodes = sim.touched_grid_nodes.clone();
        unique_nodes.sort_unstable();
        unique_nodes.dedup();
        assert_eq!(unique_nodes.len(), touched_len);

        let active_nodes = sim.update_grid(sim.config.substep_dt);
        assert!(active_nodes > 0);
        assert!(active_nodes <= touched_len);
        assert_eq!(
            active_nodes,
            sim.grid
                .iter()
                .filter(|node| node.mass > ACTIVE_MASS_EPSILON)
                .count()
        );

        sim.clear_grid();

        assert!(sim.touched_grid_nodes.is_empty());
        assert!(sim.grid.iter().all(|node| {
            node.v == Vec3::ZERO
                && node.mass == 0.0
                && !node.solid
                && node.normal == Vec3::ZERO
        }));
    }

    #[test]
    fn update_with_max_substeps_discards_excess_catchup() {
        let mut sim = test_sim_with_particles();
        let substep_dt = sim.config.substep_dt;

        sim.update_with_max_substeps(substep_dt * 10.0, false, 2);

        assert!((sim.sim_time_seconds - substep_dt * 2.0).abs() <= f32::EPSILON);
        assert!(sim.accumulator <= substep_dt * 2.0 + f32::EPSILON);
    }

    #[test]
    fn terrain_collider_substeps_keep_particles_finite_and_bounded() {
        let mut sim = test_sim_with_particles();
        let bounds = sim.config.collider;
        sim.set_terrain_collider_set(sdf_collider_set(
            bounds.min_ws,
            bounds.max_ws,
            UVec3::new(4, 4, 4),
            |p| p.y - 0.2,
        ));

        for _ in 0..120 {
            sim.substep(sim.config.substep_dt);
        }

        assert_particles_finite_and_bounded(&sim);
    }

    #[test]
    fn terrain_normal_projection_removes_inward_velocity() {
        let normal = Vec3::new(-1.0, 1.0, 0.0).normalize();
        let projected = project_velocity_away_from_surface(Vec3::new(0.0, -2.0, 0.0), normal);

        assert!(projected.dot(normal) >= -1.0e-6);
        assert!(
            projected.x < 0.0,
            "expected downhill tangent velocity: {projected:?}"
        );
    }

    #[test]
    fn terrain_tangent_damping_preserves_normal_velocity() {
        let normal = Vec3::new(-1.0, 1.0, 0.0).normalize();
        let tangent = Vec3::new(1.0, 1.0, 0.0).normalize();
        let velocity = normal * 0.5 + tangent * 2.0;

        let damped = damp_velocity_tangent_to_surface(velocity, normal, 0.25);

        assert!((damped.dot(normal) - 0.5).abs() <= 1.0e-6, "damped={damped:?}");
        assert!((damped.dot(tangent) - 0.5).abs() <= 1.0e-6, "damped={damped:?}");
    }

    #[test]
    fn terrain_tangent_damping_fades_across_contact_margin() {
        let dt = 1.0 / 60.0;
        let damping = terrain_tangent_damping_factor(6.0, 0.0, 0.2, dt);
        assert!((damping - (-6.0_f32 * dt).exp()).abs() <= 1.0e-6);
        assert_eq!(terrain_tangent_damping_factor(6.0, 0.2, 0.2, dt), 1.0);
        assert_eq!(terrain_tangent_damping_factor(0.0, 0.0, 0.2, dt), 1.0);
    }

    #[test]
    fn terrain_grid_query_projects_near_outside_cached_surface() {
        let terrain_grid = plane_terrain_grid_samples();
        let query = terrain_grid_particle_query(
            Vec3::new(0.5, 0.52, 0.5),
            1.0,
            1.0,
            UVec3::new(2, 2, 2),
            &terrain_grid,
            0.5,
        );

        match query {
            TerrainGridParticleQuery::CachedProjection { sdf, normal, .. } => {
                assert!(sdf < 0.5, "near-band projection should be conservative: {sdf}");
                assert!(normal.dot(Vec3::Y) > 0.99, "normal={normal:?}");
            }
            other => panic!("expected cached projection, got {other:?}"),
        }
    }

    #[test]
    fn terrain_grid_query_skips_outside_cached_guard_band() {
        let terrain_grid = plane_terrain_grid_samples();
        let query = terrain_grid_particle_query(
            Vec3::new(0.5, 0.80, 0.5),
            1.0,
            1.0,
            UVec3::new(2, 2, 2),
            &terrain_grid,
            0.5,
        );

        assert!(matches!(query, TerrainGridParticleQuery::Skip { .. }));
    }

    #[test]
    fn sloped_terrain_collider_substeps_keep_particles_finite_and_bounded() {
        let mut sim = test_sim_with_particles();
        let bounds = sim.config.collider;
        sim.set_terrain_collider_set(sdf_collider_set(
            bounds.min_ws,
            bounds.max_ws,
            UVec3::new(6, 4, 4),
            |p| {
                let tx = (p.x - bounds.min_ws.x) / (bounds.max_ws.x - bounds.min_ws.x);
                p.y - (0.1 + tx * 0.35)
            },
        ));

        for _ in 0..120 {
            sim.substep(sim.config.substep_dt);
        }

        assert_particles_finite_and_bounded(&sim);
    }

    #[test]
    fn terrain_particle_collision_lifts_particles_above_sdf_floor() {
        let mut sim = test_sim_with_particles();
        let bounds = sim.config.collider;
        let terrain_height = 0.5;
        let terrain_margin = sim.terrain_collision_margin();
        sim.set_terrain_collider_set(sdf_collider_set(
            bounds.min_ws,
            bounds.max_ws,
            UVec3::new(4, 4, 4),
            |p| p.y - terrain_height,
        ));

        for particle in &mut sim.particles {
            particle.x.y = terrain_height - 0.25;
            particle.v.y = -1.0;
        }

        for _ in 0..64 {
            sim.substep(sim.config.substep_dt);
        }

        let min_particle_y = terrain_height + terrain_margin - 1.0e-5;
        for particle in &sim.particles {
            assert!(
                particle.x.y >= min_particle_y,
                "particle under terrain: {:?}, terrain {} margin {}",
                particle.x,
                terrain_height,
                terrain_margin
            );
        }
        assert_particles_finite_and_bounded(&sim);
    }

    #[test]
    fn terrain_collision_above_box_keeps_particles_bounded() {
        let mut sim = test_sim_with_particles();
        let bounds = sim.config.collider;
        sim.set_terrain_collider_set(sdf_collider_set(
            bounds.min_ws,
            bounds.max_ws,
            UVec3::new(4, 4, 4),
            |p| p.y - (bounds.max_ws.y + 0.5),
        ));

        for particle in &mut sim.particles {
            particle.v.y = -1.0;
        }

        for _ in 0..16 {
            sim.substep(sim.config.substep_dt);
        }

        assert_particles_finite_and_bounded(&sim);
    }

    #[test]
    fn sdf_particle_collision_pushes_particles_up_from_floor() {
        let terrain = sdf_collider_set(Vec3::ZERO, Vec3::ONE, UVec3::new(4, 4, 4), |p| p.y - 0.5);
        let mut particle = water_particle(Vec3::new(0.5, 0.25, 0.5), -Vec3::Y);

        collide_particle_with_terrain(&mut particle, &terrain, 0.05, 1.0);

        assert!(particle.x.y >= 0.55 - 1.0e-6, "{:?}", particle.x);
        assert!(particle.v.y >= -1.0e-6, "{:?}", particle.v);
    }

    #[test]
    fn sdf_particle_collision_pushes_particles_down_from_ceiling() {
        let terrain = sdf_collider_set(Vec3::ZERO, Vec3::ONE, UVec3::new(4, 4, 4), |p| 0.6 - p.y);
        let mut particle = water_particle(Vec3::new(0.5, 0.8, 0.5), Vec3::Y);

        collide_particle_with_terrain(&mut particle, &terrain, 0.05, 1.0);

        assert!(particle.x.y <= 0.55 + 1.0e-6, "{:?}", particle.x);
        assert!(particle.v.y <= 1.0e-6, "{:?}", particle.v);
    }

    #[test]
    fn sdf_particle_collision_pushes_particles_out_of_wall() {
        let terrain = sdf_collider_set(Vec3::ZERO, Vec3::ONE, UVec3::new(4, 4, 4), |p| p.x - 0.4);
        let mut particle = water_particle(Vec3::new(0.2, 0.5, 0.5), -Vec3::X);

        collide_particle_with_terrain(&mut particle, &terrain, 0.05, 1.0);

        assert!(particle.x.x >= 0.45 - 1.0e-6, "{:?}", particle.x);
        assert!(particle.v.x >= -1.0e-6, "{:?}", particle.v);
    }

    #[test]
    fn iterative_terrain_collision_recovers_deep_penetration() {
        let terrain = sdf_collider_set(Vec3::ZERO, Vec3::ONE, UVec3::new(8, 8, 8), |p| p.y - 0.5);
        let mut particle = water_particle(Vec3::new(0.5, 0.1, 0.5), -Vec3::Y);

        collide_particle_with_terrain_iterative(
            &mut particle,
            &terrain,
            0.05,
            0.0625,
            8,
            Vec3::ZERO,
            Vec3::ONE,
            Vec3::ZERO,
            Vec3::ZERO,
        );

        assert!(particle.x.y >= 0.55 - 1.0e-5, "{:?}", particle.x);
        assert!(particle.v.y >= -1.0e-6, "{:?}", particle.v);
    }

    fn test_sim_with_particles() -> PondWaterSim {
        PondWaterSim::new(PondWaterConfig::default().with_particle_count(256))
    }

    fn plane_terrain_grid_samples() -> Vec<WaterTerrainGridSample> {
        let mut samples = Vec::new();
        for _z in 0..2 {
            for y in 0..2 {
                for _x in 0..2 {
                    samples.push(WaterTerrainGridSample {
                        sdf: y as f32,
                        normal: Vec3::Y,
                        near_surface: true,
                        has_sdf: true,
                    });
                }
            }
        }
        samples
    }

    fn terrain_density_correction_for_test(
        raw_density: f32,
        grid: &[WaterGridNode],
        terrain_grid: &[WaterTerrainGridSample],
        grid_dim: UVec3,
        weights: [f32; 3],
        rest_density: f32,
    ) -> super::TerrainBoundaryDensitySample {
        terrain_density_correction_for_test_with_gravity(
            raw_density,
            grid,
            terrain_grid,
            grid_dim,
            weights,
            rest_density,
            Vec3::new(0.0, -9.8, 0.0),
        )
    }

    fn terrain_density_correction_for_test_with_gravity(
        raw_density: f32,
        grid: &[WaterGridNode],
        terrain_grid: &[WaterTerrainGridSample],
        grid_dim: UVec3,
        weights: [f32; 3],
        rest_density: f32,
        gravity: Vec3,
    ) -> super::TerrainBoundaryDensitySample {
        terrain_boundary_density_correction(
            raw_density,
            grid,
            terrain_grid,
            grid_dim,
            IVec3::ZERO,
            weights,
            weights,
            weights,
            1.0,
            1.0,
            1.0,
            rest_density,
            16.0,
            4.0,
            -0.1,
            gravity,
        )
    }

    fn terrain_grid_from_sdf(
        grid_dim: UVec3,
        sdf: impl Fn(IVec3) -> f32,
    ) -> Vec<WaterTerrainGridSample> {
        let mut samples = Vec::new();
        for z in 0..grid_dim.z as i32 {
            for y in 0..grid_dim.y as i32 {
                for x in 0..grid_dim.x as i32 {
                    let sdf = sdf(IVec3::new(x, y, z));
                    samples.push(WaterTerrainGridSample {
                        sdf,
                        normal: Vec3::Y,
                        near_surface: true,
                        has_sdf: true,
                    });
                }
            }
        }
        samples
    }

    fn sdf_collider_set(
        bounds_min_ws: Vec3,
        bounds_max_ws: Vec3,
        dim: UVec3,
        sdf: impl Fn(Vec3) -> f32,
    ) -> WaterTerrainColliderSet {
        let min_chunk = bounds_min_ws.floor().as_ivec3();
        let max_chunk_exclusive = bounds_max_ws.floor().as_ivec3();
        assert_eq!(bounds_min_ws, min_chunk.as_vec3());
        assert_eq!(bounds_max_ws, max_chunk_exclusive.as_vec3());
        assert!(max_chunk_exclusive.cmpgt(min_chunk).all());

        let mut set = WaterTerrainColliderSet::new();
        for z in min_chunk.z..max_chunk_exclusive.z {
            for y in min_chunk.y..max_chunk_exclusive.y {
                for x in min_chunk.x..max_chunk_exclusive.x {
                    let chunk_id = IVec3::new(x, y, z);
                    let chunk_min_ws = chunk_id.as_vec3();
                    let chunk_max_ws = chunk_min_ws + Vec3::ONE;
                    let mut sdf_ws = Vec::new();
                    for sample_z in 0..dim.z {
                        let tz = sample_z as f32 / (dim.z - 1) as f32;
                        for sample_y in 0..dim.y {
                            let ty = sample_y as f32 / (dim.y - 1) as f32;
                            for sample_x in 0..dim.x {
                                let tx = sample_x as f32 / (dim.x - 1) as f32;
                                let p = chunk_min_ws
                                    + (chunk_max_ws - chunk_min_ws) * Vec3::new(tx, ty, tz);
                                sdf_ws.push(sdf(p));
                            }
                        }
                    }

                    set.insert_chunk(Arc::new(WaterTerrainColliderChunk {
                        chunk_id,
                        dim,
                        sdf_ws,
                        revision: 0,
                    }));
                }
            }
        }
        set
    }

    fn water_particle(x: Vec3, v: Vec3) -> crate::pond::WaterParticle {
        crate::pond::WaterParticle {
            x,
            v,
            c: Mat3::ZERO,
            j: 1.0,
        }
    }

    fn particle_bounds(sim: &PondWaterSim) -> (Vec3, Vec3) {
        let mut min_ws = Vec3::splat(f32::INFINITY);
        let mut max_ws = Vec3::splat(f32::NEG_INFINITY);
        for particle in &sim.particles {
            min_ws = min_ws.min(particle.x);
            max_ws = max_ws.max(particle.x);
        }
        (min_ws, max_ws)
    }

    fn assert_particles_finite_and_bounded(sim: &PondWaterSim) {
        for particle in &sim.particles {
            assert!(particle.x.is_finite());
            assert!(particle.v.is_finite());
            assert!(particle.j.is_finite());
            assert!(
                sim.config.collider.contains(particle.x),
                "particle escaped: {:?}",
                particle.x
            );
        }
    }
}
