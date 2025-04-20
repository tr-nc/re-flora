use std::collections::HashMap;

use super::Resources;
use crate::util::AllocationStrategy;
use crate::util::FirstFitAllocator;
use crate::util::ShaderCompiler;
use crate::vkn::execute_one_time_command;
use crate::vkn::BufferBuilder;
use crate::vkn::ClearValue;
use crate::vkn::CommandBuffer;
use crate::vkn::ComputePipeline;
use crate::vkn::DescriptorPool;
use crate::vkn::DescriptorSet;
use crate::vkn::MemoryBarrier;
use crate::vkn::PipelineBarrier;
use crate::vkn::ShaderModule;
use crate::vkn::TextureRegion;
use crate::vkn::VulkanContext;
use crate::vkn::WriteDescriptorSet;
use ash::vk;
use glam::UVec3;

pub struct OctreeBuilder {
    octree_init_buffers_ppl: ComputePipeline,
    octree_init_node_ppl: ComputePipeline,
    octree_tag_node_ppl: ComputePipeline,
    octree_alloc_node_ppl: ComputePipeline,
    octree_modify_args_ppl: ComputePipeline,

    octree_shared_ds: DescriptorSet,
    octree_init_buffers_ds: DescriptorSet,
    octree_init_node_ds: DescriptorSet,
    octree_tag_node_ds: DescriptorSet,
    octree_alloc_node_ds: DescriptorSet,
    octree_modify_args_ds: DescriptorSet,

    offset_allocation_table: HashMap<UVec3, u64>,
    octree_buffer_allocator: FirstFitAllocator,

    cmdbuf_table: HashMap<u32, CommandBuffer>,
}

