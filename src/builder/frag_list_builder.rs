use std::collections::HashMap;

use super::Resources;
use crate::util::ShaderCompiler;
use crate::vkn::execute_one_time_command;
use crate::vkn::BufferBuilder;
use crate::vkn::CommandBuffer;
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
use gpu_allocator::vulkan;

pub struct FragListBuilder {
    init_buffers_ppl: ComputePipeline,
    frag_list_maker_ppl: ComputePipeline,

    init_buffers_ds: DescriptorSet,
    frag_list_maker_ds: DescriptorSet,

    cmdbuf: CommandBuffer,
}

impl FragListBuilder {
    pub fn new(
        vulkan_context: &VulkanContext,
        command_pool: &CommandPool,
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
        let frag_list_maker_ds = DescriptorSet::new(
            vulkan_context.device().clone(),
            &frag_list_maker_ppl
                .get_layout()
                .get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        frag_list_maker_ds.perform_writes(&[
            WriteDescriptorSet::new_buffer_write(0, resources.frag_list_maker_info()),
            WriteDescriptorSet::new_buffer_write(1, resources.neighbor_info()),
            WriteDescriptorSet::new_buffer_write(2, resources.raw_voxels()),
            WriteDescriptorSet::new_buffer_write(3, resources.frag_list_build_result()),
            WriteDescriptorSet::new_buffer_write(4, resources.fragment_list()),
        ]);

        let cmdbuf = Self::create_cmdbuf(
            vulkan_context,
            command_pool,
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
        command_pool: &CommandPool,
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

        let cmdbuf = CommandBuffer::new(device, &command_pool);
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
        vulkan_context: &VulkanContext,
        resources: &Resources,
        voxel_dim: UVec3,
        chunk_pos: IVec3,
        data_offset_table: &HashMap<IVec3, u32>,
    ) {
        let device = vulkan_context.device();

        update_uniforms(resources, voxel_dim);
        update_neighbor_buffer(resources, chunk_pos, data_offset_table);

        self.cmdbuf
            .submit(&vulkan_context.get_general_queue(), None);
        device.wait_queue_idle(&vulkan_context.get_general_queue());

        fn update_uniforms(resources: &Resources, voxel_dim: UVec3) {
            let data = BufferBuilder::from_struct_buffer(resources.frag_list_maker_info())
                .unwrap()
                .set_uvec3("voxel_dim", voxel_dim.to_array())
                .to_raw_data();
            resources
                .frag_list_maker_info()
                .fill_with_raw_u8(&data)
                .expect("Failed to fill buffer data");
        }

        fn update_neighbor_buffer(
            resources: &Resources,
            chunk_pos: IVec3,
            data_offset_table: &HashMap<IVec3, u32>,
        ) {
            const NEIGHBOR_COUNT: usize = 3 * 3 * 3;
            let mut neighbor_offsets: [u32; NEIGHBOR_COUNT] = [0; NEIGHBOR_COUNT];
            for i in -1..=1 {
                for j in -1..=1 {
                    for k in -1..=1 {
                        let neighbor_pos = chunk_pos + IVec3::new(i, j, k);

                        let offset: u32 = if let Some(offset) = data_offset_table.get(&neighbor_pos)
                        {
                            *offset
                        } else {
                            0xFFFFFFFF
                        };

                        let serialized_idx =
                            serialize(UVec3::new((i + 1) as u32, (j + 1) as u32, (k + 1) as u32));
                        neighbor_offsets[serialized_idx as usize] = offset;
                    }
                }
            }

            resources
                .neighbor_info()
                .fill_with_raw_u32(&neighbor_offsets)
                .unwrap();

            /// idx ranges from 0-3 in three dimensions
            fn serialize(idx: UVec3) -> u32 {
                return idx.x + idx.y * 3 + idx.z * 9;
            }
        }
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
