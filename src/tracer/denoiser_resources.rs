use ash::vk;

use crate::vkn::{Allocator, Device, Extent2D, ImageDesc, Texture};

pub struct DenoiserResources {
    // 1. cur-prev paird
    pub denoiser_normal_tex: Texture,
    pub denoiser_normal_tex_prev: Texture,
    pub denoiser_position_tex: Texture,
    pub denoiser_position_tex_prev: Texture,
    pub denoiser_vox_id_tex: Texture,
    pub denoiser_vox_id_tex_prev: Texture,
    pub denoiser_accumed_tex: Texture,
    pub denoiser_accumed_tex_prev: Texture,
    // 2. cur only
    pub denoiser_motion_tex: Texture,
    pub denoiser_temporal_hist_len_tex: Texture,
    pub denoiser_hit_tex: Texture,
    // 3. ping-pong
    pub denoiser_atrous_ping_tex: Texture,
    pub denoiser_atrous_pong_tex: Texture,
}

impl DenoiserResources {
    pub fn new(device: Device, allocator: Allocator, rendering_extent: Extent2D) -> Self {
        let sam_desc = Default::default();

        // Helper closure to create a texture
        let create_texture = |format, usage| {
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

        // 1. Create current-previous paired textures for temporal denoising
        let denoiser_normal_tex = create_texture(
            vk::Format::R32_UINT,
            vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_SRC,
        );
        let denoiser_normal_tex_prev = create_texture(
            vk::Format::R32_UINT,
            vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST,
        );

        let denoiser_position_tex = create_texture(
            vk::Format::R32G32B32A32_SFLOAT,
            vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_SRC,
        );
        let denoiser_position_tex_prev = create_texture(
            vk::Format::R32G32B32A32_SFLOAT,
            vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST,
        );

        let denoiser_vox_id_tex = create_texture(
            vk::Format::R32_UINT,
            vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_SRC,
        );
        let denoiser_vox_id_tex_prev = create_texture(
            vk::Format::R32_UINT,
            vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST,
        );

        let denoiser_accumed_tex = create_texture(
            vk::Format::R32_UINT,
            vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_SRC,
        );
        let denoiser_accumed_tex_prev = create_texture(
            vk::Format::R32_UINT,
            vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST,
        );

        // 2. Create textures that only need the current frame's data
        let denoiser_motion_tex =
            create_texture(vk::Format::R16G16_SFLOAT, vk::ImageUsageFlags::STORAGE);
        let denoiser_temporal_hist_len_tex =
            create_texture(vk::Format::R8_UINT, vk::ImageUsageFlags::STORAGE);
        let denoiser_hit_tex = create_texture(vk::Format::R8_UINT, vk::ImageUsageFlags::STORAGE);

        // 3. Create ping-pong textures for the A-Trous filter
        let denoiser_atrous_ping_tex = create_texture(
            vk::Format::B10G11R11_UFLOAT_PACK32,
            vk::ImageUsageFlags::STORAGE,
        );
        let denoiser_atrous_pong_tex = create_texture(
            vk::Format::B10G11R11_UFLOAT_PACK32,
            vk::ImageUsageFlags::STORAGE,
        );

        Self {
            denoiser_normal_tex,
            denoiser_normal_tex_prev,
            denoiser_position_tex,
            denoiser_position_tex_prev,
            denoiser_vox_id_tex,
            denoiser_vox_id_tex_prev,
            denoiser_accumed_tex,
            denoiser_accumed_tex_prev,
            denoiser_motion_tex,
            denoiser_temporal_hist_len_tex,
            denoiser_hit_tex,
            denoiser_atrous_ping_tex,
            denoiser_atrous_pong_tex,
        }
    }
}
