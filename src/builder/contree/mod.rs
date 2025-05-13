mod resources;
pub use resources::*;

use std::collections::HashMap;
use std::time::Instant;

use crate::builder::PlainBuilderResources;
use crate::util::AllocationStrategy;
use crate::util::FirstFitAllocator;
use crate::util::ShaderCompiler;
use crate::util::BENCH;
use crate::vkn::execute_one_time_command;
use crate::vkn::Allocator;
use crate::vkn::Buffer;
use crate::vkn::ClearValue;
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

const SIZE_OF_NODE_ELEMENT: u64 = 3 * std::mem::size_of::<u32>() as u64;
const SIZE_OF_LEAF_ELEMENT: u64 = 1 * std::mem::size_of::<u32>() as u64;

pub struct ContreeBuilder {
    vulkan_ctx: VulkanContext,
    resources: ContreeBuilderResources,

    frag_img_buffer_setup_ppl: ComputePipeline,
    frag_img_maker_ppl: ComputePipeline,
    contree_buffer_setup_ppl: ComputePipeline,
    contree_leaf_write_ppl: ComputePipeline,
    contree_tree_write_ppl: ComputePipeline,
    contree_buffer_update_ppl: ComputePipeline,
    contree_last_buffer_update_ppl: ComputePipeline,
    contree_concat_ppl: ComputePipeline,

    frag_img_buffer_setup_ds: DescriptorSet,
    frag_img_maker_ds: DescriptorSet,
    contree_buffer_setup_ds: DescriptorSet,
    contree_leaf_write_ds: DescriptorSet,
    contree_tree_write_ds: DescriptorSet,
    contree_buffer_update_ds: DescriptorSet,
    contree_last_buffer_update_ds: DescriptorSet,
    contree_concat_ds: DescriptorSet,

    /// Atlas offset <-> (node_alloc_id, leaf_alloc_id)
    chunk_offset_allocation_table: HashMap<UVec3, (u64, u64)>,

    // cmdbuf_table: HashMap<u32, CommandBuffer>,
    frag_img_cmdbuf: CommandBuffer,
    contree_cmdbuf: CommandBuffer,

    leaf_allocator: FirstFitAllocator,
    node_allocator: FirstFitAllocator,
}

