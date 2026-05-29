mod resources;
use anyhow::Result;
use glam::UVec3;
use re_flora_vkn::vk;
pub use resources::*;
use std::time::Instant;

use crate::{generated::gpu_structs::SceneTexUpdateInfo, geom::UAabb3, util::ShaderCompiler};
use bytemuck::Zeroable;
use re_flora_vkn::{
    execute_one_time_command, Allocator, Buffer, ClearValue, ColorClearValue, CommandBuffer,
    ComputePipeline, DescriptorPool, Extent3D, GpuJobToken, ShaderModule, VulkanContext,
};

pub struct SceneAccelBuilder {
    pub vulkan_ctx: VulkanContext,
    pub resources: SceneAccelBuilderResources,

    #[allow(dead_code)]
    pool: DescriptorPool,

    #[allow(dead_code)]
    update_scene_tex_ppl: ComputePipeline,
    update_scene_tex_cmdbuf: CommandBuffer,
}

pub struct SceneTexUpdateJob {
    chunk_idx: UVec3,
    total_start: Instant,
    submitted_at: Instant,
    uniform_elapsed: std::time::Duration,
    submit_elapsed: std::time::Duration,
    _command_buffer: CommandBuffer,
    gpu_job: GpuJobToken,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub struct SceneTexUpdateResult {
    pub chunk_idx: UVec3,
    pub uniform_ms: f64,
    pub gpu_submit_ms: f64,
    pub gpu_completion_latency_ms: f64,
    pub total_ms: f64,
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
        let job = self.submit_update_scene_tex(chunk_idx, chunk_data)?;
        job.gpu_job.wait()?;
        self.finish_update_scene_tex(job);
        Ok(())
    }

    pub fn submit_update_scene_tex(
        &mut self,
        chunk_idx: UVec3,
        chunk_data: Option<(u64, u64)>,
    ) -> Result<SceneTexUpdateJob> {
        let total_start = Instant::now();
        let (node_offset_for_chunk, leaf_offset_for_chunk, is_valid) = match chunk_data {
            Some((node_offset, leaf_offset)) => (node_offset as u32, leaf_offset as u32, 1),
            None => (0, 0, 0),
        };

        let uniform_start = Instant::now();
        update_scene_tex_info(
            &self.resources.scene_tex_update_info,
            chunk_idx,
            node_offset_for_chunk,
            leaf_offset_for_chunk,
            is_valid,
        )?;
        let uniform_elapsed = uniform_start.elapsed();
        crate::util::BENCH
            .lock()
            .unwrap()
            .record("scene_tex_update_uniform", uniform_elapsed);

        let submit_start = Instant::now();
        let cmdbuf = self.update_scene_tex_cmdbuf.clone();
        let gpu_job = cmdbuf.submit_gpu_job(
            &self.vulkan_ctx.get_general_queue(),
            "scene_accel.update_scene_tex",
        )?;
        let submitted_at = Instant::now();
        let submit_elapsed = submit_start.elapsed();
        crate::util::BENCH
            .lock()
            .unwrap()
            .record("scene_tex_update_submit", submit_elapsed);

        Ok(SceneTexUpdateJob {
            chunk_idx,
            total_start,
            submitted_at,
            uniform_elapsed,
            submit_elapsed,
            _command_buffer: cmdbuf,
            gpu_job,
        })
    }

    pub fn update_scene_tex_ready(&self, job: &SceneTexUpdateJob) -> Result<bool> {
        job.gpu_job
            .is_complete()
            .map_err(|err| anyhow::anyhow!("failed to poll scene tex update GPU job: {err}"))
    }

    pub fn wait_update_scene_tex(&self, job: &SceneTexUpdateJob) -> Result<()> {
        job.gpu_job.wait()?;
        Ok(())
    }

    pub fn finish_update_scene_tex(&mut self, job: SceneTexUpdateJob) -> SceneTexUpdateResult {
        let gpu_completion_latency_elapsed = job.submitted_at.elapsed();
        crate::util::BENCH.lock().unwrap().record(
            "scene_tex_update_gpu",
            gpu_completion_latency_elapsed + job.submit_elapsed,
        );
        let total_elapsed = job.total_start.elapsed();
        crate::util::BENCH
            .lock()
            .unwrap()
            .record("scene_tex_update_total", total_elapsed);

        SceneTexUpdateResult {
            chunk_idx: job.chunk_idx,
            uniform_ms: job.uniform_elapsed.as_secs_f64() * 1000.0,
            gpu_submit_ms: job.submit_elapsed.as_secs_f64() * 1000.0,
            gpu_completion_latency_ms: gpu_completion_latency_elapsed.as_secs_f64() * 1000.0,
            total_ms: total_elapsed.as_secs_f64() * 1000.0,
        }
    }

    pub fn get_resources(&self) -> &SceneAccelBuilderResources {
        &self.resources
    }
}

fn update_scene_tex_info(
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
