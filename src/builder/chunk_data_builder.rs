use super::Resources;
use crate::util::ShaderCompiler;
use crate::vkn::execute_one_time_command;
use crate::vkn::BufferBuilder;
use crate::vkn::CommandPool;
use crate::vkn::ComputePipeline;
use crate::vkn::DescriptorPool;
use crate::vkn::DescriptorSet;
use crate::vkn::ShaderModule;
use crate::vkn::VulkanContext;
use crate::vkn::WriteDescriptorSet;
use ash::vk;
use glam::UVec3;

pub struct ChunkDataBuilder {
    chunk_init_ppl: ComputePipeline,
    chunk_init_ds: DescriptorSet,
}

impl ChunkDataBuilder {
    pub fn new(
        vulkan_context: &VulkanContext,
        shader_compiler: &ShaderCompiler,
        command_pool: &CommandPool,
        descriptor_pool: DescriptorPool,
        resources: &Resources,
    ) -> Self {
        let chunk_init_ppl = ComputePipeline::from_shader_module(
            vulkan_context.device(),
            &ShaderModule::from_glsl(
                vulkan_context.device(),
                &shader_compiler,
                "shader/builder/chunk_init/chunk_init.comp",
                "main",
            )
            .unwrap(),
        );

        let chunk_init_ds = DescriptorSet::new(
            vulkan_context.device().clone(),
            &chunk_init_ppl.get_layout().get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        chunk_init_ds.perform_writes(&[
            WriteDescriptorSet::new_buffer_write(0, resources.chunk_init_info()),
            WriteDescriptorSet::new_texture_write(
                1,
                vk::DescriptorType::STORAGE_IMAGE,
                resources.raw_atlas_tex(),
                vk::ImageLayout::GENERAL,
            ),
        ]);

        init_atlas(vulkan_context, command_pool, resources);
        fn init_atlas(
            vulkan_context: &VulkanContext,
            command_pool: &CommandPool,
            resources: &Resources,
        ) {
            execute_one_time_command(
                vulkan_context.device(),
                command_pool,
                &vulkan_context.get_general_queue(),
                |cmdbuf| {
                    resources
                        .raw_atlas_tex()
                        .get_image()
                        .record_clear(cmdbuf, Some(vk::ImageLayout::GENERAL));
                },
            );
        }

        Self {
            chunk_init_ppl,
            chunk_init_ds,
        }
    }

    pub fn build(
        &mut self,
        vulkan_context: &VulkanContext,
        command_pool: &CommandPool,
        resources: &Resources,
        voxel_dim: UVec3,
        chunk_pos: UVec3,
    ) {
        update_uniforms(resources, chunk_pos);

        execute_one_time_command(
            vulkan_context.device(),
            command_pool,
            &vulkan_context.get_general_queue(),
            |cmdbuf| {
                resources
                    .raw_atlas_tex()
                    .get_image()
                    .record_transition_barrier(cmdbuf, vk::ImageLayout::GENERAL);

                self.chunk_init_ppl.record_bind(cmdbuf);
                self.chunk_init_ppl.record_bind_descriptor_sets(
                    cmdbuf,
                    std::slice::from_ref(&self.chunk_init_ds),
                    0,
                );
                self.chunk_init_ppl
                    .record_dispatch(cmdbuf, voxel_dim.to_array());
            },
        );

        fn update_uniforms(resources: &Resources, chunk_pos: UVec3) {
            let data = BufferBuilder::from_struct_buffer(resources.chunk_init_info())
                .unwrap()
                .set_uvec3("chunk_pos", chunk_pos.to_array())
                .to_raw_data();
            resources
                .chunk_init_info()
                .fill_with_raw_u8(&data)
                .expect("Failed to fill buffer data");
        }
    }
}
