//! Player-scale traversal distinguishes low ground detail from a climbing foothold.
use glam::{IVec3, UVec3, Vec3};
use re_flora_physics::{
    BrickOccupancy, CapsuleCharacterMove, CollisionWorld, StaticVoxelBrickId,
    CAPSULE_CHARACTER_COLLISION_OFFSET,
};

const RADIUS: f32 = 4.0;
const HALF_HEIGHT: f32 = 8.0;

fn scene(filled: impl Fn(UVec3) -> bool) -> CollisionWorld {
    let mut world = CollisionWorld::new();
    let mut voxels = Vec::new();
    for x in 0..32 {
        for y in 0..32 {
            for z in 0..32 {
                let voxel = UVec3::new(x, y, z);
                if filled(voxel) {
                    voxels.push(voxel);
                }
            }
        }
    }
    world.upsert_static_voxel_brick(
        StaticVoxelBrickId(IVec3::ZERO),
        1,
        BrickOccupancy::from_filled_voxels(voxels),
    );
    world
}

fn start() -> Vec3 {
    Vec3::new(
        4.0,
        1.0 + HALF_HEIGHT + RADIUS + CAPSULE_CHARACTER_COLLISION_OFFSET,
        16.0,
    )
}

fn request(center: Vec3, hz: u32) -> CapsuleCharacterMove {
    let dt = 1.0 / hz as f32;
    CapsuleCharacterMove {
        center,
        radius: RADIUS,
        half_height: HALF_HEIGHT,
        desired_translation: Vec3::new(60.0 * dt, -6.0 * dt, 0.0),
        dt,
    }
}

// Independent capsule-to-box distance: returned positions must not penetrate geometry.
fn assert_clear(center: Vec3, min: Vec3, max: Vec3) {
    let mut gap = (center - center.clamp(min, max)).abs();
    gap.y = (gap.y - HALF_HEIGHT).max(0.0);
    assert!(
        gap.length_squared() >= RADIUS * RADIUS - 1.0e-3,
        "penetration: center={center:?}, box={min:?}..{max:?}"
    );
}

#[test]
fn ground_detail_is_opt_in_and_changes_live_without_stale_support() {
    let mut world = scene(|v| v.y == 0 || ((12..14).contains(&v.x) && v.y == 1));
    let mut center = start();
    for _ in 0..20 {
        center += world
            .move_capsule_character(request(center, 60))
            .unwrap()
            .translation;
    }
    assert!(center.x < 10.0, "legacy behavior changed: {center:?}");
    let blocked = request(center, 60);
    let legacy = world.move_capsule_character(blocked).unwrap();
    let disabled = world
        .move_capsule_character_with_ground_detail(blocked, 0.0)
        .unwrap();
    assert_eq!(legacy, disabled);
    let enabled = world
        .move_capsule_character_with_ground_detail(blocked, 3.0)
        .unwrap();
    assert!(enabled.translation.x > 0.99, "{enabled:?}");
    assert_eq!(
        world
            .move_capsule_character_with_ground_detail(blocked, 0.0)
            .unwrap(),
        disabled
    );
    // The same world immediately reflects an edit from a low curb into an unwalkable wall.
    world.upsert_static_voxel_brick(
        StaticVoxelBrickId(IVec3::ZERO),
        2,
        BrickOccupancy::from_filled_voxels((0..32).flat_map(|x| {
            (0..32).flat_map(move |z| {
                (0..=if x >= 12 { 24 } else { 0 }).map(move |y| UVec3::new(x, y, z))
            })
        })),
    );
    // Start clear of the newly enlarged wall, not embedded in it.
    let before_wall = Vec3::new(7.8, start().y, 16.0);
    let result = world
        .move_capsule_character_with_ground_detail(request(before_wall, 60), 3.0)
        .unwrap();
    assert!(
        result.translation.x < 0.2 && result.translation.y.abs() < 0.01,
        "{result:?}"
    );
}

#[test]
fn detail_height_is_a_terrain_relative_bound_not_a_new_step_height() {
    for detail_height in [0.0, 1.0, 2.0, 3.0, 4.0] {
        for height in [1, 2, 3, 4, 5, 12, 13, 24] {
            for wide in [false, true] {
                let end_x = if wide { 32 } else { 14 };
                let mut world =
                    scene(|v| v.y == 0 || ((12..end_x).contains(&v.x) && v.y <= height));
                let mut center = start();
                for _ in 0..30 {
                    center += world
                        .move_capsule_character_with_ground_detail(
                            request(center, 60),
                            detail_height,
                        )
                        .unwrap()
                        .translation;
                    assert_clear(
                        center,
                        Vec3::new(12.0, 1.0, 0.0),
                        Vec3::new(end_x as f32, height as f32 + 1.0, 32.0),
                    );
                    if center.x >= 24.0 {
                        break;
                    }
                }
                let should_cross = if wide {
                    height <= 12
                } else {
                    height as f32 <= detail_height
                };
                assert_eq!(
                    center.x > 23.8,
                    should_cross,
                    "height={height} allowance={detail_height} wide={wide} end={center:?}"
                );
            }
        }
    }
}

