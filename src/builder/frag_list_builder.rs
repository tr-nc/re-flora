use super::Resources;
use crate::util::ShaderCompiler;
use crate::vkn::BufferBuilder;
use crate::vkn::CommandBuffer;
use crate::vkn::ComputePipeline;
use crate::vkn::DescriptorPool;
use crate::vkn::DescriptorSet;
use crate::vkn::MemoryBarrier;
use crate::vkn::PipelineBarrier;
use crate::vkn::ShaderModule;
use crate::vkn::VulkanContext;
use crate::vkn::WriteDescriptorSet;
use ash::vk;
use glam::UVec3;

pub struct FragListBuilder {
    #[allow(dead_code)]
    init_buffers_ppl: ComputePipeline,
    #[allow(dead_code)]
    frag_list_maker_ppl: ComputePipeline,

    #[allow(dead_code)]
    init_buffers_ds: DescriptorSet,
    #[allow(dead_code)]
    frag_list_maker_ds: DescriptorSet,

    cmdbuf: CommandBuffer,
}

impl FragListBuilder {
    pub fn new(
        vulkan_context: &VulkanContext,
        shader_compiler: &ShaderCompiler,
        descriptor_pool: DescriptorPool,
        resources: &Resources,
    ) -> Self {
        let init_buffers_sm = ShaderModule::from_glsl(
            vulkan_context.device(),
            &shader_compiler,
            "shader/builder/frag_list_builder/init_buffers.comp",
            "main",
        )
        .unwrap();
        let init_buffers_ppl =
            ComputePipeline::from_shader_module(vulkan_context.device(), &init_buffers_sm);
        let init_buffers_ds = DescriptorSet::new(
            vulkan_context.device().clone(),
            &init_buffers_ppl.get_layout().get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        init_buffers_ds.perform_writes(&[
            WriteDescriptorSet::new_buffer_write(0, resources.voxel_dim_indirect()),
            WriteDescriptorSet::new_buffer_write(1, resources.frag_list_build_result()),
        ]);

        let frag_list_maker_sm = ShaderModule::from_glsl(
            vulkan_context.device(),
            &shader_compiler,
            "shader/builder/frag_list_builder/frag_list_maker.comp",
            "main",
        )
        .unwrap();
        let frag_list_maker_ppl =
            ComputePipeline::from_shader_module(vulkan_context.device(), &frag_list_maker_sm);
        let frag_list_maker_ds = DescriptorSet::new(
            vulkan_context.device().clone(),
            &frag_list_maker_ppl
                .get_layout()
                .get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        frag_list_maker_ds.perform_writes(&[
            WriteDescriptorSet::new_buffer_write(0, resources.frag_list_maker_info()),
            WriteDescriptorSet::new_buffer_write(1, resources.frag_list_build_result()),
            WriteDescriptorSet::new_buffer_write(2, resources.fragment_list()),
            WriteDescriptorSet::new_texture_write(
                3,
                vk::DescriptorType::STORAGE_IMAGE,
                resources.raw_atlas_tex(),
                vk::ImageLayout::GENERAL,
            ),
        ]);

        let cmdbuf = Self::create_cmdbuf(
            vulkan_context,
            resources,
            &init_buffers_ppl,
            &frag_list_maker_ppl,
            &init_buffers_ds,
            &frag_list_maker_ds,
        );

        Self {
            cmdbuf,
            init_buffers_ppl,
            frag_list_maker_ppl,
            init_buffers_ds,
            frag_list_maker_ds,
        }
    }

    fn create_cmdbuf(
        vulkan_context: &VulkanContext,
        resources: &Resources,
        init_buffers_ppl: &ComputePipeline,
        frag_list_maker_ppl: &ComputePipeline,
        init_buffers_ds: &DescriptorSet,
        frag_list_maker_ds: &DescriptorSet,
    ) -> CommandBuffer {
        let shader_access_memory_barrier = MemoryBarrier::new_shader_access();
        let indirect_access_memory_barrier = MemoryBarrier::new_indirect_access();

        let shader_access_pipeline_barrier = PipelineBarrier::new(
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vec![shader_access_memory_barrier],
        );
        let indirect_access_pipeline_barrier = PipelineBarrier::new(
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::DRAW_INDIRECT | vk::PipelineStageFlags::COMPUTE_SHADER,
            vec![indirect_access_memory_barrier],
        );

        //

        let device = vulkan_context.device();

        let cmdbuf = CommandBuffer::new(device, vulkan_context.command_pool());
        cmdbuf.begin(false);

        init_buffers_ppl.record_bind(&cmdbuf);
        init_buffers_ppl.record_bind_descriptor_sets(
            &cmdbuf,
            std::slice::from_ref(init_buffers_ds),
            0,
        );
        init_buffers_ppl.record_dispatch(&cmdbuf, [1, 1, 1]);

        shader_access_pipeline_barrier.record_insert(vulkan_context.device(), &cmdbuf);
        indirect_access_pipeline_barrier.record_insert(vulkan_context.device(), &cmdbuf);

        frag_list_maker_ppl.record_bind(&cmdbuf);
        frag_list_maker_ppl.record_bind_descriptor_sets(
            &cmdbuf,
            std::slice::from_ref(frag_list_maker_ds),
            0,
        );
        frag_list_maker_ppl.record_dispatch_indirect(&cmdbuf, resources.voxel_dim_indirect());

        cmdbuf.end();
        cmdbuf
    }

    pub fn build(&self, vulkan_context: &VulkanContext, resources: &Resources, chunk_pos: UVec3) {
        let device = vulkan_context.device();

        Self::update_uniforms(resources, chunk_pos);

        self.cmdbuf
            .submit(&vulkan_context.get_general_queue(), None);
        device.wait_queue_idle(&vulkan_context.get_general_queue());
    }

    fn update_uniforms(resources: &Resources, chunk_pos: UVec3) {
        let data = BufferBuilder::from_struct_buffer(resources.frag_list_maker_info())
            .unwrap()
            .set_uvec3("chunk_pos", chunk_pos.to_array())
            .to_raw_data();
        resources
            .frag_list_maker_info()
            .fill_with_raw_u8(&data)
            .expect("Failed to fill buffer data");
    }

    pub fn get_fraglist_length(&self, resources: &Resources) -> u32 {
        let raw_data = resources.frag_list_build_result().fetch_raw().unwrap();
        BufferBuilder::from_struct_buffer(resources.frag_list_build_result())
            .unwrap()
            .set_raw(raw_data)
            .get_uint("fragment_list_len")
            .unwrap()
    }
}
