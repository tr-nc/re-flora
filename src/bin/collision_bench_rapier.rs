use rapier3d::parry::shape::AxisMask;
use rapier3d::prelude::*;
use std::time::{Duration, Instant};

const PREFIX: &str = "[COLLISION_BENCH] backend=rapier_voxels";
const BRICK_SIZE: i32 = 32;
const FLOOR_LAYERS: i32 = 2;
const BALL_RADIUS: Real = 2.0;
const BALL_COUNT: usize = 100;
const DT: Real = 1.0 / 120.0;
const BENCH_STEPS: usize = 600;
const FRICTION: Real = 0.8;
const RESTITUTION: Real = 0.05;
const DAMPING: Real = 0.1;
const PENETRATION_THRESHOLD: Real = 0.01;
const SEAM_BOUNCE_THRESHOLD: Real = 0.01;

fn main() {
    println!(
        "{PREFIX} event=config rapier_version=0.34.0 parry_version=0.29.0 \
         features=default_dim3_f32_std units=voxel voxel_size=1 brick=32x32x32 \
         floor_layers={FLOOR_LAYERS} balls={BALL_COUNT} ball_radius={BALL_RADIUS:.1} dt={DT:.9} \
         steps={BENCH_STEPS} gravity_y=-9.81 solver_iterations=4 internal_pgs_iterations=1 \
         stabilization_iterations=1 max_ccd_substeps=1 friction={FRICTION:.2} \
         restitution={RESTITUTION:.2} linear_damping={DAMPING:.2} angular_damping={DAMPING:.2} \
         ccd=true penetration_threshold={PENETRATION_THRESHOLD:.3}"
    );

    let rss_before_kib = resident_set_kib();
    let mut world = configured_world();
    let terrain_coords = flat_floor_voxels();

    let build_started = Instant::now();
    let terrain = ColliderBuilder::voxels(Vector::splat(1.0), &terrain_coords)
        .friction(FRICTION)
        .restitution(RESTITUTION)
        .build();
    let terrain_build = build_started.elapsed();

    let insert_started = Instant::now();
    let terrain_handle = world.insert_collider(terrain, None);
    let terrain_insert = insert_started.elapsed();

    let body_build_started = Instant::now();
    let ball_handles = insert_ball_count(&mut world, BALL_COUNT);
    let body_build = body_build_started.elapsed();
    let rss_after_build_kib = resident_set_kib();

    let mut step_times = Vec::with_capacity(BENCH_STEPS);
    for _ in 0..BENCH_STEPS {
        let started = Instant::now();
        world.step();
        step_times.push(started.elapsed());
    }

    let total_step_time: Duration = step_times.iter().copied().sum();
    let avg_step = total_step_time / BENCH_STEPS as u32;
    let p95_step = percentile_duration(&step_times, 0.95);
    let max_step = step_times.iter().copied().max().unwrap_or_default();
    let sleeping = ball_handles
        .iter()
        .filter(|&&handle| world.bodies[handle].is_sleeping())
        .count();
    let escaped = ball_handles
        .iter()
        .filter(|&&handle| {
            let p = world.bodies[handle].translation();
            p.x < -BALL_RADIUS
                || p.x > BRICK_SIZE as Real + BALL_RADIUS
                || p.y < -BALL_RADIUS
                || p.z < -BALL_RADIUS
                || p.z > BRICK_SIZE as Real + BALL_RADIUS
        })
        .count();
    let (penetrations, max_penetration) = penetration_stats(&world, PENETRATION_THRESHOLD);
    let scaling_100 = ScalingResult {
        bodies: BALL_COUNT,
        body_build,
        step_total: total_step_time,
        step_avg: avg_step,
        step_p95: p95_step,
        step_max: max_step,
        sleeping,
        escaped,
        penetrations,
        max_penetration,
    };

    println!(
        "{PREFIX} event=build terrain_voxels={} terrain_build_us={} terrain_insert_us={} \
         balls_build_us={} rss_before_kib={} rss_after_build_kib={} rss_delta_kib={}",
        terrain_coords.len(),
        terrain_build.as_micros(),
        terrain_insert.as_micros(),
        body_build.as_micros(),
        display_optional(rss_before_kib),
        display_optional(rss_after_build_kib),
        display_optional(memory_delta(rss_before_kib, rss_after_build_kib)),
    );
    println!(
        "{PREFIX} event=steps count={BENCH_STEPS} total_us={} avg_us={} p95_us={} max_us={} \
         sleeping={sleeping} escaped={escaped} penetrations={penetrations} \
         max_penetration={max_penetration:.6}",
        total_step_time.as_micros(),
        avg_step.as_micros(),
        p95_step.as_micros(),
        max_step.as_micros(),
    );

    let edit_coord = IVector::new(BRICK_SIZE / 2, FLOOR_LAYERS - 1, BRICK_SIZE / 2);
    let edit_started = Instant::now();
    let previous_state = world.colliders[terrain_handle]
        .shape_mut()
        .as_voxels_mut()
        .expect("terrain collider must remain a Parry Voxels shape")
        .set_voxel(edit_coord, false);
    let edit_time = edit_started.elapsed();
    let edit_apply_started = Instant::now();
    world.step();
    let edit_apply_step = edit_apply_started.elapsed();
    let edit_removed_filled_voxel = !previous_state.is_empty();
    println!(
        "{PREFIX} event=local_edit coord=({}, {}, {}) operation=remove previous_filled={} \
         edit_ns={} edit_us={:.3} apply_step_us={}",
        edit_coord.x,
        edit_coord.y,
        edit_coord.z,
        edit_removed_filled_voxel,
        edit_time.as_nanos(),
        edit_time.as_secs_f64() * 1_000_000.0,
        edit_apply_step.as_micros(),
    );

    let ccd = validate_ccd();
    println!(
        "{PREFIX} event=validation test=ccd passed={} start_y={:.3} start_vy={:.3} \
         after_one_step_y={:.6} after_one_step_vy={:.6} final_y={:.6} final_vy={:.6} \
         min_y={:.6} validation_steps={} floor_top={:.1}",
        ccd.passed,
        ccd.start_y,
        ccd.start_vy,
        ccd.after_one_step_y,
        ccd.after_one_step_vy,
        ccd.final_y,
        ccd.final_vy,
        ccd.min_y,
        ccd.steps,
        ccd.floor_top,
    );

    let rolling = validate_flat_rolling();
    println!(
        "{PREFIX} event=validation test=flat_voxel_rolling passed={} steps={} seam_crossings={} \
         start_x={:.6} final_x={:.6} backward_steps={} seam_stalls={} max_abs_vy={:.6} \
         max_abs_delta_vx={:.6} max_center_height_error={:.6}",
        rolling.passed,
        rolling.steps,
        rolling.seam_crossings,
        rolling.start_x,
        rolling.final_x,
        rolling.backward_steps,
        rolling.seam_stalls,
        rolling.max_abs_vy,
        rolling.max_abs_delta_vx,
        rolling.max_center_height_error,
    );

    let sleep = validate_sleep();
    println!(
        "{PREFIX} event=validation test=sleep passed={} sleeping_step={} final_y={:.6} \
         final_linear_speed={:.6} final_angular_speed={:.6}",
        sleep.passed,
        sleep
            .sleeping_step
            .map(|step| step.to_string())
            .unwrap_or_else(|| "none".to_owned()),
        sleep.final_y,
        sleep.final_linear_speed,
        sleep.final_angular_speed,
    );

    let isolated_brick_seam = validate_brick_seam(false);
    print_brick_seam(&isolated_brick_seam);
    let combined_brick_seam = validate_brick_seam(true);
    print_brick_seam(&combined_brick_seam);

    let boundary_edit = validate_boundary_edit_neighbor_states();
    println!(
        "{PREFIX} event=validation test=boundary_voxel_edit passed={} \
         before_left_x_pos_hidden={} before_right_x_neg_hidden={} \
         stale_right_x_neg_hidden_without_propagate={} updated_right_x_neg_exposed={} \
         updated_left_inner_x_pos_exposed={} removed_voxel_empty={} \
         required_api=propagate_voxel_change",
        boundary_edit.passed,
        boundary_edit.before_left_x_pos_hidden,
        boundary_edit.before_right_x_neg_hidden,
        boundary_edit.stale_right_x_neg_hidden_without_propagate,
        boundary_edit.updated_right_x_neg_exposed,
        boundary_edit.updated_left_inner_x_pos_exposed,
        boundary_edit.removed_voxel_empty,
    );

    let scaling_10 = benchmark_scaling(10);
    let scaling_1000 = benchmark_scaling(1_000);
    print_scaling(&scaling_10);
    print_scaling(&scaling_100);
    print_scaling(&scaling_1000);

    let all_valid = edit_removed_filled_voxel
        && ccd.passed
        && rolling.passed
        && sleep.passed
        && combined_brick_seam.passed
        && boundary_edit.passed;
    let memory_bytes = memory_delta(rss_before_kib, rss_after_build_kib).map(|kib| kib * 1024);
    println!(
        "{PREFIX} brick_dim=32 voxel_size=1 sphere_radius=2 bodies=100 steps=600 dt=0.008333 \
         build_ms={:.6} edit_ms={:.6} step_total_ms={:.6} step_avg_us={:.3} memory_bytes={} \
         penetrations={} seam_stalls={} step_p95_us={} max_penetration={:.6} ccd_pass={} \
         sleep_pass={} rolling_pass={} brick_seam_pass={} boundary_edit_pass={} sleeping={} \
         escaped={} rapier_version=0.34.0 parry_version=0.29.0 features=default_dim3_f32_std",
        terrain_build.as_secs_f64() * 1_000.0,
        edit_time.as_secs_f64() * 1_000.0,
        total_step_time.as_secs_f64() * 1_000.0,
        avg_step.as_secs_f64() * 1_000_000.0,
        display_optional(memory_bytes),
        penetrations,
        rolling.seam_stalls,
        p95_step.as_micros(),
        max_penetration,
        ccd.passed,
        sleep.passed,
        rolling.passed,
        combined_brick_seam.passed,
        boundary_edit.passed,
        sleeping,
        escaped,
    );
    println!("{PREFIX} event=summary passed={all_valid}");
    assert!(all_valid, "one or more collision validations failed");
}

