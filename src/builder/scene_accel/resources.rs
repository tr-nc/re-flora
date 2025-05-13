use ash::vk;
use glam::UVec3;

use crate::vkn::{Allocator, Buffer, BufferUsage, Device, ShaderModule, Texture, TextureDesc};

pub struct SceneAccelResources {
    pub scene_tex_update_info: Buffer,

    pub scene_offset_tex: Texture,
}

impl SceneAccelResources {
    pub fn new(
        device: Device,
        allocator: Allocator,
        scene_chunk_dim: UVec3,
        update_scene_tex_sm: &ShaderModule,
    ) -> Self {
        let tex_desc = TextureDesc {
            extent: scene_chunk_dim.to_array(),
            format: vk::Format::R32G32_UINT,
            usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST,
            initial_layout: vk::ImageLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let sam_desc = Default::default();
        let scene_offset_tex =
            Texture::new(device.clone(), allocator.clone(), &tex_desc, &sam_desc);

        let scene_tex_update_info_layout = update_scene_tex_sm
            .get_buffer_layout("U_SceneTexUpdateInfo")
            .unwrap();
        let scene_tex_update_info = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            scene_tex_update_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        return Self {
            scene_offset_tex,
            scene_tex_update_info,
        };
    }
}
