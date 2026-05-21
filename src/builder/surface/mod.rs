mod resources;
use super::PlainBuilderResources;
use crate::{
    flora::species,
    generated::gpu_structs::{
        ClearOccupancyInfo, EditOccupancyInfo, InstancesToOccupancyInfo, MakeSurfaceInfo,
        OccupancyToInstancesInfo,
    },
    geom::UAabb3,
    util::ShaderCompiler,
    vkn::{
        Buffer, ClearValue, ColorClearValue, CommandBuffer, ComputePipeline, DescriptorPool,
        Extent3D, Fence, MemoryBarrier, PipelineBarrier, ShaderModule, VulkanContext,
        WriteDescriptorSet,
    },
};
use anyhow::Result;
use ash::vk;
use bytemuck::Zeroable;
use glam::{UVec3, Vec3};
pub use resources::*;
use std::time::{Duration, Instant};

#[derive(Copy, Clone, Eq, PartialEq)]
enum OccupancyEditMode {
    Remove = 0,
    Add = 1,
    Trim = 2,
}

#[allow(dead_code)]
pub struct FloraRegenStats {
    pub appended_total: u32,
    pub before_total: u32,
    pub after_total: u32,
    pub dispatch_dim: UVec3,
    pub has_growing_flora: bool,
}

struct OccupancyToInstancesResultReadback {
    flora_instance_len: Vec<u32>,
    has_growing_flora: bool,
}

struct MakeSurfaceResultReadback {
    active_voxel_len: u32,
    active_brick_len: u32,
}

pub struct SurfaceBuildJob {
    chunk_id: UVec3,
    place_flora: bool,
    total_start: Instant,
    submitted_at: Instant,
    setup_elapsed: Duration,
    record_elapsed: Duration,
    submit_elapsed: Duration,
    _command_buffer: CommandBuffer,
    fence: Fence,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct SurfaceBuildResult {
    pub chunk_id: UVec3,
    pub active_voxel_len: u32,
    pub active_brick_len: u32,
    pub place_flora: bool,
    pub flora_rebuilt: bool,
    pub setup_ms: f64,
    pub record_ms: f64,
    pub gpu_submit_ms: f64,
    pub fence_latency_ms: f64,
    pub readback_ms: f64,
    pub flora_ms: f64,
    pub total_ms: f64,
}

#[derive(Clone, Copy)]
struct SurfacePassTimingPass {
    label: &'static str,
    bench_key: &'static str,
}

struct SurfacePassTiming {
    device: crate::vkn::Device,
    query_pool: vk::QueryPool,
    timestamp_period_ns: f32,
    max_query_count: u32,
}

impl Drop for SurfacePassTiming {
    fn drop(&mut self) {
        unsafe {
            self.device
                .as_raw()
                .destroy_query_pool(self.query_pool, None);
        }
    }
}

impl SurfacePassTiming {
    fn maybe_new(vulkan_ctx: &VulkanContext) -> Option<Self> {
        if !log::log_enabled!(target: module_path!(), log::Level::Debug) {
            return None;
        }

        let properties = unsafe {
            vulkan_ctx
                .instance()
                .as_raw()
                .get_physical_device_properties(vulkan_ctx.physical_device().as_raw())
        };
        if properties.limits.timestamp_compute_and_graphics != vk::TRUE {
            log::debug!(
                "[PERF][SURFACE_PASS_TIMING] disabled: timestamp_compute_and_graphics unsupported"
            );
            return None;
        }
        if properties.limits.timestamp_period <= 0.0 {
            log::debug!(
                "[PERF][SURFACE_PASS_TIMING] disabled: timestamp_period={}ns",
                properties.limits.timestamp_period
            );
            return None;
        }

        let max_pass_count = SURFACE_BUILD_TIMING_PASSES
            .len()
            .max(FLORA_REBUILD_TIMING_PASSES.len())
            .max(FLORA_EDIT_TIMING_PASSES_WITH_INSTANCES.len())
            .max(FLORA_GROWTH_TIMING_PASSES.len());
        let max_query_count = (max_pass_count * 2) as u32;
        if max_query_count == 0 {
            return None;
        }

        let create_info = vk::QueryPoolCreateInfo::default()
            .query_type(vk::QueryType::TIMESTAMP)
            .query_count(max_query_count);
        let device = vulkan_ctx.device().clone();
        let query_pool = match unsafe { device.as_raw().create_query_pool(&create_info, None) } {
            Ok(pool) => pool,
            Err(err) => {
                log::warn!("[PERF][SURFACE_PASS_TIMING] disabled: query pool create failed: {err}");
                return None;
            }
        };

        log::debug!(
            "[PERF][SURFACE_PASS_TIMING] enabled max_passes={} queries={} timestamp_period_ns={:.3}",
            max_pass_count,
            max_query_count,
            properties.limits.timestamp_period,
        );

        Some(Self {
            device,
            query_pool,
            timestamp_period_ns: properties.limits.timestamp_period,
            max_query_count,
        })
    }

    fn record_reset(&self, cmdbuf: &CommandBuffer, pass_count: usize) {
        let query_count = self.query_count(pass_count);
        unsafe {
            self.device.as_raw().cmd_reset_query_pool(
                cmdbuf.as_raw(),
                self.query_pool,
                0,
                query_count,
            );
        }
    }

