//! One-shot safe placement when entering collision-enabled walking.
//! Search the actual body volume, not a point ray. Never force an unsafe switch.
use super::surface_motion::PLAYER;
use glam::Vec3;
use re_flora_physics::{CapsuleCharacterMoveError, CollisionWorld};

fn clear(world: &mut CollisionWorld, feet: Vec3) -> Result<bool, CapsuleCharacterMoveError> {
    world.capsule_character_is_clear(
        PLAYER.center_voxels(feet),
        PLAYER.radius_voxels,
        PLAYER.half_height_voxels,
    )
}

pub(super) fn recover(
    world: &mut CollisionWorld,
    feet: Vec3,
) -> Result<Option<Vec3>, CapsuleCharacterMoveError> {
    if clear(world, feet)? {
        return Ok(Some(feet));
    }
    // Small shells resolve walls/edges without jumping straight to the roof.
    for radius in [1., 2., 4., 8., 16., 32.] {
        for y in [1, 0, -1] {
            for x in -1..=1 {
                for z in -1..=1 {
                    let direction = Vec3::new(x as f32, y as f32, z as f32);
                    if direction == Vec3::ZERO {
                        continue;
                    }
                    let candidate = feet + direction.normalize() * (radius / 256.);
                    if clear(world, candidate)? {
                        return Ok(Some(candidate));
                    }
                }
            }
        }
    }
    // Bounded upward escape for deep terrain burial. Each location must fit the
    // complete capsule, including headroom; clear airborne positions may fall.
    for height in (36..=512).step_by(4) {
        let candidate = feet + Vec3::Y * (height as f32 / 256.);
        if clear(world, candidate)? {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{IVec3, UVec3};
    use re_flora_physics::{BrickOccupancy, StaticVoxelBrickId};

    #[test]
    fn embedded_walking_entry_recovers_the_whole_capsule_and_can_move() {
        let mut world = CollisionWorld::new();
        world.upsert_static_voxel_brick(
            StaticVoxelBrickId(IVec3::ZERO),
            1,
            BrickOccupancy::from_filled_voxels((0..32).flat_map(|x| {
                (0..32).flat_map(move |z| (0..16).map(move |y| UVec3::new(x, y, z)))
            })),
        );
        let feet = Vec3::new(16., 0., 16.) / 256.;
        assert!(!clear(&mut world, feet).unwrap());
        let recovered = recover(&mut world, feet).unwrap().unwrap();
        assert!(
            clear(&mut world, recovered).unwrap(),
            "Walking must not begin with an embedded capsule"
        );
        let movement = PLAYER
            .move_by(&mut world, recovered, Vec3::X * 0.01, 1. / 60.)
            .unwrap();
        assert!(
            movement.translation.x > 0.005,
            "recovered player must be able to move"
        );
    }

    #[test]
    fn clear_airborne_position_is_not_snapped_to_ground() {
        let mut world = CollisionWorld::new();
        let feet = Vec3::new(0.2, 0.8, 0.3);
        assert_eq!(recover(&mut world, feet).unwrap(), Some(feet));
    }
}
