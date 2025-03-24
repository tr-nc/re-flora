use std::collections::HashMap;

use super::BuilderResources;
use super::Chunk;
use crate::util::compiler::ShaderCompiler;
use crate::vkn::execute_one_time_command;
use crate::vkn::Allocator;
use crate::vkn::BufferBuilder;
use crate::vkn::CommandPool;
use crate::vkn::ComputePipeline;
use crate::vkn::DescriptorPool;
use crate::vkn::DescriptorSet;
use crate::vkn::ShaderModule;
use crate::vkn::VulkanContext;
use crate::vkn::WriteDescriptorSet;
use ash::vk;
use glam::IVec3;
use glam::UVec3;

pub struct Builder {
    vulkan_context: VulkanContext,

    allocator: Allocator,
    resources: BuilderResources,

    chunk_init_sm: ShaderModule,
    chunk_init_ppl: ComputePipeline,
    chunk_init_ds: DescriptorSet,
    descriptor_pool: DescriptorPool,

    chunk_res: UVec3,
    chunks: HashMap<IVec3, Chunk>,
}

impl Builder {
    pub fn new(
        vulkan_context: VulkanContext,
        allocator: Allocator,
        shader_compiler: &ShaderCompiler,
        chunk_res: UVec3,
    ) -> Self {
        if chunk_res.x != chunk_res.y || chunk_res.y != chunk_res.z {
            log::error!("Resolution must be equal in all dimensions");
        }
        if chunk_res.x & (chunk_res.x - 1) != 0 {
            log::error!("Resolution must be a power of 2");
        }

        let chunk_init_sm = ShaderModule::from_glsl(
            vulkan_context.device(),
            &shader_compiler,
            "shader/builder/chunk_init.comp",
            "main",
        )
        .unwrap();
        let chunk_init_ppl =
            ComputePipeline::from_shader_module(vulkan_context.device(), &chunk_init_sm);

        // reuse the descriptor pool later.
        let descriptor_pool = DescriptorPool::from_descriptor_set_layouts(
            vulkan_context.device(),
            chunk_init_ppl
                .get_pipeline_layout()
                .get_descriptor_set_layouts(),
        )
        .unwrap();

        let resources = BuilderResources::new(
            vulkan_context.device().clone(),
            allocator.clone(),
            &chunk_init_sm,
            chunk_res,
        );

        let chunk_init_ds = Self::create_chunk_init_descriptor_set(
            descriptor_pool.clone(),
            &vulkan_context,
            &chunk_init_ppl,
            &resources,
        );

        Self {
            vulkan_context,
            allocator,
            resources,
            chunk_init_sm,
            chunk_init_ppl,
            chunk_init_ds,
            descriptor_pool,
            chunk_res,
            chunks: HashMap::new(),
        }
    }

    pub fn build(&mut self, command_pool: &CommandPool, chunk_pos: IVec3) {
        let chunk_data = self.generate_chunk_data_gpu(command_pool, self.chunk_res, chunk_pos);

        // debug the chunkdata
        log::debug!("Chunk data: {:?}", chunk_data);

        let chunk = Chunk {
            res: self.chunk_res,
            pos: chunk_pos,
            data: chunk_data,
        };
        self.chunks.insert(chunk_pos, chunk);
    }

    fn create_chunk_init_descriptor_set(
        descriptor_pool: DescriptorPool,
        vulkan_context: &VulkanContext,
        compute_pipeline: &ComputePipeline,
        resources: &BuilderResources,
    ) -> DescriptorSet {
        let compute_descriptor_set = DescriptorSet::new(
            vulkan_context.device().clone(),
            compute_pipeline
                .get_pipeline_layout()
                .get_descriptor_set_layouts(),
            descriptor_pool,
        );
        compute_descriptor_set.perform_writes(&[
            WriteDescriptorSet::new_buffer_write(
                0,
                vk::DescriptorType::UNIFORM_BUFFER,
                &resources.chunk_build_info_buf,
            ),
            WriteDescriptorSet::new_texture_write(
                1,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.weight_tex,
                vk::ImageLayout::GENERAL,
            ),
        ]);
        compute_descriptor_set
    }

    fn generate_chunk_data_gpu(
        &mut self,
        command_pool: &CommandPool,
        resolution: UVec3,
        chunk_pos: IVec3,
    ) -> Vec<u8> {
        // modify the uniform buffer to guide the chunk generation
        let chunk_build_info_layout = BufferBuilder::from_layout(
            self.chunk_init_sm
                .get_buffer_layout("ChunkBuildInfo")
                .unwrap(),
        );
        let chunk_build_info_data = chunk_build_info_layout
            .set_uvec3("chunk_res", resolution.to_array())
            .set_ivec3("chunk_pos", chunk_pos.to_array())
            .build();
        self.resources
            .chunk_build_info_buf
            .fill_raw(&chunk_build_info_data)
            .expect("Failed to fill buffer data");

        execute_one_time_command(
            self.vulkan_context.device(),
            command_pool,
            &self.vulkan_context.get_general_queue(),
            |cmdbuf| {
                self.resources
                    .weight_tex
                    .get_image()
                    .record_transition_barrier(cmdbuf, vk::ImageLayout::GENERAL);

                self.chunk_init_ppl.record_bind(cmdbuf);
                self.chunk_init_ppl.record_bind_descriptor_sets(
                    cmdbuf,
                    std::slice::from_ref(&self.chunk_init_ds),
                    0,
                );
                self.chunk_init_ppl
                    .record_dispatch(cmdbuf, resolution.to_array());
            },
        );

        // self.resources
        //     .weight_data_buf
        //     .fetch_raw()
        //     .expect("Failed to fetch buffer data")

        vec![]
    }
}
