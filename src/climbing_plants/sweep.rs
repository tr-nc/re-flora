//! Conservative whole-segment sweep used by the stem mechanics.
use super::Terrain;
use glam::Vec3;

// The convex hull of both endpoint pairs encloses the entire segment sweep.
// SAT against radius-expanded voxels allows tangential/away motion at contacts
// without the false blocking of an isotropically inflated old segment.
pub(super) fn clear_sweep(terrain: &impl Terrain, points: [Vec3; 4], radius: f32) -> Option<bool> {
    let min = (points.into_iter().fold(points[0], Vec3::min) - Vec3::splat(radius))
        .floor()
        .as_ivec3();
    let max = (points.into_iter().fold(points[0], Vec3::max) + Vec3::splat(radius))
        .floor()
        .as_ivec3();
    for z in min.z..=max.z {
        for y in min.y..=max.y {
            for x in min.x..=max.x {
                let cell = glam::IVec3::new(x, y, z);
                let center = cell.as_vec3() + Vec3::splat(0.5);
                let p = points.map(|point| point - center);
                let extent = 0.5 + radius;
                let separated = |axis: Vec3| {
                    if axis.length_squared() < 1e-14 {
                        return false;
                    }
                    let lo = p
                        .into_iter()
                        .map(|v| v.dot(axis))
                        .fold(f32::INFINITY, f32::min);
                    let hi = p
                        .into_iter()
                        .map(|v| v.dot(axis))
                        .fold(f32::NEG_INFINITY, f32::max);
                    let bound = (extent + 0.0001) * axis.abs().element_sum();
                    lo > bound || hi < -bound
                };
                if [Vec3::X, Vec3::Y, Vec3::Z].into_iter().any(separated) {
                    continue;
                }
                if terrain.voxel(cell)? == 0 {
                    continue;
                }
                if [(0, 1, 2), (0, 1, 3), (0, 2, 3), (1, 2, 3)]
                    .into_iter()
                    .any(|(a, b, c)| separated((p[b] - p[a]).cross(p[c] - p[a])))
                {
                    continue;
                }
                if [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)]
                    .into_iter()
                    .any(|(a, b)| {
                        [Vec3::X, Vec3::Y, Vec3::Z]
                            .into_iter()
                            .any(|axis| separated((p[b] - p[a]).cross(axis)))
                    })
                {
                    continue;
                }
                return Some(false);
            }
        }
    }
    Some(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::climbing_plants::clear_segment;
    use glam::IVec3;
    struct Cube;
    impl Terrain for Cube {
        fn voxel(&self, cell: IVec3) -> Option<u8> {
            Some(u8::from(cell == IVec3::ZERO))
        }
        fn current(&self) -> bool {
            true
        }
    }
    #[test]
    fn swept_segment_checks_interior_and_allows_tangential_contact_motion() {
        let points = [
            Vec3::new(-2., -2., 0.5),
            Vec3::new(-2., 2., 0.5),
            Vec3::new(2., -2., 0.5),
            Vec3::new(2., 2., 0.5),
        ];
        for (a, b) in [(0, 1), (2, 3), (0, 2), (1, 3)] {
            assert_eq!(clear_segment(&Cube, points[a], points[b], 0.1), Some(true));
        }
        assert_eq!(
            clear_sweep(&Cube, points, 0.1),
            Some(false),
            "swept interior tunnels through voxel"
        );
        let points = [
            Vec3::new(-1., 0.5, 1.7),
            Vec3::new(2., 0.5, 1.7),
            Vec3::new(-1., 0., 1.7),
            Vec3::new(2., 0., 1.7),
        ];
        assert_eq!(clear_sweep(&Cube, points, 0.65), Some(true));
    }
}
