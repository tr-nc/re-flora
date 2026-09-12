//! Deterministic diagnostic using the game's four-voxel apple and material.
use glam::{IVec3, UVec3, Vec3};
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
    desc
}

#[test]
#[ignore = "diagnostic: baseline Parry 0.29.0 loses predictive voxel contacts"]
fn apple_settles_without_persistent_ground_jitter() {
    let mut world = CollisionWorld::new();
    world.set_gravity(Vec3::new(0.0, -9.8 * 256.0, 0.0)).unwrap();
    for z in 0..1 {
        for x in 0..1 {
            let floor = BrickOccupancy::from_filled_voxels((0..2).flat_map(|y| {
                (0..32).flat_map(move |z| (0..32).map(move |x| UVec3::new(x, y, z)))
            }));
            world.upsert_static_voxel_brick(StaticVoxelBrickId(IVec3::new(x, 0, z)), 1, floor);
        }
    }
    let body = world.spawn_dynamic_body(apple(Vec3::new(16., 28., 16.))).unwrap();
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    let mut sleeping_steps = 0;
    for step in 1..=30 * 120 {
        assert_eq!(world.advance(1.0 / 120.0).steps, 1);
        let state = world.dynamic_body_state(body).unwrap();
        if step % 30 == 0 || (2400..2460).contains(&step) {
            println!("[FRUIT_GROUND] step={step} state={state:?} contacts={:?}", world.dynamic_body_diagnostics(body).unwrap());
        }
        if step > 20 * 120 {
            min = min.min(state.position);
            max = max.max(state.position);
            sleeping_steps += usize::from(state.sleeping);
        }
    }
    println!("[FRUIT_GROUND] final_10s_range={:?} sleeping_steps={sleeping_steps}/1200", max-min);
    assert!((max-min).max_element() < 0.01, "persistent ground motion: {:?}", max-min);
    assert_eq!(sleeping_steps, 1200, "resting apple never settles into natural sleep");
}
