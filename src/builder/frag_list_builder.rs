use super::Resources;
use crate::util::compiler::ShaderCompiler;
use crate::vkn::execute_one_time_command;
use crate::vkn::BufferBuilder;
use crate::vkn::CommandPool;
use crate::vkn::ComputePipeline;
use crate::vkn::DescriptorPool;
use crate::vkn::DescriptorSet;
use crate::vkn::ShaderModule;
use crate::vkn::VulkanContext;
use crate::vkn::WriteDescriptorSet;
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

    pub fn reset_fragment_list_info_buf(&self, resources: &Resources) {
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

    pub fn make_frag_list(
        &self,
        vulkan_context: &VulkanContext,
        command_pool: &CommandPool,
        resolution: UVec3,
    ) {
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
                    .record_dispatch(cmdbuf, resolution.to_array());
            },
        );
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
