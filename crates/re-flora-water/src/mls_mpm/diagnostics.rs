use glam::Vec3;
use std::time::Instant;

use super::{
    density::MAX_J,
    repair::{mat3_is_finite, max_particle_speed_for_substep},
    transfer::{
        should_shadow_sample_terrain, terrain_grid_particle_query, TerrainGridParticleQuery,
        TerrainShadowSampleStats, WaterG2pBreakdown,
    },
};
use crate::{
    collider::{WaterBoxCollider, WaterTerrainColliderSet},
    pond::{PondWaterSim, WaterParticle},
};

// Quiet puddles can retain low-energy numerical circulation indefinitely. Apply
// extra damping only after the whole water body is already slow; fast
// falling/splashing water bypasses this path so it does not turn the material
// into honey.
const QUIET_SETTLING_AVG_SPEED_THRESHOLD: f32 = 0.08;
const QUIET_SETTLING_MAX_SPEED_THRESHOLD: f32 = 0.40;
const QUIET_SETTLING_LOCAL_SPEED_THRESHOLD: f32 = 0.35;

#[derive(Clone, Copy, Debug)]
pub(super) struct QuietMotionSample {
    pub(super) avg_speed: f32,
    pub(super) max_speed: f32,
}

pub(super) fn quiet_motion_sample(particles: &[WaterParticle]) -> Option<QuietMotionSample> {
    let mut finite_particles = 0usize;
    let mut sum_speed = 0.0f32;
    let mut max_speed = 0.0f32;

    for particle in particles {
        if !particle.v.is_finite() {
            continue;
        }

        finite_particles += 1;
        let speed = particle.v.length();
        sum_speed += speed;
        max_speed = max_speed.max(speed);
    }

    if finite_particles == 0 {
        return None;
    }

    Some(QuietMotionSample {
        avg_speed: sum_speed / finite_particles as f32,
        max_speed,
    })
}

pub(super) fn quiet_settling_damping_weight(
    avg_speed: f32,
    max_speed: f32,
    velocity_damping_per_sec: f32,
    affine_damping_per_sec: f32,
) -> f32 {
    if !avg_speed.is_finite()
        || !max_speed.is_finite()
        || (velocity_damping_per_sec <= 0.0 && affine_damping_per_sec <= 0.0)
    {
        return 0.0;
    }

    let avg_weight = 1.0
        - smoothstep(
            QUIET_SETTLING_AVG_SPEED_THRESHOLD,
            QUIET_SETTLING_AVG_SPEED_THRESHOLD * 2.0,
            avg_speed,
        );
    let max_weight = 1.0
        - smoothstep(
            QUIET_SETTLING_MAX_SPEED_THRESHOLD,
            QUIET_SETTLING_MAX_SPEED_THRESHOLD * 2.0,
            max_speed,
        );
    avg_weight.min(max_weight).clamp(0.0, 1.0)
}

pub(super) fn quiet_settling_local_velocity_weight(speed: f32) -> f32 {
    if !speed.is_finite() {
        return 0.0;
    }

    (1.0
        - smoothstep(
            QUIET_SETTLING_LOCAL_SPEED_THRESHOLD,
            QUIET_SETTLING_LOCAL_SPEED_THRESHOLD * 2.0,
            speed,
        ))
    .clamp(0.0, 1.0)
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    if !edge0.is_finite() || !edge1.is_finite() || edge1 <= edge0 {
        return if value >= edge1 { 1.0 } else { 0.0 };
    }

    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}


impl PondWaterSim {
    pub(super) fn record_diagnostic_substep(
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

    pub(super) fn log_diagnostics_after_update(&mut self, frame_dt: f32, ran_substeps: usize) {
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

    pub(super) fn log_perf_report(&mut self) {
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
}

#[derive(Clone, Copy, Debug)]
pub(super) struct WaterParticleDebugStats {
    pub(super) finite_particles: usize,
    pub(super) min_ws: Vec3,
    pub(super) max_ws: Vec3,
    pub(super) avg_ws: Vec3,
    pub(super) avg_speed: f32,
    pub(super) max_speed: f32,
    pub(super) speed_limited_particles: usize,
    pub(super) max_speed_index: usize,
    pub(super) max_speed_position: Vec3,
    pub(super) max_speed_velocity: Vec3,
    pub(super) max_speed_j: f32,
    pub(super) max_speed_terrain_sdf: Option<f32>,
    pub(super) min_j: f32,
    pub(super) max_j: f32,
    pub(super) j_min_clamped_particles: usize,
    pub(super) j_max_clamped_particles: usize,
    pub(super) max_abs_affine: f32,
    pub(super) min_terrain_sdf: Option<f32>,
    pub(super) max_terrain_penetration: f32,
    pub(super) terrain_contact_particles: usize,
    pub(super) terrain_penetrating: usize,
    pub(super) no_terrain_sdf: usize,
    pub(super) floor_pinned_particles: usize,
    pub(super) ceiling_pinned_particles: usize,
    pub(super) wall_pinned_particles: usize,
    pub(super) out_of_bounds_particles: usize,
    pub(super) non_finite_particles: usize,
}

pub(super) fn water_particle_debug_stats(
    particles: &[WaterParticle],
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
