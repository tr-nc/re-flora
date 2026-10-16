//! Walking support must distinguish a real stairway from a thin ridge on a wall.
use glam::{IVec3, Quat, UVec3, Vec3};
use re_flora_physics::{
    BrickOccupancy, CapsuleCharacterMove, CollisionWorld, StaticVoxelBrickId,
    CAPSULE_CHARACTER_COLLISION_OFFSET, STATIC_VOXEL_BRICK_DIM,
};

const RADIUS: f32 = 4.0;
const HALF_HEIGHT: f32 = 8.0;
const WALL_HEIGHT: u32 = 64;

#[derive(Clone, Copy)]
struct WallStairs {
    width: u32,
    quarter_turns: u32,
    origin: IVec3,
}

impl WallStairs {
    fn new(width: u32) -> Self {
        Self {
            width,
            quarter_turns: 0,
            origin: IVec3::ZERO,
        }
    }

    fn rotate(self, mut vector: Vec3) -> Vec3 {
        for _ in 0..self.quarter_turns {
            vector = Vec3::new(-vector.z, vector.y, vector.x);
        }
        vector
    }

    fn world_point(self, local: Vec3) -> Vec3 {
        self.rotate(local) + self.origin.as_vec3() * STATIC_VOXEL_BRICK_DIM as f32
    }

    fn local_point(self, world: Vec3) -> Vec3 {
        let mut local = world - self.origin.as_vec3() * STATIC_VOXEL_BRICK_DIM as f32;
        for _ in 0..self.quarter_turns {
            local = Vec3::new(local.z, local.y, -local.x);
        }
        local
    }

    fn publish(self, world: &mut CollisionWorld, revision: u64) {
        for by in 0..3 {
            for bz in 0..3 {
                let brick = IVec3::new(0, by, bz);
                let mut filled = Vec::new();
                for z in 0..STATIC_VOXEL_BRICK_DIM {
                    for y in 0..STATIC_VOXEL_BRICK_DIM {
                        for x in 0..STATIC_VOXEL_BRICK_DIM {
                            let mut local = UVec3::new(x, y, z);
                            let voxel = local + brick.as_uvec3() * STATIC_VOXEL_BRICK_DIM;
                            let stair_height = voxel.z.saturating_sub(8).min(WALL_HEIGHT);
                            if voxel.y == 0
                                || (voxel.z >= 72 && voxel.y <= WALL_HEIGHT)
                                || ((16..16 + self.width).contains(&voxel.x)
                                    && voxel.y <= stair_height)
                            {
                                for _ in 0..self.quarter_turns {
                                    local = UVec3::new(31 - local.z, local.y, local.x);
                                }
                                filled.push(local);
                            }
                        }
                    }
                }
                let mut rotated_brick = brick;
                for _ in 0..self.quarter_turns {
                    rotated_brick =
                        IVec3::new(-rotated_brick.z - 1, rotated_brick.y, rotated_brick.x);
                }
                world.upsert_static_voxel_brick(
                    StaticVoxelBrickId(rotated_brick + self.origin),
                    revision,
                    BrickOccupancy::from_filled_voxels(filled),
                );
            }
        }
    }

    // Independent capsule/box distance, not the queries making the movement decision.
    fn assert_clear(self, center: Vec3, radius: f32, half_height: f32) {
        let center = self.local_point(center);
        let assert_box = |min, max| {
            let nearest = center.clamp(min, max);
            let mut gap = (center - nearest).abs();
            gap.y = (gap.y - half_height).max(0.0);
            assert!(
                gap.length_squared() >= radius * radius - 1.0e-3,
                "capsule penetrated {min:?}..{max:?}: {center:?}"
            );
        };
        assert_box(Vec3::ZERO, Vec3::new(32.0, 1.0, 96.0));
        assert_box(Vec3::new(0.0, 1.0, 72.0), Vec3::new(32.0, 65.0, 96.0));
        for z in 9..72 {
            assert_box(
                Vec3::new(16.0, 1.0, z as f32),
                Vec3::new((16 + self.width) as f32, (z - 7) as f32, (z + 1) as f32),
            );
        }
    }

    fn walk(self, world: &mut CollisionWorld, hz: u32, scale: f32, frames: u32) -> Vec3 {
        self.walk_with_detail(world, hz, scale, frames, 0.0)
    }

    fn walk_with_detail(
        self,
        world: &mut CollisionWorld,
        hz: u32,
        scale: f32,
        frames: u32,
        detail_height: f32,
    ) -> Vec3 {
        let radius = RADIUS * scale;
        let half_height = HALF_HEIGHT * scale;
        let start = self.world_point(Vec3::new(
            16.0 + self.width as f32 * 0.5,
            1.0 + half_height + radius + CAPSULE_CHARACTER_COLLISION_OFFSET,
            4.0,
        ));
        let mut center = start;
        let dt = 1.0 / hz as f32;
        for frame in 0..frames {
            let movement = world
                .move_capsule_character_with_ground_detail(
                    CapsuleCharacterMove {
                        center,
                        radius,
                        half_height,
                        desired_translation: self.rotate(Vec3::new(0.0, -6.0 * dt, 60.0 * dt)),
                        dt,
                    },
                    detail_height,
                )
                .unwrap();
            center += movement.translation;
            self.assert_clear(center, radius, half_height);
            if self.width == 1 {
                assert!(
                    center.y < start.y + detail_height.min(radius) + 1.0,
                    "walked up a one-voxel wall ridge: hz={hz} scale={scale} frame={frame} start={start:?} center={center:?}"
                );
            }
        }
        self.local_point(center)
    }
}

