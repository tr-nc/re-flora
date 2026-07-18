use rapier3d::prelude::*;
use std::{
    fs,
    time::{Duration, Instant},
};

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
    let ball_handles = insert_ball_grid(&mut world);
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

    let all_valid = edit_removed_filled_voxel && ccd.passed && rolling.passed && sleep.passed;
    let memory_bytes = memory_delta(rss_before_kib, rss_after_build_kib).map(|kib| kib * 1024);
    println!(
        "{PREFIX} brick_dim=32 voxel_size=1 sphere_radius=2 bodies=100 steps=600 dt=0.008333 \
         build_ms={:.6} edit_ms={:.6} step_total_ms={:.6} step_avg_us={:.3} memory_bytes={} \
         penetrations={} seam_stalls={} step_p95_us={} max_penetration={:.6} ccd_pass={} \
         sleep_pass={} rolling_pass={} sleeping={} escaped={} rapier_version=0.34.0 \
         parry_version=0.29.0 features=default_dim3_f32_std",
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

fn insert_ball_grid(world: &mut PhysicsWorld) -> Vec<RigidBodyHandle> {
    let mut handles = Vec::with_capacity(BALL_COUNT);
    for layer in 0..4 {
        for z in 0..5 {
            for x in 0..5 {
                let position = Vector::new(
                    4.0 + x as Real * 6.0,
                    7.0 + layer as Real * 5.0,
                    4.0 + z as Real * 6.0,
                );
                let (body, _) = world.insert(ball_body_builder(position), ball_collider_builder());
                handles.push(body);
            }
        }
    }
    assert_eq!(handles.len(), BALL_COUNT);
    handles
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
}
