//! One character-world movement seam for the player and cursor-guided ground props.
//! Actor dimensions differ; terrain/model collision, stepping, sliding and snapping do not.
use glam::{Quat, Vec3};
use re_flora_physics::{CapsuleCharacterMove, CapsuleCharacterMoveError, CollisionWorld};

const VOXELS_PER_UNIT: f32 = 256.;
pub(super) const PLAYER: SurfaceMover = SurfaceMover {
    radius_voxels: 4.,
    half_height_voxels: 8.,
};
pub(super) const MOWER: SurfaceMover = SurfaceMover {
    radius_voxels: 10.,
    half_height_voxels: 2.,
};

#[derive(Clone, Copy)]
pub(super) struct SurfaceMover {
    pub radius_voxels: f32,
    pub half_height_voxels: f32,
}

pub(super) struct SurfaceMove {
    pub translation: Vec3,
    pub grounded: bool,
}

impl SurfaceMover {
    pub fn center_voxels(self, feet: Vec3) -> Vec3 {
        feet * VOXELS_PER_UNIT + Vec3::Y * (self.radius_voxels + self.half_height_voxels)
    }

    pub fn move_by(
        self,
        world: &mut CollisionWorld,
        feet: Vec3,
        translation: Vec3,
        dt: f32,
    ) -> Result<SurfaceMove, CapsuleCharacterMoveError> {
        let movement = world.move_capsule_character(CapsuleCharacterMove {
            center: self.center_voxels(feet),
            radius: self.radius_voxels,
            half_height: self.half_height_voxels,
            desired_translation: translation * VOXELS_PER_UNIT,
            dt,
        })?;
        Ok(SurfaceMove {
            translation: movement.translation / VOXELS_PER_UNIT,
            grounded: movement.grounded,
        })
    }

    /// Settle a prop onto a picked surface, with a little clearance for the controller skin.
    /// Unsupported placements are rejected, instead of inventing support from ray samples.
    pub fn place(
        self,
        world: &mut CollisionWorld,
        surface: Vec3,
    ) -> Result<Option<Vec3>, CapsuleCharacterMoveError> {
        let start = surface + Vec3::Y * (2. / VOXELS_PER_UNIT);
        let movement =
            self.move_by(world, start, Vec3::NEG_Y * (4. / VOXELS_PER_UNIT), 1. / 60.)?;
        Ok(movement.grounded.then_some(start + movement.translation))
    }

    /// Ground-guided props stop at an unsupported drop rather than walking off it.
    /// The tiny down bias enables the same ground-snap path as the walking player.
    pub fn follow(
        self,
        world: &mut CollisionWorld,
        feet: Vec3,
        planar_translation: Vec3,
        dt: f32,
    ) -> Result<Option<Vec3>, CapsuleCharacterMoveError> {
        let movement = self.move_by(
            world,
            feet,
            planar_translation + Vec3::NEG_Y * (0.1 / VOXELS_PER_UNIT),
            dt,
        )?;
        Ok(movement.grounded.then_some(feet + movement.translation))
    }
}

pub(super) struct MowerSupport {
    pub position: Vec3,
    pub normal: Vec3,
}

fn walkable_surface_height(world: &mut CollisionWorld, origin: Vec3) -> Option<f32> {
    world
        .cast_character_surface_ray(origin, Vec3::NEG_Y, 32.5)
        .filter(|hit| hit.normal.y >= 0.5)
        .map(|hit| hit.position.y)
}

