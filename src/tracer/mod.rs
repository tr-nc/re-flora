mod resources;
use glam::{Vec2, Vec3};
pub use resources::*;

mod vertex;
pub use vertex::*;
use winit::event::KeyEvent;

mod grass_construct;

use crate::builder::SurfaceResources;
use crate::gameplay::{Camera, CameraDesc};
use crate::util::ShaderCompiler;
use crate::vkn::{
    Allocator, Buffer, ComputePipeline, DescriptorPool, DescriptorSet, Framebuffer,
    GraphicsPipeline, GraphicsPipelineDesc, Image, PlainMemberTypeWithData, RenderPass,
    ShaderModule, StructMemberDataBuilder, Texture, WriteDescriptorSet,
};
use crate::vkn::{CommandBuffer, VulkanContext};
use ash::vk;

pub struct Tracer {
    vulkan_ctx: VulkanContext,

    allocator: Allocator,
    resources: TracerResources,

    camera: Camera,

    tracer_ppl: ComputePipeline,
    tracer_sets: [DescriptorSet; 2],
    graphics_sets: [DescriptorSet; 1],
    gfx_ppl: GraphicsPipeline,
    gfx_render_pass: RenderPass,
    gfx_framebuffers: Vec<Framebuffer>,

    #[allow(dead_code)]
    descriptor_pool_ds_0: DescriptorPool,
    descriptor_pool_ds_1: DescriptorPool,

    frame_serial_idx: u32,
}

impl Drop for Tracer {
    fn drop(&mut self) {}
}

impl Tracer {
    pub fn new(
        vulkan_ctx: VulkanContext,
        allocator: Allocator,
        shader_compiler: &ShaderCompiler,
        screen_extent: &[u32; 2],
        node_data: &Buffer,
        leaf_data: &Buffer,
        scene_tex: &Texture,
        swapchain_image_views: &[vk::ImageView],
    ) -> Self {
        let camera = Camera::new(
            Vec3::new(0.5, 1.2, 0.5),
            135.0,
            -5.0,
            CameraDesc {
                movement: Default::default(),
                projection: Default::default(),
                aspect_ratio: screen_extent[0] as f32 / screen_extent[1] as f32,
            },
        );

        let tracer_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/tracer/tracer.comp",
            "main",
        )
        .unwrap();
        let tracer_ppl = ComputePipeline::new(vulkan_ctx.device(), &tracer_sm);

        let descriptor_pool_ds_0 = DescriptorPool::from_descriptor_set_layouts(
            vulkan_ctx.device(),
            tracer_ppl.get_layout().get_descriptor_set_layouts(),
        )
        .unwrap();
        let descriptor_pool_ds_1 = DescriptorPool::from_descriptor_set_layouts(
            vulkan_ctx.device(),
            tracer_ppl.get_layout().get_descriptor_set_layouts(),
        )
        .unwrap();

