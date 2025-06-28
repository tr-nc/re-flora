mod resources;
use glam::{Mat4, Vec2, Vec3};
pub use resources::*;

mod vertex;
pub use vertex::*;
use winit::event::KeyEvent;

mod grass_construct;

use crate::builder::SurfaceResources;
use crate::gameplay::{calculate_directional_light_matrices, Camera, CameraDesc};
use crate::util::ShaderCompiler;
use crate::vkn::{
    Allocator, AttachmentDesc, AttachmentReference, Buffer, ComputePipeline, DescriptorPool,
    DescriptorSet, Extent2D, Framebuffer, GraphicsPipeline, GraphicsPipelineDesc, MemoryBarrier,
    PipelineBarrier, PlainMemberTypeWithData, RenderPass, RenderPassDesc, ShaderModule,
    StructMemberDataBuilder, SubpassDesc, Texture, Viewport, WriteDescriptorSet,
};
use crate::vkn::{CommandBuffer, VulkanContext};
use anyhow::Result;
use ash::vk;

pub struct TracerDesc {
    pub scaling_factor: f32,
}

pub struct Tracer {
    vulkan_ctx: VulkanContext,

    desc: TracerDesc,

    allocator: Allocator,
    resources: TracerResources,

    camera: Camera,

    tracer_ppl: ComputePipeline,
    tracer_sets: [DescriptorSet; 2],

    post_processing_ppl: ComputePipeline,
    post_processing_sets: [DescriptorSet; 1],

    tracer_shadow_ppl: ComputePipeline,
    tracer_shadow_sets: [DescriptorSet; 1],

    main_sets: [DescriptorSet; 1],
    main_ppl: GraphicsPipeline,
    main_render_pass: RenderPass,
    main_framebuffer: Framebuffer,

    #[allow(dead_code)]
    shadow_sets: [DescriptorSet; 1],
    #[allow(dead_code)]
    shadow_ppl: GraphicsPipeline,
    #[allow(dead_code)]
    shadow_render_pass: RenderPass,
    #[allow(dead_code)]
    shadow_framebuffer: Framebuffer,

