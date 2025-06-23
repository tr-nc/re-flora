mod resources;

use crate::{
    builder::{instance::resources::InstanceResources, SurfaceResources},
    util::ShaderCompiler,
    vkn::{
        execute_one_time_command, Allocator, Buffer, ComputePipeline, DescriptorPool,
        DescriptorSet, PlainMemberTypeWithData, ShaderModule, StructMemberDataBuilder,
        VulkanContext, WriteDescriptorSet,
    },
};

pub struct InstanceBuilder {
    vulkan_ctx: VulkanContext,
    allocator: Allocator,

    resources: InstanceResources,

    instance_maker_ppl: ComputePipeline,
    instance_maker_ds: DescriptorSet,
}

impl InstanceBuilder {
    pub fn new(
        vulkan_ctx: VulkanContext,
        allocator: Allocator,
        shader_compiler: &ShaderCompiler,
        instance_cap: u64,
        surface_resources: &SurfaceResources,
    ) -> Self {
        let descriptor_pool = DescriptorPool::a_big_one(vulkan_ctx.device()).unwrap();

        let device = vulkan_ctx.device();
        let instance_maker_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/instance/instance_maker.comp",
            "main",
        )
        .unwrap();

        let resources = InstanceResources::new(
            vulkan_ctx.clone(),
            allocator.clone(),
            &instance_maker_sm,
            instance_cap,
        );

        let instance_maker_ppl = ComputePipeline::new(device, &instance_maker_sm);

        let instance_maker_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &instance_maker_ppl
                .get_layout()
                .get_descriptor_set_layouts()
                .get(&0)
                .unwrap(),
            descriptor_pool.clone(),
        );
        instance_maker_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.instance_build_info),
            WriteDescriptorSet::new_buffer_write(1, &surface_resources.grass_instances),
            WriteDescriptorSet::new_buffer_write(2, &resources.instances),
        ]);

        Self {
            vulkan_ctx,
            allocator,

            resources,

            instance_maker_ppl,
            instance_maker_ds,
        }
    }

    pub fn build_instances(&mut self, instance_count: u32) {
        update_buffers(&self.resources.instance_build_info, instance_count);

        execute_one_time_command(
            self.vulkan_ctx.device(),
            self.vulkan_ctx.command_pool(),
            &self.vulkan_ctx.get_general_queue(),
            |cmdbuf| {
                self.instance_maker_ppl.record_bind(&cmdbuf);
                self.instance_maker_ppl.record_bind_descriptor_sets(
                    &cmdbuf,
                    std::slice::from_ref(&self.instance_maker_ds),
                    0,
                );
                self.instance_maker_ppl
                    .record_dispatch(&cmdbuf, [instance_count, 1, 1]);
            },
        );

        fn update_buffers(instance_info: &Buffer, instance_count: u32) {
            let data = StructMemberDataBuilder::from_buffer(instance_info)
                .set_field(
                    "instance_count",
                    PlainMemberTypeWithData::UInt(instance_count),
                )
                .unwrap()
                .build();
            instance_info.fill_with_raw_u8(&data).unwrap();
        }
    }
}
