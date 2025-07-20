use anyhow::Result;
use glam::UVec3;

/// Encodes a position into 24 bits (8 bits per component).
/// Each component must be in the range [0, 255].
pub fn encode_pos(pos: UVec3) -> Result<u32> {
    // pos.x (8 bits) | pos.y (8 bits) | pos.z (8 bits)
    let encoded = pos.x | (pos.y << 8) | (pos.z << 16);
    if encoded > 0xFFFFFF {
        return Err(anyhow::anyhow!("Invalid position"));
    }
    Ok(encoded)
}

/// Encodes a voxel offset (within a unit cube) into 3 bits.
/// Each component must be 0 or 1.
pub fn encode_voxel_offset(base_vert: UVec3) -> Result<u32> {
    let encoded = base_vert.x | (base_vert.y << 1) | (base_vert.z << 2);
    if encoded > 0x7 {
        return Err(anyhow::anyhow!("Invalid base vert"));
    }
    Ok(encoded)
}

/// Encodes a gradient value [0, 1] into 5 bits [0, 31].
pub fn encode_gradient(gradient: f32) -> Result<u32> {
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
pub fn make_value_from_parts(encoded_pos: u32, encoded_offset: u32, encoded_gradient: u32) -> u32 {
    encoded_pos | (encoded_offset << 24) | (encoded_gradient << 27)
}

/// Encodes a position with a 128 offset to handle negative coordinates.
/// This allows positions to range from [-128, 127] in each component.
pub fn encode_pos_with_offset(pos: UVec3, offset: u32) -> Result<u32> {
    let offset_pos = UVec3::new(
        pos.x.wrapping_add(offset),
        pos.y.wrapping_add(offset),
        pos.z.wrapping_add(offset),
    );
    encode_pos(offset_pos)
}
