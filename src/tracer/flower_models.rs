//! Six-vertex analytic stem proxy shared by every flower.
//! ModelPixelFrame supplies geometry/parts, never a second retirement owner.
use crate::{flora::models, resource::Resource};
use re_flora_vkn::{vk, Allocator, Buffer, BufferUsage, Device, MemoryLocation};
use resource_container_derive::ResourceContainer;

#[derive(ResourceContainer)]
pub struct FlowerModelResources {
    pub flower_parts: Resource<Buffer>,
    pub flower_stem_vertices: Resource<Buffer>,
    pub flower_stem_indices: Resource<Buffer>,
}
impl FlowerModelResources {
    pub fn new(device: Device, allocator: Allocator) -> Self {
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
        let stem_indices = (0..6u32).collect::<Vec<_>>();
        log::info!(
            "[FLOWER_MODELS] assets={} source=head_models assembly=native heads=1 calyx=head leaves=0 stem=continuous_tapered wind=live",
            models::flowers().len()
        );
        Self {
            // Descriptor initializer; draws bind the current shared part metadata.
            flower_parts: make(&[0; 64], vk::BufferUsageFlags::STORAGE_BUFFER),
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
