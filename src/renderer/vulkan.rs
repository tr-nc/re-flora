//! Vulkan helpers.
//!
//! A set of functions used to ease Vulkan resources creations. These are supposed to be internal but
//! are exposed since they might help users create descriptors sets when using the custom textures.

use ash::{vk, Device};
pub use buffer::*;
use shaderc;
use std::{
    ffi::CString,
    mem::{self, size_of},
};
pub use texture::*;

use super::RendererOptions;

/// Return a `&[u8]` for any sized object passed in.
pub unsafe fn any_as_u8_slice<T: Sized>(any: &T) -> &[u8] {
    let ptr = (any as *const T) as *const u8;
    std::slice::from_raw_parts(ptr, std::mem::size_of::<T>())
}

/// Create a descriptor set layout compatible with the graphics pipeline.
pub fn create_vulkan_descriptor_set_layout(device: &Device) -> vk::DescriptorSetLayout {
    log::debug!("Creating vulkan descriptor set layout");
    let bindings = [vk::DescriptorSetLayoutBinding::default()
        .binding(0)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .descriptor_count(1)
        .stage_flags(vk::ShaderStageFlags::FRAGMENT)];

    let descriptor_set_create_info =
        vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);

    unsafe {
        device
            .create_descriptor_set_layout(&descriptor_set_create_info, None)
            .unwrap()
    }
}

pub fn create_vulkan_pipeline_layout(
    device: &Device,
    descriptor_set_layout: vk::DescriptorSetLayout,
) -> vk::PipelineLayout {
    log::debug!("Creating vulkan pipeline layout");
    let push_const_range = [vk::PushConstantRange {
        stage_flags: vk::ShaderStageFlags::VERTEX,
        offset: 0,
        size: mem::size_of::<[f32; 16]>() as u32,
    }];

    let descriptor_set_layouts = [descriptor_set_layout];
    let layout_info = vk::PipelineLayoutCreateInfo::default()
        .set_layouts(&descriptor_set_layouts)
        .push_constant_ranges(&push_const_range);
    let pipeline_layout = unsafe { device.create_pipeline_layout(&layout_info, None).unwrap() };

    pipeline_layout
}

