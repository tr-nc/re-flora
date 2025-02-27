use super::mesh::Mesh;
use ash::vk::Extent2D;
use ash::vk::{self};
use egui::ViewportId;
use egui::{
    epaint::{ImageDelta, Primitive},
    ClippedPrimitive, ImageData, TextureId,
};
use egui_winit::EventResponse;
use gpu_allocator::vulkan::AllocatorCreateDesc;
use std::{collections::HashMap, mem};
use winit::event::WindowEvent;
use winit::window::Window;

use crate::util::compiler::ShaderCompiler;
use crate::vkn::Swapchain;
use crate::vkn::VulkanContext;
use crate::vkn::{
    Allocator, DescriptorPool, DescriptorSetLayout, DescriptorSetLayoutBinding,
    DescriptorSetLayoutBuilder, Device, GraphicsPipeline, PipelineLayout, ShaderModule, Texture,
};

use std::sync::{Arc, Mutex};

/// Optional parameters of the renderer.
#[derive(Debug, Clone, Copy)]
pub struct EguiRendererDesc {
    /// Is the target framebuffer sRGB.
    ///
    /// If not, the fragment shader converts colors to sRGB, otherwise it outputs color in linear space.
    pub srgb_framebuffer: bool,
}

impl Default for EguiRendererDesc {
    fn default() -> Self {
        Self {
            srgb_framebuffer: false,
        }
    }
}

/// Winit-Egui Renderer implemented for Ash Vulkan.
pub struct EguiRenderer {
    vulkan_context: VulkanContext,
    allocator: Allocator,
    pipeline: GraphicsPipeline,
    pipeline_layout: PipelineLayout,
    vert_shader_module: ShaderModule,
    frag_shader_module: ShaderModule,

    descriptor_set_layout: DescriptorSetLayout,
    descriptor_pool: DescriptorPool,
    managed_textures: HashMap<TextureId, Texture>,
    textures: HashMap<TextureId, vk::DescriptorSet>,
    frames: Option<Mesh>,

    textures_to_free: Option<Vec<TextureId>>,

    egui_context: egui::Context,
    egui_winit_state: egui_winit::State,

    desc: EguiRendererDesc,

    // late init
    pixels_per_point: Option<f32>,
    clipped_primitives: Option<Vec<ClippedPrimitive>>,
}

impl EguiRenderer {
    pub fn new(
        vulkan_context: &VulkanContext,
        window: &Window,
        compiler: &ShaderCompiler,
        render_pass: vk::RenderPass,
        desc: EguiRendererDesc,
    ) -> Self {
        let gpu_allocator = {
            let allocator_create_info = AllocatorCreateDesc {
                instance: vulkan_context.instance().as_raw().clone(),
                device: vulkan_context.device().as_raw().clone(),
                physical_device: vulkan_context.physical_device().as_raw(),
                debug_settings: Default::default(),
                buffer_device_address: false,
                allocation_sizes: Default::default(),
            };
            gpu_allocator::vulkan::Allocator::new(&allocator_create_info)
                .expect("Failed to create gpu allocator")
        };
        let allocator = Allocator::new(Arc::new(Mutex::new(gpu_allocator)));

        let device = vulkan_context.device();

        let binding = DescriptorSetLayoutBinding {
            no: 0,
            descriptor_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: 1,
            stage_flags: vk::ShaderStageFlags::FRAGMENT,
        };
        let mut builder = DescriptorSetLayoutBuilder::new();
        builder.add_binding(binding);
        let descriptor_set_layout = builder.build(device).unwrap();
        let descriptor_set_layouts = std::slice::from_ref(&descriptor_set_layout);

        let push_const_range = vk::PushConstantRange {
            stage_flags: vk::ShaderStageFlags::VERTEX,
            offset: 0,
            size: mem::size_of::<[f32; 16]>() as u32,
        };
        let push_const_ranges = std::slice::from_ref(&push_const_range);
        let pipeline_layout = PipelineLayout::new(
            device,
            Some(descriptor_set_layouts),
            Some(push_const_ranges),
        );

        let vert_shader_module = ShaderModule::from_glsl(
            device,
            compiler,
            "src/egui_renderer/shaders/shader.vert",
            "main",
        )
        .unwrap();

        let frag_shader_module = ShaderModule::from_glsl(
            device,
            compiler,
            "src/egui_renderer/shaders/shader.frag",
            "main",
        )
        .unwrap();

        let pipeline = create_pipeline(
            device,
            &pipeline_layout,
            &vert_shader_module,
            &frag_shader_module,
            render_pass,
            desc,
        );

        let descriptor_pool = DescriptorPool::from_descriptor_set_layouts(
            device,
            std::slice::from_ref(&descriptor_set_layout),
        )
        .unwrap();

        let managed_textures = HashMap::new();
        let textures = HashMap::new();

        let egui_context = egui::Context::default();
        let egui_winit_state = egui_winit::State::new(
            egui_context.clone(),
            ViewportId::ROOT,
            window,
            None,
            None,
            None,
        );

        Self {
            vulkan_context: vulkan_context.clone(),
            allocator,
            pipeline,
            pipeline_layout,
            vert_shader_module,
            frag_shader_module,
            descriptor_set_layout,
            descriptor_pool,
            managed_textures,
            textures,
            desc,
            frames: None,
            textures_to_free: None,

            egui_context,
            egui_winit_state,

            pixels_per_point: None,
            clipped_primitives: None,
        }
    }

