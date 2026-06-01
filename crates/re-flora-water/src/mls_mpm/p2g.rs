use glam::{IVec3, Vec3};

use super::{
    density::{
        fluid_eos_pressure, fluid_stress, particle_density_from_grid,
        terrain_boundary_density_correction, terrain_ghost_density_for_grid_node,
        terrain_solid_occupancy_from_sdf,
    },
    repair::mat3_is_finite,
    transfer::{
        base_coord, grid_index_dims, grid_node_coord_from_index, in_grid,
        particle_stencil_interior, quadratic_weights,
    },
};
use crate::pond::PondWaterSim;

impl PondWaterSim {
    pub(super) fn clear_grid(&mut self) {
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

    pub(super) fn particle_to_grid(&mut self, dt: f32) {
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
}
