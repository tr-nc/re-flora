use super::allocator::Allocator;
use super::buffer::create_and_fill_buffer;
use ash::{vk, Device};
use egui::epaint::{Primitive, Vertex};
use egui::ClippedPrimitive;
use std::mem::size_of;

/// Vertex and index buffer resources for one frame in flight.
pub struct Mesh {
    pub vertices_buffer: vk::Buffer,
    pub indices_buffer: vk::Buffer,
    vertices_mem: gpu_allocator::vulkan::Allocation,
    vertex_count: usize,
    indices_mem: gpu_allocator::vulkan::Allocation,
    index_count: usize,
}

impl Mesh {
    pub fn new(
        device: &Device,
        allocator: &mut Allocator,
        primitives: &[ClippedPrimitive],
    ) -> Self {
        let vertices = create_vertices(primitives);
        let vertex_count = vertices.len();
        let indices = create_indices(primitives);
        let index_count = indices.len();

        let (vertices_buffer, vertices_mem) = create_and_fill_buffer(
            device,
            allocator,
            &vertices,
            vk::BufferUsageFlags::VERTEX_BUFFER,
        );

        let (indices_buffer, indices_mem) = create_and_fill_buffer(
            device,
            allocator,
            &indices,
            vk::BufferUsageFlags::INDEX_BUFFER,
        );

        Mesh {
            vertices_buffer,
            vertices_mem,
            vertex_count,
            indices_buffer,
            indices_mem,
            index_count,
        }
    }

    pub fn update(
        &mut self,
        device: &Device,
        allocator: &mut Allocator,
        primitives: &[ClippedPrimitive],
    ) {
        let vertices = create_vertices(primitives);
        if vertices.len() > self.vertex_count {
            log::trace!("Resizing vertex buffers");

            let vertex_count = vertices.len();
            let size = vertex_count * size_of::<Vertex>();
            let (vertices, vertices_mem) =
                allocator.create_buffer(device, size, vk::BufferUsageFlags::VERTEX_BUFFER);

            self.vertex_count = vertex_count;

            let old_vertices = self.vertices_buffer;
            self.vertices_buffer = vertices;

            let old_vertices_mem = std::mem::replace(&mut self.vertices_mem, vertices_mem);

            allocator.destroy_buffer(device, old_vertices, old_vertices_mem);
        }
        allocator.update_buffer(device, &mut self.vertices_mem, &vertices);

        let indices = create_indices(primitives);
        if indices.len() > self.index_count {
            log::trace!("Resizing index buffers");

            let index_count = indices.len();
            let size = index_count * size_of::<u32>();
            let (indices, indices_mem) =
                allocator.create_buffer(device, size, vk::BufferUsageFlags::INDEX_BUFFER);

            self.index_count = index_count;

            let old_indices = self.indices_buffer;
            self.indices_buffer = indices;

            let old_indices_mem = std::mem::replace(&mut self.indices_mem, indices_mem);

            allocator.destroy_buffer(device, old_indices, old_indices_mem);
        }
        allocator.update_buffer(device, &mut self.indices_mem, &indices);
    }

    pub fn destroy(self, device: &Device, allocator: &mut Allocator) {
        allocator.destroy_buffer(device, self.vertices_buffer, self.vertices_mem);
        allocator.destroy_buffer(device, self.indices_buffer, self.indices_mem);
    }
}

fn create_vertices(primitives: &[ClippedPrimitive]) -> Vec<Vertex> {
    let vertex_count = primitives
        .iter()
        .map(|p| match &p.primitive {
            Primitive::Mesh(m) => m.vertices.len(),
            _ => 0,
        })
        .sum();

    let mut vertices = Vec::with_capacity(vertex_count);
    for p in primitives {
        if let Primitive::Mesh(m) = &p.primitive {
            vertices.extend_from_slice(&m.vertices);
        }
    }
    vertices
}

fn create_indices(primitives: &[ClippedPrimitive]) -> Vec<u32> {
    let index_count = primitives
        .iter()
        .map(|p| match &p.primitive {
            Primitive::Mesh(m) => m.indices.len(),
            _ => 0,
        })
        .sum();

    let mut indices = Vec::with_capacity(index_count);
    for p in primitives {
        if let Primitive::Mesh(m) = &p.primitive {
            indices.extend_from_slice(&m.indices);
        }
    }

    indices
}
