//! One resident packed topology shared by both stem source adapters.
use super::voxel_geometry::{CUBE_INDICES, VOXEL_VERTICES};

pub const MAX_GRASS_BANDS: u32 = 8;

pub fn topology(bands: u32, ribbons: bool) -> (Vec<u32>, Vec<u32>) {
    let vertices = (0..bands * 8).collect();
    let cube = CUBE_INDICES.map(|i| {
        let p = VOXEL_VERTICES[i as usize];
        p.x | (p.y << 1) | (p.z << 2)
    });
    let ribbon = [0, 5, 7, 0, 7, 2, 1, 4, 6, 1, 6, 3];
    let base: &[u32] = if ribbons { &ribbon } else { &cube };
    let indices = (0..bands)
        .flat_map(|band| base.iter().map(move |corner| band * 8 + corner))
        .collect();
    (vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_triangle_has_one_material_band_and_valid_corners() {
        for ribbons in [false, true] {
            let (vertices, indices) = topology(MAX_GRASS_BANDS, ribbons);
            assert_eq!(vertices.len(), 64);
            assert_eq!(indices.len(), if ribbons { 96 } else { 288 });
            for triangle in indices.chunks_exact(3) {
                assert!(triangle.iter().all(|i| *i < 64));
                assert!(triangle.iter().all(|i| i / 8 == triangle[0] / 8));
            }
        }
    }
}
