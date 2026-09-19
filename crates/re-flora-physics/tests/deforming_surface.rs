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

#[test]
fn boxes_move_remove_and_switch_representation_without_stale_queries() {
    use re_flora_physics::DeformingGeometry;
    let mut world = CollisionWorld::new();
    let movement = CapsuleCharacterMove {
        center: Vec3::new(0., 10., 0.),
        radius: 0.5,
        half_height: 1.,
        desired_translation: Vec3::new(0., -12., 0.),
        dt: 1. / 60.,
    };
    let publish = |world: &mut CollisionWorld, center| {
        world.set_deforming_geometry(DeformingGeometry::Boxes {
            centers: &[center],
            half_extents: Vec3::new(10., 0.5, 10.),
        })
    };
    publish(&mut world, Vec3::ZERO).unwrap();
    let first = world.move_capsule_character(movement).unwrap();
    publish(&mut world, Vec3::Y * 2.375).unwrap();
    let moved = world.move_capsule_character(movement).unwrap();
    assert!((moved.translation.y - first.translation.y - 2.375).abs() < 0.01);
    assert!(publish(&mut world, Vec3::splat(f32::NAN)).is_err());
    assert!(
        (world
            .move_capsule_character(movement)
            .unwrap()
            .translation
            .y
            - moved.translation.y)
            .abs()
            < 0.001
    );
    // Same primitive count, different geometry kind must replace the old shape.
    world
        .set_deforming_surface(
            &[
                Vec3::new(-20., 1., -20.),
                Vec3::new(0., 1., 20.),
                Vec3::new(20., 1., -20.),
            ],
            &[[0, 1, 2]],
        )
        .unwrap();
    let triangle = world.move_capsule_character(movement).unwrap();
    assert!((triangle.translation.y - first.translation.y - 0.5).abs() < 0.01);
    publish(&mut world, Vec3::X * 100.).unwrap();
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
    publish(&mut world, Vec3::ZERO).unwrap();
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

#[test]
fn same_size_triangle_topology_change_replaces_and_invalid_update_preserves_surface() {
    let mut world = CollisionWorld::new();
    let vertices = [
        Vec3::new(-20., 0., -20.),
        Vec3::new(0., 0., 20.),
        Vec3::new(20., 0., -20.),
        Vec3::new(-20., 3., -20.),
        Vec3::new(0., 3., 20.),
        Vec3::new(20., 3., -20.),
    ];
    let movement = CapsuleCharacterMove {
        center: Vec3::Y * 10.,
        radius: 0.5,
        half_height: 1.,
        desired_translation: Vec3::Y * -12.,
        dt: 1. / 60.,
    };
    world
        .set_deforming_surface(&vertices, &[[0, 1, 2]])
        .unwrap();
    let first = world.move_capsule_character(movement).unwrap();
    world
        .set_deforming_surface(&vertices, &[[3, 4, 5]])
        .unwrap();
    let raised = world.move_capsule_character(movement).unwrap();
    assert!((raised.translation.y - first.translation.y - 3.).abs() < 0.01);
    assert!(world
        .set_deforming_surface(&vertices, &[[3, 4, 6]])
        .is_err());
    assert!(
        (world
            .move_capsule_character(movement)
            .unwrap()
            .translation
            .y
            - raised.translation.y)
            .abs()
            < 0.001
    );
}

#[test]
fn rigid_body_contacts_survive_repeated_in_place_geometry_updates() {
    use re_flora_physics::{DeformingGeometry, DynamicBodyDesc};
    for boxes in [false, true] {
        let mut world = CollisionWorld::new();
        let body = world
            .spawn_dynamic_body(DynamicBodyDesc::sphere(Vec3::new(0., 5., 0.), 0.5))
            .unwrap();
        for frame in 0..480 {
            // Continuously update the geometry, including a slow translation,
            // rather than testing only a static custom-shape insertion.
            let y = if frame < 120 {
                frame as f32 / 240.
            } else {
                0.5
            };
            if boxes {
                world
                    .set_deforming_geometry(DeformingGeometry::Boxes {
                        centers: &[Vec3::new(0., y - 0.5, 0.)],
                        half_extents: Vec3::new(10., 0.5, 10.),
                    })
                    .unwrap();
            } else {
                world
                    .set_deforming_surface(
                        &[
                            Vec3::new(-20., y, -20.),
                            Vec3::new(0., y, 20.),
                            Vec3::new(20., y, -20.),
                        ],
                        &[[0, 1, 2]],
                    )
                    .unwrap();
            }
            world.advance(1. / 120.);
        }
        let state = world.dynamic_body_state(body).unwrap();
        assert!(
            (state.position.y - 1.).abs() < 0.08,
            "boxes={boxes}: {state:?}"
        );
        assert!(state.linear_velocity.length() < 0.1);
    }
}