impl OctreeBuilder {
    pub fn new(
        vulkan_context: &VulkanContext,
        shader_compiler: &ShaderCompiler,
        descriptor_pool: DescriptorPool,
        resources: &Resources,
        octree_buffer_size: u64,
    ) -> Self {
        let octree_init_buffers_sm = ShaderModule::from_glsl(
            vulkan_context.device(),
            &shader_compiler,
            "shader/builder/octree/init_buffers.comp",
            "main",
        )
        .unwrap();
        let octree_init_buffers_ppl =
            ComputePipeline::from_shader_module(vulkan_context.device(), &octree_init_buffers_sm);

        let octree_init_node_sm = ShaderModule::from_glsl(
            vulkan_context.device(),
            &shader_compiler,
            "shader/builder/octree/init_node.comp",
            "main",
        )
        .unwrap();
        let octree_init_node_ppl =
            ComputePipeline::from_shader_module(vulkan_context.device(), &octree_init_node_sm);

        let octree_tag_node_sm = ShaderModule::from_glsl(
            vulkan_context.device(),
            &shader_compiler,
            "shader/builder/octree/tag_node.comp",
            "main",
        )
        .unwrap();
        let octree_tag_node_ppl =
            ComputePipeline::from_shader_module(vulkan_context.device(), &octree_tag_node_sm);

        let octree_alloc_node_sm = ShaderModule::from_glsl(
            vulkan_context.device(),
            &shader_compiler,
            "shader/builder/octree/alloc_node.comp",
            "main",
        )
        .unwrap();
        let octree_alloc_node_ppl =
            ComputePipeline::from_shader_module(vulkan_context.device(), &octree_alloc_node_sm);

        let octree_modify_args_sm = ShaderModule::from_glsl(
            vulkan_context.device(),
            &shader_compiler,
            "shader/builder/octree/modify_args.comp",
            "main",
        )
        .unwrap();
        let octree_modify_args_ppl =
            ComputePipeline::from_shader_module(vulkan_context.device(), &octree_modify_args_sm);

        let shared_ds = DescriptorSet::new(
            vulkan_context.device().clone(),
            &octree_init_buffers_ppl
                .get_layout()
                .get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        shared_ds.perform_writes(&[
            WriteDescriptorSet::new_buffer_write(0, resources.octree_build_info()),
            WriteDescriptorSet::new_buffer_write(1, resources.octree_alloc_info()),
        ]);

        let init_buffers_ds = DescriptorSet::new(
            vulkan_context.device().clone(),
            &octree_init_buffers_ppl
                .get_layout()
                .get_descriptor_set_layouts()[1],
            descriptor_pool.clone(),
        );
        init_buffers_ds.perform_writes(&[
            WriteDescriptorSet::new_buffer_write(0, resources.voxel_count_indirect()),
            WriteDescriptorSet::new_buffer_write(1, resources.alloc_number_indirect()),
            WriteDescriptorSet::new_buffer_write(2, resources.counter()),
            WriteDescriptorSet::new_buffer_write(3, resources.octree_build_result()),
        ]);

        let init_node_ds = DescriptorSet::new(
            vulkan_context.device().clone(),
            &octree_init_node_ppl
                .get_layout()
                .get_descriptor_set_layouts()[1],
            descriptor_pool.clone(),
        );
        init_node_ds.perform_writes(&[WriteDescriptorSet::new_buffer_write(
            0,
            resources.octree_data_single(),
        )]);

        let tag_node_ds = DescriptorSet::new(
            vulkan_context.device().clone(),
            &octree_tag_node_ppl
                .get_layout()
                .get_descriptor_set_layouts()[1],
            descriptor_pool.clone(),
        );
        tag_node_ds.perform_writes(&[
            WriteDescriptorSet::new_buffer_write(0, resources.octree_data_single()),
            WriteDescriptorSet::new_buffer_write(1, resources.fragment_list()),
        ]);

        let alloc_node_ds = DescriptorSet::new(
            vulkan_context.device().clone(),
            &octree_alloc_node_ppl
                .get_layout()
                .get_descriptor_set_layouts()[1],
            descriptor_pool.clone(),
        );
        alloc_node_ds.perform_writes(&[
            WriteDescriptorSet::new_buffer_write(0, resources.octree_data_single()),
            WriteDescriptorSet::new_buffer_write(1, resources.fragment_list()),
            WriteDescriptorSet::new_buffer_write(2, resources.counter()),
        ]);

        let modify_args_ds = DescriptorSet::new(
            vulkan_context.device().clone(),
            &octree_modify_args_ppl
                .get_layout()
                .get_descriptor_set_layouts()[1],
            descriptor_pool.clone(),
        );
        modify_args_ds.perform_writes(&[
            WriteDescriptorSet::new_buffer_write(0, resources.counter()),
            WriteDescriptorSet::new_buffer_write(1, resources.octree_build_result()),
            WriteDescriptorSet::new_buffer_write(2, resources.alloc_number_indirect()),
        ]);

        let octree_buffer_allocator = FirstFitAllocator::new(octree_buffer_size);

        init_atlas(vulkan_context, resources);
        fn init_atlas(vulkan_context: &VulkanContext, resources: &Resources) {
            execute_one_time_command(
                vulkan_context.device(),
                vulkan_context.command_pool(),
                &vulkan_context.get_general_queue(),
                |cmdbuf| {
                    resources
                        .octree_offset_atlas_tex()
                        .get_image()
                        .record_clear(
                            cmdbuf,
                            Some(vk::ImageLayout::GENERAL),
                            ClearValue::UInt([0, 0, 0, 0]),
                        );
                },
            );
        }

        Self {
            octree_init_buffers_ppl,
            octree_init_node_ppl,
            octree_tag_node_ppl,
            octree_alloc_node_ppl,
            octree_modify_args_ppl,
            octree_shared_ds: shared_ds,
            octree_init_buffers_ds: init_buffers_ds,
            octree_init_node_ds: init_node_ds,
            octree_tag_node_ds: tag_node_ds,
            octree_alloc_node_ds: alloc_node_ds,
            octree_modify_args_ds: modify_args_ds,

            offset_allocation_table: HashMap::new(),
            octree_buffer_allocator,

            cmdbuf_table: HashMap::new(),
        }
    }

    pub fn get_octree_data_size_in_bytes(&self, resources: &Resources) -> u32 {
        let raw_data = resources.octree_build_result().fetch_raw().unwrap();
        BufferBuilder::from_struct_buffer(resources.octree_build_result())
            .unwrap()
            .set_raw(raw_data)
            .get_uint("size_u32")
            .unwrap()
            * std::mem::size_of::<u32>() as u32
    }

    fn copy_octree_data_single_to_octree_data(
        &self,
        vulkan_context: &VulkanContext,
        resources: &Resources,
        write_offset: u64,
        size: u64,
    ) {
        execute_one_time_command(
            vulkan_context.device(),
            vulkan_context.command_pool(),
            &vulkan_context.get_general_queue(),
            |cmdbuf| {
                resources.octree_data_single().record_copy_to_buffer(
                    cmdbuf,
                    resources.octree_data(),
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
        resources: &Resources,
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
                .record_dispatch_indirect(&cmdbuf, resources.alloc_number_indirect());

            shader_access_pipeline_barrier.record_insert(device, &cmdbuf);

            self.octree_tag_node_ppl.record_bind(&cmdbuf);
            self.octree_tag_node_ppl.record_bind_descriptor_sets(
                &cmdbuf,
                std::slice::from_ref(&self.octree_tag_node_ds),
                1,
            );
            self.octree_tag_node_ppl
                .record_dispatch_indirect(&cmdbuf, resources.voxel_count_indirect());

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
                    .record_dispatch_indirect(&cmdbuf, resources.alloc_number_indirect());

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

    fn get_level(&self, voxel_dim: UVec3) -> u32 {
        assert!(voxel_dim.x == voxel_dim.y && voxel_dim.y == voxel_dim.z);
        assert!(voxel_dim.x.is_power_of_two());

        return log2(voxel_dim.x);

        fn log2(x: u32) -> u32 {
            31 - x.leading_zeros()
        }
    }

    pub fn build(
        &mut self,
        vulkan_context: &VulkanContext,
        resources: &Resources,
        fragment_list_len: u32,
        chunk_pos: UVec3,
        voxel_dim: UVec3,
    ) {
        let device = vulkan_context.device();

        update_uniforms(resources, fragment_list_len);

        let level = self.get_level(voxel_dim);
        let cmdbuf = if let Some(cmdbuf) = self.cmdbuf_table.get(&level) {
            cmdbuf.clone()
        } else {
            let newly_created = self.build_cmdbuf_for_level(vulkan_context, resources, level);
            self.cmdbuf_table.insert(level, newly_created.clone());
            newly_created
        };

        cmdbuf.submit(&vulkan_context.get_general_queue(), None);
        device.wait_queue_idle(&vulkan_context.get_general_queue());

        let octree_size = self.get_octree_data_size_in_bytes(resources);
        assert!(octree_size > 0);

        let write_offset = self.allocate_chunk(octree_size as u64, chunk_pos);

        self.copy_octree_data_single_to_octree_data(
            &vulkan_context,
            resources,
            write_offset,
            octree_size as u64,
        );

        fn update_uniforms(resources: &Resources, fragment_list_len: u32) {
            let octree_build_info_data =
                BufferBuilder::from_struct_buffer(resources.octree_build_info())
                    .unwrap()
                    .set_uint("fragment_list_len", fragment_list_len)
                    .to_raw_data();
            resources
                .octree_build_info()
                .fill_with_raw_u8(&octree_build_info_data)
                .expect("Failed to fill buffer data");
        }
    }

    fn allocate_chunk(&mut self, chunk_buffer_size: u64, chunk_pos: UVec3) -> u64 {
        let allocation = self
            .octree_buffer_allocator
            .allocate(chunk_buffer_size)
            .unwrap();
        self.offset_allocation_table
            .insert(chunk_pos, allocation.id);
        allocation.offset
    }

    pub fn update_octree_offset_atlas(
        &mut self,
        vulkan_context: &VulkanContext,
        resources: &Resources,
        visible_chunk_dim: UVec3,
    ) {
        let mut offset_table = vec![];
        for (chunk_pos, allocation_id) in self.offset_allocation_table.iter() {
            let allocation = self.octree_buffer_allocator.lookup(*allocation_id).unwrap();
            let offset_in_bytes = allocation.offset;
            let offset_in_u32 = offset_in_bytes / std::mem::size_of::<u32>() as u64;
            offset_table.push((*chunk_pos, offset_in_u32));
        }

        // TODO: implement further
        // for now, just a simple logic, to fit all chunk offsets stored inside the table into a fixed size buffer.
        let mut offset_data: Vec<u32> = vec![
            0;
            visible_chunk_dim.x as usize
                * visible_chunk_dim.y as usize
                * visible_chunk_dim.z as usize
        ];

        // update offset_data accordingly
        for (chunk_pos, offset_in_u32) in offset_table.iter() {
            assert!(in_bounds(*chunk_pos, visible_chunk_dim));
            let linear_index = to_linear_index(*chunk_pos, visible_chunk_dim);
            // write with an offset of 1, because 0 is reserved for empty chunk
            offset_data[linear_index as usize] = (*offset_in_u32 + 1) as u32;
        }

        // fill the texture
        resources
            .octree_offset_atlas_tex()
            .get_image()
            .fill_with_raw_u32(
                &vulkan_context.get_general_queue(),
                vulkan_context.command_pool(),
                TextureRegion::from_image(&resources.octree_offset_atlas_tex().get_image()),
                &offset_data,
                None,
            )
            .unwrap();

        fn to_linear_index(chunk_pos: UVec3, dim: UVec3) -> u32 {
            chunk_pos.x + chunk_pos.y * dim.x + chunk_pos.z * dim.x * dim.y
        }

        fn in_bounds(chunk_pos: UVec3, dim: UVec3) -> bool {
            chunk_pos.x < dim.x && chunk_pos.y < dim.y && chunk_pos.z < dim.z
        }
    }
}
