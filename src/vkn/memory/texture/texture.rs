use super::{Image, ImageView, ImageViewDesc, Sampler, SamplerDesc, ImageDesc};
use crate::vkn::{Allocator, Device};
use ash::vk::{self, ImageType};

/// A texture is a combination of an image, image view, and sampler.
#[derive(Clone)]
pub struct Texture {
    image: Image,
    image_view: ImageView,
    sampler: Sampler,
}

impl Texture {
    /// Creates a new texture with the given `texture_desc` and `sampler_desc`.
    ///
    /// The memory location is always `GpuOnly`, in contrast to the buffer, which can be mapped on the CPU side.
    pub fn new(
        device: Device,
        allocator: Allocator,
        texture_desc: &ImageDesc,
        sampler_desc: &SamplerDesc,
    ) -> Self {
        let image = Image::new(device.clone(), allocator, &texture_desc).unwrap();

        let image_view_desc = ImageViewDesc {
            image: image.as_raw(),
            format: texture_desc.format,
            image_view_type: image_type_to_image_view_type(texture_desc.get_image_type()).unwrap(),
            aspect: texture_desc.aspect,
        };
        let image_view = ImageView::new(device.clone(), image_view_desc);
        let sampler = Sampler::new(device.clone(), sampler_desc);

        Self {
            image,
            image_view,
            sampler,
        }
    }

    pub fn get_image(&self) -> &Image {
        &self.image
    }

    pub fn get_image_view(&self) -> &ImageView {
        &self.image_view
    }

    pub fn get_sampler(&self) -> &Sampler {
        &self.sampler
    }
}

fn image_type_to_image_view_type(image_type: ImageType) -> Result<vk::ImageViewType, String> {
    match image_type {
        ImageType::TYPE_1D => Ok(vk::ImageViewType::TYPE_1D),
        ImageType::TYPE_2D => Ok(vk::ImageViewType::TYPE_2D),
        ImageType::TYPE_3D => Ok(vk::ImageViewType::TYPE_3D),
        _ => Err(format!("Unsupported image type: {:?}", image_type)),
    }
}