fn configured_world() -> PhysicsWorld {
    let mut world = PhysicsWorld::new();
    world.gravity = Vector::new(0.0, -9.81, 0.0);
    world.integration_parameters.dt = DT;
    world
}

fn flat_floor_voxels() -> Vec<IVector> {
    let mut coords =
        Vec::with_capacity((BRICK_SIZE as usize) * (BRICK_SIZE as usize) * (FLOOR_LAYERS as usize));
    for y in 0..FLOOR_LAYERS {
        for z in 0..BRICK_SIZE {
            for x in 0..BRICK_SIZE {
                coords.push(IVector::new(x, y, z));
            }
        }
    }
    coords
}

fn insert_terrain(world: &mut PhysicsWorld) -> ColliderHandle {
    world.insert_collider(
        ColliderBuilder::voxels(Vector::splat(1.0), &flat_floor_voxels())
            .friction(FRICTION)
            .restitution(RESTITUTION),
        None,
    )
}

fn ball_body_builder(position: Vector) -> RigidBodyBuilder {
    RigidBodyBuilder::dynamic()
        .translation(position)
        .linear_damping(DAMPING)
        .angular_damping(DAMPING)
        .ccd_enabled(true)
        .can_sleep(true)
}

fn ball_collider_builder() -> ColliderBuilder {
    ColliderBuilder::ball(BALL_RADIUS)
        .density(1.0)
        .friction(FRICTION)
        .restitution(RESTITUTION)
}

