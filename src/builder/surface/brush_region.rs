//! Conservative, chunk-local work tiles for a capsule. Surface halo sampling remains
//! owned by the shared extractor; expanding the dispatch by its halo is unnecessary.
use glam::{UVec3, Vec3};

pub(super) fn surface_region(
    start: Vec3,
    end: Vec3,
    radius: f32,
    chunk_offset: UVec3,
    chunk_dim: UVec3,
) -> (UVec3, UVec3) {
    let origin = chunk_offset.as_vec3();
    // A one-voxel conservative margin includes voxel centers on either boundary.
    let min = (start.min(end) - Vec3::splat(radius + 1.) - origin)
        .floor()
        .max(Vec3::ZERO)
        .min(chunk_dim.as_vec3())
        .as_uvec3();
    let max = (start.max(end) + Vec3::splat(radius + 1.) - origin)
        .ceil()
        .max(Vec3::ZERO)
        .min(chunk_dim.as_vec3())
        .as_uvec3();
    let offset = min / 8 * 8;
    let end = ((max + UVec3::splat(7)) / 8 * 8).min(chunk_dim);
    (offset, end.saturating_sub(offset))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swept_capsule_centers_are_covered_in_every_crossed_chunk() {
        let start = Vec3::new(240., 127.5, 120.);
        let end = Vec3::new(270., 140., 160.);
        for x in 0..2 {
            let chunk = UVec3::new(x * 256, 0, 0);
            let (offset, dim) = surface_region(start, end, 14., chunk, UVec3::splat(256));
            assert_eq!(offset % 8, UVec3::ZERO);
            assert!((offset + dim).cmple(UVec3::splat(256)).all());
            for i in 0..=32 {
                let point = start.lerp(end, i as f32 / 32.);
                for d in [
                    Vec3::ZERO,
                    Vec3::X,
                    -Vec3::X,
                    Vec3::Y,
                    -Vec3::Y,
                    Vec3::Z,
                    -Vec3::Z,
                ] {
                    let p = (point + d * 14.).floor().as_uvec3();
                    if (p / 256) == (chunk / 256) {
                        let local = p - chunk;
                        assert!(local.cmpge(offset).all() && local.cmplt(offset + dim).all());
                    }
                }
            }
        }
    }

    #[test]
    fn point_is_local_and_world_edges_are_clipped() {
        let (min, dim) = surface_region(
            Vec3::splat(128.5),
            Vec3::splat(128.5),
            2.,
            UVec3::ZERO,
            UVec3::splat(256),
        );
        assert_eq!(min, UVec3::splat(120));
        assert_eq!(dim, UVec3::splat(16));
        let (min, dim) =
            surface_region(Vec3::ZERO, Vec3::ZERO, 14., UVec3::ZERO, UVec3::splat(256));
        assert_eq!(min, UVec3::ZERO);
        assert_eq!(dim, UVec3::splat(16));
    }
}
