use glam::{IVec3, UVec3, Vec3};
use re_flora_physics::{BrickOccupancy, CapsuleCharacterMove, CollisionWorld, StaticVoxelBrickId};

#[test]
fn replacement_mask_tracks_edits_and_restores_latest_source() {
    let mut world = CollisionWorld::new();
    let id = StaticVoxelBrickId(IVec3::ZERO);
    world.upsert_static_voxel_brick(
        id,
        1,
        BrickOccupancy::from_filled_voxels([UVec3::ONE, UVec3::splat(2)]),
    );
    world.set_deforming_surface_exclusions([IVec3::ONE]);
    assert!(!world
        .static_voxel_brick_occupancy(id)
        .unwrap()
        .contains(UVec3::ONE));
    assert!(world
        .static_voxel_brick_occupancy(id)
        .unwrap()
        .contains(UVec3::splat(2)));
    world.upsert_static_voxel_brick(
        id,
        2,
        BrickOccupancy::from_filled_voxels([UVec3::ONE, UVec3::splat(3)]),
    );
    world.set_deforming_surface_exclusions([]);
    let restored = world.static_voxel_brick_occupancy(id).unwrap();
    assert!(restored.contains(UVec3::ONE));
    assert!(!restored.contains(UVec3::splat(2)));
    assert!(restored.contains(UVec3::splat(3)));
    assert_eq!(world.static_brick_revision(id), Some(2));
}

#[test]
fn capsule_queries_follow_replaced_triangle_surface_and_removal() {
    let mut world = CollisionWorld::new();
    let floor = |y| {
        [
            Vec3::new(-100., y, -100.),
            Vec3::new(-100., y, 100.),
            Vec3::new(100., y, -100.),
            Vec3::new(100., y, 100.),
        ]
    };
    let indices = [[0, 1, 2], [2, 1, 3]];
    let movement = CapsuleCharacterMove {
        center: Vec3::new(0., 10., 0.),
        radius: 0.5,
        half_height: 1.,
        desired_translation: Vec3::new(0., -12., 0.),
        dt: 1. / 60.,
    };
    world.set_deforming_surface(&floor(0.), &indices).unwrap();
    let first = world.move_capsule_character(movement).unwrap();
    assert!(first.translation.y > -9. && first.translation.y < -7.);
    world.set_deforming_surface(&floor(3.), &indices).unwrap();
    let raised = world.move_capsule_character(movement).unwrap();
    assert!((raised.translation.y - first.translation.y - 3.).abs() < 0.01);
    world.set_deforming_surface(&[], &[]).unwrap();
    assert!(
        (world
            .move_capsule_character(movement)
            .unwrap()
            .translation
            .y
            + 12.)
            .abs()
            < 0.001
    );
}
