use super::BuilderResources;
use crate::util::compiler::ShaderCompiler;
use crate::vkn::Allocator;
use crate::vkn::ComputePipeline;
use crate::vkn::DescriptorPool;
use crate::vkn::DescriptorSet;
use crate::vkn::ShaderModule;
use crate::vkn::VulkanContext;
use crate::vkn::WriteDescriptorSet;
use ash::vk;
use glam::UVec3;

pub struct Builder {
    vulkan_context: VulkanContext,

    allocator: Allocator,
    resources: BuilderResources,

    no_of_chunks: UVec3,

    chunk_init_sm: ShaderModule,
    chunk_init_ppl: ComputePipeline,
    chunk_init_ds: DescriptorSet,
    descriptor_pool: DescriptorPool,
}

impl Builder {
    pub fn new(
        vulkan_context: VulkanContext,
        allocator: Allocator,
        shader_compiler: &ShaderCompiler,
        chunk_res: UVec3,
        no_of_chunks: UVec3,
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
            no_of_chunks,
            chunk_init_sm,
            chunk_init_ppl,
            chunk_init_ds,
            descriptor_pool,
        }
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
            WriteDescriptorSet::new_buffer_write(
                1,
                vk::DescriptorType::STORAGE_BUFFER,
                &resources.weight_data_buf,
            ),
        ]);
        compute_descriptor_set
    }
}
