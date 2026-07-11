use glam::{IVec3, Mat3, Vec3};
use std::time::Instant;

use super::{
    density::{grid_density_no_tension_j, NO_TENSION_MAX_J},
    diagnostics::{
        quiet_motion_sample, quiet_settling_damping_weight, quiet_settling_local_velocity_weight,
    },
    repair::{
        affine_damping_factor, clamp_mat3_components, clamp_vec3_length,
        collide_particle_with_box_with_padding, collide_particle_with_terrain_iterative, mat3_is_finite,
        max_particle_speed_for_substep, repair_particle_state_after_g2p_with_padding,
        velocity_damping_factor, MAX_AFFINE_COMPONENT,
    },
    transfer::{
        base_coord, grid_index_dims, in_grid, particle_stencil_interior,
        project_grid_node_collisions, project_particle_with_cached_terrain, quadratic_weights,
        terrain_grid_particle_query, TerrainGridParticleQuery, WaterG2pBreakdown,
    },
    ACTIVE_MASS_EPSILON, TERRAIN_PARTICLE_COLLISION_ITERATIONS,
};
use crate::pond::PondWaterSim;

impl PondWaterSim {
    pub(super) fn update_grid(&mut self, dt: f32) -> usize {
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

    pub(super) fn grid_to_particle(&mut self, dt: f32) -> WaterG2pBreakdown {
        self.grid_to_particle_impl::<false>(dt)
    }

    pub(super) fn grid_to_particle_timed(&mut self, dt: f32) -> WaterG2pBreakdown {
        self.grid_to_particle_impl::<true>(dt)
    }

    pub(super) fn apply_quiet_settling_damping(&mut self, dt: f32) {
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