        let vert_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/foliage/foliage.vert",
            "main",
        )
        .unwrap();
        let frag_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/foliage/foliage.frag",
            "main",
        )
        .unwrap();

        let resources = TracerResources::new(
            &vulkan_ctx,
            allocator.clone(),
            &vert_sm,
            &tracer_sm,
            screen_extent,
        );

        let (gfx_ppl, gfx_render_pass) = Self::create_render_pass_and_graphics_pipeline(
            &vulkan_ctx,
            &vert_sm,
            &frag_sm,
            resources.depth_tex.clone(),
            resources.shader_write_tex.clone(),
        );

        let gfx_framebuffers = Self::create_framebuffers(
            &vulkan_ctx,
            &gfx_render_pass,
            &resources.depth_tex,
            &resources.shader_write_tex,
            swapchain_image_views,
        );

        let tracer_set_0 = Self::create_descriptor_set_0(
            descriptor_pool_ds_0.clone(),
            &vulkan_ctx,
            &tracer_ppl,
            &resources,
            node_data,
            leaf_data,
            scene_tex,
        );

        let tracer_set_1 = Self::create_descriptor_set_1(
            descriptor_pool_ds_1.clone(),
            &vulkan_ctx,
            &tracer_ppl,
            &resources,
        );

        let graphics_set_0 = Self::create_graphics_set_0(
            descriptor_pool_ds_0.clone(),
            &vulkan_ctx,
            &gfx_ppl,
            &resources,
        );

        return Self {
            vulkan_ctx,
            allocator,
            resources,
            camera,
            tracer_ppl,
            tracer_sets: [tracer_set_0, tracer_set_1],
            graphics_sets: [graphics_set_0],
            gfx_ppl,
            gfx_render_pass,
            gfx_framebuffers,
            descriptor_pool_ds_0,
            descriptor_pool_ds_1,
            frame_serial_idx: 0,
        };
    }

    fn create_graphics_set_0(
        descriptor_pool: DescriptorPool,
        vulkan_ctx: &VulkanContext,
        gfx_ppl: &GraphicsPipeline,
        resources: &TracerResources,
    ) -> DescriptorSet {
        let ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &gfx_ppl.get_layout().get_descriptor_set_layouts()[&0],
            descriptor_pool,
        );
        ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.camera_info),
            WriteDescriptorSet::new_buffer_write(1, &resources.grass_info),
        ]);
        ds
    }

    fn create_render_pass_and_graphics_pipeline(
        vulkan_ctx: &VulkanContext,
        vert_sm: &ShaderModule,
        frag_sm: &ShaderModule,
        depth_tex: Texture,
        shader_write_tex: Texture,
    ) -> (GraphicsPipeline, RenderPass) {
        let render_pass = {
            RenderPass::with_attachments(
                vulkan_ctx.device().clone(),
                shader_write_tex,
                Some(depth_tex),
                vk::AttachmentLoadOp::CLEAR,
                vk::ImageLayout::GENERAL,
                Some(vk::ImageLayout::GENERAL),
            )
        };

        let gfx_ppl = GraphicsPipeline::new(
            vulkan_ctx.device(),
            &vert_sm,
            &frag_sm,
            &render_pass,
            &GraphicsPipelineDesc {
                cull_mode: vk::CullModeFlags::BACK,
                depth_test_enable: true,
                depth_write_enable: true,
                ..Default::default()
            },
            Some(3),
        );

        (gfx_ppl, render_pass)
    }

    fn create_framebuffers(
        vulkan_ctx: &VulkanContext,
        render_pass: &RenderPass,
        depth_texture: &Texture,
        target_texture: &Texture,
        swapchain_image_views: &[vk::ImageView],
    ) -> Vec<Framebuffer> {
        let depth_image_view = depth_texture.get_image_view().as_raw();
        let target_view = target_texture.get_image_view().as_raw();

        let target_image_extent = {
            let ext = target_texture.get_image().get_desc().extent;
            vk::Extent2D {
                width: ext[0],
                height: ext[1],
            }
        };

        return swapchain_image_views
            .iter()
            .map(|_| {
                Framebuffer::new(
                    vulkan_ctx.clone(),
                    &render_pass,
                    &[target_view, depth_image_view],
                    target_image_extent,
                )
                .unwrap()
            })
            .collect();
    }

    fn create_descriptor_set_0(
        descriptor_pool: DescriptorPool,
        vulkan_ctx: &VulkanContext,
        tracer_ppl: &ComputePipeline,
        resources: &TracerResources,
        node_data: &Buffer,
        leaf_data: &Buffer,
        scene_tex: &Texture,
    ) -> DescriptorSet {
        let ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &tracer_ppl
                .get_layout()
                .get_descriptor_set_layouts()
                .get(&0)
                .unwrap(),
            descriptor_pool,
        );
        ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.gui_input),
            WriteDescriptorSet::new_buffer_write(1, &resources.camera_info),
            WriteDescriptorSet::new_buffer_write(2, &resources.env_info),
            WriteDescriptorSet::new_buffer_write(3, &node_data),
            WriteDescriptorSet::new_buffer_write(4, &leaf_data),
            WriteDescriptorSet::new_texture_write(
                5,
                vk::DescriptorType::STORAGE_IMAGE,
                &scene_tex,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                6,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.scalar_bn,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                7,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.unit_vec2_bn,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                8,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.unit_vec3_bn,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                9,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.weighted_cosine_bn,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                10,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.fast_unit_vec3_bn,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                11,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.fast_weighted_cosine_bn,
                vk::ImageLayout::GENERAL,
            ),
        ]);
        ds
    }

    fn create_descriptor_set_1(
        descriptor_pool: DescriptorPool,
        vulkan_ctx: &VulkanContext,
        tracer_ppl: &ComputePipeline,
        resources: &TracerResources,
    ) -> DescriptorSet {
        let ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &tracer_ppl.get_layout().get_descriptor_set_layouts()[&1],
            descriptor_pool,
        );
        ds.perform_writes(&mut [
            WriteDescriptorSet::new_texture_write(
                0,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.shader_write_tex,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                1,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.depth_tex,
                vk::ImageLayout::GENERAL,
            ),
        ]);
        ds
    }

    pub fn on_resize(&mut self, screen_extent: &[u32; 2], swapchain_image_views: &[vk::ImageView]) {
        self.camera.on_resize(screen_extent);

        self.resources.on_resize(
            self.vulkan_ctx.device().clone(),
            self.allocator.clone(),
            &screen_extent,
        );

        self.gfx_framebuffers = Self::create_framebuffers(
            &self.vulkan_ctx,
            &self.gfx_render_pass,
            &self.resources.depth_tex,
            &self.resources.shader_write_tex,
            swapchain_image_views,
        );

        self.descriptor_pool_ds_1.reset().unwrap();
        self.tracer_sets[1] = Self::create_descriptor_set_1(
            self.descriptor_pool_ds_1.clone(),
            &self.vulkan_ctx,
            &self.tracer_ppl,
            &self.resources,
        );
    }

    pub fn record_command_buffer(
        &mut self,
        cmdbuf: &CommandBuffer,
        image_index: usize,
        surface_resources: &SurfaceResources,
        grass_instances_len: u32,
    ) {
        self.record_screen_space_pass(cmdbuf, image_index, surface_resources, grass_instances_len);
        self.record_trace_pass(cmdbuf);
    }

    pub fn get_dst_image(&self) -> &Image {
        self.resources.shader_write_tex.get_image()
    }

    pub fn record_trace_pass(&self, cmdbuf: &CommandBuffer) {
        let screen_extent = self.get_dst_image().get_desc().extent;

        self.get_dst_image()
            .record_transition_barrier(cmdbuf, 0, vk::ImageLayout::GENERAL);
        self.resources
            .depth_tex
            .get_image()
            .record_transition_barrier(cmdbuf, 0, vk::ImageLayout::GENERAL);

        self.tracer_ppl.record_bind(cmdbuf);

        self.tracer_ppl
            .record_bind_descriptor_sets(cmdbuf, &self.tracer_sets, 0);
        self.tracer_ppl
            .record_dispatch(cmdbuf, [screen_extent[0], screen_extent[1], 1]);
    }

    pub fn record_screen_space_pass(
        &self,
        cmdbuf: &CommandBuffer,
        image_index: usize,
        surface_resources: &SurfaceResources,
        grass_instances_len: u32,
    ) {
        self.gfx_ppl.record_bind(cmdbuf);

        // When beginning the render pass, you must also provide a clear value for the depth buffer.
        let clear_values = [
            vk::ClearValue {
                // Color
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 1.0],
                },
            },
            vk::ClearValue {
                // Depth
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
            },
        ];

        self.gfx_render_pass.record_begin(
            cmdbuf,
            &self.gfx_framebuffers[image_index],
            &clear_values,
        );

        let image_extent = self.get_dst_image().get_desc().extent;

        let viewport = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: image_extent[0] as f32,
            height: image_extent[1] as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };
        let scissor = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: vk::Extent2D {
                width: image_extent[0],
                height: image_extent[1],
            },
        };

        // must be done before record draw, can be swapped with record_viewport_scissor
        self.gfx_ppl
            .record_bind_descriptor_sets(cmdbuf, &self.graphics_sets, 0);

        // TODO: wrap them!
        unsafe {
            // Bind the vertex buffer to binding point 0
            self.vulkan_ctx.device().cmd_bind_vertex_buffers(
                cmdbuf.as_raw(),
                0,
                &[
                    self.resources.vertices.as_raw(),
                    surface_resources.grass_instances.as_raw(),
                ],
                &[0, 0],
            );
            // Bind the index buffer
            self.vulkan_ctx.device().cmd_bind_index_buffer(
                cmdbuf.as_raw(),
                self.resources.indices.as_raw(),
                0,
                vk::IndexType::UINT32, // Use 32-bit indices
            );
        }

        self.gfx_ppl
            .record_viewport_scissor(cmdbuf, viewport, scissor);

        self.gfx_ppl.record_draw_indexed(
            cmdbuf,
            self.resources.indices_len,
            grass_instances_len,
            0,
            0,
            0,
        );

        self.gfx_render_pass.record_end(cmdbuf);

        let desc = self.gfx_render_pass.get_desc();
        // The color attachment is at index 0
        self.get_dst_image()
            .set_layout(0, desc.attachments[0].final_layout);
        // The depth attachment is at index 1
        self.resources
            .depth_tex
            .get_image()
            .set_layout(0, desc.attachments[1].final_layout);
    }

    pub fn handle_keyboard(&mut self, key_event: &KeyEvent) {
        self.camera.handle_keyboard(key_event);
    }

    pub fn handle_mouse(&mut self, delta: Vec2) {
        self.camera.handle_mouse(delta);
    }

    pub fn update_transform(&mut self, frame_delta_time: f32) {
        self.camera.update_transform(frame_delta_time);
    }

    pub fn update_buffers(
        &mut self,
        debug_float: f32,
        debug_bool: bool,
        sun_dir: Vec3,
        sun_size: f32,
        sun_color: Vec3,
        grass_offset: Vec2,
    ) -> Result<(), String> {
        update_gui_input(
            &self.resources,
            debug_float,
            debug_bool,
            sun_dir,
            sun_size,
            sun_color,
        )?;
        update_cam_info(&self.camera, &self.resources)?;
        update_env_info(&self.resources, self.frame_serial_idx)?;

        update_grass_info(&self.resources, grass_offset)?;

        self.frame_serial_idx += 1;

        return Ok(());

        fn update_gui_input(
            resources: &TracerResources,
            debug_float: f32,
            debug_bool: bool,
            sun_dir: Vec3,
            sun_size: f32,
            sun_color: Vec3,
        ) -> Result<(), String> {
            let data = StructMemberDataBuilder::from_buffer(&resources.gui_input)
                .set_field("debug_float", PlainMemberTypeWithData::Float(debug_float))
                .unwrap()
                .set_field(
                    "debug_bool",
                    PlainMemberTypeWithData::UInt(debug_bool as u32),
                )
                .unwrap()
                .set_field("sun_dir", PlainMemberTypeWithData::Vec3(sun_dir.to_array()))
                .unwrap()
                .set_field("sun_size", PlainMemberTypeWithData::Float(sun_size))
                .unwrap()
                .set_field(
                    "sun_color",
                    PlainMemberTypeWithData::Vec3(sun_color.to_array()),
                )
                .unwrap()
                .build();
            resources.gui_input.fill_with_raw_u8(&data)?;
            return Ok(());
        }

        fn update_cam_info(camera: &Camera, resources: &TracerResources) -> Result<(), String> {
            let view_mat = camera.get_view_mat();
            let proj_mat = camera.get_proj_mat();
            let view_proj_mat = proj_mat * view_mat;
            let data = StructMemberDataBuilder::from_buffer(&resources.camera_info)
                .set_field(
                    "camera_pos",
                    PlainMemberTypeWithData::Vec4(camera.position_vec4().to_array()),
                )
                .unwrap()
                .set_field(
                    "view_mat",
                    PlainMemberTypeWithData::Mat4(view_mat.to_cols_array_2d()),
                )
                .unwrap()
                .set_field(
                    "view_mat_inv",
                    PlainMemberTypeWithData::Mat4(view_mat.inverse().to_cols_array_2d()),
                )
                .unwrap()
                .set_field(
                    "proj_mat",
                    PlainMemberTypeWithData::Mat4(proj_mat.to_cols_array_2d()),
                )
                .unwrap()
                .set_field(
                    "proj_mat_inv",
                    PlainMemberTypeWithData::Mat4(proj_mat.inverse().to_cols_array_2d()),
                )
                .unwrap()
                .set_field(
                    "view_proj_mat",
                    PlainMemberTypeWithData::Mat4(view_proj_mat.to_cols_array_2d()),
                )
                .unwrap()
                .set_field(
                    "view_proj_mat_inv",
                    PlainMemberTypeWithData::Mat4(view_proj_mat.inverse().to_cols_array_2d()),
                )
                .unwrap()
                .build();
            resources.camera_info.fill_with_raw_u8(&data)?;
            Ok(())
        }

        fn update_env_info(
            resources: &TracerResources,
            frame_serial_idx: u32,
        ) -> Result<(), String> {
            let data = StructMemberDataBuilder::from_buffer(&resources.env_info)
                .set_field(
                    "frame_serial_idx",
                    PlainMemberTypeWithData::UInt(frame_serial_idx),
                )
                .unwrap()
                .build();
            resources.env_info.fill_with_raw_u8(&data)?;
            Ok(())
        }

        fn update_grass_info(
            resources: &TracerResources,
            grass_offset: Vec2,
        ) -> Result<(), String> {
            let data = StructMemberDataBuilder::from_buffer(&resources.grass_info)
                .set_field(
                    "grass_offset",
                    PlainMemberTypeWithData::Vec2(grass_offset.to_array()),
                )
                .unwrap()
                .build();
            resources.grass_info.fill_with_raw_u8(&data)?;
            Ok(())
        }
    }
}
