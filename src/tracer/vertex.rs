#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Vertex {
    pub packed_data: u32,
    pub voxel_index: u32,
}

/// Vertex layout for leaf, fruit, and particle billboard meshes.
///
/// These meshes do not consume the per-vertex voxel index used by surface flora meshes. Keeping
/// that distinction explicit keeps the CPU stride coupled to the active shader inputs.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LeafVertex {
    pub packed_data: u32,
}
