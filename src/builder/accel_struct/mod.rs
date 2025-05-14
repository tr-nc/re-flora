mod resources;

use ash::vk;
use glam::Vec3;
pub use resources::*;

use crate::{
    util::ShaderCompiler,
    vkn::{
        execute_one_time_command, Allocator, Buffer, CommandBuffer, ComputePipeline,
        DescriptorPool, DescriptorSet, PlainMemberTypeWithData, ShaderModule,
        StructMemberDataBuilder, VulkanContext, WriteDescriptorSet,
    },
};

pub struct AccelStructBuilder {
    vulkan_ctx: VulkanContext,
    resources: AccelStructResources,
    descriptor_pool: DescriptorPool,

    _unit_cube_maker_ds: DescriptorSet,
    _unit_cube_maker_ppl: ComputePipeline,

    instance_maker_ds: DescriptorSet,
    instance_maker_ppl: ComputePipeline,

    unit_cube_maker_cmdbuf: CommandBuffer,
}

impl AccelStructBuilder {
    pub fn new(
        vulkan_ctx: VulkanContext,
        allocator: Allocator,
        shader_compiler: &ShaderCompiler,
        tlas_instance_cap: u64,
    ) -> Self {
        let descriptor_pool = DescriptorPool::a_big_one(vulkan_ctx.device()).unwrap();

        let device = vulkan_ctx.device();
        let unit_cube_maker_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/acc_struct/unit_cube_maker.comp",
            "main",
        )
        .unwrap();
        let instance_maker_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/acc_struct/instance_maker.comp",
            "main",
        )
        .unwrap();

        let resources = AccelStructResources::new(
            vulkan_ctx.clone(),
            allocator.clone(),
            &unit_cube_maker_sm,
            &instance_maker_sm,
            2000,
            2000 * 3,
            tlas_instance_cap,
        );

        let unit_cube_maker_ppl = ComputePipeline::from_shader_module(device, &unit_cube_maker_sm);
        let instance_maker_ppl = ComputePipeline::from_shader_module(device, &instance_maker_sm);

        let unit_cube_maker_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &unit_cube_maker_ppl
                .get_layout()
                .get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        unit_cube_maker_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.vertices),
            WriteDescriptorSet::new_buffer_write(1, &resources.indices),
        ]);
        let instance_maker_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &instance_maker_ppl.get_layout().get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        instance_maker_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.instance_info),
            WriteDescriptorSet::new_buffer_write(1, &resources.instance_descriptor),
            WriteDescriptorSet::new_buffer_write(2, &resources.tlas_instances),
        ]);

        let unit_cube_maker_cmdbuf =
            create_unit_cube_maker_cmdbuf(&vulkan_ctx, &unit_cube_maker_ppl, &unit_cube_maker_ds);

        return Self {
            vulkan_ctx,
            resources,
            descriptor_pool,

            _unit_cube_maker_ppl: unit_cube_maker_ppl,
            _unit_cube_maker_ds: unit_cube_maker_ds,

            instance_maker_ppl,
            instance_maker_ds,

            unit_cube_maker_cmdbuf,
        };

        fn create_unit_cube_maker_cmdbuf(
            vulkan_ctx: &VulkanContext,
            unit_cube_maker_ppl: &ComputePipeline,
            unit_cube_maker_ds: &DescriptorSet,
        ) -> CommandBuffer {
            let device = vulkan_ctx.device();

            let cmdbuf = CommandBuffer::new(device, vulkan_ctx.command_pool());
            cmdbuf.begin(false);

            unit_cube_maker_ppl.record_bind(&cmdbuf);
            unit_cube_maker_ppl.record_bind_descriptor_sets(
                &cmdbuf,
                std::slice::from_ref(unit_cube_maker_ds),
                0,
            );
            unit_cube_maker_ppl.record_dispatch(&cmdbuf, [1, 1, 1]);

            cmdbuf.end();
            return cmdbuf;
        }
    }

    pub fn build_cube_blas(&mut self) {
        self.unit_cube_maker_cmdbuf
            .submit(&self.vulkan_ctx.get_general_queue(), None);
        self.vulkan_ctx
            .device()
            .wait_queue_idle(&self.vulkan_ctx.get_general_queue());
        self.resources.blas.build(
            &self.resources.vertices,
            &self.resources.indices,
            // this controls the culling mode, it can be overwritten by gl_RayFlagsNoneEXT in rayQuery
            vk::GeometryFlagsKHR::OPAQUE,
            86,
            171,
        );
    }

    pub fn build_tlas(&mut self, instances: &[(Vec3, u32)]) {
        // build the buffer first
        // this step takes 90% of the time! optimize it later
        self.build_tlas_instances(instances, self.resources.blas.get_device_address().unwrap());
        // then build the tlas using the buffer
        self.resources.tlas.build(
            &self.resources.tlas_instances,
            instances.len() as u32,
            vk::GeometryFlagsKHR::OPAQUE,
        );
    }

    fn build_tlas_instances(&mut self, instances: &[(Vec3, u32)], blas_device_address: u64) {
        update_buffers(&self.resources, instances, blas_device_address);

        let x_dispatch_count = instances.len() as u32;
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
                    .record_dispatch(&cmdbuf, [x_dispatch_count, 1, 1]);
            },
        );

        self.vulkan_ctx
            .device()
            .wait_queue_idle(&self.vulkan_ctx.get_general_queue());

        fn update_buffers(
            resources: &AccelStructResources,
            instances: &[(Vec3, u32)],
            blas_device_address: u64,
        ) {
            let instance_count = instances.len() as u32;

            update_instance_info(
                &resources.instance_info,
                instance_count,
                blas_device_address,
            );

            update_instance_descriptor(instances, &resources.instance_descriptor);

            fn update_instance_info(
                instance_info_buf: &Buffer,
                instance_count: u32,
                blas_device_address: u64,
            ) {
                // TODO: just use u64 directly with that extension in glsl
                let lower = (blas_device_address & 0xFFFF_FFFF) as u32;
                let upper = (blas_device_address >> 32) as u32;

                // blas_device_address
                let data = StructMemberDataBuilder::from_buffer(instance_info_buf)
                    .set_field(
                        "instance_count",
                        PlainMemberTypeWithData::UInt(instance_count),
                    )
                    .unwrap()
                    .set_field(
                        "blas_device_address",
                        PlainMemberTypeWithData::UVec2([lower, upper]),
                    )
                    .unwrap()
                    .get_data_u8();
                instance_info_buf.fill_with_raw_u8(&data).unwrap();
            }

            fn update_instance_descriptor(
                instances: &[(Vec3, u32)],
                instance_descriptor_buf: &Buffer,
            ) {
                const SCALE: f32 = 0.01;
                for (i, (pos, custom_idx)) in instances.iter().enumerate() {
                    let data = StructMemberDataBuilder::from_buffer(instance_descriptor_buf)
                        .set_field(
                            "data.position",
                            PlainMemberTypeWithData::Vec3(pos.to_array()),
                        )
                        .unwrap()
                        .set_field(
                            "data.rotation",
                            PlainMemberTypeWithData::Vec3([0.0, 0.0, 0.0]),
                        )
                        .unwrap()
                        .set_field(
                            "data.scale",
                            PlainMemberTypeWithData::Vec3([SCALE, SCALE, SCALE]),
                        )
                        .unwrap()
                        .set_field(
                            "data.custom_idx",
                            PlainMemberTypeWithData::UInt(*custom_idx as u32),
                        )
                        .unwrap()
                        .get_data_u8();
                    instance_descriptor_buf
                        .fill_element_with_raw_u8(&data, i as u64)
                        .unwrap();
                }
            }
        }
    }

    pub fn get_resources(&self) -> &AccelStructResources {
        &self.resources
    }
}
