use glam::UVec3;

/// The 8 vertices of a unit cube, starting with the bottom face.
/// These define the corners of a voxel in local coordinates.
pub const VOXEL_VERTICES: [UVec3; 8] = [
    UVec3::new(0, 0, 0), // Bottom face
    UVec3::new(1, 0, 0),
    UVec3::new(1, 0, 1),
    UVec3::new(0, 0, 1),
    UVec3::new(0, 1, 0), // Top face
    UVec3::new(1, 1, 0),
    UVec3::new(1, 1, 1),
    UVec3::new(0, 1, 1),
];

/// Indices for 12 triangles (6 faces) of a cube.
/// The winding order is Counter-Clockwise (CCW) when viewed from the outside.
pub const CUBE_INDICES: [u32; 36] = [
    0, 1, 2, 0, 2, 3, // Bottom face (-Y)
    4, 6, 5, 4, 7, 6, // Top face (+Y)
    1, 0, 4, 1, 4, 5, // Back face (-Z)
    2, 7, 3, 2, 6, 7, // Front face (+Z)
    0, 3, 7, 0, 7, 4, // Left face (-X)
    1, 6, 2, 1, 5, 6, // Right face (+X)
];

pub const VOXEL_VERTICES_LOD: [UVec3; 4] = [
    UVec3::new(0, 0, 0),
    UVec3::new(1, 0, 0),
    UVec3::new(1, 1, 0),
    UVec3::new(0, 1, 0),
];

pub const CUBE_INDICES_LOD: [u32; 6] = [0, 1, 3, 1, 2, 3];
