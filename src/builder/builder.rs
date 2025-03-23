use glam::U16Vec3;
use glam::UVec3;

use super::BuilderResources;
use crate::vkn::Allocator;
use crate::vkn::VulkanContext;

pub struct Builder {
    vulkan_context: VulkanContext,

    allocator: Allocator,
    resources: BuilderResources,

    chunk_resolution: u32,
    no_of_chunks: UVec3,
}

impl Builder {
    fn validate(resolution: u32) -> Result<(), String> {
        // resolution must be a power of 2
        if resolution & (resolution - 1) != 0 {
            return Err("Resolution must be a power of 2".to_string());
        }
        Ok(())
    }

    pub fn new(
        vulkan_context: VulkanContext,
        allocator: Allocator,
        chunk_resolution: u32,
        no_of_chunks: UVec3,
    ) -> Self {
        Self::validate(chunk_resolution).unwrap();

        let resources = BuilderResources::new(
            vulkan_context.device().clone(),
            allocator.clone(),
            chunk_resolution,
        );

        Self {
            vulkan_context,
            allocator,
            resources,
            chunk_resolution,
            no_of_chunks,
        }
    }
}