    pub fn on_window_event(&mut self, window: &Window, event: &WindowEvent) -> EventResponse {
        self.egui_winit_state.on_window_event(window, event)
    }

    /// Set the render pass used by the renderer, by recreating the pipeline.
    ///
    /// This is an expensive operation.
    pub fn set_render_pass(&mut self, render_pass: vk::RenderPass) {
        self.pipeline = create_pipeline(
            &self.vulkan_context.device(),
            &self.pipeline_layout,
            &self.vert_shader_module,
            &self.frag_shader_module,
            render_pass,
            self.desc,
        );
    }

    /// Free egui managed textures.
    ///
    /// You should pass the list of textures detla contained in the [`egui::TexturesDelta::set`].
    /// This method should be called _before_ the frame starts rendering.
    fn set_textures(
        &mut self,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        textures_delta: &[(TextureId, ImageDelta)],
    ) {
        for (id, delta) in textures_delta {
            let (width, height, data) = match &delta.image {
                ImageData::Font(font) => {
                    let w = font.width() as u32;
                    let h = font.height() as u32;
                    let data = font
                        .srgba_pixels(None)
                        .flat_map(|c| c.to_array())
                        .collect::<Vec<_>>();

                    (w, h, data)
                }
                ImageData::Color(image) => {
                    let w = image.width() as u32;
                    let h = image.height() as u32;
                    let data = image
                        .pixels
                        .iter()
                        .flat_map(|c| c.to_array())
                        .collect::<Vec<_>>();

                    (w, h, data)
                }
            };

            let device = &self.vulkan_context.device();

            if let Some([offset_x, offset_y]) = delta.pos {
                let texture = self.managed_textures.get_mut(id).unwrap();

                texture.update(
                    device,
                    queue,
                    command_pool,
                    &mut self.allocator,
                    vk::Rect2D {
                        offset: vk::Offset2D {
                            x: offset_x as _,
                            y: offset_y as _,
                        },
                        extent: vk::Extent2D { width, height },
                    },
                    data.as_slice(),
                );
            } else {
                let texture = Texture::from_rgba8(
                    device,
                    queue,
                    command_pool,
                    &mut self.allocator,
                    width,
                    height,
                    data.as_slice(),
                );

                let set = create_vulkan_descriptor_set(
                    device,
                    self.descriptor_set_layout.as_raw(),
                    self.descriptor_pool.as_raw(),
                    texture.image_view,
                    texture.sampler,
                );

                if let Some(previous) = self.managed_textures.insert(*id, texture) {
                    previous.destroy(device, &mut self.allocator);
                }
                if let Some(previous) = self.textures.insert(*id, set) {
                    unsafe {
                        device
                            .free_descriptor_sets(self.descriptor_pool.as_raw(), &[previous])
                            .unwrap();
                    };
                }
            }
        }
    }

    /// Free egui managed textures.
    ///
    /// You should pass the list of ids contained in the [`egui::TexturesDelta::free`].
    /// This method should be called _after_ the frame is done rendering.
    ///
    /// # Arguments
    ///
    /// * `ids` - The list of ids of textures to free.
    ///
    /// # Errors
    ///
    /// * [`RendererError`] - If any Vulkan error is encountered when free the texture.
    fn free_textures(&mut self, ids: &[TextureId]) {
        log::trace!("Freeing {} textures", ids.len());
        let device = &self.vulkan_context.device();
        for id in ids {
            if let Some(texture) = self.managed_textures.remove(id) {
                texture.destroy(device, &mut self.allocator);
            }
            if let Some(set) = self.textures.remove(id) {
                unsafe {
                    device
                        .free_descriptor_sets(self.descriptor_pool.as_raw(), &[set])
                        .unwrap();
                };
            }
        }
    }

