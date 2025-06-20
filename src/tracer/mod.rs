mod resources;
use glam::Vec3;
pub use resources::*;

use crate::gameplay::Camera;
use crate::util::ShaderCompiler;
use crate::vkn::{
    AccelStruct, Allocator, Buffer, ComputePipeline, DescriptorPool, DescriptorSet,
    GraphicsPipeline, Image, PlainMemberTypeWithData, RenderPass, ShaderModule,
    StructMemberDataBuilder, Texture, WriteDescriptorSet,
};
use crate::vkn::{CommandBuffer, VulkanContext};
use ash::vk;

pub struct Tracer {
    vulkan_ctx: VulkanContext,

    allocator: Allocator,
    resources: TracerResources,

    tracer_ppl: ComputePipeline,
    tracer_sets: [DescriptorSet; 3],
    gfx_ppl: GraphicsPipeline,
    gfx_render_pass: RenderPass,

    descriptor_pool_ds_0: DescriptorPool,
    descriptor_pool_ds_1: DescriptorPool,
    descriptor_pool_ds_2: DescriptorPool,

    frame_serial_idx: u32,
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
        final_render_target_format: vk::Format,
    ) -> Self {
        let tracer_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/tracer/tracer.comp",
            "main",
        )
        .unwrap();
        let tracer_ppl = ComputePipeline::new(vulkan_ctx.device(), &tracer_sm);

        let (gfx_ppl, gfx_render_pass) = Self::create_graphics_pipeline(
            &vulkan_ctx,
            shader_compiler,
            final_render_target_format,
        );

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

        return Self {
            vulkan_ctx,
            allocator,
            resources,
            tracer_ppl,
            tracer_sets: [tracer_set_0, tracer_set_1, tracer_set_2],
            gfx_ppl,
            gfx_render_pass,
            descriptor_pool_ds_0,
            descriptor_pool_ds_1,
            descriptor_pool_ds_2,
            frame_serial_idx: 0,
        };
    }

    fn create_graphics_pipeline(
        vulkan_ctx: &VulkanContext,
        shader_compiler: &ShaderCompiler,
        final_render_target_format: vk::Format,
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

        let render_pass = RenderPass::new(
            vulkan_ctx.device().clone(),
            final_render_target_format,
            vk::SampleCountFlags::TYPE_1,
        );

        let gfx_ppl =
            GraphicsPipeline::new(vulkan_ctx.device(), &vert_sm, &frag_sm, render_pass.inner);

        (gfx_ppl, render_pass)
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
        let compute_descriptor_set = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &tracer_ppl
                .get_layout()
                .get_descriptor_set_layouts()
                .get(&0)
                .unwrap(),
            descriptor_pool,
        );
        compute_descriptor_set.perform_writes(&mut [
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
        compute_descriptor_set
    }

    fn create_descriptor_set_1(
        descriptor_pool: DescriptorPool,
        vulkan_ctx: &VulkanContext,
        tracer_ppl: &ComputePipeline,
        resources: &TracerResources,
    ) -> DescriptorSet {
        let compute_descriptor_set = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &tracer_ppl.get_layout().get_descriptor_set_layouts()[&1],
            descriptor_pool,
        );
        compute_descriptor_set.perform_writes(&mut [WriteDescriptorSet::new_texture_write(
            0,
            vk::DescriptorType::STORAGE_IMAGE,
            &resources.shader_write_tex,
            vk::ImageLayout::GENERAL,
        )]);
        compute_descriptor_set
    }

    fn create_descriptor_set_2(
        descriptor_pool: DescriptorPool,
        vulkan_ctx: &VulkanContext,
        tracer_ppl: &ComputePipeline,
        tlas: &AccelStruct,
    ) -> DescriptorSet {
        let compute_descriptor_set = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &tracer_ppl.get_layout().get_descriptor_set_layouts()[&2],
            descriptor_pool,
        );
        compute_descriptor_set.perform_writes(&mut [
            WriteDescriptorSet::new_acceleration_structure_write(0, tlas),
        ]);
        compute_descriptor_set
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

    pub fn on_resize(&mut self, screen_extent: &[u32; 2]) {
        self.resources.on_resize(
            self.vulkan_ctx.device().clone(),
            self.allocator.clone(),
            &screen_extent,
        );

        self.descriptor_pool_ds_1.reset().unwrap();
        self.tracer_sets[1] = Self::create_descriptor_set_1(
            self.descriptor_pool_ds_1.clone(),
            &self.vulkan_ctx,
            &self.tracer_ppl,
            &self.resources,
        );
    }

    pub fn record_command_buffer(&mut self, cmdbuf: &CommandBuffer, screen_extent: &[u32; 2]) {
        self.resources
            .shader_write_tex
            .get_image()
            .record_transition_barrier(cmdbuf, 0, vk::ImageLayout::GENERAL);
        self.tracer_ppl.record_bind(cmdbuf);

        self.tracer_ppl
            .record_bind_descriptor_sets(cmdbuf, &self.tracer_sets, 0);
        self.tracer_ppl
            .record_dispatch(cmdbuf, [screen_extent[0], screen_extent[1], 1]);
    }

    pub fn get_dst_image(&self) -> &Image {
        self.resources.shader_write_tex.get_image()
    }

    pub fn record_screen_space_pass(
        &self,
        cmdbuf: &CommandBuffer,
        dst_image_view: vk::ImageView,
        dst_image_extent: vk::Extent2D,
    ) {
        let framebuffer = self
            .gfx_render_pass
            .create_framebuffer(dst_image_view, dst_image_extent)
            .unwrap();

        let render_pass_begin_info = vk::RenderPassBeginInfo::default()
            .render_pass(self.gfx_render_pass.inner)
            .framebuffer(framebuffer)
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: dst_image_extent,
            })
            .clear_values(&[vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 1.0],
                },
            }]);

        self.gfx_ppl.record_bind(cmdbuf);

        unsafe {
            self.vulkan_ctx.device().cmd_begin_render_pass(
                cmdbuf.as_raw(),
                &render_pass_begin_info,
                vk::SubpassContents::INLINE,
            );
            self.gfx_ppl.record_draw(cmdbuf, 3, 1, 0, 0);
            self.vulkan_ctx
                .device()
                .cmd_end_render_pass(cmdbuf.as_raw());
            self.vulkan_ctx
                .device()
                .destroy_framebuffer(framebuffer, None);
        }
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
