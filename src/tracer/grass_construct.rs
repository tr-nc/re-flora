use crate::tracer::voxel_encoding::{
    encode_gradient, encode_pos, encode_voxel_offset, make_value_from_parts,
};
use crate::tracer::voxel_geometry::{CUBE_INDICES, VOXEL_VERTICES};
use crate::tracer::Vertex;
use anyhow::Result;
use glam::{IVec3, UVec3};

pub fn generate_indexed_voxel_grass_blade(voxel_count: u32) -> Result<(Vec<Vertex>, Vec<u32>)> {
    if voxel_count == 0 {
        return Ok((Vec::new(), Vec::new()));
    }

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for i in 0..voxel_count {
        let vertex_offset = vertices.len() as u32;
        // let base_position = vec3(0.0, i as f32, 0.0);
        let base_pos = UVec3::new(0, i, 0);

        // Calculate color gradient: 0.0 for bottom (i=0), 1.0 for tip (i=voxel_count-1)
        let color_gradient = if voxel_count > 1 {
            i as f32 / (voxel_count - 1) as f32
        } else {
            0.0
        };

        let pre_offset = IVec3::new(128, 128, 128);
        let unsigned_pos = (base_pos.as_ivec3() + pre_offset).as_uvec3();
        
        append_indexed_cube_data(
            &mut vertices,
            &mut indices,
            unsigned_pos,
            color_gradient,
            vertex_offset,
        )?;
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

    let encoded_pos = encode_pos(base_position)?;
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
