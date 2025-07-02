use ash::vk;

use crate::vkn::{
    Allocator, Buffer, BufferUsage, Device, Extent2D, ImageDesc, ShaderModule, Texture,
};

pub struct DenoiserResources {
    // 1. cur-prev paired
    pub denoiser_normal_tex: Texture,
    pub denoiser_normal_tex_prev: Texture,
    pub denoiser_position_tex: Texture,
    pub denoiser_position_tex_prev: Texture,
    pub denoiser_vox_id_tex: Texture,
    pub denoiser_vox_id_tex_prev: Texture,
    pub denoiser_accumed_tex: Texture,
    pub denoiser_accumed_tex_prev: Texture,
    // 2. current-only
    pub denoiser_motion_tex: Texture,
    pub denoiser_temporal_hist_len_tex: Texture,
    pub denoiser_hit_tex: Texture,
    // 3. ping-pong
    pub denoiser_atrous_ping_tex: Texture,
    pub denoiser_atrous_pong_tex: Texture,

    pub temporal_info: Buffer,

    device: Device,
    allocator: Allocator,
}

impl DenoiserResources {
    pub fn new(
        device: Device,
        allocator: Allocator,
        rendering_extent: Extent2D,
        temporal_sm: &ShaderModule,
    ) -> Self {
        let tex_arr = Self::create_textures(device.clone(), allocator.clone(), rendering_extent);

        let temporal_info_layout = temporal_sm.get_buffer_layout("U_TemporalInfo").unwrap();
        let temporal_info = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            temporal_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let [denoiser_normal_tex, denoiser_normal_tex_prev, denoiser_position_tex, denoiser_position_tex_prev, denoiser_vox_id_tex, denoiser_vox_id_tex_prev, denoiser_accumed_tex, denoiser_accumed_tex_prev, denoiser_motion_tex, denoiser_temporal_hist_len_tex, denoiser_hit_tex, denoiser_atrous_ping_tex, denoiser_atrous_pong_tex] =
            tex_arr;

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
            temporal_info,
            device,
            allocator,
        }
    }

    pub fn on_resize(&mut self, rendering_extent: Extent2D) {
        let tex_arr = Self::create_textures(
            self.device.clone(),
            self.allocator.clone(),
            rendering_extent,
        );

        [
            self.denoiser_normal_tex,
            self.denoiser_normal_tex_prev,
            self.denoiser_position_tex,
            self.denoiser_position_tex_prev,
            self.denoiser_vox_id_tex,
            self.denoiser_vox_id_tex_prev,
            self.denoiser_accumed_tex,
            self.denoiser_accumed_tex_prev,
            self.denoiser_motion_tex,
            self.denoiser_temporal_hist_len_tex,
            self.denoiser_hit_tex,
            self.denoiser_atrous_ping_tex,
            self.denoiser_atrous_pong_tex,
        ] = tex_arr;
    }

    fn create_textures(
        device: Device,
        allocator: Allocator,
        rendering_extent: Extent2D,
    ) -> [Texture; 13] {
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

        let denoiser_motion_tex =
            create_texture(vk::Format::R16G16_SFLOAT, vk::ImageUsageFlags::STORAGE);
        let denoiser_temporal_hist_len_tex =
            create_texture(vk::Format::R8_UINT, vk::ImageUsageFlags::STORAGE);
        let denoiser_hit_tex = create_texture(vk::Format::R8_UINT, vk::ImageUsageFlags::STORAGE);

        let denoiser_atrous_ping_tex = create_texture(
            vk::Format::B10G11R11_UFLOAT_PACK32,
            vk::ImageUsageFlags::STORAGE,
        );
        let denoiser_atrous_pong_tex = create_texture(
            vk::Format::B10G11R11_UFLOAT_PACK32,
            vk::ImageUsageFlags::STORAGE,
        );

        [
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
        ]
    }
}
