use crate::tracer::voxel_encoding::{
    encode_gradient, encode_pos_with_offset, encode_voxel_offset, make_value_from_parts,
};
use crate::tracer::voxel_geometry::{CUBE_INDICES, VOXEL_VERTICES};
use crate::tracer::Vertex;
use anyhow::Result;
use glam::{IVec3, UVec3};
use noise::{NoiseFn, Perlin};

/// Generates indexed voxel data for sphere-shaped leaves.
///
/// # Parameters
/// - `density`: Controls how dense the leaves are (0.0 to 1.0)
/// - `radius`: Radius of the sphere (max 128 due to encoding constraints)
///
/// # Returns
/// A tuple of (vertices, indices) for rendering the voxel leaves.
pub fn generate_indexed_voxel_leaves(density: f32, radius: f32) -> Result<(Vec<Vertex>, Vec<u32>)> {
    if radius > 128.0 {
        return Err(anyhow::anyhow!(
            "Radius must be <= 128 due to encoding constraints"
        ));
    }

    if density <= 0.0 {
        return Ok((Vec::new(), Vec::new()));
    }

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    let noise = Perlin::new(42); // Fixed seed for consistent results
    let radius_i = radius as i32;

    // Iterate through a bounding box around the sphere
    for x in -radius_i..=radius_i {
        for y in -radius_i..=radius_i {
            for z in -radius_i..=radius_i {
                let pos = IVec3::new(x, y, z);
                let distance_from_center = pos.as_vec3().length();

                if distance_from_center > radius {
                    continue;
                }

                let gradient = if radius > 0.0 {
                    (distance_from_center / radius).min(1.0)
                } else {
                    0.0
                };

                // Use noise to determine if we should place a voxel here
                let noise_freq = 1.1;
                let noise_value = noise.get([
                    x as f64 * noise_freq,
                    y as f64 * noise_freq,
                    z as f64 * noise_freq,
                ]);
                let noise_threshold = (1.0 - density) as f64; // Higher density = lower threshold

                if noise_value > noise_threshold {
                    let vertex_offset = vertices.len() as u32;

                    // Convert to unsigned coordinates with offset
                    let pre_offset = IVec3::new(128, 128, 128);
                    let unsigned_pos = (IVec3::new(x, y, z) + pre_offset).as_uvec3();

                    append_indexed_cube_data(
                        &mut vertices,
                        &mut indices,
                        unsigned_pos,
                        gradient,
                        vertex_offset,
                    )?;
                }
            }
        }
    }

    Ok((vertices, indices))
}

/// Appends 8 vertices and 36 indices for a single cube to the provided lists.
fn append_indexed_cube_data(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    base_position: UVec3,
    color_gradient: f32,
    vertex_offset: u32,
) -> Result<()> {
    // Use shared voxel vertices
    let voxel_verts = VOXEL_VERTICES;

    let encoded_pos = encode_pos_with_offset(base_position, 0)?;
    let encoded_gradient = encode_gradient(color_gradient)?;

    for voxel_vert in voxel_verts {
        let encoded_offset = encode_voxel_offset(voxel_vert)?;
        let packed_data = make_value_from_parts(encoded_pos, encoded_offset, encoded_gradient);
        vertices.push(Vertex { packed_data });
    }

    // Use shared cube indices
    let base_indices = &CUBE_INDICES;

    for &index in base_indices {
        indices.push(vertex_offset + index);
    }

    Ok(())
}
