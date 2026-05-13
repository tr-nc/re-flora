use glam::{IVec3, Mat3, Vec3};
use std::time::Instant;

use super::pond::PondWaterSim;

const MAX_SUBSTEPS_PER_UPDATE: usize = 8;
const ACTIVE_MASS_EPSILON: f32 = 1.0e-8;

impl PondWaterSim {
    /// Advance the pond by fixed MLS-MPM substeps.
    pub fn update(&mut self, dt: f32, perf_logging: bool) {
        if dt <= 0.0 || !dt.is_finite() {
            return;
        }

        self.accumulator += dt.min(0.25);
        let substep_dt = self.config.substep_dt;
        for _ in 0..MAX_SUBSTEPS_PER_UPDATE {
            if self.accumulator < substep_dt {
                break;
            }
            self.substep_timed(substep_dt, perf_logging);
            self.accumulator -= substep_dt;
        }

        // Avoid a long catch-up spiral if a frame stalls while the sim is enabled.
        let max_remainder = substep_dt * MAX_SUBSTEPS_PER_UPDATE as f32;
        self.accumulator = self.accumulator.min(max_remainder);

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
            self.update_grid(dt);
            self.grid_to_particle(dt);
            return;
        }

        let total_start = Instant::now();
        self.clear_grid();

        let p2g_start = Instant::now();
        self.particle_to_grid(dt);
        let p2g_seconds = p2g_start.elapsed().as_secs_f64();

        let grid_start = Instant::now();
        let active_nodes = self.update_grid(dt);
        let grid_seconds = grid_start.elapsed().as_secs_f64();

        let g2p_start = Instant::now();
        self.grid_to_particle(dt);
        let g2p_seconds = g2p_start.elapsed().as_secs_f64();

