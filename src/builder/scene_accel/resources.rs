use crate::vkn::{Allocator, Buffer, BufferUsage, Device, ImageDesc, ShaderModule, Texture};
use crate::{geom::UAabb3, resource::Resource};
use ash::vk;
use resource_container_derive::ResourceContainer;

#[derive(ResourceContainer)]
pub struct SceneAccelResources {
    pub scene_tex_update_info: Resource<Buffer>,
    pub scene_offset_tex: Resource<Texture>,
}

impl SceneAccelResources {
    pub fn new(
        device: Device,
        allocator: Allocator,
        chunk_bound: UAabb3,
        update_scene_tex_sm: &ShaderModule,
    ) -> Self {
        let tex_desc = ImageDesc {
            extent: chunk_bound.get_extent(),
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
            scene_offset_tex: Resource::new(scene_offset_tex),
            scene_tex_update_info: Resource::new(scene_tex_update_info),
        };
    }
}
