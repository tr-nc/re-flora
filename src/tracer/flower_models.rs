//! Bounded stem index capacity only; draw counts follow the current cache source.
//! ModelPixelFrame supplies geometry/parts, never a second retirement owner.
use crate::{flora::models, resource::Resource};
use re_flora_vkn::{vk, Allocator, Buffer, BufferUsage, Device, MemoryLocation};
use resource_container_derive::ResourceContainer;

#[derive(ResourceContainer)]
pub struct FlowerModelResources {
    pub flower_triangles: Resource<Buffer>,
    pub flower_parts: Resource<Buffer>,
    pub flower_stem_vertices: Resource<Buffer>,
    pub flower_stem_indices: Resource<Buffer>,
}
impl FlowerModelResources {
    pub fn new(device: Device, allocator: Allocator) -> Self {
        let max_stem_vertices = models::flowers()
            .iter()
            // Twelve triangles per closed moving cube; no stem leaves.
            .map(|f| f.column.scaled_height(models::MAX_SHAPE_SCALE).count() * 12 * 3)
            .max()
            .unwrap();
        let make = |bytes: &[u8], flags| {
            let buffer = Buffer::new_sized(
                device.clone(),
                allocator.clone(),
                BufferUsage::from_flags(flags),
                MemoryLocation::CpuToGpu,
                bytes.len() as u64,
            );
            buffer
                .fill_range_with_raw_u8(0, bytes)
                .expect("flower topology upload");
            Resource::new(buffer)
        };
        let stem_indices = (0..max_stem_vertices).collect::<Vec<_>>();
        log::info!(
            "[FLOWER_MODELS] assets={} source=head_models assembly=native heads=1 calyx=head leaves=0 stem=single_column layer_pose=translation",
            models::flowers().len()
        );
        Self {
            // Descriptor initializers only; every draw binds the same transformed
            // triangle allocation used by the current shared surface bake.
            flower_triangles: make(&[0; 128], vk::BufferUsageFlags::STORAGE_BUFFER),
            flower_parts: make(&[0; 48], vk::BufferUsageFlags::STORAGE_BUFFER),
            flower_stem_vertices: make(
                bytemuck::cast_slice(&stem_indices),
                vk::BufferUsageFlags::VERTEX_BUFFER,
            ),
            flower_stem_indices: make(
                bytemuck::cast_slice(&stem_indices),
                vk::BufferUsageFlags::INDEX_BUFFER,
            ),
        }
    }
}
