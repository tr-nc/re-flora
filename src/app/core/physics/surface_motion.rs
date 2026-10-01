//! One character-world movement seam for the player and cursor-guided ground props.
//! Actor dimensions differ; terrain/model collision, stepping, sliding and snapping do not.
use glam::Vec3;
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