    fn record_start(&self, cmdbuf: &CommandBuffer, pass_index: usize) {
        self.record_timestamp(cmdbuf, pass_index * 2);
    }

    fn record_end(&self, cmdbuf: &CommandBuffer, pass_index: usize) {
        self.record_timestamp(cmdbuf, pass_index * 2 + 1);
    }

    fn record_timestamp(&self, cmdbuf: &CommandBuffer, query_index: usize) {
        unsafe {
            self.device.as_raw().cmd_write_timestamp(
                cmdbuf.as_raw(),
                vk::PipelineStageFlags::ALL_COMMANDS,
                self.query_pool,
                query_index as u32,
            );
        }
    }

    fn collect_and_log(
        &self,
        log_tag: &'static str,
        total_bench_key: &'static str,
        chunk_id: UVec3,
        passes: &[SurfacePassTimingPass],
    ) {
        let query_count = self.query_count(passes.len());
        let mut timestamps = vec![0_u64; query_count as usize];
        let readback_start = Instant::now();
        let result = unsafe {
            self.device.as_raw().get_query_pool_results(
                self.query_pool,
                0,
                &mut timestamps,
                vk::QueryResultFlags::TYPE_64 | vk::QueryResultFlags::WAIT,
            )
        };
        crate::util::BENCH
            .lock()
            .unwrap()
            .record("surface_pass_timestamp_readback", readback_start.elapsed());

        if let Err(err) = result {
            log::warn!(
                "[PERF][{}] chunk {:?} query readback failed: {err}",
                log_tag,
                chunk_id
            );
            return;
        }

        let mut parts = Vec::with_capacity(passes.len());
        let mut total_ms = 0.0;
        let mut bench = crate::util::BENCH.lock().unwrap();
        for (pass_index, pass) in passes.iter().enumerate() {
            let start = timestamps[pass_index * 2];
            let end = timestamps[pass_index * 2 + 1];
            if end < start {
                log::debug!(
                    "[PERF][{}] chunk {:?} pass {} timestamp wrapped or reordered start={} end={}",
                    log_tag,
                    chunk_id,
                    pass.label,
                    start,
                    end,
                );
                continue;
            }

            let duration_ms = (end - start) as f64 * self.timestamp_period_ns as f64 / 1_000_000.0;
            total_ms += duration_ms;
            bench.record(
                pass.bench_key,
                Duration::from_secs_f64(duration_ms / 1000.0),
            );
            parts.push(format!("{}={:.3}ms", pass.label, duration_ms));
        }
        bench.record(total_bench_key, Duration::from_secs_f64(total_ms / 1000.0));
        drop(bench);

        log::debug!(
            "[PERF][{}] chunk {:?} pass_total={:.3}ms {}",
            log_tag,
            chunk_id,
            total_ms,
            parts.join(" "),
        );
    }

    fn query_count(&self, pass_count: usize) -> u32 {
        let query_count = (pass_count * 2) as u32;
        assert!(
            query_count <= self.max_query_count,
            "surface pass timing query count {} exceeds pool capacity {}",
            query_count,
            self.max_query_count,
        );
        query_count
    }
}

const SURFACE_BUILD_TIMING_PASSES: [SurfacePassTimingPass; 4] = [
    SurfacePassTimingPass {
        label: "surface_clear",
        bench_key: "surface_pass_surface_clear_gpu",
    },
    SurfacePassTimingPass {
        label: "make_surface_result_clear",
        bench_key: "surface_pass_make_surface_result_clear_gpu",
    },
    SurfacePassTimingPass {
        label: "active_brick_flags_clear",
        bench_key: "surface_pass_active_brick_flags_clear_gpu",
    },
    SurfacePassTimingPass {
        label: "make_surface",
        bench_key: "surface_pass_make_surface_gpu",
    },
];

const FLORA_REBUILD_TIMING_PASSES: [SurfacePassTimingPass; 1] = [SurfacePassTimingPass {
    label: "active_surface_to_flora",
    bench_key: "flora_rebuild_pass_active_surface_to_flora_gpu",
}];

const FLORA_EDIT_TIMING_PASSES_WITH_INSTANCES: [SurfacePassTimingPass; 4] = [
    SurfacePassTimingPass {
        label: "clear_occupancy",
        bench_key: "flora_edit_pass_clear_occupancy_gpu",
    },
    SurfacePassTimingPass {
        label: "instances_to_occupancy",
        bench_key: "flora_edit_pass_instances_to_occupancy_gpu",
    },
    SurfacePassTimingPass {
        label: "edit_occupancy",
        bench_key: "flora_edit_pass_edit_occupancy_gpu",
    },
    SurfacePassTimingPass {
        label: "occupancy_to_instances",
        bench_key: "flora_edit_pass_occupancy_to_instances_gpu",
    },
];

const FLORA_EDIT_TIMING_PASSES_WITHOUT_INSTANCES: [SurfacePassTimingPass; 3] = [
    SurfacePassTimingPass {
        label: "clear_occupancy",
        bench_key: "flora_edit_pass_clear_occupancy_gpu",
    },
    SurfacePassTimingPass {
        label: "edit_occupancy",
        bench_key: "flora_edit_pass_edit_occupancy_gpu",
    },
    SurfacePassTimingPass {
        label: "occupancy_to_instances",
        bench_key: "flora_edit_pass_occupancy_to_instances_gpu",
    },
];

const FLORA_GROWTH_TIMING_PASSES: [SurfacePassTimingPass; 1] = [SurfacePassTimingPass {
    label: "update_flora_growth",
    bench_key: "flora_growth_pass_update_gpu",
}];

pub struct SurfaceBuilder {
    vulkan_ctx: VulkanContext,
    pub resources: SurfaceResources,

