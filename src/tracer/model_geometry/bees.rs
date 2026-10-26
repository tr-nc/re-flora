//! Original low-poly bee and roofed solitary-bee nesting box. No terrain stamps
//! or external artwork. Materials survive the shared cached-angle surface bake.
use super::{firefly::ellipsoid, Triangle};
use glam::{Quat, Vec3};
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
}
