use crate::tracer::Vertex;
use glam::{vec3, Vec3};

pub fn generate_indexed_voxel_grass_blade(
    voxel_count: u32,
    bottom_color: Vec3,
    tip_color: Vec3,
) -> (Vec<Vertex>, Vec<u32>) {
    if voxel_count == 0 {
        return (Vec::new(), Vec::new());
    }

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for i in 0..voxel_count {
        // The current vertex offset is the number of vertices we've already added.
        let vertex_offset = vertices.len() as u32;

        // The position passed to the helper is now just the vertical stack position.
        // The bending will be done in the vertex shader.
        let base_position = vec3(0.0, i as f32, 0.0);

        // Interpolate the color based on the height (i).
        // 't' will go from 0.0 for the first voxel to 1.0 for the last one.
        let t = i as f32 / (voxel_count - 1).max(1) as f32;
        let voxel_color = bottom_color.lerp(tip_color, t);

        append_indexed_cube_data(
            &mut vertices,
            &mut indices,
            base_position,
            voxel_color, // Pass the interpolated color for this specific voxel.
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
    color: Vec3,
    height: u32,
    vertex_offset: u32,
) {
    // 8 unique vertices for a cube, relative to its center.
    let base_verts = [
        vec3(-0.5, -0.5, -0.5),
        vec3(0.5, -0.5, -0.5),
        vec3(0.5, 0.5, -0.5),
        vec3(-0.5, 0.5, -0.5),
        vec3(-0.5, -0.5, 0.5),
        vec3(0.5, -0.5, 0.5),
        vec3(0.5, 0.5, 0.5),
        vec3(-0.5, 0.5, 0.7),
    ];

    for &local_pos in &base_verts {
        vertices.push(Vertex {
            // The position stored in the buffer is the un-bent, stacked position.
            position: local_pos + base_position,
            color,
            height, // Store the voxel's height level for shader calculations.
        });
    }

    // 36 indices that reference the 8 vertices just added.
    // Must be offset by the number of vertices already in the buffer.
    let base_indices: [u32; 36] = [
        0, 1, 2, 2, 3, 0, // -Z
        4, 6, 5, 6, 4, 7, // +Z
        0, 3, 7, 7, 4, 0, // -X
        1, 5, 6, 6, 2, 1, // +X
        0, 4, 5, 5, 1, 0, // -Y
        3, 2, 6, 6, 7, 3, // +Y
    ];

    for &index in &base_indices {
        indices.push(vertex_offset + index);
    }
}