/// Reconstruct a continuous contact height from the voxel-center lattice. Direct
/// point rays jump a whole voxel when a wheel crosses a stair edge, feeding both
/// chassis height and tilt. Bilinear sampling removes that discontinuity without
/// lagging planar motion. Planar model surfaces are reproduced exactly.
/// This is presentation only; capsule movement still uses the unmodified geometry.
fn wheel_surface_height(world: &mut CollisionWorld, origin: Vec3) -> Option<f32> {
    let base_x = (origin.x - 0.5).floor() + 0.5;
    let base_z = (origin.z - 0.5).floor() + 0.5;
    let fraction_x = origin.x - base_x;
    let fraction_z = origin.z - base_z;
    let mut heights = [0.; 4];
    for (i, (x, z)) in [(0., 0.), (1., 0.), (0., 1.), (1., 1.)]
        .into_iter()
        .enumerate()
    {
        let sample = Vec3::new(base_x + x, origin.y, base_z + z);
        let Some(height) = walkable_surface_height(world, sample) else {
            // At unsupported edges keep the original contact semantics rather
            // than inventing a surface across a gap.
            return walkable_surface_height(world, origin);
        };
        heights[i] = height;
    }
    let min = heights.into_iter().fold(f32::INFINITY, f32::min);
    let max = heights.into_iter().fold(f32::NEG_INFINITY, f32::max);
    if max - min > 2. {
        // Smooth voxel-scale stairs, not distinct levels at a ledge or wall.
        return walkable_surface_height(world, origin);
    }
    let back = heights[0] + (heights[1] - heights[0]) * fraction_x;
    let front = heights[2] + (heights[3] - heights[2]) * fraction_x;
    Some(back + (front - back) * fraction_z)
}

