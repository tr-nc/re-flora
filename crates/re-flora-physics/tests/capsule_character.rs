use glam::{IVec3, UVec3, Vec3};
use re_flora_physics::{
    BrickOccupancy, CapsuleCharacterMove, CapsuleCharacterMoveError, CollisionWorld,
    DynamicBodyDesc, StaticVoxelBrickId, CAPSULE_CHARACTER_COLLISION_OFFSET,
    DEFAULT_FIXED_STEP_SECONDS, STATIC_VOXEL_BRICK_DIM,
};

const DT: f32 = 1.0 / 60.0;
const RADIUS: f32 = 0.5;
const HALF_HEIGHT: f32 = 1.0;

fn occupancy_where(mut filled: impl FnMut(UVec3) -> bool) -> BrickOccupancy {
    let mut voxels = Vec::new();
    for z in 0..STATIC_VOXEL_BRICK_DIM {
        for y in 0..STATIC_VOXEL_BRICK_DIM {
            for x in 0..STATIC_VOXEL_BRICK_DIM {
                let voxel = UVec3::new(x, y, z);
                if filled(voxel) {
                    voxels.push(voxel);
                }
            }
        }
    }
    BrickOccupancy::from_filled_voxels(voxels)
}

fn layer_floor(height: u32) -> BrickOccupancy {
    occupancy_where(|voxel| voxel.y < height)
}

fn capsule_move(center: Vec3, desired_translation: Vec3) -> CapsuleCharacterMove {
    CapsuleCharacterMove {
        center,
        radius: RADIUS,
        half_height: HALF_HEIGHT,
        desired_translation,
        dt: DT,
    }
}

fn standing_center_y(surface_y: f32) -> f32 {
    surface_y + HALF_HEIGHT + RADIUS + CAPSULE_CHARACTER_COLLISION_OFFSET
}

#[test]
fn capsule_lands_on_flat_voxel_ground() {
    let mut world = CollisionWorld::new();
    world.upsert_static_voxel_brick(StaticVoxelBrickId(IVec3::ZERO), 1, layer_floor(1));

    let start = Vec3::new(12.0, 8.0, 12.0);
    let result = world
        .move_capsule_character(capsule_move(start, Vec3::NEG_Y * 10.0))
        .unwrap();
    let final_center = start + result.translation;

    assert!(result.grounded, "result={result:?}");
    assert!(
        (final_center.y - standing_center_y(1.0)).abs() < 1.0e-3,
        "final_center={final_center:?} result={result:?}"
    );
    assert!(
        result
            .collisions
            .iter()
            .any(|collision| collision.normal.y > 0.99),
        "result={result:?}"
    );
}

#[test]
fn capsule_slides_along_a_voxel_wall() {
    let mut world = CollisionWorld::new();
    world.upsert_static_voxel_brick(
        StaticVoxelBrickId(IVec3::ZERO),
        1,
        occupancy_where(|voxel| voxel.y == 0 || (voxel.x == 8 && voxel.y < 24)),
    );

    let start = Vec3::new(4.0, standing_center_y(1.0), 8.0);
    let result = world
        .move_capsule_character(capsule_move(start, Vec3::new(8.0, -0.1, 4.0)))
        .unwrap();

    assert!(result.grounded, "result={result:?}");
    assert!(result.translation.x > 3.2 && result.translation.x < 3.5);
    assert!(result.translation.z > 3.9, "result={result:?}");
    assert!(
        result
            .collisions
            .iter()
            .any(|collision| collision.normal.x < -0.99),
        "result={result:?}"
    );
}

#[test]
fn capsule_stops_below_a_voxel_ceiling() {
    let mut world = CollisionWorld::new();
    world.upsert_static_voxel_brick(
        StaticVoxelBrickId(IVec3::ZERO),
        1,
        occupancy_where(|voxel| voxel.y == 6),
    );

    let start = Vec3::new(12.0, 3.0, 12.0);
    let result = world
        .move_capsule_character(capsule_move(start, Vec3::Y * 5.0))
        .unwrap();
    let final_center = start + result.translation;

    assert!(!result.grounded, "result={result:?}");
    assert!((final_center.y - 4.4).abs() < 1.0e-3, "result={result:?}");
    assert!(
        result
            .collisions
            .iter()
            .any(|collision| collision.normal.y < -0.99),
        "result={result:?}"
    );
}