pub fn create_vulkan_pipeline(
    device: &Device,
    pipeline_layout: vk::PipelineLayout,
    render_pass: vk::RenderPass,
    options: RendererOptions,
) -> vk::Pipeline {
    let frag_source = std::include_str!("./shaders/shader.frag");
    let vert_source = std::include_str!("./shaders/shader.vert");

    let compiler = shaderc::Compiler::new().unwrap();
    let mut compile_options = shaderc::CompileOptions::new().unwrap();
    compile_options.set_optimization_level(shaderc::OptimizationLevel::Performance);

    let frag_artifact = compiler
        .compile_into_spirv(
            frag_source,
            shaderc::ShaderKind::Fragment,
            "shader.frag",
            "main",
            Some(&compile_options),
        )
        .unwrap();

    let vert_artifact = compiler
        .compile_into_spirv(
            vert_source,
            shaderc::ShaderKind::Vertex,
            "shader.vert",
            "main",
            Some(&compile_options),
        )
        .unwrap();

    let entry_point_name = CString::new("main").unwrap();

    let vertex_create_info = vk::ShaderModuleCreateInfo::default().code(&vert_artifact.as_binary());
    let vertex_module = unsafe {
        device
            .create_shader_module(&vertex_create_info, None)
            .unwrap()
    };

    let fragment_create_info =
        vk::ShaderModuleCreateInfo::default().code(&frag_artifact.as_binary());
    let fragment_module = unsafe {
        device
            .create_shader_module(&fragment_create_info, None)
            .unwrap()
    };

    let specialization_entries = [vk::SpecializationMapEntry {
        constant_id: 0,
        offset: 0,
        size: size_of::<vk::Bool32>(),
    }];
    let data = [vk::Bool32::from(options.srgb_framebuffer)];
    let data_raw = unsafe { any_as_u8_slice(&data) };
    let specialization_info = vk::SpecializationInfo::default()
        .map_entries(&specialization_entries)
        .data(data_raw);

    let shader_states_infos = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vertex_module)
            .name(&entry_point_name),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(fragment_module)
            .specialization_info(&specialization_info)
            .name(&entry_point_name),
    ];

    let binding_desc = [vk::VertexInputBindingDescription::default()
        .binding(0)
        .stride(20)
        .input_rate(vk::VertexInputRate::VERTEX)];
    let attribute_desc = [
        vk::VertexInputAttributeDescription::default()
            .binding(0)
            .location(0)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(0),
        vk::VertexInputAttributeDescription::default()
            .binding(0)
            .location(1)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(8),
        vk::VertexInputAttributeDescription::default()
            .binding(0)
            .location(2)
            .format(vk::Format::R8G8B8A8_UNORM)
            .offset(16),
    ];

    let vertex_input_info = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(&binding_desc)
        .vertex_attribute_descriptions(&attribute_desc);

    let input_assembly_info = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
        .primitive_restart_enable(false);

    let rasterizer_info = vk::PipelineRasterizationStateCreateInfo::default()
        .depth_clamp_enable(false)
        .rasterizer_discard_enable(false)
        .polygon_mode(vk::PolygonMode::FILL)
        .line_width(1.0)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::CLOCKWISE)
        .depth_bias_enable(false)
        .depth_bias_constant_factor(0.0)
        .depth_bias_clamp(0.0)
        .depth_bias_slope_factor(0.0);

    let viewports = [Default::default()];
    let scissors = [Default::default()];
    let viewport_info = vk::PipelineViewportStateCreateInfo::default()
        .viewports(&viewports)
        .scissors(&scissors);

    let multisampling_info = vk::PipelineMultisampleStateCreateInfo::default()
        .sample_shading_enable(false)
        .rasterization_samples(vk::SampleCountFlags::TYPE_1)
        .min_sample_shading(1.0)
        .alpha_to_coverage_enable(false)
        .alpha_to_one_enable(false);

    let color_blend_attachments = [vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(
            vk::ColorComponentFlags::R
                | vk::ColorComponentFlags::G
                | vk::ColorComponentFlags::B
                | vk::ColorComponentFlags::A,
        )
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::ONE)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_DST_ALPHA)
        .dst_alpha_blend_factor(vk::BlendFactor::ONE)
        .alpha_blend_op(vk::BlendOp::ADD)];
    let color_blending_info = vk::PipelineColorBlendStateCreateInfo::default()
        .logic_op_enable(false)
        .logic_op(vk::LogicOp::COPY)
        .attachments(&color_blend_attachments)
        .blend_constants([0.0, 0.0, 0.0, 0.0]);

    let depth_stencil_state_create_info = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(options.enable_depth_test)
        .depth_write_enable(options.enable_depth_write)
        .depth_compare_op(vk::CompareOp::ALWAYS)
        .depth_bounds_test_enable(false)
        .stencil_test_enable(false);

    let dynamic_states = [vk::DynamicState::SCISSOR, vk::DynamicState::VIEWPORT];
    let dynamic_states_info =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

    let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&shader_states_infos)
        .vertex_input_state(&vertex_input_info)
        .input_assembly_state(&input_assembly_info)
        .rasterization_state(&rasterizer_info)
        .viewport_state(&viewport_info)
        .multisample_state(&multisampling_info)
        .color_blend_state(&color_blending_info)
        .depth_stencil_state(&depth_stencil_state_create_info)
        .dynamic_state(&dynamic_states_info)
        .layout(pipeline_layout);

    let pipeline_info = pipeline_info.render_pass(render_pass);

    let pipeline = unsafe {
        device
            .create_graphics_pipelines(
                vk::PipelineCache::null(),
                std::slice::from_ref(&pipeline_info),
                None,
            )
            .map_err(|e| e.1)
            .unwrap()[0]
    };

    unsafe {
        device.destroy_shader_module(vertex_module, None);
        device.destroy_shader_module(fragment_module, None);
    }

    pipeline
}

/// Create a descriptor pool of sets compatible with the graphics pipeline.
pub fn create_vulkan_descriptor_pool(device: &Device, max_sets: u32) -> vk::DescriptorPool {
    log::debug!("Creating vulkan descriptor pool");

    let sizes = [vk::DescriptorPoolSize {
        ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
        descriptor_count: max_sets,
    }];
    let create_info = vk::DescriptorPoolCreateInfo::default()
        .pool_sizes(&sizes)
        .max_sets(max_sets)
        .flags(vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET);
    unsafe { device.create_descriptor_pool(&create_info, None).unwrap() }
}

/// Create a descriptor set compatible with the graphics pipeline from a texture.
pub fn create_vulkan_descriptor_set(
    device: &Device,
    set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    image_view: vk::ImageView,
    sampler: vk::Sampler,
) -> vk::DescriptorSet {
    log::trace!("Creating vulkan descriptor set");

    let set = {
        let set_layouts = [set_layout];
        let allocate_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&set_layouts);

        unsafe { device.allocate_descriptor_sets(&allocate_info).unwrap()[0] }
    };

    unsafe {
        let image_info = [vk::DescriptorImageInfo {
            sampler,
            image_view,
            image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        }];

        let writes = [vk::WriteDescriptorSet::default()
            .dst_set(set)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&image_info)];
        device.update_descriptor_sets(&writes, &[])
    }
    set
}

mod buffer {

    use crate::renderer::allocator::Allocator;
    use ash::vk;
    use ash::Device;

    pub fn create_and_fill_buffer<T>(
        device: &Device,
        allocator: &mut Allocator,
        data: &[T],
        usage: vk::BufferUsageFlags,
    ) -> (vk::Buffer, gpu_allocator::vulkan::Allocation)
    where
        T: Copy,
    {
        let size = std::mem::size_of_val(data);
        let (buffer, mut memory) = allocator.create_buffer(device, size, usage);
        allocator.update_buffer(device, &mut memory, data);
        (buffer, memory)
    }
}

mod texture {

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
}
