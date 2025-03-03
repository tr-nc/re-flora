use super::{Image, ImageView, ImageViewDesc, Sampler, SamplerDesc};
use crate::vkn::{execute_one_time_command, Allocator, Buffer, CommandPool, Device, Queue};
use ash::vk::{self, ImageType};

/// A texture is a combination of an image, image view, and sampler.
#[derive(Clone)]
pub struct Texture {
    device: Device,
    image: Image,
    image_view: ImageView,
    sampler: Sampler,
}

pub struct TextureUploadRegion {
    pub offset: [i32; 2],
    pub extent: [u32; 2],
}

impl Default for TextureUploadRegion {
    fn default() -> Self {
        Self {
            offset: [0, 0],
            extent: [0, 0],
        }
    }
}

/// Responsible for the creation of image and image view.
#[derive(Copy, Clone)]
pub struct TextureDesc {
    pub extent: [u32; 3],
    pub format: vk::Format,
    pub usage: vk::ImageUsageFlags,
    pub initial_layout: vk::ImageLayout,
    pub aspect: vk::ImageAspectFlags,
    pub samples: vk::SampleCountFlags,
    pub tilting: vk::ImageTiling,
}

impl Default for TextureDesc {
    fn default() -> Self {
        Self {
            extent: [0, 0, 0],
            format: vk::Format::UNDEFINED,
            usage: vk::ImageUsageFlags::empty(),
            initial_layout: vk::ImageLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            samples: vk::SampleCountFlags::TYPE_1,
            tilting: vk::ImageTiling::OPTIMAL,
        }
    }
}

impl TextureDesc {
    pub fn get_extent(&self) -> vk::Extent3D {
        vk::Extent3D {
            width: self.extent[0],
            height: self.extent[1],
            depth: self.extent[2],
        }
    }

    pub fn get_image_type(&self) -> vk::ImageType {
        match self.extent {
            [_, 1, 1] => vk::ImageType::TYPE_1D,
            [_, _, 1] => vk::ImageType::TYPE_2D,
            _ => vk::ImageType::TYPE_3D,
        }
    }
}

impl Texture {
    pub fn new(
        device: &Device,
        allocator: &Allocator,
        texture_desc: &TextureDesc,
        sampler_desc: &SamplerDesc,
    ) -> Self {
        let image = Image::new(device, allocator, &texture_desc).unwrap();

        let image_view_desc = ImageViewDesc {
            image: image.as_raw(),
            format: texture_desc.format,
            image_view_type: image_type_to_image_view_type(texture_desc.get_image_type()).unwrap(),
            aspect: texture_desc.aspect,
        };
        let image_view = ImageView::new(device, image_view_desc);
        let sampler = Sampler::new(device, sampler_desc);

        Self {
            device: device.clone(),
            image,
            image_view,
            sampler,
        }
    }

    /// Uploads an RGBA image to the texture. The image is transitioned to the `dst_image_layout` afterwords.
    pub fn upload_rgba_image(
        &mut self,
        queue: &Queue,
        command_pool: &CommandPool,
        dst_image_layout: vk::ImageLayout,
        region: TextureUploadRegion,
        data: &[u8],
    ) -> &mut Self {
        let mut buffer = Buffer::new_sized(
            &self.device,
            &mut self.image.get_allocator().clone(),
            vk::BufferUsageFlags::TRANSFER_SRC,
            data.len(),
        );
        buffer.fill(data);

        execute_one_time_command(&self.device.clone(), command_pool, queue, |cmdbuf| {
            self.image
                .record_transition(cmdbuf, vk::ImageLayout::TRANSFER_DST_OPTIMAL);
            let region = vk::BufferImageCopy::default()
                .buffer_offset(0)
                .buffer_row_length(0)
                .buffer_image_height(0)
                .image_subresource(vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .image_offset(vk::Offset3D {
                    x: region.offset[0],
                    y: region.offset[1],
                    z: 0,
                })
                .image_extent(vk::Extent3D {
                    width: region.extent[0],
                    height: region.extent[1],
                    depth: 1,
                });
            unsafe {
                self.device.cmd_copy_buffer_to_image(
                    cmdbuf.as_raw(),
                    buffer.as_raw(),
                    self.image.as_raw(),
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[region],
                )
            }
            self.image.record_transition(cmdbuf, dst_image_layout);
        });
        self
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
