use crate::vkn::Extent3D;
use ash::vk;

/// Responsible for the creation of image and image view.
#[derive(Copy, Clone)]
pub struct ImageDesc {
    pub extent: Extent3D,
    pub array_len: u32,
    pub format: vk::Format,
    pub usage: vk::ImageUsageFlags,
    pub initial_layout: vk::ImageLayout,
    pub aspect: vk::ImageAspectFlags,
    pub samples: vk::SampleCountFlags,
    pub tilting: vk::ImageTiling,
}

impl Default for ImageDesc {
    fn default() -> Self {
        Self {
            extent: Extent3D::default(),
            array_len: 1,
            format: vk::Format::UNDEFINED,
            usage: vk::ImageUsageFlags::empty(),
            initial_layout: vk::ImageLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            samples: vk::SampleCountFlags::TYPE_1,
            tilting: vk::ImageTiling::OPTIMAL,
        }
    }
}

pub fn format_to_aspect_mask(format: vk::Format) -> vk::ImageAspectFlags {
    match format {
        // --- Depth-Only Formats ---
        vk::Format::D16_UNORM
        | vk::Format::X8_D24_UNORM_PACK32
        | vk::Format::D32_SFLOAT => vk::ImageAspectFlags::DEPTH,

        // --- Stencil-Only Formats ---
        vk::Format::S8_UINT => vk::ImageAspectFlags::STENCIL,

        // --- Combined Depth-Stencil Formats ---
        vk::Format::D16_UNORM_S8_UINT
        | vk::Format::D24_UNORM_S8_UINT
        | vk::Format::D32_SFLOAT_S8_UINT => {
            vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL
        }

        // --- Color Formats ---
        // This large block handles most common color formats to prevent warnings.
        vk::Format::R8_UNORM | vk::Format::R8_SNORM | vk::Format::R8_USCALED | vk::Format::R8_SSCALED | vk::Format::R8_UINT | vk::Format::R8_SINT | vk::Format::R8_SRGB
        | vk::Format::R8G8_UNORM | vk::Format::R8G8_SNORM | vk::Format::R8G8_USCALED | vk::Format::R8G8_SSCALED | vk::Format::R8G8_UINT | vk::Format::R8G8_SINT | vk::Format::R8G8_SRGB
        | vk::Format::R8G8B8_UNORM | vk::Format::R8G8B8_SNORM | vk::Format::R8G8B8_USCALED | vk::Format::R8G8B8_SSCALED | vk::Format::R8G8B8_UINT | vk::Format::R8G8B8_SINT | vk::Format::R8G8B8_SRGB
        | vk::Format::B8G8R8_UNORM | vk::Format::B8G8R8_SNORM | vk::Format::B8G8R8_USCALED | vk::Format::B8G8R8_SSCALED | vk::Format::B8G8R8_UINT | vk::Format::B8G8R8_SINT | vk::Format::B8G8R8_SRGB
        | vk::Format::R8G8B8A8_UNORM | vk::Format::R8G8B8A8_SNORM | vk::Format::R8G8B8A8_USCALED | vk::Format::R8G8B8A8_SSCALED | vk::Format::R8G8B8A8_UINT | vk::Format::R8G8B8A8_SINT | vk::Format::R8G8B8A8_SRGB
        | vk::Format::B8G8R8A8_UNORM | vk::Format::B8G8R8A8_SNORM | vk::Format::B8G8R8A8_USCALED | vk::Format::B8G8R8A8_SSCALED | vk::Format::B8G8R8A8_UINT | vk::Format::B8G8R8A8_SINT | vk::Format::B8G8R8A8_SRGB
        | vk::Format::A8B8G8R8_UNORM_PACK32 | vk::Format::A8B8G8R8_SNORM_PACK32 | vk::Format::A8B8G8R8_USCALED_PACK32 | vk::Format::A8B8G8R8_SSCALED_PACK32 | vk::Format::A8B8G8R8_UINT_PACK32 | vk::Format::A8B8G8R8_SINT_PACK32 | vk::Format::A8B8G8R8_SRGB_PACK32
        | vk::Format::A2R10G10B10_UNORM_PACK32 | vk::Format::A2R10G10B10_UINT_PACK32
        | vk::Format::A2B10G10R10_UNORM_PACK32 | vk::Format::A2B10G10R10_UINT_PACK32
        | vk::Format::R16_UNORM | vk::Format::R16_SNORM | vk::Format::R16_USCALED | vk::Format::R16_SSCALED | vk::Format::R16_UINT | vk::Format::R16_SINT | vk::Format::R16_SFLOAT
        | vk::Format::R16G16_UNORM | vk::Format::R16G16_SNORM | vk::Format::R16G16_USCALED | vk::Format::R16G16_SSCALED | vk::Format::R16G16_UINT | vk::Format::R16G16_SINT | vk::Format::R16G16_SFLOAT
        | vk::Format::R16G16B16_UNORM | vk::Format::R16G16B16_SNORM | vk::Format::R16G16B16_USCALED | vk::Format::R16G16B16_SSCALED | vk::Format::R16G16B16_UINT | vk::Format::R16G16B16_SINT | vk::Format::R16G16B16_SFLOAT
        | vk::Format::R16G16B16A16_UNORM | vk::Format::R16G16B16A16_SNORM | vk::Format::R16G16B16A16_USCALED | vk::Format::R16G16B16A16_SSCALED | vk::Format::R16G16B16A16_UINT | vk::Format::R16G16B16A16_SINT | vk::Format::R16G16B16A16_SFLOAT
        | vk::Format::R32_UINT | vk::Format::R32_SINT | vk::Format::R32_SFLOAT
        | vk::Format::R32G32_UINT | vk::Format::R32G32_SINT | vk::Format::R32G32_SFLOAT
        | vk::Format::R32G32B32_UINT | vk::Format::R32G32B32_SINT | vk::Format::R32G32B32_SFLOAT
        | vk::Format::R32G32B32A32_UINT | vk::Format::R32G32B32A32_SINT | vk::Format::R32G32B32A32_SFLOAT
        | vk::Format::B10G11R11_UFLOAT_PACK32
        | vk::Format::E5B9G9R9_UFLOAT_PACK32
        | vk::Format::BC1_RGB_UNORM_BLOCK | vk::Format::BC1_RGB_SRGB_BLOCK
        | vk::Format::BC1_RGBA_UNORM_BLOCK | vk::Format::BC1_RGBA_SRGB_BLOCK
        | vk::Format::BC2_UNORM_BLOCK | vk::Format::BC2_SRGB_BLOCK
        | vk::Format::BC3_UNORM_BLOCK | vk::Format::BC3_SRGB_BLOCK
        | vk::Format::BC4_UNORM_BLOCK | vk::Format::BC4_SNORM_BLOCK
        | vk::Format::BC5_UNORM_BLOCK | vk::Format::BC5_SNORM_BLOCK
        | vk::Format::BC6H_UFLOAT_BLOCK | vk::Format::BC6H_SFLOAT_BLOCK
        | vk::Format::BC7_UNORM_BLOCK | vk::Format::BC7_SRGB_BLOCK
        | vk::Format::ETC2_R8G8B8_UNORM_BLOCK | vk::Format::ETC2_R8G8B8_SRGB_BLOCK
        | vk::Format::ETC2_R8G8B8A1_UNORM_BLOCK | vk::Format::ETC2_R8G8B8A1_SRGB_BLOCK
        | vk::Format::ETC2_R8G8B8A8_UNORM_BLOCK | vk::Format::ETC2_R8G8B8A8_SRGB_BLOCK
        | vk::Format::ASTC_4X4_UNORM_BLOCK | vk::Format::ASTC_4X4_SRGB_BLOCK
        // Add other ASTC block sizes if needed, e.g., ASTC_5x5, etc.
        => {
            vk::ImageAspectFlags::COLOR
        }

        // --- Fallback Case ---
        // This handles any format not explicitly listed above.
        _ => {
            log::warn!("Unknown format: {:?}, using COLOR aspect mask as a fallback.", format);
            vk::ImageAspectFlags::COLOR
        }
    }
}

