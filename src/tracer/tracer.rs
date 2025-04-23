use super::TracerResources;
use crate::builder::ExternalSharedResources;
use crate::gameplay::Camera;
use crate::util::ShaderCompiler;
use crate::vkn::{
    Allocator, ComputePipeline, DescriptorPool, DescriptorSet, Image, PlainMemberDataBuilder,
    ShaderModule, WriteDescriptorSet,
};
use crate::vkn::{CommandBuffer, VulkanContext};
use ash::vk;
use glam::UVec3;

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
        visible_chunk_dim: UVec3,
        builder_shared_resources: &ExternalSharedResources,
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
            vulkan_context.device().clone(),
            allocator.clone(),
            &tracer_sm,
            screen_extent,
        );

        let tracer_ds = Self::create_compute_descriptor_set(
            descriptor_pool.clone(),
            &vulkan_context,
            &tracer_ppl,
            &resources,
            &builder_shared_resources,
        );

        let this_self = Self {
            vulkan_context,
            allocator,
            resources,
            tracer_ppl,
            tracer_ds,
            descriptor_pool,
        };

        this_self.initialize_buffers(visible_chunk_dim);

        this_self
    }

    pub fn on_resize(
        &mut self,
        screen_extent: &[u32; 2],
        builder_shared_resources: &ExternalSharedResources,
    ) {
        self.resources.on_resize(
            self.vulkan_context.device().clone(),
            self.allocator.clone(),
            &screen_extent,
        );

        self.descriptor_pool.reset().unwrap();
        self.tracer_ds = Self::create_compute_descriptor_set(
            self.descriptor_pool.clone(),
            &self.vulkan_context,
            &self.tracer_ppl,
            &self.resources,
            &builder_shared_resources,
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

    fn initialize_buffers(&self, visible_chunk_dim: UVec3) {
        // let scene_info_data =
        //     PlainMemberDataBuilder::from_struct_buffer(&self.resources.scene_info)
        //         .unwrap()
        //         .set_uvec3("visible_chunk_dim", visible_chunk_dim.to_array())
        //         .to_raw_data();
        // self.resources
        //     .scene_info
        //     .fill_with_raw_u8(&scene_info_data)
        //     .unwrap();
    }

    /// Update the uniform buffers with the latest camera and debug values, called every frame
    pub fn update_uniforms(&mut self, camera: &Camera, debug_float: f32) {
        // let gui_input_data = PlainMemberDataBuilder::from_struct_buffer(&self.resources.gui_input)
        //     .unwrap()
        //     .set_float("debug_float", debug_float)
        //     .to_raw_data();
        // self.resources
        //     .gui_input
        //     .fill_with_raw_u8(&gui_input_data)
        //     .unwrap();

        // let view_mat = camera.get_view_mat();
        // let proj_mat = camera.get_proj_mat();
        // let view_proj_mat = proj_mat * view_mat;
        // let camera_info_data =
        //     PlainMemberDataBuilder::from_struct_buffer(&self.resources.camera_info)
        //         .unwrap()
        //         .set_vec4("camera_pos", camera.position_vec4().to_array())
        //         .set_mat4("view_mat", view_mat.to_cols_array_2d())
        //         .set_mat4("view_mat_inv", view_mat.inverse().to_cols_array_2d())
        //         .set_mat4("proj_mat", proj_mat.to_cols_array_2d())
        //         .set_mat4("proj_mat_inv", proj_mat.inverse().to_cols_array_2d())
        //         .set_mat4("view_proj_mat", view_proj_mat.to_cols_array_2d())
        //         .set_mat4(
        //             "view_proj_mat_inv",
        //             view_proj_mat.inverse().to_cols_array_2d(),
        //         )
        //         .to_raw_data();
        // self.resources
        //     .camera_info
        //     .fill_with_raw_u8(&camera_info_data)
        //     .unwrap();
    }

    fn create_compute_descriptor_set(
        descriptor_pool: DescriptorPool,
        vulkan_context: &VulkanContext,
        compute_pipeline: &ComputePipeline,
        resources: &TracerResources,
        builder_shared_resources: &ExternalSharedResources,
    ) -> DescriptorSet {
        let compute_descriptor_set = DescriptorSet::new(
            vulkan_context.device().clone(),
            &compute_pipeline.get_layout().get_descriptor_set_layouts()[0],
            descriptor_pool,
        );
        compute_descriptor_set.perform_writes(&[
            WriteDescriptorSet::new_buffer_write(0, &resources.gui_input),
            WriteDescriptorSet::new_buffer_write(1, &resources.camera_info),
            WriteDescriptorSet::new_buffer_write(2, &resources.scene_info),
            WriteDescriptorSet::new_buffer_write(3, &builder_shared_resources.octree_data),
            WriteDescriptorSet::new_texture_write(
                4,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.shader_write_tex,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                5,
                vk::DescriptorType::STORAGE_IMAGE,
                &builder_shared_resources.octree_offset_atlas_tex,
                vk::ImageLayout::GENERAL,
            ),
        ]);
        compute_descriptor_set
    }
}
