use std::collections::HashMap;

use super::Resources;
use crate::util::ShaderCompiler;
use crate::vkn::execute_one_time_command;
use crate::vkn::BufferBuilder;
use crate::vkn::CommandPool;
use crate::vkn::ComputePipeline;
use crate::vkn::DescriptorPool;
use crate::vkn::DescriptorSet;
use crate::vkn::ShaderModule;
use crate::vkn::VulkanContext;
use crate::vkn::WriteDescriptorSet;
use glam::IVec3;
use glam::UVec3;

pub struct FragListBuilder {
    frag_list_maker_ppl: ComputePipeline,
    frag_list_maker_ds: DescriptorSet,
}

impl FragListBuilder {
    pub fn new(
        vulkan_context: &VulkanContext,
        shader_compiler: &ShaderCompiler,
        descriptor_pool: DescriptorPool,
        resources: &Resources,
    ) -> Self {
        let frag_list_maker_sm = ShaderModule::from_glsl(
            vulkan_context.device(),
            &shader_compiler,
            "shader/builder/frag_list_maker/frag_list_maker.comp",
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
            WriteDescriptorSet::new_buffer_write(3, resources.fragment_list_info()),
            WriteDescriptorSet::new_buffer_write(4, resources.fragment_list()),
        ]);

        Self {
            frag_list_maker_ppl,
            frag_list_maker_ds,
        }
    }

    pub fn build(
        &self,
        vulkan_context: &VulkanContext,
        command_pool: &CommandPool,
        resources: &Resources,
        voxel_dim: UVec3,
        chunk_pos: IVec3,
        data_offset_table: &HashMap<IVec3, u32>,
    ) {
        update_frag_list_maker_info(resources, voxel_dim);
        update_neighbor_buffer(resources, chunk_pos, data_offset_table);
        reset_frag_list_info(resources);

        execute_one_time_command(
            vulkan_context.device(),
            command_pool,
            &vulkan_context.get_general_queue(),
            |cmdbuf| {
                self.frag_list_maker_ppl.record_bind(cmdbuf);
                self.frag_list_maker_ppl.record_bind_descriptor_sets(
                    cmdbuf,
                    std::slice::from_ref(&self.frag_list_maker_ds),
                    0,
                );
                self.frag_list_maker_ppl
                    .record_dispatch(cmdbuf, voxel_dim.to_array());
            },
        );

        fn update_frag_list_maker_info(resources: &Resources, voxel_dim: UVec3) {
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

        fn reset_frag_list_info(resources: &Resources) {
            let fragment_list_info_data =
                BufferBuilder::from_struct_buffer(resources.fragment_list_info())
                    .unwrap()
                    .set_uint("fragment_list_len", 0)
                    .to_raw_data();
            resources
                .fragment_list_info()
                .fill_with_raw_u8(&fragment_list_info_data)
                .expect("Failed to fill buffer data");
        }
    }

    pub fn get_fraglist_length(&self, resources: &Resources) -> u32 {
        let raw_data = resources.fragment_list_info().fetch_raw().unwrap();
        BufferBuilder::from_struct_buffer(resources.fragment_list_info())
            .unwrap()
            .set_raw(raw_data)
            .get_uint("fragment_list_len")
            .unwrap()
    }
}
