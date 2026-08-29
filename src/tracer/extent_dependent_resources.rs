use crate::ddgi::DDGI_SPATIAL_WEIGHT_READBACK_BYTE_COUNT;
use crate::resource::Resource;
use re_flora_vkn::vk;
use re_flora_vkn::{
    Allocator, Buffer, BufferUsage, Device, Extent2D, ImageDesc, MemoryLocation, SamplerDesc,
    Texture, TextureLayout,
};
use resource_container_derive::ResourceContainer;

const LENS_FLARE_DOWNSAMPLE_FACTOR: u32 = 2;
const GLASS_RESOURCE_BYTES_PER_PIXEL: u64 = 32;
pub(crate) const GLASS_VOXEL_CACHE_CAPACITY: u32 = 1 << 18;
pub(crate) const GLASS_VOXEL_CACHE_METADATA_BYTES_PER_ENTRY: u64 = 16;
const GLASS_VOXEL_CACHE_RADIANCE_BYTES_PER_ENTRY: u64 = 32;
const GLASS_VOXEL_CACHE_ACTIVE_SLOT_BYTES_PER_ENTRY: u64 = 4;
pub(crate) const GLASS_VOXEL_CACHE_ACTIVE_COUNT_BYTES: u64 = 4;

fn lens_flare_extent(rendering_extent: Extent2D) -> Extent2D {
    Extent2D::new(
        (rendering_extent.width / LENS_FLARE_DOWNSAMPLE_FACTOR).max(1),
        (rendering_extent.height / LENS_FLARE_DOWNSAMPLE_FACTOR).max(1),
    )
}

fn glass_resource_extent(rendering_extent: Extent2D, enabled: bool) -> Extent2D {
    if enabled {
        rendering_extent
    } else {
        // The texture wrapper infers a 1D view when height is one. Keep the smallest true 2D
        // extent so reflected RWTexture2D descriptors remain Vulkan-compatible.
        Extent2D::new(2, 2)
    }
}

fn glass_voxel_cache_capacity(enabled: bool) -> u32 {
    if enabled {
        GLASS_VOXEL_CACHE_CAPACITY
    } else {
        1
    }
}

#[derive(ResourceContainer)]
pub struct ExtentDependentResources {
    pub gfx_depth_tex: Resource<Texture>,
    pub compute_depth_tex: Resource<Texture>,
    pub compute_output_tex: Resource<Texture>,
    pub glass_front_depth_tex: Resource<Texture>,
    pub glass_front_data_tex: Resource<Texture>,
    pub environment_irradiance_capture: Resource<Buffer>,
    pub ddgi_spatial_weight_readback: Resource<Buffer>,
    pub gfx_output_tex: Resource<Texture>,
    pub god_ray_raw_tex: Resource<Texture>,
    pub god_ray_history_tex: Resource<Texture>,
    pub god_ray_output_tex: Resource<Texture>,
    pub lens_flare_required_count_tex: Resource<Texture>,
    pub lens_flare_visible_count_tex: Resource<Texture>,
    pub lens_flare_raw_tex: Resource<Texture>,
    pub lens_flare_history_tex: Resource<Texture>,
    pub lens_flare_output_tex: Resource<Texture>,
    pub cloud_raw_tex: Resource<Texture>,
    pub cloud_history_tex: Resource<Texture>,
    pub cloud_output_tex: Resource<Texture>,
    pub screen_output_tex: Resource<Texture>,
    pub screenshot_output_tex: Resource<Texture>,
    pub unified_opaque_hdr_tex: Resource<Texture>,
    pub unified_opaque_depth_tex: Resource<Texture>,
    pub opaque_provenance_tex: Resource<Texture>,
    pub glass_debug_tex: Resource<Texture>,
    pub glass_voxel_cache_metadata: Resource<Buffer>,
    pub glass_voxel_cache_radiance: Resource<Buffer>,
    pub glass_voxel_cache_active_slots: Resource<Buffer>,
    pub glass_voxel_cache_active_count: Resource<Buffer>,
    pub composited_tex: Resource<Texture>,
}

