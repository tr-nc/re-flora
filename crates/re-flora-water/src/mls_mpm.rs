use glam::{IVec3, Mat3, UVec3, Vec3};
use std::{collections::HashMap, time::Instant};

use super::{
    collider::{WaterBoxCollider, WaterTerrainColliderSet},
    pond::{
        PondWaterSim, WaterTerrainGridSample, WATER_GRID_BOUNDARY_X_MAX,
        WATER_GRID_BOUNDARY_X_MIN, WATER_GRID_BOUNDARY_Y_MAX, WATER_GRID_BOUNDARY_Y_MIN,
        WATER_GRID_BOUNDARY_Z_MAX, WATER_GRID_BOUNDARY_Z_MIN,
    },
};

const MAX_SUBSTEPS_PER_UPDATE: usize = 8;
const ACTIVE_MASS_EPSILON: f32 = 1.0e-8;
const MAX_J: f32 = 8.0;
const MAX_PARTICLE_SPEED: f32 = 20.0;
const MAX_PARTICLE_CFL_CELLS_PER_SUBSTEP: f32 = 0.5;
const MAX_AFFINE_COMPONENT: f32 = 100.0;
// Incompressible projection removes volumetric compression on the grid. Keeping
// the full APIC affine term after that makes sparse/free-surface particles ring
// like springs, so incompressible mode uses a more PIC-like transfer and only
// carries a small traceless affine component for local swirl.
const INCOMPRESSIBLE_APIC_BLEND: f32 = 0.10;
const MAX_INCOMPRESSIBLE_AFFINE_COMPONENT: f32 = 20.0;
// Particles are marker samples, but rendering them directly makes marker
// clumping visible. This local spacing projection gives each sample a small
// excluded volume so incompressible water settles into a puddle instead of a
// single visual point.
const INCOMPRESSIBLE_PARTICLE_SPACING_SCALE: f32 = 0.80;
const INCOMPRESSIBLE_PARTICLE_SPACING_STRENGTH: f32 = 0.45;
const MAX_PARTICLE_SPACING_CORRECTION_CELLS: f32 = 0.35;
// A particle can end a substep deeper than one capped correction can resolve.
// Iterate bounded SDF corrections so the next P2G pass does not deposit mass
// from inside terrain.
const TERRAIN_PARTICLE_COLLISION_ITERATIONS: usize = 8;

impl PondWaterSim {
    /// Advance the pond by fixed MLS-MPM substeps.
    pub fn update(&mut self, dt: f32, perf_logging: bool) {
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

        self.accumulator += dt.min(0.25);
        let substep_dt = self.config.substep_dt;
        let mut ran_substeps = 0usize;
        for _ in 0..MAX_SUBSTEPS_PER_UPDATE {
            if self.accumulator < substep_dt {
                break;
            }
            self.substep_timed(substep_dt, perf_logging);
            self.accumulator -= substep_dt;
            ran_substeps += 1;
        }

        // Avoid a long catch-up spiral if a frame stalls while the sim is enabled.
        let max_remainder = substep_dt * MAX_SUBSTEPS_PER_UPDATE as f32;
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
            // G2P already resolves terrain every substep; the pre-P2G repair pass
            // only needs finite/box/J/speed cleanup in steady state.
            self.repair_particles(dt, false);
            self.clear_grid();
            self.particle_to_grid(dt);
            let active_nodes = self.update_grid(dt);
            self.project_grid_incompressible(dt);
            let g2p_breakdown = self.grid_to_particle(dt);
            self.relax_incompressible_particle_spacing(dt);
            self.record_diagnostic_substep(active_nodes, g2p_breakdown);
            return;
        }

        let total_start = Instant::now();
        let repair_start = Instant::now();
        // G2P already resolves terrain every substep; the pre-P2G repair pass
        // only needs finite/box/J/speed cleanup in steady state.
        self.repair_particles(dt, false);
        let repair_seconds = repair_start.elapsed().as_secs_f64();

        let clear_start = Instant::now();
        self.clear_grid();
        let clear_seconds = clear_start.elapsed().as_secs_f64();

        let p2g_start = Instant::now();
        self.particle_to_grid(dt);
        let p2g_seconds = p2g_start.elapsed().as_secs_f64();

        let grid_start = Instant::now();
        let active_nodes = self.update_grid(dt);
        self.project_grid_incompressible(dt);
        let grid_seconds = grid_start.elapsed().as_secs_f64();

        let g2p_breakdown = self.grid_to_particle_timed(dt);
        self.relax_incompressible_particle_spacing(dt);
        self.record_diagnostic_substep(active_nodes, g2p_breakdown);

        self.perf_stats.substeps += 1;
        self.perf_stats.repair_seconds += repair_seconds;
        self.perf_stats.clear_seconds += clear_seconds;
        self.perf_stats.p2g_seconds += p2g_seconds;
        self.perf_stats.grid_seconds += grid_seconds;
        self.perf_stats.g2p_seconds += g2p_breakdown.total_seconds;
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
                self.config.j_min,
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
            self.config.j_min,
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

