//! Deterministic diagnostic using the game's four-voxel apple and material.
use glam::{IVec3, UVec3, Vec3, Vec3Swizzles};
use re_flora_physics::{
    BrickOccupancy, CollisionWorld, DynamicBodyDesc, DynamicColliderShape, StaticVoxelBrickId,
};

fn apple(position: Vec3) -> DynamicBodyDesc {
    let mut points = Vec::new();
    for x in -2..2 {
        for y in -2..2 {
            for z in -2..2 {
                let cell = IVec3::new(x, y, z);
                if (cell.as_vec3() + Vec3::splat(0.5)).length_squared() > 4.0 {
                    continue;
                }
                for dx in 0..=1 {
                    for dy in 0..=1 {
                        for dz in 0..=1 {
                            points.push(cell + IVec3::new(dx, dy, dz));
                        }
                    }
                }
            }
        }
    }
    points.sort_unstable_by_key(|p| (p.x, p.y, p.z));
    points.dedup();
    let mut desc = DynamicBodyDesc::sphere(position, 2.0);
    desc.collider = DynamicColliderShape::ConvexHull {
        points: points.into_iter().map(IVec3::as_vec3).collect(),
    };
    desc.linear_velocity = Vec3::new(7.0, -2.0, 7.0);
    desc.angular_velocity = Vec3::new(2.5, 2.5, -2.5);
    desc.friction = 0.82;
    desc.restitution = 0.12;
    desc.contact_skin = 0.15;
    desc.linear_damping = 0.06;
    desc.angular_damping = 0.10;
    desc.additional_solver_iterations = 4;
    desc
}

#[test]
fn apple_inside_contact_skin_keeps_a_support_polygon() {
    let mut world = CollisionWorld::new();
    world
        .set_gravity(Vec3::new(0.0, -9.8 * 256.0, 0.0))
        .unwrap();
    let floor = BrickOccupancy::from_filled_voxels(
        (0..2).flat_map(|y| (0..32).flat_map(move |z| (0..32).map(move |x| UVec3::new(x, y, z)))),
    );
    world.upsert_static_voxel_brick(StaticVoxelBrickId(IVec3::ZERO), 1, floor);
    let mut desc = apple(Vec3::new(16.0, 4.1, 16.0));
    desc.linear_velocity = Vec3::ZERO;
    desc.angular_velocity = Vec3::ZERO;
    let body = world.spawn_dynamic_body(desc).unwrap();
    world.advance(1.0 / 120.0);
    let contacts = world.dynamic_body_diagnostics(body).unwrap();
    assert!(
        contacts.solver_contacts >= 3,
        "skin contact lost its supporting polygon: {contacts:?}"
    );
}

fn floor_with_hole(origin: IVec3, hole: Option<Vec3>) -> BrickOccupancy {
    BrickOccupancy::from_filled_voxels(
        (0..2)
            .flat_map(|y| (0..32).flat_map(move |z| (0..32).map(move |x| UVec3::new(x, y, z))))
            .filter(|voxel| {
                hole.is_none_or(|center| {
                    let world = (origin + voxel.as_ivec3()).as_vec3() + Vec3::splat(0.5);
                    (world.x - center.x).abs() > 5.0 || (world.z - center.z).abs() > 5.0
                })
            }),
    )
}

