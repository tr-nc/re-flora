//! Shared apple shape projected into a fixed N×N tile, matching the butterfly
//! and falling-leaf camera grid. The tree and dropped-fruit draw adapters only
//! publish their pose; the pixel sampler owns color, coverage and depth.
use super::{apple_preview, LeafVertex};
use crate::resource::Resource;
use bytemuck::{Pod, Zeroable};
use re_flora_vkn::{vk, Allocator, Buffer, BufferUsage, Device, MemoryLocation};
use resource_container_derive::ResourceContainer;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PixelTriangle {
    a: [f32; 4],
    e1: [f32; 4],
    e2: [f32; 4],
    normal0: [f32; 4],
    normal1: [f32; 4],
    normal2: [f32; 4],
    uv01: [f32; 4],
    uv2: [f32; 4],
}

pub const MAX_APPLE_RESOLUTION: u32 = 64;

#[derive(ResourceContainer)]
pub struct ApplePixelResources {
    pub apple_pixel_triangles: Resource<Buffer>,
    pub apple_pixel_quad_vertices: Resource<Buffer>,
    pub apple_pixel_quad_indices: Resource<Buffer>,
    pub triangle_count: u32,
}
impl ApplePixelResources {
    pub fn new(device: Device, allocator: Allocator) -> Self {
        let source = apple_preview::mesh();
        let mut triangles = Vec::with_capacity(source.indices.len() / 3);
        for tri in source.indices.chunks_exact(3) {
            let point = [tri[0], tri[1], tri[2]].map(|i| source.positions[i as usize] * 0.5);
            let normal = [tri[0], tri[1], tri[2]].map(|i| source.normals[i as usize]);
            let material = source.materials[tri[0] as usize];
            assert!(tri
                .iter()
                .all(|&i| source.materials[i as usize] == material));
            triangles.push(PixelTriangle {
                a: point[0].extend(0.).to_array(),
                e1: (point[1] - point[0]).extend(0.).to_array(),
                e2: (point[2] - point[0]).extend(0.).to_array(),
                normal0: normal[0].extend(0.).to_array(),
                normal1: normal[1].extend(0.).to_array(),
                normal2: normal[2].extend(0.).to_array(),
                uv01: [0.; 4],
                uv2: [0., 0., material as f32, 0.],
            });
        }
        let create = |flags, bytes: u64| {
            Buffer::new_sized(
                device.clone(),
                allocator.clone(),
                BufferUsage::from_flags(flags),
                MemoryLocation::CpuToGpu,
                bytes,
            )
        };
        let triangle_buffer = create(
            vk::BufferUsageFlags::STORAGE_BUFFER,
            std::mem::size_of_val(triangles.as_slice()) as u64,
        );
        triangle_buffer.fill(&triangles).expect("apple triangles");
        let quad = [
            LeafVertex { packed_data: 0 },
            LeafVertex { packed_data: 1 },
            LeafVertex { packed_data: 2 },
            LeafVertex { packed_data: 3 },
        ];
        let vertices = create(
            vk::BufferUsageFlags::VERTEX_BUFFER,
            std::mem::size_of_val(&quad) as u64,
        );
        vertices.fill(&quad).expect("apple quad");
        let indices = [0u32, 1, 2, 2, 1, 3];
        let index_buffer = create(
            vk::BufferUsageFlags::INDEX_BUFFER,
            std::mem::size_of_val(&indices) as u64,
        );
        index_buffer.fill(&indices).expect("apple quad indices");
        Self {
            apple_pixel_triangles: Resource::new(triangle_buffer),
            apple_pixel_quad_vertices: Resource::new(vertices),
            apple_pixel_quad_indices: Resource::new(index_buffer),
            triangle_count: triangles.len() as u32,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn apple_pixel_triangles_match_published_mesh() {
        let source = apple_preview::mesh();
        assert_eq!(source.indices.len() / 3, 780);
        assert!(
            source.materials.contains(&0)
                && source.materials.contains(&1)
                && source.materials.contains(&2)
        );
    }
}