fn insert_ball_count(world: &mut PhysicsWorld, count: usize) -> Vec<RigidBodyHandle> {
    let mut handles = Vec::with_capacity(count);
    for index in 0..count {
        let layer = index / 25;
        let index_in_layer = index % 25;
        let z = index_in_layer / 5;
        let x = index_in_layer % 5;
        let position = Vector::new(
            4.0 + x as Real * 6.0,
            7.0 + layer as Real * 5.0,
            4.0 + z as Real * 6.0,
        );
        let (body, _) = world.insert(ball_body_builder(position), ball_collider_builder());
        handles.push(body);
    }
    assert_eq!(handles.len(), count);
    handles
}

#[derive(Debug)]
struct ScalingResult {
    bodies: usize,
    body_build: Duration,
    step_total: Duration,
    step_avg: Duration,
    step_p95: Duration,
    step_max: Duration,
    sleeping: usize,
    escaped: usize,
    penetrations: usize,
    max_penetration: Real,
}

fn benchmark_scaling(body_count: usize) -> ScalingResult {
    let mut world = configured_world();
    insert_terrain(&mut world);
    let build_started = Instant::now();
    let handles = insert_ball_count(&mut world, body_count);
    let body_build = build_started.elapsed();
    let mut step_times = Vec::with_capacity(BENCH_STEPS);
    for _ in 0..BENCH_STEPS {
        let started = Instant::now();
        world.step();
        step_times.push(started.elapsed());
    }
    scaling_result(body_count, body_build, &step_times, &world, &handles)
}

