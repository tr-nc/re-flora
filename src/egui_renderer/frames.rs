use super::allocator::Allocator;
use super::mesh::Mesh;
use ash::Device;
use egui::ClippedPrimitive;

// Structure holding data for all frames in flight.
pub struct Frames {
    index: usize,
    count: usize,
    meshes: Vec<Mesh>,
}

impl Frames {
    pub fn new(
        device: &Device,
        allocator: &mut Allocator,
        primitives: &[ClippedPrimitive],
        count: usize,
    ) -> Self {
        let meshes = (0..count)
            .map(|_| Mesh::new(device, allocator, primitives))
            .collect();
        Self {
            index: 0,
            count,
            meshes,
        }
    }

    pub fn next(&mut self) -> &mut Mesh {
        let result = &mut self.meshes[self.index];
        self.index = (self.index + 1) % self.count;
        result
    }

    pub fn destroy(self, device: &Device, allocator: &mut Allocator) {
        for mesh in self.meshes.into_iter() {
            mesh.destroy(device, allocator);
        }
    }
}
