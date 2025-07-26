use anyhow::Result;
use glam::{IVec3, UVec3};

use crate::tracer::{
    voxel_geometry::{CUBE_INDICES, VOXEL_VERTICES},
    Vertex,
};

/// Encodes a position into 24 bits (8 bits per component).
/// Each component must be in the range [0, 255].
fn encode_pos(pos: IVec3) -> Result<u32> {
    if pos.x < 0 || pos.x > 255 || pos.y < 0 || pos.y > 255 || pos.z < 0 || pos.z > 255 {
        return Err(anyhow::anyhow!("Invalid position"));
    }
    let pos = pos.as_uvec3(); // this is safe now
    let encoded = pos.x | (pos.y << 8) | (pos.z << 16);
    Ok(encoded)
}

/// Encodes a voxel offset (within a unit cube) into 3 bits.
/// Each component must be 0 or 1.
fn encode_voxel_offset(base_vert: UVec3) -> Result<u32> {
    let encoded = base_vert.x | (base_vert.y << 1) | (base_vert.z << 2);
    if encoded > 0x7 {
        return Err(anyhow::anyhow!("Invalid base vert"));
    }
    Ok(encoded)
}

/// Encodes a gradient value [0, 1] into 5 bits [0, 31].
fn encode_gradient(gradient: f32) -> Result<u32> {
    // input gradient is in [0, 1]
    // output gradient is in [0, 31]
    let encoded = (gradient * 31.0) as u32;
    if encoded > 0x1F {
        return Err(anyhow::anyhow!("Invalid gradient"));
    }
    Ok(encoded)
}

/// Combines encoded position, offset, and gradient into a single 32-bit value.
/// Layout: position (24 bits) | offset (3 bits) | gradient (5 bits)
fn make_value_from_parts(encoded_pos: u32, encoded_offset: u32, encoded_gradient: u32) -> u32 {
    encoded_pos | (encoded_offset << 24) | (encoded_gradient << 27)
}

/// Appends 8 vertices and 36 indices for a single cube to the provided lists.
/// The input range of pos is [-128, 127] in each component.
pub fn append_indexed_cube_data(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    pos: IVec3,
    color_gradient: f32,
    vertex_offset: u32,
) -> Result<()> {
    if pos.x < -128 || pos.x > 127 || pos.y < -128 || pos.y > 127 || pos.z < -128 || pos.z > 127 {
        return Err(anyhow::anyhow!("Invalid local position"));
    }

    // Use shared voxel vertices
    let voxel_verts = VOXEL_VERTICES;

    const OFFSET: i32 = 128;
    let pos_with_offset = pos + OFFSET;
    let encoded_pos = encode_pos(pos_with_offset)?;
    let encoded_gradient = encode_gradient(color_gradient)?;

    for voxel_vert in voxel_verts {
        let encoded_offset = encode_voxel_offset(voxel_vert)?;
        let packed_data = make_value_from_parts(encoded_pos, encoded_offset, encoded_gradient);
        vertices.push(Vertex { packed_data });
    }
    let base_indices = &CUBE_INDICES;

    for &index in base_indices {
        indices.push(vertex_offset + index);
    }

    Ok(())
}