    /// Record commands to render the [`egui::Ui`].
    fn cmd_draw(
        device: &Device,
        frames: &mut Option<Mesh>,
        pipeline: &GraphicsPipeline,
        pipeline_layout: &PipelineLayout,
        textures: &mut HashMap<TextureId, vk::DescriptorSet>,
        allocator: &mut Allocator,
        command_buffer: vk::CommandBuffer,
        extent: vk::Extent2D,
        pixels_per_point: f32,
        primitives: &[ClippedPrimitive],
    ) {
        if primitives.is_empty() {
            return;
        }

        if frames.is_none() {
            println!("Creating new frames");
            frames.replace(Mesh::new(device, allocator, primitives));
        }

        frames
            .as_mut()
            .unwrap()
            .update(device, allocator, primitives);

        unsafe {
            device.cmd_bind_pipeline(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline.as_raw(),
            )
        };

        let screen_width = extent.width as f32;
        let screen_height = extent.height as f32;

        unsafe {
            device.cmd_set_viewport(
                command_buffer,
                0,
                &[vk::Viewport {
                    width: screen_width,
                    height: screen_height,
                    max_depth: 1.0,
                    ..Default::default()
                }],
            )
        };

        // Ortho projection
        let projection = orthographic_vk(
            0.0,
            screen_width / pixels_per_point,
            0.0,
            -(screen_height / pixels_per_point),
            -1.0,
            1.0,
        );
        unsafe {
            let push = any_as_u8_slice(&projection);
            device.cmd_push_constants(
                command_buffer,
                pipeline_layout.as_raw(),
                vk::ShaderStageFlags::VERTEX,
                0,
                push,
            )
        };

        unsafe {
            device.cmd_bind_index_buffer(
                command_buffer,
                frames.as_mut().unwrap().indices_buffer.as_raw(),
                0,
                vk::IndexType::UINT32,
            )
        };

        unsafe {
            device.cmd_bind_vertex_buffers(
                command_buffer,
                0,
                &[frames.as_mut().unwrap().vertices_buffer.as_raw()],
                &[0],
            )
        };

        let mut index_offset = 0u32;
        let mut vertex_offset = 0i32;
        let mut current_texture_id: Option<TextureId> = None;

        for p in primitives {
            let clip_rect = p.clip_rect;
            match &p.primitive {
                Primitive::Mesh(m) => {
                    let clip_x = clip_rect.min.x * pixels_per_point;
                    let clip_y = clip_rect.min.y * pixels_per_point;
                    let clip_w = clip_rect.max.x * pixels_per_point - clip_x;
                    let clip_h = clip_rect.max.y * pixels_per_point - clip_y;

                    let scissors = [vk::Rect2D {
                        offset: vk::Offset2D {
                            x: (clip_x as i32).max(0),
                            y: (clip_y as i32).max(0),
                        },
                        extent: vk::Extent2D {
                            width: clip_w.min(screen_width) as _,
                            height: clip_h.min(screen_height) as _,
                        },
                    }];

                    unsafe {
                        device.cmd_set_scissor(command_buffer, 0, &scissors);
                    }

                    if Some(m.texture_id) != current_texture_id {
                        let descriptor_set = *textures.get(&m.texture_id).unwrap();

                        unsafe {
                            device.cmd_bind_descriptor_sets(
                                command_buffer,
                                vk::PipelineBindPoint::GRAPHICS,
                                pipeline_layout.as_raw(),
                                0,
                                &[descriptor_set],
                                &[],
                            )
                        };
                        current_texture_id = Some(m.texture_id);
                    }

                    let index_count = m.indices.len() as u32;
                    unsafe {
                        device.cmd_draw_indexed(
                            command_buffer,
                            index_count,
                            1,
                            index_offset,
                            vertex_offset,
                            0,
                        )
                    };

                    index_offset += index_count;
                    vertex_offset += m.vertices.len() as i32;
                }
                Primitive::Callback(_) => {
                    log::warn!("Callback primitives not yet supported")
                }
            }
        }
    }

    pub fn update(
        &mut self,
        command_pool: vk::CommandPool,
        window: &Window,
        run_ui: impl FnMut(&egui::Context),
    ) {
        let raw_input = self.egui_winit_state.take_egui_input(window);

        // free last frames textures after the previous frame is done rendering
        if let Some(textures) = self.textures_to_free.take() {
            self.free_textures(&textures);
        }

        let egui::FullOutput {
            platform_output,
            textures_delta,
            shapes,
            pixels_per_point,
            ..
        } = self.egui_context.run(raw_input, run_ui);

        self.egui_winit_state
            .handle_platform_output(window, platform_output);

        if !textures_delta.free.is_empty() {
            self.textures_to_free = Some(textures_delta.free.clone());
        }

        if !textures_delta.set.is_empty() {
            self.set_textures(
                self.vulkan_context.get_general_queue(),
                command_pool,
                textures_delta.set.as_slice(),
            );
        }

        let clipped_primitives = self.egui_context.tessellate(shapes, pixels_per_point);

        self.pixels_per_point = Some(pixels_per_point);
        self.clipped_primitives = Some(clipped_primitives);
    }