    #[allow(dead_code)]
    pool: DescriptorPool,

    make_surface_ppl: ComputePipeline,
    clear_occupancy_ppl: ComputePipeline,
    instances_to_occupancy_ppl: ComputePipeline,
    edit_occupancy_ppl: ComputePipeline,
    occupancy_to_instances_ppl: ComputePipeline,
    active_surface_to_flora_ppl: ComputePipeline,
    update_flora_growth_ppl: ComputePipeline,
    pass_timing: Option<SurfacePassTiming>,

    chunk_bound: UAabb3,
    voxel_dim_per_chunk: UVec3,
    flora_species_count: usize,
}

impl SurfaceBuilder {
    pub fn new(
        vulkan_ctx: VulkanContext,
        allocator: crate::vkn::Allocator,
        shader_compiler: &ShaderCompiler,
        plain_builder_resources: &PlainBuilderResources,
        voxel_dim_per_chunk: UVec3,
        chunk_bound: UAabb3,
    ) -> Self {
        let device = vulkan_ctx.device();
        species::assert_species_limit();
        let flora_species_count = species::species_count();

        let make_surface_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/surface/make_surface.comp",
            "main",
        )
        .unwrap();

        let clear_occupancy_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/surface/clear_occupancy.comp",
            "main",
        )
        .unwrap();

