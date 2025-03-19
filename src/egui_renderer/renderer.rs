use super::mesh::Mesh;
use crate::util::compiler::ShaderCompiler;
use crate::vkn::CommandBuffer;
use crate::vkn::CommandPool;
use crate::vkn::DescriptorSet;
use crate::vkn::Queue;
use crate::vkn::TextureDesc;
use crate::vkn::TextureUploadRegion;
use crate::vkn::VulkanContext;
use crate::vkn::WriteDescriptorSet;
use crate::vkn::{
    Allocator, DescriptorPool, DescriptorSetLayout, DescriptorSetLayoutBinding,
    DescriptorSetLayoutBuilder, Device, GraphicsPipeline, PipelineLayout, ShaderModule, Texture,
};
use ash::vk;
use ash::vk::Extent2D;
use egui::ViewportId;
use egui::{
    epaint::{ImageDelta, Primitive},
    ClippedPrimitive, ImageData, TextureId,
};
use egui_winit::EventResponse;
use glam::Mat4;
use std::{collections::HashMap, mem};
use winit::event::WindowEvent;
use winit::window::Window;

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
    gui_pipeline: GraphicsPipeline,
    pipeline_layout: PipelineLayout,
    vert_shader_module: ShaderModule,
    frag_shader_module: ShaderModule,

    descriptor_set_layout: DescriptorSetLayout,
    descriptor_pool: DescriptorPool,
    managed_textures: HashMap<TextureId, Texture>,
    textures: HashMap<TextureId, DescriptorSet>,
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
        allocator: &Allocator,
        compiler: &ShaderCompiler,
        render_pass: vk::RenderPass,
        desc: EguiRendererDesc,
    ) -> Self {
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

        let pipeline = create_gui_pipeline(
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
            allocator: allocator.clone(),
            gui_pipeline: pipeline,
            pipeline_layout,
            vert_shader_module,
            frag_shader_module,
            descriptor_set_layout,
            descriptor_pool,
            managed_textures: HashMap::new(),
            textures: HashMap::new(),
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
        self.gui_pipeline = create_gui_pipeline(
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
        queue: &Queue,
        command_pool: &CommandPool,
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

            let device = self.vulkan_context.device();

            if let Some([offset_x, offset_y]) = delta.pos {
                let texture = self.managed_textures.get_mut(id).unwrap();

                texture
                    .upload_rgba_image(
                        queue,
                        &command_pool,
                        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                        TextureUploadRegion {
                            offset: [offset_x as _, offset_y as _],
                            extent: [width, height],
                        },
                        data.as_slice(),
                    )
                    .unwrap();
            } else {
                let tex_desc = TextureDesc {
                    extent: [width, height, 1],
                    format: vk::Format::R8G8B8A8_SRGB,
                    usage: vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
                    initial_layout: vk::ImageLayout::UNDEFINED,
                    aspect: vk::ImageAspectFlags::COLOR,
                    ..Default::default()
                };
                let sam_desc = Default::default();

                let mut texture =
                    Texture::new(device.clone(), self.allocator.clone(), &tex_desc, &sam_desc);

                texture
                    .upload_rgba_image(
                        queue,
                        &command_pool,
                        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                        TextureUploadRegion {
                            offset: [0, 0],
                            extent: [width, height],
                        },
                        data.as_slice(),
                    )
                    .unwrap();

                let set = DescriptorSet::new(
                    device.clone(),
                    std::slice::from_ref(&self.descriptor_set_layout),
                    self.descriptor_pool.clone(),
                );

                set.perform_writes(&[WriteDescriptorSet::new_texture_write(
                    0,
                    vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    &texture,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                )]);

                self.managed_textures.insert(*id, texture);
                self.textures.insert(*id, set);
            }
        }
    }

    /// Record commands to render the [`egui::Ui`].
    fn cmd_draw(
        device: &Device,
        frames: &mut Option<Mesh>,
        pipeline: &GraphicsPipeline,
        pipeline_layout: &PipelineLayout,
        textures: &mut HashMap<TextureId, DescriptorSet>,
        allocator: &mut Allocator,
        cmdbuf: &CommandBuffer,
        extent: vk::Extent2D,
        pixels_per_point: f32,
        primitives: &[ClippedPrimitive],
    ) {
        if primitives.is_empty() {
            return;
        }

        if frames.is_none() {
            frames.replace(Mesh::new(device, allocator, primitives));
        }

        frames
            .as_mut()
            .unwrap()
            .update(device, allocator, primitives);

        unsafe {
            device.cmd_bind_pipeline(
                cmdbuf.as_raw(),
                vk::PipelineBindPoint::GRAPHICS,
                pipeline.as_raw(),
            )
        };

        let screen_width = extent.width as f32;
        let screen_height = extent.height as f32;

        unsafe {
            device.cmd_set_viewport(
                cmdbuf.as_raw(),
                0,
                &[vk::Viewport {
                    width: screen_width,
                    height: screen_height,
                    max_depth: 1.0,
                    ..Default::default()
                }],
            )
        };

        let projection = Mat4::orthographic_rh(
            0.0,
            screen_width / pixels_per_point,
            0.0,
            screen_height / pixels_per_point,
            -1.0,
            1.0,
        )
        .to_cols_array();

        unsafe {
            let push = any_as_u8_slice(&projection);
            device.cmd_push_constants(
                cmdbuf.as_raw(),
                pipeline_layout.as_raw(),
                vk::ShaderStageFlags::VERTEX,
                0,
                push,
            )
        };

        unsafe {
            device.cmd_bind_index_buffer(
                cmdbuf.as_raw(),
                frames.as_mut().unwrap().indices_buffer.as_raw(),
                0,
                vk::IndexType::UINT32,
            )
        };

        unsafe {
            device.cmd_bind_vertex_buffers(
                cmdbuf.as_raw(),
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
                        device.cmd_set_scissor(cmdbuf.as_raw(), 0, &scissors);
                    }

                    if Some(m.texture_id) != current_texture_id {
                        let descriptor_set = textures.get(&m.texture_id).unwrap().as_raw();

                        unsafe {
                            device.cmd_bind_descriptor_sets(
                                cmdbuf.as_raw(),
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
                            cmdbuf.as_raw(),
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
        command_pool: &CommandPool,
        window: &Window,
        run_ui: impl FnMut(&egui::Context),
    ) {
        let raw_input = self.egui_winit_state.take_egui_input(window);

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
                &self.vulkan_context.get_general_queue(),
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
        cmdbuf: &CommandBuffer,
        render_area: Extent2D,
    ) {
        Self::cmd_draw(
            device,
            &mut self.frames,
            &self.gui_pipeline,
            &self.pipeline_layout,
            &mut self.textures,
            &mut self.allocator,
            cmdbuf,
            render_area,
            self.pixels_per_point.unwrap(),
            &self.clipped_primitives.as_ref().unwrap(),
        );
    }
}

/// Return a `&[u8]` for any sized object passed in.
unsafe fn any_as_u8_slice<T: Sized>(any: &T) -> &[u8] {
    let ptr = (any as *const T) as *const u8;
    std::slice::from_raw_parts(ptr, std::mem::size_of::<T>())
}

fn create_gui_pipeline(
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
