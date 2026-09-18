use glam::Vec3;
use re_flora_physics::{CollisionWorld, DeformingGeometry};

#[test]
fn editing_candidates_share_current_physics_geometry_across_updates() {
    let mut world = CollisionWorld::new();
    let publish = |world: &mut CollisionWorld, centers: &[Vec3]| {
        world.set_deforming_geometry(DeformingGeometry::Boxes {
            centers,
            half_extents: Vec3::splat(0.5),
        })
    };
    let origin = Vec3::new(0., 4., 0.);
    assert!(world.deforming_ray_candidates(origin, -Vec3::Y).is_empty());
    publish(&mut world, &[Vec3::ZERO, Vec3::X * 3.]).unwrap();
    assert_eq!(world.deforming_ray_candidates(origin, -Vec3::Y), [0]);
    assert_eq!(world.deforming_ray_candidates(Vec3::ZERO, Vec3::Y), [0]);
    publish(&mut world, &[Vec3::X * 3., Vec3::ZERO]).unwrap();
    assert_eq!(world.deforming_ray_candidates(origin, -Vec3::Y), [1]);
    assert!(publish(&mut world, &[Vec3::splat(f32::NAN)]).is_err());
    assert_eq!(world.deforming_ray_candidates(origin, -Vec3::Y), [1]);
    assert!(world
        .deforming_ray_candidates(origin, Vec3::ZERO)
        .is_empty());
    assert!(world
        .deforming_ray_candidates(Vec3::splat(f32::NAN), Vec3::Y)
        .is_empty());

    let points = [
        Vec3::new(-1., 0., -1.),
        Vec3::new(1., 0., -1.),
        Vec3::new(0., 0., 1.),
        Vec3::new(5., 0., -1.),
        Vec3::new(7., 0., -1.),
        Vec3::new(6., 0., 1.),
    ];
    world.set_deforming_surface(&points, &[[0, 1, 2]]).unwrap();
    assert_eq!(world.deforming_ray_candidates(origin, -Vec3::Y), [0]);
    world.set_deforming_surface(&points, &[[3, 4, 5]]).unwrap();
    assert!(world.deforming_ray_candidates(origin, -Vec3::Y).is_empty());
    assert_eq!(
        world.deforming_ray_candidates(origin + Vec3::X * 6., -Vec3::Y),
        [0]
    );
    publish(&mut world, &[]).unwrap();
    assert!(world
        .deforming_ray_candidates(origin + Vec3::X * 6., -Vec3::Y)
        .is_empty());
}
