use crate::resource::Resource;
use resource_container_derive::ResourceContainer;
use verdarium_vkn::vk;
use verdarium_vkn::{Allocator, Device, Extent2D, ImageDesc, SamplerDesc, Texture, TextureLayout};

#[derive(ResourceContainer)]
pub struct ExtentDependentResources {
    pub gfx_depth_tex: Resource<Texture>,
    pub compute_depth_tex: Resource<Texture>,
    pub compute_output_tex: Resource<Texture>,
    pub gfx_output_tex: Resource<Texture>,
    pub god_ray_output_tex: Resource<Texture>,
    pub lens_flare_required_count_tex: Resource<Texture>,
    pub lens_flare_visible_count_tex: Resource<Texture>,
    pub lens_flare_full_output_tex: Resource<Texture>,
    pub lens_flare_output_tex: Resource<Texture>,
    pub cloud_raw_tex: Resource<Texture>,
    pub cloud_history_tex: Resource<Texture>,
    pub cloud_output_tex: Resource<Texture>,
    pub screen_output_tex: Resource<Texture>,
    pub screenshot_output_tex: Resource<Texture>,
    pub composited_tex: Resource<Texture>,
}

impl ExtentDependentResources {
    pub fn new(
        device: Device,
        allocator: Allocator,
        rendering_extent: Extent2D,
        screen_extent: Extent2D,
    ) -> Self {
        let gfx_depth_tex =
            Self::create_gfx_depth_tex(device.clone(), allocator.clone(), rendering_extent);
        let compute_depth_tex =
            Self::create_compute_depth_tex(device.clone(), allocator.clone(), rendering_extent);
        let compute_output_tex =
            Self::create_compute_output_tex(device.clone(), allocator.clone(), rendering_extent);
        let gfx_output_tex =
            Self::create_gfx_output_tex(device.clone(), allocator.clone(), rendering_extent);
        let god_ray_output_tex =
            Self::create_god_ray_output_tex(device.clone(), allocator.clone(), rendering_extent);
        let lens_flare_required_count_tex =
            Self::create_lens_flare_count_tex(device.clone(), allocator.clone());
        let lens_flare_visible_count_tex =
            Self::create_lens_flare_count_tex(device.clone(), allocator.clone());
        let lens_flare_full_output_tex = Self::create_lens_flare_full_output_tex(
            device.clone(),
            allocator.clone(),
            rendering_extent,
        );
        let lens_flare_output_tex =
            Self::create_lens_flare_output_tex(device.clone(), allocator.clone(), rendering_extent);
        let cloud_raw_tex =
            Self::create_cloud_tex(device.clone(), allocator.clone(), rendering_extent);
        let cloud_history_tex =
            Self::create_cloud_tex(device.clone(), allocator.clone(), rendering_extent);
        let cloud_output_tex =
            Self::create_cloud_tex(device.clone(), allocator.clone(), rendering_extent);
        let screen_output_tex =
            Self::create_screen_output_tex(device.clone(), allocator.clone(), screen_extent);
        let screenshot_output_tex =
            Self::create_screenshot_output_tex(device.clone(), allocator.clone(), rendering_extent);
        let composited_tex = Self::create_composited_tex(device, allocator, rendering_extent);

        Self {
            gfx_depth_tex: Resource::new(gfx_depth_tex),
            compute_depth_tex: Resource::new(compute_depth_tex),
            compute_output_tex: Resource::new(compute_output_tex),
            gfx_output_tex: Resource::new(gfx_output_tex),
            god_ray_output_tex: Resource::new(god_ray_output_tex),
            lens_flare_required_count_tex: Resource::new(lens_flare_required_count_tex),
            lens_flare_visible_count_tex: Resource::new(lens_flare_visible_count_tex),
            lens_flare_full_output_tex: Resource::new(lens_flare_full_output_tex),
            lens_flare_output_tex: Resource::new(lens_flare_output_tex),
            cloud_raw_tex: Resource::new(cloud_raw_tex),
            cloud_history_tex: Resource::new(cloud_history_tex),
            cloud_output_tex: Resource::new(cloud_output_tex),
            screen_output_tex: Resource::new(screen_output_tex),
            screenshot_output_tex: Resource::new(screenshot_output_tex),
            composited_tex: Resource::new(composited_tex),
        }
    }

    pub fn on_resize(
        &mut self,
        device: Device,
        allocator: Allocator,
        rendering_extent: Extent2D,
        screen_extent: Extent2D,
    ) {
        *self = Self::new(device, allocator, rendering_extent, screen_extent);
    }

    fn create_gfx_depth_tex(
        device: Device,
        allocator: Allocator,
        rendering_extent: Extent2D,
    ) -> Texture {
        let tex_desc = ImageDesc {
            extent: rendering_extent.into(),
            format: vk::Format::D32_SFLOAT,
            usage: vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT
                | vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::TRANSFER_DST,
            initial_layout: TextureLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::DEPTH,
            ..Default::default()
        };
        Texture::new(device, allocator, &tex_desc, &Default::default())
    }

