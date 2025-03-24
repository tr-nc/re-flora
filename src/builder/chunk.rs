use glam::{IVec3, UVec3};

pub struct Chunk {
    pub res: UVec3,
    pub pos: IVec3,
    pub data: Vec<u8>,
}

/// Linear index for a 3D coordinate (x,y,z) in a chunk with the given resolution.
#[inline]
fn index_3d(x: u32, y: u32, z: u32, resolution: UVec3) -> usize {
    (x * resolution.y * resolution.z + y * resolution.z + z) as usize
}