fn scaling_result(
    body_count: usize,
    body_build: Duration,
    step_times: &[Duration],
    world: &PhysicsWorld,
    handles: &[RigidBodyHandle],
) -> ScalingResult {
    let step_total: Duration = step_times.iter().copied().sum();
    let sleeping = handles
        .iter()
        .filter(|&&handle| world.bodies[handle].is_sleeping())
        .count();
    let escaped = handles
        .iter()
        .filter(|&&handle| {
            let p = world.bodies[handle].translation();
            p.x < -BALL_RADIUS
                || p.x > BRICK_SIZE as Real + BALL_RADIUS
                || p.y < -BALL_RADIUS
                || p.z < -BALL_RADIUS
                || p.z > BRICK_SIZE as Real + BALL_RADIUS
        })
        .count();
    let (penetrations, max_penetration) = penetration_stats(world, PENETRATION_THRESHOLD);
    ScalingResult {
        bodies: body_count,
        body_build,
        step_total,
        step_avg: step_total / step_times.len() as u32,
        step_p95: percentile_duration(step_times, 0.95),
        step_max: step_times.iter().copied().max().unwrap_or_default(),
        sleeping,
        escaped,
        penetrations,
        max_penetration,
    }
}

fn print_scaling(result: &ScalingResult) {
    println!(
        "{PREFIX} event=scaling bodies={} steps={BENCH_STEPS} body_build_ms={:.6} \
         step_total_ms={:.6} step_avg_us={:.3} step_p95_us={} step_max_us={} sleeping={} \
         active={} escaped={} penetrations={} max_penetration={:.6}",
        result.bodies,
        result.body_build.as_secs_f64() * 1_000.0,
        result.step_total.as_secs_f64() * 1_000.0,
        result.step_avg.as_secs_f64() * 1_000_000.0,
        result.step_p95.as_micros(),
        result.step_max.as_micros(),
        result.sleeping,
        result.bodies - result.sleeping,
        result.escaped,
        result.penetrations,
        result.max_penetration,
    );
}

fn adjacent_floor_shapes(combine_neighbor_states: bool) -> (Voxels, Voxels) {
    let coords = flat_floor_voxels();
    let mut left = Voxels::new(Vector::splat(1.0), &coords);
    let mut right = Voxels::new(Vector::splat(1.0), &coords);
    if combine_neighbor_states {
        left.combine_voxel_states(&mut right, IVector::new(BRICK_SIZE, 0, 0));
    }
    (left, right)
}

