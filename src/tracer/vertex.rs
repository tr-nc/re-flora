/// Compact packed position/corner layout for voxel leaves and particle meshes.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LeafVertex {
    pub packed_data: u32,
}
