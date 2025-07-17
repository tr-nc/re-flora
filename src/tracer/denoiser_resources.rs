use ash::vk;
use resource_container_derive::ResourceContainer;

use crate::resource::Resource;
use crate::vkn::{
    Allocator, Buffer, BufferUsage, Device, Extent2D, ImageDesc, ShaderModule, Texture,
};

#[derive(ResourceContainer)]
pub struct DenoiserTextureSet {
    pub denoiser_normal_tex: Resource<Texture>,
    pub denoiser_normal_tex_prev: Resource<Texture>,
    pub denoiser_position_tex: Resource<Texture>,
    pub denoiser_position_tex_prev: Resource<Texture>,
    pub denoiser_vox_id_tex: Resource<Texture>,
    pub denoiser_vox_id_tex_prev: Resource<Texture>,
    pub denoiser_accumed_tex: Resource<Texture>,
    pub denoiser_accumed_tex_prev: Resource<Texture>,
    pub denoiser_motion_tex: Resource<Texture>,
    pub denoiser_temporal_hist_len_tex: Resource<Texture>,
    pub denoiser_hit_tex: Resource<Texture>,
    pub denoiser_spatial_ping_tex: Resource<Texture>,
    pub denoiser_spatial_pong_tex: Resource<Texture>,
}

#[derive(ResourceContainer)]
pub struct DenoiserResources {
    pub tex: DenoiserTextureSet,
    pub temporal_info: Resource<Buffer>,
    pub spatial_info: Resource<Buffer>,

    device: Device,
    allocator: Allocator,
}

impl DenoiserResources {
    pub fn new(
        device: Device,
        allocator: Allocator,
        rendering_extent: Extent2D,
        temporal_sm: &ShaderModule,
        spatial_sm: &ShaderModule,
    ) -> Self {
        let tex = Self::create_textures(device.clone(), allocator.clone(), rendering_extent);

        let temporal_info_layout = temporal_sm.get_buffer_layout("U_TemporalInfo").unwrap();
        let temporal_info = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            temporal_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let spatial_info_layout = spatial_sm.get_buffer_layout("U_SpatialInfo").unwrap();
        let spatial_info = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            spatial_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        Self {
            device,
            allocator,
            tex,
            temporal_info: Resource::new(temporal_info),
            spatial_info: Resource::new(spatial_info),
        }
    }

    pub fn on_resize(&mut self, rendering_extent: Extent2D) {
        self.tex = Self::create_textures(
            self.device.clone(),
            self.allocator.clone(),
            rendering_extent,
        );
    }

    fn create_textures(
        device: Device,
        allocator: Allocator,
        rendering_extent: Extent2D,
    ) -> DenoiserTextureSet {
        let sam_desc = Default::default();

        let create_texture = |format: vk::Format, usage: vk::ImageUsageFlags| {
            let tex_desc = ImageDesc {
                extent: rendering_extent.into(),
                format,
                usage,
                initial_layout: vk::ImageLayout::UNDEFINED,
                aspect: vk::ImageAspectFlags::COLOR,
                ..Default::default()
            };
            Texture::new(device.clone(), allocator.clone(), &tex_desc, &sam_desc)
        };

        DenoiserTextureSet {
            denoiser_normal_tex: Resource::new(create_texture(
                vk::Format::R32_UINT,
                vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_SRC,
            )),
            denoiser_normal_tex_prev: Resource::new(create_texture(
                vk::Format::R32_UINT,
                vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST,
            )),
            denoiser_position_tex: Resource::new(create_texture(
                vk::Format::R32G32B32A32_SFLOAT,
                vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_SRC,
            )),
            denoiser_position_tex_prev: Resource::new(create_texture(
                vk::Format::R32G32B32A32_SFLOAT,
                vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST,
            )),
            denoiser_vox_id_tex: Resource::new(create_texture(
                vk::Format::R32_UINT,
                vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_SRC,
            )),
            denoiser_vox_id_tex_prev: Resource::new(create_texture(
                vk::Format::R32_UINT,
                vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST,
            )),
            denoiser_accumed_tex: Resource::new(create_texture(
                vk::Format::R32_UINT,
                vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_SRC,
            )),
            denoiser_accumed_tex_prev: Resource::new(create_texture(
                vk::Format::R32_UINT,
                vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST,
            )),
            denoiser_motion_tex: Resource::new(create_texture(
                vk::Format::R16G16_SFLOAT,
                vk::ImageUsageFlags::STORAGE,
            )),
            denoiser_temporal_hist_len_tex: Resource::new(create_texture(
                vk::Format::R8_UINT,
                vk::ImageUsageFlags::STORAGE,
            )),
            denoiser_hit_tex: Resource::new(create_texture(
                vk::Format::R8_UINT,
                vk::ImageUsageFlags::STORAGE,
            )),
            denoiser_spatial_ping_tex: Resource::new(create_texture(
                vk::Format::B10G11R11_UFLOAT_PACK32,
                vk::ImageUsageFlags::STORAGE,
            )),
            denoiser_spatial_pong_tex: Resource::new(create_texture(
                vk::Format::B10G11R11_UFLOAT_PACK32,
                vk::ImageUsageFlags::STORAGE,
            )),
        }
    }
}