impl ExtentDependentResources {
    pub fn new(
        device: Device,
        allocator: Allocator,
        rendering_extent: Extent2D,
        screen_extent: Extent2D,
        environment_irradiance_capture_enabled: bool,
        glass_experiment_enabled: bool,
    ) -> Self {
        let glass_extent = glass_resource_extent(rendering_extent, glass_experiment_enabled);
        let gfx_depth_tex =
            Self::create_gfx_depth_tex(device.clone(), allocator.clone(), rendering_extent);
        let compute_depth_tex =
            Self::create_compute_depth_tex(device.clone(), allocator.clone(), rendering_extent);
        let compute_output_tex =
            Self::create_compute_output_tex(device.clone(), allocator.clone(), rendering_extent);
        let glass_front_depth_tex =
            Self::create_r32_float_tex(device.clone(), allocator.clone(), glass_extent, false);
        let glass_front_data_tex =
            Self::create_r32_uint_tex(device.clone(), allocator.clone(), glass_extent, false);
        let environment_irradiance_capture = Self::create_environment_irradiance_capture(
            device.clone(),
            allocator.clone(),
            rendering_extent,
            environment_irradiance_capture_enabled,
        );
        let ddgi_spatial_weight_readback =
            Self::create_ddgi_spatial_weight_readback(device.clone(), allocator.clone());
        let gfx_output_tex =
            Self::create_gfx_output_tex(device.clone(), allocator.clone(), rendering_extent);
        let god_ray_raw_tex =
            Self::create_god_ray_tex(device.clone(), allocator.clone(), rendering_extent);
        let god_ray_history_tex =
            Self::create_god_ray_tex(device.clone(), allocator.clone(), rendering_extent);
        let god_ray_output_tex =
            Self::create_god_ray_tex(device.clone(), allocator.clone(), rendering_extent);
        let lens_flare_required_count_tex =
            Self::create_lens_flare_count_tex(device.clone(), allocator.clone());
        let lens_flare_visible_count_tex =
            Self::create_lens_flare_count_tex(device.clone(), allocator.clone());
        let lens_flare_raw_tex =
            Self::create_lens_flare_tex(device.clone(), allocator.clone(), rendering_extent);
        let lens_flare_history_tex =
            Self::create_lens_flare_tex(device.clone(), allocator.clone(), rendering_extent);
        let lens_flare_output_tex =
            Self::create_lens_flare_tex(device.clone(), allocator.clone(), rendering_extent);
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
        let unified_opaque_hdr_tex =
            Self::create_hdr_tex(device.clone(), allocator.clone(), glass_extent);
        let unified_opaque_depth_tex =
            Self::create_r32_float_tex(device.clone(), allocator.clone(), glass_extent, false);
        let opaque_provenance_tex =
            Self::create_r32_uint_tex(device.clone(), allocator.clone(), glass_extent, false);
        let glass_debug_tex =
            Self::create_rg32_uint_tex(device.clone(), allocator.clone(), glass_extent, true);
        let glass_voxel_cache_capacity = glass_voxel_cache_capacity(glass_experiment_enabled);
        let glass_voxel_cache_metadata = Self::create_glass_voxel_cache_buffer(
            device.clone(),
            allocator.clone(),
            glass_voxel_cache_capacity,
            GLASS_VOXEL_CACHE_METADATA_BYTES_PER_ENTRY,
            true,
        );
        let glass_voxel_cache_radiance = Self::create_glass_voxel_cache_buffer(
            device.clone(),
            allocator.clone(),
            glass_voxel_cache_capacity,
            GLASS_VOXEL_CACHE_RADIANCE_BYTES_PER_ENTRY,
            false,
        );
        let glass_voxel_cache_active_slots = Self::create_glass_voxel_cache_buffer(
            device.clone(),
            allocator.clone(),
            glass_voxel_cache_capacity,
            GLASS_VOXEL_CACHE_ACTIVE_SLOT_BYTES_PER_ENTRY,
            false,
        );
        let glass_voxel_cache_active_count = Self::create_glass_voxel_cache_buffer(
            device.clone(),
            allocator.clone(),
            1,
            GLASS_VOXEL_CACHE_ACTIVE_COUNT_BYTES,
            true,
        );
        let composited_tex = Self::create_hdr_tex(device, allocator, rendering_extent);

        let glass_image_bytes = u64::from(glass_extent.width)
            * u64::from(glass_extent.height)
            * GLASS_RESOURCE_BYTES_PER_PIXEL;
        let glass_cache_bytes = u64::from(glass_voxel_cache_capacity)
            * (GLASS_VOXEL_CACHE_METADATA_BYTES_PER_ENTRY
                + GLASS_VOXEL_CACHE_RADIANCE_BYTES_PER_ENTRY
                + GLASS_VOXEL_CACHE_ACTIVE_SLOT_BYTES_PER_ENTRY)
            + GLASS_VOXEL_CACHE_ACTIVE_COUNT_BYTES;
        let glass_resource_bytes = glass_image_bytes + glass_cache_bytes;
        log::info!(
            "[GLASS][RESOURCES] enabled={} extent={}x{} bytes={} mib={:.2} image_bytes={} cache_entries={} cache_bytes={} feature_off_placeholder={}",
            glass_experiment_enabled,
            glass_extent.width,
            glass_extent.height,
            glass_resource_bytes,
            glass_resource_bytes as f64 / (1024.0 * 1024.0),
            glass_image_bytes,
            glass_voxel_cache_capacity,
            glass_cache_bytes,
            !glass_experiment_enabled,
        );

        Self {
            gfx_depth_tex: Resource::new(gfx_depth_tex),
            compute_depth_tex: Resource::new(compute_depth_tex),
            compute_output_tex: Resource::new(compute_output_tex),
            glass_front_depth_tex: Resource::new(glass_front_depth_tex),
            glass_front_data_tex: Resource::new(glass_front_data_tex),
            environment_irradiance_capture: Resource::new(environment_irradiance_capture),
            ddgi_spatial_weight_readback: Resource::new(ddgi_spatial_weight_readback),
            gfx_output_tex: Resource::new(gfx_output_tex),
            god_ray_raw_tex: Resource::new(god_ray_raw_tex),
            god_ray_history_tex: Resource::new(god_ray_history_tex),
            god_ray_output_tex: Resource::new(god_ray_output_tex),
            lens_flare_required_count_tex: Resource::new(lens_flare_required_count_tex),
            lens_flare_visible_count_tex: Resource::new(lens_flare_visible_count_tex),
            lens_flare_raw_tex: Resource::new(lens_flare_raw_tex),
            lens_flare_history_tex: Resource::new(lens_flare_history_tex),
            lens_flare_output_tex: Resource::new(lens_flare_output_tex),
            cloud_raw_tex: Resource::new(cloud_raw_tex),
            cloud_history_tex: Resource::new(cloud_history_tex),
            cloud_output_tex: Resource::new(cloud_output_tex),
            screen_output_tex: Resource::new(screen_output_tex),
            screenshot_output_tex: Resource::new(screenshot_output_tex),
            unified_opaque_hdr_tex: Resource::new(unified_opaque_hdr_tex),
            unified_opaque_depth_tex: Resource::new(unified_opaque_depth_tex),
            opaque_provenance_tex: Resource::new(opaque_provenance_tex),
            glass_debug_tex: Resource::new(glass_debug_tex),
            glass_voxel_cache_metadata: Resource::new(glass_voxel_cache_metadata),
            glass_voxel_cache_radiance: Resource::new(glass_voxel_cache_radiance),
            glass_voxel_cache_active_slots: Resource::new(glass_voxel_cache_active_slots),
            glass_voxel_cache_active_count: Resource::new(glass_voxel_cache_active_count),
            composited_tex: Resource::new(composited_tex),
        }
    }

