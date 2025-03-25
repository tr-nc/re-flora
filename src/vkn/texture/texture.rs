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

    /// Uploads an RGBA image to the texture. The image is transitioned into `dst_image_layout` the data upload.
    pub fn upload_rgba_image(
        &self,
        queue: &Queue,
        command_pool: &CommandPool,
        dst_image_layout: vk::ImageLayout,
        region: TextureRegion,
        data: &[u8],
    ) -> Result<&Self, String> {
        let buffer = Buffer::new_sized(
            self.device.clone(),
            self.image.get_allocator().clone(),
            vk::BufferUsageFlags::TRANSFER_SRC,
            gpu_allocator::MemoryLocation::CpuToGpu,
            data.len() as _,
        );
        buffer
            .fill(data)
            .map_err(|e| format!("Failed to fill buffer: {}", e))?;

        execute_one_time_command(&self.device.clone(), command_pool, queue, |cmdbuf| {
            self.image
                .record_transition_barrier(cmdbuf, vk::ImageLayout::TRANSFER_DST_OPTIMAL);
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
            self.image
                .record_transition_barrier(cmdbuf, dst_image_layout);
        });

        Ok(self)
    }

    /// Obtain the image data from the texture of the full image region.
    // TODO: Add support for regions and other formats.
    pub fn fetch_data(&self, queue: &Queue, command_pool: &CommandPool) -> Result<Vec<u8>, String> {
        let buffer = Buffer::new_sized(
            self.device.clone(),
            self.image.get_allocator().clone(),
            vk::BufferUsageFlags::TRANSFER_DST,
            gpu_allocator::MemoryLocation::GpuToCpu,
            self.image.get_size() as _,
        );

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
                .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
                .image_extent(vk::Extent3D {
                    width: self.image.get_desc().extent[0],
                    height: self.image.get_desc().extent[1],
                    depth: self.image.get_desc().extent[2],
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
        });

        let fetched_data = buffer.fetch_raw()?;
        Ok(fetched_data)
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
