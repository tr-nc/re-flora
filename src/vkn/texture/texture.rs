use super::{Image, ImageView, ImageViewDesc, Sampler, SamplerDesc};
use crate::vkn::{execute_one_time_command, Allocator, Buffer, CommandPool, Device, Queue};
use ash::vk::{self, ImageType};

/// A texture is a combination of an image, image view, and sampler.
// TODO: #[derive(Clone)]
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

#[derive(Copy, Clone)]
pub struct TextureDesc {
    pub extent: [u32; 3],
    pub format: vk::Format,
    pub usage: vk::ImageUsageFlags,
    pub initial_layout: vk::ImageLayout,
    pub samples: vk::SampleCountFlags,
    pub tilting: vk::ImageTiling,
    pub aspect: vk::ImageAspectFlags,
}

impl Default for TextureDesc {
    fn default() -> Self {
        Self {
            extent: [0, 0, 0],
            format: vk::Format::UNDEFINED,
            usage: vk::ImageUsageFlags::empty(),
            initial_layout: vk::ImageLayout::GENERAL,
            samples: vk::SampleCountFlags::TYPE_1,
            tilting: vk::ImageTiling::OPTIMAL,
            aspect: vk::ImageAspectFlags::COLOR,
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
        allocator: &mut Allocator,
        texture_desc: TextureDesc,
        sampler_desc: SamplerDesc,
    ) -> Self {
        let image = Image::new(device, allocator, &texture_desc);

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

    pub fn upload_rgba_image(
        &mut self,
        queue: &Queue,
        command_pool: &CommandPool,
        region: TextureUploadRegion,
        data: &[u8],
    ) -> &mut Self {
        let mut buffer = Buffer::new_sized(
            &self.device,
            self.image.get_allocator_mut(),
            vk::BufferUsageFlags::TRANSFER_SRC,
            data.len(),
        );
        buffer.fill(data);

        execute_one_time_command(&self.device.clone(), command_pool, queue, |cmdbuf| {
            // Transition the image layout and copy the buffer into the image
            // and transition the layout again to be readable from fragment shader.
            let mut barrier = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(self.image.as_raw())
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);

            unsafe {
                self.device.cmd_pipeline_barrier(
                    cmdbuf.as_raw(),
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[barrier],
                )
            };

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

            barrier.old_layout = vk::ImageLayout::TRANSFER_DST_OPTIMAL;
            barrier.new_layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
            barrier.src_access_mask = vk::AccessFlags::TRANSFER_WRITE;
            barrier.dst_access_mask = vk::AccessFlags::SHADER_READ;

            unsafe {
                self.device.cmd_pipeline_barrier(
                    cmdbuf.as_raw(),
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[barrier],
                )
            };
        });
        self
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