#[test]
fn apple_settles_without_persistent_ground_jitter_and_wakes_when_support_is_edited() {
    for (origin, local_start) in [
        (IVec3::ZERO, Vec3::new(16.0, 28.0, 16.0)),
        (IVec3::ZERO, Vec3::new(32.0, 28.0, 32.0)),
        (IVec3::new(256, 96, 256), Vec3::new(32.0, 28.0, 16.0)),
    ] {
        let mut world = CollisionWorld::new();
        world
            .set_gravity(Vec3::new(0.0, -9.8 * 256.0, 0.0))
            .unwrap();
        let mut bricks = Vec::new();
        for z in 0..2 {
            for x in 0..2 {
                let brick_origin = origin + IVec3::new(x * 32, 0, z * 32);
                let id = StaticVoxelBrickId(brick_origin / 32);
                world.upsert_static_voxel_brick(id, 1, floor_with_hole(brick_origin, None));
                bricks.push((id, brick_origin));
            }
        }
        let start = origin.as_vec3() + local_start;
        let body = world.spawn_dynamic_body(apple(start)).unwrap();
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        let mut sleeping_steps = 0;
        let mut highest_upward_velocity = 0.0_f32;
        let mut largest_rotation = 0.0_f32;
        let mut rest_pose = None;
        for step in 1..=30 * 120 {
            assert_eq!(world.advance(1.0 / 120.0).steps, 1);
            let state = world.dynamic_body_state(body).unwrap();
            highest_upward_velocity = highest_upward_velocity.max(state.linear_velocity.y);
            largest_rotation =
                largest_rotation.max(state.rotation.angle_between(glam::Quat::IDENTITY));
            if step > 20 * 120 {
                min = min.min(state.position);
                max = max.max(state.position);
                sleeping_steps += usize::from(state.sleeping);
                let pose = (state.position, state.rotation);
                assert_eq!(
                    *rest_pose.get_or_insert(pose),
                    pose,
                    "pose keeps moving at step {step}: {state:?}"
                );
            }
        }
        let resting = world.dynamic_body_state(body).unwrap();
        println!("[FRUIT_GROUND] origin={origin:?} start={local_start:?} final_10s_range={:?} sleeping_steps={sleeping_steps}/1200", max-min);
        assert!(
            (max - min).max_element() < 0.01,
            "persistent ground motion: {:?}",
            max - min
        );
        assert_eq!(
            sleeping_steps, 1200,
            "resting apple never settles into natural sleep"
        );
        assert!(
            resting.position.y > origin.y as f32 + 4.0,
            "apple penetrated the floor: {resting:?}"
        );
        assert!(
            resting.position.y < origin.y as f32 + 4.2,
            "apple floated above the contact skin: {resting:?}"
        );
        assert!(
            highest_upward_velocity > 1.0,
            "normal impact bounce was lost"
        );
        assert!(largest_rotation > 0.1, "apple no longer tumbles");
        assert!(
            (resting.position - start).xz().length() > 0.1,
            "apple no longer rolls/slides"
        );

        // Keep each collider and its other voxels. Only excavate the support through
        // the production incremental edit path, including a cross-brick edit.
        for (id, brick_origin) in bricks {
            world.upsert_static_voxel_brick(
                id,
                2,
                floor_with_hole(brick_origin, Some(resting.position)),
            );
        }
        assert!(
            !world.dynamic_body_state(body).unwrap().sleeping,
            "support edit failed to wake natural sleep"
        );
        for _ in 0..24 {
            world.advance(1.0 / 120.0);
        }
        let falling = world.dynamic_body_state(body).unwrap();
        assert!(
            falling.position.y < resting.position.y - 10.0 && falling.linear_velocity.y < -20.0,
            "apple did not fall after excavation: {falling:?}"
        );
    }
}

#[test]
fn apple_moves_down_a_voxel_slope() {
    let mut world = CollisionWorld::new();
    world
        .set_gravity(Vec3::new(0.0, -9.8 * 256.0, 0.0))
        .unwrap();
    for x_brick in 0..2 {
        let occupancy = BrickOccupancy::from_filled_voxels((0..32).flat_map(|x| {
            let height = (28_i32 - (x + x_brick * 32) as i32).max(2) as u32;
            (0..height).flat_map(move |y| (0..32).map(move |z| UVec3::new(x, y, z)))
        }));
        world.upsert_static_voxel_brick(
            StaticVoxelBrickId(IVec3::new(x_brick as i32, 0, 0)),
            1,
            occupancy,
        );
    }
    let mut desc = apple(Vec3::new(6.0, 27.0, 16.0));
    desc.linear_velocity = Vec3::ZERO;
    desc.angular_velocity = Vec3::ZERO;
    let body = world.spawn_dynamic_body(desc).unwrap();
    let mut max_angular_speed = 0.0_f32;
    for _ in 0..2 * 120 {
        world.advance(1.0 / 120.0);
        max_angular_speed = max_angular_speed.max(
            world
                .dynamic_body_state(body)
                .unwrap()
                .angular_velocity
                .length(),
        );
    }
    let state = world.dynamic_body_state(body).unwrap();
    assert!(
        state.position.x > 16.0 && state.position.y < 20.0,
        "apple stopped on a descending slope: {state:?}"
    );
    assert!(max_angular_speed > 0.5, "slope tumbling disappeared");
}

