use std::time::Instant;

use super::MAX_SUBSTEPS_PER_UPDATE;
use crate::pond::PondWaterSim;

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
            self.apply_quiet_settling_damping(dt);
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
        self.apply_quiet_settling_damping(dt);

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
}