fn insert_adjacent_floor_bricks(world: &mut PhysicsWorld, combine_neighbor_states: bool) {
    let (left, right) = adjacent_floor_shapes(combine_neighbor_states);
    world.insert_collider(
        ColliderBuilder::new(SharedShape::new(left))
            .friction(FRICTION)
            .restitution(0.0),
        None,
    );
    world.insert_collider(
        ColliderBuilder::new(SharedShape::new(right))
            .translation(Vector::new(BRICK_SIZE as Real, 0.0, 0.0))
            .friction(FRICTION)
            .restitution(0.0),
        None,
    );
}

#[derive(Debug)]
struct BrickSeamValidation {
    combined_neighbor_states: bool,
    passed: bool,
    steps: usize,
    crossing_step: Option<usize>,
    start_x: Real,
    final_x: Real,
    seam_stalls: usize,
    backward_steps: usize,
    max_abs_vy_near_seam: Real,
    max_abs_delta_vx_near_seam: Real,
    max_height_error_near_seam: Real,
}

fn validate_brick_seam(combine_neighbor_states: bool) -> BrickSeamValidation {
    const STEPS: usize = 720;
    const START_X: Real = 20.0;
    const START_VX: Real = 4.0;
    const SEAM_X: Real = BRICK_SIZE as Real;
    let expected_center_y = FLOOR_LAYERS as Real + BALL_RADIUS;
    let mut world = configured_world();
    insert_adjacent_floor_bricks(&mut world, combine_neighbor_states);
    let (ball, _) = world.insert(
        RigidBodyBuilder::dynamic()
            .translation(Vector::new(START_X, expected_center_y + 0.005, 16.0))
            .linvel(Vector::new(START_VX, 0.0, 0.0))
            .angvel(Vector::new(0.0, 0.0, -START_VX / BALL_RADIUS))
            .linear_damping(0.0)
            .angular_damping(0.0)
            .ccd_enabled(true)
            .can_sleep(false),
        ball_collider_builder().restitution(0.0),
    );

    let mut previous_x = START_X;
    let mut previous_vx = START_VX;
    let mut crossing_step = None;
    let mut seam_stalls = 0;
    let mut backward_steps = 0;
    let mut max_abs_vy_near_seam: Real = 0.0;
    let mut max_abs_delta_vx_near_seam: Real = 0.0;
    let mut max_height_error_near_seam: Real = 0.0;
    for step in 0..STEPS {
        world.step();
        let body = &world.bodies[ball];
        let position = body.translation();
        let velocity = body.linvel();
        if previous_x < SEAM_X && position.x >= SEAM_X {
            crossing_step = Some(step + 1);
        }
        if position.x + 1.0e-5 < previous_x {
            backward_steps += 1;
        }
        if step >= 30 && position.x - previous_x < START_VX * DT * 0.5 {
            seam_stalls += 1;
        }
        if (SEAM_X - 4.0..=SEAM_X + 4.0).contains(&position.x) {
            max_abs_vy_near_seam = max_abs_vy_near_seam.max(velocity.y.abs());
            max_abs_delta_vx_near_seam =
                max_abs_delta_vx_near_seam.max((velocity.x - previous_vx).abs());
            max_height_error_near_seam =
                max_height_error_near_seam.max((position.y - expected_center_y).abs());
        }
        previous_x = position.x;
        previous_vx = velocity.x;
    }

    let final_x = world.bodies[ball].translation().x;
    let passed = crossing_step.is_some()
        && final_x > SEAM_X + 8.0
        && seam_stalls == 0
        && backward_steps == 0
        && max_abs_vy_near_seam < SEAM_BOUNCE_THRESHOLD
        && max_abs_delta_vx_near_seam < 0.1
        && max_height_error_near_seam < 0.1;
    BrickSeamValidation {
        combined_neighbor_states: combine_neighbor_states,
        passed,
        steps: STEPS,
        crossing_step,
        start_x: START_X,
        final_x,
        seam_stalls,
        backward_steps,
        max_abs_vy_near_seam,
        max_abs_delta_vx_near_seam,
        max_height_error_near_seam,
    }
}

