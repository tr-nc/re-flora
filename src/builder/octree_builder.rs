use std::collections::HashMap;

use super::Resources;
use crate::util::AllocationStrategy;
use crate::util::FirstFitAllocator;
use crate::util::ShaderCompiler;
use crate::vkn::execute_one_time_command;
use crate::vkn::BufferBuilder;
use crate::vkn::CommandPool;
use crate::vkn::ComputePipeline;
use crate::vkn::DescriptorPool;
use crate::vkn::DescriptorSet;
use crate::vkn::MemoryBarrier;
use crate::vkn::PipelineBarrier;
use crate::vkn::ShaderModule;
use crate::vkn::VulkanContext;
use crate::vkn::WriteDescriptorSet;
use ash::vk;
use glam::IVec3;
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

    offset_table: HashMap<IVec3, u32>,
    octree_buffer_allocator: FirstFitAllocator,
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

            offset_table: HashMap::new(),
            octree_buffer_allocator,
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

    fn update_uniforms(&mut self, resources: &Resources, dimension: UVec3, fragment_list_len: u32) {
        // here's octree's limitation
        assert!(dimension.x == dimension.y && dimension.y == dimension.z);

        let octree_build_info_data =
            BufferBuilder::from_struct_buffer(resources.octree_build_info())
                .unwrap()
                .set_uint("voxel_dim_xyz", dimension.x as u32)
                .set_uint("fragment_list_len", fragment_list_len)
                .to_raw_data();
        resources
            .octree_build_info()
            .fill_with_raw_u8(&octree_build_info_data)
            .expect("Failed to fill buffer data");
    }

    fn copy_octree_data_single_to_octree_data(
        &self,
        vulkan_context: &VulkanContext,
        command_pool: &CommandPool,
        resources: &Resources,
        write_offset: u64,
        size: u64,
    ) {
        execute_one_time_command(
            vulkan_context.device(),
            command_pool,
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

    pub fn frag_list_to_octree_data(
        &mut self,
        vulkan_context: &VulkanContext,
        command_pool: &CommandPool,
        resources: &Resources,
        fragment_list_len: u32,
        chunk_pos: IVec3,
        voxel_dim: UVec3,
    ) {
        self.update_uniforms(resources, voxel_dim, fragment_list_len);

        let device = vulkan_context.device();

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

        fn log2(x: u32) -> u32 {
            31 - x.leading_zeros()
        }

        execute_one_time_command(
            vulkan_context.device(),
            command_pool,
            &vulkan_context.get_general_queue(),
            |cmdbuf| {
                self.octree_init_buffers_ppl.record_bind(cmdbuf);
                self.octree_init_buffers_ppl.record_bind_descriptor_sets(
                    cmdbuf,
                    std::slice::from_ref(&self.octree_shared_ds),
                    0,
                );
                self.octree_init_buffers_ppl.record_bind_descriptor_sets(
                    cmdbuf,
                    std::slice::from_ref(&self.octree_init_buffers_ds),
                    1,
                );
                self.octree_init_buffers_ppl
                    .record_dispatch(cmdbuf, [1, 1, 1]);
                shader_access_pipeline_barrier.record_insert(device, cmdbuf);
                indirect_access_pipeline_barrier.record_insert(device, cmdbuf);

                let voxel_level_count = log2(voxel_dim.x);

                for i in 0..voxel_level_count {
                    self.octree_init_node_ppl.record_bind(cmdbuf);
                    self.octree_init_node_ppl.record_bind_descriptor_sets(
                        cmdbuf,
                        std::slice::from_ref(&self.octree_init_node_ds),
                        1,
                    );
                    self.octree_init_node_ppl
                        .record_dispatch_indirect(cmdbuf, resources.alloc_number_indirect());

                    shader_access_pipeline_barrier.record_insert(device, cmdbuf);

                    self.octree_tag_node_ppl.record_bind(cmdbuf);
                    self.octree_tag_node_ppl.record_bind_descriptor_sets(
                        cmdbuf,
                        std::slice::from_ref(&self.octree_tag_node_ds),
                        1,
                    );
                    self.octree_tag_node_ppl
                        .record_dispatch_indirect(cmdbuf, resources.voxel_count_indirect());

                    // if not last level
                    if i != voxel_level_count - 1 {
                        shader_access_pipeline_barrier.record_insert(device, cmdbuf);

                        self.octree_alloc_node_ppl.record_bind(cmdbuf);
                        self.octree_alloc_node_ppl.record_bind_descriptor_sets(
                            cmdbuf,
                            std::slice::from_ref(&self.octree_alloc_node_ds),
                            1,
                        );
                        self.octree_alloc_node_ppl
                            .record_dispatch_indirect(cmdbuf, resources.alloc_number_indirect());

                        shader_access_pipeline_barrier.record_insert(device, cmdbuf);

                        self.octree_modify_args_ppl.record_bind(cmdbuf);
                        self.octree_modify_args_ppl.record_bind_descriptor_sets(
                            cmdbuf,
                            std::slice::from_ref(&self.octree_modify_args_ds),
                            1,
                        );
                        self.octree_modify_args_ppl
                            .record_dispatch(cmdbuf, [1, 1, 1]);

                        shader_access_pipeline_barrier.record_insert(device, cmdbuf);
                        indirect_access_pipeline_barrier.record_insert(device, cmdbuf);
                    }
                }
            },
        );

        let octree_size = self.get_octree_data_size_in_bytes(resources);

        let write_offset = self.allocate_chunk(octree_size as u64, chunk_pos);

        self.copy_octree_data_single_to_octree_data(
            &vulkan_context,
            command_pool,
            resources,
            write_offset,
            octree_size as u64,
        );
    }

    fn allocate_chunk(&mut self, chunk_buffer_size: u64, chunk_pos: IVec3) -> u64 {
        let allocation = self
            .octree_buffer_allocator
            .allocate(chunk_buffer_size)
            .unwrap();
        self.offset_table
            .insert(chunk_pos, allocation.offset as u32);
        allocation.offset
    }
}