#[test]
fn apples_still_collide_and_bounce_against_each_other() {
    let mut world = CollisionWorld::new();
    world.set_gravity(Vec3::ZERO).unwrap();
    let mut left = apple(Vec3::new(8.0, 8.0, 16.0));
    left.linear_velocity = Vec3::X * 12.0;
    left.angular_velocity = Vec3::ZERO;
    let mut right = left.clone();
    right.position.x = 20.0;
    right.linear_velocity = -left.linear_velocity;
    let left = world.spawn_dynamic_body(left).unwrap();
    let right = world.spawn_dynamic_body(right).unwrap();
    for _ in 0..120 {
        world.advance(1.0 / 120.0);
    }
    let a = world.dynamic_body_state(left).unwrap();
    let b = world.dynamic_body_state(right).unwrap();
    assert!(
        a.linear_velocity.x < 0.0 && b.linear_velocity.x > 0.0,
        "apples did not bounce: {a:?}, {b:?}"
    );
    assert!(
        b.position.x - a.position.x > 4.0,
        "apple colliders interpenetrated"
    );
}

#[test]
fn tilted_apple_ground_replay() {
    let mut world = CollisionWorld::new();
    world
        .set_gravity(Vec3::new(0.0, -9.8 * 256.0, 0.0))
        .unwrap();
    let floor = BrickOccupancy::from_filled_voxels(
        (0..32).flat_map(|x| (0..32).map(move |z| UVec3::new(x, 10, z))),
    );
    world.upsert_static_voxel_brick(StaticVoxelBrickId(IVec3::new(8, 3, 7)), 1, floor);
    let mut desc = apple(Vec3::new(265.72668457, 109.43418121, 251.00657654));
    desc.rotation = glam::Quat::from_xyzw(0.35835579, 0.10532805, -0.28461754, -0.88288164);
    desc.linear_velocity = Vec3::new(0.23035912, -0.02919560, -0.08298521);
    desc.angular_velocity = Vec3::new(-0.03494840, -0.00000006, -0.09701693);
    let body = world.spawn_dynamic_body(desc).unwrap();
    for _ in 0..30 * 120 {
        world.advance(1.0 / 120.0);
    }
    let state = world.dynamic_body_state(body).unwrap();
    println!("tilted replay {state:?}");
    assert!(state.sleeping, "tilted apple does not settle: {state:?}");
}

mod real_ground {
    include!("fixtures/fruit_ground_scene.rs");
}

#[test]
fn tilted_apple_settles_on_recorded_contree_terrain() {
    for fps in [30, 60, 144] {
        let mut world = CollisionWorld::new();
        world
            .set_gravity(Vec3::new(0.0, -9.8 * 256.0, 0.0))
            .unwrap();
        for (id, runs) in real_ground::BRICKS {
            let occupancy =
                BrickOccupancy::from_filled_voxels(runs.iter().flat_map(|&(start, len)| {
                    (start..start + len).map(|v| {
                        UVec3::new((v % 32) as u32, (v / 32 % 32) as u32, (v / 1024) as u32)
                    })
                }));
            world.upsert_static_voxel_brick(
                StaticVoxelBrickId(IVec3::from_array(*id)),
                1,
                occupancy,
            );
        }
        let mut desc = apple(Vec3::new(265.72668457, 109.43418121, 251.00657654));
        desc.rotation = glam::Quat::from_xyzw(0.35835579, 0.10532805, -0.28461754, -0.88288164);
        desc.linear_velocity = Vec3::new(0.23035912, -0.02919560, -0.08298521);
        desc.angular_velocity = Vec3::new(-0.03494840, -0.00000006, -0.09701693);
        let body = world.spawn_dynamic_body(desc).unwrap();
        let mut rest = None;
        for frame in 0..30 * fps {
            let step = world.advance(1.0 / fps as f32);
            assert_eq!(step.dropped_seconds, 0.0);
            let state = world.dynamic_body_state(body).unwrap();
            assert!(
                (259.0..285.0).contains(&state.position.x)
                    && (227.0..253.0).contains(&state.position.z),
                "replay left its captured brick: {state:?}"
            );
            assert!(
                state.position.y > 105.0,
                "recorded apple escaped the ground: {state:?}"
            );
            if frame >= 20 * fps {
                assert!(
                    state.sleeping,
                    "recorded tilted fruit does not settle at {fps} Hz: {state:?}"
                );
                assert_eq!(
                    *rest.get_or_insert(state),
                    state,
                    "resting pose changes at {fps} Hz"
                );
            }
        }
    }
}