fn print_brick_seam(result: &BrickSeamValidation) {
    let neighbor_states = if result.combined_neighbor_states {
        "combined"
    } else {
        "isolated"
    };
    println!(
        "{PREFIX} event=validation test=adjacent_brick_seam neighbor_states={} passed={} \
         steps={} seam_x={} crossing_step={} start_x={:.6} final_x={:.6} seam_stalls={} \
         backward_steps={} bounce_detected={} bounce_threshold={:.3} \
         max_abs_vy_near_seam={:.6} max_abs_delta_vx_near_seam={:.6} \
         max_height_error_near_seam={:.6} required_api=combine_voxel_states",
        neighbor_states,
        result.passed,
        result.steps,
        BRICK_SIZE,
        result
            .crossing_step
            .map(|step| step.to_string())
            .unwrap_or_else(|| "none".to_owned()),
        result.start_x,
        result.final_x,
        result.seam_stalls,
        result.backward_steps,
        result.max_abs_vy_near_seam >= SEAM_BOUNCE_THRESHOLD,
        SEAM_BOUNCE_THRESHOLD,
        result.max_abs_vy_near_seam,
        result.max_abs_delta_vx_near_seam,
        result.max_height_error_near_seam,
    );
}

#[derive(Debug)]
struct BoundaryEditValidation {
    passed: bool,
    before_left_x_pos_hidden: bool,
    before_right_x_neg_hidden: bool,
    stale_right_x_neg_hidden_without_propagate: bool,
    updated_right_x_neg_exposed: bool,
    updated_left_inner_x_pos_exposed: bool,
    removed_voxel_empty: bool,
}

fn validate_boundary_edit_neighbor_states() -> BoundaryEditValidation {
    let origin_shift = IVector::new(BRICK_SIZE, 0, 0);
    let removed = IVector::new(BRICK_SIZE - 1, FLOOR_LAYERS - 1, BRICK_SIZE / 2);
    let right_neighbor = IVector::new(0, FLOOR_LAYERS - 1, BRICK_SIZE / 2);
    let left_inner = removed - IVector::X;
    let (left, right) = adjacent_floor_shapes(true);
    let before_left_x_pos_hidden = !left
        .voxel_state(removed)
        .expect("left boundary voxel")
        .free_faces()
        .contains(AxisMask::X_POS);
    let before_right_x_neg_hidden = !right
        .voxel_state(right_neighbor)
        .expect("right boundary voxel")
        .free_faces()
        .contains(AxisMask::X_NEG);

    let mut stale_left = left.clone();
    let stale_right = right.clone();
    stale_left.set_voxel(removed, false);
    let stale_right_x_neg_hidden_without_propagate = !stale_right
        .voxel_state(right_neighbor)
        .expect("stale right boundary voxel")
        .free_faces()
        .contains(AxisMask::X_NEG);

    let mut updated_left = left;
    let mut updated_right = right;
    updated_left.set_voxel(removed, false);
    updated_left.propagate_voxel_change(&mut updated_right, removed, origin_shift);
    let updated_right_x_neg_exposed = updated_right
        .voxel_state(right_neighbor)
        .expect("updated right boundary voxel")
        .free_faces()
        .contains(AxisMask::X_NEG);
    let updated_left_inner_x_pos_exposed = updated_left
        .voxel_state(left_inner)
        .expect("updated left inner voxel")
        .free_faces()
        .contains(AxisMask::X_POS);
    let removed_voxel_empty = updated_left
        .voxel_state(removed)
        .is_some_and(VoxelState::is_empty);
    let passed = before_left_x_pos_hidden
        && before_right_x_neg_hidden
        && stale_right_x_neg_hidden_without_propagate
        && updated_right_x_neg_exposed
        && updated_left_inner_x_pos_exposed
        && removed_voxel_empty;
    BoundaryEditValidation {
        passed,
        before_left_x_pos_hidden,
        before_right_x_neg_hidden,
        stale_right_x_neg_hidden_without_propagate,
        updated_right_x_neg_exposed,
        updated_left_inner_x_pos_exposed,
        removed_voxel_empty,
    }
}

#[derive(Debug)]
struct CcdValidation {
    passed: bool,
    start_y: Real,
    start_vy: Real,
    after_one_step_y: Real,
    after_one_step_vy: Real,
    final_y: Real,
    final_vy: Real,
    min_y: Real,
    steps: usize,
    floor_top: Real,
}

