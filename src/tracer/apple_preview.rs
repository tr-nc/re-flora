//! Game adapters for the exact apple geometry authored in apple-source.mjs.
//! The legacy voxel mesh and collision shape remain available for the A/B mode.
use glam::Vec3;
use serde::Deserialize;
use std::sync::OnceLock;

const APPLE_BYTES: &str = include_str!("../../assets/models/apple-preview.json");
const VOXEL_SCALE: f32 = 1.0 / 256.0;
const APPLE_RADIUS_VOXELS: f32 = 2.0;

#[derive(Deserialize)]
struct PublishedApple {
    parts: Vec<ApplePart>,
}
#[derive(Deserialize)]
struct ApplePart {
    material: u32,
    positions: Vec<f32>,
    indices: Vec<u32>,
}
pub struct AppleMesh {
    pub positions: Vec<Vec3>, // voxel units, relative to the existing fruit anchor
    pub normals: Vec<Vec3>,
    pub materials: Vec<u32>,
    pub indices: Vec<u32>,
}

pub fn mesh() -> &'static AppleMesh {
    static MESH: OnceLock<AppleMesh> = OnceLock::new();
    MESH.get_or_init(|| {
        let published: PublishedApple =
            serde_json::from_str(APPLE_BYTES).expect("validated apple mesh");
        let mut result = AppleMesh {
            positions: Vec::new(),
            normals: Vec::new(),
            materials: Vec::new(),
            indices: Vec::new(),
        };
        for part in published.parts {
            assert!(
                part.material < 3
                    && part.positions.len().is_multiple_of(3)
                    && part.indices.len().is_multiple_of(3)
            );
            let base = result.positions.len() as u32;
            let points = part
                .positions
                .chunks_exact(3)
                .map(|p| Vec3::from_slice(p) * APPLE_RADIUS_VOXELS)
                .collect::<Vec<_>>();
            let mut normals = vec![Vec3::ZERO; points.len()];
            for tri in part.indices.chunks_exact(3) {
                let [a, b, c] = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
                assert!(a < points.len() && b < points.len() && c < points.len());
                let normal = (points[b] - points[a]).cross(points[c] - points[a]);
                normals[a] += normal;
                normals[b] += normal;
                normals[c] += normal;
            }
            result
                .normals
                .extend(normals.into_iter().map(|v| v.normalize_or_zero()));
            result
                .materials
                .extend(std::iter::repeat_n(part.material, points.len()));
            result.positions.extend(points);
            result.indices.extend(part.indices.iter().map(|i| base + i));
            // The thin leaf is DoubleSide in Three.js. The game fruit pipelines
            // cull back faces, so include its reverse winding as well.
            if part.material == 2 {
                for tri in part.indices.chunks_exact(3) {
                    result
                        .indices
                        .extend([base + tri[2], base + tri[1], base + tri[0]]);
                }
            }
        }
        result
    })
}

/// Marker bit + three signed 1/16-voxel coordinates + two material bits.
/// Legacy flora vertices never set the high bit. Fractional positions preserve
/// the stem and silhouette without changing the tree instance ABI.
pub fn pack_vertex(position: Vec3, material: u32) -> u32 {
    let packed = position.to_array().map(|v| {
        let quantized = (v * 16.0).round() as i32 + 128;
        assert!(
            (0..512).contains(&quantized),
            "apple vertex exceeds packed position range"
        );
        quantized as u32
    });
    assert!(material < 3);
    0x8000_0000 | packed[0] | (packed[1] << 9) | (packed[2] << 18) | (material << 27)
}

pub fn world_position(position: Vec3) -> Vec3 {
    position * VOXEL_SCALE
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_mesh_has_three_materials_and_fits_legacy_apple_scale() {
        let mesh = mesh();
        assert_eq!(mesh.positions.len(), mesh.materials.len());
        assert!(mesh.indices.len() > 2000);
        assert!(mesh.positions.iter().all(|p| p.abs().max_element() < 4.0));
        for (&p, &material) in mesh.positions.iter().zip(&mesh.materials) {
            let packed = pack_vertex(p, material);
            let axis = |packed: u32, shift: u32| ((packed >> shift) & 511) as f32 / 16.0 - 8.0;
            assert!(
                (p - Vec3::new(axis(packed, 0), axis(packed, 9), axis(packed, 18)))
                    .abs()
                    .max_element()
                    <= 1.0 / 32.0 + 1e-6
            );
        }
    }
}