#[test]
fn ground_detail_allowance_cannot_ratchet_up_a_wall_ridge() {
    for (hz, scale, turns, origin) in [
        (30, 0.5, 1, IVec3::new(2, 4, -3)),
        (60, 1.0, 2, IVec3::ZERO),
        (120, 2.0, 3, IVec3::new(-2, -4, 3)),
    ] {
        for requested_height in [RADIUS * scale * 0.75, 1000.0] {
            let scene = WallStairs {
                width: 1,
                quarter_turns: turns,
                origin,
            };
            let mut world = CollisionWorld::new();
            scene.publish(&mut world, 1);
            // Sustained pushing, not just the first blocked frame. Also exercises
            // the radius safety cap when a caller supplies an excessive allowance.
            let end = scene.walk_with_detail(&mut world, hz, scale, hz * 4, requested_height);
            assert!(end.z < 24.0, "narrow ridge became a staircase: {end:?}");
        }
    }
}

#[test]
fn walking_cannot_climb_a_one_voxel_ridge_on_a_tall_wall() {
    let scene = WallStairs::new(1);
    let mut world = CollisionWorld::new();
    scene.publish(&mut world, 1);
    // Keep pushing long after contact to catch even slow normal-nudge climbing.
    scene.walk(&mut world, 60, 1.0, 360);
}

#[test]
fn walking_can_climb_wide_stairs_to_the_same_wall_top() {
    let scene = WallStairs::new(12);
    let mut world = CollisionWorld::new();
    scene.publish(&mut world, 1);
    let end = scene.walk(&mut world, 60, 1.0, 80);
    assert!(end.z > 80.0, "wide stairs blocked: {end:?}");
    assert!(end.y > 76.0, "wide stairs did not reach the top: {end:?}");
}

#[test]
fn footholds_track_frame_rate_body_scale_rotation_and_translated_brick_seams() {
    for (hz, scale, turns, origin) in [
        (30, 0.5, 1, IVec3::new(2, 4, -3)),
        (60, 1.0, 2, IVec3::ZERO),
        (120, 2.0, 3, IVec3::new(-2, -4, 3)),
    ] {
        for width in [1, 12] {
            let scene = WallStairs {
                width,
                quarter_turns: turns,
                origin,
            };
            let mut world = CollisionWorld::new();
            scene.publish(&mut world, 1);
            let end = scene.walk(&mut world, hz, scale, hz * 4 / 3);
            if width == 12 {
                assert!(
                    end.z > 80.0 && end.y > 64.0 + (RADIUS + HALF_HEIGHT) * scale,
                    "wide stairs blocked hz={hz} scale={scale}: {end:?}"
                );
            }
        }
    }
}

#[test]
fn narrowing_and_rebuilding_the_stairs_immediately_updates_footholds() {
    let mut world = CollisionWorld::new();
    for (revision, width) in [(1, 12), (2, 1), (3, 12)] {
        let scene = WallStairs::new(width);
        scene.publish(&mut world, revision);
        let end = scene.walk(&mut world, 60, 1.0, 80);
        if width == 12 {
            assert!(end.z > 80.0 && end.y > 76.0, "revision={revision}: {end:?}");
        }
    }
}

fn model_ramp(world: &mut CollisionWorld, width: f32, rotation: Quat, offset: Vec3) {
    let mut positions = Vec::new();
    let mut indices = Vec::new();
    for (half_width, bottom, top) in [
        (
            100.0,
            Vec3::new(0.0, 0.0, -100.0),
            Vec3::new(0.0, 0.0, 100.0),
        ),
        (width * 0.5, Vec3::ZERO, Vec3::new(0.0, 20.0, 40.0)),
        (
            width * 0.5,
            Vec3::new(0.0, 20.0, 40.0),
            Vec3::new(0.0, 20.0, 100.0),
        ),
    ] {
        let base = positions.len() as u32;
        positions.extend(
            [
                bottom - Vec3::X * half_width,
                top - Vec3::X * half_width,
                bottom + Vec3::X * half_width,
                top + Vec3::X * half_width,
            ]
            .map(|p| rotation * p + offset),
        );
        indices.extend([[base, base + 1, base + 2], [base + 2, base + 1, base + 3]]);
    }
    world.set_fixed_scene_surface(&positions, &indices).unwrap();
}

#[test]
fn smooth_model_ramps_use_the_same_width_rule_at_oblique_angles() {
    for yaw in [0.0, std::f32::consts::FRAC_PI_4, 1.1] {
        for width in [1.0, 12.0] {
            let mut world = CollisionWorld::new();
            let rotation = Quat::from_rotation_y(yaw);
            let offset = Vec3::new(-64.0, -32.0, 96.0);
            model_ramp(&mut world, width, rotation, offset);
            let start = rotation * Vec3::new(0.0, 12.1, -4.0) + offset;
            let mut center = start;
            for _ in 0..100 {
                let movement = world
                    .move_capsule_character(CapsuleCharacterMove {
                        center,
                        radius: RADIUS,
                        half_height: HALF_HEIGHT,
                        desired_translation: rotation * Vec3::new(0.0, -0.1, 0.5),
                        dt: 1.0 / 60.0,
                    })
                    .unwrap();
                center += movement.translation;
                if width == 1.0 {
                    assert!(
                        center.y < start.y + 1.0,
                        "narrow model ramp climbed: {center:?}"
                    );
                }
            }
            if width == 12.0 {
                let local = rotation.inverse() * (center - offset);
                assert!(
                    local.z > 38.0 && local.y > 31.0,
                    "broad model ramp blocked: {local:?}"
                );
            }
        }
    }
}
