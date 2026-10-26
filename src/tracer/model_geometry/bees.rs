//! Original low-poly bee and roofed solitary-bee nesting box. No terrain stamps
//! or external artwork. Materials survive the shared cached-angle surface bake.
use super::{firefly::ellipsoid, triangle, Triangle};
use glam::{Quat, Vec2, Vec3};
pub(super) fn bee(phase: u32) -> Vec<Triangle> {
    let mut out = Vec::new();
    ellipsoid(
        &mut out,
        Vec3::new(0., 0., -0.18),
        Vec3::new(0.19, 0.17, 0.22),
        Quat::IDENTITY,
        2,
    );
    ellipsoid(
        &mut out,
        Vec3::new(0., 0.025, -0.43),
        Vec3::new(0.15, 0.14, 0.14),
        Quat::IDENTITY,
        0,
    );
    let start = out.len();
    ellipsoid(
        &mut out,
        Vec3::new(0., -0.02, 0.22),
        Vec3::new(0.23, 0.18, 0.36),
        Quat::IDENTITY,
        2,
    );
    for t in &mut out[start..] {
        let z = t.a[2] + (t.e1[2] + t.e2[2]) / 3.;
        if ((z + 0.2) * 10.).floor() as i32 % 2 == 0 {
            t.uv2[2] = 0.;
        }
    }
    for side in [-1., 1.] {
        let angle = [-0.1f32, 0.5, 1.05, 0.5][phase as usize % 4];
        let rotation = Quat::from_rotation_z(angle * side);
        ellipsoid(
            &mut out,
            Vec3::new(side * 0.13, 0.12, -0.05) + rotation * Vec3::new(side * 0.29, 0., 0.),
            Vec3::new(0.4, 0.025, 0.25),
            rotation,
            1,
        );
        ellipsoid(
            &mut out,
            Vec3::new(side * 0.09, 0.16, -0.48),
            Vec3::new(0.018, 0.12, 0.018),
            Quat::from_rotation_z(side * 0.4),
            0,
        );
    }
    out
}
fn box_part(out: &mut Vec<Triangle>, center: Vec3, half: Vec3, rotation: Quat, material: u32) {
    let p: Vec<_> = (0..8)
        .map(|i| {
            center
                + rotation
                    * (Vec3::new(
                        if i & 1 == 0 { -1. } else { 1. },
                        if i & 2 == 0 { -1. } else { 1. },
                        if i & 4 == 0 { -1. } else { 1. },
                    ) * half)
        })
        .collect();
    for (indices, n) in [
        ([0, 4, 6, 2], -Vec3::X),
        ([1, 3, 7, 5], Vec3::X),
        ([0, 1, 5, 4], -Vec3::Y),
        ([2, 6, 7, 3], Vec3::Y),
        ([0, 2, 3, 1], -Vec3::Z),
        ([4, 5, 7, 6], Vec3::Z),
    ] {
        for ids in [[0, 1, 2], [0, 2, 3]] {
            out.push(triangle(
                ids.map(|j| p[indices[j]]),
                [rotation * n; 3],
                [Vec2::ZERO; 3],
                material,
            ));
        }
    }
}
pub(super) fn hive() -> Vec<Triangle> {
    let mut out = Vec::new();
    box_part(
        &mut out,
        Vec3::new(0., 0., -0.28),
        Vec3::new(0.5, 0.5, 0.045),
        Quat::IDENTITY,
        0,
    );
    for side in [-1., 1.] {
        box_part(
            &mut out,
            Vec3::new(side * 0.48, 0., 0.),
            Vec3::new(0.045, 0.5, 0.3),
            Quat::IDENTITY,
            0,
        );
        box_part(
            &mut out,
            Vec3::new(0., side * 0.48, 0.),
            Vec3::new(0.5, 0.045, 0.3),
            Quat::IDENTITY,
            0,
        );
        box_part(
            &mut out,
            Vec3::new(side * 0.29, 0.64, 0.),
            Vec3::new(0.37, 0.045, 0.4),
            Quat::from_rotation_z(-side * 0.45),
            1,
        );
    }
    // Nine open bamboo-like tubes, each with a recessed dark end and a rim.
    for y in [-0.3, 0., 0.3] {
        for x in [-0.3, 0., 0.3] {
            let c = Vec3::new(x, y, 0.3);
            for i in 0..12 {
                let a = std::f32::consts::TAU * i as f32 / 12.;
                let b = std::f32::consts::TAU * (i + 1) as f32 / 12.;
                let na = Vec3::new(a.cos(), a.sin(), 0.);
                let nb = Vec3::new(b.cos(), b.sin(), 0.);
                let outer = [c + na * 0.135, c + nb * 0.135];
                let inner = [c + na * 0.085, c + nb * 0.085];
                out.push(triangle(
                    [outer[0], outer[1], inner[1]],
                    [Vec3::Z; 3],
                    [Vec2::ZERO; 3],
                    2,
                ));
                out.push(triangle(
                    [outer[0], inner[1], inner[0]],
                    [Vec3::Z; 3],
                    [Vec2::ZERO; 3],
                    2,
                ));
                out.push(triangle(
                    [inner[0], inner[1], c - Vec3::Z * 0.24],
                    [Vec3::Z; 3],
                    [Vec2::ZERO; 3],
                    3,
                ));
                for p in [
                    [
                        outer[0],
                        outer[0] - Vec3::Z * 0.55,
                        outer[1] - Vec3::Z * 0.55,
                    ],
                    [outer[0], outer[1] - Vec3::Z * 0.55, outer[1]],
                ] {
                    out.push(triangle(p, [na, na, nb], [Vec2::ZERO; 3], 2));
                }
            }
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bees_have_stripes_wings_and_distinct_cached_poses() {
        for phase in 0..4 {
            let mesh = bee(phase);
            for material in 0..3 {
                assert!(mesh.iter().any(|t| t.uv2[2] == material as f32));
            }
            assert!(mesh.iter().flat_map(|t| t.a).all(f32::is_finite));
        }
        assert_ne!(bee(0)[400].a, bee(2)[400].a);
    }
    #[test]
    fn house_has_wood_roof_tubes_and_recessed_holes() {
        let mesh = hive();
        for material in 0..4 {
            assert!(mesh.iter().any(|t| t.uv2[2] == material as f32));
        }
        assert!(mesh.iter().flat_map(|t| t.a).all(f32::is_finite));
    }
}
