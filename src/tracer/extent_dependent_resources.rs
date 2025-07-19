use crate::{
    resource::Resource,
    vkn::{Allocator, Device, Extent2D, ImageDesc, Texture},
};
use ash::vk;
use resource_container_derive::ResourceContainer;

#[derive(ResourceContainer)]
pub struct ExtentDependentResources {
    pub gfx_depth_tex: Resource<Texture>,
    pub compute_depth_tex: Resource<Texture>,
    pub compute_output_tex: Resource<Texture>,
    pub gfx_output_tex: Resource<Texture>,
    pub god_ray_output_tex: Resource<Texture>,
    pub screen_output_tex: Resource<Texture>,
    pub composited_tex: Resource<Texture>,
    pub taa_tex: Resource<Texture>,
    pub taa_tex_prev: Resource<Texture>,
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
        let screen_output_tex =
            Self::create_screen_output_tex(device.clone(), allocator.clone(), screen_extent);
        let composited_tex =
            Self::create_composited_tex(device.clone(), allocator.clone(), rendering_extent);
        let taa_tex = Self::create_taa_tex(device.clone(), allocator.clone(), rendering_extent);
        let taa_tex_prev = Self::create_taa_tex(device, allocator, rendering_extent);

        Self {
            gfx_depth_tex: Resource::new(gfx_depth_tex),
            compute_depth_tex: Resource::new(compute_depth_tex),
            compute_output_tex: Resource::new(compute_output_tex),
            gfx_output_tex: Resource::new(gfx_output_tex),
            god_ray_output_tex: Resource::new(god_ray_output_tex),
            screen_output_tex: Resource::new(screen_output_tex),
            composited_tex: Resource::new(composited_tex),
            taa_tex: Resource::new(taa_tex),
            taa_tex_prev: Resource::new(taa_tex_prev),
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
            usage: vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::STORAGE,
            initial_layout: vk::ImageLayout::UNDEFINED,
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
            format: vk::Format::D32_SFLOAT,
            usage: vk::ImageUsageFlags::STORAGE,
            initial_layout: vk::ImageLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::DEPTH,
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
            usage: vk::ImageUsageFlags::STORAGE,
            initial_layout: vk::ImageLayout::UNDEFINED,
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
            usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::COLOR_ATTACHMENT,
            initial_layout: vk::ImageLayout::UNDEFINED,
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
            initial_layout: vk::ImageLayout::UNDEFINED,
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
            initial_layout: vk::ImageLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        Texture::new(device, allocator, &tex_desc, &Default::default())
    }

    fn create_taa_tex(device: Device, allocator: Allocator, rendering_extent: Extent2D) -> Texture {
        let tex_desc = ImageDesc {
            extent: rendering_extent.into(),
            format: vk::Format::R16G16B16A16_SFLOAT,
            usage: vk::ImageUsageFlags::STORAGE
                | vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::TRANSFER_DST,
            initial_layout: vk::ImageLayout::UNDEFINED,
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
            initial_layout: vk::ImageLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        Texture::new(device, allocator, &tex_desc, &Default::default())
    }
}