        let instances_to_occupancy_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/surface/instances_to_occupancy.comp",
            "main",
        )
        .unwrap();

        let edit_occupancy_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/surface/edit_occupancy_sphere.comp",
            "main",
        )
        .unwrap();

        let occupancy_to_instances_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/surface/occupancy_to_flora_instances.comp",
            "main",
        )
        .unwrap();

        let active_surface_to_flora_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/surface/active_surface_to_flora_instances.comp",
            "main",
        )
        .unwrap();

        let update_flora_growth_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/surface/update_flora_growth.comp",
            "main",
        )
        .unwrap();

        let resources = SurfaceResources::new(
            device.clone(),
            allocator,
            voxel_dim_per_chunk,
            &make_surface_sm,
            &clear_occupancy_sm,
            &instances_to_occupancy_sm,
            &edit_occupancy_sm,
            &occupancy_to_instances_sm,
            &update_flora_growth_sm,
            chunk_bound,
        );

        let pool = DescriptorPool::new(device).unwrap();

        let make_surface_ppl = ComputePipeline::new(
            device,
            &make_surface_sm,
            &pool,
            &[&resources, plain_builder_resources],
        );
        let clear_occupancy_ppl = ComputePipeline::new(
            device,
            &clear_occupancy_sm,
            &pool,
            &[&resources, plain_builder_resources],
        );
        let instances_to_occupancy_ppl = ComputePipeline::new(
            device,
            &instances_to_occupancy_sm,
            &pool,
            &[&resources, plain_builder_resources],
        );
        let edit_occupancy_ppl = ComputePipeline::new(
            device,
            &edit_occupancy_sm,
            &pool,
            &[&resources, plain_builder_resources],
        );
        let occupancy_to_instances_ppl = ComputePipeline::new(
            device,
            &occupancy_to_instances_sm,
            &pool,
            &[&resources, plain_builder_resources],
        );
        let active_surface_to_flora_ppl = ComputePipeline::new(
            device,
            &active_surface_to_flora_sm,
            &pool,
            &[&resources, plain_builder_resources],
        );
        let update_flora_growth_ppl = ComputePipeline::new(
            device,
            &update_flora_growth_sm,
            &pool,
            &[&resources, plain_builder_resources],
        );

        let pass_timing = SurfacePassTiming::maybe_new(&vulkan_ctx);

        Self {
            vulkan_ctx,
            resources,
            pool,
            make_surface_ppl,
            clear_occupancy_ppl,
            instances_to_occupancy_ppl,
            edit_occupancy_ppl,
            occupancy_to_instances_ppl,
            active_surface_to_flora_ppl,
            update_flora_growth_ppl,
            pass_timing,
            chunk_bound,
            voxel_dim_per_chunk,
            flora_species_count,
        }
    }

    pub fn build_surface(&mut self, chunk_id: UVec3, place_flora: bool) -> Result<u32> {
        let job = self.submit_build_surface(chunk_id, place_flora)?;
        self.vulkan_ctx.wait_for_fences(&[job.fence.as_raw()])?;
        let result = self.finish_build_surface(job)?;
        Ok(result.active_voxel_len)
    }

    pub fn submit_build_surface(
        &mut self,
        chunk_id: UVec3,
        place_flora: bool,
    ) -> Result<SurfaceBuildJob> {
        if !self.chunk_bound.in_bound(chunk_id) {
            return Err(anyhow::anyhow!("Chunk ID out of bounds"));
        }

        let total_start = Instant::now();
        let atlas_read_offset = chunk_id * self.voxel_dim_per_chunk;
        let atlas_read_dim = self.voxel_dim_per_chunk;
        let device = self.vulkan_ctx.device();

        let setup_start = Instant::now();
        update_make_surface_info(
            &self.resources.make_surface_info,
            atlas_read_offset,
            atlas_read_dim,
            true,
        )?;
        let setup_elapsed = setup_start.elapsed();

        let record_start = Instant::now();
        let cmdbuf = CommandBuffer::new(device, self.vulkan_ctx.command_pool());
        cmdbuf.begin(true);

        let pass_timing = self.pass_timing.as_ref();
        if let Some(timing) = pass_timing {
            timing.record_reset(&cmdbuf, SURFACE_BUILD_TIMING_PASSES.len());
        }
        let mut timing_pass_index = 0usize;
        macro_rules! record_timed_surface_pass {
            ($body:block) => {{
                if let Some(timing) = pass_timing {
                    timing.record_start(&cmdbuf, timing_pass_index);
                }
                let result = { $body };
                if let Some(timing) = pass_timing {
                    timing.record_end(&cmdbuf, timing_pass_index);
                    timing_pass_index += 1;
                }
                result
            }};
        }

        record_timed_surface_pass!({
            self.resources.surface.get_image().record_clear(
                &cmdbuf,
                Some(vk::ImageLayout::GENERAL),
                0,
                ClearValue::Color(ColorClearValue::UInt([0, 0, 0, 0])),
            );
        });
        record_timed_surface_pass!({
            record_clear_buffer_for_compute(device, &cmdbuf, &self.resources.make_surface_result);
        });
        record_timed_surface_pass!({
            record_clear_buffer_for_compute(
                device,
                &cmdbuf,
                &self.resources.surface_active_brick_flags,
            );
        });

        let extent = Extent3D {
            width: self.voxel_dim_per_chunk.x,
            height: self.voxel_dim_per_chunk.y,
            depth: self.voxel_dim_per_chunk.z,
        };
        record_timed_surface_pass!({
            self.make_surface_ppl.record(&cmdbuf, extent, None);
        });

        if pass_timing.is_some() {
            assert_eq!(timing_pass_index, SURFACE_BUILD_TIMING_PASSES.len());
        }

        cmdbuf.end();
        let record_elapsed = record_start.elapsed();

        let submit_start = Instant::now();
        let fence = Fence::new(device, false);
        cmdbuf.submit(&self.vulkan_ctx.get_general_queue(), Some(&fence));
        let submitted_at = Instant::now();
        let submit_elapsed = submit_start.elapsed();

        Ok(SurfaceBuildJob {
            chunk_id,
            place_flora,
            total_start,
            submitted_at,
            setup_elapsed,
            record_elapsed,
            submit_elapsed,
            _command_buffer: cmdbuf,
            fence,
        })
    }

    pub fn build_surface_ready(&self, job: &SurfaceBuildJob) -> Result<bool> {
        unsafe {
            self.vulkan_ctx
                .device()
                .as_raw()
                .get_fence_status(job.fence.as_raw())
        }
        .map_err(|err| anyhow::anyhow!("failed to poll surface build fence: {err}"))
    }

    pub fn wait_build_surface(&self, job: &SurfaceBuildJob) -> Result<()> {
        self.vulkan_ctx.wait_for_fences(&[job.fence.as_raw()])?;
        Ok(())
    }

    pub fn finish_build_surface(&mut self, job: SurfaceBuildJob) -> Result<SurfaceBuildResult> {
        let fence_latency_elapsed = job.submitted_at.elapsed();
        let readback_start = Instant::now();
        let make_surface_result = get_make_surface_result(&self.resources.make_surface_result);
        let active_voxel_len = make_surface_result.active_voxel_len;
        let active_brick_len = make_surface_result.active_brick_len;
        let readback_elapsed = readback_start.elapsed();

        if let Some(timing) = self.pass_timing.as_ref() {
            timing.collect_and_log(
                "SURFACE_BUILD_PASS_TIMING",
                "surface_pass_timed_total_gpu",
                job.chunk_id,
                &SURFACE_BUILD_TIMING_PASSES,
            );
        }

        let should_rebuild_flora = job.place_flora && active_voxel_len > 0;
        let flora_start = Instant::now();
        if should_rebuild_flora {
            self.seed_and_rebuild_flora_from_surface(job.chunk_id, active_brick_len, 0)?;
        }
        let flora_elapsed = flora_start.elapsed();
        let total_elapsed = job.total_start.elapsed();

        log::debug!(
            "[PERF][SURFACE_BUILD] chunk {:?} total {:.2}ms setup {:.2}ms record {:.2}ms gpu_submit {:.2}ms fence_latency {:.2}ms readback {:.2}ms flora {:.2}ms active_voxels {} active_bricks {} place_flora {} flora_rebuilt {}",
            job.chunk_id,
            total_elapsed.as_secs_f32() * 1000.0,
            job.setup_elapsed.as_secs_f32() * 1000.0,
            job.record_elapsed.as_secs_f32() * 1000.0,
            job.submit_elapsed.as_secs_f32() * 1000.0,
            fence_latency_elapsed.as_secs_f32() * 1000.0,
            readback_elapsed.as_secs_f32() * 1000.0,
            flora_elapsed.as_secs_f32() * 1000.0,
            active_voxel_len,
            active_brick_len,
            job.place_flora,
            should_rebuild_flora,
        );

        Ok(SurfaceBuildResult {
            chunk_id: job.chunk_id,
            active_voxel_len,
            active_brick_len,
            place_flora: job.place_flora,
            flora_rebuilt: should_rebuild_flora,
            setup_ms: job.setup_elapsed.as_secs_f64() * 1000.0,
            record_ms: job.record_elapsed.as_secs_f64() * 1000.0,
            gpu_submit_ms: job.submit_elapsed.as_secs_f64() * 1000.0,
            fence_latency_ms: fence_latency_elapsed.as_secs_f64() * 1000.0,
            readback_ms: readback_elapsed.as_secs_f64() * 1000.0,
            flora_ms: flora_elapsed.as_secs_f64() * 1000.0,
            total_ms: total_elapsed.as_secs_f64() * 1000.0,
        })
    }

    pub fn edit_flora_instances(
        &mut self,
        chunk_id: UVec3,
        edit_center: Vec3,
        edit_radius: f32,
        flora_tick: u32,
    ) -> Result<()> {
        let _ = self.run_occupancy_edit(
            chunk_id,
            edit_center,
            edit_radius,
            flora_tick,
            0,
            OccupancyEditMode::Remove,
        )?;
        Ok(())
    }

    pub fn regenerate_flora_instances(
        &mut self,
        chunk_id: UVec3,
        edit_center: Vec3,
        edit_radius: f32,
        flora_tick: u32,
    ) -> Result<FloraRegenStats> {
        self.run_occupancy_edit(
            chunk_id,
            edit_center,
            edit_radius,
            flora_tick,
            0,
            OccupancyEditMode::Add,
        )
    }

    pub fn trim_flora_instances(
        &mut self,
        chunk_id: UVec3,
        edit_center: Vec3,
        edit_radius: f32,
        flora_tick: u32,
        target_age: u32,
    ) -> Result<FloraRegenStats> {
        self.run_occupancy_edit(
            chunk_id,
            edit_center,
            edit_radius,
            flora_tick,
            target_age,
            OccupancyEditMode::Trim,
        )
    }

    fn seed_and_rebuild_flora_from_surface(
        &mut self,
        chunk_id: UVec3,
        active_brick_len: u32,
        _flora_tick: u32,
    ) -> Result<()> {
        let active_surface_dispatch_len = active_brick_len.saturating_mul(64);
        if active_surface_dispatch_len == 0 {
            return Ok(());
        }

        let chunk_world_offset = chunk_id * self.voxel_dim_per_chunk;
        update_occupancy_to_instances_info(
            &self.resources.occupancy_to_instances_info,
            chunk_world_offset,
            self.voxel_dim_per_chunk,
        )?;
        cleanup_occupancy_to_instances_result(&self.resources.occupancy_to_instances_result)?;

        let chunk_idx = self.get_chunk_resource_index(chunk_id)?;
        let chunk_resources = &self.resources.instances.chunk_flora_instances[chunk_idx]
            .1
            .resources;
        self.bind_manual_instance_buffers(&self.active_surface_to_flora_ppl, chunk_resources);

        let device = self.vulkan_ctx.device();
        let cmdbuf = CommandBuffer::new(device, self.vulkan_ctx.command_pool());
        cmdbuf.begin(true);

        let pass_timing = self.pass_timing.as_ref();
        if let Some(timing) = pass_timing {
            timing.record_reset(&cmdbuf, FLORA_REBUILD_TIMING_PASSES.len());
        }
        let mut timing_pass_index = 0usize;
        macro_rules! record_timed_flora_pass {
            ($body:block) => {{
                if let Some(timing) = pass_timing {
                    timing.record_start(&cmdbuf, timing_pass_index);
                }
                let result = { $body };
                if let Some(timing) = pass_timing {
                    timing.record_end(&cmdbuf, timing_pass_index);
                    timing_pass_index += 1;
                }
                result
            }};
        }

        let active_surface_to_flora_push = [active_brick_len, 0, 0, 0];
        record_timed_flora_pass!({
            self.active_surface_to_flora_ppl.record(
                &cmdbuf,
                Extent3D::new(active_surface_dispatch_len, 1, 1),
                Some(bytemuck::bytes_of(&active_surface_to_flora_push)),
            );
        });

        if pass_timing.is_some() {
            assert_eq!(timing_pass_index, FLORA_REBUILD_TIMING_PASSES.len());
        }

        cmdbuf.end();
        cmdbuf.submit(&self.vulkan_ctx.get_general_queue(), None);
        device.wait_queue_idle(&self.vulkan_ctx.get_general_queue());

        if let Some(timing) = self.pass_timing.as_ref() {
            timing.collect_and_log(
                "FLORA_REBUILD_PASS_TIMING",
                "flora_rebuild_pass_timed_total_gpu",
                chunk_id,
                &FLORA_REBUILD_TIMING_PASSES,
            );
        }

        let result = get_occupancy_to_instances_result(
            &self.resources.occupancy_to_instances_result,
            self.flora_species_count,
        );
        let chunk_resources_mut = &mut self.resources.instances.chunk_flora_instances[chunk_idx].1;
        for (species_idx, len) in result.flora_instance_len.iter().enumerate() {
            chunk_resources_mut.get_mut(species_idx).instances_len = *len;
        }

        Ok(())
    }

    fn run_occupancy_edit(
        &mut self,
        chunk_id: UVec3,
        edit_center: Vec3,
        edit_radius: f32,
        flora_tick: u32,
        target_age: u32,
        mode: OccupancyEditMode,
    ) -> Result<FloraRegenStats> {
        if !self.chunk_bound.in_bound(chunk_id) {
            return Err(anyhow::anyhow!("Chunk ID out of bounds"));
        }

        let chunk_idx = self.get_chunk_resource_index(chunk_id)?;
        let chunk_world_offset = chunk_id * self.voxel_dim_per_chunk;
        let edit_center_vox = edit_center * 256.0;
        let edit_radius_vox = edit_radius * 256.0;

        let before_total = self.resources.instances.chunk_flora_instances[chunk_idx]
            .1
            .iter()
            .fold(0_u32, |acc, r| acc.saturating_add(r.instances_len));

        let mut species_len = [0_u32; 4];
        let mut max_len = 0_u32;
        for (species_idx, species) in species_len
            .iter_mut()
            .enumerate()
            .take(self.flora_species_count.min(4))
        {
            let len = self.resources.instances.chunk_flora_instances[chunk_idx]
                .1
                .get(species_idx)
                .instances_len;
            *species = len;
            max_len = max_len.max(len);
        }

        update_clear_occupancy_info(
            &self.resources.clear_occupancy_info,
            self.voxel_dim_per_chunk,
        )?;
        update_instances_to_occupancy_info(
            &self.resources.instances_to_occupancy_info,
            chunk_world_offset,
            self.voxel_dim_per_chunk,
            species_len,
            0u32, // tick_delta=0 for rebuild
        )?;
        update_edit_occupancy_info(
            &self.resources.edit_occupancy_info,
            edit_center_vox,
            edit_radius_vox,
            chunk_world_offset,
            self.voxel_dim_per_chunk,
            mode,
            flora_tick,
            target_age,
        )?;
        update_occupancy_to_instances_info(
            &self.resources.occupancy_to_instances_info,
            chunk_world_offset,
            self.voxel_dim_per_chunk,
        )?;
        cleanup_occupancy_to_instances_result(&self.resources.occupancy_to_instances_result)?;

        let chunk_resources = &self.resources.instances.chunk_flora_instances[chunk_idx]
            .1
            .resources;
        self.bind_manual_instance_buffers(&self.instances_to_occupancy_ppl, chunk_resources);
        self.bind_manual_instance_buffers(&self.occupancy_to_instances_ppl, chunk_resources);

        let flora_edit_timing_passes: &[SurfacePassTimingPass] = if max_len > 0 {
            &FLORA_EDIT_TIMING_PASSES_WITH_INSTANCES
        } else {
            &FLORA_EDIT_TIMING_PASSES_WITHOUT_INSTANCES
        };

        let device = self.vulkan_ctx.device();
        let cmdbuf = CommandBuffer::new(device, self.vulkan_ctx.command_pool());
        cmdbuf.begin(true);

        let pass_timing = self.pass_timing.as_ref();
        if let Some(timing) = pass_timing {
            timing.record_reset(&cmdbuf, flora_edit_timing_passes.len());
        }
        let mut timing_pass_index = 0usize;
        macro_rules! record_timed_flora_edit_pass {
            ($body:block) => {{
                if let Some(timing) = pass_timing {
                    timing.record_start(&cmdbuf, timing_pass_index);
                }
                let result = { $body };
                if let Some(timing) = pass_timing {
                    timing.record_end(&cmdbuf, timing_pass_index);
                    timing_pass_index += 1;
                }
                result
            }};
        }

        self.resources
            .occupancy_data
            .get_image()
            .record_transition_barrier(&cmdbuf, 0, vk::ImageLayout::GENERAL);

        record_timed_flora_edit_pass!({
            self.clear_occupancy_ppl.record(
                &cmdbuf,
                Extent3D::new(
                    self.voxel_dim_per_chunk.x,
                    self.voxel_dim_per_chunk.y,
                    self.voxel_dim_per_chunk.z,
                ),
                None,
            );
        });

        if max_len > 0 {
            record_compute_barrier(device, &cmdbuf);
            record_timed_flora_edit_pass!({
                self.instances_to_occupancy_ppl
                    .record(&cmdbuf, Extent3D::new(max_len, 1, 1), None);
            });
        }

        record_compute_barrier(device, &cmdbuf);

        record_timed_flora_edit_pass!({
            self.edit_occupancy_ppl.record(
                &cmdbuf,
                Extent3D::new(
                    self.voxel_dim_per_chunk.x,
                    self.voxel_dim_per_chunk.y,
                    self.voxel_dim_per_chunk.z,
                ),
                None,
            );
        });

        record_compute_barrier(device, &cmdbuf);

        record_timed_flora_edit_pass!({
            self.occupancy_to_instances_ppl.record(
                &cmdbuf,
                Extent3D::new(
                    self.voxel_dim_per_chunk.x,
                    self.voxel_dim_per_chunk.y,
                    self.voxel_dim_per_chunk.z,
                ),
                None,
            );
        });

        if pass_timing.is_some() {
            assert_eq!(timing_pass_index, flora_edit_timing_passes.len());
        }

        cmdbuf.end();
        cmdbuf.submit(&self.vulkan_ctx.get_general_queue(), None);
        device.wait_queue_idle(&self.vulkan_ctx.get_general_queue());

        if let Some(timing) = self.pass_timing.as_ref() {
            timing.collect_and_log(
                "FLORA_EDIT_PASS_TIMING",
                "flora_edit_pass_timed_total_gpu",
                chunk_id,
                flora_edit_timing_passes,
            );
        }

        let result = get_occupancy_to_instances_result(
            &self.resources.occupancy_to_instances_result,
            self.flora_species_count,
        );
        let chunk_resources_mut = &mut self.resources.instances.chunk_flora_instances[chunk_idx].1;
        let mut after_total = 0_u32;
        for (species_idx, len) in result.flora_instance_len.iter().enumerate() {
            chunk_resources_mut.get_mut(species_idx).instances_len = *len;
            after_total = after_total.saturating_add(*len);
        }

        let appended_total = if mode == OccupancyEditMode::Add {
            after_total.saturating_sub(before_total)
        } else {
            0
        };

        Ok(FloraRegenStats {
            appended_total,
            before_total,
            after_total,
            dispatch_dim: self.voxel_dim_per_chunk,
            has_growing_flora: result.has_growing_flora,
        })
    }

    pub fn update_flora_growth_for_chunk(
        &mut self,
        chunk_id: UVec3,
        tick_delta: u32,
    ) -> Result<bool> {
        if !self.chunk_bound.in_bound(chunk_id) {
            return Err(anyhow::anyhow!("Chunk ID out of bounds"));
        }

        let chunk_idx = self.get_chunk_resource_index(chunk_id)?;
        let chunk_world_offset = chunk_id * self.voxel_dim_per_chunk;
        let mut species_len = [0_u32; 4];
        let mut max_len = 0_u32;
        for (species_idx, species) in species_len
            .iter_mut()
            .enumerate()
            .take(self.flora_species_count.min(4))
        {
            let len = self.resources.instances.chunk_flora_instances[chunk_idx]
                .1
                .get(species_idx)
                .instances_len;
            *species = len;
            max_len = max_len.max(len);
        }

        if max_len == 0 {
            return Ok(false);
        }

        update_instances_to_occupancy_info(
            &self.resources.instances_to_occupancy_info,
            chunk_world_offset,
            self.voxel_dim_per_chunk,
            species_len,
            tick_delta,
        )?;
        cleanup_occupancy_to_instances_result(&self.resources.occupancy_to_instances_result)?;

        let chunk_resources = &self.resources.instances.chunk_flora_instances[chunk_idx]
            .1
            .resources;
        self.bind_manual_instance_buffers(&self.update_flora_growth_ppl, chunk_resources);

        let device = self.vulkan_ctx.device();
        let cmdbuf = CommandBuffer::new(device, self.vulkan_ctx.command_pool());
        cmdbuf.begin(true);

        let pass_timing = self.pass_timing.as_ref();
        if let Some(timing) = pass_timing {
            timing.record_reset(&cmdbuf, FLORA_GROWTH_TIMING_PASSES.len());
            timing.record_start(&cmdbuf, 0);
        }
        self.update_flora_growth_ppl
            .record(&cmdbuf, Extent3D::new(max_len, 1, 1), None);
        if let Some(timing) = pass_timing {
            timing.record_end(&cmdbuf, 0);
        }

        cmdbuf.end();
        cmdbuf.submit(&self.vulkan_ctx.get_general_queue(), None);
        device.wait_queue_idle(&self.vulkan_ctx.get_general_queue());

        if let Some(timing) = self.pass_timing.as_ref() {
            timing.collect_and_log(
                "FLORA_GROWTH_PASS_TIMING",
                "flora_growth_pass_timed_total_gpu",
                chunk_id,
                &FLORA_GROWTH_TIMING_PASSES,
            );
        }

        Ok(get_occupancy_to_instances_result(
            &self.resources.occupancy_to_instances_result,
            self.flora_species_count,
        )
        .has_growing_flora)
    }

    fn get_chunk_resource_index(&self, chunk_id: UVec3) -> Result<usize> {
        self.resources
            .instances
            .chunk_flora_instances
            .iter()
            .position(|(_, resources)| resources.chunk_id == chunk_id)
            .ok_or_else(|| anyhow::anyhow!("Chunk {:?} has no flora instance resources", chunk_id))
    }

    fn bind_manual_instance_buffers(
        &self,
        pipeline: &ComputePipeline,
        resources: &[InstanceResource],
    ) {
        for (species_index, instance_resource) in resources.iter().enumerate() {
            pipeline.write_descriptor_set(
                1,
                WriteDescriptorSet::new_buffer_write(0, &instance_resource.instances_buf)
                    .with_array_element(species_index as u32),
            );
        }
    }

    pub fn get_resources(&self) -> &SurfaceResources {
        &self.resources
    }
}

