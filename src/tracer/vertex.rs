#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Vertex {
    pub position: glam::Vec3,
    pub height: u32,
}
