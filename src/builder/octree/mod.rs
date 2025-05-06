mod resources;
pub use resources::*;

use std::collections::HashMap;

use crate::builder::PlainBuilderResources;
use crate::util::AllocationStrategy;
use crate::util::FirstFitAllocator;
use crate::util::ShaderCompiler;
use crate::vkn::execute_one_time_command;
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

pub struct OctreeBuilder {
    vulkan_ctx: VulkanContext,
    resources: OctreeBuilderResources,

    frag_list_init_buffers_ppl: ComputePipeline,
    frag_list_maker_ppl: ComputePipeline,
    octree_init_buffers_ppl: ComputePipeline,
    octree_init_node_ppl: ComputePipeline,
    octree_tag_node_ppl: ComputePipeline,
    octree_alloc_node_ppl: ComputePipeline,
    octree_modify_args_ppl: ComputePipeline,

    frag_list_init_buffers_ds: DescriptorSet,
    frag_list_maker_ds: DescriptorSet,
    // frag_list_maker_free_atlas_ds: DescriptorSet,
    octree_shared_ds: DescriptorSet,
    octree_init_buffers_ds: DescriptorSet,
    octree_init_node_ds: DescriptorSet,
    octree_tag_node_ds: DescriptorSet,
    octree_alloc_node_ds: DescriptorSet,
    octree_modify_args_ds: DescriptorSet,

    chunk_offset_allocation_table: HashMap<UVec3, u64>,
    octree_buffer_allocator: FirstFitAllocator,

    cmdbuf_table: HashMap<u32, CommandBuffer>,

    frag_list_cmdbuf: CommandBuffer,
}