fn record_compute_barrier(device: &crate::vkn::Device, cmdbuf: &CommandBuffer) {
    let barrier = PipelineBarrier::new(
        vk::PipelineStageFlags::COMPUTE_SHADER,
        vk::PipelineStageFlags::COMPUTE_SHADER,
        vec![MemoryBarrier::new_shader_access()],
    );
    barrier.record_insert(device, cmdbuf);
}

fn record_clear_buffer_for_compute(
    device: &crate::vkn::Device,
    cmdbuf: &CommandBuffer,
    buffer: &Buffer,
) {
    unsafe {
        device.as_raw().cmd_fill_buffer(
            cmdbuf.as_raw(),
            buffer.as_raw(),
            0,
            buffer.get_size_bytes(),
            0,
        );
    }

    let barrier = PipelineBarrier::new(
        vk::PipelineStageFlags::TRANSFER,
        vk::PipelineStageFlags::COMPUTE_SHADER,
        vec![MemoryBarrier::new(
            vk::AccessFlags::TRANSFER_WRITE,
            vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE,
        )],
    );
    barrier.record_insert(device, cmdbuf);
}

fn update_make_surface_info(
    make_surface_info: &Buffer,
    atlas_read_offset: UVec3,
    atlas_read_dim: UVec3,
    is_crossing_boundary: bool,
) -> Result<()> {
    make_surface_info.fill_uniform(&MakeSurfaceInfo {
        atlas_read_offset: atlas_read_offset.to_array(),
        atlas_read_dim: atlas_read_dim.to_array(),
        is_crossing_boundary: if is_crossing_boundary { 1 } else { 0 },
        ..MakeSurfaceInfo::zeroed()
    })
}