impl ImageDesc {
    pub fn get_aspect_mask(&self) -> vk::ImageAspectFlags {
        format_to_aspect_mask(self.format)
    }

    pub fn get_image_type(&self) -> vk::ImageType {
        if self.extent.depth == 1 {
            if self.extent.height == 1 {
                vk::ImageType::TYPE_1D
            } else {
                vk::ImageType::TYPE_2D
            }
        } else {
            vk::ImageType::TYPE_3D
        }
    }

    /// Returns the size of a single pixel in bytes.
    pub fn get_pixel_size(&self) -> u32 {
        match self.format {
            // 8-bit formats (1 byte per channel)
            vk::Format::R8_UNORM
            | vk::Format::R8_SNORM
            | vk::Format::R8_USCALED
            | vk::Format::R8_SSCALED
            | vk::Format::R8_UINT
            | vk::Format::R8_SINT
            | vk::Format::R8_SRGB
            | vk::Format::S8_UINT => 1,

            // 16-bit formats (2 bytes per channel or packed formats)
            vk::Format::R16_UNORM
            | vk::Format::R16_SNORM
            | vk::Format::R16_USCALED
            | vk::Format::R16_SSCALED
            | vk::Format::R16_UINT
            | vk::Format::R16_SINT
            | vk::Format::R16_SFLOAT
            | vk::Format::D16_UNORM => 2,

            vk::Format::R8G8_UNORM
            | vk::Format::R8G8_SNORM
            | vk::Format::R8G8_USCALED
            | vk::Format::R8G8_SSCALED
            | vk::Format::R8G8_UINT
            | vk::Format::R8G8_SINT
            | vk::Format::R8G8_SRGB => 2,

            vk::Format::R4G4_UNORM_PACK8 => 1,
            vk::Format::R4G4B4A4_UNORM_PACK16
            | vk::Format::B4G4R4A4_UNORM_PACK16
            | vk::Format::R5G6B5_UNORM_PACK16
            | vk::Format::B5G6R5_UNORM_PACK16
            | vk::Format::R5G5B5A1_UNORM_PACK16
            | vk::Format::B5G5R5A1_UNORM_PACK16
            | vk::Format::A1R5G5B5_UNORM_PACK16 => 2,

            // 24-bit formats (3 bytes)
            vk::Format::R8G8B8_UNORM
            | vk::Format::R8G8B8_SNORM
            | vk::Format::R8G8B8_USCALED
            | vk::Format::R8G8B8_SSCALED
            | vk::Format::R8G8B8_UINT
            | vk::Format::R8G8B8_SINT
            | vk::Format::R8G8B8_SRGB
            | vk::Format::B8G8R8_UNORM
            | vk::Format::B8G8R8_SNORM
            | vk::Format::B8G8R8_USCALED
            | vk::Format::B8G8R8_SSCALED
            | vk::Format::B8G8R8_UINT
            | vk::Format::B8G8R8_SINT
            | vk::Format::B8G8R8_SRGB => 3,

            // 32-bit formats (4 bytes)
            vk::Format::R8G8B8A8_UNORM
            | vk::Format::R8G8B8A8_SNORM
            | vk::Format::R8G8B8A8_USCALED
            | vk::Format::R8G8B8A8_SSCALED
            | vk::Format::R8G8B8A8_UINT
            | vk::Format::R8G8B8A8_SINT
            | vk::Format::R8G8B8A8_SRGB
            | vk::Format::B8G8R8A8_UNORM
            | vk::Format::B8G8R8A8_SNORM
            | vk::Format::B8G8R8A8_USCALED
            | vk::Format::B8G8R8A8_SSCALED
            | vk::Format::B8G8R8A8_UINT
            | vk::Format::B8G8R8A8_SINT
            | vk::Format::B8G8R8A8_SRGB => 4,

            vk::Format::A8B8G8R8_UNORM_PACK32
            | vk::Format::A8B8G8R8_SNORM_PACK32
            | vk::Format::A8B8G8R8_USCALED_PACK32
            | vk::Format::A8B8G8R8_SSCALED_PACK32
            | vk::Format::A8B8G8R8_UINT_PACK32
            | vk::Format::A8B8G8R8_SINT_PACK32
            | vk::Format::A8B8G8R8_SRGB_PACK32 => 4,

            vk::Format::A2R10G10B10_UNORM_PACK32
            | vk::Format::A2R10G10B10_SNORM_PACK32
            | vk::Format::A2R10G10B10_USCALED_PACK32
            | vk::Format::A2R10G10B10_SSCALED_PACK32
            | vk::Format::A2R10G10B10_UINT_PACK32
            | vk::Format::A2R10G10B10_SINT_PACK32
            | vk::Format::A2B10G10R10_UNORM_PACK32
            | vk::Format::A2B10G10R10_SNORM_PACK32
            | vk::Format::A2B10G10R10_USCALED_PACK32
            | vk::Format::A2B10G10R10_SSCALED_PACK32
            | vk::Format::A2B10G10R10_UINT_PACK32
            | vk::Format::A2B10G10R10_SINT_PACK32 => 4,

            vk::Format::R16G16_UNORM
            | vk::Format::R16G16_SNORM
            | vk::Format::R16G16_USCALED
            | vk::Format::R16G16_SSCALED
            | vk::Format::R16G16_UINT
            | vk::Format::R16G16_SINT
            | vk::Format::R16G16_SFLOAT => 4,

            vk::Format::R32_UINT
            | vk::Format::R32_SINT
            | vk::Format::R32_SFLOAT
            | vk::Format::D32_SFLOAT => 4,

            vk::Format::X8_D24_UNORM_PACK32 | vk::Format::D24_UNORM_S8_UINT => 4,

            vk::Format::B10G11R11_UFLOAT_PACK32 | vk::Format::E5B9G9R9_UFLOAT_PACK32 => 4,

            // 48-bit formats (6 bytes)
            vk::Format::R16G16B16_UNORM
            | vk::Format::R16G16B16_SNORM
            | vk::Format::R16G16B16_USCALED
            | vk::Format::R16G16B16_SSCALED
            | vk::Format::R16G16B16_UINT
            | vk::Format::R16G16B16_SINT
            | vk::Format::R16G16B16_SFLOAT => 6,

            // 64-bit formats (8 bytes)
            vk::Format::R16G16B16A16_UNORM
            | vk::Format::R16G16B16A16_SNORM
            | vk::Format::R16G16B16A16_USCALED
            | vk::Format::R16G16B16A16_SSCALED
            | vk::Format::R16G16B16A16_UINT
            | vk::Format::R16G16B16A16_SINT
            | vk::Format::R16G16B16A16_SFLOAT => 8,

            vk::Format::R32G32_UINT | vk::Format::R32G32_SINT | vk::Format::R32G32_SFLOAT => 8,

            vk::Format::R64_UINT | vk::Format::R64_SINT | vk::Format::R64_SFLOAT => 8,

            vk::Format::D32_SFLOAT_S8_UINT => 8, // This is actually a special case that might require 5 bytes

            // 96-bit formats (12 bytes)
            vk::Format::R32G32B32_UINT
            | vk::Format::R32G32B32_SINT
            | vk::Format::R32G32B32_SFLOAT => 12,

            // 128-bit formats (16 bytes)
            vk::Format::R32G32B32A32_UINT
            | vk::Format::R32G32B32A32_SINT
            | vk::Format::R32G32B32A32_SFLOAT => 16,

            vk::Format::R64G64_UINT | vk::Format::R64G64_SINT | vk::Format::R64G64_SFLOAT => 16,

            // 192-bit formats (24 bytes)
            vk::Format::R64G64B64_UINT
            | vk::Format::R64G64B64_SINT
            | vk::Format::R64G64B64_SFLOAT => 24,

            // 256-bit formats (32 bytes)
            vk::Format::R64G64B64A64_UINT
            | vk::Format::R64G64B64A64_SINT
            | vk::Format::R64G64B64A64_SFLOAT => 32,

            // Block compressed formats
            // BC1 formats (64 bits per 4x4 block = 0.5 bytes per pixel)
            vk::Format::BC1_RGB_UNORM_BLOCK
            | vk::Format::BC1_RGB_SRGB_BLOCK
            | vk::Format::BC1_RGBA_UNORM_BLOCK
            | vk::Format::BC1_RGBA_SRGB_BLOCK => 8,

            // BC2/BC3 formats (128 bits per 4x4 block = 1 byte per pixel)
            vk::Format::BC2_UNORM_BLOCK
            | vk::Format::BC2_SRGB_BLOCK
            | vk::Format::BC3_UNORM_BLOCK
            | vk::Format::BC3_SRGB_BLOCK => 16,

            // BC4 formats (64 bits per 4x4 block = 0.5 bytes per pixel)
            vk::Format::BC4_UNORM_BLOCK | vk::Format::BC4_SNORM_BLOCK => 8,

            // BC5 formats (128 bits per 4x4 block = 1 byte per pixel)
            vk::Format::BC5_UNORM_BLOCK | vk::Format::BC5_SNORM_BLOCK => 16,

            // BC6H/BC7 formats (128 bits per 4x4 block = 1 byte per pixel)
            vk::Format::BC6H_UFLOAT_BLOCK
            | vk::Format::BC6H_SFLOAT_BLOCK
            | vk::Format::BC7_UNORM_BLOCK
            | vk::Format::BC7_SRGB_BLOCK => 16,

            // ETC2/EAC formats
            vk::Format::ETC2_R8G8B8_UNORM_BLOCK
            | vk::Format::ETC2_R8G8B8_SRGB_BLOCK
            | vk::Format::ETC2_R8G8B8A1_UNORM_BLOCK
            | vk::Format::ETC2_R8G8B8A1_SRGB_BLOCK => 8,

            vk::Format::ETC2_R8G8B8A8_UNORM_BLOCK | vk::Format::ETC2_R8G8B8A8_SRGB_BLOCK => 16,

            vk::Format::EAC_R11_UNORM_BLOCK | vk::Format::EAC_R11_SNORM_BLOCK => 8,

            vk::Format::EAC_R11G11_UNORM_BLOCK | vk::Format::EAC_R11G11_SNORM_BLOCK => 16,

            // ASTC formats
            vk::Format::ASTC_4X4_UNORM_BLOCK
            | vk::Format::ASTC_4X4_SRGB_BLOCK
            | vk::Format::ASTC_5X4_UNORM_BLOCK
            | vk::Format::ASTC_5X4_SRGB_BLOCK
            | vk::Format::ASTC_5X5_UNORM_BLOCK
            | vk::Format::ASTC_5X5_SRGB_BLOCK
            | vk::Format::ASTC_6X5_UNORM_BLOCK
            | vk::Format::ASTC_6X5_SRGB_BLOCK
            | vk::Format::ASTC_6X6_UNORM_BLOCK
            | vk::Format::ASTC_6X6_SRGB_BLOCK
            | vk::Format::ASTC_8X5_UNORM_BLOCK
            | vk::Format::ASTC_8X5_SRGB_BLOCK
            | vk::Format::ASTC_8X6_UNORM_BLOCK
            | vk::Format::ASTC_8X6_SRGB_BLOCK
            | vk::Format::ASTC_8X8_UNORM_BLOCK
            | vk::Format::ASTC_8X8_SRGB_BLOCK
            | vk::Format::ASTC_10X5_UNORM_BLOCK
            | vk::Format::ASTC_10X5_SRGB_BLOCK
            | vk::Format::ASTC_10X6_UNORM_BLOCK
            | vk::Format::ASTC_10X6_SRGB_BLOCK
            | vk::Format::ASTC_10X8_UNORM_BLOCK
            | vk::Format::ASTC_10X8_SRGB_BLOCK
            | vk::Format::ASTC_10X10_UNORM_BLOCK
            | vk::Format::ASTC_10X10_SRGB_BLOCK
            | vk::Format::ASTC_12X10_UNORM_BLOCK
            | vk::Format::ASTC_12X10_SRGB_BLOCK
            | vk::Format::ASTC_12X12_UNORM_BLOCK
            | vk::Format::ASTC_12X12_SRGB_BLOCK => 16,

            // Special cases or undefined
            vk::Format::UNDEFINED => 0,

            vk::Format::D16_UNORM_S8_UINT => 3, // Special case

            _ => {
                log::error!(
                    "Unsupported format: {:?}, consider implement it here",
                    self.format
                );
                0
            }
        }
    }
}

