use glam::{IVec3, UVec3, Vec3};
use re_flora_physics::{
    BrickOccupancy, CollisionWorld, DynamicBodyDesc, StaticVoxelBrickId,
    StaticVoxelBrickUpdate, DEFAULT_FIXED_STEP_SECONDS, STATIC_VOXEL_BRICK_DIM,
};

fn checkerboard_chunks(odd_chunks: bool) -> BrickOccupancy {
    BrickOccupancy::from_filled_voxels((0..STATIC_VOXEL_BRICK_DIM).flat_map(move |z| {
        (0..STATIC_VOXEL_BRICK_DIM).flat_map(move |y| {
            (0..STATIC_VOXEL_BRICK_DIM).filter_map(move |x| {
                let chunk_is_odd = (x / 8 + y / 8 + z / 8) % 2 != 0;
                (chunk_is_odd == odd_chunks).then_some(UVec3::new(x, y, z))
            })
        })
    }))
}

fn two_layer_floor() -> BrickOccupancy {
    BrickOccupancy::from_filled_voxels((0..2).flat_map(|y| {
        (0..STATIC_VOXEL_BRICK_DIM).flat_map(move |z| {
            (0..STATIC_VOXEL_BRICK_DIM).map(move |x| UVec3::new(x, y, z))
        })
    }))
}

#[test]
fn repeated_large_terrain_rewrites_preserve_stepping_and_brick_seams() {
    let left = StaticVoxelBrickId(IVec3::ZERO);
    let right = StaticVoxelBrickId(IVec3::X);
    let mut world = CollisionWorld::new();

    world.upsert_static_voxel_brick(left, 1, checkerboard_chunks(false));
    world.upsert_static_voxel_brick(right, 1, checkerboard_chunks(true));

    let mut edit_witness = DynamicBodyDesc::sphere(Vec3::new(32.0, 40.0, 16.0), 1.0);
    edit_witness.can_sleep = false;
    let edit_witness = world.spawn_dynamic_body(edit_witness).unwrap();
    assert_eq!(world.advance(DEFAULT_FIXED_STEP_SECONDS).steps, 1);

    let mut odd_chunks = false;
    for revision in 2..=7 {
        odd_chunks = !odd_chunks;
        for (id, odd) in [(left, odd_chunks), (right, !odd_chunks)] {
            assert_eq!(
                world.upsert_static_voxel_brick(id, revision, checkerboard_chunks(odd)),
                StaticVoxelBrickUpdate::Applied {
                    changed_voxels: (STATIC_VOXEL_BRICK_DIM as usize).pow(3),
                    collider_present: true,
                }
            );
        }

        assert_eq!(world.advance(DEFAULT_FIXED_STEP_SECONDS).steps, 1);
        let state = world.dynamic_body_state(edit_witness).unwrap();
        assert!(state.position.is_finite());
        assert!(state.linear_velocity.is_finite());
    }

    assert!(matches!(
        world.remove_static_voxel_brick(right, 8),
        StaticVoxelBrickUpdate::Applied {
            collider_present: false,
            ..
        }
    ));
    assert_eq!(world.advance(DEFAULT_FIXED_STEP_SECONDS).steps, 1);
    assert_eq!(world.static_brick_count(), 1);

    assert!(matches!(
        world.upsert_static_voxel_brick(right, 9, checkerboard_chunks(!odd_chunks)),
        StaticVoxelBrickUpdate::Applied {
            collider_present: true,
            ..
        }
    ));
    assert_eq!(world.advance(DEFAULT_FIXED_STEP_SECONDS).steps, 1);
    assert_eq!(world.static_brick_count(), 2);

    world.upsert_static_voxel_brick(left, 10, two_layer_floor());
    world.upsert_static_voxel_brick(right, 10, two_layer_floor());
    assert_eq!(world.advance(DEFAULT_FIXED_STEP_SECONDS).steps, 1);
    assert!(world.remove_dynamic_body(edit_witness));

    let mut rolling = DynamicBodyDesc::sphere(Vec3::new(20.0, 4.005, 16.0), 2.0);
    rolling.linear_velocity = Vec3::new(4.0, 0.0, 0.0);
    rolling.angular_velocity = Vec3::new(0.0, 0.0, -2.0);
    rolling.linear_damping = 0.0;
    rolling.angular_damping = 0.0;
    rolling.restitution = 0.0;
    rolling.can_sleep = false;
    let rolling = world.spawn_dynamic_body(rolling).unwrap();

    let mut max_abs_vertical_velocity_near_seam = 0.0_f32;
    for _ in 0..720 {
        assert_eq!(world.advance(DEFAULT_FIXED_STEP_SECONDS).steps, 1);
        let state = world.dynamic_body_state(rolling).unwrap();
        if (28.0..=36.0).contains(&state.position.x) {
            max_abs_vertical_velocity_near_seam =
                max_abs_vertical_velocity_near_seam.max(state.linear_velocity.y.abs());
        }
    }

    let state = world.dynamic_body_state(rolling).unwrap();
    assert!(state.position.x > 40.0, "final state: {state:?}");
    assert!(
        max_abs_vertical_velocity_near_seam < 0.02,
        "vertical seam kick after rewrites: {max_abs_vertical_velocity_near_seam}"
    );
}
