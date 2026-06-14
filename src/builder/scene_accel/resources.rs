use crate::{geom::UAabb3, resource::Resource};
use resource_container_derive::ResourceContainer;
use verdarium_vkn::vk;
use verdarium_vkn::{Allocator, Buffer, Device, ImageDesc, ShaderModule, Texture, TextureLayout};

#[derive(ResourceContainer)]
pub struct SceneAccelBuilderResources {
    pub scene_tex_update_info: Resource<Buffer>,
    pub scene_tex: Resource<Texture>,
}

impl SceneAccelBuilderResources {
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
            initial_layout: TextureLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            image_type_override: Some(vk::ImageType::TYPE_3D),
            ..Default::default()
        };
        let sam_desc = Default::default();
        let scene_tex = Texture::new(device.clone(), allocator.clone(), &tex_desc, &sam_desc);

        let scene_tex_update_info_layout = update_scene_tex_sm
            .get_buffer_layout("U_SceneTexUpdateInfo")
            .unwrap();
        let scene_tex_update_info = Buffer::from_uniform_layout(
            device.clone(),
            allocator.clone(),
            scene_tex_update_info_layout.clone(),
        );

        Self {
            scene_tex: Resource::new(scene_tex),
            scene_tex_update_info: Resource::new(scene_tex_update_info),
        }
    }
}
