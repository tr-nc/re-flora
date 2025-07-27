use anyhow::Result;
use glam::{IVec3, UVec3};

use crate::tracer::{
    voxel_geometry::{CUBE_INDICES, VOXEL_VERTICES},
    Vertex,
};

const BIT_PER_POS: u32 = 7;
const BIT_PER_GRADIENT: u32 = 5;
const BIT_PER_OFFSET: u32 = 1;

/// Encodes a position into BIT_PER_POS * 3 bits.
fn encode_pos(pos: IVec3) -> Result<u32> {
    const OFFSET: i32 = 1 << (BIT_PER_POS - 1);
    let pos = pos + OFFSET;

    const LOWER_BOUND: i32 = 0;
    const UPPER_BOUND: i32 = (1 << BIT_PER_POS) - 1;
    if pos.x < LOWER_BOUND
        || pos.x > UPPER_BOUND
        || pos.y < LOWER_BOUND
        || pos.y > UPPER_BOUND
        || pos.z < LOWER_BOUND
        || pos.z > UPPER_BOUND
    {
        return Err(anyhow::anyhow!("Invalid position"));
    }
    let pos = pos.as_uvec3(); // this is safe now
    let encoded = pos.x | (pos.y << BIT_PER_POS) | (pos.z << (BIT_PER_POS * 2));
    Ok(encoded)
}

/// Encodes a voxel offset (within a unit cube) into BIT_PER_OFFSET bits.
fn encode_voxel_offset(base_vert: UVec3) -> Result<u32> {
    const LOWER_BOUND: u32 = 0;
    const UPPER_BOUND: u32 = (1 << BIT_PER_OFFSET) - 1;
    if base_vert.x < LOWER_BOUND
        || base_vert.x > UPPER_BOUND
        || base_vert.y < LOWER_BOUND
        || base_vert.y > UPPER_BOUND
        || base_vert.z < LOWER_BOUND
        || base_vert.z > UPPER_BOUND
    {
        return Err(anyhow::anyhow!("Invalid base vert"));
    }
    let encoded =
        base_vert.x | (base_vert.y << BIT_PER_OFFSET) | (base_vert.z << (BIT_PER_OFFSET * 2));
    Ok(encoded)
}

/// Encodes a gradient value [0, 1] into BIT_PER_GRADIENT bits.
fn encode_gradient(gradient: f32) -> Result<u32> {
    const LOWER_BOUND: f32 = 0.0;
    const UPPER_BOUND: f32 = 1.0;
    if gradient < LOWER_BOUND || gradient > UPPER_BOUND {
        return Err(anyhow::anyhow!("Invalid gradient"));
    }
    const MAX_GRADIENT: u32 = (1 << BIT_PER_GRADIENT) - 1;
    let encoded = (gradient * MAX_GRADIENT as f32) as u32;
    Ok(encoded)
}

fn make_value_from_parts(encoded_pos: u32, encoded_offset: u32, encoded_gradient: u32) -> u32 {
    const POS_BITS: u32 = BIT_PER_POS * 3;
    const OFFSET_BITS: u32 = BIT_PER_OFFSET * 3;
    // const GRADIENT_BITS: u32 = BIT_PER_GRADIENT;
    encoded_pos | (encoded_offset << POS_BITS) | (encoded_gradient << (POS_BITS + OFFSET_BITS))
}

/// Appends 8 vertices and 36 indices for a single cube to the provided lists.
pub fn append_indexed_cube_data(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    pos: IVec3,
    color_gradient: f32,
    vertex_offset: u32,
) -> Result<()> {
    const LOWER_BOUND: i32 = -(1 << (BIT_PER_POS - 1));
    const UPPER_BOUND: i32 = (1 << (BIT_PER_POS - 1)) - 1;
    if pos.x < LOWER_BOUND
        || pos.x > UPPER_BOUND
        || pos.y < LOWER_BOUND
        || pos.y > UPPER_BOUND
        || pos.z < LOWER_BOUND
        || pos.z > UPPER_BOUND
    {
        return Err(anyhow::anyhow!("Invalid local position"));
    }

    let encoded_pos = encode_pos(pos)?;
    let encoded_gradient = encode_gradient(color_gradient)?;

    for voxel_vert in VOXEL_VERTICES {
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
