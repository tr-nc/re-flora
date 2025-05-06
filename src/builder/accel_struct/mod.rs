mod resources;
use ash::vk;
use glam::UVec3;
pub use resources::*;

use crate::{
    util::ShaderCompiler,
    vkn::{
        Allocator, CommandBuffer, ComputePipeline, DescriptorPool, DescriptorSet,
        PlainMemberTypeWithData, ShaderModule, StructMemberDataBuilder, VulkanContext,
        WriteDescriptorSet,
    },
};

pub struct AccelStructBuilder {
    vulkan_ctx: VulkanContext,
    resources: AccelStructResources,

    _unit_cube_maker_ds: DescriptorSet,
    _unit_cube_maker_ppl: ComputePipeline,

    _chunk_instance_maker_ds: DescriptorSet,
    _chunk_instance_maker_ppl: ComputePipeline,

    unit_cube_maker_cmdbuf: CommandBuffer,
    chunk_instance_maker_cmdbuf: CommandBuffer,

    voxel_dim_per_chunk: UVec3,
}

impl AccelStructBuilder {
    pub fn new(
        vulkan_ctx: VulkanContext,
        allocator: Allocator,
        shader_compiler: &ShaderCompiler,
        voxel_dim_per_chunk: UVec3,
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

        const VOXELS_CAP: u64 = 1; // only a single voxel is needed for the only BLAS, then we can instantiate it more than once

        const VERTICES_COUNT_PER_VOXEL: u64 = 8;
        const PRIMITIVE_COUNT_PER_VOXEL: u64 = 12;
        const INDICES_COUNT_PER_PRIMITIVE: u64 = 3;

        let vertices_buffer_max_len = VOXELS_CAP * VERTICES_COUNT_PER_VOXEL;
        let indices_buffer_max_len =
            VOXELS_CAP * PRIMITIVE_COUNT_PER_VOXEL as u64 * INDICES_COUNT_PER_PRIMITIVE as u64;

        let resources = AccelStructResources::new(
            vulkan_ctx.clone(),
            allocator.clone(),
            &unit_cube_maker_sm,
            &instance_maker_sm,
            vertices_buffer_max_len,
            indices_buffer_max_len,
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

        let unit_cube_maker_cmdbuf = create_unit_cube_maker_cmdbuf(
            &vulkan_ctx,
            &unit_cube_maker_ppl,
            &unit_cube_maker_ds,
            voxel_dim_per_chunk,
        );
        let chunk_instance_maker_cmdbuf =
            create_instance_maker_cmdbuf(&vulkan_ctx, &instance_maker_ppl, &instance_maker_ds);

        let mut this = Self {
            vulkan_ctx,
            resources,

            _unit_cube_maker_ppl: unit_cube_maker_ppl,
            _unit_cube_maker_ds: unit_cube_maker_ds,

            _chunk_instance_maker_ppl: instance_maker_ppl,
            _chunk_instance_maker_ds: instance_maker_ds,

            unit_cube_maker_cmdbuf,
            chunk_instance_maker_cmdbuf,

            voxel_dim_per_chunk,
        };
        this.init();
        return this;

        fn create_unit_cube_maker_cmdbuf(
            vulkan_ctx: &VulkanContext,
            unit_cube_maker_ppl: &ComputePipeline,
            unit_cube_maker_ds: &DescriptorSet,
            voxel_dim_per_chunk: UVec3,
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
            unit_cube_maker_ppl.record_dispatch(&cmdbuf, voxel_dim_per_chunk.to_array());

            cmdbuf.end();
            return cmdbuf;
        }

        fn create_instance_maker_cmdbuf(
            vulkan_ctx: &VulkanContext,
            instance_maker_ppl: &ComputePipeline,
            instance_maker_ds: &DescriptorSet,
        ) -> CommandBuffer {
            let device = vulkan_ctx.device();

            let cmdbuf = CommandBuffer::new(device, vulkan_ctx.command_pool());
            cmdbuf.begin(false);

            instance_maker_ppl.record_bind(&cmdbuf);
            instance_maker_ppl.record_bind_descriptor_sets(
                &cmdbuf,
                std::slice::from_ref(instance_maker_ds),
                0,
            );
            instance_maker_ppl.record_dispatch(&cmdbuf, [1, 1, 1]); // TODO:

            cmdbuf.end();
            return cmdbuf;
        }
    }

    fn init(&mut self) {
        self.build_cube_blas();
    }

    fn build_cube_blas(&mut self) {
        self.unit_cube_maker_cmdbuf
            .submit(&self.vulkan_ctx.get_general_queue(), None);
        self.vulkan_ctx
            .device()
            .wait_queue_idle(&self.vulkan_ctx.get_general_queue());

        self.resources
            .blas
            .build(&self.resources.vertices, &self.resources.indices);
    }

    pub fn build_chunks_tlas(&mut self) {
        self.build_chunk_instances(
            1, // TODO:
            self.resources.blas.get_device_address().unwrap(),
        );

        self.resources.tlas.build(&self.resources.tlas_instances);
    }

    pub fn build_chunk_instances(&mut self, instance_count: u32, blas_device_address: u64) {
        update_buffers(&self.resources, instance_count, blas_device_address);

        self.chunk_instance_maker_cmdbuf
            .submit(&self.vulkan_ctx.get_general_queue(), None);
        self.vulkan_ctx
            .device()
            .wait_queue_idle(&self.vulkan_ctx.get_general_queue());

        fn update_buffers(
            resources: &AccelStructResources,
            instance_count: u32,
            blas_device_address: u64,
        ) {
            let lower = (blas_device_address & 0xFFFF_FFFF) as u32;
            let upper = (blas_device_address >> 32) as u32;

            // blas_device_address
            let data = StructMemberDataBuilder::from_buffer(&resources.instance_info)
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
            resources.instance_info.fill_with_raw_u8(&data).unwrap();
        }
    }

    pub fn get_resources(&self) -> &AccelStructResources {
        &self.resources
    }
}
