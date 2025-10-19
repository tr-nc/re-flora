use super::{Image, ImageDesc, ImageView, ImageViewDesc, Sampler, SamplerDesc};
use crate::vkn::{Allocator, Device};
use ash::vk::{self, ImageType};
use std::fmt;

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
        img_desc: &ImageDesc,
        sampler_desc: &SamplerDesc,
    ) -> Self {
        let image = Image::new(device.clone(), allocator, img_desc).unwrap();

        let image_view_desc = ImageViewDesc {
            image: image.as_raw(),
            format: img_desc.format,
            image_view_type: image_type_to_image_view_type(
                img_desc.get_image_type(),
                img_desc.array_len,
            )
            .unwrap(),
            aspect: img_desc.aspect,
            base_array_layer: 0,
            layer_count: img_desc.array_len,
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

impl fmt::Debug for Texture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Texture")
            .field("image", &self.image)
            .field("image_view", &self.image_view)
            .field("sampler", &self.sampler)
            .finish()
    }
}

pub fn image_type_to_image_view_type(
    image_type: ImageType,
    array_len: u32,
) -> Result<vk::ImageViewType, String> {
    if array_len == 0 {
        return Err("array_len must be at least 1".into());
    }

    let view = match image_type {
        ImageType::TYPE_1D => {
            if array_len == 1 {
                vk::ImageViewType::TYPE_1D
            } else {
                vk::ImageViewType::TYPE_1D_ARRAY
            }
        }

        ImageType::TYPE_2D => {
            if array_len == 1 {
                vk::ImageViewType::TYPE_2D
            } else {
                vk::ImageViewType::TYPE_2D_ARRAY
            }
        }

        ImageType::TYPE_3D => {
            if array_len == 1 {
                vk::ImageViewType::TYPE_3D
            } else {
                return Err(format!(
                    "3D images cannot be viewed as arrays (array_len = {})",
                    array_len
                ));
            }
        }

        other => {
            return Err(format!("Unsupported image type: {:?}", other));
        }
    };

    Ok(view)
}
