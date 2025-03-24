use ash::vk;
use glam::UVec3;

use crate::vkn::{Allocator, Buffer, Device, ShaderModule};

pub struct BuilderResources {
    pub chunk_build_info_buf: Buffer,

    pub weight_data_buf: Buffer,
}

impl BuilderResources {
    pub fn new(
        device: Device,
        allocator: Allocator,
        chunk_init_sm: &ShaderModule,
        chunk_res: UVec3,
    ) -> Self {
        let chunk_build_info_buf_layout =
            chunk_init_sm.get_buffer_layout("ChunkBuildInfo").unwrap();
        let chunk_build_info_buf = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            gpu_allocator::MemoryLocation::CpuToGpu,
            chunk_build_info_buf_layout.get_size() as _,
        );

        let weight_data_buf = Buffer::new_sized(
            device.clone(),
            allocator,
            vk::BufferUsageFlags::STORAGE_BUFFER,
            gpu_allocator::MemoryLocation::GpuOnly,
            (chunk_res.x * chunk_res.y * chunk_res.z) as _,
        );

        Self {
            chunk_build_info_buf,
            weight_data_buf,
        }
    }
}