impl ContreeBuilder {
    pub fn new(
        vulkan_ctx: VulkanContext,
        allocator: Allocator,
        shader_compiler: &ShaderCompiler,
        plain_builder_resources: &PlainBuilderResources,
        voxel_dim_per_chunk: UVec3,
        node_pool_size_in_bytes: u64,
        leaf_pool_size_in_bytes: u64,
    ) -> Self {
        let descriptor_pool = DescriptorPool::a_big_one(vulkan_ctx.device()).unwrap();

        let frag_img_buffer_setup_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/builder/frag_img_builder/buffer_setup.comp",
            "main",
        )
        .unwrap();
        let frag_img_maker_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/builder/frag_img_builder/frag_img_maker.comp",
            "main",
        )
        .unwrap();
        let contree_buffer_setup_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/builder/contree/buffer_setup.comp",
            "main",
        )
        .unwrap();
        let contree_leaf_write_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/builder/contree/leaf_write.comp",
            "main",
        )
        .unwrap();
        let contree_tree_write_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/builder/contree/tree_write.comp",
            "main",
        )
        .unwrap();
        let contree_buffer_update_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/builder/contree/buffer_update.comp",
            "main",
        )
        .unwrap();
        let contree_last_buffer_update_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/builder/contree/last_buffer_update.comp",
            "main",
        )
        .unwrap();
        let contree_concat_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/builder/contree/concat.comp",
            "main",
        )
        .unwrap();

        let resources = ContreeBuilderResources::new(
            vulkan_ctx.device().clone(),
            allocator.clone(),
            voxel_dim_per_chunk,
            node_pool_size_in_bytes,
            leaf_pool_size_in_bytes,
            &frag_img_buffer_setup_sm,
            &contree_buffer_setup_sm,
            &contree_leaf_write_sm,
            &contree_tree_write_sm,
            &contree_last_buffer_update_sm,
        );

        let frag_img_buffer_setup_ppl =
            ComputePipeline::from_shader_module(vulkan_ctx.device(), &frag_img_buffer_setup_sm);
        let frag_img_maker_ppl =
            ComputePipeline::from_shader_module(vulkan_ctx.device(), &frag_img_maker_sm);
        let contree_buffer_setup_ppl =
            ComputePipeline::from_shader_module(vulkan_ctx.device(), &contree_buffer_setup_sm);
        let contree_leaf_write_ppl =
            ComputePipeline::from_shader_module(vulkan_ctx.device(), &contree_leaf_write_sm);
        let contree_tree_write_ppl =
            ComputePipeline::from_shader_module(vulkan_ctx.device(), &contree_tree_write_sm);
        let contree_buffer_update_ppl =
            ComputePipeline::from_shader_module(vulkan_ctx.device(), &contree_buffer_update_sm);
        let contree_last_buffer_update_ppl = ComputePipeline::from_shader_module(
            vulkan_ctx.device(),
            &contree_last_buffer_update_sm,
        );
        let contree_concat_ppl =
            ComputePipeline::from_shader_module(vulkan_ctx.device(), &contree_concat_sm);

        let frag_img_buffer_setup_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &frag_img_buffer_setup_ppl
                .get_layout()
                .get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        frag_img_buffer_setup_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.frag_img_maker_info),
            WriteDescriptorSet::new_buffer_write(1, &resources.voxel_dim_indirect),
            WriteDescriptorSet::new_buffer_write(2, &resources.frag_img_build_result),
        ]);
        let frag_img_maker_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &frag_img_maker_ppl.get_layout().get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        frag_img_maker_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.frag_img_maker_info),
            WriteDescriptorSet::new_buffer_write(1, &resources.frag_img_build_result),
            WriteDescriptorSet::new_texture_write(
                2,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.frag_img,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                3,
                vk::DescriptorType::STORAGE_IMAGE,
                &plain_builder_resources.chunk_atlas,
                vk::ImageLayout::GENERAL,
            ),
        ]);
        let contree_buffer_setup_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &contree_buffer_setup_ppl
                .get_layout()
                .get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        contree_buffer_setup_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.contree_build_info),
            WriteDescriptorSet::new_buffer_write(1, &resources.contree_build_state),
            WriteDescriptorSet::new_buffer_write(2, &resources.level_dispatch_indirect),
            WriteDescriptorSet::new_buffer_write(3, &resources.counter_for_levels),
            WriteDescriptorSet::new_buffer_write(4, &resources.node_offset_for_levels),
            WriteDescriptorSet::new_buffer_write(5, &resources.contree_build_result),
        ]);
        let contree_leaf_write_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &contree_leaf_write_ppl
                .get_layout()
                .get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        contree_leaf_write_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.contree_build_info),
            WriteDescriptorSet::new_buffer_write(1, &resources.contree_build_state),
            WriteDescriptorSet::new_texture_write(
                2,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.frag_img,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_buffer_write(3, &resources.node_offset_for_levels),
            WriteDescriptorSet::new_buffer_write(4, &resources.sparse_nodes),
            WriteDescriptorSet::new_buffer_write(5, &resources.leaf_data),
            WriteDescriptorSet::new_buffer_write(6, &resources.contree_build_result),
        ]);
        let contree_tree_write_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &contree_tree_write_ppl
                .get_layout()
                .get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        contree_tree_write_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.contree_build_state),
            WriteDescriptorSet::new_buffer_write(1, &resources.node_offset_for_levels),
            WriteDescriptorSet::new_buffer_write(2, &resources.sparse_nodes),
            WriteDescriptorSet::new_buffer_write(3, &resources.dense_nodes),
            WriteDescriptorSet::new_buffer_write(4, &resources.counter_for_levels),
            WriteDescriptorSet::new_buffer_write(5, &resources.contree_build_result),
        ]);
        let contree_buffer_update_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &contree_buffer_update_ppl
                .get_layout()
                .get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        contree_buffer_update_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.contree_build_state),
            WriteDescriptorSet::new_buffer_write(1, &resources.level_dispatch_indirect),
        ]);
        let contree_last_buffer_update_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &contree_last_buffer_update_ppl
                .get_layout()
                .get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        contree_last_buffer_update_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.contree_build_result),
            WriteDescriptorSet::new_buffer_write(1, &resources.concat_dispatch_indirect),
            WriteDescriptorSet::new_buffer_write(2, &resources.sparse_nodes),
            WriteDescriptorSet::new_buffer_write(3, &resources.dense_nodes),
            WriteDescriptorSet::new_buffer_write(4, &resources.counter_for_levels),
        ]);
        let contree_concat_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &contree_concat_ppl.get_layout().get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        contree_concat_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.contree_build_info),
            WriteDescriptorSet::new_buffer_write(1, &resources.node_offset_for_levels),
            WriteDescriptorSet::new_buffer_write(2, &resources.dense_nodes),
            WriteDescriptorSet::new_buffer_write(3, &resources.counter_for_levels),
            WriteDescriptorSet::new_buffer_write(4, &resources.node_data),
            WriteDescriptorSet::new_buffer_write(5, &resources.contree_build_result),
        ]);

        let frag_img_cmdbuf = record_frag_img_cmdbuf(
            &vulkan_ctx,
            &resources,
            &frag_img_buffer_setup_ppl,
            &frag_img_maker_ppl,
            &frag_img_buffer_setup_ds,
            &frag_img_maker_ds,
        );

        let contree_cmdbuf = record_contree_cmdbuf(
            &vulkan_ctx,
            &resources,
            Self::get_level(voxel_dim_per_chunk),
            &contree_buffer_setup_ppl,
            &contree_leaf_write_ppl,
            &contree_tree_write_ppl,
            &contree_buffer_update_ppl,
            &contree_last_buffer_update_ppl,
            &contree_concat_ppl,
            &contree_buffer_setup_ds,
            &contree_leaf_write_ds,
            &contree_tree_write_ds,
            &contree_buffer_update_ds,
            &contree_last_buffer_update_ds,
            &contree_concat_ds,
        );

        init_atlas_images(&vulkan_ctx, &resources);

        let node_allocator = FirstFitAllocator::new(node_pool_size_in_bytes);
        let leaf_allocator = FirstFitAllocator::new(leaf_pool_size_in_bytes);

        return Self {
            vulkan_ctx,
            resources,

            frag_img_buffer_setup_ppl,
            frag_img_maker_ppl,
            contree_buffer_setup_ppl,
            contree_leaf_write_ppl,
            contree_tree_write_ppl,
            contree_buffer_update_ppl,
            contree_last_buffer_update_ppl,
            contree_concat_ppl,

            frag_img_buffer_setup_ds,
            frag_img_maker_ds,
            contree_buffer_setup_ds,
            contree_leaf_write_ds,
            contree_tree_write_ds,
            contree_buffer_update_ds,
            contree_last_buffer_update_ds,
            contree_concat_ds,

            chunk_offset_allocation_table: HashMap::new(),

            frag_img_cmdbuf,
            contree_cmdbuf,

            node_allocator,
            leaf_allocator,
        };

        fn init_atlas_images(vulkan_context: &VulkanContext, resources: &ContreeBuilderResources) {
            execute_one_time_command(
                vulkan_context.device(),
                vulkan_context.command_pool(),
                &vulkan_context.get_general_queue(),
                |cmdbuf| {
                    resources.frag_img.get_image().record_clear(
                        cmdbuf,
                        Some(vk::ImageLayout::GENERAL),
                        ClearValue::UInt([0, 0, 0, 0]),
                    );
                },
            );
        }

        fn record_frag_img_cmdbuf(
            vulkan_ctx: &VulkanContext,
            resources: &ContreeBuilderResources,
            buffer_setup_ppl: &ComputePipeline,
            frag_img_maker_ppl: &ComputePipeline,
            buffer_setup_ds: &DescriptorSet,
            frag_img_maker_ds: &DescriptorSet,
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

            buffer_setup_ppl.record_bind(&cmdbuf);
            buffer_setup_ppl.record_bind_descriptor_sets(
                &cmdbuf,
                std::slice::from_ref(buffer_setup_ds),
                0,
            );
            buffer_setup_ppl.record_dispatch(&cmdbuf, [1, 1, 1]);

            shader_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);
            indirect_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);

            frag_img_maker_ppl.record_bind(&cmdbuf);
            frag_img_maker_ppl.record_bind_descriptor_sets(
                &cmdbuf,
                std::slice::from_ref(frag_img_maker_ds),
                0,
            );
            frag_img_maker_ppl.record_dispatch_indirect(&cmdbuf, &resources.voxel_dim_indirect);

            cmdbuf.end();
            return cmdbuf;
        }

        fn record_contree_cmdbuf(
            vulkan_ctx: &VulkanContext,
            resources: &ContreeBuilderResources,
            total_levels: u32,
            contree_buffer_setup_ppl: &ComputePipeline,
            contree_leaf_write_ppl: &ComputePipeline,
            contree_tree_write_ppl: &ComputePipeline,
            contree_buffer_update_ppl: &ComputePipeline,
            contree_last_buffer_update_ppl: &ComputePipeline,
            contree_concat_ppl: &ComputePipeline,
            contree_buffer_setup_ds: &DescriptorSet,
            contree_leaf_write_ds: &DescriptorSet,
            contree_tree_write_ds: &DescriptorSet,
            contree_buffer_update_ds: &DescriptorSet,
            contree_last_buffer_update_ds: &DescriptorSet,
            contree_concat_ds: &DescriptorSet,
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

            contree_buffer_setup_ppl.record_bind(&cmdbuf);
            contree_buffer_setup_ppl.record_bind_descriptor_sets(
                &cmdbuf,
                std::slice::from_ref(contree_buffer_setup_ds),
                0,
            );
            contree_buffer_setup_ppl.record_dispatch(&cmdbuf, [1, 1, 1]);

            shader_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);
            indirect_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);

            contree_leaf_write_ppl.record_bind(&cmdbuf);
            contree_leaf_write_ppl.record_bind_descriptor_sets(
                &cmdbuf,
                std::slice::from_ref(contree_leaf_write_ds),
                0,
            );
            contree_leaf_write_ppl
                .record_dispatch_indirect(&cmdbuf, &resources.level_dispatch_indirect);

            shader_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);

            contree_buffer_update_ppl.record_bind(&cmdbuf);
            contree_buffer_update_ppl.record_bind_descriptor_sets(
                &cmdbuf,
                std::slice::from_ref(contree_buffer_update_ds),
                0,
            );
            contree_buffer_update_ppl.record_dispatch(&cmdbuf, [1, 1, 1]);

            shader_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);
            indirect_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);

            for i in 0..(total_levels - 2) {
                contree_tree_write_ppl.record_bind(&cmdbuf);
                contree_tree_write_ppl.record_bind_descriptor_sets(
                    &cmdbuf,
                    std::slice::from_ref(contree_tree_write_ds),
                    0,
                );
                contree_tree_write_ppl
                    .record_dispatch_indirect(&cmdbuf, &resources.level_dispatch_indirect);

                shader_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);

                // not the last one
                if i != total_levels - 3 {
                    contree_buffer_update_ppl.record_bind(&cmdbuf);
                    contree_buffer_update_ppl.record_bind_descriptor_sets(
                        &cmdbuf,
                        std::slice::from_ref(contree_buffer_update_ds),
                        0,
                    );
                    contree_buffer_update_ppl.record_dispatch(&cmdbuf, [1, 1, 1]);
                } else {
                    contree_last_buffer_update_ppl.record_bind(&cmdbuf);
                    contree_last_buffer_update_ppl.record_bind_descriptor_sets(
                        &cmdbuf,
                        std::slice::from_ref(contree_last_buffer_update_ds),
                        0,
                    );
                    contree_last_buffer_update_ppl.record_dispatch(&cmdbuf, [1, 1, 1]);
                }

                shader_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);
                indirect_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);
            }

            contree_concat_ppl.record_bind(&cmdbuf);
            contree_concat_ppl.record_bind_descriptor_sets(
                &cmdbuf,
                std::slice::from_ref(contree_concat_ds),
                0,
            );
            contree_concat_ppl
                .record_dispatch_indirect(&cmdbuf, &resources.concat_dispatch_indirect);

            cmdbuf.end();
            return cmdbuf;
        }
    }

    /// Returns: (node_size_in_bytes, leaf_size_in_bytes)
    pub fn get_contree_size_info(&self, resources: &ContreeBuilderResources) -> (u64, u64) {
        let layout = &resources
            .contree_build_result
            .get_layout()
            .unwrap()
            .root_member;
        let raw_data = resources.contree_build_result.read_back().unwrap();
        let reader = StructMemberDataReader::new(layout, &raw_data);

        let leaf_len = reader.get_field("leaf_len").unwrap();
        let leaf_size_in_bytes = if let PlainMemberTypeWithData::UInt(val) = leaf_len {
            val as u64 * SIZE_OF_LEAF_ELEMENT
        } else {
            panic!("Expected UInt type for leaf_len")
        };

        let node_len = reader.get_field("node_len").unwrap();
        let node_size_in_bytes = if let PlainMemberTypeWithData::UInt(val) = node_len {
            val as u64 * SIZE_OF_NODE_ELEMENT
        } else {
            panic!("Expected UInt type for node_len")
        };

        return (node_size_in_bytes, leaf_size_in_bytes);
    }

    fn build_cmdbuf_for_level(
        &self,
        vulkan_context: &VulkanContext,
        resources: &ContreeBuilderResources,
        voxel_level: u32,
    ) -> CommandBuffer {
        // let shader_access_memory_barrier = MemoryBarrier::new_shader_access();
        // let indirect_access_memory_barrier = MemoryBarrier::new_indirect_access();

        // let shader_access_pipeline_barrier = PipelineBarrier::new(
        //     vk::PipelineStageFlags::COMPUTE_SHADER,
        //     vk::PipelineStageFlags::COMPUTE_SHADER,
        //     vec![shader_access_memory_barrier],
        // );
        // let indirect_access_pipeline_barrier = PipelineBarrier::new(
        //     vk::PipelineStageFlags::COMPUTE_SHADER,
        //     vk::PipelineStageFlags::DRAW_INDIRECT | vk::PipelineStageFlags::COMPUTE_SHADER,
        //     vec![indirect_access_memory_barrier],
        // );

        // //

        // let device = vulkan_context.device();

        // let cmdbuf = CommandBuffer::new(device, vulkan_context.command_pool());
        // cmdbuf.begin(false);

        // cmdbuf.end();
        // cmdbuf

        todo!();
    }

    fn build_frag_img(
        &self,
        resources: &ContreeBuilderResources,
        atlas_read_offset: UVec3,
        atlas_read_dim: UVec3,
    ) -> u32 {
        let device = self.vulkan_ctx.device();

        update_buffers(
            resources,
            atlas_read_offset,
            atlas_read_dim,
            true, // is_crossing_boundary,
        );

        self.frag_img_cmdbuf
            .submit(&self.vulkan_ctx.get_general_queue(), None);
        device.wait_queue_idle(&self.vulkan_ctx.get_general_queue());

        return get_active_voxel_len(&resources.frag_img_build_result);

        fn update_buffers(
            resources: &ContreeBuilderResources,
            atlas_read_offset: UVec3,
            atlas_read_dim: UVec3,
            is_crossing_boundary: bool,
        ) {
            let data = StructMemberDataBuilder::from_buffer(&resources.frag_img_maker_info)
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
                .frag_img_maker_info
                .fill_with_raw_u8(&data)
                .unwrap();
        }

        fn get_active_voxel_len(frag_img_build_result: &Buffer) -> u32 {
            let layout = &frag_img_build_result.get_layout().unwrap().root_member;
            let raw_data = frag_img_build_result.read_back().unwrap();
            let reader = StructMemberDataReader::new(layout, &raw_data);
            let field_val = reader.get_field("active_voxel_len").unwrap();
            if let PlainMemberTypeWithData::UInt(val) = field_val {
                val
            } else {
                panic!("Expected UInt type for active_voxel_len")
            }
        }
    }

    pub fn get_resources(&self) -> &ContreeBuilderResources {
        &self.resources
    }

    fn get_level(contree_dim: UVec3) -> u32 {
        assert!(
            contree_dim.x == contree_dim.y && contree_dim.x == contree_dim.z,
            "ContreeBuilderResources: contree_dim must be a cube"
        );
        assert!(is_power_of_four(contree_dim.x));
        let level = log_4(contree_dim.x) + 1;
        return level;

        /// Returns true if `n` is a power of four (1, 4, 16, 64, …).
        ///
        /// Uses two bit-tricks:
        /// 1. `n & (n - 1) == 0` ensures `n` is a power of two (only one bit set).
        /// 2. `0x5555_5555` has 1s in all even bit positions (0,2,4,…).
        ///    Masking with it ensures the single bit of `n` is in an even position.
        fn is_power_of_four(n: u32) -> bool {
            n != 0
        && (n & (n - 1)) == 0         // power of two?
        && (n & 0x5555_5555) != 0 // bit in an even position?
        }

        fn log_4(n: u32) -> u32 {
            // trailing_zeros gives 2*k, so divide by 2:
            n.trailing_zeros() / 2
        }
    }

    fn build_contree(
        &mut self,
        contree_dim: UVec3,
        node_write_offset: u64,
        leaf_write_offset: u64,
    ) {
        let device = self.vulkan_ctx.device();

        update_buffers(
            &self.resources.contree_build_info,
            contree_dim,
            Self::get_level(contree_dim),
            node_write_offset as u32,
            leaf_write_offset as u32,
        );

        let cmdbuf = self.contree_cmdbuf.clone();
        cmdbuf.submit(&self.vulkan_ctx.get_general_queue(), None);
        device.wait_queue_idle(&self.vulkan_ctx.get_general_queue());

        fn update_buffers(
            contree_build_info: &Buffer,
            contree_dim: UVec3,
            max_level: u32,
            node_write_offset: u32,
            leaf_write_offset: u32,
        ) {
            let data = StructMemberDataBuilder::from_buffer(contree_build_info)
                .set_field("dim", PlainMemberTypeWithData::UInt(contree_dim.x))
                .unwrap()
                .set_field("max_level", PlainMemberTypeWithData::UInt(max_level))
                .unwrap()
                .set_field(
                    "node_write_offset",
                    PlainMemberTypeWithData::UInt(node_write_offset),
                )
                .unwrap()
                .set_field(
                    "leaf_write_offset",
                    PlainMemberTypeWithData::UInt(leaf_write_offset),
                )
                .unwrap()
                .get_data_u8();
            contree_build_info.fill_with_raw_u8(&data).unwrap();
        }
    }

    /// Returns: (node_alloc_offset, leaf_alloc_offset)
    pub fn build_and_alloc(
        &mut self,
        atlas_offset: UVec3,
        atlas_dim: UVec3,
    ) -> Result<Option<(u64, u64)>, String> {
        check_dim(atlas_dim)?;

        let t1 = Instant::now();
        let active_voxel_len = self.build_frag_img(&self.resources, atlas_offset, atlas_dim);
        BENCH.lock().unwrap().record("build_frag_img", t1.elapsed());
        log::debug!("Active voxel len: {}", active_voxel_len);

        if active_voxel_len == 0 {
            log::debug!("No fragments found, skipping contree build.");
            return Ok(None);
        }

        // preallocate 10MB for both the currentl node and leaf buffer to be built
        const MAX_NODE_BUFFER_SIZE_IN_BYTES: u64 = 10 * 1024 * 1024;
        const MAX_LEAF_BUFFER_SIZE_IN_BYTES: u64 = 10 * 1024 * 1024;
        let (node_alloc_offset_in_bytes, leaf_alloc_offset_in_bytes) = self.pre_allocate_chunk(
            MAX_NODE_BUFFER_SIZE_IN_BYTES,
            MAX_LEAF_BUFFER_SIZE_IN_BYTES,
            atlas_offset,
        );
        // the offset's unit is in bytes, we need to convert it to array idx, each element is a 3*u32
        let node_alloc_offset = node_alloc_offset_in_bytes / SIZE_OF_NODE_ELEMENT as u64;
        // the element of leaf data is a u32
        let leaf_alloc_offset = leaf_alloc_offset_in_bytes / SIZE_OF_LEAF_ELEMENT as u64;

        log::debug!(
            "Node alloc offset: {}, Leaf alloc offset: {}",
            node_alloc_offset,
            leaf_alloc_offset
        );

        let t2 = Instant::now();
        self.build_contree(atlas_dim, node_alloc_offset, leaf_alloc_offset);
        BENCH.lock().unwrap().record("build_contree", t2.elapsed());

        let (confirmed_node_buffer_size_in_bytes, confirmed_leaf_buffer_size_in_bytes) =
            self.get_contree_size_info(&self.resources);

        self.confirm_allocation_of_chunk(
            confirmed_node_buffer_size_in_bytes,
            confirmed_leaf_buffer_size_in_bytes,
            atlas_offset,
        );

        return Ok(Some((
            node_alloc_offset_in_bytes,
            leaf_alloc_offset_in_bytes,
        )));

        // return Ok(Some(write_offset));

        // ───────── helper ─────────
        fn check_dim(voxel_dim: UVec3) -> Result<(), String> {
            if voxel_dim.x != voxel_dim.y || voxel_dim.y != voxel_dim.z {
                return Err(format!(
                    "Voxel dimension must be equal in all dimensions, but got: {}",
                    voxel_dim
                ));
            }
            if !voxel_dim.x.is_power_of_two() {
                return Err(format!(
                    "Voxel dimension must be a power of two, but got: {}",
                    voxel_dim
                ));
            }
            Ok(())
        }
    }

    /// Allocate a chunk of data and store the allocation id in the offset_allocation_table.
    ///
    /// Returns: (node_alloc_offset_in_bytes, leaf_alloc_offset_in_bytes)
    /// If the chunk already exists, deallocate it first.
    fn pre_allocate_chunk(
        &mut self,
        max_node_buffer_size_in_bytes: u64,
        max_leaf_buffer_size_in_bytes: u64,
        atlas_offset: UVec3,
    ) -> (u64, u64) {
        if self
            .chunk_offset_allocation_table
            .contains_key(&atlas_offset)
        {
            let (node_alloc_id, leaf_alloc_id) = self
                .chunk_offset_allocation_table
                .remove(&atlas_offset)
                .unwrap();
            self.node_allocator.deallocate(node_alloc_id).unwrap();
            self.leaf_allocator.deallocate(leaf_alloc_id).unwrap();
        }
        let node_allocation = self
            .node_allocator
            .allocate(max_node_buffer_size_in_bytes)
            .unwrap();
        let leaf_allocation = self
            .leaf_allocator
            .allocate(max_leaf_buffer_size_in_bytes)
            .unwrap();

        self.chunk_offset_allocation_table
            .insert(atlas_offset, (node_allocation.id, leaf_allocation.id));
        return (node_allocation.offset, leaf_allocation.offset);
    }

    fn confirm_allocation_of_chunk(
        &mut self,
        confirmed_node_buffer_size_in_bytes: u64,
        confirmed_leaf_buffer_size_in_bytes: u64,
        atlas_offset: UVec3,
    ) {
        let (node_alloc_id, leaf_alloc_id) = self
            .chunk_offset_allocation_table
            .get(&atlas_offset)
            .expect("Chunk not found in allocation table");

        self.node_allocator
            .resize(*node_alloc_id, confirmed_node_buffer_size_in_bytes)
            .unwrap();
        self.leaf_allocator
            .resize(*leaf_alloc_id, confirmed_leaf_buffer_size_in_bytes)
            .unwrap();
    }

    // pub fn update_contree_offset_atlas_tex(
    //     &mut self,
    //     vulkan_context: &VulkanContext,
    //     resources: &ContreeBuilderResources,
    //     voxel_dim: UVec3,
    //     visible_chunk_dim: UVec3,
    // ) {
    //     let mut offset_table = vec![];
    //     for (chunk_pos, allocation_id) in self.chunk_offset_allocation_table.iter() {
    //         let allocation = self.contree_buffer_allocator.lookup(*allocation_id).unwrap();
    //         let offset_in_bytes = allocation.offset;
    //         let offset_in_u32 = offset_in_bytes / std::mem::size_of::<u32>() as u64;
    //         offset_table.push((*chunk_pos, offset_in_u32));
    //     }

    //     // TODO: implement further
    //     // for now, just a simple logic, to fit all chunk offsets stored inside the table into a fixed size buffer.
    //     let mut offset_data: Vec<u32> = vec![
    //         0;
    //         visible_chunk_dim.x as usize
    //             * visible_chunk_dim.y as usize
    //             * visible_chunk_dim.z as usize
    //     ];

    //     // update offset_data accordingly
    //     for (atlas_offset, offset_in_u32) in offset_table.iter() {
    //         let chunk_offset = atlas_offset_to_chunk_offset(*atlas_offset, voxel_dim);

    //         assert!(in_bounds(chunk_offset, visible_chunk_dim));
    //         let linear_index = to_linear_index(chunk_offset, visible_chunk_dim);
    //         // write with an offset of 1, because 0 is reserved for empty chunk
    //         offset_data[linear_index as usize] = (*offset_in_u32 + 1) as u32;

    //         fn atlas_offset_to_chunk_offset(atlas_offset: UVec3, voxel_dim: UVec3) -> UVec3 {
    //             atlas_offset / voxel_dim
    //         }
    //         fn in_bounds(chunk_pos: UVec3, dim: UVec3) -> bool {
    //             chunk_pos.x < dim.x && chunk_pos.y < dim.y && chunk_pos.z < dim.z
    //         }
    //         fn to_linear_index(chunk_pos: UVec3, dim: UVec3) -> u32 {
    //             chunk_pos.x + chunk_pos.y * dim.x + chunk_pos.z * dim.x * dim.y
    //         }
    //     }

    //     // fill the texture
    //     resources
    //         .contree_offset_atlas_tex
    //         .get_image()
    //         .fill_with_raw_u32(
    //             &vulkan_context.get_general_queue(),
    //             vulkan_context.command_pool(),
    //             TextureRegion::from_image(&resources.contree_offset_atlas_tex.get_image()),
    //             &offset_data,
    //             None,
    //         )
    //         .unwrap();
    // }
}
