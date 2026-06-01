use glam::{IVec3, Mat3, Vec3};
use std::time::Instant;

use super::{
    collider::WaterTerrainColliderSet,
    pond::{PondWaterSim, WaterGridNode, WaterTerrainGridSample},
};

mod density;
mod diagnostics;
mod repair;
mod terrain;
mod transfer;

use density::{
    fluid_eos_pressure, fluid_stress, grid_density_no_tension_j, particle_density_from_grid,
    terrain_boundary_density_correction, terrain_ghost_density_for_grid_node,
    terrain_solid_occupancy_from_sdf, NO_TENSION_MAX_J,
};
use diagnostics::{
    quiet_motion_sample, quiet_settling_damping_weight, quiet_settling_local_velocity_weight,
};
pub use terrain::{
    build_terrain_grid_cache_patch, WaterTerrainCacheApplyReport,
    WaterTerrainCacheBuildRequest, WaterTerrainCachePatch, WaterTerrainCacheRebuildStats,
};
use transfer::{
    base_coord, grid_index_dims, grid_node_coord_from_index, in_grid, particle_stencil_interior,
    project_grid_node_collisions, project_particle_with_cached_terrain, quadratic_weights,
    terrain_grid_particle_query, TerrainGridParticleQuery, WaterG2pBreakdown,
};
#[cfg(test)]
use density::{
    blend_no_tension_j, density_j_feedback_blend, eos_pressure, integrate_no_tension_j,
    terrain_solid_kernel_weight,
};

use repair::{
    affine_damping_factor, clamp_mat3_components, clamp_vec3_length,
    collide_particle_with_box_with_padding, collide_particle_with_terrain_iterative, mat3_is_finite,
    max_particle_speed_for_substep,
    repair_particle_state_after_g2p_with_padding, velocity_damping_factor, MAX_AFFINE_COMPONENT,
};
#[cfg(test)]
use repair::{
    damp_velocity_tangent_to_surface, project_velocity_away_from_surface,
    terrain_tangent_damping_factor,
};
#[cfg(test)]
use repair::collide_particle_with_terrain;

