mod resources;
pub use resources::*;

use crate::{
    util::ShaderCompiler,
    vkn::{
        Allocator, CommandBuffer, ComputePipeline, DescriptorPool, DescriptorSet,
        PlainMemberTypeWithData, ShaderModule, StructMemberDataReader, VulkanContext,
        WriteDescriptorSet,
    },
};

pub struct AccelStructBuilder {
    vulkan_ctx: VulkanContext,
    resources: AccelStructResources,

    vert_maker_cmdbuf: CommandBuffer,
    _vert_maker_ds: DescriptorSet,
    _vert_maker_ppl: ComputePipeline,
}

impl AccelStructBuilder {
    pub fn new(
        vulkan_ctx: VulkanContext,
        allocator: Allocator,
        shader_compiler: &ShaderCompiler,
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

        let vertices_buffer_max_len = 10000;
        let indices_buffer_max_len = 10000;
        let resources = AccelStructResources::new(
            vulkan_ctx.clone(),
            allocator.clone(),
            &vert_maker_sm,
            vertices_buffer_max_len,
            indices_buffer_max_len,
        );

        let vert_maker_ppl = ComputePipeline::from_shader_module(device, &vert_maker_sm);
        let _vert_maker_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &vert_maker_ppl.get_layout().get_descriptor_set_layouts()[0],
            descriptor_pool.clone(),
        );
        _vert_maker_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.vertices),
            WriteDescriptorSet::new_buffer_write(1, &resources.indices),
            WriteDescriptorSet::new_buffer_write(2, &resources.vert_maker_result),
        ]);

        // TODO: maybe cache this later
        let vert_maker_cmdbuf =
            create_vert_maker_cmdbuf(&vulkan_ctx, &vert_maker_ppl, &_vert_maker_ds);

        return Self {
            vulkan_ctx,
            resources,
            _vert_maker_ppl: vert_maker_ppl,
            vert_maker_cmdbuf,
            _vert_maker_ds,
        };

        fn create_vert_maker_cmdbuf(
            vulkan_ctx: &VulkanContext,
            vert_maker_ppl: &ComputePipeline,
            vert_maker_ds: &DescriptorSet,
        ) -> CommandBuffer {
            let device = vulkan_ctx.device();

            let cmdbuf = CommandBuffer::new(device, vulkan_ctx.command_pool());
            cmdbuf.begin(true);

            vert_maker_ppl.record_bind(&cmdbuf);
            vert_maker_ppl.record_bind_descriptor_sets(
                &cmdbuf,
                std::slice::from_ref(vert_maker_ds),
                0,
            );
            vert_maker_ppl.record_dispatch(&cmdbuf, [4, 4, 4]);

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

        let valid_voxel_count = read_back_valid_voxel_count(&self.resources);
        log::debug!("Valid voxel count: {}", valid_voxel_count);

        if valid_voxel_count == 0 {
            // TODO: handle this case properly
            panic!("No valid voxels found!");
        }

        self.resources.blas.build(
            &self.resources.vertices,
            &self.resources.indices,
            valid_voxel_count,
        );
        self.resources.tlas.build(&self.resources.blas);

        fn read_back_valid_voxel_count(resources: &AccelStructResources) -> u32 {
            // read the reslt back
            let layout = &resources
                .vert_maker_result
                .get_layout()
                .unwrap()
                .root_member;
            let raw_data = resources.vert_maker_result.read_back().unwrap();
            let reader = StructMemberDataReader::new(layout, &raw_data);
            let field_val = reader.get_field("valid_voxel_count").unwrap();
            if let PlainMemberTypeWithData::UInt(val) = field_val {
                return val;
            } else {
                panic!("Invalid type for valid_voxel_count");
            }
        }
    }

    pub fn get_resources(&self) -> &AccelStructResources {
        &self.resources
    }
}
