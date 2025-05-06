mod resources;
use glam::UVec3;
pub use resources::*;

use crate::{
    util::ShaderCompiler,
    vkn::{
        Allocator, CommandBuffer, ComputePipeline, DescriptorPool, DescriptorSet, ShaderModule,
        VulkanContext, WriteDescriptorSet,
    },
};

pub struct AccelStructBuilder {
    vulkan_ctx: VulkanContext,
    resources: AccelStructResources,

    vert_maker_cmdbuf: CommandBuffer,
    _vert_maker_ds: DescriptorSet,
    _vert_maker_ppl: ComputePipeline,

    voxel_dim_per_chunk: UVec3,
}

impl AccelStructBuilder {
    pub fn new(
        vulkan_ctx: VulkanContext,
        allocator: Allocator,
        shader_compiler: &ShaderCompiler,
        voxel_dim_per_chunk: UVec3,
        max_voxels: u64,
    ) -> Self {
        let descriptor_pool = DescriptorPool::a_big_one(vulkan_ctx.device()).unwrap();

        let device = vulkan_ctx.device();
        let vert_maker_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/acc_struct/vert_maker.comp",
            "main",
        )
        .unwrap();

        const VERTICES_COUNT_PER_VOXEL: u32 = 8;
        const PRIMITIVE_COUNT_PER_VOXEL: u32 = 12;
        const INDICES_COUNT_PER_PRIMITIVE: u32 = 3;

        let vertices_buffer_max_len = max_voxels * VERTICES_COUNT_PER_VOXEL as u64;
        let indices_buffer_max_len =
            max_voxels * PRIMITIVE_COUNT_PER_VOXEL as u64 * INDICES_COUNT_PER_PRIMITIVE as u64;
        let resources = AccelStructResources::new(
            vulkan_ctx.clone(),
            allocator.clone(),
            &vert_maker_sm,
            vertices_buffer_max_len,
            indices_buffer_max_len,
        );

        let vert_maker_ppl = ComputePipeline::from_shader_module(device, &vert_maker_sm);
        let vert_maker_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &vert_maker_ppl.get_layout().get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        vert_maker_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.vertices),
            WriteDescriptorSet::new_buffer_write(1, &resources.indices),
        ]);

        // TODO: maybe cache this later
        let vert_maker_cmdbuf = create_vert_maker_cmdbuf(
            &vulkan_ctx,
            &vert_maker_ppl,
            &vert_maker_ds,
            voxel_dim_per_chunk,
        );

        return Self {
            vulkan_ctx,
            resources,
            _vert_maker_ppl: vert_maker_ppl,
            vert_maker_cmdbuf,
            _vert_maker_ds: vert_maker_ds,
            voxel_dim_per_chunk,
        };

        fn create_vert_maker_cmdbuf(
            vulkan_ctx: &VulkanContext,
            vert_maker_ppl: &ComputePipeline,
            vert_maker_ds: &DescriptorSet,
            voxel_dim_per_chunk: UVec3,
        ) -> CommandBuffer {
            let device = vulkan_ctx.device();

            let cmdbuf = CommandBuffer::new(device, vulkan_ctx.command_pool());
            cmdbuf.begin(false);

            vert_maker_ppl.record_bind(&cmdbuf);
            vert_maker_ppl.record_bind_descriptor_sets(
                &cmdbuf,
                std::slice::from_ref(vert_maker_ds),
                0,
            );
            vert_maker_ppl.record_dispatch(&cmdbuf, voxel_dim_per_chunk.to_array());

            cmdbuf.end();
            return cmdbuf;
        }
    }

    pub fn build(&mut self) {
        self.vert_maker_cmdbuf
            .submit(&self.vulkan_ctx.get_general_queue(), None);
        self.vulkan_ctx
            .device()
            .wait_queue_idle(&self.vulkan_ctx.get_general_queue());

        self.resources
            .blas
            .build(&self.resources.vertices, &self.resources.indices);

        self.resources.tlas.build(&self.resources.blas);
    }

    pub fn get_resources(&self) -> &AccelStructResources {
        &self.resources
    }
}