    pub fn record_command_buffer(
        &mut self,
        device: &Device,
        swapchain: &Swapchain,
        command_pool: vk::CommandPool,
        command_buffer: vk::CommandBuffer,
        image_index: u32,
        render_area: Extent2D,
    ) {
        unsafe {
            device
                .reset_command_pool(command_pool, vk::CommandPoolResetFlags::empty())
                .expect("Failed to reset command pool")
        };

        let command_buffer_begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::SIMULTANEOUS_USE);
        unsafe {
            device
                .begin_command_buffer(command_buffer, &command_buffer_begin_info)
                .expect("Failed to begin command buffer")
        };
        swapchain.record_begin_render_pass_cmdbuf(command_buffer, image_index, &render_area);

        Self::cmd_draw(
            device,
            &mut self.frames,
            &self.pipeline,
            &self.pipeline_layout,
            &mut self.textures,
            &mut self.allocator,
            command_buffer,
            render_area,
            self.pixels_per_point.unwrap(),
            &self.clipped_primitives.as_ref().unwrap(),
        );

        unsafe {
            device.cmd_end_render_pass(command_buffer);
            device.end_command_buffer(command_buffer).unwrap()
        };
    }
}

impl Drop for EguiRenderer {
    fn drop(&mut self) {
        let device = &self.vulkan_context.device();
        for (_, t) in self.managed_textures.drain() {
            t.destroy(device, &mut self.allocator);
        }
    }
}

/// Orthographic projection matrix for use with Vulkan.
///
/// This matrix is meant to be used when the source coordinate space is right-handed and y-up
/// (the standard computer graphics coordinate space)and the destination space is right-handed
/// and y-down, with Z (depth) clip extending from 0.0 (close) to 1.0 (far).
///
/// from: https://github.com/fu5ha/ultraviolet (to limit dependencies)
#[inline]
fn orthographic_vk(left: f32, right: f32, bottom: f32, top: f32, near: f32, far: f32) -> [f32; 16] {
    let rml = right - left;
    let rpl = right + left;
    let tmb = top - bottom;
    let tpb = top + bottom;
    let fmn = far - near;

    #[rustfmt::skip]
    let res = [
        2.0 / rml, 0.0, 0.0, 0.0,
        0.0, -2.0 / tmb, 0.0, 0.0,
        0.0, 0.0, -1.0 / fmn, 0.0,
        -(rpl / rml), -(tpb / tmb), -(near / fmn), 1.0
    ];
    res
}

/// Return a `&[u8]` for any sized object passed in.
unsafe fn any_as_u8_slice<T: Sized>(any: &T) -> &[u8] {
    let ptr = (any as *const T) as *const u8;
    std::slice::from_raw_parts(ptr, std::mem::size_of::<T>())
}

fn create_pipeline(
    device: &Device,
    pipeline_layout: &PipelineLayout,
    vert_shader_module: &ShaderModule,
    frag_shader_module: &ShaderModule,
    render_pass: vk::RenderPass,
    options: EguiRendererDesc,
) -> GraphicsPipeline {
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

    let vert_state_info = vert_shader_module.get_shader_stage_create_info();
    let frag_state_info = frag_shader_module
        .get_shader_stage_create_info()
        .specialization_info(&specialization_info);
    let shader_states_infos = [vert_state_info, frag_state_info];

    let mut pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&shader_states_infos)
        .render_pass(render_pass)
        .layout(pipeline_layout.as_raw());

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
        .depth_test_enable(false)
        .depth_write_enable(false)
        .depth_compare_op(vk::CompareOp::ALWAYS)
        .depth_bounds_test_enable(false)
        .stencil_test_enable(false);

    let dynamic_states = [vk::DynamicState::SCISSOR, vk::DynamicState::VIEWPORT];
    let dynamic_states_info =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

    pipeline_info = pipeline_info
        .vertex_input_state(&vertex_input_info)
        .input_assembly_state(&input_assembly_info)
        .rasterization_state(&rasterizer_info)
        .viewport_state(&viewport_info)
        .multisample_state(&multisampling_info)
        .color_blend_state(&color_blending_info)
        .depth_stencil_state(&depth_stencil_state_create_info)
        .dynamic_state(&dynamic_states_info);

    let pipeline = GraphicsPipeline::new(device, pipeline_info);
    pipeline
}

/// Create a descriptor set compatible with the graphics pipeline from a texture.
fn create_vulkan_descriptor_set(
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