    #[allow(dead_code)]
    fixed_pool: DescriptorPool,
    flexible_pool: DescriptorPool,

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
        screen_extent: Extent2D,
        node_data: &Buffer,
        leaf_data: &Buffer,
        scene_tex: &Texture,
        desc: TracerDesc,
    ) -> Self {
        let render_extent = Self::get_render_extent(screen_extent, desc.scaling_factor);

        let camera = Camera::new(
            Vec3::new(0.5, 1.2, 0.5),
            135.0,
            -5.0,
            CameraDesc {
                movement: Default::default(),
                projection: Default::default(),
                aspect_ratio: render_extent.get_aspect_ratio(),
            },
        );

        let tracer_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/tracer/tracer.comp",
            "main",
        )
        .unwrap();

        let tracer_shadow_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/tracer/tracer_shadow.comp",
            "main",
        )
        .unwrap();

        let post_processing_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/tracer/post_processing.comp",
            "main",
        )
        .unwrap();

        let tracer_ppl = ComputePipeline::new(vulkan_ctx.device(), &tracer_sm);
        let tracer_shadow_ppl = ComputePipeline::new(vulkan_ctx.device(), &tracer_shadow_sm);
        let post_processing_ppl = ComputePipeline::new(vulkan_ctx.device(), &post_processing_sm);

        let fixed_pool = DescriptorPool::a_big_one(vulkan_ctx.device()).unwrap();
        let flexible_pool = DescriptorPool::a_big_one(vulkan_ctx.device()).unwrap();

        let main_vert_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/foliage/foliage.vert",
            "main",
        )
        .unwrap();
        let main_frag_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/foliage/foliage.frag",
            "main",
        )
        .unwrap();

        let shadow_vert_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/foliage/shadow.vert",
            "main",
        )
        .unwrap();
        let shadow_frag_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/foliage/shadow.frag",
            "main",
        )
        .unwrap();

        let resources = TracerResources::new(
            &vulkan_ctx,
            allocator.clone(),
            &main_vert_sm,
            &tracer_sm,
            &tracer_shadow_sm,
            &post_processing_sm,
            render_extent,
            screen_extent,
            Extent2D::new(1024, 1024),
        );
        let (main_ppl, main_render_pass) = Self::create_main_render_pass_and_graphics_pipeline(
            &vulkan_ctx,
            &main_vert_sm,
            &main_frag_sm,
            resources.gfx_output_tex.clone(),
            resources.gfx_depth_tex.clone(),
        );

        let (shadow_ppl, shadow_render_pass) =
            Self::create_shadow_render_pass_and_graphics_pipeline(
                &vulkan_ctx,
                &shadow_vert_sm,
                &shadow_frag_sm,
                resources.shadow_map_tex.clone(),
            );

        let main_framebuffer = Self::create_main_framebuffer(
            &vulkan_ctx,
            &main_render_pass,
            &resources.gfx_output_tex,
            &resources.gfx_depth_tex,
        );

        let shadow_framebuffer = Self::create_shadow_framebuffer(
            &vulkan_ctx,
            &shadow_render_pass,
            &resources.shadow_map_tex,
        );

        let tracer_set_0 = Self::create_tracer_set_0(
            fixed_pool.clone(),
            &vulkan_ctx,
            &tracer_ppl,
            &resources,
            node_data,
            leaf_data,
            scene_tex,
        );

        let tracer_set_1 =
            Self::create_tracer_set_1(flexible_pool.clone(), &vulkan_ctx, &tracer_ppl, &resources);

        let tracer_shadow_set = Self::create_tracer_shadow_set(
            fixed_pool.clone(),
            &vulkan_ctx,
            &tracer_shadow_ppl,
            &resources,
            node_data,
            leaf_data,
            scene_tex,
        );

        let post_processing_set_0 = Self::create_post_processing_set_0(
            flexible_pool.clone(),
            &vulkan_ctx,
            &post_processing_ppl,
            &resources,
        );

        let main_ds = Self::create_main_ds(fixed_pool.clone(), &vulkan_ctx, &main_ppl, &resources);

        let shadow_ds =
            Self::create_shadow_ds(fixed_pool.clone(), &vulkan_ctx, &shadow_ppl, &resources);

        return Self {
            vulkan_ctx,
            desc,
            allocator,
            resources,
            camera,
            tracer_ppl,
            tracer_shadow_ppl,
            post_processing_ppl,
            post_processing_sets: [post_processing_set_0],
            tracer_sets: [tracer_set_0, tracer_set_1],
            tracer_shadow_sets: [tracer_shadow_set],
            main_sets: [main_ds],
            shadow_sets: [shadow_ds],
            main_ppl,
            main_render_pass,
            main_framebuffer,
            shadow_ppl,
            shadow_render_pass,
            shadow_framebuffer,
            fixed_pool,
            flexible_pool,
            frame_serial_idx: 0,
        };
    }

    fn create_main_ds(
        descriptor_pool: DescriptorPool,
        vulkan_ctx: &VulkanContext,
        main_ppl: &GraphicsPipeline,
        resources: &TracerResources,
    ) -> DescriptorSet {
        let ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &main_ppl.get_layout().get_descriptor_set_layouts()[&0],
            descriptor_pool,
        );
        ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.camera_info),
            WriteDescriptorSet::new_buffer_write(1, &resources.shadow_camera_info),
            WriteDescriptorSet::new_buffer_write(2, &resources.grass_info),
            WriteDescriptorSet::new_texture_write(
                3,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                &resources.shadow_map_tex,
                vk::ImageLayout::GENERAL,
            ),
        ]);
        ds
    }

    fn create_shadow_ds(
        descriptor_pool: DescriptorPool,
        vulkan_ctx: &VulkanContext,
        shadow_ppl: &GraphicsPipeline,
        resources: &TracerResources,
    ) -> DescriptorSet {
        let ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &shadow_ppl.get_layout().get_descriptor_set_layouts()[&0],
            descriptor_pool,
        );
        ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.shadow_camera_info),
            WriteDescriptorSet::new_buffer_write(1, &resources.grass_info),
        ]);
        ds
    }

    fn create_main_render_pass_and_graphics_pipeline(
        vulkan_ctx: &VulkanContext,
        vert_sm: &ShaderModule,
        frag_sm: &ShaderModule,
        output_tex: Texture,
        depth_tex: Texture,
    ) -> (GraphicsPipeline, RenderPass) {
        let render_pass = {
            RenderPass::with_attachments(
                vulkan_ctx.device().clone(),
                output_tex,
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

    fn create_shadow_render_pass_and_graphics_pipeline(
        vulkan_ctx: &VulkanContext,
        vert_sm: &ShaderModule,
        frag_sm: &ShaderModule,
        shadow_depth_tex: Texture,
    ) -> (GraphicsPipeline, RenderPass) {
        let render_pass_desc = RenderPassDesc {
            attachments: vec![AttachmentDesc {
                format: shadow_depth_tex.get_image().get_desc().format,
                samples: vk::SampleCountFlags::TYPE_1,
                load_op: vk::AttachmentLoadOp::CLEAR,
                store_op: vk::AttachmentStoreOp::STORE,
                stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
                stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
                initial_layout: vk::ImageLayout::UNDEFINED,
                final_layout: vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL,
            }],
            subpasses: vec![SubpassDesc {
                color_attachments: vec![],
                depth_stencil_attachment: Some(AttachmentReference {
                    attachment: 0,
                    layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                }),
                ..Default::default()
            }],
            // Add a dependency to ensure writes to the depth buffer are complete
            // before any subsequent pass tries to read from it.
            dependencies: vec![vk::SubpassDependency::default()
                .src_subpass(vk::SUBPASS_EXTERNAL)
                .dst_subpass(0)
                .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT) // Or an earlier stage
                .dst_stage_mask(vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS)
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE)],
        };

        // 2. Create the render pass using the flexible `from_desc` constructor.
        let render_pass = RenderPass::from_desc(vulkan_ctx.device().clone(), render_pass_desc);

        // 3. Create the graphics pipeline for this render pass.
        let gfx_ppl = GraphicsPipeline::new(
            vulkan_ctx.device(),
            &vert_sm,
            &frag_sm,
            &render_pass,
            &GraphicsPipelineDesc {
                cull_mode: vk::CullModeFlags::BACK, // Or FRONT depending on your shadow bias needs
                depth_test_enable: true,
                depth_write_enable: true,
                ..Default::default()
            },
            Some(3),
        );

        (gfx_ppl, render_pass)
    }

    fn create_main_framebuffer(
        vulkan_ctx: &VulkanContext,
        render_pass: &RenderPass,
        target_texture: &Texture,
        depth_texture: &Texture,
    ) -> Framebuffer {
        let target_view = target_texture.get_image_view().as_raw();
        let depth_image_view = depth_texture.get_image_view().as_raw();

        let target_image_extent = target_texture
            .get_image()
            .get_desc()
            .extent
            .as_extent_2d()
            .unwrap();

        Framebuffer::new(
            vulkan_ctx.clone(),
            &render_pass,
            &[target_view, depth_image_view],
            target_image_extent,
        )
        .unwrap()
    }

    fn create_shadow_framebuffer(
        vulkan_ctx: &VulkanContext,
        render_pass: &RenderPass,
        shadow_depth_tex: &Texture,
    ) -> Framebuffer {
        let shadow_depth_image_view = shadow_depth_tex.get_image_view().as_raw();
        let shadow_depth_image_extent = shadow_depth_tex
            .get_image()
            .get_desc()
            .extent
            .as_extent_2d()
            .unwrap();

        Framebuffer::new(
            vulkan_ctx.clone(),
            &render_pass,
            &[shadow_depth_image_view],
            shadow_depth_image_extent,
        )
        .unwrap()
    }

    fn create_tracer_set_0(
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
            WriteDescriptorSet::new_texture_write(
                12,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.shadow_map_tex,
                vk::ImageLayout::GENERAL,
            ),
        ]);
        ds
    }

    fn create_tracer_set_1(
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
                &resources.compute_output_tex,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                1,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.compute_depth_tex,
                vk::ImageLayout::GENERAL,
            ),
        ]);
        ds
    }

    fn create_post_processing_set_0(
        descriptor_pool: DescriptorPool,
        vulkan_ctx: &VulkanContext,
        post_processing_ppl: &ComputePipeline,
        resources: &TracerResources,
    ) -> DescriptorSet {
        let ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &post_processing_ppl
                .get_layout()
                .get_descriptor_set_layouts()[&0],
            descriptor_pool,
        );
        ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.post_processing_info),
            WriteDescriptorSet::new_texture_write(
                1,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.gfx_output_tex,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                2,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.gfx_depth_tex,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                3,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.compute_output_tex,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                4,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.compute_depth_tex,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                5,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.screen_output_tex,
                vk::ImageLayout::GENERAL,
            ),
        ]);
        ds
    }

    fn create_tracer_shadow_set(
        descriptor_pool: DescriptorPool,
        vulkan_ctx: &VulkanContext,
        tracer_depth_only_ppl: &ComputePipeline,
        resources: &TracerResources,
        node_data: &Buffer,
        leaf_data: &Buffer,
        scene_tex: &Texture,
    ) -> DescriptorSet {
        let ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &tracer_depth_only_ppl
                .get_layout()
                .get_descriptor_set_layouts()[&0],
            descriptor_pool,
        );
        ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.shadow_camera_info),
            WriteDescriptorSet::new_buffer_write(1, &node_data),
            WriteDescriptorSet::new_buffer_write(2, &leaf_data),
            WriteDescriptorSet::new_texture_write(
                3,
                vk::DescriptorType::STORAGE_IMAGE,
                &scene_tex,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                4,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.shadow_map_tex,
                vk::ImageLayout::GENERAL,
            ),
        ]);
        ds
    }

    pub fn on_resize(&mut self, screen_extent: Extent2D) {
        let render_extent = Self::get_render_extent(screen_extent, self.desc.scaling_factor);

        self.camera.on_resize(render_extent);

        self.resources.on_resize(
            self.vulkan_ctx.device().clone(),
            self.allocator.clone(),
            render_extent,
            screen_extent,
        );

        self.main_framebuffer = Self::create_main_framebuffer(
            &self.vulkan_ctx,
            &self.main_render_pass,
            &self.resources.gfx_output_tex,
            &self.resources.gfx_depth_tex,
        );

        self.flexible_pool.reset().unwrap();
        self.tracer_sets[1] = Self::create_tracer_set_1(
            self.flexible_pool.clone(),
            &self.vulkan_ctx,
            &self.tracer_ppl,
            &self.resources,
        );

        self.post_processing_sets[0] = Self::create_post_processing_set_0(
            self.flexible_pool.clone(),
            &self.vulkan_ctx,
            &self.post_processing_ppl,
            &self.resources,
        );
    }

    // create a lower resolution texture for rendering, for better performance,
    // less memory usage, and stylized rendering
    fn get_render_extent(screen_extent: Extent2D, scaling_factor: f32) -> Extent2D {
        let extent = Extent2D::new(
            (screen_extent.width as f32 * scaling_factor) as u32,
            (screen_extent.height as f32 * scaling_factor) as u32,
        );
        extent
    }

    pub fn get_screen_output_tex(&self) -> &Texture {
        &self.resources.screen_output_tex
    }

    pub fn update_buffers_and_record(
        &mut self,
        cmdbuf: &CommandBuffer,
        grass_offset: Vec2,
        surface_resources: &SurfaceResources,
        grass_instances_len: u32,
        debug_float: f32,
        debug_bool: bool,
        sun_dir: Vec3,
        sun_size: f32,
        sun_color: Vec3,
    ) -> Result<()> {
        let shader_access_memory_barrier = MemoryBarrier::new_shader_access();

        let (shadow_view_mat, shadow_proj_mat) =
            calculate_directional_light_matrices(&self.camera, sun_dir);
        Self::update_cam_info(
            &mut self.resources.shadow_camera_info,
            shadow_view_mat,
            shadow_proj_mat,
        )?;
        self.record_tracer_shadow_pass(cmdbuf);

        let b1 = PipelineBarrier::new(
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::VERTEX_SHADER | vk::PipelineStageFlags::COMPUTE_SHADER,
            vec![shader_access_memory_barrier],
        );
        b1.record_insert(self.vulkan_ctx.device(), cmdbuf);

        Self::update_cam_info(
            &mut self.resources.camera_info,
            self.camera.get_view_mat(),
            self.camera.get_proj_mat(),
        )?;
        Self::update_grass_info(&self.resources, grass_offset)?;
        self.record_main_pass(cmdbuf, surface_resources, grass_instances_len);

        Self::update_gui_input(
            &self.resources,
            debug_float,
            debug_bool,
            sun_dir,
            sun_size,
            sun_color,
        )?;
        Self::update_env_info(&self.resources, self.frame_serial_idx)?;

        self.record_compute_pass(cmdbuf);

        let b2 = PipelineBarrier::new(
            vk::PipelineStageFlags::FRAGMENT_SHADER | vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vec![shader_access_memory_barrier],
        );
        b2.record_insert(self.vulkan_ctx.device(), cmdbuf);

        Self::update_post_processing_info(&self.resources, self.desc.scaling_factor)?;
        self.record_post_processing_pass(cmdbuf);

        self.frame_serial_idx += 1;

        Ok(())
    }

    fn record_main_pass(
        &self,
        cmdbuf: &CommandBuffer,
        surface_resources: &SurfaceResources,
        grass_instances_len: u32,
    ) {
        self.main_ppl.record_bind(cmdbuf);

        let clear_values = [
            vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 1.0],
                },
            },
            vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
            },
        ];

        self.main_render_pass
            .record_begin(cmdbuf, &self.main_framebuffer, &clear_values);

        let render_extent = self.resources.gfx_output_tex.get_image().get_desc().extent;
        let viewport = Viewport::from_extent(render_extent.as_extent_2d().unwrap());
        let scissor = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: vk::Extent2D {
                width: render_extent.width,
                height: render_extent.height,
            },
        };

        // must be done before record draw, can be swapped with record_viewport_scissor
        self.main_ppl
            .record_bind_descriptor_sets(cmdbuf, &self.main_sets, 0);

        // TODO: wrap them later
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

        self.main_ppl
            .record_viewport_scissor(cmdbuf, viewport, scissor);

        self.main_ppl.record_draw_indexed(
            cmdbuf,
            self.resources.indices_len,
            grass_instances_len,
            0,
            0,
            0,
        );

        self.main_render_pass.record_end(cmdbuf);

        let desc = self.main_render_pass.get_desc();
        self.resources
            .gfx_output_tex
            .get_image()
            .set_layout(0, desc.attachments[0].final_layout);
        self.resources
            .gfx_depth_tex
            .get_image()
            .set_layout(0, desc.attachments[1].final_layout);
    }

    fn record_tracer_shadow_pass(&mut self, cmdbuf: &CommandBuffer) {
        self.resources
            .shadow_map_tex
            .get_image()
            .record_transition_barrier(cmdbuf, 0, vk::ImageLayout::GENERAL);
        self.tracer_shadow_ppl.record_bind(cmdbuf);
        self.tracer_shadow_ppl
            .record_bind_descriptor_sets(cmdbuf, &self.tracer_shadow_sets, 0);
        self.tracer_shadow_ppl.record_dispatch(
            cmdbuf,
            [
                self.resources
                    .shadow_map_tex
                    .get_image()
                    .get_desc()
                    .extent
                    .width,
                self.resources
                    .shadow_map_tex
                    .get_image()
                    .get_desc()
                    .extent
                    .height,
                self.resources
                    .shadow_map_tex
                    .get_image()
                    .get_desc()
                    .extent
                    .depth,
            ],
        );
    }

    fn record_compute_pass(&self, cmdbuf: &CommandBuffer) {
        self.resources
            .compute_output_tex
            .get_image()
            .record_transition_barrier(cmdbuf, 0, vk::ImageLayout::GENERAL);
        self.resources
            .compute_depth_tex
            .get_image()
            .record_transition_barrier(cmdbuf, 0, vk::ImageLayout::GENERAL);

        self.tracer_ppl.record_bind(cmdbuf);

        self.tracer_ppl
            .record_bind_descriptor_sets(cmdbuf, &self.tracer_sets, 0);

        let render_extent = self
            .resources
            .compute_output_tex
            .get_image()
            .get_desc()
            .extent;
        self.tracer_ppl.record_dispatch(
            cmdbuf,
            [
                render_extent.width,
                render_extent.height,
                render_extent.depth,
            ],
        );
    }

    fn record_post_processing_pass(&self, cmdbuf: &CommandBuffer) {
        self.resources
            .screen_output_tex
            .get_image()
            .record_transition_barrier(cmdbuf, 0, vk::ImageLayout::GENERAL);

        self.post_processing_ppl.record_bind(cmdbuf);
        self.post_processing_ppl
            .record_bind_descriptor_sets(cmdbuf, &self.post_processing_sets, 0);

        let screen_extent = self
            .resources
            .screen_output_tex
            .get_image()
            .get_desc()
            .extent;
        self.post_processing_ppl.record_dispatch(
            cmdbuf,
            [
                screen_extent.width,
                screen_extent.height,
                screen_extent.depth,
            ],
        );
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

    fn update_gui_input(
        resources: &TracerResources,
        debug_float: f32,
        debug_bool: bool,
        sun_dir: Vec3,
        sun_size: f32,
        sun_color: Vec3,
    ) -> Result<()> {
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

    fn update_cam_info(camera_info: &mut Buffer, view_mat: Mat4, proj_mat: Mat4) -> Result<()> {
        let view_proj_mat = proj_mat * view_mat;

        let camera_pos = view_mat.inverse().w_axis;
        let data = StructMemberDataBuilder::from_buffer(camera_info)
            .set_field("pos", PlainMemberTypeWithData::Vec4(camera_pos.to_array()))
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
        camera_info.fill_with_raw_u8(&data)?;
        Ok(())
    }

    fn update_env_info(resources: &TracerResources, frame_serial_idx: u32) -> Result<()> {
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

    fn update_grass_info(resources: &TracerResources, grass_offset: Vec2) -> Result<()> {
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

    fn update_post_processing_info(resources: &TracerResources, scaling_factor: f32) -> Result<()> {
        let data = StructMemberDataBuilder::from_buffer(&resources.post_processing_info)
            .set_field(
                "scaling_factor",
                PlainMemberTypeWithData::Float(scaling_factor),
            )
            .unwrap()
            .build();
        resources.post_processing_info.fill_with_raw_u8(&data)?;
        Ok(())
    }
}