    fn create_glass_voxel_cache_buffer(
        device: Device,
        allocator: Allocator,
        capacity: u32,
        bytes_per_entry: u64,
        transfer_dst: bool,
    ) -> Buffer {
        Buffer::new_sized(
            device,
            allocator,
            BufferUsage::from_flags(
                vk::BufferUsageFlags::STORAGE_BUFFER
                    | if transfer_dst {
                        vk::BufferUsageFlags::TRANSFER_DST
                    } else {
                        vk::BufferUsageFlags::empty()
                    },
            ),
            MemoryLocation::GpuOnly,
            u64::from(capacity) * bytes_per_entry,
        )
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
        Self::create_r32_float_tex(device, allocator, rendering_extent, false)
    }

    fn create_compute_output_tex(
        device: Device,
        allocator: Allocator,
        rendering_extent: Extent2D,
    ) -> Texture {
        Self::create_r32_uint_tex(device, allocator, rendering_extent, false)
    }

    fn create_r32_float_tex(
        device: Device,
        allocator: Allocator,
        rendering_extent: Extent2D,
        transfer_src: bool,
    ) -> Texture {
        let tex_desc = ImageDesc {
            extent: rendering_extent.into(),
            format: vk::Format::R32_SFLOAT,
            usage: vk::ImageUsageFlags::STORAGE
                | vk::ImageUsageFlags::SAMPLED
                | if transfer_src {
                    vk::ImageUsageFlags::TRANSFER_SRC
                } else {
                    vk::ImageUsageFlags::empty()
                },
            initial_layout: TextureLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        Texture::new(device, allocator, &tex_desc, &Default::default())
    }

    fn create_r32_uint_tex(
        device: Device,
        allocator: Allocator,
        rendering_extent: Extent2D,
        transfer_src: bool,
    ) -> Texture {
        let tex_desc = ImageDesc {
            extent: rendering_extent.into(),
            format: vk::Format::R32_UINT,
            usage: vk::ImageUsageFlags::STORAGE
                | vk::ImageUsageFlags::SAMPLED
                | if transfer_src {
                    vk::ImageUsageFlags::TRANSFER_SRC
                } else {
                    vk::ImageUsageFlags::empty()
                },
            initial_layout: TextureLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        Texture::new(device, allocator, &tex_desc, &Default::default())
    }

    fn create_rg32_uint_tex(
        device: Device,
        allocator: Allocator,
        rendering_extent: Extent2D,
        transfer_src: bool,
    ) -> Texture {
        let tex_desc = ImageDesc {
            extent: rendering_extent.into(),
            format: vk::Format::R32G32_UINT,
            usage: vk::ImageUsageFlags::STORAGE
                | vk::ImageUsageFlags::SAMPLED
                | if transfer_src {
                    vk::ImageUsageFlags::TRANSFER_SRC
                } else {
                    vk::ImageUsageFlags::empty()
                },
            initial_layout: TextureLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        Texture::new(device, allocator, &tex_desc, &Default::default())
    }

    fn create_environment_irradiance_capture(
        device: Device,
        allocator: Allocator,
        rendering_extent: Extent2D,
        enabled: bool,
    ) -> Buffer {
        let captured_pixel_count = if enabled {
            u64::from(rendering_extent.width) * u64::from(rendering_extent.height)
        } else {
            1
        };
        let byte_count = captured_pixel_count
            * std::mem::size_of::<[f32; 4]>() as u64
            * u64::from(super::ENVIRONMENT_IRRADIANCE_CAPTURE_PLANE_COUNT);
        Buffer::new_sized(
            device,
            allocator,
            BufferUsage::storage_buffer().with_transfer_src(),
            MemoryLocation::GpuOnly,
            byte_count,
        )
    }

    fn create_ddgi_spatial_weight_readback(device: Device, allocator: Allocator) -> Buffer {
        Buffer::new_sized(
            device,
            allocator,
            BufferUsage::storage_buffer().with_transfer_src(),
            MemoryLocation::GpuOnly,
            DDGI_SPATIAL_WEIGHT_READBACK_BYTE_COUNT as u64,
        )
    }

    fn create_gfx_output_tex(
        device: Device,
        allocator: Allocator,
        rendering_extent: Extent2D,
    ) -> Texture {
        let tex_desc = ImageDesc {
            extent: rendering_extent.into(),
            format: vk::Format::R16G16B16A16_SFLOAT,
            usage: vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::COLOR_ATTACHMENT
                | vk::ImageUsageFlags::TRANSFER_DST,
            initial_layout: TextureLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        Texture::new(device, allocator, &tex_desc, &Default::default())
    }

    fn create_god_ray_tex(
        device: Device,
        allocator: Allocator,
        rendering_extent: Extent2D,
    ) -> Texture {
        let god_ray_extent = Extent2D::new(
            (rendering_extent.width / 2).max(1),
            (rendering_extent.height / 2).max(1),
        );
        let tex_desc = ImageDesc {
            extent: god_ray_extent.into(),
            format: vk::Format::R32_SFLOAT,
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

    fn create_screen_output_tex(
        device: Device,
        allocator: Allocator,
        screen_extent: Extent2D,
    ) -> Texture {
        let tex_desc = ImageDesc {
            extent: screen_extent.into(),
            format: vk::Format::R16G16B16A16_SFLOAT,
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

    fn create_lens_flare_tex(
        device: Device,
        allocator: Allocator,
        rendering_extent: Extent2D,
    ) -> Texture {
        let lens_flare_extent = lens_flare_extent(rendering_extent);
        let tex_desc = ImageDesc {
            extent: lens_flare_extent.into(),
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

    fn create_hdr_tex(device: Device, allocator: Allocator, rendering_extent: Extent2D) -> Texture {
        let tex_desc = ImageDesc {
            extent: rendering_extent.into(),
            format: vk::Format::R16G16B16A16_SFLOAT,
            usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lens_flare_runs_at_half_the_internal_render_extent() {
        assert_eq!(
            lens_flare_extent(Extent2D::new(1920, 1080)),
            Extent2D::new(960, 540)
        );
        assert_eq!(lens_flare_extent(Extent2D::new(1, 1)), Extent2D::new(1, 1));
    }

    #[test]
    fn glass_resources_use_full_extent_only_when_the_experiment_is_enabled() {
        let extent = Extent2D::new(960, 540);
        assert_eq!(glass_resource_extent(extent, true), extent);
        assert_eq!(glass_resource_extent(extent, false), Extent2D::new(2, 2));
    }

    #[test]
    fn glass_voxel_cache_is_large_only_for_the_isolated_experiment() {
        assert_eq!(glass_voxel_cache_capacity(true), GLASS_VOXEL_CACHE_CAPACITY);
        assert_eq!(glass_voxel_cache_capacity(false), 1);
        assert!(GLASS_VOXEL_CACHE_CAPACITY.is_power_of_two());
    }
}
