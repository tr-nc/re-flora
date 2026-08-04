#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Vertex {
    pub packed_data: u32,
    pub voxel_index: u32,
}

/// Vertex layout for leaf and fruit meshes.
///
/// These meshes look up their voxel metadata from the instance position, so they do not need
/// the per-vertex voxel index used by surface flora meshes. Keeping that distinction explicit
/// keeps the CPU stride coupled to the leaf shader's single vertex input.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LeafVertex {
    pub packed_data: u32,
}
