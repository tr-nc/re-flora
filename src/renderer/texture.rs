use super::buffer::*;
use crate::renderer::allocator::Allocator;
use ash::vk;
use ash::Device;

/// Helper struct representing a sampled texture.
pub struct Texture {
    pub image: vk::Image,
    image_mem: gpu_allocator::vulkan::Allocation,
    pub image_view: vk::ImageView,
    pub sampler: vk::Sampler,
}

impl Texture {
    pub fn from_rgba8(
        device: &Device,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        allocator: &mut Allocator,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> Self {
        let (texture, staging_buff, staging_mem) =
            execute_one_time_commands(device, queue, command_pool, |buffer| {
                Self::cmd_from_rgba(device, allocator, buffer, width, height, data)
            });
        allocator.destroy_buffer(device, staging_buff, staging_mem);
        texture
    }

    fn cmd_from_rgba(
        device: &Device,
        allocator: &mut Allocator,
        command_buffer: vk::CommandBuffer,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> (Self, vk::Buffer, gpu_allocator::vulkan::Allocation) {
        let (image, image_mem) = allocator.create_image(device, width, height);

        let image_view = {
            let create_info = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(vk::Format::R8G8B8A8_SRGB)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });

            unsafe { device.create_image_view(&create_info, None).unwrap() }
        };

        let sampler = {
            let sampler_info = vk::SamplerCreateInfo::default()
                .mag_filter(vk::Filter::LINEAR)
                .min_filter(vk::Filter::LINEAR)
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .anisotropy_enable(false)
                .max_anisotropy(1.0)
                .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
                .unnormalized_coordinates(false)
                .compare_enable(false)
                .compare_op(vk::CompareOp::ALWAYS)
                .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
                .mip_lod_bias(0.0)
                .min_lod(0.0)
                .max_lod(1.0);
            unsafe { device.create_sampler(&sampler_info, None).unwrap() }
        };

        let mut texture = Self {
            image,
            image_mem,
            image_view,
            sampler,
        };

        let region = vk::Rect2D {
            extent: vk::Extent2D { width, height },

            ..Default::default()
        };
        let (staging_buffer, staging_buffer_mem) =
            texture.cmd_update(device, command_buffer, allocator, region, data);

        (texture, staging_buffer, staging_buffer_mem)
    }

    pub fn update(
        &mut self,
        device: &Device,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        allocator: &mut Allocator,
        region: vk::Rect2D,
        data: &[u8],
    ) {
        let (staging_buff, staging_mem) =
            execute_one_time_commands(device, queue, command_pool, |buffer| {
                self.cmd_update(device, buffer, allocator, region, data)
            });
        allocator.destroy_buffer(device, staging_buff, staging_mem);
    }

    fn cmd_update(
        &mut self,
        device: &Device,
        command_buffer: vk::CommandBuffer,
        allocator: &mut Allocator,
        region: vk::Rect2D,
        data: &[u8],
    ) -> (vk::Buffer, gpu_allocator::vulkan::Allocation) {
        let (buffer, buffer_mem) =
            create_and_fill_buffer(device, allocator, data, vk::BufferUsageFlags::TRANSFER_SRC);

        // Transition the image layout and copy the buffer into the image
        // and transition the layout again to be readable from fragment shader.
        {
            let mut barrier = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(self.image)
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
                device.cmd_pipeline_barrier(
                    command_buffer,
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
                    x: region.offset.x,
                    y: region.offset.y,
                    z: 0,
                })
                .image_extent(vk::Extent3D {
                    width: region.extent.width,
                    height: region.extent.height,
                    depth: 1,
                });
            unsafe {
                device.cmd_copy_buffer_to_image(
                    command_buffer,
                    buffer,
                    self.image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[region],
                )
            }

            barrier.old_layout = vk::ImageLayout::TRANSFER_DST_OPTIMAL;
            barrier.new_layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
            barrier.src_access_mask = vk::AccessFlags::TRANSFER_WRITE;
            barrier.dst_access_mask = vk::AccessFlags::SHADER_READ;

            unsafe {
                device.cmd_pipeline_barrier(
                    command_buffer,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[barrier],
                )
            };
        }

        (buffer, buffer_mem)
    }

    /// Free texture's resources.
    pub fn destroy(self, device: &Device, allocator: &mut Allocator) {
        unsafe {
            device.destroy_sampler(self.sampler, None);
            device.destroy_image_view(self.image_view, None);
            allocator.destroy_image(device, self.image, self.image_mem);
        }
    }
}

fn execute_one_time_commands<R, F: FnOnce(vk::CommandBuffer) -> R>(
    device: &Device,
    queue: vk::Queue,
    pool: vk::CommandPool,
    executor: F,
) -> R {
    let command_buffer = {
        let alloc_info = vk::CommandBufferAllocateInfo::default()
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_pool(pool)
            .command_buffer_count(1);

        unsafe { device.allocate_command_buffers(&alloc_info).unwrap()[0] }
    };
    let command_buffers = [command_buffer];

    // Begin recording
    {
        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe {
            device
                .begin_command_buffer(command_buffer, &begin_info)
                .unwrap()
        };
    }

    // Execute user function
    let executor_result = executor(command_buffer);

    // End recording
    unsafe { device.end_command_buffer(command_buffer).unwrap() };

    // Submit and wait
    {
        let submit_info = vk::SubmitInfo::default().command_buffers(&command_buffers);
        let submit_infos = [submit_info];
        unsafe {
            device
                .queue_submit(queue, &submit_infos, vk::Fence::null())
                .unwrap();
            device.queue_wait_idle(queue).unwrap();
        };
    }

    // Free
    unsafe { device.free_command_buffers(pool, &command_buffers) };

    executor_result
}
