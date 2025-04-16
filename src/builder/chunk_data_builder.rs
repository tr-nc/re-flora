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
use std::collections::HashMap;

pub struct ChunkDataBuilder {
    chunk_init_ppl: ComputePipeline,
    chunk_init_ds: DescriptorSet,

    offset_table: HashMap<IVec3, u32>,
    /// Notice here's a limitation of total 4 GB addressing space because of u32,
    /// it should be enough, it's just raw data!
    /// TODO: use u32 offset so that it can address up to 16 GB of data
    write_offset: u32,
}

impl ChunkDataBuilder {
    pub fn new(
        vulkan_context: &VulkanContext,
        shader_compiler: &ShaderCompiler,
        descriptor_pool: DescriptorPool,
        resources: &Resources,
    ) -> Self {
        let chunk_init_ppl = ComputePipeline::from_shader_module(
            vulkan_context.device(),
            &ShaderModule::from_glsl(
                vulkan_context.device(),
                &shader_compiler,
                "shader/builder/chunk_init/chunk_init.comp",
                "main",
            )
            .unwrap(),
        );

        let chunk_init_ds = DescriptorSet::new(
            vulkan_context.device().clone(),
            &chunk_init_ppl.get_layout().get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        chunk_init_ds.perform_writes(&[
            WriteDescriptorSet::new_buffer_write(0, resources.chunk_init_info()),
            WriteDescriptorSet::new_buffer_write(1, resources.raw_voxels()),
        ]);

        Self {
            chunk_init_ppl,
            chunk_init_ds,

            offset_table: HashMap::new(),
            write_offset: 0,
        }
    }

    pub fn update_uniforms(&mut self, resources: &Resources, voxel_dim: UVec3, chunk_pos: IVec3) {
        let data = BufferBuilder::from_struct_buffer(resources.chunk_init_info())
            .unwrap()
            .set_uvec3("voxel_dim", voxel_dim.to_array())
            .set_ivec3("chunk_pos", chunk_pos.to_array())
            .set_uint("write_offset", self.write_offset)
            .to_raw_data();
        resources
            .chunk_init_info()
            .fill_with_raw_u8(&data)
            .expect("Failed to fill buffer data");

        self.offset_table.insert(chunk_pos, self.write_offset);
        self.write_offset += voxel_dim.x * voxel_dim.y * voxel_dim.z;
    }

    pub fn update_frag_list_maker_info_buf(&self, resources: &Resources, dimension: UVec3) {
        let data = BufferBuilder::from_struct_buffer(resources.frag_list_maker_info())
            .unwrap()
            .set_uvec3("voxel_dim", dimension.to_array())
            .to_raw_data();
        resources
            .frag_list_maker_info()
            .fill_with_raw_u8(&data)
            .expect("Failed to fill buffer data");
    }

    pub fn init_chunk_by_noise(
        &self,
        vulkan_context: &VulkanContext,
        command_pool: &CommandPool,
        dimension: UVec3,
    ) {
        execute_one_time_command(
            vulkan_context.device(),
            command_pool,
            &vulkan_context.get_general_queue(),
            |cmdbuf| {
                self.chunk_init_ppl.record_bind(cmdbuf);
                self.chunk_init_ppl.record_bind_descriptor_sets(
                    cmdbuf,
                    std::slice::from_ref(&self.chunk_init_ds),
                    0,
                );
                self.chunk_init_ppl
                    .record_dispatch(cmdbuf, dimension.to_array());
            },
        );
    }

    pub fn get_chunk_offset(&self, chunk_pos: IVec3) -> Option<u32> {
        if let Some(offset) = self.offset_table.get(&chunk_pos) {
            Some(*offset)
        } else {
            None
        }
    }
}
