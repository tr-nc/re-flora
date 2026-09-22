//! Deterministic ordinary-terrain fixture: isolated cells, rods, a thin sheet and
//! a thick control surface. All are rock, not raster-tree geometry.
use crate::geom::Cuboid;
use glam::Vec3;

pub(super) fn rock() -> Vec<Cuboid> {
    let mut shapes = vec![
        Cuboid::from_min_max(Vec3::new(230., 176., 280.), Vec3::new(298., 178., 312.)),
        // Broad control block on the right, one-voxel sheet beside it.
        Cuboid::from_min_max(Vec3::new(278., 183., 298.), Vec3::new(294., 207., 306.)),
        Cuboid::from_min_max(Vec3::new(261., 184., 298.), Vec3::new(273., 203., 299.)),
    ];
    for y in [186., 194., 202.] {
        for x in [237., 242., 247.] {
            let min = Vec3::new(x, y, 300.);
            shapes.push(Cuboid::from_min_max(min, min + Vec3::ONE));
        }
    }
    shapes.push(Cuboid::from_min_max(
        Vec3::new(252., 184., 300.),
        Vec3::new(253., 205., 301.),
    ));
    shapes.push(Cuboid::from_min_max(
        Vec3::new(234., 212., 300.),
        Vec3::new(256., 213., 301.),
    ));
    shapes
}

pub(super) fn camera_pose() -> (Vec3, Vec3) {
    (
        Vec3::new(263.5, 199., 354.) / 256.,
        Vec3::new(263.5, 199., 300.) / 256.,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_contains_isolated_voxels_and_stays_in_publication_bounds() {
        let shapes = rock();
        assert_eq!(shapes.len(), 14);
        assert_eq!(
            shapes
                .iter()
                .filter(|c| c.aabb().max() - c.aabb().min() == Vec3::ONE)
                .count(),
            9
        );
        for shape in shapes {
            assert!(shape
                .aabb()
                .min()
                .cmpge(super::super::TEST_REBUILD_MIN.as_vec3())
                .all());
            assert!(shape
                .aabb()
                .max()
                .cmple(super::super::TEST_REBUILD_MAX.as_vec3())
                .all());
        }
    }
}
