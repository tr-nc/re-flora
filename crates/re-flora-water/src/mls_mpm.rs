use glam::{IVec3, Mat3, Vec3};
use std::time::Instant;

use super::{
    collider::WaterTerrainColliderSet,
    pond::{PondWaterSim, WaterTerrainGridSample},
};

const MAX_SUBSTEPS_PER_UPDATE: usize = 8;
const ACTIVE_MASS_EPSILON: f32 = 1.0e-8;
const MAX_J: f32 = 8.0;
const MAX_PARTICLE_SPEED: f32 = 20.0;
const MAX_PARTICLE_CFL_CELLS_PER_SUBSTEP: f32 = 0.5;
const MAX_AFFINE_COMPONENT: f32 = 100.0;
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
            // G2P already resolves terrain every substep; the pre-P2G repair pass
            // only needs finite/box/J/speed cleanup in steady state.
            self.repair_particles(dt, false);
            self.clear_grid();
            self.particle_to_grid(dt);
            self.update_grid(dt);
            self.grid_to_particle(dt);
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
        let grid_seconds = grid_start.elapsed().as_secs_f64();

        let g2p_breakdown = self.grid_to_particle_timed(dt);

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

    fn log_perf_report(&mut self) {
        let stats = self.perf_stats;
        if stats.substeps == 0 {
            return;
        }

        let substeps = stats.substeps as f64;
        let grid_nodes = self.grid.len();
        let particle_stats = water_particle_debug_stats(&self.particles, self.terrain.as_ref());
        log::info!(
            "[PERF][WATER] particles {} grid {:?} nodes {} substeps {} total {:.2}ms avg {:.3}ms/substep repair {:.2}ms clear {:.2}ms p2g {:.2}ms grid {:.2}ms g2p {:.2}ms g2p_gather {:.2}ms g2p_box {:.2}ms g2p_terrain {:.2}ms g2p_repair {:.2}ms terrain_cache_skips/substep {:.0} terrain_cache_projections/substep {:.0} terrain_exact_fallbacks/substep {:.0} terrain_exact_checks/substep {:.0} terrain_exact_corrections/substep {:.0} active_nodes/substep {:.0} particle_y {:.3}..{:.3} avg {:.3} terrain_sdf_min {:.4} penetrating {} no_sdf {}",
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
            stats.active_node_visits as f64 / substeps,
            particle_stats.min_y,
            particle_stats.max_y,
            particle_stats.avg_y,
            particle_stats.min_terrain_sdf.unwrap_or(f32::NAN),
            particle_stats.terrain_penetrating,
            particle_stats.no_terrain_sdf,
        );

        self.perf_stats.reset();
        self.perf_report_seconds = 0.0;
    }

    pub(crate) fn rebuild_terrain_grid_cache(&mut self) {
        let terrain_collision_margin = self.dx * 0.5;
        // Cache a conservative narrow band around terrain. Hot loops use this
        // cheap water-grid SDF to skip exact collider queries for particles and
        // grid nodes that are clearly away from solids.
        let near_surface_band = terrain_collision_margin + self.dx * 2.0;
        let terrain = self.terrain.as_ref();
        let origin_ws = self.origin_ws;
        let grid_dim = self.grid_dim;
        let dx = self.dx;

        if self.terrain_grid.len() != self.grid.len() {
            self.terrain_grid = vec![WaterTerrainGridSample::default(); self.grid.len()];
        }

        for z in 0..grid_dim.z {
            for y in 0..grid_dim.y {
                for x in 0..grid_dim.x {
                    let idx = grid_index_dims(grid_dim, x, y, z);
                    let mut sample = WaterTerrainGridSample::default();
                    if let Some(terrain) = terrain {
                        let node_world = origin_ws + Vec3::new(x as f32, y as f32, z as f32) * dx;
                        if let Some(sdf) = terrain.sample_sdf_ws(node_world) {
                            sample.sdf = sdf;
                            sample.has_sdf = true;
                            sample.near_surface = sdf <= near_surface_band;
                            if sample.near_surface {
                                sample.normal = terrain.sample_normal_ws(node_world).unwrap_or(Vec3::Y);
                            }
                        }
                    }
                    self.terrain_grid[idx] = sample;
                }
            }
        }
    }

    fn repair_particles(&mut self, dt: f32, repair_terrain: bool) {
        let bounds = self.config.collider;
        let padding = self.dx * self.config.wall_padding_cells.max(1.0);
        let min_padding = Vec3::splat(padding);
        let max_padding = Vec3::splat(padding);
        let terrain_collision_margin = self.dx * 0.5;
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
        let terrain_grid = &self.terrain_grid;

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
                    let terrain_sample = terrain_grid.get(idx).copied().unwrap_or_default();
                    if terrain_sample.near_surface && terrain_sample.normal.length_squared() > 0.0 {
                        node.v = project_velocity_away_from_surface(node.v, terrain_sample.normal);
                        normal += terrain_sample.normal;
                    }

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
        let _ = self.grid_to_particle_impl(dt, false);
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
        let terrain_collision_margin = self.dx * 0.5;
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
            particle.c = clamp_mat3_components(new_c, MAX_AFFINE_COMPONENT);
            let trace_c = particle.c.x_axis.x + particle.c.y_axis.y + particle.c.z_axis.z;
            particle.j = (particle.j * (1.0 + dt * trace_c)).max(j_min);
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
                    match terrain_grid_particle_query(local_pos, inv_dx, dx, grid_dim, terrain_grid) {
                        TerrainGridParticleQuery::Skip => {
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

#[derive(Clone, Copy, Debug, PartialEq)]
enum TerrainGridParticleQuery {
    Skip,
    CachedProjection { sdf: f32, normal: Vec3 },
    ExactFallback,
}

fn terrain_grid_particle_query(
    local_pos: Vec3,
    inv_dx: f32,
    dx: f32,
    grid_dim: glam::UVec3,
    terrain_grid: &[WaterTerrainGridSample],
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
    let collision_margin = dx * 0.5;
    let interpolation_slack = dx * 0.5;
    if sdf > collision_margin + interpolation_slack {
        return TerrainGridParticleQuery::Skip;
    }

    if sdf <= collision_margin {
        let normal = trilinear_sdf_gradient(corner_sdf, f).normalize_or_zero();
        if normal.is_finite() && normal.length_squared() > 0.0 {
            return TerrainGridParticleQuery::CachedProjection { sdf, normal };
        }
    }

    TerrainGridParticleQuery::ExactFallback
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
    min_y: f32,
    max_y: f32,
    avg_y: f32,
    min_terrain_sdf: Option<f32>,
    terrain_penetrating: usize,
    no_terrain_sdf: usize,
}

fn water_particle_debug_stats(
    particles: &[super::pond::WaterParticle],
    terrain: Option<&WaterTerrainColliderSet>,
) -> WaterParticleDebugStats {
    if particles.is_empty() {
        return WaterParticleDebugStats {
            min_y: f32::NAN,
            max_y: f32::NAN,
            avg_y: f32::NAN,
            min_terrain_sdf: None,
            terrain_penetrating: 0,
            no_terrain_sdf: 0,
        };
    }

    let mut min_y = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut sum_y = 0.0;
    let mut min_terrain_sdf = f32::INFINITY;
    let mut terrain_penetrating = 0usize;
    let mut no_terrain_sdf = 0usize;

    for particle in particles {
        min_y = min_y.min(particle.x.y);
        max_y = max_y.max(particle.x.y);
        sum_y += particle.x.y;

        if let Some(terrain) = terrain {
            if let Some(sdf) = terrain.sample_sdf_ws(particle.x) {
                min_terrain_sdf = min_terrain_sdf.min(sdf);
                if sdf < 0.0 {
                    terrain_penetrating += 1;
                }
            } else {
                no_terrain_sdf += 1;
            }
        }
    }

    WaterParticleDebugStats {
        min_y,
        max_y,
        avg_y: sum_y / particles.len() as f32,
        min_terrain_sdf: min_terrain_sdf.is_finite().then_some(min_terrain_sdf),
        terrain_penetrating,
        no_terrain_sdf,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        collide_particle_with_terrain, collide_particle_with_terrain_iterative,
        project_velocity_away_from_surface,
    };
    use crate::{PondWaterSim, WaterTerrainColliderChunk, WaterTerrainColliderSet};
    use glam::{Mat3, UVec3, Vec3};

    #[test]
    fn fixed_box_substeps_keep_particles_finite_and_bounded() {
        let mut sim = PondWaterSim::fixed_test_box();
        for _ in 0..120 {
            sim.substep(sim.config.substep_dt);
        }

        assert_particles_finite_and_bounded(&sim);
    }

    #[test]
    fn terrain_collider_substeps_keep_particles_finite_and_bounded() {
        let mut sim = PondWaterSim::fixed_test_box();
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
        let mut sim = PondWaterSim::fixed_test_box();
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
        let mut sim = PondWaterSim::fixed_test_box();
        let bounds = sim.config.collider;
        let terrain_height = 0.5;
        let terrain_margin = sim.dx * 0.5;
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
        let mut sim = PondWaterSim::fixed_test_box();
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

    fn sdf_collider_set(
        bounds_min_ws: Vec3,
        bounds_max_ws: Vec3,
        dim: UVec3,
        sdf: impl Fn(Vec3) -> f32,
    ) -> WaterTerrainColliderSet {
        let chunk_id = bounds_min_ws.floor().as_ivec3();
        assert_eq!(bounds_min_ws, chunk_id.as_vec3());
        assert_eq!(bounds_max_ws, bounds_min_ws + Vec3::ONE);

        let mut sdf_ws = Vec::new();
        for z in 0..dim.z {
            let tz = z as f32 / (dim.z - 1) as f32;
            for y in 0..dim.y {
                let ty = y as f32 / (dim.y - 1) as f32;
                for x in 0..dim.x {
                    let tx = x as f32 / (dim.x - 1) as f32;
                    let p = bounds_min_ws + (bounds_max_ws - bounds_min_ws) * Vec3::new(tx, ty, tz);
                    sdf_ws.push(sdf(p));
                }
            }
        }

        WaterTerrainColliderSet::from_chunk(WaterTerrainColliderChunk {
            chunk_id,
            dim,
            sdf_ws,
            revision: 0,
        })
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