const MAX_SUBSTEPS_PER_UPDATE: usize = 8;
const ACTIVE_MASS_EPSILON: f32 = 1.0e-8;
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

    fn clear_grid(&mut self) {
        for node_idx in self.touched_grid_nodes.drain(..) {
            if let Some(node) = self.grid.get_mut(node_idx) {
                node.v = Vec3::ZERO;
                node.mass = 0.0;
                node.solid = false;
                node.normal = Vec3::ZERO;
            }
            if let Some(ghost_density) = self.terrain_ghost_density.get_mut(node_idx) {
                *ghost_density = 0.0;
            }
        }
    }

    fn particle_to_grid(&mut self, dt: f32) {
        self.particle_to_grid_mass_momentum();
        self.update_terrain_ghost_density_grid();
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

    fn ensure_terrain_ghost_density_len(&mut self) {
        let node_count = self.grid.len();
        if self.terrain_ghost_density.len() != node_count {
            self.terrain_ghost_density = vec![0.0; node_count];
        }
    }

    fn update_terrain_ghost_density_grid(&mut self) {
        if self.touched_grid_nodes.is_empty() {
            return;
        }

        self.ensure_terrain_ghost_density_len();
        let grid = &self.grid;
        let terrain_grid = &self.terrain_grid;
        let terrain_ghost_density = &mut self.terrain_ghost_density;
        let grid_dim = self.grid_dim;
        let dx = self.dx;
        let inv_dx = self.inv_dx;
        let inv_cell_volume = inv_dx * inv_dx * inv_dx;
        let rest_density = self.config.particle_mass / self.config.particle_volume;
        let stiffness = self.config.stiffness;
        let gamma = self.config.gamma;
        let pressure_floor = self.config.pressure_floor;
        let gravity = self.config.gravity;
        let occupancy_transition_cells = self.config.terrain_density_occupancy_transition_cells;

        for &node_idx in &self.touched_grid_nodes {
            if let Some(cached_density) = terrain_ghost_density.get_mut(node_idx) {
                *cached_density = 0.0;
            }

            let Some(sample) = terrain_grid.get(node_idx).copied() else {
                continue;
            };
            if !sample.has_sdf
                || terrain_solid_occupancy_from_sdf(sample.sdf, dx, occupancy_transition_cells)
                    <= 0.0
            {
                continue;
            }

            let Some(node) = grid_node_coord_from_index(grid_dim, node_idx) else {
                continue;
            };
            let Some(ghost_density) = terrain_ghost_density_for_grid_node(
                grid,
                grid_dim,
                node,
                sample,
                dx,
                inv_dx,
                inv_cell_volume,
                rest_density,
                stiffness,
                gamma,
                pressure_floor,
                gravity,
                occupancy_transition_cells,
            ) else {
                continue;
            };
            if let Some(cached_density) = terrain_ghost_density.get_mut(node_idx) {
                *cached_density = ghost_density;
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
        let terrain_ghost_density = &self.terrain_ghost_density;
        let terrain_density_min_fluid_fraction = self.config.terrain_density_min_fluid_fraction;
        let terrain_density_max_correction_factor = self.config.terrain_density_max_correction_factor;
        let terrain_density_occupancy_transition_cells =
            self.config.terrain_density_occupancy_transition_cells;
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
                terrain_grid,
                terrain_ghost_density,
                grid_dim,
                base,
                wx,
                wy,
                wz,
                dx,
                terrain_density_min_fluid_fraction,
                terrain_density_max_correction_factor,
                terrain_density_occupancy_transition_cells,
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

    fn apply_quiet_settling_damping(&mut self, dt: f32) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }

        let Some(sample) = quiet_motion_sample(&self.particles) else {
            return;
        };
        let velocity_damping_per_sec = if self.config.quiet_settling_velocity_damping_per_sec.is_finite() {
            self.config.quiet_settling_velocity_damping_per_sec.max(0.0)
        } else {
            0.0
        };
        let affine_damping_per_sec = if self.config.quiet_settling_affine_damping_per_sec.is_finite() {
            self.config.quiet_settling_affine_damping_per_sec.max(0.0)
        } else {
            0.0
        };
        let damping_weight = quiet_settling_damping_weight(
            sample.avg_speed,
            sample.max_speed,
            velocity_damping_per_sec,
            affine_damping_per_sec,
        );
        if damping_weight <= 0.0 {
            return;
        }

        let affine_damping = (-affine_damping_per_sec * damping_weight * dt)
            .exp()
            .clamp(0.0, 1.0);
        for particle in &mut self.particles {
            if !particle.v.is_finite() || !mat3_is_finite(particle.c) {
                continue;
            }

            let speed = particle.v.length();
            let local_weight = quiet_settling_local_velocity_weight(speed);
            if local_weight > 0.0 {
                let velocity_damping = (-velocity_damping_per_sec
                    * damping_weight
                    * local_weight
                    * dt)
                    .exp()
                    .clamp(0.0, 1.0);
                particle.v *= velocity_damping;
            }
            particle.c *= affine_damping;
        }
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

#[cfg(test)]
mod tests {
    use super::{
        affine_damping_factor, blend_no_tension_j, collide_particle_with_terrain,
        collide_particle_with_terrain_iterative, damp_velocity_tangent_to_surface,
        density_j_feedback_blend, eos_pressure, fluid_eos_pressure, fluid_stress,
        grid_density_no_tension_j, grid_node_coord_from_index, integrate_no_tension_j,
        project_velocity_away_from_surface, quiet_settling_damping_weight,
        quiet_settling_local_velocity_weight, terrain_boundary_density_correction,
        terrain_ghost_density_for_grid_node, terrain_grid_particle_query, terrain_solid_kernel_weight,
        terrain_solid_occupancy_from_sdf,
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
        assert_eq!(terrain_solid_occupancy_from_sdf(0.6, 1.0, 1.0), 0.0);
        assert_eq!(terrain_solid_occupancy_from_sdf(0.5, 1.0, 1.0), 0.0);
        assert!((terrain_solid_occupancy_from_sdf(0.0, 1.0, 1.0) - 0.5).abs() <= 1.0e-6);
        assert_eq!(terrain_solid_occupancy_from_sdf(-0.5, 1.0, 1.0), 1.0);
        assert_eq!(terrain_solid_occupancy_from_sdf(-0.6, 1.0, 1.0), 1.0);
        assert_eq!(terrain_solid_occupancy_from_sdf(0.0, 0.0, 1.0), 0.0);
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
    fn quiet_settling_damping_is_gated_by_body_speed() {
        assert_eq!(quiet_settling_damping_weight(0.03, 0.20, 0.0, 0.0), 0.0);
        assert!(quiet_settling_damping_weight(0.03, 0.20, 4.0, 10.0) > 0.99);
        assert_eq!(quiet_settling_damping_weight(0.30, 0.20, 4.0, 10.0), 0.0);
        assert_eq!(quiet_settling_damping_weight(0.03, 1.00, 4.0, 10.0), 0.0);

        assert!(quiet_settling_local_velocity_weight(0.10) > 0.99);
        assert_eq!(quiet_settling_local_velocity_weight(1.0), 0.0);
    }

    #[test]
    fn quiet_settling_damping_skips_fast_splashes() {
        let mut sim = PondWaterSim::new(PondWaterConfig::default());
        sim.particles = vec![
            crate::pond::WaterParticle {
                x: Vec3::new(0.25, 0.5, 0.25),
                v: Vec3::new(0.05, 0.0, 0.0),
                c: Mat3::from_diagonal(Vec3::splat(2.0)),
                j: 1.0,
            },
            crate::pond::WaterParticle {
                x: Vec3::new(0.75, 0.5, 0.75),
                v: Vec3::new(0.04, 0.0, 0.0),
                c: Mat3::from_diagonal(Vec3::splat(2.0)),
                j: 1.0,
            },
        ];

        sim.apply_quiet_settling_damping(1.0 / 60.0);
        assert!(sim.particles[0].v.length() < 0.05);
        assert!(sim.particles[0].c.x_axis.x < 2.0);

        let quiet_velocity = sim.particles[0].v;
        let quiet_affine = sim.particles[0].c;
        sim.particles[1].v = Vec3::X;
        sim.apply_quiet_settling_damping(1.0 / 60.0);
        assert_eq!(sim.particles[0].v, quiet_velocity);
        assert_eq!(sim.particles[0].c, quiet_affine);
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
    ) -> super::density::TerrainBoundaryDensitySample {
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
    ) -> super::density::TerrainBoundaryDensitySample {
        let mut terrain_ghost_density = vec![0.0; terrain_grid.len()];
        for (node_idx, sample) in terrain_grid.iter().copied().enumerate() {
            let Some(node) = grid_node_coord_from_index(grid_dim, node_idx) else {
                continue;
            };
            let Some(ghost_density) = terrain_ghost_density_for_grid_node(
                grid,
                grid_dim,
                node,
                sample,
                1.0,
                1.0,
                1.0,
                rest_density,
                16.0,
                4.0,
                -0.1,
                gravity,
                1.0,
            ) else {
                continue;
            };
            terrain_ghost_density[node_idx] = ghost_density;
        }

        terrain_boundary_density_correction(
            raw_density,
            terrain_grid,
            &terrain_ghost_density,
            grid_dim,
            IVec3::ZERO,
            weights,
            weights,
            weights,
            1.0,
            0.50,
            2.0,
            1.0,
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