impl OctreeBuilder {
    pub fn new(
        vulkan_ctx: VulkanContext,
        allocator: Allocator,
        shader_compiler: &ShaderCompiler,
        plain_builder_resources: &PlainBuilderResources,
        max_voxel_dim_per_chunk: UVec3,
        octree_buffer_pool_size: u64,
    ) -> Self {
        let descriptor_pool = DescriptorPool::a_big_one(vulkan_ctx.device()).unwrap();

        let frag_list_init_buffers_sm = ShaderModule::from_glsl(
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
            "shader/builder/octree_builder/init_buffers.comp",
            "main",
        )
        .unwrap();
        let octree_init_node_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/builder/octree_builder/init_node.comp",
            "main",
        )
        .unwrap();
        let octree_tag_node_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/builder/octree_builder/tag_node.comp",
            "main",
        )
        .unwrap();
        let octree_alloc_node_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/builder/octree_builder/alloc_node.comp",
            "main",
        )
        .unwrap();
        let octree_modify_args_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/builder/octree_builder/modify_args.comp",
            "main",
        )
        .unwrap();

        let resources = OctreeBuilderResources::new(
            vulkan_ctx.device().clone(),
            allocator.clone(),
            max_voxel_dim_per_chunk,
            octree_buffer_pool_size,
            &frag_list_init_buffers_sm,
            &frag_list_maker_sm,
            &octree_init_buffers_sm,
        );

        let frag_list_init_buffers_ppl =
            ComputePipeline::from_shader_module(vulkan_ctx.device(), &frag_list_init_buffers_sm);
        let frag_list_maker_ppl =
            ComputePipeline::from_shader_module(vulkan_ctx.device(), &frag_list_maker_sm);
        let octree_init_buffers_ppl =
            ComputePipeline::from_shader_module(vulkan_ctx.device(), &octree_init_buffers_sm);
        let octree_init_node_ppl =
            ComputePipeline::from_shader_module(vulkan_ctx.device(), &octree_init_node_sm);
        let octree_tag_node_ppl =
            ComputePipeline::from_shader_module(vulkan_ctx.device(), &octree_tag_node_sm);
        let octree_alloc_node_ppl =
            ComputePipeline::from_shader_module(vulkan_ctx.device(), &octree_alloc_node_sm);
        let octree_modify_args_ppl =
            ComputePipeline::from_shader_module(vulkan_ctx.device(), &octree_modify_args_sm);

        let frag_list_init_buffers_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &frag_list_init_buffers_ppl
                .get_layout()
                .get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        frag_list_init_buffers_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.frag_list_maker_info),
            WriteDescriptorSet::new_buffer_write(1, &resources.voxel_dim_indirect),
            WriteDescriptorSet::new_buffer_write(2, &resources.frag_list_build_result),
        ]);
        let frag_list_maker_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &frag_list_maker_ppl
                .get_layout()
                .get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        frag_list_maker_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.frag_list_maker_info),
            WriteDescriptorSet::new_buffer_write(1, &resources.frag_list_build_result),
            WriteDescriptorSet::new_buffer_write(2, &resources.frag_list),
            WriteDescriptorSet::new_texture_write(
                3,
                vk::DescriptorType::STORAGE_IMAGE,
                &plain_builder_resources.chunk_atlas,
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
            WriteDescriptorSet::new_buffer_write(2, &resources.frag_list),
            WriteDescriptorSet::new_texture_write(
                3,
                vk::DescriptorType::STORAGE_IMAGE,
                &plain_builder_resources.free_atlas,
                vk::ImageLayout::GENERAL,
            ),
        ]);

        let octree_shared_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &octree_init_buffers_ppl
                .get_layout()
                .get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        octree_shared_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.octree_build_info),
            WriteDescriptorSet::new_buffer_write(1, &resources.octree_alloc_info),
        ]);

        let octree_init_buffers_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &octree_init_buffers_ppl
                .get_layout()
                .get_descriptor_set_layouts()[1],
            descriptor_pool.clone(),
        );
        octree_init_buffers_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.voxel_count_indirect),
            WriteDescriptorSet::new_buffer_write(1, &resources.alloc_number_indirect),
            WriteDescriptorSet::new_buffer_write(2, &resources.counter),
            WriteDescriptorSet::new_buffer_write(3, &resources.octree_build_result),
        ]);

        let octree_init_node_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &octree_init_node_ppl
                .get_layout()
                .get_descriptor_set_layouts()[1],
            descriptor_pool.clone(),
        );
        octree_init_node_ds.perform_writes(&mut [WriteDescriptorSet::new_buffer_write(
            0,
            &resources.octree_data_single,
        )]);

        let octree_tag_node_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &octree_tag_node_ppl
                .get_layout()
                .get_descriptor_set_layouts()[1],
            descriptor_pool.clone(),
        );
        octree_tag_node_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.octree_data_single),
            WriteDescriptorSet::new_buffer_write(1, &resources.frag_list),
        ]);

        let octree_alloc_node_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &octree_alloc_node_ppl
                .get_layout()
                .get_descriptor_set_layouts()[1],
            descriptor_pool.clone(),
        );
        octree_alloc_node_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.octree_data_single),
            WriteDescriptorSet::new_buffer_write(1, &resources.frag_list),
            WriteDescriptorSet::new_buffer_write(2, &resources.counter),
        ]);

        let octree_modify_args_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &octree_modify_args_ppl
                .get_layout()
                .get_descriptor_set_layouts()[1],
            descriptor_pool.clone(),
        );
        octree_modify_args_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.counter),
            WriteDescriptorSet::new_buffer_write(1, &resources.octree_build_result),
            WriteDescriptorSet::new_buffer_write(2, &resources.alloc_number_indirect),
        ]);

        let octree_buffer_allocator = FirstFitAllocator::new(octree_buffer_pool_size);

        let frag_list_cmdbuf = record_frag_list_cmdbuf(
            &vulkan_ctx,
            &resources,
            &frag_list_init_buffers_ppl,
            &frag_list_maker_ppl,
            &frag_list_init_buffers_ds,
            &frag_list_maker_free_atlas_ds,
        );

        return Self {
            vulkan_ctx,
            resources,

            frag_list_init_buffers_ppl,
            frag_list_maker_ppl,
            octree_init_buffers_ppl,
            octree_init_node_ppl,
            octree_tag_node_ppl,
            octree_alloc_node_ppl,
            octree_modify_args_ppl,

            frag_list_init_buffers_ds,
            frag_list_maker_ds,
            octree_shared_ds,
            octree_init_buffers_ds,
            octree_init_node_ds,
            octree_tag_node_ds,
            octree_alloc_node_ds,
            octree_modify_args_ds,

            chunk_offset_allocation_table: HashMap::new(),
            octree_buffer_allocator,

            cmdbuf_table: HashMap::new(),

            frag_list_cmdbuf,
        };

        fn record_frag_list_cmdbuf(
            vulkan_ctx: &VulkanContext,
            resources: &OctreeBuilderResources,
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
    }

    pub fn get_octree_data_size_in_bytes(&self, resources: &OctreeBuilderResources) -> u32 {
        let layout = &resources
            .octree_build_result
            .get_layout()
            .unwrap()
            .root_member;
        let raw_data = resources.octree_build_result.read_back().unwrap();
        let reader = StructMemberDataReader::new(layout, &raw_data);
        let field_val = reader.get_field("size_u32").unwrap();
        if let PlainMemberTypeWithData::UInt(val) = field_val {
            return val * std::mem::size_of::<u32>() as u32;
        } else {
            panic!("Failed to get size_u32 from octree_build_result");
        }
    }

    fn copy_octree_data_single_to_octree_data(
        &self,
        vulkan_context: &VulkanContext,
        resources: &OctreeBuilderResources,
        write_offset: u64,
        size: u64,
    ) {
        execute_one_time_command(
            vulkan_context.device(),
            vulkan_context.command_pool(),
            &vulkan_context.get_general_queue(),
            |cmdbuf| {
                resources.octree_data_single.record_copy_to_buffer(
                    cmdbuf,
                    &resources.octree_data,
                    size,
                    0,
                    write_offset,
                );
            },
        );
    }

    fn build_cmdbuf_for_level(
        &self,
        vulkan_context: &VulkanContext,
        resources: &OctreeBuilderResources,
        voxel_level: u32,
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

        self.octree_init_buffers_ppl.record_bind(&cmdbuf);
        self.octree_init_buffers_ppl.record_bind_descriptor_sets(
            &cmdbuf,
            std::slice::from_ref(&self.octree_shared_ds),
            0,
        );
        self.octree_init_buffers_ppl.record_bind_descriptor_sets(
            &cmdbuf,
            std::slice::from_ref(&self.octree_init_buffers_ds),
            1,
        );
        self.octree_init_buffers_ppl
            .record_dispatch(&cmdbuf, [1, 1, 1]);
        shader_access_pipeline_barrier.record_insert(device, &cmdbuf);
        indirect_access_pipeline_barrier.record_insert(device, &cmdbuf);

        for i in 0..voxel_level {
            self.octree_init_node_ppl.record_bind(&cmdbuf);
            self.octree_init_node_ppl.record_bind_descriptor_sets(
                &cmdbuf,
                std::slice::from_ref(&self.octree_init_node_ds),
                1,
            );
            self.octree_init_node_ppl
                .record_dispatch_indirect(&cmdbuf, &resources.alloc_number_indirect);

            shader_access_pipeline_barrier.record_insert(device, &cmdbuf);

            self.octree_tag_node_ppl.record_bind(&cmdbuf);
            self.octree_tag_node_ppl.record_bind_descriptor_sets(
                &cmdbuf,
                std::slice::from_ref(&self.octree_tag_node_ds),
                1,
            );
            self.octree_tag_node_ppl
                .record_dispatch_indirect(&cmdbuf, &resources.voxel_count_indirect);

            // if not last level
            if i != voxel_level - 1 {
                shader_access_pipeline_barrier.record_insert(device, &cmdbuf);

                self.octree_alloc_node_ppl.record_bind(&cmdbuf);
                self.octree_alloc_node_ppl.record_bind_descriptor_sets(
                    &cmdbuf,
                    std::slice::from_ref(&self.octree_alloc_node_ds),
                    1,
                );
                self.octree_alloc_node_ppl
                    .record_dispatch_indirect(&cmdbuf, &resources.alloc_number_indirect);

                shader_access_pipeline_barrier.record_insert(device, &cmdbuf);

                self.octree_modify_args_ppl.record_bind(&cmdbuf);
                self.octree_modify_args_ppl.record_bind_descriptor_sets(
                    &cmdbuf,
                    std::slice::from_ref(&self.octree_modify_args_ds),
                    1,
                );
                self.octree_modify_args_ppl
                    .record_dispatch(&cmdbuf, [1, 1, 1]);

                shader_access_pipeline_barrier.record_insert(device, &cmdbuf);
                indirect_access_pipeline_barrier.record_insert(device, &cmdbuf);
            }
        }
        cmdbuf.end();
        cmdbuf
    }

    fn build_frag_list(
        &self,
        resources: &OctreeBuilderResources,
        atlas_read_offset: UVec3,
        atlas_read_dim: UVec3,
    ) {
        let device = self.vulkan_ctx.device();

        update_buffers(
            resources,
            atlas_read_offset,
            atlas_read_dim,
            true, // is_crossing_boundary,
        );

        self.frag_list_cmdbuf
            .submit(&self.vulkan_ctx.get_general_queue(), None);
        device.wait_queue_idle(&self.vulkan_ctx.get_general_queue());

        fn update_buffers(
            resources: &OctreeBuilderResources,
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

    fn get_fraglist_length(&self) -> u32 {
        let layout = &self
            .resources
            .frag_list_build_result
            .get_layout()
            .unwrap()
            .root_member;
        let raw_data = self.resources.frag_list_build_result.read_back().unwrap();
        let reader = StructMemberDataReader::new(layout, &raw_data);
        let field_val = reader.get_field("frag_list_len").unwrap();
        if let PlainMemberTypeWithData::UInt(val) = field_val {
            val
        } else {
            panic!("Expected UInt type for frag_list_len")
        }
    }

    pub fn build_and_alloc(
        &mut self,
        atlas_offset: UVec3,
        atlas_dim: UVec3,
    ) -> Result<Option<u64>, String> {
        check_dim(atlas_dim)?;

        // TODO: use a barrier instead of a halt.
        self.build_frag_list(&self.resources, atlas_offset, atlas_dim);

        let frag_list_len = self.get_fraglist_length();
        if frag_list_len == 0 {
            return Ok(None);
        }

        let device = self.vulkan_ctx.device();

        update_buffers(&self.resources, frag_list_len, atlas_dim.x);

        let level = get_level(atlas_dim);
        let cmdbuf = if let Some(cmdbuf) = self.cmdbuf_table.get(&level) {
            cmdbuf.clone()
        } else {
            let newly_created =
                self.build_cmdbuf_for_level(&self.vulkan_ctx, &self.resources, level);
            self.cmdbuf_table.insert(level, newly_created.clone());
            newly_created
        };

        cmdbuf.submit(&self.vulkan_ctx.get_general_queue(), None);
        device.wait_queue_idle(&self.vulkan_ctx.get_general_queue());

        let octree_size = self.get_octree_data_size_in_bytes(&self.resources);
        assert!(octree_size > 0);

        let write_offset = self.allocate_chunk(octree_size as u64, atlas_offset);

        self.copy_octree_data_single_to_octree_data(
            &self.vulkan_ctx,
            &self.resources,
            write_offset,
            octree_size as u64,
        );

        return Ok(Some(write_offset));

        fn check_dim(voxel_dim: UVec3) -> Result<(), String> {
            if voxel_dim.x != voxel_dim.y || voxel_dim.y != voxel_dim.z {
                return Err(format!(
                    "Voxel dimension must be equal in all dimensions, but got: {}",
                    voxel_dim
                ));
            }

            if voxel_dim.x.is_power_of_two() == false {
                return Err(format!(
                    "Voxel dimension must be a power of two, but got: {}",
                    voxel_dim
                ));
            }

            return Ok(());
        }

        fn get_level(voxel_dim: UVec3) -> u32 {
            return log2(voxel_dim.x);

            fn log2(x: u32) -> u32 {
                31 - x.leading_zeros()
            }
        }

        fn update_buffers(
            resources: &OctreeBuilderResources,
            frag_list_len: u32,
            voxel_dim_xyz: u32,
        ) {
            let data = StructMemberDataBuilder::from_buffer(&resources.octree_build_info)
                .set_field(
                    "frag_list_len",
                    PlainMemberTypeWithData::UInt(frag_list_len),
                )
                .unwrap()
                .set_field(
                    "voxel_dim_xyz",
                    PlainMemberTypeWithData::UInt(voxel_dim_xyz),
                )
                .unwrap()
                .get_data_u8();
            resources.octree_build_info.fill_with_raw_u8(&data).unwrap();
        }
    }

    /// Allocate a chunk of octree data and store the allocation id in the offset_allocation_table.
    ///
    /// If the chunk already exists, deallocate it first.
    fn allocate_chunk(&mut self, buffer_size: u64, atlas_offset: UVec3) -> u64 {
        if self
            .chunk_offset_allocation_table
            .contains_key(&atlas_offset)
        {
            let allocation_id = self
                .chunk_offset_allocation_table
                .remove(&atlas_offset)
                .unwrap();
            self.octree_buffer_allocator
                .deallocate(allocation_id)
                .unwrap();
        }
        let allocation = self.octree_buffer_allocator.allocate(buffer_size).unwrap();
        self.chunk_offset_allocation_table
            .insert(atlas_offset, allocation.id);
        return allocation.offset;
    }

    // pub fn update_octree_offset_atlas_tex(
    //     &mut self,
    //     vulkan_context: &VulkanContext,
    //     resources: &OctreeBuilderResources,
    //     voxel_dim: UVec3,
    //     visible_chunk_dim: UVec3,
    // ) {
    //     let mut offset_table = vec![];
    //     for (chunk_pos, allocation_id) in self.chunk_offset_allocation_table.iter() {
    //         let allocation = self.octree_buffer_allocator.lookup(*allocation_id).unwrap();
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
    //         .octree_offset_atlas_tex
    //         .get_image()
    //         .fill_with_raw_u32(
    //             &vulkan_context.get_general_queue(),
    //             vulkan_context.command_pool(),
    //             TextureRegion::from_image(&resources.octree_offset_atlas_tex.get_image()),
    //             &offset_data,
    //             None,
    //         )
    //         .unwrap();
    // }
}
