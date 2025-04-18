use super::{Image, ImageView, ImageViewDesc, Sampler, SamplerDesc, TextureDesc, TextureRegion};
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

impl Texture {
    /// Creates a new texture with the given `texture_desc` and `sampler_desc`.
    ///
    /// The memory location is always `GpuOnly`, in contrast to the buffer, which can be mapped on the CPU side.
    pub fn new(
        device: Device,
        allocator: Allocator,
        texture_desc: &TextureDesc,
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
            device,
            image,
            image_view,
            sampler,
        }
    }

    #[allow(dead_code)]
    pub fn copy_image_to_buffer(
        &self,
        buffer: &mut Buffer,
        queue: &Queue,
        command_pool: &CommandPool,
        dst_image_layout: vk::ImageLayout,
        region: TextureRegion,
    ) {
        execute_one_time_command(&self.device.clone(), command_pool, queue, |cmdbuf| {
            self.image
                .record_transition_barrier(cmdbuf, vk::ImageLayout::TRANSFER_SRC_OPTIMAL);
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
                    z: region.offset[2],
                })
                .image_extent(vk::Extent3D {
                    width: region.extent[0],
                    height: region.extent[1],
                    depth: region.extent[2],
                });
            unsafe {
                self.device.cmd_copy_image_to_buffer(
                    cmdbuf.as_raw(),
                    self.image.as_raw(),
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    buffer.as_raw(),
                    &[region],
                )
            }
            self.image
                .record_transition_barrier(cmdbuf, dst_image_layout);
        });
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