fn validate_ccd() -> CcdValidation {
    const VALIDATION_STEPS: usize = 8;
    let mut world = configured_world();
    insert_terrain(&mut world);
    let start_y = 40.0;
    let start_vy = -6_000.0;
    let (ball, _) = world.insert(
        ball_body_builder(Vector::new(16.0, start_y, 16.0)).linvel(Vector::new(0.0, start_vy, 0.0)),
        ball_collider_builder(),
    );

    world.step();
    let after_one_step_y = world.bodies[ball].translation().y;
    let after_one_step_vy = world.bodies[ball].linvel().y;
    let mut min_y = after_one_step_y;
    for _ in 1..VALIDATION_STEPS {
        world.step();
        min_y = min_y.min(world.bodies[ball].translation().y);
    }
    let final_y = world.bodies[ball].translation().y;
    let final_vy = world.bodies[ball].linvel().y;
    let floor_top = FLOOR_LAYERS as Real;
    let passed = min_y >= floor_top + BALL_RADIUS - 0.1;
    CcdValidation {
        passed,
        start_y,
        start_vy,
        after_one_step_y,
        after_one_step_vy,
        final_y,
        final_vy,
        min_y,
        steps: VALIDATION_STEPS,
        floor_top,
    }
}

#[derive(Debug)]
struct RollingValidation {
    passed: bool,
    steps: usize,
    seam_crossings: usize,
    start_x: Real,
    final_x: Real,
    backward_steps: usize,
    seam_stalls: usize,
    max_abs_vy: Real,
    max_abs_delta_vx: Real,
    max_center_height_error: Real,
}

fn validate_flat_rolling() -> RollingValidation {
    const STEPS: usize = 480;
    const START_X: Real = 4.0;
    const START_VX: Real = 4.0;
    let floor_top = FLOOR_LAYERS as Real;
    let expected_center_y = floor_top + BALL_RADIUS;
    let mut world = configured_world();
    insert_terrain(&mut world);
    let (ball, _) = world.insert(
        RigidBodyBuilder::dynamic()
            .translation(Vector::new(START_X, expected_center_y + 0.005, 16.0))
            .linvel(Vector::new(START_VX, 0.0, 0.0))
            .angvel(Vector::new(0.0, 0.0, -START_VX / BALL_RADIUS))
            .linear_damping(0.0)
            .angular_damping(0.0)
            .ccd_enabled(true)
            .can_sleep(false),
        ball_collider_builder().restitution(0.0),
    );

    let mut previous_x = START_X;
    let mut previous_vx = START_VX;
    let mut previous_cell = START_X.floor() as i32;
    let mut seam_crossings = 0;
    let mut backward_steps = 0;
    let mut seam_stalls = 0;
    let mut max_abs_vy: Real = 0.0;
    let mut max_abs_delta_vx: Real = 0.0;
    let mut max_center_height_error: Real = 0.0;

    for step in 0..STEPS {
        world.step();
        let body = &world.bodies[ball];
        let position = body.translation();
        let velocity = body.linvel();
        if position.x + 1.0e-5 < previous_x {
            backward_steps += 1;
        }
        let cell = position.x.floor() as i32;
        if cell > previous_cell {
            seam_crossings += (cell - previous_cell) as usize;
        }
        if step >= 30 {
            if position.x - previous_x < START_VX * DT * 0.5 {
                seam_stalls += 1;
            }
            max_abs_vy = max_abs_vy.max(velocity.y.abs());
            max_abs_delta_vx = max_abs_delta_vx.max((velocity.x - previous_vx).abs());
            max_center_height_error =
                max_center_height_error.max((position.y - expected_center_y).abs());
        }
        previous_x = position.x;
        previous_vx = velocity.x;
        previous_cell = cell;
    }

    let final_x = world.bodies[ball].translation().x;
    let passed = backward_steps == 0
        && seam_crossings >= 10
        && seam_stalls == 0
        && final_x > START_X + 10.0
        && max_abs_vy < 0.1
        && max_abs_delta_vx < 0.1
        && max_center_height_error < 0.1;
    RollingValidation {
        passed,
        steps: STEPS,
        seam_crossings,
        start_x: START_X,
        final_x,
        backward_steps,
        seam_stalls,
        max_abs_vy,
        max_abs_delta_vx,
        max_center_height_error,
    }
}