        self.perf_stats.substeps += 1;
        self.perf_stats.p2g_seconds += p2g_seconds;
        self.perf_stats.grid_seconds += grid_seconds;
        self.perf_stats.g2p_seconds += g2p_seconds;
        self.perf_stats.total_seconds += total_start.elapsed().as_secs_f64();
        self.perf_stats.active_node_visits += active_nodes as u64;
    }

    fn log_perf_report(&mut self) {
        let stats = self.perf_stats;
        if stats.substeps == 0 {
            return;
        }

        let substeps = stats.substeps as f64;
        let grid_nodes = self.grid.len();
        log::info!(
            "[PERF][WATER] particles {} grid {:?} nodes {} substeps {} total {:.2}ms avg {:.3}ms/substep p2g {:.2}ms grid {:.2}ms g2p {:.2}ms active_nodes/substep {:.0}",
            self.particles.len(),
            self.grid_dim,
            grid_nodes,
            stats.substeps,
            stats.total_seconds * 1000.0,
            stats.total_seconds * 1000.0 / substeps,
            stats.p2g_seconds * 1000.0,
            stats.grid_seconds * 1000.0,
            stats.g2p_seconds * 1000.0,
            stats.active_node_visits as f64 / substeps,
        );

        self.perf_stats.reset();
        self.perf_report_seconds = 0.0;
    }

    fn clear_grid(&mut self) {
        for node in &mut self.grid {
            node.v = Vec3::ZERO;
            node.mass = 0.0;
            node.solid = false;
            node.normal = Vec3::ZERO;
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

            let pressure = stiffness * (particle.j.max(self.config.j_min).powf(-gamma) - 1.0);
            let pressure_scale = dt * volume * particle.j * pressure * d_inv;
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
                        grid_node.mass += weight * mass;
                        grid_node.v += weight * (momentum + affine * dpos);
                    }
                }
            }
        }
    }

    fn update_grid(&mut self, dt: f32) -> usize {
        let grid_dim = self.grid_dim;
        let gravity = self.config.gravity;
        let wall_cells = self.config.wall_padding_cells.max(1.0);
        let wall_damping = self.config.wall_damping.clamp(0.0, 1.0);

        let mut active_nodes = 0usize;

        for z in 0..grid_dim.z {
            for y in 0..grid_dim.y {
                for x in 0..grid_dim.x {
                    let idx = grid_index_dims(grid_dim, x, y, z);
                    let node = &mut self.grid[idx];
                    if node.mass <= ACTIVE_MASS_EPSILON {
                        continue;
                    }

                    active_nodes += 1;
                    node.v /= node.mass;
                    node.v += gravity * dt;

                    let mut normal = Vec3::ZERO;
                    if x as f32 <= wall_cells && node.v.x < 0.0 {
                        node.v.x *= -wall_damping;
                        normal += Vec3::X;
                    }
                    if (grid_dim.x - 1 - x) as f32 <= wall_cells && node.v.x > 0.0 {
                        node.v.x *= -wall_damping;
                        normal -= Vec3::X;
                    }
                    if y as f32 <= wall_cells && node.v.y < 0.0 {
                        node.v.y *= -wall_damping;
                        normal += Vec3::Y;
                    }
                    if (grid_dim.y - 1 - y) as f32 <= wall_cells && node.v.y > 0.0 {
                        node.v.y *= -wall_damping;
                        normal -= Vec3::Y;
                    }
                    if z as f32 <= wall_cells && node.v.z < 0.0 {
                        node.v.z *= -wall_damping;
                        normal += Vec3::Z;
                    }
                    if (grid_dim.z - 1 - z) as f32 <= wall_cells && node.v.z > 0.0 {
                        node.v.z *= -wall_damping;
                        normal -= Vec3::Z;
                    }

                    if normal.length_squared() > 0.0 {
                        node.solid = true;
                        node.normal = normal.normalize_or_zero();
                    }
                }
            }
        }

        active_nodes
    }

    fn grid_to_particle(&mut self, dt: f32) {
        let origin_ws = self.origin_ws;
        let grid_dim = self.grid_dim;
        let dx = self.dx;
        let inv_dx = self.inv_dx;
        let c_scale = 4.0 * inv_dx * inv_dx;
        let bounds = self.config.collider;
        let particle_padding = self.dx * self.config.wall_padding_cells.max(1.0);
        let j_min = self.config.j_min;
        let grid = &self.grid;

        for particle in &mut self.particles {
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

            particle.v = new_v;
            particle.c = new_c;
            let trace_c = new_c.x_axis.x + new_c.y_axis.y + new_c.z_axis.z;
            particle.j = (particle.j * (1.0 + dt * trace_c)).max(j_min);
            particle.x += particle.v * dt;
            collide_particle_with_box(particle, bounds.min_ws, bounds.max_ws, particle_padding);

            if !particle.x.is_finite() || !particle.v.is_finite() || !particle.j.is_finite() {
                particle.x = bounds.clamp_point(particle.x, particle_padding);
                particle.v = Vec3::ZERO;
                particle.c = Mat3::ZERO;
                particle.j = 1.0;
            }
        }
    }
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

fn grid_index_dims(grid_dim: glam::UVec3, x: u32, y: u32, z: u32) -> usize {
    ((z as usize * grid_dim.y as usize + y as usize) * grid_dim.x as usize) + x as usize
}

fn outer_product(a: Vec3, b: Vec3) -> Mat3 {
    Mat3::from_cols(a * b.x, a * b.y, a * b.z)
}

fn collide_particle_with_box(
    particle: &mut super::pond::WaterParticle,
    min_ws: Vec3,
    max_ws: Vec3,
    padding: f32,
) {
    let min = min_ws + Vec3::splat(padding);
    let max = max_ws - Vec3::splat(padding);

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

#[cfg(test)]
mod tests {
    use crate::PondWaterSim;

    #[test]
    fn fixed_box_substeps_keep_particles_finite_and_bounded() {
        let mut sim = PondWaterSim::fixed_test_box();
        for _ in 0..120 {
            sim.substep(sim.config.substep_dt);
        }

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
