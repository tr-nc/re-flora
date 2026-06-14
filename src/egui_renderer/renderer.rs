use super::mesh::Mesh;
use crate::util::ShaderCompiler;
use egui::ViewportId;
use egui::{
    epaint::{ImageDelta, Primitive},
    ClippedPrimitive, ImageData, TextureId,
};
use egui_winit::EventResponse;
use glam::Mat4;
use std::collections::HashMap;
use verdarium_vkn::vk;
use verdarium_vkn::CommandBuffer;
use verdarium_vkn::FormatOverride;
use verdarium_vkn::ImageDesc;
use verdarium_vkn::RenderPass;
use verdarium_vkn::TextureLayout;
use verdarium_vkn::TextureRegion;
use verdarium_vkn::VulkanContext;
use verdarium_vkn::WriteDescriptorSet;
use verdarium_vkn::{
    Allocator, DescriptorPool, DescriptorSet, Device, Extent2D, Extent3D, GraphicsPipeline,
    GraphicsPipelineDesc, ShaderModule, Texture,
};
use winit::event::WindowEvent;
use winit::window::Window;

/// Winit-Egui Renderer implemented for Ash Vulkan.
pub struct EguiRenderer {
    vulkan_context: VulkanContext,
    allocator: Allocator,
    gui_ppl: GraphicsPipeline,
    egui_vert_sm: ShaderModule,
    egui_frag_sm: ShaderModule,

    pool: DescriptorPool,
    managed_textures: HashMap<TextureId, Texture>,
    managed_texture_descriptor_sets: HashMap<TextureId, DescriptorSet>,
    frames: Option<Mesh>,

    egui_context: egui::Context,
    egui_winit_state: egui_winit::State,

    pixels_per_point: Option<f32>,
    clipped_primitives: Option<Vec<ClippedPrimitive>>,
}