fn get_make_surface_result(make_surface_result: &Buffer) -> MakeSurfaceResultReadback {
    let raw_data = make_surface_result.read_back().unwrap();
    let total_u32 = raw_data.len() / std::mem::size_of::<u32>();
    let data = unsafe { std::slice::from_raw_parts(raw_data.as_ptr() as *const u32, total_u32) };
    assert!(
        total_u32 >= 2,
        "make_surface_result buffer too small: expected at least 2 u32s, got {}",
        total_u32
    );
    MakeSurfaceResultReadback {
        active_voxel_len: data[0],
        active_brick_len: data[1],
    }
}

fn update_clear_occupancy_info(clear_occupancy_info: &Buffer, chunk_dim: UVec3) -> Result<()> {
    clear_occupancy_info.fill_uniform(&ClearOccupancyInfo {
        chunk_dim: chunk_dim.to_array(),
        ..ClearOccupancyInfo::zeroed()
    })
}

fn update_instances_to_occupancy_info(
    instances_to_occupancy_info: &Buffer,
    chunk_world_offset: UVec3,
    chunk_dim: UVec3,
    species_instance_len: [u32; 4],
    tick_delta: u32,
) -> Result<()> {
    instances_to_occupancy_info.fill_uniform(&InstancesToOccupancyInfo {
        chunk_world_offset: chunk_world_offset.to_array(),
        chunk_dim: chunk_dim.to_array(),
        species_instance_len,
        tick_delta,
        ..InstancesToOccupancyInfo::zeroed()
    })
}

