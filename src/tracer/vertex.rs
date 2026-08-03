#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Vertex {
    pub packed_data: u32,
    pub voxel_index: u32,
}
