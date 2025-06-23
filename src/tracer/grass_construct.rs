use glam::{Vec2, Vec3};

use crate::tracer::Vertex;

pub fn generate_voxel_grass_blade(
    voxel_count: u32,
    bend_dir_and_strength: Vec2,
    color: Vec3,
) -> (Vec<Vertex>, u32) {
    if voxel_count == 0 {
        return (Vec::new(), 0);
    }

    let mut vertices = Vec::new();
    let denom = (voxel_count - 1).max(1) as f32;

    for i in 0..voxel_count {
        let t = i as f32 / denom;
        let t_curve = t * t; // ease-in curve for a natural bend

        // Calculate the floating-point position of the cube's center
        let float_center = Vec3::new(
            bend_dir_and_strength.x * t_curve, // X bend
            i as f32,                          // Y position
            bend_dir_and_strength.y * t_curve, // Z bend
        );

        // Round the position to the nearest integer grid point to snap it.
        // The +0.5 moves it to the center of the voxel cell.
        // The extra +1.0 on Y matches the GLSL reference.
        let snapped_center = float_center.round() + Vec3::new(0.5, 1.5, 0.5);

        append_cube_vertices(&mut vertices, snapped_center, color);
    }

    let vertex_count = vertices.len() as u32;
    (vertices, vertex_count)
}

/// Appends the 36 vertices of a single, non-indexed cube to the vertex list.
fn append_cube_vertices(vertices: &mut Vec<Vertex>, center: Vec3, color: Vec3) {
    // Base vertices for a 1x1x1 cube centered at the origin
    let base_verts = [
        Vec3::new(-0.5, -0.5, -0.5), // 0
        Vec3::new(0.5, -0.5, -0.5),  // 1
        Vec3::new(0.5, 0.5, -0.5),   // 2
        Vec3::new(-0.5, 0.5, -0.5),  // 3
        Vec3::new(-0.5, -0.5, 0.5),  // 4
        Vec3::new(0.5, -0.5, 0.5),   // 5
        Vec3::new(0.5, 0.5, 0.5),    // 6
        Vec3::new(-0.5, 0.5, 0.5),   // 7
    ];

    // Indices defining the 12 triangles (36 vertices) for a cube
    let indices = [
        // -Z face
        0, 1, 2, 2, 3, 0, // +Z face
        4, 6, 5, 6, 4, 7, // -X face
        0, 3, 7, 7, 4, 0, // +X face
        1, 5, 6, 6, 2, 1, // -Y face
        0, 4, 5, 5, 1, 0, // +Y face
        3, 2, 6, 6, 7, 3,
    ];

    for &index in &indices {
        vertices.push(Vertex {
            position: base_verts[index] + center,
            color,
        });
    }
}