#[test]
fn capsule_autosteps_one_voxel() {
    let mut world = CollisionWorld::new();
    world.upsert_static_voxel_brick(
        StaticVoxelBrickId(IVec3::ZERO),
        1,
        occupancy_where(|voxel| voxel.y == 0 || (voxel.x >= 8 && voxel.y == 1)),
    );

    let start = Vec3::new(4.0, standing_center_y(1.0), 12.0);
    let result = world
        .move_capsule_character(capsule_move(start, Vec3::new(8.0, -0.1, 0.0)))
        .unwrap();
    let final_center = start + result.translation;

    assert!(result.grounded, "result={result:?}");
    assert!(result.translation.x > 7.9, "result={result:?}");
    assert!(
        (final_center.y - standing_center_y(2.0)).abs() < 1.0e-3,
        "final_center={final_center:?} result={result:?}"
    );
}

#[test]
fn capsule_autosteps_sixteen_voxels() {
    let mut world = CollisionWorld::new();
    world.upsert_static_voxel_brick(
        StaticVoxelBrickId(IVec3::ZERO),
        1,
        occupancy_where(|voxel| voxel.y == 0 || (voxel.x >= 8 && voxel.y <= 16)),
    );

    let start = Vec3::new(4.0, standing_center_y(1.0), 12.0);
    let result = world
        .move_capsule_character(capsule_move(start, Vec3::new(8.0, -0.1, 0.0)))
        .unwrap();
    let final_center = start + result.translation;

    assert!(result.grounded, "result={result:?}");
    assert!(result.translation.x > 7.9, "result={result:?}");
    assert!(
        (final_center.y - standing_center_y(17.0)).abs() < 1.0e-3,
        "final_center={final_center:?} result={result:?}"
    );
}

#[test]
fn capsule_does_not_autostep_seventeen_voxels() {
    let mut world = CollisionWorld::new();
    world.upsert_static_voxel_brick(
        StaticVoxelBrickId(IVec3::ZERO),
        1,
        occupancy_where(|voxel| voxel.y == 0 || (voxel.x >= 8 && voxel.y <= 17)),
    );

    let start = Vec3::new(4.0, standing_center_y(1.0), 12.0);
    let result = world
        .move_capsule_character(capsule_move(start, Vec3::new(8.0, -0.1, 0.0)))
        .unwrap();
    let final_center = start + result.translation;

    assert!(result.grounded, "result={result:?}");
    assert!(result.translation.x < 4.0, "result={result:?}");
    assert!(
        (final_center.y - standing_center_y(1.0)).abs() < 1.0e-3,
        "final_center={final_center:?} result={result:?}"
    );
}

#[test]
fn capsule_snaps_down_sixteen_voxels() {
    let mut world = CollisionWorld::new();
    world.upsert_static_voxel_brick(
        StaticVoxelBrickId(IVec3::ZERO),
        1,
        occupancy_where(|voxel| voxel.y == 0 || (voxel.x < 8 && voxel.y <= 16)),
    );

    let start = Vec3::new(4.0, standing_center_y(17.0), 12.0);
    let result = world
        .move_capsule_character(capsule_move(start, Vec3::new(8.0, -0.1, 0.0)))
        .unwrap();
    let final_center = start + result.translation;

    assert!(result.grounded, "result={result:?}");
    assert!(result.translation.x > 7.9, "result={result:?}");
    assert!(
        (final_center.y - standing_center_y(1.0)).abs() < 1.0e-3,
        "final_center={final_center:?} result={result:?}"
    );
}

#[test]
fn capsule_climbs_a_voxel_stair_slope() {
    let mut world = CollisionWorld::new();
    world.upsert_static_voxel_brick(
        StaticVoxelBrickId(IVec3::ZERO),
        1,
        occupancy_where(|voxel| {
            voxel.y == 0
                || (voxel.x >= 8 && voxel.y == 1)
                || (voxel.x >= 12 && voxel.y == 2)
                || (voxel.x >= 16 && voxel.y == 3)
        }),
    );

    let mut center = Vec3::new(4.0, standing_center_y(1.0), 12.0);
    for _ in 0..4 {
        let result = world
            .move_capsule_character(capsule_move(center, Vec3::new(4.0, -0.1, 0.0)))
            .unwrap();
        assert!(result.grounded, "center={center:?} result={result:?}");
        assert!(result.translation.x > 3.9, "result={result:?}");
        center += result.translation;
    }

    assert!(
        (center.y - standing_center_y(4.0)).abs() < 1.0e-3,
        "center={center:?}"
    );
}