#[test]
fn low_obstacles_still_require_headroom_and_real_surrounding_ground() {
    for ceiling in [None, Some(26), Some(28)] {
        for ground in [true, false] {
            let mut world = scene(|v| {
                ((ground || v.x < 10) && v.y == 0)
                    || ((12..14).contains(&v.x) && v.y == 1)
                    || ceiling == Some(v.y)
            });
            let mut center = start();
            for _ in 0..20 {
                let movement = world
                    .move_capsule_character_with_ground_detail(request(center, 60), 3.0)
                    .unwrap();
                center += movement.translation;
                assert_clear(
                    center,
                    Vec3::new(12.0, 1.0, 0.0),
                    Vec3::new(14.0, 2.0, 32.0),
                );
                if let Some(y) = ceiling {
                    assert_clear(
                        center,
                        Vec3::new(0.0, y as f32, 0.0),
                        Vec3::new(32.0, y as f32 + 1.0, 32.0),
                    );
                }
            }
            if ground && ceiling != Some(26) {
                assert!(center.x > 23.8, "valid clearance blocked: {center:?}");
            } else {
                assert!(
                    center.x < 12.0,
                    "unsupported or low-ceiling passage accepted: {center:?}"
                );
            }
        }
    }
}

#[test]
fn ground_detail_does_not_change_jump_fall_or_idle_queries() {
    let mut world = scene(|v| v.y == 0 || ((12..14).contains(&v.x) && v.y <= 3));
    for (center, translation) in [
        (start(), Vec3::ZERO),
        (start(), Vec3::new(10.0, 2.0, 0.0)),
        (start() + Vec3::Y * 15.0, Vec3::new(10.0, -1.0, 0.0)),
    ] {
        let movement = CapsuleCharacterMove {
            desired_translation: translation,
            ..request(center, 60)
        };
        assert_eq!(
            world.move_capsule_character(movement).unwrap(),
            world
                .move_capsule_character_with_ground_detail(movement, 3.0)
                .unwrap(),
        );
    }
    for invalid in [-1.0, f32::NAN, f32::INFINITY] {
        assert!(world
            .move_capsule_character_with_ground_detail(request(start(), 60), invalid)
            .is_err());
    }
}

#[test]
fn low_obstacles_preserve_heading_on_oblique_and_reverse_approaches() {
    for reverse in [false, true] {
        let min_x = if reverse { 18 } else { 12 };
        let mut world = scene(|v| v.y == 0 || ((min_x..min_x + 2).contains(&v.x) && v.y <= 3));
        let initial = Vec3::new(if reverse { 28.0 } else { 4.0 }, start().y, 8.0);
        let direction = Vec3::new(if reverse { -1.0 } else { 1.0 }, 0.0, 0.3).normalize();
        let mut center = initial;
        for frame in 0..22 {
            let movement = CapsuleCharacterMove {
                desired_translation: direction - Vec3::Y * 0.1,
                ..request(center, 60)
            };
            center += world
                .move_capsule_character_with_ground_detail(movement, 3.0)
                .unwrap()
                .translation;
            let expected = initial + direction * (frame + 1) as f32;
            assert!((center.x - expected.x).abs() < 0.01 && (center.z - expected.z).abs() < 0.01,
                "heading drift: reverse={reverse} frame={frame} actual={center:?} expected={expected:?}");
        }
        assert!(
            (center.y - initial.y).abs() < 0.01,
            "did not settle: {center:?}"
        );
    }
}

#[test]
fn low_narrow_obstacles_do_not_block_forward_walking() {
    for hz in [30, 60, 120] {
        for height in [1, 2, 3] {
            for pebble in [false, true] {
                let mut world = scene(|v| {
                    v.y == 0
                        || ((12..14).contains(&v.x)
                            && (!pebble || (15..17).contains(&v.z))
                            && v.y <= height)
                });
                let mut center = start();
                for _ in 0..hz / 3 {
                    let result = world
                        .move_capsule_character_with_ground_detail(request(center, hz), 3.0)
                        .unwrap();
                    center += result.translation;
                    assert_clear(center, Vec3::ZERO, Vec3::new(32.0, 1.0, 32.0));
                    assert_clear(
                        center,
                        Vec3::new(12.0, 1.0, if pebble { 15.0 } else { 0.0 }),
                        Vec3::new(14.0, height as f32 + 1.0, if pebble { 17.0 } else { 32.0 }),
                    );
                    assert!(
                        (center.z - start().z).abs() < 0.01,
                        "sideways detour: {center:?}"
                    );
                }
                assert!(
                    center.x > 23.8,
                    "low obstacle blocked: hz={hz} height={height} pebble={pebble} end={center:?}"
                );
                assert!(
                    (center.y - start().y).abs() < 0.01,
                    "did not return to ground: {center:?}"
                );
            }
        }
    }
}