impl EguiRenderer {
    pub fn new(
        vulkan_ctx: VulkanContext,
        window: &Window,
        allocator: Allocator,
        compiler: &ShaderCompiler,
        render_pass: &RenderPass,
    ) -> Self {
        let device = vulkan_ctx.device();

        let egui_vert_sm =
            ShaderModule::from_glsl(device, compiler, "shader/egui/egui.vert", "main").unwrap();
        let egui_frag_sm =
            ShaderModule::from_glsl(device, compiler, "shader/egui/egui.frag", "main").unwrap();

        let pool = DescriptorPool::new(vulkan_ctx.device()).unwrap();

        let gui_ppl = GraphicsPipeline::new(
            device,
            &egui_vert_sm,
            &egui_frag_sm,
            render_pass,
            &GraphicsPipelineDesc {
                format_overrides: vec![FormatOverride {
                    location: 2,
                    format: vk::Format::R8G8B8A8_UNORM,
                }],
                ..Default::default()
            },
            None,
            &pool,
            &[],
        );

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
            vulkan_context: vulkan_ctx,
            allocator,
            gui_ppl,
            egui_vert_sm,
            egui_frag_sm,
            pool,
            managed_textures: HashMap::new(),
            managed_texture_descriptor_sets: HashMap::new(),
            frames: None,

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
    pub fn set_render_pass(&mut self, render_pass: &RenderPass) {
        self.gui_ppl = GraphicsPipeline::new(
            self.vulkan_context.device(),
            &self.egui_vert_sm,
            &self.egui_frag_sm,
            render_pass,
            &GraphicsPipelineDesc {
                format_overrides: vec![FormatOverride {
                    location: 2,
                    format: vk::Format::R8G8B8A8_UNORM,
                }],
                ..Default::default()
            },
            None,
            &self.pool,
            &[],
        );
    }

    /// Get a reference to the underlying egui context so the caller can configure global settings.
    pub fn context(&self) -> &egui::Context {
        &self.egui_context
    }

    /// Free egui managed textures.
    ///
    /// You should pass the list of textures detla contained in the [`egui::TexturesDelta::set`].
    /// This method should be called _before_ the frame starts rendering.
    fn set_textures(&mut self, textures_delta: &[(TextureId, ImageDelta)]) {
        for (id, delta) in textures_delta {
            let (width, height, data) = match &delta.image {
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
            let extent = Extent3D::new(width, height, 1);
            if let Some([offset_x, offset_y]) = delta.pos {
                let texture = self.managed_textures.get_mut(id).unwrap();

                let region = TextureRegion {
                    offset: [offset_x as _, offset_y as _, 0],
                    extent,
                };

                texture
                    .get_image()
                    .fill_with_raw_u8(
                        &self.vulkan_context.get_general_queue(),
                        self.vulkan_context.command_pool(),
                        region,
                        data.as_slice(),
                        0,
                        Some(TextureLayout::SHADER_READ_ONLY),
                    )
                    .unwrap();
            } else {
                let tex_desc = ImageDesc {
                    extent,
                    format: vk::Format::B8G8R8A8_SRGB,
                    usage: vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
                    initial_layout: TextureLayout::UNDEFINED,
                    aspect: vk::ImageAspectFlags::COLOR,
                    ..Default::default()
                };
                let sam_desc = Default::default();

                let texture =
                    Texture::new(device.clone(), self.allocator.clone(), &tex_desc, &sam_desc);

                texture
                    .get_image()
                    .fill_with_raw_u8(
                        &self.vulkan_context.get_general_queue(),
                        self.vulkan_context.command_pool(),
                        TextureRegion::from_image(texture.get_image()),
                        data.as_slice(),
                        0,
                        Some(TextureLayout::SHADER_READ_ONLY),
                    )
                    .unwrap();

                self.gui_ppl.write_descriptor_set(
                    0,
                    WriteDescriptorSet::new_texture_write(
                        0,
                        vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                        &texture,
                        TextureLayout::SHADER_READ_ONLY,
                    ),
                );

                let descriptor_layout = self
                    .gui_ppl
                    .get_layout()
                    .get_descriptor_set_layouts()
                    .get(&0)
                    .expect("Egui pipeline is expected to expose descriptor set 0");
                let descriptor_set = self
                    .pool
                    .allocate_set(descriptor_layout)
                    .expect("Failed to allocate egui texture descriptor set");
                descriptor_set.perform_writes(&mut [WriteDescriptorSet::new_texture_write(
                    0,
                    vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    &texture,
                    TextureLayout::SHADER_READ_ONLY,
                )]);

                self.managed_textures.insert(*id, texture);
                self.managed_texture_descriptor_sets
                    .insert(*id, descriptor_set);
            }
        }
    }

    fn free_textures(&mut self, texture_ids: &[TextureId]) {
        for texture_id in texture_ids {
            self.managed_textures.remove(texture_id);
            self.managed_texture_descriptor_sets.remove(texture_id);
        }
    }

    /// Record commands to render the [`egui::Ui`].
    #[allow(clippy::too_many_arguments)]
    fn cmd_draw(
        device: &Device,
        frames: &mut Option<Mesh>,
        pipeline: &GraphicsPipeline,
        managed_texture_descriptor_sets: &HashMap<TextureId, DescriptorSet>,
        allocator: &mut Allocator,
        cmdbuf: &CommandBuffer,
        extent: Extent2D,
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

        cmdbuf.bind_graphics_pipeline(pipeline);

        let screen_width = extent.width as f32;
        let screen_height = extent.height as f32;

        cmdbuf.set_viewport_from_extent(extent);

        let projection = Mat4::orthographic_rh(
            0.0,
            screen_width / pixels_per_point,
            0.0,
            screen_height / pixels_per_point,
            -1.0,
            1.0,
        )
        .to_cols_array();

        let push = bytemuck::bytes_of(&projection);
        cmdbuf.push_vertex_constants(pipeline, push);

        let frame = frames.as_ref().unwrap();
        cmdbuf.bind_index_buffer_u32(&frame.indices_buffer);
        cmdbuf.bind_vertex_buffers(0, &[&frame.vertices_buffer]);

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

                    cmdbuf.set_scissor(
                        [(clip_x as i32).max(0), (clip_y as i32).max(0)],
                        Extent2D {
                            width: clip_w.min(screen_width) as _,
                            height: clip_h.min(screen_height) as _,
                        },
                    );

                    if Some(m.texture_id) != current_texture_id {
                        let descriptor_set =
                            managed_texture_descriptor_sets.get(&m.texture_id).unwrap();
                        cmdbuf.bind_graphics_descriptor_set(pipeline, 0, descriptor_set);
                        current_texture_id = Some(m.texture_id);
                    }

                    let index_count = m.indices.len() as u32;

                    cmdbuf.draw_indexed(index_count, 1, index_offset, vertex_offset, 0);

                    index_offset += index_count;
                    vertex_offset += m.vertices.len() as i32;
                }
                Primitive::Callback(_) => {
                    log::warn!("Callback primitives not yet supported")
                }
            }
        }
    }

    pub fn update(&mut self, window: &Window, run_ui: impl FnMut(&egui::Context)) {
        let raw_input = self.egui_winit_state.take_egui_input(window);

        #[allow(deprecated)]
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
            self.free_textures(&textures_delta.free);
        }

        if !textures_delta.set.is_empty() {
            self.set_textures(textures_delta.set.as_slice());
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
            &self.gui_ppl,
            &self.managed_texture_descriptor_sets,
            &mut self.allocator,
            cmdbuf,
            render_area,
            self.pixels_per_point.unwrap(),
            self.clipped_primitives.as_ref().unwrap(),
        );
    }
}
