//! Feeding articulation of the shared GLB, plus an original low-poly proboscis.
//! Model-local origin is directly above its mouth tip: the GPU flower attachment
//! therefore pins the mouth, not the wing/body center, to the live flower surface.
use super::{triangle, Triangle, REST_FRAMES};
use glam::{Mat3, Quat, Vec2, Vec3};

pub(super) fn mesh(frame: u32) -> Vec<Triangle> {
    let model = crate::model_assets::butterfly();
    let opening = 10. + 50. * frame as f32 / (REST_FRAMES - 1) as f32;
    let angle = ((180. - opening) * 0.5).to_radians();
    let transforms = model.transforms_with_rotations(
        0.,
        0,
        &[
            (model.node("Flight pose"), Quat::IDENTITY),
            (model.node("Wing hinge R"), Quat::from_rotation_z(angle)),
            (model.node("Wing hinge L"), Quat::from_rotation_z(-angle)),
        ],
    );
    let root = transforms[model.node("Flight pose")].w_axis.truncate();
    let mouth_offset = Vec3::new(0., 0., 0.38);
    let mut out: Vec<_> = model
        .triangles
        .iter()
        .map(|t| {
            let m = transforms[t.node];
            let nm = Mat3::from_mat4(m).inverse().transpose();
            triangle(
                t.positions
                    .map(|p| m.transform_point3(p) - root + mouth_offset),
                t.normals.map(|n| (nm * n).normalize()),
                t.uvs,
                0,
            )
        })
        .collect();
    // A narrow, slightly bent feeding tube, extending just INTO the flower.
    let points = [
        Vec3::new(0., 0.02, 0.08),
        Vec3::new(0., -0.08, 0.015),
        Vec3::new(
            0.,
            -crate::particles::butterfly_wingbeat::FEEDING_CLEARANCE - 0.015,
            0.,
        ),
    ];
    for ends in points.windows(2) {
        for side in 0..6 {
            let ring = |p: Vec3, i: usize| {
                let a = i as f32 * std::f32::consts::TAU / 6.;
                p + Vec3::new(a.cos(), 0., a.sin()) * 0.012
            };
            let a = ring(ends[0], side);
            let b = ring(ends[0], side + 1);
            let c = ring(ends[1], side);
            let d = ring(ends[1], side + 1);
            for p in [[a, b, c], [b, d, c]] {
                let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize();
                out.push(triangle(p, [n; 3], [Vec2::ZERO; 3], 0));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn feeding_pose_keeps_mouth_inside_contact_and_wings_above_surface() {
        for frame in 0..REST_FRAMES {
            let mesh = mesh(frame);
            let clearance = crate::particles::butterfly_wingbeat::FEEDING_CLEARANCE;
            let wing_count = crate::model_assets::butterfly().triangles.len();
            for (i, t) in mesh.iter().enumerate() {
                let a = Vec3::from_slice(&t.a);
                for p in [a, a + Vec3::from_slice(&t.e1), a + Vec3::from_slice(&t.e2)] {
                    assert!(p.is_finite() && p.length() < 1.7);
                    if i < wing_count {
                        assert!(p.y + clearance > 0.);
                    }
                }
            }
            assert!(mesh[wing_count..]
                .iter()
                .any(|t| t.a[1] + t.e2[1] + clearance < 0.));
        }
        assert_ne!(mesh(0)[0].a, mesh(REST_FRAMES - 1)[0].a);
    }
}
