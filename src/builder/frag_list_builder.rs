use super::Resources;
use crate::util::ShaderCompiler;
use crate::vkn::CommandBuffer;
use crate::vkn::ComputePipeline;
use crate::vkn::DescriptorPool;
use crate::vkn::DescriptorSet;
use crate::vkn::MemoryBarrier;
use crate::vkn::PipelineBarrier;
use crate::vkn::PlainMemberTypeWithData;
use crate::vkn::ShaderModule;
use crate::vkn::StructMemberDataBuilder;
use crate::vkn::StructMemberDataReader;
use crate::vkn::VulkanContext;
use crate::vkn::WriteDescriptorSet;
use ash::vk;
use glam::UVec3;

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum FragListBuildType {
    ChunkAtlas,
    FreeAtlas,
}

#[allow(dead_code)]
pub struct FragListBuilder {
    init_buffers_ppl: ComputePipeline,
    frag_list_maker_ppl: ComputePipeline,

    init_buffers_ds: DescriptorSet,
    frag_list_maker_chunk_atlas_ds: DescriptorSet,
    frag_list_maker_free_atlas_ds: DescriptorSet,

    cmdbuf_chunk_atlas: CommandBuffer,
    cmdbuf_free_atlas: CommandBuffer,
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
            WriteDescriptorSet::new_buffer_write(0, resources.frag_list_maker_info()),
            WriteDescriptorSet::new_buffer_write(1, resources.voxel_dim_indirect()),
            WriteDescriptorSet::new_buffer_write(2, resources.frag_list_build_result()),
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
        let frag_list_maker_chunk_atlas_ds = DescriptorSet::new(
            vulkan_context.device().clone(),
            &frag_list_maker_ppl
                .get_layout()
                .get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        frag_list_maker_chunk_atlas_ds.perform_writes(&[
            WriteDescriptorSet::new_buffer_write(0, resources.frag_list_maker_info()),
            WriteDescriptorSet::new_buffer_write(1, resources.frag_list_build_result()),
            WriteDescriptorSet::new_buffer_write(2, resources.fragment_list()),
            WriteDescriptorSet::new_texture_write(
                3,
                vk::DescriptorType::STORAGE_IMAGE,
                resources.chunk_atlas(),
                vk::ImageLayout::GENERAL,
            ),
        ]);
        //
        let frag_list_maker_free_atlas_ds = DescriptorSet::new(
            vulkan_context.device().clone(),
            &frag_list_maker_ppl
                .get_layout()
                .get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        frag_list_maker_free_atlas_ds.perform_writes(&[
            WriteDescriptorSet::new_buffer_write(0, resources.frag_list_maker_info()),
            WriteDescriptorSet::new_buffer_write(1, resources.frag_list_build_result()),
            WriteDescriptorSet::new_buffer_write(2, resources.fragment_list()),
            WriteDescriptorSet::new_texture_write(
                3,
                vk::DescriptorType::STORAGE_IMAGE,
                resources.free_atlas(),
                vk::ImageLayout::GENERAL,
            ),
        ]);

        let cmdbuf_chunk_atlas = Self::create_cmdbuf(
            vulkan_context,
            resources,
            &init_buffers_ppl,
            &frag_list_maker_ppl,
            &init_buffers_ds,
            &frag_list_maker_chunk_atlas_ds,
        );

        let cmdbuf_free_atlas = Self::create_cmdbuf(
            vulkan_context,
            resources,
            &init_buffers_ppl,
            &frag_list_maker_ppl,
            &init_buffers_ds,
            &frag_list_maker_free_atlas_ds,
        );

        Self {
            init_buffers_ppl,
            frag_list_maker_ppl,
            init_buffers_ds,
            frag_list_maker_chunk_atlas_ds,
            frag_list_maker_free_atlas_ds,

            cmdbuf_chunk_atlas,
            cmdbuf_free_atlas,
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

    pub fn build(
        &self,
        build_type: FragListBuildType,
        vulkan_context: &VulkanContext,
        resources: &Resources,
        atlas_read_offset: UVec3,
        atlas_read_dim: UVec3,
        is_crossing_boundary: bool,
    ) {
        let device = vulkan_context.device();

        update_buffers(
            resources,
            atlas_read_offset,
            atlas_read_dim,
            is_crossing_boundary,
        );

        let cmdbuf = match build_type {
            FragListBuildType::ChunkAtlas => &self.cmdbuf_chunk_atlas,
            FragListBuildType::FreeAtlas => &self.cmdbuf_free_atlas,
        };
        cmdbuf.submit(&vulkan_context.get_general_queue(), None);
        device.wait_queue_idle(&vulkan_context.get_general_queue());

        fn update_buffers(
            resources: &Resources,
            atlas_read_offset: UVec3,
            atlas_read_dim: UVec3,
            is_crossing_boundary: bool,
        ) {
            let data = StructMemberDataBuilder::from_buffer(resources.frag_list_maker_info())
                .set_field(
                    "atlas_read_offset",
                    PlainMemberTypeWithData::UVec3(atlas_read_offset.to_array()),
                )
                .unwrap()
                .set_field(
                    "atlas_read_dim",
                    PlainMemberTypeWithData::UVec3(atlas_read_dim.to_array()),
                )
                .unwrap()
                .set_field(
                    "is_crossing_boundary",
                    PlainMemberTypeWithData::UInt(if is_crossing_boundary { 1 } else { 0 }),
                )
                .unwrap()
                .get_data_u8();
            resources
                .frag_list_maker_info()
                .fill_with_raw_u8(&data)
                .unwrap();
        }
    }

    pub fn get_fraglist_length(&self, resources: &Resources) -> u32 {
        let layout = &resources
            .frag_list_build_result()
            .get_layout()
            .unwrap()
            .root_member;
        let raw_data = resources.frag_list_build_result().fetch_raw().unwrap();
        let reader = StructMemberDataReader::new(layout, &raw_data);
        let field_val = reader.get_field("fragment_list_len").unwrap();
        if let PlainMemberTypeWithData::UInt(val) = field_val {
            val
        } else {
            panic!("Expected UInt type for fragment_list_len")
        }
    }
}
