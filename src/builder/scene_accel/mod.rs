mod resources;
use ash::vk;
use glam::UVec3;
pub use resources::*;

use crate::{
    util::ShaderCompiler,
    vkn::{
        execute_one_time_command, Allocator, Buffer, ClearValue, CommandBuffer, ComputePipeline,
        DescriptorPool, DescriptorSet, PlainMemberTypeWithData, ShaderModule,
        StructMemberDataBuilder, VulkanContext, WriteDescriptorSet,
    },
};

pub struct SceneAccelBuilder {
    pub vulkan_ctx: VulkanContext,
    pub resources: SceneAccelResources,

    _update_scene_tex_ppl: ComputePipeline,
    _update_scene_tex_ds: DescriptorSet,

    update_scene_tex_cmdbuf: CommandBuffer,
}

impl SceneAccelBuilder {
    pub fn new(
        vulkan_ctx: VulkanContext,
        allocator: Allocator,
        shader_compiler: &ShaderCompiler,
        scene_chunk_dim: UVec3,
    ) -> Self {
        let descriptor_pool = DescriptorPool::a_big_one(vulkan_ctx.device()).unwrap();

        let update_scene_tex_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/builder/scene_accel/update_scene_tex.comp",
            "main",
        )
        .unwrap();

        let resources = SceneAccelResources::new(
            vulkan_ctx.device().clone(),
            allocator,
            scene_chunk_dim,
            &update_scene_tex_sm,
        );

        let update_scene_tex_ppl = ComputePipeline::new(vulkan_ctx.device(), &update_scene_tex_sm);

        let update_scene_tex_ds = DescriptorSet::new(
            vulkan_ctx.device().clone(),
            &update_scene_tex_ppl
                .get_layout()
                .get_descriptor_set_layouts()
                .get(&0)
                .unwrap(),
            descriptor_pool.clone(),
        );
        update_scene_tex_ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.scene_tex_update_info),
            WriteDescriptorSet::new_texture_write(
                1,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.scene_offset_tex,
                vk::ImageLayout::GENERAL,
            ),
        ]);

        let update_scene_tex_cmdbuf = record_update_scene_tex_cmdbuf(
            vulkan_ctx.clone(),
            &update_scene_tex_ppl,
            &update_scene_tex_ds,
        );

        Self::clear_tex(&vulkan_ctx, &resources);

        return Self {
            vulkan_ctx,
            resources,
            _update_scene_tex_ppl: update_scene_tex_ppl,
            _update_scene_tex_ds: update_scene_tex_ds,
            update_scene_tex_cmdbuf,
        };

        fn record_update_scene_tex_cmdbuf(
            vulkan_ctx: VulkanContext,
            update_scene_tex_ppl: &ComputePipeline,
            update_scene_tex_ds: &DescriptorSet,
        ) -> CommandBuffer {
            let device = vulkan_ctx.device();

            let cmdbuf = CommandBuffer::new(device, vulkan_ctx.command_pool());
            cmdbuf.begin(false);

            update_scene_tex_ppl.record_bind(&cmdbuf);
            update_scene_tex_ppl.record_bind_descriptor_sets(
                &cmdbuf,
                std::slice::from_ref(update_scene_tex_ds),
                0,
            );
            update_scene_tex_ppl.record_dispatch(&cmdbuf, [1, 1, 1]);

            cmdbuf.end();
            return cmdbuf;
        }
    }

    /// Clears the scene offset texture to zero.
    ///
    /// Also can be used at init time since it can transfer the image layout to general.
    fn clear_tex(vulkan_context: &VulkanContext, resources: &SceneAccelResources) {
        execute_one_time_command(
            vulkan_context.device(),
            vulkan_context.command_pool(),
            &vulkan_context.get_general_queue(),
            |cmdbuf| {
                resources.scene_offset_tex.get_image().record_clear(
                    cmdbuf,
                    Some(vk::ImageLayout::GENERAL),
                    0,
                    ClearValue::UInt([0, 0, 0, 0]),
                );
            },
        );
    }

    pub fn update_scene_tex(
        &mut self,
        chunk_idx: UVec3,
        node_offset_for_chunk: u64,
        node_count_for_chunk: u64,
    ) {
        update_buffers(
            &self.resources.scene_tex_update_info,
            chunk_idx,
            node_offset_for_chunk as u32,
            node_count_for_chunk as u32,
        );

        self.update_scene_tex_cmdbuf
            .submit(&self.vulkan_ctx.get_general_queue(), None);
        self.vulkan_ctx
            .device()
            .wait_queue_idle(&self.vulkan_ctx.get_general_queue());

        fn update_buffers(
            scene_tex_update_info: &Buffer,
            chunk_idx: UVec3,
            node_offset_for_chunk: u32,
            leaf_offset_for_chunk: u32,
        ) {
            let data = StructMemberDataBuilder::from_buffer(scene_tex_update_info)
                .set_field(
                    "chunk_idx",
                    PlainMemberTypeWithData::UVec3(chunk_idx.to_array()),
                )
                .unwrap()
                .set_field(
                    "node_offset_for_chunk",
                    PlainMemberTypeWithData::UInt(node_offset_for_chunk),
                )
                .unwrap()
                .set_field(
                    "leaf_offset_for_chunk",
                    PlainMemberTypeWithData::UInt(leaf_offset_for_chunk),
                )
                .unwrap()
                .get_data_u8();
            scene_tex_update_info.fill_with_raw_u8(&data).unwrap();
        }
    }

    pub fn get_resources(&self) -> &SceneAccelResources {
        &self.resources
    }
}
