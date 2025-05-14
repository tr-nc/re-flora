use super::TracerResources;
use crate::builder::AccelStructResources;
use crate::gameplay::Camera;
use crate::util::ShaderCompiler;
use crate::vkn::{
    Allocator, Buffer, ComputePipeline, DescriptorPool, DescriptorSet, Image,
    PlainMemberTypeWithData, ShaderModule, StructMemberDataBuilder, Texture, Tlas,
    WriteDescriptorSet,
};
use crate::vkn::{CommandBuffer, VulkanContext};
use ash::vk;

pub struct Tracer {
    vulkan_context: VulkanContext,

    allocator: Allocator,
    resources: TracerResources,

    tracer_ppl: ComputePipeline,
    tracer_ds: DescriptorSet,
    descriptor_pool: DescriptorPool,
}

impl Tracer {
    pub fn new(
        vulkan_context: VulkanContext,
        allocator: Allocator,
        shader_compiler: &ShaderCompiler,
        screen_extent: &[u32; 2],
        node_data: &Buffer,
        leaf_data: &Buffer,
        scene_tex: &Texture,
        tlas: &Tlas,
    ) -> Self {
        let tracer_sm = ShaderModule::from_glsl(
            vulkan_context.device(),
            &shader_compiler,
            "shader/tracer/tracer.comp",
            "main",
        )
        .unwrap();
        let tracer_ppl = ComputePipeline::from_shader_module(vulkan_context.device(), &tracer_sm);

        let descriptor_pool = DescriptorPool::from_descriptor_set_layouts(
            vulkan_context.device(),
            tracer_ppl.get_layout().get_descriptor_set_layouts(),
        )
        .unwrap();

        let resources = TracerResources::new(
            &vulkan_context,
            allocator.clone(),
            &tracer_sm,
            screen_extent,
        );

        let tracer_ds = Self::create_tracer_ds(
            descriptor_pool.clone(),
            &vulkan_context,
            &tracer_ppl,
            &resources,
            node_data,
            leaf_data,
            scene_tex,
            tlas,
        );

        return Self {
            vulkan_context,
            allocator,
            resources,
            tracer_ppl,
            tracer_ds,
            descriptor_pool,
        };
    }

    pub fn on_resize(
        &mut self,
        screen_extent: &[u32; 2],
        node_data: &Buffer,
        leaf_data: &Buffer,
        scene_tex: &Texture,
        tlas: &Tlas,
    ) {
        self.resources.on_resize(
            self.vulkan_context.device().clone(),
            self.allocator.clone(),
            &screen_extent,
        );

        self.descriptor_pool.reset().unwrap();
        // TODO: use two descriptor sets to avoid binding swapchain-size unrelated resources
        self.tracer_ds = Self::create_tracer_ds(
            self.descriptor_pool.clone(),
            &self.vulkan_context,
            &self.tracer_ppl,
            &self.resources,
            node_data,
            leaf_data,
            scene_tex,
            tlas,
        );
    }

    pub fn record_command_buffer(&mut self, cmdbuf: &CommandBuffer, screen_extent: &[u32; 2]) {
        self.resources
            .shader_write_tex
            .get_image()
            .record_transition_barrier(cmdbuf, vk::ImageLayout::GENERAL);
        self.tracer_ppl.record_bind(cmdbuf);
        self.tracer_ppl.record_bind_descriptor_sets(
            cmdbuf,
            std::slice::from_ref(&self.tracer_ds),
            0,
        );
        self.tracer_ppl
            .record_dispatch(cmdbuf, [screen_extent[0], screen_extent[1], 1]);
    }

    pub fn get_dst_image(&self) -> &Image {
        self.resources.shader_write_tex.get_image()
    }

    pub fn update_buffers(
        &mut self,
        debug_float: f32,
        debug_bool: bool,
        camera: &Camera,
        time_stamp: u32,
    ) -> Result<(), String> {
        update_gui_input(&self.resources, debug_float, debug_bool)?;
        update_cam_info(&self.resources, camera)?;
        update_env_info(&self.resources, time_stamp)?;
        return Ok(());

        fn update_gui_input(
            resources: &TracerResources,
            debug_float: f32,
            debug_bool: bool,
        ) -> Result<(), String> {
            let data = StructMemberDataBuilder::from_buffer(&resources.gui_input)
                .set_field("debug_float", PlainMemberTypeWithData::Float(debug_float))
                .unwrap()
                .set_field(
                    "debug_bool",
                    PlainMemberTypeWithData::UInt(debug_bool as u32),
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

        fn update_env_info(resources: &TracerResources, time_stamp: u32) -> Result<(), String> {
            let data = StructMemberDataBuilder::from_buffer(&resources.env_info)
                .set_field("time_stamp", PlainMemberTypeWithData::UInt(time_stamp))
                .unwrap()
                .get_data_u8();
            resources.env_info.fill_with_raw_u8(&data)?;
            Ok(())
        }
    }

    fn create_tracer_ds(
        descriptor_pool: DescriptorPool,
        vulkan_context: &VulkanContext,
        compute_pipeline: &ComputePipeline,
        resources: &TracerResources,
        node_data: &Buffer,
        leaf_data: &Buffer,
        scene_tex: &Texture,
        tlas: &Tlas,
    ) -> DescriptorSet {
        let compute_descriptor_set = DescriptorSet::new(
            vulkan_context.device().clone(),
            &compute_pipeline.get_layout().get_descriptor_set_layouts()[0],
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
                &resources.shader_write_tex,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_acceleration_structure_write(7, tlas),
        ]);
        compute_descriptor_set
    }
}