#[test]
fn capsule_crosses_a_combined_voxel_brick_seam() {
    let mut world = CollisionWorld::new();
    world.upsert_static_voxel_brick(StaticVoxelBrickId(IVec3::ZERO), 1, layer_floor(1));
    world.upsert_static_voxel_brick(StaticVoxelBrickId(IVec3::X), 1, layer_floor(1));

    let start = Vec3::new(28.0, standing_center_y(1.0), 12.0);
    let result = world
        .move_capsule_character(capsule_move(start, Vec3::new(8.0, -0.1, 0.0)))
        .unwrap();

    assert!(result.grounded, "result={result:?}");
    assert!(result.translation.x > 7.99, "result={result:?}");
    assert!(result.translation.y.abs() < 1.0e-3, "result={result:?}");
}

#[test]
fn terrain_insert_remove_and_shape_edit_refresh_character_queries() {
    let id = StaticVoxelBrickId(IVec3::ZERO);
    let mut world = CollisionWorld::new();
    let start = Vec3::new(4.0, standing_center_y(1.0), 12.0);
    let movement = Vec3::new(20.0, -0.1, 0.0);

    world.upsert_static_voxel_brick(
        id,
        1,
        occupancy_where(|voxel| voxel.y == 0 || (voxel.x == 8 && voxel.y < 24)),
    );
    let near_wall = world
        .move_capsule_character(capsule_move(start, movement))
        .unwrap();
    assert!(near_wall.translation.x < 3.5, "result={near_wall:?}");

    world.upsert_static_voxel_brick(
        id,
        2,
        occupancy_where(|voxel| voxel.y == 0 || (voxel.x == 16 && voxel.y < 24)),
    );
    let moved_wall = world
        .move_capsule_character(capsule_move(start, movement))
        .unwrap();
    assert!(
        moved_wall.translation.x > 11.2 && moved_wall.translation.x < 11.5,
        "result={moved_wall:?}"
    );
    assert_eq!(world.advance(DEFAULT_FIXED_STEP_SECONDS).steps, 1);
    let moved_wall_after_step = world
        .move_capsule_character(capsule_move(start, movement))
        .unwrap();
    assert!(
        moved_wall_after_step.translation.x > 11.2 && moved_wall_after_step.translation.x < 11.5,
        "result={moved_wall_after_step:?}"
    );

    world.remove_static_voxel_brick(id, 3);
    let removed = world
        .move_capsule_character(capsule_move(start, movement))
        .unwrap();
    assert_eq!(removed.translation, movement);

    world.upsert_static_voxel_brick(
        id,
        4,
        occupancy_where(|voxel| voxel.y == 0 || (voxel.x == 8 && voxel.y < 24)),
    );
    let reinserted = world
        .move_capsule_character(capsule_move(start, movement))
        .unwrap();
    assert!(reinserted.translation.x < 3.5, "result={reinserted:?}");
}

#[test]
fn dynamic_bodies_do_not_block_the_terrain_only_capsule_query() {
    let mut world = CollisionWorld::new();
    world
        .spawn_dynamic_body(DynamicBodyDesc::sphere(Vec3::new(4.0, 2.0, 0.0), 2.0))
        .unwrap();

    let movement = Vec3::X * 8.0;
    let result = world
        .move_capsule_character(capsule_move(Vec3::ZERO, movement))
        .unwrap();

    assert_eq!(result.translation, movement);
    assert!(result.collisions.is_empty());
}

#[test]
fn capsule_move_rejects_invalid_dimensions_and_timestep() {
    let mut world = CollisionWorld::new();
    let mut movement = capsule_move(Vec3::ZERO, Vec3::ZERO);
    movement.radius = 0.0;
    assert_eq!(
        world.move_capsule_character(movement),
        Err(CapsuleCharacterMoveError::NonPositive {
            field: "radius",
            value: 0.0,
        })
    );

    movement = capsule_move(Vec3::ZERO, Vec3::ZERO);
    movement.half_height = -1.0;
    assert_eq!(
        world.move_capsule_character(movement),
        Err(CapsuleCharacterMoveError::Negative {
            field: "half_height",
            value: -1.0,
        })
    );

    movement = capsule_move(Vec3::ZERO, Vec3::ZERO);
    movement.dt = f32::NAN;
    assert_eq!(
        world.move_capsule_character(movement),
        Err(CapsuleCharacterMoveError::NonFinite { field: "dt" })
    );
}