    fn create_compute_depth_tex(
        device: Device,
        allocator: Allocator,
        rendering_extent: Extent2D,
    ) -> Texture {
        let tex_desc = ImageDesc {
            extent: rendering_extent.into(),
            format: vk::Format::R32_SFLOAT,
            usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED,
            initial_layout: TextureLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        Texture::new(device, allocator, &tex_desc, &Default::default())
    }

    fn create_compute_output_tex(
        device: Device,
        allocator: Allocator,
        rendering_extent: Extent2D,
    ) -> Texture {
        let tex_desc = ImageDesc {
            extent: rendering_extent.into(),
            format: vk::Format::R32_UINT,
            usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED,
            initial_layout: TextureLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        Texture::new(device, allocator, &tex_desc, &Default::default())
    }

    fn create_gfx_output_tex(
        device: Device,
        allocator: Allocator,
        rendering_extent: Extent2D,
    ) -> Texture {
        let tex_desc = ImageDesc {
            extent: rendering_extent.into(),
            format: vk::Format::R8G8B8A8_UNORM,
            usage: vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::COLOR_ATTACHMENT
                | vk::ImageUsageFlags::TRANSFER_DST,
            initial_layout: TextureLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        Texture::new(device, allocator, &tex_desc, &Default::default())
    }

    fn create_god_ray_output_tex(
        device: Device,
        allocator: Allocator,
        rendering_extent: Extent2D,
    ) -> Texture {
        let tex_desc = ImageDesc {
            extent: rendering_extent.into(),
            format: vk::Format::R32_SFLOAT,
            usage: vk::ImageUsageFlags::STORAGE
                | vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::TRANSFER_DST,
            initial_layout: TextureLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        Texture::new(device, allocator, &tex_desc, &Default::default())
    }

    fn create_screen_output_tex(
        device: Device,
        allocator: Allocator,
        screen_extent: Extent2D,
    ) -> Texture {
        let tex_desc = ImageDesc {
            extent: screen_extent.into(),
            format: vk::Format::R8G8B8A8_UNORM,
            usage: vk::ImageUsageFlags::STORAGE
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::COLOR_ATTACHMENT,
            initial_layout: TextureLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        Texture::new(device, allocator, &tex_desc, &Default::default())
    }

    fn create_screenshot_output_tex(
        device: Device,
        allocator: Allocator,
        rendering_extent: Extent2D,
    ) -> Texture {
        let tex_desc = ImageDesc {
            extent: rendering_extent.into(),
            format: vk::Format::R8G8B8A8_SRGB,
            usage: vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::COLOR_ATTACHMENT
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::TRANSFER_DST,
            initial_layout: TextureLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        Texture::new(device, allocator, &tex_desc, &Default::default())
    }

    fn create_lens_flare_output_tex(
        device: Device,
        allocator: Allocator,
        rendering_extent: Extent2D,
    ) -> Texture {
        let lens_flare_extent = Extent2D::new(
            (rendering_extent.width / 4).max(1),
            (rendering_extent.height / 4).max(1),
        );
        let tex_desc = ImageDesc {
            extent: lens_flare_extent.into(),
            format: vk::Format::B10G11R11_UFLOAT_PACK32,
            usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST,
            initial_layout: TextureLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        Texture::new(device, allocator, &tex_desc, &Default::default())
    }

    fn create_lens_flare_full_output_tex(
        device: Device,
        allocator: Allocator,
        rendering_extent: Extent2D,
    ) -> Texture {
        let tex_desc = ImageDesc {
            extent: rendering_extent.into(),
            format: vk::Format::B10G11R11_UFLOAT_PACK32,
            usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST,
            initial_layout: TextureLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        Texture::new(device, allocator, &tex_desc, &Default::default())
    }

    fn create_cloud_tex(
        device: Device,
        allocator: Allocator,
        rendering_extent: Extent2D,
    ) -> Texture {
        // The whole tracer already renders at `TracerDesc::scaling_factor` (currently 0.5x
        // screen resolution), so matching the main internal render extent keeps clouds cheap
        // while avoiding a second half-resolution blur before the final upscaler.
        let cloud_extent = rendering_extent;
        let tex_desc = ImageDesc {
            extent: cloud_extent.into(),
            format: vk::Format::R16G16B16A16_SFLOAT,
            usage: vk::ImageUsageFlags::STORAGE
                | vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::TRANSFER_DST,
            initial_layout: TextureLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let sam_desc = SamplerDesc {
            mag_filter: vk::Filter::LINEAR,
            min_filter: vk::Filter::LINEAR,
            ..Default::default()
        };
        Texture::new(device, allocator, &tex_desc, &sam_desc)
    }

    fn create_lens_flare_count_tex(device: Device, allocator: Allocator) -> Texture {
        let tex_desc = ImageDesc {
            extent: Extent2D::new(1, 1).into(),
            format: vk::Format::R32_UINT,
            usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST,
            initial_layout: TextureLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        Texture::new(device, allocator, &tex_desc, &Default::default())
    }

    fn create_composited_tex(
        device: Device,
        allocator: Allocator,
        rendering_extent: Extent2D,
    ) -> Texture {
        let tex_desc = ImageDesc {
            extent: rendering_extent.into(),
            format: vk::Format::B10G11R11_UFLOAT_PACK32,
            usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED,
            initial_layout: TextureLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        Texture::new(device, allocator, &tex_desc, &Default::default())
    }
}
