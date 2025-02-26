use crate::vkn::{Allocator, Buffer, Device};
use ash::vk;
use egui::epaint::{Primitive, Vertex};
use egui::ClippedPrimitive;
use std::mem::size_of;

/// Vertex and index buffer resources for one frame in flight.
pub struct Mesh {
    pub vertices_buffer: Buffer,
    pub indices_buffer: Buffer,
    vertex_count: usize,
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

        let mut vertices_buffer = Buffer::new_sized(
            device,
            allocator,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            vertex_count * size_of::<Vertex>(),
        );
        vertices_buffer.fill(&vertices);

        let mut indices_buffer = Buffer::new_sized(
            device,
            allocator,
            vk::BufferUsageFlags::INDEX_BUFFER,
            index_count * size_of::<u32>(),
        );
        indices_buffer.fill(&indices);

        Mesh {
            vertices_buffer,
            vertex_count,
            indices_buffer,
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
            self.vertex_count = vertices.len();
            let size = self.vertex_count * size_of::<Vertex>();
            self.vertices_buffer =
                Buffer::new_sized(device, allocator, vk::BufferUsageFlags::VERTEX_BUFFER, size);
        }
        self.vertices_buffer.fill(&vertices);

        let indices = create_indices(primitives);
        if indices.len() > self.index_count {
            log::trace!("Resizing index buffers");

            self.index_count = indices.len();
            let size = self.index_count * size_of::<u32>();
            self.indices_buffer =
                Buffer::new_sized(device, allocator, vk::BufferUsageFlags::INDEX_BUFFER, size);
        }
        self.indices_buffer.fill(&indices);
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
