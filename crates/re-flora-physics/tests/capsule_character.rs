use glam::{IVec3, UVec3, Vec3};
use re_flora_physics::{
    BrickOccupancy, CapsuleCharacterMove, CapsuleCharacterMoveError, CollisionWorld,
    DynamicBodyDesc, StaticVoxelBrickId, CAPSULE_CHARACTER_COLLISION_OFFSET,
    DEFAULT_FIXED_STEP_SECONDS, STATIC_VOXEL_BRICK_DIM,
};

const DT: f32 = 1.0 / 60.0;
const RADIUS: f32 = 0.5;
const HALF_HEIGHT: f32 = 1.0;
const PLAYER_RADIUS: f32 = 4.0;
const PLAYER_HALF_HEIGHT: f32 = 8.0;

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
        smooth_microvoxel_walk: false,
    }
}

fn standing_center_y(surface_y: f32) -> f32 {
    surface_y + HALF_HEIGHT + RADIUS + CAPSULE_CHARACTER_COLLISION_OFFSET
}

fn player_capsule_move(
    center: Vec3,
    desired_translation: Vec3,
    smooth_microvoxel_walk: bool,
) -> CapsuleCharacterMove {
    CapsuleCharacterMove {
        center,
        radius: PLAYER_RADIUS,
        half_height: PLAYER_HALF_HEIGHT,
        desired_translation,
        dt: DT,
        smooth_microvoxel_walk,
    }
}

