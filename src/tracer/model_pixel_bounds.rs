//! Conservative camera-space bounds for frame tile visibility; no geometry sampling.
use glam::{Mat4, Vec3, Vec4};
pub fn tile_bounds(center: Vec3, size: f32, view: Mat4, projection: Mat4) -> Vec4 {
    let center = view.transform_point3(center);
    let r = size * (1.53125 * 0.5);
    let near = projection.inverse() * Vec4::new(0., 0., 0., 1.);
    let near_z = near.z / near.w;
    if center.z - r >= near_z {
        return Vec4::new(2., 2., 3., 3.);
    }
    let mut lo = glam::Vec2::splat(f32::INFINITY);
    let mut hi = glam::Vec2::splat(f32::NEG_INFINITY);
    for i in 0..8 {
        let p = Vec4::new(
            center.x + if i & 1 == 0 { -r } else { r },
            center.y + if i & 2 == 0 { -r } else { r },
            if i & 4 == 0 {
                center.z - r
            } else {
                (center.z + r).min(near_z)
            },
            1.,
        );
        let clip = projection * p;
        let ndc = glam::Vec2::new(clip.x, clip.y) / clip.w;
        lo = lo.min(ndc);
        hi = hi.max(ndc);
    }
    Vec4::new(lo.x, lo.y, hi.x, hi.y)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn visibility_bounds_remain_finite_across_near_plane_and_eye() {
        let projection = Mat4::perspective_rh(0.8, 1.6, 0.001, 10.);
        for z in [-2., -0.1, -0.03, -0.024, -0.023, -0.02, -0.001, 0., 0.02] {
            let rect = tile_bounds(Vec3::new(0., 0., z), 0.03, Mat4::IDENTITY, projection);
            assert!(rect.is_finite());
            assert!(rect.z > rect.x && rect.w > rect.y);
        }
        assert_eq!(
            tile_bounds(Vec3::Z, 0.03, Mat4::IDENTITY, projection),
            Vec4::new(2., 2., 3., 3.)
        );
    }
}
