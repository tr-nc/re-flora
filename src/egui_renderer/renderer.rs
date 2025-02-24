use super::frames::Frames;
use super::{allocator::Allocator, texture::Texture};
use ash::vk::Extent2D;
use ash::{vk, Device};
use egui::ViewportId;
use egui::{
    epaint::{ImageDelta, Primitive},
    ClippedPrimitive, ImageData, TextureId,
};
use egui_winit::EventResponse;
use gpu_allocator::vulkan::AllocatorCreateDesc;
use std::{collections::HashMap, ffi::CString, mem};
use winit::event::WindowEvent;
use winit::window::Window;

use crate::shader_util::{load_from_glsl, load_from_spv, ShaderCompiler};
use crate::vkn::context::VulkanContext;
use crate::vkn::swapchain::Swapchain;

use std::sync::{Arc, Mutex};

const MAX_TEXTURE_COUNT: u32 = 1024;

/// Optional parameters of the renderer.
#[derive(Debug, Clone, Copy)]
pub struct EguiRendererDesc {
    /// The number of in flight frames of the application.
    pub in_flight_frames: usize,

    /// If true enables depth test when rendering.
    pub enable_depth_test: bool,

    /// If true enables depth writes when rendering.
    ///
    /// Note that depth writes are always disabled when enable_depth_test is false.
    /// See <https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/VkPipelineDepthStencilStateCreateInfo.html>
    pub enable_depth_write: bool,

    /// Is the target framebuffer sRGB.
    ///
    /// If not, the fragment shader converts colors to sRGB, otherwise it outputs color in linear space.
    pub srgb_framebuffer: bool,
}

impl Default for EguiRendererDesc {
    fn default() -> Self {
        Self {
            in_flight_frames: 1,
            enable_depth_test: false,
            enable_depth_write: false,
            srgb_framebuffer: false,
        }
    }
}

impl EguiRendererDesc {
    fn validate(&self) -> Result<(), String> {
        if self.in_flight_frames <= 0 {
            return Err("in_flight_frames should be at least one".to_string());
        }
        Ok(())
    }
}

/// Winit-Egui Renderer implemented for Ash Vulkan.
pub struct EguiRenderer {
    vulkan_context: Arc<VulkanContext>,
    allocator: Allocator,
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    managed_textures: HashMap<TextureId, Texture>,
    textures: HashMap<TextureId, vk::DescriptorSet>,
    frames: Option<Frames>,

    textures_to_free: Option<Vec<TextureId>>,

    // these shader modules are cached to avoid recompiling them during window scaling
    vert_module: vk::ShaderModule,
    frag_module: vk::ShaderModule,

    egui_context: egui::Context,
    egui_winit_state: egui_winit::State,

    desc: EguiRendererDesc,
}

