use anyhow::Result;
use glam::{IVec3, UVec3};

use crate::tracer::{
    voxel_geometry::{CUBE_INDICES, CUBE_INDICES_LOD, VOXEL_VERTICES, VOXEL_VERTICES_LOD},
    Vertex,
};

const BIT_PER_POS: u32 = 7;
const BIT_PER_OFFSET: u32 = 1;

const BIT_PER_COLOR_GRADIENT: u32 = 4;
const BIT_PER_WIND_GRADIENT: u32 = 4;

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
    const UPPER_BOUND: u32 = (1 << BIT_PER_OFFSET) - 1;
    if base_vert.x > UPPER_BOUND || base_vert.y > UPPER_BOUND || base_vert.z > UPPER_BOUND {
        return Err(anyhow::anyhow!("Invalid base vert"));
    }
    let encoded =
        base_vert.x | (base_vert.y << BIT_PER_OFFSET) | (base_vert.z << (BIT_PER_OFFSET * 2));
    Ok(encoded)
}

/// Encodes a gradient value [0, 1] into BIT_PER_GRADIENT bits.
fn encode_gradients(color_gradient: f32, wind_gradient: f32) -> Result<u32> {
    const LOWER_BOUND: f32 = 0.0;
    const UPPER_BOUND: f32 = 1.0;
    if !(LOWER_BOUND..=UPPER_BOUND).contains(&color_gradient) {
        return Err(anyhow::anyhow!("Invalid color gradient"));
    }
    if !(LOWER_BOUND..=UPPER_BOUND).contains(&wind_gradient) {
        return Err(anyhow::anyhow!("Invalid wind gradient"));
    }

    const MAX_COLOR_GRADIENT: u32 = (1 << BIT_PER_COLOR_GRADIENT) - 1;
    const MAX_WIND_GRADIENT: u32 = (1 << BIT_PER_WIND_GRADIENT) - 1;
    let encoded_color_gradient = (color_gradient * MAX_COLOR_GRADIENT as f32) as u32;
    let encoded_wind_gradient = (wind_gradient * MAX_WIND_GRADIENT as f32) as u32;
    let encoded = encoded_color_gradient | (encoded_wind_gradient << BIT_PER_COLOR_GRADIENT);
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
    vertex_offset: u32,
    color_gradient: f32,
    wind_gradient: f32,
    is_lod_used: bool,
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
    let encoded_gradient = encode_gradients(color_gradient, wind_gradient)?;

    let voxel_verts: Vec<UVec3> = if is_lod_used {
        VOXEL_VERTICES_LOD.to_vec()
    } else {
        VOXEL_VERTICES.to_vec()
    };
    let base_indices = if is_lod_used {
        CUBE_INDICES_LOD.to_vec()
    } else {
        CUBE_INDICES.to_vec()
    };

    for voxel_vert in voxel_verts {
        let encoded_offset = encode_voxel_offset(voxel_vert)?;
        let packed_data = make_value_from_parts(encoded_pos, encoded_offset, encoded_gradient);
        vertices.push(Vertex { packed_data });
    }
    for index in base_indices {
        indices.push(vertex_offset + index);
    }

    Ok(())
}
