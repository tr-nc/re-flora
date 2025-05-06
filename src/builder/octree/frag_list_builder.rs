use crate::builder::plain::Resources as PlainResources;
use crate::util::ShaderCompiler;
use crate::vkn::Allocator;
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

use super::Resources;

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
    free_atlas_cmdbuf: CommandBuffer,
}

impl FragListBuilder {
    pub fn new(
        vulkan_ctx: &VulkanContext,
        shader_compiler: &ShaderCompiler,
        descriptor_pool: DescriptorPool,
        plain_resources: &PlainResources,
        allocator: &Allocator,
        voxel_dim: UVec3,
        visible_chunk_dim: UVec3,
        octree_buffer_size: u64,
    ) -> Self {
        let init_buffers_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/builder/frag_list_builder/init_buffers.comp",
            "main",
        )
        .unwrap();
        let frag_list_maker_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/builder/frag_list_builder/frag_list_maker.comp",
            "main",
        )
        .unwrap();
        let octree_init_buffers_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/builder/frag_list_builder/octree_init_buffers.comp",
            "main",
        )
        .unwrap();
        let tracer_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/builder/frag_list_builder/tracer.comp",
            "main",
        )
        .unwrap();

        let resources = Resources::new(
            vulkan_ctx.device().clone(),
            allocator.clone(),
            voxel_dim,
            visible_chunk_dim,
            octree_buffer_size,
            &init_buffers_sm,
            &frag_list_maker_sm,
            &octree_init_buffers_sm,
            &tracer_sm,
        );

        let init_buffers_ppl =
            ComputePipeline::from_shader_module(vulkan_ctx.device(), &init_buffers_sm);
        let init_buffers_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &init_buffers_ppl.get_layout().get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        init_buffers_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.frag_list_maker_info),
            WriteDescriptorSet::new_buffer_write(1, &resources.voxel_dim_indirect),
            WriteDescriptorSet::new_buffer_write(2, &resources.frag_list_build_result),
        ]);

        let frag_list_maker_ppl =
            ComputePipeline::from_shader_module(vulkan_ctx.device(), &frag_list_maker_sm);
        let frag_list_maker_chunk_atlas_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &frag_list_maker_ppl
                .get_layout()
                .get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        frag_list_maker_chunk_atlas_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.frag_list_maker_info),
            WriteDescriptorSet::new_buffer_write(1, &resources.frag_list_build_result),
            WriteDescriptorSet::new_buffer_write(2, &resources.fragment_list),
            WriteDescriptorSet::new_texture_write(
                3,
                vk::DescriptorType::STORAGE_IMAGE,
                &plain_resources.chunk_atlas,
                vk::ImageLayout::GENERAL,
            ),
        ]);
        //
        let frag_list_maker_free_atlas_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &frag_list_maker_ppl
                .get_layout()
                .get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        frag_list_maker_free_atlas_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.frag_list_maker_info),
            WriteDescriptorSet::new_buffer_write(1, &resources.frag_list_build_result),
            WriteDescriptorSet::new_buffer_write(2, &resources.fragment_list),
            WriteDescriptorSet::new_texture_write(
                3,
                vk::DescriptorType::STORAGE_IMAGE,
                &plain_resources.free_atlas,
                vk::ImageLayout::GENERAL,
            ),
        ]);

        let cmdbuf_chunk_atlas = Self::record_cmdbuf(
            vulkan_ctx,
            &resources,
            &init_buffers_ppl,
            &frag_list_maker_ppl,
            &init_buffers_ds,
            &frag_list_maker_chunk_atlas_ds,
        );

        let free_atlas_cmdbuf = Self::record_cmdbuf(
            vulkan_ctx,
            &resources,
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
            free_atlas_cmdbuf,
        }
    }

    fn record_cmdbuf(
        vulkan_ctx: &VulkanContext,
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

        let device = vulkan_ctx.device();

        let cmdbuf = CommandBuffer::new(device, vulkan_ctx.command_pool());
        cmdbuf.begin(false);

        init_buffers_ppl.record_bind(&cmdbuf);
        init_buffers_ppl.record_bind_descriptor_sets(
            &cmdbuf,
            std::slice::from_ref(init_buffers_ds),
            0,
        );
        init_buffers_ppl.record_dispatch(&cmdbuf, [1, 1, 1]);

        shader_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);
        indirect_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);

        frag_list_maker_ppl.record_bind(&cmdbuf);
        frag_list_maker_ppl.record_bind_descriptor_sets(
            &cmdbuf,
            std::slice::from_ref(frag_list_maker_ds),
            0,
        );
        frag_list_maker_ppl.record_dispatch_indirect(&cmdbuf, &resources.voxel_dim_indirect);

        cmdbuf.end();
        return cmdbuf;
    }

    pub fn build(
        &self,
        build_type: FragListBuildType,
        vulkan_ctx: &VulkanContext,
        resources: &Resources,
        atlas_read_offset: UVec3,
        atlas_read_dim: UVec3,
        is_crossing_boundary: bool,
    ) {
        let device = vulkan_ctx.device();

        update_buffers(
            resources,
            atlas_read_offset,
            atlas_read_dim,
            is_crossing_boundary,
        );

        let cmdbuf = match build_type {
            FragListBuildType::ChunkAtlas => &self.cmdbuf_chunk_atlas,
            FragListBuildType::FreeAtlas => &self.free_atlas_cmdbuf,
        };
        cmdbuf.submit(&vulkan_ctx.get_general_queue(), None);
        device.wait_queue_idle(&vulkan_ctx.get_general_queue());

        fn update_buffers(
            resources: &Resources,
            atlas_read_offset: UVec3,
            atlas_read_dim: UVec3,
            is_crossing_boundary: bool,
        ) {
            let data = StructMemberDataBuilder::from_buffer(&resources.frag_list_maker_info)
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
                .frag_list_maker_info
                .fill_with_raw_u8(&data)
                .unwrap();
        }
    }

    pub fn get_fraglist_length(&self, resources: &Resources) -> u32 {
        let layout = &resources
            .frag_list_build_result
            .get_layout()
            .unwrap()
            .root_member;
        let raw_data = resources.frag_list_build_result.read_back().unwrap();
        let reader = StructMemberDataReader::new(layout, &raw_data);
        let field_val = reader.get_field("fragment_list_len").unwrap();
        if let PlainMemberTypeWithData::UInt(val) = field_val {
            val
        } else {
            panic!("Expected UInt type for fragment_list_len")
        }
    }
}
