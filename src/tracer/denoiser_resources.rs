use ash::vk;
use resource_container_derive::ResourceContainer;

use crate::resource::Resource;
use crate::vkn::{
    Allocator, Buffer, BufferUsage, Device, Extent2D, ImageDesc, ShaderModule, Texture,
};

pub struct DenoiserTextureSet {
    pub normal: Texture,
    pub normal_prev: Texture,
    pub position: Texture,
    pub position_prev: Texture,
    pub vox_id: Texture,
    pub vox_id_prev: Texture,
    pub accumed: Texture,
    pub accumed_prev: Texture,
    pub motion: Texture,
    pub temporal_hist_len: Texture,
    pub hit: Texture,
    pub spatial_ping: Texture,
    pub spatial_pong: Texture,
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
            normal: create_texture(
                vk::Format::R32_UINT,
                vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_SRC,
            ),
            normal_prev: create_texture(
                vk::Format::R32_UINT,
                vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST,
            ),
            position: create_texture(
                vk::Format::R32G32B32A32_SFLOAT,
                vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_SRC,
            ),
            position_prev: create_texture(
                vk::Format::R32G32B32A32_SFLOAT,
                vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST,
            ),
            vox_id: create_texture(
                vk::Format::R32_UINT,
                vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_SRC,
            ),
            vox_id_prev: create_texture(
                vk::Format::R32_UINT,
                vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST,
            ),
            accumed: create_texture(
                vk::Format::R32_UINT,
                vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_SRC,
            ),
            accumed_prev: create_texture(
                vk::Format::R32_UINT,
                vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST,
            ),
            motion: create_texture(vk::Format::R16G16_SFLOAT, vk::ImageUsageFlags::STORAGE),
            temporal_hist_len: create_texture(vk::Format::R8_UINT, vk::ImageUsageFlags::STORAGE),
            hit: create_texture(vk::Format::R8_UINT, vk::ImageUsageFlags::STORAGE),
            spatial_ping: create_texture(
                vk::Format::B10G11R11_UFLOAT_PACK32,
                vk::ImageUsageFlags::STORAGE,
            ),
            spatial_pong: create_texture(
                vk::Format::B10G11R11_UFLOAT_PACK32,
                vk::ImageUsageFlags::STORAGE,
            ),
        }
    }
}
