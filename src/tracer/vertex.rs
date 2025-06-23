#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Vertex {
    pub position: glam::Vec3,
    pub color: glam::Vec3,
    pub height: u32,
}
