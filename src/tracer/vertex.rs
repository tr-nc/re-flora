// In your shared Vertex definition file (e.g., src/tracer/mod.rs)
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Vertex {
    pub position: glam::Vec3, // Local position within the cube (-0.5 to 0.5)
    pub color: glam::Vec3,
    pub height: u32, // The stack level of the voxel (0, 1, 2, ...)
}
