//! Shared apple shape projected into a fixed N×N tile, matching the butterfly
//! and falling-leaf camera grid. The tree and dropped-fruit draw adapters only
//! publish their pose; the pixel sampler owns color, coverage and depth.
use super::LeafVertex;
use crate::resource::Resource;
use re_flora_vkn::{vk, Allocator, Buffer, BufferUsage, Device, MemoryLocation};
use resource_container_derive::ResourceContainer;

pub const MAX_APPLE_RESOLUTION: u32 = 64;

#[derive(ResourceContainer)]
pub struct ApplePixelResources {
    pub apple_pixel_quad_vertices: Resource<Buffer>,
    pub apple_pixel_quad_indices: Resource<Buffer>,
}
impl ApplePixelResources {
    pub fn new(device: Device, allocator: Allocator) -> Self {
        let create = |flags, bytes: u64| {
            Buffer::new_sized(
                device.clone(),
                allocator.clone(),
                BufferUsage::from_flags(flags),
                MemoryLocation::CpuToGpu,
                bytes,
            )
        };
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
            apple_pixel_quad_vertices: Resource::new(vertices),
            apple_pixel_quad_indices: Resource::new(index_buffer),
        }
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn apple_pixel_triangles_match_published_mesh() {
        let source = super::super::apple_preview::mesh();
        assert_eq!(source.indices.len() / 3, 780);
        assert!(
            source.materials.contains(&0)
                && source.materials.contains(&1)
                && source.materials.contains(&2)
        );
    }
}