impl EguiRenderer {
    /// Create a renderer using gpu-allocator.
    ///
    /// At initialization all Vulkan resources are initialized. Vertex and index buffers are not created yet.
    pub fn new(
        vulkan_context: &Arc<VulkanContext>,
        window: &Window,
        compiler: &ShaderCompiler,
        render_pass: vk::RenderPass,
        desc: EguiRendererDesc,
    ) -> Self {
        desc.validate().expect("Invalid options");

        let gpu_allocator = {
            let allocator_create_info = AllocatorCreateDesc {
                instance: vulkan_context.instance.clone(),
                device: vulkan_context.device.clone(),
                physical_device: vulkan_context.physical_device,
                debug_settings: Default::default(),
                buffer_device_address: false,
                allocation_sizes: Default::default(),
            };
            gpu_allocator::vulkan::Allocator::new(&allocator_create_info)
                .expect("Failed to create gpu allocator")
        };
        let allocator = Allocator::new(Arc::new(Mutex::new(gpu_allocator)));

        let device = &vulkan_context.device;

        let vertex_shader_loaded = load_from_glsl(
            "src/egui_renderer/shaders/shader.vert",
            device.clone(),
            &compiler,
        )
        .unwrap();
        let fragment_shader_loaded = load_from_glsl(
            "src/egui_renderer/shaders/shader.frag",
            device.clone(),
            &compiler,
        )
        .unwrap();

        let descriptor_set_layout = create_descriptor_set_layout(device);
        let pipeline_layout = create_pipeline_layout(device, descriptor_set_layout);
        let pipeline = create_pipeline(
            device,
            vertex_shader_loaded.shader_module,
            fragment_shader_loaded.shader_module,
            pipeline_layout,
            render_pass,
            desc,
        );

        // Descriptor pool
        let descriptor_pool = create_descriptor_pool(device, MAX_TEXTURE_COUNT);

        // Textures
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
            descriptor_set_layout,
            descriptor_pool,
            managed_textures,
            textures,
            desc,
            frames: None,
            textures_to_free: None,
            vert_module: vertex_shader_loaded.shader_module,
            frag_module: fragment_shader_loaded.shader_module,

            egui_context,
            egui_winit_state,
        }
    }

    pub fn on_window_event(&mut self, window: &Window, event: &WindowEvent) -> EventResponse {
        self.egui_winit_state.on_window_event(window, event)
    }

    /// Set the render pass used by the renderer, by recreating the pipeline.
    ///
    /// This is an expensive operation.
    pub fn set_render_pass(&mut self, render_pass: vk::RenderPass) {
        unsafe {
            self.vulkan_context
                .device
                .destroy_pipeline(self.pipeline, None)
        };
        self.pipeline = create_pipeline(
            &self.vulkan_context.device,
            self.vert_module,
            self.frag_module,
            self.pipeline_layout,
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

            let device = &self.vulkan_context.device;

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
                    self.descriptor_set_layout,
                    self.descriptor_pool,
                    texture.image_view,
                    texture.sampler,
                );

                if let Some(previous) = self.managed_textures.insert(*id, texture) {
                    previous.destroy(device, &mut self.allocator);
                }
                if let Some(previous) = self.textures.insert(*id, set) {
                    unsafe {
                        device
                            .free_descriptor_sets(self.descriptor_pool, &[previous])
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
        let device = &self.vulkan_context.device;

        for id in ids {
            if let Some(texture) = self.managed_textures.remove(id) {
                texture.destroy(device, &mut self.allocator);
            }
            if let Some(set) = self.textures.remove(id) {
                unsafe {
                    device
                        .free_descriptor_sets(self.descriptor_pool, &[set])
                        .unwrap();
                };
            }
        }
    }

    /// Record commands to render the [`egui::Ui`].
    ///
    /// # Arguments
    ///
    /// * `command_buffer` - The Vulkan command buffer that command will be recorded to.
    /// * `extent` - The extent of the surface to render to.
    /// * `pixel_per_point` - The number of physical pixels per point. See [`egui::FullOutput::pixels_per_point`].
    /// * `primitives` - The primitives to render. See [`egui::Context::tessellate`].
    fn cmd_draw(
        &mut self,
        command_buffer: vk::CommandBuffer,
        extent: vk::Extent2D,
        pixels_per_point: f32,
        primitives: &[ClippedPrimitive],
    ) {
        if primitives.is_empty() {
            return;
        }

        let device = &self.vulkan_context.device;

        if self.frames.is_none() {
            self.frames.replace(Frames::new(
                device,
                &mut self.allocator,
                primitives,
                self.desc.in_flight_frames,
            ));
        }

        let mesh = self.frames.as_mut().unwrap().next();
        mesh.update(device, &mut self.allocator, primitives);

        unsafe {
            device.cmd_bind_pipeline(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline,
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
                self.pipeline_layout,
                vk::ShaderStageFlags::VERTEX,
                0,
                push,
            )
        };

        unsafe {
            device.cmd_bind_index_buffer(command_buffer, mesh.indices, 0, vk::IndexType::UINT32)
        };

        unsafe { device.cmd_bind_vertex_buffers(command_buffer, 0, &[mesh.vertices], &[0]) };

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
                        let descriptor_set = *self.textures.get(&m.texture_id).unwrap();

                        unsafe {
                            device.cmd_bind_descriptor_sets(
                                command_buffer,
                                vk::PipelineBindPoint::GRAPHICS,
                                self.pipeline_layout,
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
        window: &Window,
        run_ui: impl FnMut(&egui::Context),
    ) -> (f32, Vec<ClippedPrimitive>) {
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
                self.vulkan_context.command_pool,
                textures_delta.set.as_slice(),
            );
        }

        let clipped_primitives = self.egui_context.tessellate(shapes, pixels_per_point);

        (pixels_per_point, clipped_primitives)
    }

    pub fn record_command_buffer(
        &mut self,
        device: &Device,
        swapchain: &Swapchain,
        command_pool: vk::CommandPool,
        command_buffer: vk::CommandBuffer,
        image_index: u32,
        render_area: Extent2D,
        pixels_per_point: f32,
        clipped_primitives: &[ClippedPrimitive],
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

        self.cmd_draw(
            command_buffer,
            render_area,
            pixels_per_point,
            clipped_primitives,
        );

        unsafe { device.cmd_end_render_pass(command_buffer) };
        unsafe { device.end_command_buffer(command_buffer).unwrap() };
    }
}

impl Drop for EguiRenderer {
    fn drop(&mut self) {
        let device = &self.vulkan_context.device;
        unsafe {
            if let Some(frames) = self.frames.take() {
                frames.destroy(device, &mut self.allocator);
            }
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_pipeline_layout(self.pipeline_layout, None);
            device.destroy_descriptor_pool(self.descriptor_pool, None);

            for (_, t) in self.managed_textures.drain() {
                t.destroy(device, &mut self.allocator);
            }
            device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            device.destroy_shader_module(self.vert_module, None);
            device.destroy_shader_module(self.frag_module, None);
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

/// Create a descriptor set layout compatible with the graphics pipeline.
fn create_descriptor_set_layout(device: &Device) -> vk::DescriptorSetLayout {
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

fn create_pipeline_layout(
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

fn create_pipeline(
    device: &Device,
    vert_module: vk::ShaderModule,
    frag_module: vk::ShaderModule,
    pipeline_layout: vk::PipelineLayout,
    render_pass: vk::RenderPass,
    options: EguiRendererDesc,
) -> vk::Pipeline {
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

    let entry_point_name = CString::new("main").unwrap();
    let shader_states_infos = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert_module)
            .name(&entry_point_name),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag_module)
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

    pipeline
}

/// Create a descriptor pool of sets compatible with the graphics pipeline.
fn create_descriptor_pool(device: &Device, max_sets: u32) -> vk::DescriptorPool {
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
