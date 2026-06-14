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