#[allow(clippy::too_many_arguments)]
fn update_edit_occupancy_info(
    edit_occupancy_info: &Buffer,
    edit_center_vox: Vec3,
    edit_radius_vox: f32,
    chunk_world_offset: UVec3,
    chunk_dim: UVec3,
    mode: OccupancyEditMode,
    flora_tick: u32,
    target_age: u32,
) -> Result<()> {
    edit_occupancy_info.fill_uniform(&EditOccupancyInfo {
        edit_center_radius_vox: [
            edit_center_vox.x,
            edit_center_vox.y,
            edit_center_vox.z,
            edit_radius_vox,
        ],
        chunk_world_offset: chunk_world_offset.to_array(),
        chunk_dim: chunk_dim.to_array(),
        mode: mode as u32,
        flora_tick,
        target_age,
        ..EditOccupancyInfo::zeroed()
    })
}

fn update_occupancy_to_instances_info(
    occupancy_to_instances_info: &Buffer,
    chunk_world_offset: UVec3,
    chunk_dim: UVec3,
) -> Result<()> {
    occupancy_to_instances_info.fill_uniform(&OccupancyToInstancesInfo {
        chunk_world_offset: chunk_world_offset.to_array(),
        chunk_dim: chunk_dim.to_array(),
        ..OccupancyToInstancesInfo::zeroed()
    })
}

fn cleanup_occupancy_to_instances_result(result: &Buffer) -> Result<()> {
    let layout = result.get_layout().unwrap();
    let buffer_size = layout.root_member.get_size_bytes() as usize;
    let zeroed = vec![0u8; buffer_size];
    result.fill_with_raw_u8(&zeroed)?;
    Ok(())
}

fn get_occupancy_to_instances_result(
    result: &Buffer,
    species_count: usize,
) -> OccupancyToInstancesResultReadback {
    let raw_data = result.read_back().unwrap();
    let total_u32 = raw_data.len() / std::mem::size_of::<u32>();
    let data = unsafe { std::slice::from_raw_parts(raw_data.as_ptr() as *const u32, total_u32) };
    assert!(
        total_u32 > species_count,
        "occupancy_to_instances_result buffer too small: expected more than {} u32s, got {}",
        species_count,
        total_u32
    );
    let mut flora_instance_len = Vec::with_capacity(species_count);
    flora_instance_len.extend_from_slice(&data[0..species_count]);
    OccupancyToInstancesResultReadback {
        flora_instance_len,
        has_growing_flora: data[species_count] != 0,
    }
}
