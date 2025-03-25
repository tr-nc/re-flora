use ash::vk;
use glam::UVec3;

use crate::vkn::{Allocator, Buffer, Device, ShaderModule, Texture, TextureDesc};

pub struct BuilderResources {
    pub weight_tex: Texture,
    pub chunk_build_info_buf: Buffer,
}

impl BuilderResources {
    pub fn new(
        device: Device,
        allocator: Allocator,
        chunk_init_sm: &ShaderModule,
        chunk_res: UVec3,
    ) -> Self {
        let weight_tex = Self::create_weight_tex(device.clone(), allocator.clone(), chunk_res);

        let chunk_build_info_buf_layout =
            chunk_init_sm.get_buffer_layout("ChunkBuildInfo").unwrap();
        let chunk_build_info_buf = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            gpu_allocator::MemoryLocation::CpuToGpu,
            chunk_build_info_buf_layout.get_size() as _,
        );

        Self {
            weight_tex,
            chunk_build_info_buf,
        }
    }

    fn create_weight_tex(device: Device, allocator: Allocator, chunk_res: UVec3) -> Texture {
        let tex_desc = TextureDesc {
            extent: chunk_res.to_array(),
            format: vk::Format::R8_UINT,
            usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_SRC,
            initial_layout: vk::ImageLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let sam_desc = Default::default();
        let tex = Texture::new(device, allocator, &tex_desc, &sam_desc);
        tex
    }
}
