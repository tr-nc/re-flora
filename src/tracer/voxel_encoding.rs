use anyhow::Result;
use glam::{IVec3, UVec3};

use crate::tracer::{
    voxel_geometry::{CUBE_INDICES, CUBE_INDICES_LOD, VOXEL_VERTICES, VOXEL_VERTICES_LOD},
    Vertex,
};

const BIT_PER_POS: u32 = 7;
const BIT_PER_OFFSET: u32 = 1;
const POS_BITS: u32 = BIT_PER_POS * 3;
const LOOKUP_BITS_PER_AXIS: u32 = 10;
pub const FLORA_VOXEL_LOOKUP_EMPTY_KEY: u32 = u32::MAX;

pub const FLORA_VOXEL_MATERIAL_GRADIENT: u8 = 0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FloraVoxelInfo {
    pub packed: u32,
}

impl FloraVoxelInfo {
    pub fn new(
        color_gradient: f32,
        wind_gradient: f32,
        growth_gradient: f32,
        material_id: u8,
    ) -> Self {
        let color = quantize_unorm8(color_gradient);
        let wind = quantize_unorm8(wind_gradient);
        let growth = quantize_unorm8(growth_gradient);
        Self {
            packed: color | (wind << 8) | (growth << 16) | ((material_id as u32) << 24),
        }
    }

    pub fn gradient(gradient: f32) -> Self {
        Self::new(gradient, gradient, gradient, FLORA_VOXEL_MATERIAL_GRADIENT)
    }

    pub const fn fallback() -> Self {
        Self { packed: 0 }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FloraVoxelInfoEntry {
    pub pos: IVec3,
    pub info: FloraVoxelInfo,
}

#[derive(Clone, Debug)]
pub struct FloraMeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub voxel_infos: Vec<FloraVoxelInfoEntry>,
    pub max_length: u32,
}

impl FloraMeshData {
    pub fn new(max_length: u32) -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
            voxel_infos: Vec::new(),
            max_length: max_length.max(1),
        }
    }

    pub fn into_render_data(self) -> (Vec<Vertex>, Vec<u32>) {
        (self.vertices, self.indices)
    }
}

fn quantize_unorm8(value: f32) -> u32 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u32
}

fn gradient_from_origin(pos: IVec3, origin: IVec3, max_length: u32) -> f32 {
    let denom = max_length.max(1) as f32;
    let dist = (pos - origin).as_vec3().length();
    (dist / denom).clamp(0.0, 1.0)
}

/// Encodes a position into BIT_PER_POS * 3 bits for the vertex stream.
fn encode_pos(pos: IVec3) -> Result<u32> {
    encode_vec3_with_bits(pos, BIT_PER_POS, "local position")
}

/// Encodes a local voxel position into a 30-bit lookup key.
///
/// The vertex stream still uses the historical 7-bit local position, but tree leaf
/// instances already carry signed 10-bit local offsets. The lookup table uses the
/// wider range so all flora render paths can share one key format.
pub fn encode_lookup_pos_key(pos: IVec3) -> Result<u32> {
    encode_vec3_with_bits(pos, LOOKUP_BITS_PER_AXIS, "flora lookup local position")
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

fn make_value_from_parts(encoded_pos: u32, encoded_offset: u32) -> u32 {
    encoded_pos | (encoded_offset << POS_BITS)
}

fn encode_vec3_with_bits(pos: IVec3, bits: u32, label: &str) -> Result<u32> {
    let offset: i32 = 1 << (bits - 1);
    let pos = pos + IVec3::splat(offset);

    let lower_bound: i32 = 0;
    let upper_bound: i32 = (1 << bits) - 1;
    if pos.x < lower_bound
        || pos.x > upper_bound
        || pos.y < lower_bound
        || pos.y > upper_bound
        || pos.z < lower_bound
        || pos.z > upper_bound
    {
        return Err(anyhow::anyhow!(
            "Invalid {} {:?}",
            label,
            pos - IVec3::splat(offset)
        ));
    }
    let pos = pos.as_uvec3();
    let encoded = pos.x | (pos.y << bits) | (pos.z << (bits * 2));
    Ok(encoded)
}

/// Appends indexed cube data and a default gradient-based voxel lookup entry.
pub fn append_indexed_cube_data(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    voxel_infos: &mut Vec<FloraVoxelInfoEntry>,
    pos: IVec3,
    vertex_offset: u32,
    origin: IVec3,
    max_length: u32,
    is_lod_used: bool,
) -> Result<()> {
    let gradient = gradient_from_origin(pos, origin, max_length);
    append_indexed_cube_data_with_info(
        vertices,
        indices,
        voxel_infos,
        pos,
        vertex_offset,
        FloraVoxelInfo::gradient(gradient),
        is_lod_used,
    )
}

/// Appends 8 vertices and 36 indices for a single cube to the provided lists.
pub fn append_indexed_cube_data_with_info(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    voxel_infos: &mut Vec<FloraVoxelInfoEntry>,
    pos: IVec3,
    vertex_offset: u32,
    info: FloraVoxelInfo,
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
        let packed_data = make_value_from_parts(encoded_pos, encoded_offset);
        vertices.push(Vertex { packed_data });
    }
    for index in base_indices {
        indices.push(vertex_offset + index);
    }
    voxel_infos.push(FloraVoxelInfoEntry { pos, info });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_key_encodes_signed_ten_bit_leaf_range() {
        assert_ne!(
            encode_lookup_pos_key(IVec3::new(-512, 0, 511)).unwrap(),
            FLORA_VOXEL_LOOKUP_EMPTY_KEY
        );
        assert!(encode_lookup_pos_key(IVec3::new(512, 0, 0)).is_err());
    }
}
