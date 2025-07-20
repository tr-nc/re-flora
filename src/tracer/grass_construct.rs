use crate::tracer::Vertex;
use anyhow::Result;
use glam::UVec3;

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

        append_indexed_cube_data(
            &mut vertices,
            &mut indices,
            base_pos,
            color_gradient,
            vertex_offset,
        )?;
    }

    Ok((vertices, indices))
}

/// The output value is guaranteed to take up 24 bits
fn encode_pos(pos: UVec3) -> Result<u32> {
    // pos.x (8 bits) | pos.y (8 bits) | pos.z (8 bits)
    let encoded = pos.x | (pos.y << 8) | (pos.z << 16);
    if encoded > 0xFFFFFF {
        return Err(anyhow::anyhow!("Invalid position"));
    }
    Ok(encoded)
}

/// The output value is guaranteed to take up 3 bits
fn encode_voxel_offset(base_vert: UVec3) -> Result<u32> {
    // let encoded = base_vert.x * 4 + base_vert.y * 2 + base_vert.z;
    let encoded = base_vert.x | (base_vert.y << 1) | (base_vert.z << 2);
    if encoded > 0x7 {
        return Err(anyhow::anyhow!("Invalid base vert"));
    }
    Ok(encoded)
}

/// The output value is guaranteed to take up 5 bits
fn encode_gradient(gradient: f32) -> Result<u32> {
    // input gradient is in [0, 1]
    // output gradient is in [0, 31]
    let encoded = (gradient * 31.0) as u32;
    if encoded > 0x1F {
        return Err(anyhow::anyhow!("Invalid gradient"));
    }
    Ok(encoded)
}

fn make_value_from_parts(encoded_pos: u32, encoded_offset: u32, encoded_gradient: u32) -> u32 {
    encoded_pos | (encoded_offset << 24) | (encoded_gradient << 27)
}

/// Appends 8 vertices and 36 indices for a single cube to the provided lists.
fn append_indexed_cube_data(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    base_position: UVec3,
    color_gradient: f32,
    vertex_offset: u32,
) -> Result<()> {
    // Define the 8 vertices of a unit cube, starting with the bottom face.
    let voxel_verts = [
        UVec3::new(0, 0, 0),
        UVec3::new(1, 0, 0),
        UVec3::new(1, 0, 1),
        UVec3::new(0, 0, 1),
        UVec3::new(0, 1, 0),
        UVec3::new(1, 1, 0),
        UVec3::new(1, 1, 1),
        UVec3::new(0, 1, 1),
    ];

    let encoded_pos = encode_pos(base_position)?;
    let encoded_gradient = encode_gradient(color_gradient)?;

    for voxel_vert in voxel_verts {
        let encoded_offset = encode_voxel_offset(voxel_vert)?;
        let packed_data = make_value_from_parts(encoded_pos, encoded_offset, encoded_gradient);
        vertices.push(Vertex { packed_data });
    }

    // Define 36 indices for 12 triangles (6 faces).
    // The winding order is Counter-Clockwise (CCW) when viewed from the outside.
    let base_indices = vec![
        0, 1, 2, 0, 2, 3, // Bottom face (-Y)
        4, 6, 5, 4, 7, 6, // Top face (+Y)
        1, 0, 4, 1, 4, 5, // Back face (-Z)
        2, 7, 3, 2, 6, 7, // Front face (+Z)
        0, 3, 7, 0, 7, 4, // Left face (-X)
        1, 6, 2, 1, 5, 6, // Right face (+X)
    ];

    for &index in &base_indices {
        indices.push(vertex_offset + index);
    }

    Ok(())
}