/// Fit the wheel footprint over continuous contact heights rather than raw voxel
/// faces. Model triangles use the same character collision source.
pub(super) fn mower_support(
    world: &mut CollisionWorld,
    feet: Vec3,
    rotation: Quat,
) -> Option<MowerSupport> {
    let wheels = [
        Vec3::new(-8., 0., -9.),
        Vec3::new(8., 0., -9.),
        Vec3::new(-8., 0., 9.),
        Vec3::new(8., 0., 9.),
    ];
    let mut points = [Vec3::ZERO; 4];
    let mut root_y = f32::NEG_INFINITY;
    for (i, wheel) in wheels.into_iter().enumerate() {
        let offset = rotation * wheel;
        let origin = feet * VOXELS_PER_UNIT + Vec3::new(offset.x, 16.25, offset.z);
        let height = wheel_surface_height(world, origin)?;
        points[i] = Vec3::new(origin.x, height, origin.z);
        root_y =
            root_y.max(height - offset.y + re_flora_physics::CAPSULE_CHARACTER_COLLISION_OFFSET);
    }
    let right = (points[1] + points[3] - points[0] - points[2]) * 0.5;
    let forward = (points[2] + points[3] - points[0] - points[1]) * 0.5;
    let normal = forward.cross(right).normalize_or_zero();
    if normal.y < 0.5 || !normal.is_finite() {
        return None;
    }
    Some(MowerSupport {
        position: Vec3::new(feet.x, root_y / VOXELS_PER_UNIT, feet.z),
        normal,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracer::StaticSceneMesh;
    use glam::Vec4;

    fn model_world(boxes: &[(Vec3, Vec3)]) -> CollisionWorld {
        let mut mesh = StaticSceneMesh::default();
        for &(min, max) in boxes {
            mesh.append_box(min, max, Vec4::ONE);
        }
        let (positions, triangles) = mesh.collision_geometry();
        let mut world = CollisionWorld::new();
        world
            .set_fixed_scene_surface(&positions, &triangles)
            .unwrap();
        world
    }
    fn floor() -> (Vec3, Vec3) {
        (Vec3::new(-100., -10., -100.), Vec3::new(100., 0., 100.))
    }
    fn drive(world: &mut CollisionWorld, mut feet: Vec3, frames: usize) -> Vec3 {
        for _ in 0..frames {
            if let Some(next) = MOWER
                .follow(world, feet, Vec3::X * (0.5 / 256.), 1. / 60.)
                .unwrap()
            {
                feet = next;
            }
        }
        feet
    }
    #[test]
    fn wheel_support_recovers_sloped_model_normal_and_stair_slope() {
        // y = x/4 + z/5: both pitch and roll must be present.
        let positions = [(-50., -50.), (-50., 50.), (50., 50.), (50., -50.)]
            .map(|(x, z)| Vec3::new(x, x / 4. + z / 5., z));
        let mut world = CollisionWorld::new();
        world
            .set_fixed_scene_surface(&positions, &[[0, 1, 2], [0, 2, 3]])
            .unwrap();
        let frame = mower_support(&mut world, Vec3::ZERO, Quat::IDENTITY).unwrap();
        assert!(
            frame
                .normal
                .distance(Vec3::new(-0.25, 1., -0.2).normalize())
                < 1e-5
        );
        assert!(
            frame.position.y > 0.,
            "upright model should clear raised wheel contacts"
        );
        let rotation = Quat::from_rotation_arc(Vec3::Y, frame.normal);
        let tilted = mower_support(&mut world, Vec3::ZERO, rotation).unwrap();
        assert!(
            (tilted.position.y * 256. - re_flora_physics::CAPSULE_CHARACTER_COLLISION_OFFSET).abs()
                < 1e-4,
            "tilted wheel contacts must sit on the plane: {:?}",
            tilted.position
        );
        let mut boxes = vec![floor()];
        for i in 0..24 {
            boxes.push((
                Vec3::new(-36. + i as f32 * 3., 0., -40.),
                Vec3::new(100., i as f32 + 1., 40.),
            ));
        }
        let mut world = model_world(&boxes);
        let frame = mower_support(&mut world, Vec3::Y * (13. / 256.), Quat::IDENTITY).unwrap();
        assert!(frame.normal.x < -0.2 && frame.normal.y > 0.8);
    }

    #[test]
    fn contact_smoothing_preserves_ledges_and_unsupported_edges() {
        let mut world =
            model_world(&[floor(), (Vec3::new(0., 0., -40.), Vec3::new(100., 8., 40.))]);
        for x in [-0.1, 0.1] {
            let origin = Vec3::new(x, 16., 0.);
            assert_eq!(
                wheel_surface_height(&mut world, origin),
                walkable_surface_height(&mut world, origin),
                "a tall ledge must not become an interpolated ramp"
            );
        }
        let mut world = model_world(&[(Vec3::new(-100., -10., -100.), Vec3::new(0., 0., 100.))]);
        assert_eq!(
            wheel_surface_height(&mut world, Vec3::new(-0.1, 16., 0.)),
            Some(0.)
        );
        assert_eq!(
            wheel_surface_height(&mut world, Vec3::new(0.1, 16., 0.)),
            None
        );
    }

    #[test]
    fn moving_wheel_support_is_continuous_on_a_gentle_voxel_slope() {
        use glam::{IVec3, UVec3};
        use re_flora_physics::{BrickOccupancy, StaticVoxelBrickId};
        let mut filled = Vec::new();
        for x in 0..32 {
            for z in 0..32 {
                for y in 0..1 + x / 4 {
                    filled.push(UVec3::new(x, y, z));
                }
            }
        }
        let mut world = CollisionWorld::new();
        world.upsert_static_voxel_brick(
            StaticVoxelBrickId(IVec3::ZERO),
            1,
            BrickOccupancy::from_filled_voxels(filled),
        );
        let rotation = Quat::from_rotation_arc(Vec3::Y, Vec3::new(-0.25, 1., 0.).normalize());
        let mut previous: Option<MowerSupport> = None;
        for frame in 0..120 {
            let feet = Vec3::new(10. + frame as f32 * 0.1, 5., 16.) / 256.;
            let support = mower_support(&mut world, feet, rotation).unwrap();
            if let Some(previous) = previous {
                let rise = (support.position.y - previous.position.y).abs() * 256.;
                assert!(
                    rise < 0.15,
                    "wheel support jumped {rise} voxels in one frame"
                );
                let angle = previous.normal.angle_between(support.normal);
                assert!(angle < 0.02, "support normal snapped by {angle} radians");
            }
            previous = Some(support);
        }
    }

    #[test]
    fn mower_places_on_the_complete_rooftop_shell_after_translation() {
        let scene = crate::app::core::rooftop_scene::RooftopScene::new();
        let (positions, triangles) = scene.mesh().collision_geometry();
        for offset in [Vec3::ZERO, Vec3::new(-180., -24., -160.)] {
            let mut world = CollisionWorld::new();
            let positions: Vec<_> = positions.iter().map(|p| *p * 256. + offset).collect();
            world
                .set_fixed_scene_surface(&positions, &triangles)
                .unwrap();
            world.advance(1. / 60.);
            let surface = (Vec3::new(434., 216., 406.) + offset) / 256.;
            assert!(
                MOWER.place(&mut world, surface).unwrap().is_some(),
                "mower rejected full scene offset={offset:?}"
            );
        }
    }

    #[test]
    fn mower_places_and_drives_on_bare_fixed_roof_and_road() {
        for height in [192., 64.] {
            let mut world = model_world(&[(
                Vec3::new(-100., height - 10., -100.),
                Vec3::new(100., height, 100.),
            )]);
            let start = MOWER
                .place(&mut world, Vec3::new(-20., height, 0.) / 256.)
                .unwrap()
                .unwrap();
            let end = drive(&mut world, start, 100);
            assert!(end.x > 20. / 256., "no horizontal progress on fixed model");
            assert!((end.y * 256. - height).abs() < 0.2);
        }
    }
    #[test]
    fn mower_climbs_five_voxel_bump_that_old_four_voxel_sampler_rejected() {
        let mut world =
            model_world(&[floor(), (Vec3::new(0., 0., -40.), Vec3::new(100., 5., 40.))]);
        let start = MOWER
            .place(&mut world, Vec3::new(-25., 0., 0.) / 256.)
            .unwrap()
            .unwrap();
        let end = drive(&mut world, start, 110);
        assert!(end.x > 20. / 256., "stuck at small bump: {end:?}");
        assert!((end.y * 256. - 5.).abs() < 0.2);
    }
    #[test]
    fn mower_climbs_voxel_stair_slope_but_not_a_tall_wall() {
        let mut boxes = vec![floor()];
        for i in 0..24 {
            boxes.push((
                Vec3::new(i as f32 * 3., 0., -40.),
                Vec3::new(100., i as f32 + 1., 40.),
            ));
        }
        let mut world = model_world(&boxes);
        let start = MOWER
            .place(&mut world, Vec3::new(-25., 0., 0.) / 256.)
            .unwrap()
            .unwrap();
        let end = drive(&mut world, start, 210);
        assert!(
            end.x > 65. / 256. && end.y > 20. / 256.,
            "slope blocked: {end:?}"
        );
        let mut wall = model_world(&[
            floor(),
            (Vec3::new(0., 0., -100.), Vec3::new(50., 60., 100.)),
        ]);
        let start = MOWER
            .place(&mut wall, Vec3::new(-25., 0., 0.) / 256.)
            .unwrap()
            .unwrap();
        let end = drive(&mut wall, start, 140);
        assert!(
            end.x < 0. && end.y < 1. / 256.,
            "climbed tall wall: {end:?}"
        );
    }
    #[test]
    fn mower_uses_the_same_step_solver_on_real_voxel_bumps_and_slopes() {
        use glam::{IVec3, UVec3};
        use re_flora_physics::{BrickOccupancy, StaticVoxelBrickId};
        let occupancy = |slope: bool, raised: bool| {
            let mut filled = Vec::new();
            for x in 0..32 {
                for z in 0..32 {
                    let height = if !raised {
                        1
                    } else if slope {
                        1 + x / 3
                    } else {
                        6
                    };
                    for y in 0..height {
                        filled.push(UVec3::new(x, y, z));
                    }
                }
            }
            BrickOccupancy::from_filled_voxels(filled)
        };
        for slope in [false, true] {
            let mut world = CollisionWorld::new();
            world.upsert_static_voxel_brick(
                StaticVoxelBrickId(IVec3::new(-1, 0, 0)),
                1,
                occupancy(slope, false),
            );
            world.upsert_static_voxel_brick(
                StaticVoxelBrickId(IVec3::ZERO),
                1,
                occupancy(slope, true),
            );
            let start = MOWER
                .place(&mut world, Vec3::new(-15., 1., 16.) / 256.)
                .unwrap()
                .unwrap();
            let end = drive(&mut world, start, 76);
            assert!(
                end.x > 18. / 256. && end.y >= 5.9 / 256.,
                "voxel surface blocked slope={slope}: {end:?}"
            );
        }
    }

    #[test]
    fn mower_does_not_float_across_a_large_gap() {
        let mut world = model_world(&[
            (Vec3::new(-100., -10., -100.), Vec3::new(0., 0., 100.)),
            (Vec3::new(45., -10., -100.), Vec3::new(100., 0., 100.)),
        ]);
        let start = MOWER
            .place(&mut world, Vec3::new(-25., 0., 0.) / 256.)
            .unwrap()
            .unwrap();
        let end = drive(&mut world, start, 220);
        assert!(end.x < 20. / 256., "crossed unsupported gap: {end:?}");
    }
}
