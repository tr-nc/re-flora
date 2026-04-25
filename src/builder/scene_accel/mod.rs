mod resources;
use anyhow::Result;
use ash::vk;
use glam::UVec3;
pub use resources::*;
use std::time::Instant;

use crate::{
    generated::gpu_structs::SceneTexUpdateInfo,
    geom::UAabb3,
    util::ShaderCompiler,
    vkn::{
        execute_one_time_command, Allocator, Buffer, ClearValue, ColorClearValue, CommandBuffer,
        ComputePipeline, DescriptorPool, Extent3D, Fence, ShaderModule, VulkanContext,
    },
};
use bytemuck::Zeroable;

pub struct SceneAccelBuilder {
    pub vulkan_ctx: VulkanContext,
    pub resources: SceneAccelBuilderResources,

    #[allow(dead_code)]
    pool: DescriptorPool,

    #[allow(dead_code)]
    update_scene_tex_ppl: ComputePipeline,
    update_scene_tex_cmdbuf: CommandBuffer,
}

impl SceneAccelBuilder {
    pub fn new(
        vulkan_ctx: VulkanContext,
        allocator: Allocator,
        shader_compiler: &ShaderCompiler,
        chunk_bound: UAabb3,
    ) -> Result<Self> {
        let device = vulkan_ctx.device();
        let pool = DescriptorPool::new(device).unwrap();

        let update_scene_tex_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/scene_accel/update_scene_tex.comp",
            "main",
        )
        .unwrap();

        let resources = SceneAccelBuilderResources::new(
            device.clone(),
            allocator,
            chunk_bound,
            &update_scene_tex_sm,
        );

        let update_scene_tex_ppl =
            ComputePipeline::new(device, &update_scene_tex_sm, &pool, &[&resources]);

        let update_scene_tex_cmdbuf =
            Self::record_update_scene_tex_cmdbuf(vulkan_ctx.clone(), &update_scene_tex_ppl);

        Self::clear_tex(&vulkan_ctx, &resources);

        Ok(Self {
            vulkan_ctx,
            resources,
            pool,
            update_scene_tex_ppl,
            update_scene_tex_cmdbuf,
        })
    }

    fn record_update_scene_tex_cmdbuf(
        vulkan_ctx: VulkanContext,
        update_scene_tex_ppl: &ComputePipeline,
    ) -> CommandBuffer {
        let device = vulkan_ctx.device();
        let cmdbuf = CommandBuffer::new(device, vulkan_ctx.command_pool());
        cmdbuf.begin(false);

        let extent = Extent3D {
            width: 1,
            height: 1,
            depth: 1,
        };
        update_scene_tex_ppl.record(&cmdbuf, extent, None);

        cmdbuf.end();
        cmdbuf
    }

    /// Clears the scene offset texture to zero.
    ///
    /// Also can be used at init time since it can transfer the image layout to general.
    fn clear_tex(vulkan_context: &VulkanContext, resources: &SceneAccelBuilderResources) {
        execute_one_time_command(
            vulkan_context.device(),
            vulkan_context.command_pool(),
            &vulkan_context.get_general_queue(),
            |cmdbuf| {
                resources.scene_tex.get_image().record_clear(
                    cmdbuf,
                    Some(vk::ImageLayout::GENERAL),
                    0,
                    ClearValue::Color(ColorClearValue::UInt([0, 0, 0, 0])),
                );
            },
        );
    }

    pub fn update_scene_tex(
        &mut self,
        chunk_idx: UVec3,
        chunk_data: Option<(u64, u64)>,
    ) -> Result<()> {
        let total_start = Instant::now();
        let (node_offset_for_chunk, leaf_offset_for_chunk, is_valid) = match chunk_data {
            Some((node_offset, leaf_offset)) => (node_offset as u32, leaf_offset as u32, 1),
            None => (0, 0, 0),
        };

        let uniform_start = Instant::now();
        update_buffers(
            &self.resources.scene_tex_update_info,
            chunk_idx,
            node_offset_for_chunk,
            leaf_offset_for_chunk,
            is_valid,
        )?;
        crate::util::BENCH
            .lock()
            .unwrap()
            .record("scene_tex_update_uniform", uniform_start.elapsed());

        let gpu_start = Instant::now();
        let fence = Fence::new(self.vulkan_ctx.device(), false);
        self.update_scene_tex_cmdbuf
            .submit(&self.vulkan_ctx.get_general_queue(), Some(&fence));
        self.vulkan_ctx.wait_for_fences(&[fence.as_raw()]).unwrap();
        crate::util::BENCH
            .lock()
            .unwrap()
            .record("scene_tex_update_gpu", gpu_start.elapsed());
        crate::util::BENCH
            .lock()
            .unwrap()
            .record("scene_tex_update_total", total_start.elapsed());
        return Ok(());

        fn update_buffers(
            scene_tex_update_info: &Buffer,
            chunk_idx: UVec3,
            node_offset_for_chunk: u32,
            leaf_offset_for_chunk: u32,
            is_valid: u32,
        ) -> Result<()> {
            scene_tex_update_info.fill_uniform(&SceneTexUpdateInfo {
                chunk_idx: chunk_idx.to_array(),
                node_offset_for_chunk,
                leaf_offset_for_chunk,
                is_valid,
                ..SceneTexUpdateInfo::zeroed()
            })
        }
    }

    pub fn get_resources(&self) -> &SceneAccelBuilderResources {
        &self.resources
    }
}
