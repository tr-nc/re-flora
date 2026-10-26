//! Original small insect mesh: ellipsoidal head/thorax, paired articulated wings
//! and a luminous abdomen. It enters the ordinary cached-angle model bank, never
//! the terrain atlas or voxel particle path. Material IDs: body 0, wing 1, tail 2.
use super::{triangle, Triangle};
use glam::{Quat, Vec2, Vec3};
pub(super) fn ellipsoid(
    out: &mut Vec<Triangle>,
    center: Vec3,
    radius: Vec3,
    rotation: Quat,
    material: u32,
) {
    let point = |lat: u32, lon: u32| {
        let t = std::f32::consts::PI * lat as f32 / 6.;
        let p = std::f32::consts::TAU * lon as f32 / 12.;
        let unit = Vec3::new(t.sin() * p.cos(), t.cos(), t.sin() * p.sin());
        (
            center + rotation * (unit * radius),
            rotation * (unit / radius).normalize(),
        )
    };
    for lat in 0..6 {
        for lon in 0..12 {
            let a = point(lat, lon);
            let b = point(lat + 1, lon);
            let c = point(lat + 1, lon + 1);
            let d = point(lat, lon + 1);
            for v in [[a, c, b], [a, d, c]] {
                if (v[1].0 - v[0].0).cross(v[2].0 - v[0].0).length_squared() < 1e-12 {
                    continue;
                }
                out.push(triangle(
                    v.map(|p| p.0),
                    v.map(|p| p.1),
                    [Vec2::ZERO; 3],
                    material,
                ));
            }
        }
    }
}
pub(super) fn mesh(phase: u32) -> Vec<Triangle> {
    let mut out = Vec::new();
    ellipsoid(
        &mut out,
        Vec3::new(0., 0., -0.1),
        Vec3::new(0.18, 0.14, 0.3),
        Quat::IDENTITY,
        0,
    );
    ellipsoid(
        &mut out,
        Vec3::new(0., 0., -0.40),
        Vec3::splat(0.13),
        Quat::IDENTITY,
        0,
    );
    ellipsoid(
        &mut out,
        Vec3::new(0., -0.025, 0.29),
        Vec3::new(0.21, 0.16, 0.29),
        Quat::IDENTITY,
        2,
    );
    let angle = [-0.25f32, 0.30, 0.85, 0.30][phase as usize % 4];
    for side in [-1., 1.] {
        let rotation = Quat::from_rotation_z(angle * side);
        ellipsoid(
            &mut out,
            Vec3::new(side * 0.13, 0.08, 0.) + rotation * Vec3::new(side * 0.30, 0., 0.),
            Vec3::new(0.42, 0.025, 0.23),
            rotation,
            1,
        );
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_wing_poses_have_finite_geometry_and_a_real_emissive_abdomen() {
        for phase in 0..4 {
            let triangles = mesh(phase);
            assert!(triangles.len() > 100);
            for material in 0..3 {
                assert!(triangles.iter().any(|t| t.uv2[2] == material as f32));
            }
            assert!(triangles.iter().flat_map(|t| t.a).all(f32::is_finite));
        }
        assert_ne!(mesh(0)[400].a, mesh(2)[400].a);
    }
}