#[derive(Debug)]
struct SleepValidation {
    passed: bool,
    sleeping_step: Option<usize>,
    final_y: Real,
    final_linear_speed: Real,
    final_angular_speed: Real,
}

fn validate_sleep() -> SleepValidation {
    const MAX_STEPS: usize = 1_800;
    let mut world = configured_world();
    insert_terrain(&mut world);
    let (ball, _) = world.insert(
        ball_body_builder(Vector::new(16.0, 12.0, 16.0)),
        ball_collider_builder(),
    );
    let mut sleeping_step = None;
    for step in 1..=MAX_STEPS {
        world.step();
        if world.bodies[ball].is_sleeping() {
            sleeping_step = Some(step);
            break;
        }
    }
    let body = &world.bodies[ball];
    SleepValidation {
        passed: sleeping_step.is_some(),
        sleeping_step,
        final_y: body.translation().y,
        final_linear_speed: body.linvel().length(),
        final_angular_speed: body.angvel().length(),
    }
}

fn percentile_duration(samples: &[Duration], quantile: f64) -> Duration {
    assert!(!samples.is_empty());
    assert!((0.0..=1.0).contains(&quantile));
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let rank = ((sorted.len() as f64 * quantile).ceil() as usize)
        .saturating_sub(1)
        .min(sorted.len() - 1);
    sorted[rank]
}

fn penetration_stats(world: &PhysicsWorld, threshold: Real) -> (usize, Real) {
    let mut count = 0;
    let mut max_penetration: Real = 0.0;
    for pair in world.narrow_phase.contact_pairs() {
        if let Some((_, contact)) = pair.find_deepest_contact() {
            let depth = (-contact.dist).max(0.0);
            max_penetration = max_penetration.max(depth);
            if depth > threshold {
                count += 1;
            }
        }
    }
    (count, max_penetration)
}

fn resident_set_kib() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let status = fs::read_to_string("/proc/self/status").ok()?;
        let line = status.lines().find(|line| line.starts_with("VmRSS:"))?;
        return line.split_whitespace().nth(1)?.parse().ok();
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

fn memory_delta(before: Option<u64>, after: Option<u64>) -> Option<u64> {
    Some(after?.saturating_sub(before?))
}

fn display_optional(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unavailable".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_fills_two_layers_of_a_32_cubed_logical_brick() {
        let coords = flat_floor_voxels();
        assert_eq!(coords.len(), 32 * 32 * 2);
        assert!(coords.contains(&IVector::new(0, 0, 0)));
        assert!(coords.contains(&IVector::new(31, 1, 31)));
        assert!(!coords.contains(&IVector::new(0, 2, 0)));
    }

    #[test]
    fn percentile_uses_nearest_rank() {
        let samples = (1..=100).map(Duration::from_micros).collect::<Vec<_>>();
        assert_eq!(percentile_duration(&samples, 0.95).as_micros(), 95);
    }

    #[test]
    fn combining_adjacent_bricks_hides_the_shared_faces() {
        let (left, right) = adjacent_floor_shapes(true);
        let left_state = left
            .voxel_state(IVector::new(31, 1, 16))
            .expect("left boundary voxel");
        let right_state = right
            .voxel_state(IVector::new(0, 1, 16))
            .expect("right boundary voxel");
        assert!(!left_state.free_faces().contains(AxisMask::X_POS));
        assert!(!right_state.free_faces().contains(AxisMask::X_NEG));
    }

    #[test]
    fn boundary_edit_propagates_to_the_adjacent_brick() {
        let validation = validate_boundary_edit_neighbor_states();
        assert!(validation.passed, "{validation:?}");
    }
}
