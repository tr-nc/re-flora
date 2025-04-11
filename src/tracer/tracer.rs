use super::TracerResources;
use crate::gameplay::Camera;
use crate::util::compiler::ShaderCompiler;
use crate::vkn::{
    Allocator, Buffer, BufferBuilder, ComputePipeline, DescriptorPool, DescriptorSet, Image,
    ShaderModule, WriteDescriptorSet,
};
use crate::vkn::{CommandBuffer, VulkanContext};
use ash::vk;

pub struct Tracer {
    vulkan_context: VulkanContext,

    allocator: Allocator,
    resources: TracerResources,

    tracer_sm: ShaderModule,
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
        octree_data: &Buffer,
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
            octree_data,
        );

        Self {
            vulkan_context,
            allocator,
            resources,
            tracer_sm,
            tracer_ppl,
            tracer_ds,
            descriptor_pool,
        }
    }

    pub fn on_resize(&mut self, screen_extent: &[u32; 2], octree_data: &Buffer) {
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
            octree_data,
        );
    }

    pub fn record_command_buffer(&mut self, cmdbuf: &CommandBuffer, screen_extent: &[u32; 2]) {
        self.resources
            .shader_write
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
        self.resources.shader_write.get_image()
    }

    /// Update the uniform buffers with the latest camera and debug values, called every frame
    pub fn update_uniform_buffers(&mut self, camera: &Camera, debug_float: f32) {
        let gui_input_layout = self.tracer_sm.get_buffer_layout("GuiInput").unwrap();
        let gui_input_data = BufferBuilder::from_layout(gui_input_layout)
            .set_float("debug_float", debug_float)
            .to_raw_data();
        self.resources.gui_input.fill_raw(&gui_input_data).unwrap();

        let camera_info_layout = self.tracer_sm.get_buffer_layout("CameraInfo").unwrap();

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
            .to_raw_data();
        self.resources
            .camera_info
            .fill_raw(&camera_info_data)
            .unwrap();
    }

    fn create_compute_descriptor_set(
        descriptor_pool: DescriptorPool,
        vulkan_context: &VulkanContext,
        compute_pipeline: &ComputePipeline,
        resources: &TracerResources,
        octree_data: &Buffer,
    ) -> DescriptorSet {
        let compute_descriptor_set = DescriptorSet::new(
            vulkan_context.device().clone(),
            &compute_pipeline.get_layout().get_descriptor_set_layouts()[0],
            descriptor_pool,
        );
        compute_descriptor_set.perform_writes(&[
            WriteDescriptorSet::new_texture_write(
                0,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.shader_write,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_buffer_write(1, &resources.gui_input),
            WriteDescriptorSet::new_buffer_write(2, &resources.camera_info),
            WriteDescriptorSet::new_buffer_write(3, octree_data),
        ]);
        compute_descriptor_set
    }
}