        let message = format!(
            "[WATER][DIAG] t={:.3}s frame_dt={:.4}s ran_substeps={} diag_substeps={} particles={} finite={} pos_x={:.3}..{:.3} pos_y={:.3}..{:.3} avg_y={:.3} pos_z={:.3}..{:.3} speed_avg={:.3} speed_max={:.3}/{:.3} speed_limited={} j={:.3}..{:.3} j_min_clamped={} j_max_clamped={} affine_max={:.2} terrain_contact={} terrain_penetrating={} terrain_no_sdf={} terrain_sdf_min={:.5} terrain_penetration_max={:.5} g2p_cache_proj/substep={:.1} g2p_exact_checks/substep={:.1} g2p_exact_corr/substep={:.1} active_nodes/substep={:.0} floor_pinned={} ceil_pinned={} wall_pinned={} out_of_bounds={} non_finite={} fastest_idx={} fastest_pos={:?} fastest_v={:?} fastest_j={:.3} fastest_sdf={:.5}",
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

        let shadow_stats = self.measure_terrain_shadow_samples();
        stats.g2p_terrain_shadow_samples += shadow_stats.samples;
        stats.g2p_terrain_shadow_false_skips += shadow_stats.false_skips;
        stats.g2p_terrain_shadow_sdf_abs_error_sum += shadow_stats.sdf_abs_error_sum;
        stats.g2p_terrain_shadow_sdf_abs_error_max = stats
            .g2p_terrain_shadow_sdf_abs_error_max
            .max(shadow_stats.sdf_abs_error_max);

        let substeps = stats.substeps as f64;
        let grid_nodes = self.grid.len();
        let particle_padding = self.dx * self.config.wall_padding_cells.max(1.0);
        let max_particle_speed = max_particle_speed_for_substep(self.dx, self.config.substep_dt);
        let particle_stats = water_particle_debug_stats(
            &self.particles,
            self.terrain.as_ref(),
            self.config.collider,
            particle_padding,
            max_particle_speed,
            self.config.j_min,
            self.terrain_collision_margin(),
        );
        log::info!(
            "[PERF][WATER] particles {} grid {:?} nodes {} substeps {} total {:.2}ms avg {:.3}ms/substep repair {:.2}ms clear {:.2}ms p2g {:.2}ms grid {:.2}ms g2p {:.2}ms g2p_gather {:.2}ms g2p_box {:.2}ms g2p_terrain {:.2}ms g2p_repair {:.2}ms terrain_cache_skips/substep {:.0} terrain_cache_projections/substep {:.0} terrain_exact_fallbacks/substep {:.0} terrain_exact_checks/substep {:.0} terrain_exact_corrections/substep {:.0} terrain_shadow_samples/substep {:.1} terrain_shadow_false_skips {} terrain_shadow_sdf_err_avg {:.5} terrain_shadow_sdf_err_max {:.5} active_nodes/substep {:.0} particle_y {:.3}..{:.3} avg {:.3} terrain_sdf_min {:.4} penetrating {} no_sdf {}",
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
            stats.g2p_seconds * 1000.0,
            stats.g2p_gather_seconds * 1000.0,
            stats.g2p_box_seconds * 1000.0,
            stats.g2p_terrain_seconds * 1000.0,
            stats.g2p_repair_seconds * 1000.0,
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
                TerrainGridParticleQuery::CachedProjection { sdf, .. } => {
                    stats.samples += 1;
                    if let Some(exact_sdf) = terrain.sample_sdf_ws(particle.x) {
                        let abs_error = (sdf - exact_sdf).abs();
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
        const HALO_CELLS: i32 = 1;

        let min_ws = chunk_id.as_vec3();
        let max_ws = min_ws + Vec3::ONE;
        let min_grid = ((min_ws - self.origin_ws) * self.inv_dx).floor().as_ivec3()
            - IVec3::splat(HALO_CELLS);
        let max_grid = ((max_ws - self.origin_ws) * self.inv_dx).ceil().as_ivec3()
            + IVec3::splat(HALO_CELLS + 1);
        let grid_dim = self.grid_dim.as_ivec3();
        let min_node = min_grid.max(IVec3::ZERO).min(grid_dim);
        let max_node_exclusive = max_grid.max(IVec3::ZERO).min(grid_dim);
        if min_node.cmpge(max_node_exclusive).any() {
            return None;
        }
        Some((min_node.as_uvec3(), max_node_exclusive.as_uvec3()))
    }

    fn rebuild_terrain_grid_cache_range(
        &mut self,
        min_node: UVec3,
        max_node_exclusive: UVec3,
        near_surface_band: f32,
    ) -> TerrainGridCacheRebuildStats {
        let terrain = self.terrain.as_ref();
        let origin_ws = self.origin_ws;
        let grid_dim = self.grid_dim;
        let dx = self.dx;
        let mut stats = TerrainGridCacheRebuildStats::default();

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

    fn repair_particles(&mut self, dt: f32, repair_terrain: bool) {
        let bounds = self.config.collider;
        let padding = self.dx * self.config.wall_padding_cells.max(1.0);
        let min_padding = Vec3::splat(padding);
        let max_padding = Vec3::splat(padding);
        let terrain_collision_margin = self.terrain_collision_margin();
        let terrain_max_correction = padding;
        let j_min = self.config.j_min;
        let max_particle_speed = max_particle_speed_for_substep(self.dx, dt);
        let terrain = repair_terrain.then_some(()).and(self.terrain.as_ref());
        for particle in &mut self.particles {
            repair_particle_state_with_padding(
                particle,
                bounds.min_ws,
                bounds.max_ws,
                min_padding,
                max_padding,
                j_min,
                max_particle_speed,
            );
            if let Some(terrain) = terrain {
                collide_particle_with_terrain_iterative(
                    particle,
                    terrain,
                    terrain_collision_margin,
                    terrain_max_correction,
                    TERRAIN_PARTICLE_COLLISION_ITERATIONS,
                    bounds.min_ws,
                    bounds.max_ws,
                    min_padding,
                    max_padding,
                );
            }
        }
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
        let origin_ws = self.origin_ws;
        let grid_dim = self.grid_dim;
        let dx = self.dx;
        let inv_dx = self.inv_dx;
        let mass = self.config.particle_mass;
        let volume = self.config.particle_volume;
        let stiffness = self.config.stiffness;
        let gamma = self.config.gamma;
        let d_inv = 4.0 * inv_dx * inv_dx;

        for particle in &self.particles {
            let local_pos = particle.x - origin_ws;
            let grid_pos = local_pos * inv_dx;
            let base = base_coord(grid_pos);
            let fx = grid_pos - base.as_vec3();
            let weights = quadratic_weights(fx);

            let pressure_scale = if self.config.pressure_projection_iterations == 0 {
                let pressure = stiffness * (particle.j.max(self.config.j_min).powf(-gamma) - 1.0);
                dt * volume * particle.j * pressure * d_inv
            } else {
                0.0
            };
            let affine = Mat3::from_diagonal(Vec3::splat(pressure_scale)) + particle.c * mass;
            let momentum = particle.v * mass;

            for oz in 0..3 {
                for oy in 0..3 {
                    for ox in 0..3 {
                        let node = base + IVec3::new(ox, oy, oz);
                        if !in_grid(node, grid_dim) {
                            continue;
                        }

                        let weight = weights[ox as usize].x
                            * weights[oy as usize].y
                            * weights[oz as usize].z;
                        if weight <= 0.0 {
                            continue;
                        }

                        let node_local = node.as_vec3() * dx;
                        let dpos = node_local - local_pos;
                        let node_idx =
                            grid_index_dims(grid_dim, node.x as u32, node.y as u32, node.z as u32);
                        let grid_node = &mut self.grid[node_idx];
                        if grid_node.mass <= 0.0 {
                            self.touched_grid_nodes.push(node_idx);
                        }
                        grid_node.mass += weight * mass;
                        grid_node.v += weight * (momentum + affine * dpos);
                    }
                }
            }
        }
    }

    fn update_grid(&mut self, dt: f32) -> usize {
        let gravity = self.config.gravity;
        let linear_damping = velocity_damping_factor(self.config.linear_damping_per_sec, dt);
        let terrain_collision_margin = self.terrain_collision_margin();
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
                wall_damping,
            );
        }

        active_nodes
    }

    fn project_grid_incompressible(&mut self, dt: f32) {
        let iterations = self.config.pressure_projection_iterations as usize;
        if iterations == 0 || dt <= 0.0 || !dt.is_finite() || self.grid.is_empty() {
            return;
        }

        self.ensure_pressure_projection_buffers_len();
        self.projection_active_nodes.clear();

        let terrain_collision_margin = self.terrain_collision_margin();
        for &idx in &self.touched_grid_nodes {
            if self.grid[idx].mass <= ACTIVE_MASS_EPSILON {
                continue;
            }
            if pressure_projection_solid_node(
                idx,
                &self.grid_boundary_flags,
                &self.terrain_grid,
                terrain_collision_margin,
            ) {
                continue;
            }

            self.projection_active_nodes.push(idx);
            self.projection_pressure[idx] = 0.0;
            self.projection_pressure_next[idx] = 0.0;
            self.projection_divergence[idx] = 0.0;
        }

        if self.projection_active_nodes.is_empty() {
            return;
        }

        let grid_dim = self.grid_dim;
        let inv_dx = self.inv_dx;
        for &idx in &self.projection_active_nodes {
            let coord = grid_coord_dims(grid_dim, idx);
            let divergence = pressure_projection_divergence_at(
                coord,
                grid_dim,
                &self.grid,
                &self.grid_boundary_flags,
                &self.terrain_grid,
                terrain_collision_margin,
                inv_dx,
            );
            self.projection_divergence[idx] = divergence / dt;
        }

        let dx2 = self.dx * self.dx;
        for _ in 0..iterations {
            let pressure = &self.projection_pressure;
            let pressure_next = &mut self.projection_pressure_next;
            let divergence = &self.projection_divergence;
            let grid = &self.grid;
            let grid_boundary_flags = &self.grid_boundary_flags;
            let terrain_grid = &self.terrain_grid;
            for &idx in &self.projection_active_nodes {
                let coord = grid_coord_dims(grid_dim, idx);
                let mut pressure_sum = 0.0;
                let mut diagonal = 0.0;
                for axis in 0..3 {
                    for direction in [-1, 1] {
                        let Some(neighbor_idx) = pressure_projection_neighbor_index(
                            coord,
                            axis,
                            direction,
                            grid_dim,
                        ) else {
                            continue;
                        };
                        if pressure_projection_solid_node(
                            neighbor_idx,
                            grid_boundary_flags,
                            terrain_grid,
                            terrain_collision_margin,
                        ) {
                            continue;
                        }

                        diagonal += 1.0;
                        if pressure_projection_fluid_node(
                            neighbor_idx,
                            grid,
                            grid_boundary_flags,
                            terrain_grid,
                            terrain_collision_margin,
                        ) {
                            pressure_sum += pressure[neighbor_idx];
                        }
                    }
                }

                pressure_next[idx] = if diagonal > 0.0 {
                    (pressure_sum - divergence[idx] * dx2) / diagonal
                } else {
                    0.0
                };
            }

            for &idx in &self.projection_active_nodes {
                self.projection_pressure[idx] = self.projection_pressure_next[idx];
            }
        }

        let speed_limit = max_particle_speed_for_substep(self.dx, dt);
        let pressure = &self.projection_pressure;
        let grid = &mut self.grid;
        for &idx in &self.projection_active_nodes {
            let coord = grid_coord_dims(grid_dim, idx);
            let center_pressure = pressure[idx];
            let pxm = pressure_projection_neighbor_pressure(
                coord,
                0,
                -1,
                center_pressure,
                grid_dim,
                pressure,
                grid,
                &self.grid_boundary_flags,
                &self.terrain_grid,
                terrain_collision_margin,
            );
            let pxp = pressure_projection_neighbor_pressure(
                coord,
                0,
                1,
                center_pressure,
                grid_dim,
                pressure,
                grid,
                &self.grid_boundary_flags,
                &self.terrain_grid,
                terrain_collision_margin,
            );
            let pym = pressure_projection_neighbor_pressure(
                coord,
                1,
                -1,
                center_pressure,
                grid_dim,
                pressure,
                grid,
                &self.grid_boundary_flags,
                &self.terrain_grid,
                terrain_collision_margin,
            );
            let pyp = pressure_projection_neighbor_pressure(
                coord,
                1,
                1,
                center_pressure,
                grid_dim,
                pressure,
                grid,
                &self.grid_boundary_flags,
                &self.terrain_grid,
                terrain_collision_margin,
            );
            let pzm = pressure_projection_neighbor_pressure(
                coord,
                2,
                -1,
                center_pressure,
                grid_dim,
                pressure,
                grid,
                &self.grid_boundary_flags,
                &self.terrain_grid,
                terrain_collision_margin,
            );
            let pzp = pressure_projection_neighbor_pressure(
                coord,
                2,
                1,
                center_pressure,
                grid_dim,
                pressure,
                grid,
                &self.grid_boundary_flags,
                &self.terrain_grid,
                terrain_collision_margin,
            );
            let grad = Vec3::new(pxp - pxm, pyp - pym, pzp - pzm) * (0.5 * inv_dx);
            let node = &mut grid[idx];
            node.v = clamp_vec3_length(node.v - grad * dt, speed_limit);
        }

        self.project_grid_collision_boundaries_for_active_nodes();
    }

    fn ensure_pressure_projection_buffers_len(&mut self) {
        let grid_len = self.grid.len();
        self.projection_pressure.resize(grid_len, 0.0);
        self.projection_pressure_next.resize(grid_len, 0.0);
        self.projection_divergence.resize(grid_len, 0.0);
    }

    fn project_grid_collision_boundaries_for_active_nodes(&mut self) {
        let terrain_collision_margin = self.terrain_collision_margin();
        let wall_damping = self.config.wall_damping.clamp(0.0, 1.0);
        for &idx in &self.projection_active_nodes {
            let node = &mut self.grid[idx];
            project_grid_node_collisions(
                node,
                self.grid_boundary_flags[idx],
                self.terrain_grid[idx],
                terrain_collision_margin,
                wall_damping,
            );
        }
    }

    fn relax_incompressible_particle_spacing(&mut self, dt: f32) {
        let iterations = self.config.particle_spacing_relaxation_iterations as usize;
        if self.config.pressure_projection_iterations == 0
            || iterations == 0
            || self.particles.len() < 2
            || dt <= 0.0
            || !dt.is_finite()
        {
            return;
        }

        let rest_distance = (self.config.particle_volume.max(1.0e-8)).cbrt()
            * INCOMPRESSIBLE_PARTICLE_SPACING_SCALE;
        if rest_distance <= 0.0 || !rest_distance.is_finite() {
            return;
        }

        let count = self.particles.len();
        let cell_size = rest_distance.max(self.dx * 0.5);
        let inv_cell_size = cell_size.recip();
        let min_distance_sq = rest_distance * rest_distance;
        let max_correction = self.dx * MAX_PARTICLE_SPACING_CORRECTION_CELLS;
        let padding = self.dx * self.config.wall_padding_cells.max(1.0);
        let bounds = self.config.collider;
        let particle_min_padding = Vec3::splat(padding);
        let particle_max_padding = Vec3::splat(padding);
        let terrain_collision_margin = self.terrain_collision_margin();
        let terrain_max_correction = padding;
        let terrain = self.terrain.as_ref();
        let max_particle_speed = max_particle_speed_for_substep(self.dx, dt);

        for _ in 0..iterations {
            let mut bins: HashMap<(i32, i32, i32), Vec<usize>> =
                HashMap::with_capacity(count.saturating_mul(2));
            for (idx, particle) in self.particles.iter().enumerate() {
                if !particle.x.is_finite() {
                    continue;
                }
                bins.entry(particle_spacing_cell(particle.x, inv_cell_size))
                    .or_default()
                    .push(idx);
            }

            let mut corrections = vec![Vec3::ZERO; count];
            for i in 0..count {
                let xi = self.particles[i].x;
                if !xi.is_finite() {
                    continue;
                }
                let (cx, cy, cz) = particle_spacing_cell(xi, inv_cell_size);
                for oz in -1..=1 {
                    for oy in -1..=1 {
                        for ox in -1..=1 {
                            let Some(neighbors) = bins.get(&(cx + ox, cy + oy, cz + oz)) else {
                                continue;
                            };
                            for &j in neighbors {
                                if j <= i {
                                    continue;
                                }

                                let xj = self.particles[j].x;
                                if !xj.is_finite() {
                                    continue;
                                }
                                let delta = xi - xj;
                                let distance_sq = delta.length_squared();
                                if !distance_sq.is_finite() || distance_sq >= min_distance_sq {
                                    continue;
                                }

                                let (normal, distance) = if distance_sq > 1.0e-12 {
                                    let distance = distance_sq.sqrt();
                                    (delta / distance, distance)
                                } else {
                                    (particle_pair_fallback_direction(i, j), 0.0)
                                };
                                let correction = normal
                                    * ((rest_distance - distance)
                                        * 0.5
                                        * INCOMPRESSIBLE_PARTICLE_SPACING_STRENGTH);
                                corrections[i] += correction;
                                corrections[j] -= correction;
                            }
                        }
                    }
                }
            }

            let mut moved = false;
            for (particle, correction) in self.particles.iter_mut().zip(corrections.into_iter()) {
                let correction = clamp_vec3_length(correction, max_correction);
                if correction.length_squared() <= 1.0e-12 {
                    continue;
                }
                particle.x += correction;
                particle.j = 1.0;
                moved = true;
            }

            if !moved {
                break;
            }

            for particle in &mut self.particles {
                collide_particle_with_box_with_padding(
                    particle,
                    bounds.min_ws,
                    bounds.max_ws,
                    particle_min_padding,
                    particle_max_padding,
                );
                if let Some(terrain) = terrain {
                    collide_particle_with_terrain_iterative(
                        particle,
                        terrain,
                        terrain_collision_margin,
                        terrain_max_correction,
                        TERRAIN_PARTICLE_COLLISION_ITERATIONS,
                        bounds.min_ws,
                        bounds.max_ws,
                        particle_min_padding,
                        particle_max_padding,
                    );
                }
                repair_particle_state_with_padding(
                    particle,
                    bounds.min_ws,
                    bounds.max_ws,
                    particle_min_padding,
                    particle_max_padding,
                    self.config.j_min,
                    max_particle_speed,
                );
                particle.j = 1.0;
            }
        }
    }

    fn grid_to_particle(&mut self, dt: f32) -> WaterG2pBreakdown {
        self.grid_to_particle_impl(dt, false)
    }

    fn grid_to_particle_timed(&mut self, dt: f32) -> WaterG2pBreakdown {
        self.grid_to_particle_impl(dt, true)
    }

    fn grid_to_particle_impl(&mut self, dt: f32, collect_breakdown: bool) -> WaterG2pBreakdown {
        let total_start = collect_breakdown.then(Instant::now);
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
        let c_scale = 4.0 * inv_dx * inv_dx;
        let bounds = self.config.collider;
        let particle_padding = self.dx * self.config.wall_padding_cells.max(1.0);
        let particle_min_padding = Vec3::splat(particle_padding);
        let particle_max_padding = Vec3::splat(particle_padding);
        let j_min = self.config.j_min;
        let max_particle_speed = max_particle_speed_for_substep(dx, dt);
        let grid = &self.grid;
        let terrain_grid = &self.terrain_grid;
        let terrain = self.terrain.as_ref();
        let terrain_collision_margin = self.terrain_collision_margin();
        let terrain_max_correction = particle_padding;

        for particle in &mut self.particles {
            let gather_start = collect_breakdown.then(Instant::now);
            let local_pos = particle.x - origin_ws;
            let grid_pos = local_pos * inv_dx;
            let base = base_coord(grid_pos);
            let fx = grid_pos - base.as_vec3();
            let weights = quadratic_weights(fx);

            let mut new_v = Vec3::ZERO;
            let mut new_c = Mat3::ZERO;
            for oz in 0..3 {
                for oy in 0..3 {
                    for ox in 0..3 {
                        let node = base + IVec3::new(ox, oy, oz);
                        if !in_grid(node, grid_dim) {
                            continue;
                        }

                        let weight = weights[ox as usize].x
                            * weights[oy as usize].y
                            * weights[oz as usize].z;
                        if weight <= 0.0 {
                            continue;
                        }

                        let node_idx =
                            grid_index_dims(grid_dim, node.x as u32, node.y as u32, node.z as u32);
                        let grid_v = grid[node_idx].v;
                        let node_local = node.as_vec3() * dx;
                        let dpos = node_local - local_pos;
                        new_v += weight * grid_v;
                        new_c += outer_product(weight * grid_v, dpos) * c_scale;
                    }
                }
            }
            if let Some(gather_start) = gather_start {
                gather_seconds += gather_start.elapsed().as_secs_f64();
            }

            particle.v = clamp_vec3_length(new_v, max_particle_speed);
            if self.config.pressure_projection_iterations == 0 {
                particle.c = clamp_mat3_components(new_c, MAX_AFFINE_COMPONENT);
                let trace_c = particle.c.x_axis.x + particle.c.y_axis.y + particle.c.z_axis.z;
                particle.j = (particle.j * (1.0 + dt * trace_c)).max(j_min);
            } else {
                let traceless_c = make_mat3_traceless(new_c) * INCOMPRESSIBLE_APIC_BLEND;
                particle.c =
                    clamp_mat3_components(traceless_c, MAX_INCOMPRESSIBLE_AFFINE_COMPONENT);
                particle.j = 1.0;
            }
            particle.x += particle.v * dt;

            let box_start = collect_breakdown.then(Instant::now);
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
                    let terrain_start = collect_breakdown.then(Instant::now);
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
                        TerrainGridParticleQuery::CachedProjection { sdf, normal } => {
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

            let repair_start = collect_breakdown.then(Instant::now);
            repair_particle_state_with_padding(
                particle,
                bounds.min_ws,
                bounds.max_ws,
                particle_min_padding,
                particle_max_padding,
                j_min,
                max_particle_speed,
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
struct TerrainGridCacheRebuildStats {
    node_count: usize,
    has_sdf_count: usize,
    near_surface_count: usize,
    normal_count: usize,
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
    [
        0.5 * (Vec3::splat(1.5) - fx).powf(2.0),
        Vec3::splat(0.75) - (fx - Vec3::ONE).powf(2.0),
        0.5 * (fx - Vec3::splat(0.5)).powf(2.0),
    ]
}

fn in_grid(node: IVec3, grid_dim: glam::UVec3) -> bool {
    node.x >= 0
        && node.y >= 0
        && node.z >= 0
        && node.x < grid_dim.x as i32
        && node.y < grid_dim.y as i32
        && node.z < grid_dim.z as i32
}

fn grid_coord_dims(grid_dim: glam::UVec3, idx: usize) -> glam::UVec3 {
    let x_dim = grid_dim.x as usize;
    let y_dim = grid_dim.y as usize;
    let x = idx % x_dim;
    let yz = idx / x_dim;
    let y = yz % y_dim;
    let z = yz / y_dim;
    glam::UVec3::new(x as u32, y as u32, z as u32)
}

fn pressure_projection_neighbor_index(
    coord: glam::UVec3,
    axis: usize,
    direction: i32,
    grid_dim: glam::UVec3,
) -> Option<usize> {
    let mut neighbor = coord.as_ivec3();
    match axis {
        0 => neighbor.x += direction,
        1 => neighbor.y += direction,
        2 => neighbor.z += direction,
        _ => return None,
    }

    in_grid(neighbor, grid_dim).then(|| {
        grid_index_dims(
            grid_dim,
            neighbor.x as u32,
            neighbor.y as u32,
            neighbor.z as u32,
        )
    })
}

fn pressure_projection_solid_node(
    idx: usize,
    grid_boundary_flags: &[u8],
    terrain_grid: &[WaterTerrainGridSample],
    terrain_collision_margin: f32,
) -> bool {
    grid_boundary_flags.get(idx).is_some_and(|flags| *flags != 0)
        || terrain_grid.get(idx).is_some_and(|sample| {
            sample.has_sdf
                && sample.sdf <= terrain_collision_margin
                && sample.normal.length_squared() > 0.0
        })
}

fn pressure_projection_fluid_node(
    idx: usize,
    grid: &[super::pond::WaterGridNode],
    grid_boundary_flags: &[u8],
    terrain_grid: &[WaterTerrainGridSample],
    terrain_collision_margin: f32,
) -> bool {
    grid.get(idx)
        .is_some_and(|node| node.mass > ACTIVE_MASS_EPSILON)
        && !pressure_projection_solid_node(
            idx,
            grid_boundary_flags,
            terrain_grid,
            terrain_collision_margin,
        )
}

fn pressure_projection_divergence_at(
    coord: glam::UVec3,
    grid_dim: glam::UVec3,
    grid: &[super::pond::WaterGridNode],
    grid_boundary_flags: &[u8],
    terrain_grid: &[WaterTerrainGridSample],
    terrain_collision_margin: f32,
    inv_dx: f32,
) -> f32 {
    let idx = grid_index_dims(grid_dim, coord.x, coord.y, coord.z);
    let center_velocity = grid[idx].v;
    let vxm = pressure_projection_neighbor_velocity_component(
        coord,
        0,
        -1,
        center_velocity,
        grid_dim,
        grid,
        grid_boundary_flags,
        terrain_grid,
        terrain_collision_margin,
    );
    let vxp = pressure_projection_neighbor_velocity_component(
        coord,
        0,
        1,
        center_velocity,
        grid_dim,
        grid,
        grid_boundary_flags,
        terrain_grid,
        terrain_collision_margin,
    );
    let vym = pressure_projection_neighbor_velocity_component(
        coord,
        1,
        -1,
        center_velocity,
        grid_dim,
        grid,
        grid_boundary_flags,
        terrain_grid,
        terrain_collision_margin,
    );
    let vyp = pressure_projection_neighbor_velocity_component(
        coord,
        1,
        1,
        center_velocity,
        grid_dim,
        grid,
        grid_boundary_flags,
        terrain_grid,
        terrain_collision_margin,
    );
    let vzm = pressure_projection_neighbor_velocity_component(
        coord,
        2,
        -1,
        center_velocity,
        grid_dim,
        grid,
        grid_boundary_flags,
        terrain_grid,
        terrain_collision_margin,
    );
    let vzp = pressure_projection_neighbor_velocity_component(
        coord,
        2,
        1,
        center_velocity,
        grid_dim,
        grid,
        grid_boundary_flags,
        terrain_grid,
        terrain_collision_margin,
    );

    ((vxp - vxm) + (vyp - vym) + (vzp - vzm)) * (0.5 * inv_dx)
}

fn pressure_projection_neighbor_velocity_component(
    coord: glam::UVec3,
    axis: usize,
    direction: i32,
    center_velocity: Vec3,
    grid_dim: glam::UVec3,
    grid: &[super::pond::WaterGridNode],
    grid_boundary_flags: &[u8],
    terrain_grid: &[WaterTerrainGridSample],
    terrain_collision_margin: f32,
) -> f32 {
    let center_component = vec3_component(center_velocity, axis);
    let Some(neighbor_idx) =
        pressure_projection_neighbor_index(coord, axis, direction, grid_dim)
    else {
        return center_component;
    };
    if pressure_projection_solid_node(
        neighbor_idx,
        grid_boundary_flags,
        terrain_grid,
        terrain_collision_margin,
    ) {
        return center_component;
    }
    if pressure_projection_fluid_node(
        neighbor_idx,
        grid,
        grid_boundary_flags,
        terrain_grid,
        terrain_collision_margin,
    ) {
        vec3_component(grid[neighbor_idx].v, axis)
    } else {
        0.0
    }
}

fn pressure_projection_neighbor_pressure(
    coord: glam::UVec3,
    axis: usize,
    direction: i32,
    center_pressure: f32,
    grid_dim: glam::UVec3,
    pressure: &[f32],
    grid: &[super::pond::WaterGridNode],
    grid_boundary_flags: &[u8],
    terrain_grid: &[WaterTerrainGridSample],
    terrain_collision_margin: f32,
) -> f32 {
    let Some(neighbor_idx) =
        pressure_projection_neighbor_index(coord, axis, direction, grid_dim)
    else {
        return center_pressure;
    };
    if pressure_projection_solid_node(
        neighbor_idx,
        grid_boundary_flags,
        terrain_grid,
        terrain_collision_margin,
    ) {
        return center_pressure;
    }
    if pressure_projection_fluid_node(
        neighbor_idx,
        grid,
        grid_boundary_flags,
        terrain_grid,
        terrain_collision_margin,
    ) {
        pressure[neighbor_idx]
    } else {
        0.0
    }
}

fn project_grid_node_collisions(
    node: &mut super::pond::WaterGridNode,
    boundary_flags: u8,
    terrain_sample: WaterTerrainGridSample,
    terrain_collision_margin: f32,
    wall_damping: f32,
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

fn vec3_component(value: Vec3, axis: usize) -> f32 {
    match axis {
        0 => value.x,
        1 => value.y,
        2 => value.z,
        _ => 0.0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum TerrainGridParticleQuery {
    Skip { sdf: f32 },
    CachedProjection { sdf: f32, normal: Vec3 },
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
    let interpolation_slack = dx * 0.25;
    if sdf > collision_margin + interpolation_slack {
        return TerrainGridParticleQuery::Skip { sdf };
    }

    if sdf <= collision_margin {
        let normal = trilinear_sdf_gradient(corner_sdf, f).normalize_or_zero();
        if normal.is_finite() && normal.length_squared() > 0.0 {
            return TerrainGridParticleQuery::CachedProjection { sdf, normal };
        }
    }

    TerrainGridParticleQuery::ExactFallback
}

fn should_shadow_sample_terrain(particle_idx: usize) -> bool {
    particle_idx % 128 == 0
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

fn grid_index_dims(grid_dim: glam::UVec3, x: u32, y: u32, z: u32) -> usize {
    ((z as usize * grid_dim.y as usize + y as usize) * grid_dim.x as usize) + x as usize
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn outer_product(a: Vec3, b: Vec3) -> Mat3 {
    Mat3::from_cols(a * b.x, a * b.y, a * b.z)
}

fn particle_spacing_cell(position: Vec3, inv_cell_size: f32) -> (i32, i32, i32) {
    let cell = (position * inv_cell_size).floor().as_ivec3();
    (cell.x, cell.y, cell.z)
}

fn particle_pair_fallback_direction(a: usize, b: usize) -> Vec3 {
    let mut n = (a as u32).wrapping_mul(73_856_093)
        ^ (b as u32).wrapping_mul(19_349_663)
        ^ 0x9e37_79b9;
    n ^= n >> 16;
    n = n.wrapping_mul(0x7feb_352d);
    n ^= n >> 15;
    n = n.wrapping_mul(0x846c_a68b);
    n ^= n >> 16;

    let x = ((n & 0x3ff) as f32) / 511.5 - 1.0;
    let y = (((n >> 10) & 0x3ff) as f32) / 511.5 - 1.0;
    let z = (((n >> 20) & 0x3ff) as f32) / 511.5 - 1.0;
    let direction = Vec3::new(x, y, z).normalize_or_zero();
    if direction.length_squared() > 0.0 {
        direction
    } else {
        Vec3::X
    }
}

fn make_mat3_traceless(value: Mat3) -> Mat3 {
    let trace_third = (value.x_axis.x + value.y_axis.y + value.z_axis.z) / 3.0;
    Mat3::from_cols(
        Vec3::new(value.x_axis.x - trace_third, value.x_axis.y, value.x_axis.z),
        Vec3::new(value.y_axis.x, value.y_axis.y - trace_third, value.y_axis.z),
        Vec3::new(value.z_axis.x, value.z_axis.y, value.z_axis.z - trace_third),
    )
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
    j_min: f32,
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

    if !mat3_is_finite(particle.c) {
        particle.c = Mat3::ZERO;
    }
    particle.c = clamp_mat3_components(particle.c, MAX_AFFINE_COMPONENT);
    if !particle.j.is_finite() {
        particle.j = 1.0;
    }
    particle.j = particle.j.clamp(j_min, MAX_J);
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
    j_min: f32,
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
    let j_min_threshold = j_min * 1.001;
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
    let mut min_j = f32::INFINITY;
    let mut max_j = f32::NEG_INFINITY;
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
            || !particle.j.is_finite()
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
            max_speed_j = particle.j;
            max_speed_terrain_sdf = terrain_sdf;
        }

        min_j = min_j.min(particle.j);
        max_j = max_j.max(particle.j);
        if particle.j <= j_min_threshold {
            j_min_clamped_particles += 1;
        }
        if particle.j >= j_max_threshold {
            j_max_clamped_particles += 1;
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
        collide_particle_with_terrain, collide_particle_with_terrain_iterative, grid_coord_dims,
        grid_index_dims, pressure_projection_divergence_at, pressure_projection_solid_node,
        project_velocity_away_from_surface, ACTIVE_MASS_EPSILON,
    };
    use crate::{
        PondWaterConfig, PondWaterSim, WaterTerrainColliderChunk, WaterTerrainColliderSet,
    };
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
    fn pressure_projection_reduces_divergent_grid_velocity() {
        let mut sim = PondWaterSim::new(
            PondWaterConfig::default()
                .with_particle_count(0)
                .with_grid_dim(UVec3::splat(12))
                .with_pressure_projection_iterations(96),
        );
        sim.clear_grid();

        let center = Vec3::splat(5.5);
        for z in 4..=7 {
            for y in 4..=7 {
                for x in 4..=7 {
                    let idx = grid_index_dims(sim.grid_dim, x, y, z);
                    sim.touched_grid_nodes.push(idx);
                    sim.grid[idx].mass = 1.0;
                    sim.grid[idx].v = (Vec3::new(x as f32, y as f32, z as f32) - center) * 0.1;
                }
            }
        }

        let before = active_grid_divergence_max(&sim);
        assert!(before > 0.0);

        sim.project_grid_incompressible(sim.config.substep_dt);

        let after = active_grid_divergence_max(&sim);
        assert!(
            after < before * 0.65,
            "pressure projection should reduce divergence: before={before} after={after}"
        );
    }

    #[test]
    fn incompressible_substeps_keep_particle_j_at_rest() {
        let mut sim = test_sim_with_particles();
        for _ in 0..64 {
            sim.substep(sim.config.substep_dt);
        }

        for particle in &sim.particles {
            assert!(
                (particle.j - 1.0).abs() < 1.0e-6,
                "incompressible particle j drifted: {}",
                particle.j
            );
        }
    }

    #[test]
    fn incompressible_spacing_relaxation_separates_overlapping_particles() {
        let mut sim = PondWaterSim::new(
            PondWaterConfig::default()
                .with_particle_count(0)
                .with_pressure_projection_iterations(16)
                .with_particle_spacing_relaxation_iterations(2),
        );
        let center = Vec3::new(0.5, 0.5, 0.5);
        sim.particles = vec![water_particle(center, Vec3::ZERO), water_particle(center, Vec3::ZERO)];

        sim.relax_incompressible_particle_spacing(sim.config.substep_dt);

        let distance = sim.particles[0].x.distance(sim.particles[1].x);
        assert!(
            distance > sim.config.particle_volume.cbrt() * 0.1,
            "overlapping particles were not separated: distance={distance}"
        );
        for particle in &sim.particles {
            assert!((particle.j - 1.0).abs() < 1.0e-6);
        }
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

    fn active_grid_divergence_max(sim: &PondWaterSim) -> f32 {
        let terrain_collision_margin = sim.terrain_collision_margin();
        sim.touched_grid_nodes
            .iter()
            .copied()
            .filter(|&idx| sim.grid[idx].mass > ACTIVE_MASS_EPSILON)
            .filter(|&idx| {
                !pressure_projection_solid_node(
                    idx,
                    &sim.grid_boundary_flags,
                    &sim.terrain_grid,
                    terrain_collision_margin,
                )
            })
            .map(|idx| {
                pressure_projection_divergence_at(
                    grid_coord_dims(sim.grid_dim, idx),
                    sim.grid_dim,
                    &sim.grid,
                    &sim.grid_boundary_flags,
                    &sim.terrain_grid,
                    terrain_collision_margin,
                    sim.inv_dx,
                )
                .abs()
            })
            .fold(0.0, f32::max)
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
