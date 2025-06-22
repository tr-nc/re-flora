mod resources;
use glam::Vec3;
pub use resources::*;

use crate::gameplay::Camera;
use crate::util::ShaderCompiler;
use crate::vkn::{
    AccelStruct, Allocator, Buffer, ComputePipeline, DescriptorPool, DescriptorSet, Framebuffer,
    GraphicsPipeline, GraphicsPipelineDesc, Image, PlainMemberTypeWithData, RenderPass,
    RenderPassDesc, ShaderModule, StructMemberDataBuilder, Texture, WriteDescriptorSet,
};
use crate::vkn::{CommandBuffer, VulkanContext};
use ash::vk;

pub struct Tracer {
    vulkan_ctx: VulkanContext,

    allocator: Allocator,
    resources: TracerResources,

    tracer_ppl: ComputePipeline,
    tracer_sets: [DescriptorSet; 3],
    graphics_sets: [DescriptorSet; 1],
    gfx_ppl: GraphicsPipeline,
    gfx_render_pass: RenderPass,
    gfx_framebuffers: Vec<Framebuffer>,

    descriptor_pool_ds_0: DescriptorPool,
    descriptor_pool_ds_1: DescriptorPool,
    descriptor_pool_ds_2: DescriptorPool,

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
        tlas: &AccelStruct,
        swapchain_image_views: &[vk::ImageView],
    ) -> Self {
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
        let descriptor_pool_ds_2 = DescriptorPool::from_descriptor_set_layouts(
            vulkan_ctx.device(),
            tracer_ppl.get_layout().get_descriptor_set_layouts(),
        )
        .unwrap();

        let resources =
            TracerResources::new(&vulkan_ctx, allocator.clone(), &tracer_sm, screen_extent);

        let (gfx_ppl, gfx_render_pass) = Self::create_graphics_pipeline(
            &vulkan_ctx,
            resources.shader_write_tex.get_image().get_desc().format,
            shader_compiler,
        );

        let gfx_framebuffers = Self::create_framebuffers(
            &vulkan_ctx,
            &gfx_render_pass,
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

        let tracer_set_2 = Self::create_descriptor_set_2(
            descriptor_pool_ds_2.clone(),
            &vulkan_ctx,
            &tracer_ppl,
            tlas,
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
            tracer_ppl,
            tracer_sets: [tracer_set_0, tracer_set_1, tracer_set_2],
            graphics_sets: [graphics_set_0],
            gfx_ppl,
            gfx_render_pass,
            gfx_framebuffers,
            descriptor_pool_ds_0,
            descriptor_pool_ds_1,
            descriptor_pool_ds_2,
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
        ds.perform_writes(&mut [WriteDescriptorSet::new_buffer_write(
            0,
            &resources.camera_info,
        )]);
        ds
    }

    fn create_graphics_pipeline(
        vulkan_ctx: &VulkanContext,
        shader_write_tex_format: vk::Format,
        shader_compiler: &ShaderCompiler,
    ) -> (GraphicsPipeline, RenderPass) {
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

        let render_pass = {
            let desc = RenderPassDesc {
                format: shader_write_tex_format,
                final_layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                ..Default::default()
            };
            RenderPass::new(vulkan_ctx.device().clone(), &desc)
        };

        let gfx_ppl = GraphicsPipeline::new(
            vulkan_ctx.device(),
            &vert_sm,
            &frag_sm,
            &render_pass,
            &GraphicsPipelineDesc {
                cull_mode: vk::CullModeFlags::BACK,
                ..Default::default()
            },
        );

        (gfx_ppl, render_pass)
    }

    fn create_framebuffers(
        vulkan_ctx: &VulkanContext,
        render_pass: &RenderPass,
        target_texture: &Texture,
        swapchain_image_views: &[vk::ImageView],
    ) -> Vec<Framebuffer> {
        let dst_image_view = target_texture.get_image_view().as_raw();
        let dst_image_extent = {
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
                    dst_image_view,
                    dst_image_extent,
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
        ds.perform_writes(&mut [WriteDescriptorSet::new_texture_write(
            0,
            vk::DescriptorType::STORAGE_IMAGE,
            &resources.shader_write_tex,
            vk::ImageLayout::GENERAL,
        )]);
        ds
    }

    fn create_descriptor_set_2(
        descriptor_pool: DescriptorPool,
        vulkan_ctx: &VulkanContext,
        tracer_ppl: &ComputePipeline,
        tlas: &AccelStruct,
    ) -> DescriptorSet {
        let ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &tracer_ppl.get_layout().get_descriptor_set_layouts()[&2],
            descriptor_pool,
        );
        ds.perform_writes(&mut [WriteDescriptorSet::new_acceleration_structure_write(
            0, tlas,
        )]);
        ds
    }

    pub fn update_tlas_binding(&mut self, tlas: &AccelStruct) {
        self.descriptor_pool_ds_2.reset().unwrap();
        self.tracer_sets[2] = Self::create_descriptor_set_2(
            self.descriptor_pool_ds_2.clone(),
            &self.vulkan_ctx,
            &self.tracer_ppl,
            tlas,
        );
    }

    pub fn on_resize(&mut self, screen_extent: &[u32; 2], swapchain_image_views: &[vk::ImageView]) {
        self.resources.on_resize(
            self.vulkan_ctx.device().clone(),
            self.allocator.clone(),
            &screen_extent,
        );

        self.gfx_framebuffers = Self::create_framebuffers(
            &self.vulkan_ctx,
            &self.gfx_render_pass,
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

    pub fn record_command_buffer(&mut self, cmdbuf: &CommandBuffer, image_index: usize) {
        self.record_screen_space_pass(cmdbuf, image_index);
        // disabled to use later on
        // self._record_trace_pass(cmdbuf);
    }

    pub fn get_dst_image(&self) -> &Image {
        self.resources.shader_write_tex.get_image()
    }

    pub fn _record_trace_pass(&self, cmdbuf: &CommandBuffer) {
        let screen_extent = self.get_dst_image().get_desc().extent;

        self.get_dst_image()
            .record_transition_barrier(cmdbuf, 0, vk::ImageLayout::GENERAL);
        self.tracer_ppl.record_bind(cmdbuf);

        self.tracer_ppl
            .record_bind_descriptor_sets(cmdbuf, &self.tracer_sets, 0);
        self.tracer_ppl
            .record_dispatch(cmdbuf, [screen_extent[0], screen_extent[1], 1]);
    }

    pub fn record_screen_space_pass(&self, cmdbuf: &CommandBuffer, image_index: usize) {
        // TODO: take care of the transition of the image layout, if needed

        self.gfx_ppl.record_bind(cmdbuf);

        self.gfx_render_pass.record_begin(
            cmdbuf,
            &self.gfx_framebuffers[image_index],
            &[0.0, 0.0, 0.0, 1.0],
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

        self.gfx_ppl
            .record_viewport_scissor(cmdbuf, viewport, scissor);

        self.gfx_ppl.record_draw(cmdbuf, 3, 1, 0, 0);

        self.gfx_render_pass.record_end(cmdbuf);
        // TODO: do this inside the render pass!
        self.get_dst_image()
            .set_layout(0, vk::ImageLayout::TRANSFER_SRC_OPTIMAL);
    }

    pub fn update_buffers(
        &mut self,
        debug_float: f32,
        debug_bool: bool,
        sun_dir: Vec3,
        sun_size: f32,
        sun_color: Vec3,
        camera: &Camera,
    ) -> Result<(), String> {
        update_gui_input(
            &self.resources,
            debug_float,
            debug_bool,
            sun_dir,
            sun_size,
            sun_color,
        )?;
        update_cam_info(&self.resources, camera)?;
        update_env_info(&self.resources, self.frame_serial_idx)?;

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
                .get_data_u8();
            resources.gui_input.fill_with_raw_u8(&data)?;
            return Ok(());
        }

        fn update_cam_info(resources: &TracerResources, camera: &Camera) -> Result<(), String> {
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
                .get_data_u8();
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
                .get_data_u8();
            resources.env_info.fill_with_raw_u8(&data)?;
            Ok(())
        }
    }
}