fn player_standing_center_y(surface_y: f32) -> f32 {
    surface_y + PLAYER_HALF_HEIGHT + PLAYER_RADIUS + CAPSULE_CHARACTER_COLLISION_OFFSET
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
fn player_scale_capsule_keeps_horizontal_progress_over_voxel_stair_slope() {
    let mut world = CollisionWorld::new();
    world.upsert_static_voxel_brick(
        StaticVoxelBrickId(IVec3::ZERO),
        1,
        occupancy_where(|voxel| {
            voxel.y == 0
                || (voxel.x >= 8 && voxel.y == 1)
                || (voxel.x >= 12 && voxel.y == 2)
                || (voxel.x >= 16 && voxel.y == 3)
                || (voxel.x >= 20 && voxel.y == 4)
        }),
    );

    let mut center = Vec3::new(4.0, player_standing_center_y(1.0), 16.0);
    for _ in 0..20 {
        let result = world
            .move_capsule_character(player_capsule_move(center, Vec3::new(1.0, -0.1, 0.0), true))
            .unwrap();
        assert!(result.grounded, "center={center:?} result={result:?}");
        assert!(result.translation.x > 0.98, "result={result:?}");
        center += result.translation;
    }

    assert!(center.x > 23.5, "center={center:?}");
    assert!(
        (center.y - player_standing_center_y(5.0)).abs() < 1.0e-3,
        "center={center:?}"
    );
}

#[test]
fn player_scale_capsule_original_mode_keeps_existing_voxel_edge_response() {
    let mut world = CollisionWorld::new();
    world.upsert_static_voxel_brick(
        StaticVoxelBrickId(IVec3::ZERO),
        1,
        occupancy_where(|voxel| voxel.y == 0 || (voxel.x >= 8 && voxel.y == 1)),
    );

    let mut center = Vec3::new(4.0, player_standing_center_y(1.0), 16.0);
    center += world
        .move_capsule_character(player_capsule_move(
            center,
            Vec3::new(1.0, -0.1, 0.0),
            false,
        ))
        .unwrap()
        .translation;
    let edge = world
        .move_capsule_character(player_capsule_move(
            center,
            Vec3::new(1.0, -0.1, 0.0),
            false,
        ))
        .unwrap();

    assert!(edge.grounded, "result={edge:?}");
    assert!(edge.translation.x < 0.8, "result={edge:?}");
}

#[test]
fn smooth_player_scale_capsule_still_stops_at_a_tall_wall() {
    let mut world = CollisionWorld::new();
    world.upsert_static_voxel_brick(
        StaticVoxelBrickId(IVec3::ZERO),
        1,
        occupancy_where(|voxel| voxel.y == 0 || (voxel.x == 12 && voxel.y < 28)),
    );

    let start = Vec3::new(4.0, player_standing_center_y(1.0), 16.0);
    let result = world
        .move_capsule_character(player_capsule_move(start, Vec3::new(10.0, -0.1, 0.0), true))
        .unwrap();
    let final_center = start + result.translation;

    assert!(result.grounded, "result={result:?}");
    assert!(final_center.x + PLAYER_RADIUS < 12.0, "result={result:?}");
}

#[test]
fn smooth_walk_preserves_heading_past_a_left_front_step() {
    let mut world = CollisionWorld::new();
    world.upsert_static_voxel_brick(
        StaticVoxelBrickId(IVec3::ZERO),
        1,
        occupancy_where(|v| v.y == 0 || (v.x < 16 && v.z >= 12 && v.y <= 4)),
    );
    let start = Vec3::new(17.0, player_standing_center_y(1.0), 4.0);
    let trace = |world: &mut CollisionWorld, smooth| {
        let mut center = start;
        for _ in 0..20 {
            let result = world
                .move_capsule_character(player_capsule_move(
                    center,
                    Vec3::new(0.0, -0.1, 1.0),
                    smooth,
                ))
                .unwrap();
            center += result.translation;
        }
        center
    };
    let original = trace(&mut world, false);
    let center = trace(&mut world, true);
    println!("left-front step: A={original:?}, B={center:?}");
    assert!(
        (center.x - start.x).abs() < 0.05,
        "forward input was pushed sideways: start={start:?}, end={center:?}"
    );
    assert!(center.z > 23.5, "forward input stalled: {center:?}");
}

// Exact capsule-vs-box distance for test-owned voxel boxes; checks the returned trajectory,
// independently of the shape casts used to choose it.
fn assert_player_clear_of_box(center: Vec3, min: Vec3, max: Vec3) {
    let nearest = center.clamp(min, max);
    let mut gap = (center - nearest).abs();
    gap.y = (gap.y - PLAYER_HALF_HEIGHT).max(0.0);
    assert!(
        gap.length_squared() >= PLAYER_RADIUS * PLAYER_RADIUS - 1.0e-3,
        "capsule penetrated box {min:?}..{max:?}: center={center:?}, gap={gap:?}"
    );
}

#[test]
fn smooth_walk_keeps_straight_and_diagonal_headings_at_different_frame_rates() {
    for hz in [30, 60, 120] {
        for height in [1, 2, 4] {
            for mirrored in [false, true] {
                for diagonal in [false, true] {
                    let mut world = CollisionWorld::new();
                    world.upsert_static_voxel_brick(
                        StaticVoxelBrickId(IVec3::ZERO),
                        1,
                        occupancy_where(|v| {
                            let on_side = if mirrored { v.x >= 16 } else { v.x < 16 };
                            v.y == 0 || (on_side && v.z >= 12 && v.y <= height)
                        }),
                    );
                    let start = Vec3::new(
                        if mirrored { 15.0 } else { 17.0 },
                        player_standing_center_y(1.0),
                        4.0,
                    );
                    let direction = Vec3::new(
                        if diagonal {
                            if mirrored {
                                1.0
                            } else {
                                -1.0
                            }
                        } else {
                            0.0
                        },
                        0.0,
                        1.0,
                    )
                    .normalize();
                    let side = direction.cross(Vec3::Y);
                    let mut center = start;
                    let dt = 1.0 / hz as f32;
                    let (step_min, step_max) = if mirrored {
                        (
                            Vec3::new(16.0, 1.0, 12.0),
                            Vec3::new(32.0, height as f32 + 1.0, 32.0),
                        )
                    } else {
                        (
                            Vec3::new(0.0, 1.0, 12.0),
                            Vec3::new(16.0, height as f32 + 1.0, 32.0),
                        )
                    };
                    for frame in 0..hz / 3 {
                        let mut request = player_capsule_move(
                            center,
                            direction * (60.0 * dt) - Vec3::Y * (12.8 * dt),
                            true,
                        );
                        request.dt = dt;
                        let movement = world.move_capsule_character(request).unwrap();
                        center += movement.translation;
                        let expected = start + direction * (60.0 * dt * (frame + 1) as f32);
                        assert!(
                            (center - expected).dot(side).abs() < 0.01,
                            "heading drift: hz={hz} height={height} mirrored={mirrored} diagonal={diagonal} frame={frame} result={movement:?}"
                        );
                        assert!(
                            (center - expected).dot(direction).abs() < 0.05,
                            "forward loss: hz={hz} height={height} frame={frame} center={center:?} expected={expected:?}"
                        );
                        assert!(
                            movement.grounded,
                            "unexpected loss of support: {movement:?}"
                        );
                        assert_player_clear_of_box(center, Vec3::ZERO, Vec3::new(32.0, 1.0, 32.0));
                        assert_player_clear_of_box(center, step_min, step_max);
                    }
                }
            }
        }
    }
}

#[test]
fn smooth_walk_handles_limited_headroom_without_penetrating_the_ceiling() {
    // A 1-voxel step fits under y=28, but not y=26 (player total height=24).
    for ceiling in [26, 28] {
        let mut world = CollisionWorld::new();
        world.upsert_static_voxel_brick(
            StaticVoxelBrickId(IVec3::ZERO),
            1,
            occupancy_where(|v| v.y == 0 || (v.x >= 12 && v.y == 1) || v.y == ceiling),
        );
        let mut center = Vec3::new(4.0, player_standing_center_y(1.0), 16.0);
        for _ in 0..20 {
            let result = world
                .move_capsule_character(player_capsule_move(
                    center,
                    Vec3::new(1.0, -0.1, 0.0),
                    true,
                ))
                .unwrap();
            center += result.translation;
            assert_player_clear_of_box(
                center,
                Vec3::new(12.0, 1.0, 0.0),
                Vec3::new(32.0, 2.0, 32.0),
            );
            assert_player_clear_of_box(
                center,
                Vec3::new(0.0, ceiling as f32, 0.0),
                Vec3::new(32.0, ceiling as f32 + 1.0, 32.0),
            );
        }
        if ceiling == 28 {
            assert!(center.x > 23.9, "fitting step blocked: {center:?}");
        } else {
            assert!(
                center.x < 12.0,
                "squeezed into impossible headroom: {center:?}"
            );
        }
    }
}

#[test]
fn smooth_walk_does_not_accelerate_sliding_along_a_tall_wall() {
    let mut world = CollisionWorld::new();
    world.upsert_static_voxel_brick(
        StaticVoxelBrickId(IVec3::ZERO),
        1,
        occupancy_where(|v| v.y == 0 || (v.x >= 12 && v.y < 28)),
    );
    let start = Vec3::new(7.9, player_standing_center_y(1.0), 8.0);
    let desired = Vec3::new(1.0, -0.1, 1.0);
    let a = world
        .move_capsule_character(player_capsule_move(start, desired, false))
        .unwrap();
    let b = world
        .move_capsule_character(player_capsule_move(start, desired, true))
        .unwrap();
    assert_eq!(a, b, "a rejected step must keep the original wall response");
    assert!(b.translation.z > 0.99 && b.translation.z <= 1.001, "{b:?}");
    assert_player_clear_of_box(
        start + b.translation,
        Vec3::new(12.0, 1.0, 0.0),
        Vec3::new(32.0, 28.0, 32.0),
    );
}

#[test]
fn smooth_walk_does_not_change_airborne_and_cliff_motion() {
    let mut world = CollisionWorld::new();
    world.upsert_static_voxel_brick(
        StaticVoxelBrickId(IVec3::ZERO),
        1,
        occupancy_where(|v| (v.z < 12 && v.y == 0) || (v.x < 16 && v.z >= 12 && v.y <= 4)),
    );
    // Jumping into the corner; falling beside it; leaving a cliff on the other side.
    for (start, desired) in [
        (
            Vec3::new(17.0, player_standing_center_y(1.0), 8.0),
            Vec3::new(0.0, 0.5, 1.0),
        ),
        (Vec3::new(17.0, 20.0, 8.0), Vec3::new(0.0, -1.0, 1.0)),
        (
            Vec3::new(26.0, player_standing_center_y(1.0), 10.0),
            Vec3::new(0.0, -0.1, 10.0),
        ),
    ] {
        let a = world
            .move_capsule_character(player_capsule_move(start, desired, false))
            .unwrap();
        let b = world
            .move_capsule_character(player_capsule_move(start, desired, true))
            .unwrap();
        assert_eq!(a, b, "unexpected airborne/cliff change at {start:?}");
    }
}

#[test]
fn smooth_walk_preserves_player_scale_large_step_traversability() {
    for height in [16, 17] {
        let mut world = CollisionWorld::new();
        world.upsert_static_voxel_brick(
            StaticVoxelBrickId(IVec3::ZERO),
            1,
            occupancy_where(|v| v.y == 0 || (v.x >= 12 && v.y <= height)),
        );
        let mut center = Vec3::new(4.0, player_standing_center_y(1.0), 16.0);
        let mut original = center;
        for _ in 0..20 {
            original += world
                .move_capsule_character(player_capsule_move(
                    original,
                    Vec3::new(1.0, -0.1, 0.0),
                    false,
                ))
                .unwrap()
                .translation;
        }
        println!("step height={height}: original={original:?}");
        for _ in 0..20 {
            let movement = world
                .move_capsule_character(player_capsule_move(
                    center,
                    Vec3::new(1.0, -0.1, 0.0),
                    true,
                ))
                .unwrap();
            center += movement.translation;
            assert_player_clear_of_box(
                center,
                Vec3::new(12.0, 1.0, 0.0),
                Vec3::new(32.0, height as f32 + 1.0, 32.0),
            );
        }
        println!("step height={height}: end={center:?}");
        // At player scale Rapier can already climb 17 voxels in several smaller moves, unlike
        // the small-capsule, single-move regression above. Keep that existing reachability.
        assert!(
            original.x > 20.0,
            "baseline no longer traverses this step: {original:?}"
        );
        assert!(
            center.x > 23.8,
            "previously traversable step blocked: {center:?}"
        );
    }
}

#[test]
fn smooth_walk_preserves_heading_at_a_brick_seam_and_after_collider_edits() {
    let mut world = CollisionWorld::new();
    // Use a translated scene to exercise world-space hit positions, with the step at z=64.
    let approach = StaticVoxelBrickId(IVec3::new(1, 2, 1));
    let ahead = StaticVoxelBrickId(IVec3::new(1, 2, 2));
    world.upsert_static_voxel_brick(approach, 1, layer_floor(1));
    let start = Vec3::new(49.0, 64.0 + player_standing_center_y(1.0), 56.0);
    for (revision, height) in [(1, 4), (2, 0), (3, 2)] {
        world.upsert_static_voxel_brick(
            ahead,
            revision,
            occupancy_where(|v| v.y == 0 || (v.x < 16 && v.y <= height)),
        );
        let mut center = start;
        for frame in 0..20 {
            // Alternate A/B without resetting the authoritative capsule pose. Only the B
            // trajectory is constrained when there is a real step in the way.
            let smooth = height > 0 || frame % 2 == 0;
            let result = world
                .move_capsule_character(player_capsule_move(
                    center,
                    Vec3::new(0.0, -0.1, 1.0),
                    smooth,
                ))
                .unwrap();
            center += result.translation;
            assert!(
                (center.x - start.x).abs() < 0.01,
                "seam drift at revision={revision}: {center:?}"
            );
        }
        assert!(
            (center.z - start.z - 20.0).abs() < 0.05,
            "seam blocked at revision={revision}: {center:?}"
        );
        if height == 0 {
            assert!(
                (center.y - start.y).abs() < 0.01,
                "stale step support after removal: {center:?}"
            );
        } else {
            assert!(
                center.y > start.y + 0.5,
                "replacement step support missing: {center:?}"
            );
        }
    }
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
