use glam::{IVec3, UVec3};

pub struct Chunk {
    pub res: UVec3,
    pub pos: IVec3,
    pub data: Vec<u8>,
}
