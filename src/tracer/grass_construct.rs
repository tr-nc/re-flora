use crate::tracer::Vertex;
use glam::{vec3, Vec3};

pub fn generate_indexed_voxel_grass_blade(
    voxel_count: u32,
) -> (Vec<Vertex>, Vec<u32>) {
    if voxel_count == 0 {
        return (Vec::new(), Vec::new());
    }

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for i in 0..voxel_count {
        let vertex_offset = vertices.len() as u32;
        let base_position = vec3(0.0, i as f32, 0.0);

        append_indexed_cube_data(
            &mut vertices,
            &mut indices,
            base_position,
            i,
            vertex_offset,
        );
    }

    (vertices, indices)
}

/// Appends 8 vertices and 36 indices for a single cube to the provided lists.
fn append_indexed_cube_data(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    base_position: Vec3,
    height: u32,
    vertex_offset: u32,
) {
    // Define the 8 vertices of a unit cube, starting with the bottom face.
    let base_verts = [
        // Bottom face vertices
        vec3(0.0, 0.0, 0.0), // 0
        vec3(1.0, 0.0, 0.0), // 1
        vec3(1.0, 0.0, 1.0), // 2
        vec3(0.0, 0.0, 1.0), // 3
        // Top face vertices
        vec3(0.0, 1.0, 0.0), // 4
        vec3(1.0, 1.0, 0.0), // 5
        vec3(1.0, 1.0, 1.0), // 6
        vec3(0.0, 1.0, 1.0), // 7
    ];

    for &local_pos in &base_verts {
        vertices.push(Vertex {
            // The position stored in the buffer is the un-bent, stacked position.
            position: local_pos + base_position,
            height, // Store the voxel's height level for shader calculations.
        });
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
}