#[derive(Copy, Clone)]
pub struct SamplerDesc {
    pub mag_filter: vk::Filter,
    pub min_filter: vk::Filter,
    pub address_mode_u: vk::SamplerAddressMode,
    pub address_mode_v: vk::SamplerAddressMode,
    pub address_mode_w: vk::SamplerAddressMode,
    pub anisotropy_enable: bool,
    pub max_anisotropy: f32,
    pub border_color: vk::BorderColor,
    pub unnormalized_coordinates: bool,
    pub compare_enable: bool,
    pub compare_op: vk::CompareOp,
    pub mipmap_mode: vk::SamplerMipmapMode,
    pub mip_lod_bias: f32,
    pub min_lod: f32,
    pub max_lod: f32,
}

impl Default for SamplerDesc {
    fn default() -> Self {
        Self {
            mag_filter: vk::Filter::LINEAR,
            min_filter: vk::Filter::LINEAR,
            mipmap_mode: vk::SamplerMipmapMode::NEAREST,
            address_mode_u: vk::SamplerAddressMode::CLAMP_TO_EDGE,
            address_mode_v: vk::SamplerAddressMode::CLAMP_TO_EDGE,
            address_mode_w: vk::SamplerAddressMode::CLAMP_TO_EDGE,
            anisotropy_enable: false,
            max_anisotropy: 1.0,
            border_color: vk::BorderColor::INT_OPAQUE_BLACK,
            unnormalized_coordinates: false,
            compare_enable: false,
            compare_op: vk::CompareOp::ALWAYS,
            mip_lod_bias: 0.0,
            min_lod: 0.0,
            max_lod: 0.25, // no mipmaps
        }
    }
}
