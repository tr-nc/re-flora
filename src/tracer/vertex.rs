// You can place this in a shared module or at the top of your `tracer` module.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Vertex {
    pub position: glam::Vec3,
    pub color: glam::Vec3,
}
