use super::TracerResources;
use crate::gameplay::Camera;
use crate::util::compiler::ShaderCompiler;
use crate::vkn::{
    Allocator, BufferBuilder, ComputePipeline, DescriptorPool, DescriptorSet, Image, ShaderModule,
    WriteDescriptorSet,
};
use crate::vkn::{CommandBuffer, VulkanContext};
use ash::vk;

pub struct Tracer {
    vulkan_context: VulkanContext,

    allocator: Allocator,
    resources: TracerResources,

    compute_shader_module: ShaderModule,
    compute_pipeline: ComputePipeline,
    compute_descriptor_set: DescriptorSet,
    descriptor_pool: DescriptorPool,
}

impl Tracer {
    pub fn new(
        vulkan_context: VulkanContext,
        allocator: Allocator,
        shader_compiler: &ShaderCompiler,
        screen_extent: &[u32; 2],
    ) -> Self {
        let compute_shader_module = ShaderModule::from_glsl(
            vulkan_context.device(),
            &shader_compiler,
            "shader/tracer.comp",
            "main",
        )
        .unwrap();
        let compute_pipeline =
            ComputePipeline::from_shader_module(vulkan_context.device(), &compute_shader_module);

        let descriptor_pool = DescriptorPool::from_descriptor_set_layouts(
            vulkan_context.device(),
            compute_pipeline
                .get_pipeline_layout()
                .get_descriptor_set_layouts(),
        )
        .unwrap();

        let tracer_resources = TracerResources::new(
            vulkan_context.device().clone(),
            allocator.clone(),
            &compute_shader_module,
            screen_extent,
        );

        let compute_descriptor_set = Self::create_compute_descriptor_set(
            descriptor_pool.clone(),
            &vulkan_context,
            &compute_pipeline,
            &tracer_resources,
        );

        Self {
            vulkan_context,
            allocator,
            resources: tracer_resources,
            compute_shader_module,
            compute_pipeline,
            compute_descriptor_set,
            descriptor_pool,
        }
    }

    pub fn on_resize(&mut self, screen_extent: &[u32; 2]) {
        self.resources.on_resize(
            self.vulkan_context.device().clone(),
            self.allocator.clone(),
            &screen_extent,
        );

        self.descriptor_pool.reset().unwrap();
        self.compute_descriptor_set = Self::create_compute_descriptor_set(
            self.descriptor_pool.clone(),
            &self.vulkan_context,
            &self.compute_pipeline,
            &self.resources,
        );
    }

    pub fn record_command_buffer(&mut self, cmdbuf: &CommandBuffer, screen_extent: &[u32; 2]) {
        self.resources
            .shader_write_tex
            .get_image()
            .record_transition_barrier(cmdbuf, vk::ImageLayout::GENERAL);
        self.compute_pipeline.record_bind(cmdbuf);
        self.compute_pipeline.record_bind_descriptor_sets(
            cmdbuf,
            std::slice::from_ref(&self.compute_descriptor_set),
            0,
        );
        self.compute_pipeline
            .record_dispatch(cmdbuf, [screen_extent[0], screen_extent[1], 1]);
    }

    pub fn get_dst_image(&self) -> &Image {
        self.resources.shader_write_tex.get_image()
    }

    /// Update the uniform buffers with the latest camera and debug values, called every frame
    pub fn update_uniform_buffers(&mut self, camera: &Camera, debug_float: f32) {
        let gui_input_layout = self
            .compute_shader_module
            .get_buffer_layout("GuiInput")
            .unwrap();
        let gui_input_data = BufferBuilder::from_layout(gui_input_layout)
            .set_float("debug_float", debug_float)
            .build();
        self.resources
            .gui_input_buffer
            .fill_raw(&gui_input_data)
            .unwrap();

        let camera_info_layout = self
            .compute_shader_module
            .get_buffer_layout("CameraInfo")
            .unwrap();

        let view_mat = camera.get_view_mat();
        let proj_mat = camera.get_proj_mat();
        let view_proj_mat = proj_mat * view_mat;
        let camera_info_data = BufferBuilder::from_layout(camera_info_layout)
            .set_vec4("camera_pos", camera.position_vec4().to_array())
            .set_mat4("view_mat", view_mat.to_cols_array_2d())
            .set_mat4("view_mat_inv", view_mat.inverse().to_cols_array_2d())
            .set_mat4("proj_mat", proj_mat.to_cols_array_2d())
            .set_mat4("proj_mat_inv", proj_mat.inverse().to_cols_array_2d())
            .set_mat4("view_proj_mat", view_proj_mat.to_cols_array_2d())
            .set_mat4(
                "view_proj_mat_inv",
                view_proj_mat.inverse().to_cols_array_2d(),
            )
            .build();
        self.resources
            .camera_info_buffer
            .fill_raw(&camera_info_data)
            .unwrap();
    }

    fn create_compute_descriptor_set(
        descriptor_pool: DescriptorPool,
        vulkan_context: &VulkanContext,
        compute_pipeline: &ComputePipeline,
        renderer_resources: &TracerResources,
    ) -> DescriptorSet {
        let compute_descriptor_set = DescriptorSet::new(
            vulkan_context.device().clone(),
            compute_pipeline
                .get_pipeline_layout()
                .get_descriptor_set_layouts(),
            descriptor_pool,
        );
        compute_descriptor_set.perform_writes(&[
            WriteDescriptorSet::new_texture_write(
                0,
                vk::DescriptorType::STORAGE_IMAGE,
                &renderer_resources.shader_write_tex,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_buffer_write(
                1,
                vk::DescriptorType::UNIFORM_BUFFER,
                &renderer_resources.gui_input_buffer,
            ),
            WriteDescriptorSet::new_buffer_write(
                2,
                vk::DescriptorType::UNIFORM_BUFFER,
                &renderer_resources.camera_info_buffer,
            ),
        ]);
        compute_descriptor_set
    }
}
