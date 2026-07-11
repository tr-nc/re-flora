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
};
use anyhow::Result;
use bytemuck::Zeroable;
use glam::{UVec3, Vec3};
pub use resources::*;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use verdarium_vkn::{
    Buffer, ClearValue, ColorClearValue, CommandBuffer, ComputePipeline, DescriptorPool, Extent3D,
    GpuJobProfiler, GpuJobScopeToken, GpuJobToken, PipelineBarrier, PipelineStage, QueueLane,
    ShaderModule, TextureLayout, TimestampQueryPool, VulkanContext, WriteDescriptorSet,
};

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
    solid_workgroup_len: u32,
}

pub struct SurfaceBuildJob {
    chunk_id: UVec3,
    place_flora: bool,
    flora_chunk_idx: Option<usize>,
    total_start: Instant,
    submitted_at: Instant,
    setup_elapsed: Duration,
    record_elapsed: Duration,
    submit_elapsed: Duration,
    _command_buffer: CommandBuffer,
    gpu_job: GpuJobToken,
    gpu_scope: Option<GpuJobScopeToken>,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct SurfaceBuildResult {
    pub chunk_id: UVec3,
    pub active_voxel_len: u32,
    pub active_brick_len: u32,
    pub solid_workgroup_len: u32,
    pub place_flora: bool,
    pub flora_rebuilt: bool,
    pub setup_ms: f64,
    pub record_ms: f64,
    pub gpu_submit_ms: f64,
    pub gpu_completion_latency_ms: f64,
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
    query_pool: TimestampQueryPool,
}

impl SurfacePassTiming {
    fn maybe_new(vulkan_ctx: &VulkanContext) -> Option<Self> {
        if !log::log_enabled!(target: module_path!(), log::Level::Debug) {
            return None;
        }

        let max_pass_count = SURFACE_BUILD_TIMING_PASSES_WITH_FLORA
            .len()
            .max(FLORA_EDIT_TIMING_PASSES_WITH_INSTANCES.len())
            .max(FLORA_GROWTH_TIMING_PASSES.len());
        let max_query_count = (max_pass_count * 2) as u32;
        let query_pool = TimestampQueryPool::maybe_new(
            vulkan_ctx,
            max_query_count,
            "PERF][SURFACE_PASS_TIMING",
        )?;

        log::debug!(
            "[PERF][SURFACE_PASS_TIMING] enabled max_passes={} queries={} timestamp_period_ns={:.3}",
            max_pass_count,
            max_query_count,
            query_pool.timestamp_period_ns(),
        );

        Some(Self { query_pool })
    }

    fn record_reset(&self, cmdbuf: &CommandBuffer, pass_count: usize) {
        self.query_pool
            .record_reset(cmdbuf, self.query_count(pass_count));
    }

    fn record_start(&self, cmdbuf: &CommandBuffer, pass_index: usize) {
        self.record_timestamp(cmdbuf, pass_index * 2);
    }

    fn record_end(&self, cmdbuf: &CommandBuffer, pass_index: usize) {
        self.record_timestamp(cmdbuf, pass_index * 2 + 1);
    }

    fn record_timestamp(&self, cmdbuf: &CommandBuffer, query_index: usize) {
        self.query_pool
            .record_timestamp(cmdbuf, PipelineStage::ALL_COMMANDS, query_index as u32);
    }

    fn collect_and_log(
        &self,
        log_tag: &'static str,
        total_bench_key: &'static str,
        chunk_id: UVec3,
        passes: &[SurfacePassTimingPass],
    ) {
        let query_count = self.query_count(passes.len());
        let readback_start = Instant::now();
        let timestamps = match self.query_pool.read_u64(query_count) {
            Ok(timestamps) => timestamps,
            Err(err) => {
                crate::util::BENCH
                    .lock()
                    .unwrap()
                    .record("surface_pass_timestamp_readback", readback_start.elapsed());
                log::warn!(
                    "[PERF][{}] chunk {:?} query readback failed: {err}",
                    log_tag,
                    chunk_id
                );
                return;
            }
        };
        crate::util::BENCH
            .lock()
            .unwrap()
            .record("surface_pass_timestamp_readback", readback_start.elapsed());

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

            let duration_ms =
                (end - start) as f64 * self.query_pool.timestamp_period_ns() as f64 / 1_000_000.0;
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
            query_count <= self.query_pool.max_query_count(),
            "surface pass timing query count {} exceeds pool capacity {}",
            query_count,
            self.query_pool.max_query_count(),
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
        label: "active_brick_flags_clear",
        bench_key: "surface_pass_active_brick_flags_clear_gpu",
    },
    SurfacePassTimingPass {
        label: "prepare_sparse_surface",
        bench_key: "surface_pass_prepare_sparse_surface_gpu",
    },
    SurfacePassTimingPass {
        label: "make_surface_sparse",
        bench_key: "surface_pass_make_surface_sparse_gpu",
    },
];

const SURFACE_BUILD_TIMING_PASSES_WITH_FLORA: [SurfacePassTimingPass; 6] = [
    SurfacePassTimingPass {
        label: "surface_clear",
        bench_key: "surface_pass_surface_clear_gpu",
    },
    SurfacePassTimingPass {
        label: "active_brick_flags_clear",
        bench_key: "surface_pass_active_brick_flags_clear_gpu",
    },
    SurfacePassTimingPass {
        label: "prepare_sparse_surface",
        bench_key: "surface_pass_prepare_sparse_surface_gpu",
    },
    SurfacePassTimingPass {
        label: "make_surface_sparse",
        bench_key: "surface_pass_make_surface_sparse_gpu",
    },
    SurfacePassTimingPass {
        label: "prepaverdarium_dispatch",
        bench_key: "surface_pass_prepaverdarium_dispatch_gpu",
    },
    SurfacePassTimingPass {
        label: "active_surface_to_flora",
        bench_key: "surface_pass_active_surface_to_flora_gpu",
    },
];

fn surface_build_timing_passes(place_flora: bool) -> &'static [SurfacePassTimingPass] {
    if place_flora {
        &SURFACE_BUILD_TIMING_PASSES_WITH_FLORA
    } else {
        &SURFACE_BUILD_TIMING_PASSES
    }
}

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

#[derive(Clone, Copy, Debug)]
pub struct AuthoredFloraInstance {
    pub species_index: u32,
    pub base_world_vox: UVec3,
    pub growth_progress: u32,
    pub spawn_start_ms: u32,
    #[allow(dead_code)]
    pub seed: u32,
}

#[derive(Default)]
struct AuthoredFloraStore {
    instances_by_chunk: HashMap<UVec3, Vec<AuthoredFloraInstance>>,
}

impl AuthoredFloraStore {
    fn instances_for_chunk(&self, chunk_id: UVec3) -> &[AuthoredFloraInstance] {
        self.instances_by_chunk
            .get(&chunk_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    fn instances_for_chunk_mut(&mut self, chunk_id: UVec3) -> &mut Vec<AuthoredFloraInstance> {
        self.instances_by_chunk.entry(chunk_id).or_default()
    }
}

pub struct SurfaceBuilder {
    vulkan_ctx: VulkanContext,
    pub resources: SurfaceResources,

    #[allow(dead_code)]
    pool: DescriptorPool,

    prepare_sparse_surface_dispatch_ppl: ComputePipeline,
    make_surface_ppl: ComputePipeline,
    clear_occupancy_ppl: ComputePipeline,
    instances_to_occupancy_ppl: ComputePipeline,
    edit_occupancy_ppl: ComputePipeline,
    occupancy_to_instances_ppl: ComputePipeline,
    prepare_active_surface_flora_dispatch_ppl: ComputePipeline,
    active_surface_to_flora_ppl: ComputePipeline,
    update_flora_growth_ppl: ComputePipeline,
    pass_timing: Option<SurfacePassTiming>,
    gpu_job_profiler: Option<GpuJobProfiler>,
    authored_flora: AuthoredFloraStore,

    chunk_bound: UAabb3,
    voxel_dim_per_chunk: UVec3,
    flora_species_count: usize,
}

impl SurfaceBuilder {
    pub fn new(
        vulkan_ctx: VulkanContext,
        allocator: verdarium_vkn::Allocator,
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
            "shader/builder/surface/make_surface_sparse.comp",
            "main",
        )
        .unwrap();

        let prepare_sparse_surface_dispatch_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/surface/prepare_sparse_surface_dispatch.comp",
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
            "shader/builder/surface/edit_occupancy_capsule.comp",
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

        let prepare_active_surface_flora_dispatch_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/surface/prepare_active_surface_flora_dispatch.comp",
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
            &prepare_sparse_surface_dispatch_sm,
            &clear_occupancy_sm,
            &instances_to_occupancy_sm,
            &edit_occupancy_sm,
            &occupancy_to_instances_sm,
            &update_flora_growth_sm,
            &prepare_active_surface_flora_dispatch_sm,
            chunk_bound,
        );

        let pool = DescriptorPool::new(device).unwrap();

        let prepare_sparse_surface_dispatch_ppl = ComputePipeline::new(
            device,
            &prepare_sparse_surface_dispatch_sm,
            &pool,
            &[&resources, plain_builder_resources],
        );
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
        let prepare_active_surface_flora_dispatch_ppl = ComputePipeline::new(
            device,
            &prepare_active_surface_flora_dispatch_sm,
            &pool,
            &[&resources],
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
            prepare_sparse_surface_dispatch_ppl,
            make_surface_ppl,
            clear_occupancy_ppl,
            instances_to_occupancy_ppl,
            edit_occupancy_ppl,
            occupancy_to_instances_ppl,
            prepare_active_surface_flora_dispatch_ppl,
            active_surface_to_flora_ppl,
            update_flora_growth_ppl,
            pass_timing,
            gpu_job_profiler: None,
            authored_flora: AuthoredFloraStore::default(),
            chunk_bound,
            voxel_dim_per_chunk,
            flora_species_count,
        }
    }

    pub fn enable_gpu_job_profiling(&mut self, max_scopes: usize) {
        self.gpu_job_profiler = GpuJobProfiler::maybe_new(
            &self.vulkan_ctx,
            max_scopes,
            "PERF][SURFACE_GPU_JOB_PROFILER",
        );
        if let Some(profiler) = self.gpu_job_profiler.as_ref() {
            log::info!(
                "[PERF][SURFACE_GPU_JOB_PROFILER] enabled max_scopes={} timestamp_period_ns={:.3}",
                profiler.max_scopes(),
                profiler.timestamp_period_ns(),
            );
        }
    }

    pub fn build_surface(&mut self, chunk_id: UVec3, place_flora: bool) -> Result<u32> {
        let job = self.submit_build_surface(chunk_id, place_flora)?;
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
        let flora_chunk_idx = if place_flora {
            let chunk_idx = self.get_chunk_resource_index(chunk_id)?;
            update_occupancy_to_instances_info(
                &self.resources.occupancy_to_instances_info,
                atlas_read_offset,
                self.voxel_dim_per_chunk,
                0,
            )?;
            cleanup_occupancy_to_instances_result(&self.resources.occupancy_to_instances_result)?;
            let chunk_resources = &self.resources.instances.chunk_flora_instances[chunk_idx].1;
            self.bind_manual_instance_buffer(&self.active_surface_to_flora_ppl, chunk_resources);
            Some(chunk_idx)
        } else {
            None
        };
        let setup_elapsed = setup_start.elapsed();

        let record_start = Instant::now();
        let cmdbuf = CommandBuffer::new(device, self.vulkan_ctx.command_pool());
        cmdbuf.begin(true);

        let gpu_scope = self.gpu_job_profiler.as_mut().and_then(|profiler| {
            profiler.begin_scope(
                &cmdbuf,
                "surface.build",
                QueueLane::General,
                PipelineStage::COMPUTE_SHADER,
            )
        });

        let pass_timing = self.pass_timing.as_ref();
        let timing_passes = surface_build_timing_passes(place_flora);
        if let Some(timing) = pass_timing {
            timing.record_reset(&cmdbuf, timing_passes.len());
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
                Some(TextureLayout::GENERAL),
                0,
                ClearValue::Color(ColorClearValue::UInt([0, 0, 0, 0])),
            );
        });
        record_timed_surface_pass!({
            record_clear_buffer_for_compute(
                device,
                &cmdbuf,
                &self.resources.surface_active_brick_flags,
            );
        });

        let solid_workgroup_dim = (self.voxel_dim_per_chunk + UVec3::splat(7)) / 8;
        let solid_workgroup_count =
            solid_workgroup_dim.x * solid_workgroup_dim.y * solid_workgroup_dim.z;
        record_timed_surface_pass!({
            // Keep this clear on the GPU timeline. Sync rebuilds can submit a contree
            // build that still reads the previous chunk's make_surface_result while
            // the CPU starts recording the next surface build. A host-side clear here
            // can zero active_brick_len before that queued contree pass consumes it.
            record_clear_buffer_for_compute(device, &cmdbuf, &self.resources.make_surface_result);
            record_clear_buffer_for_compute(
                device,
                &cmdbuf,
                &self.resources.surface_solid_workgroup_dispatch_indirect,
            );
            self.prepare_sparse_surface_dispatch_ppl.record(
                &cmdbuf,
                Extent3D::new(solid_workgroup_count, 1, 1),
                None,
            );
        });

        record_compute_to_indirect_and_shader_barrier(device, &cmdbuf);
        record_timed_surface_pass!({
            self.make_surface_ppl.record_indirect(
                &cmdbuf,
                &self.resources.surface_solid_workgroup_dispatch_indirect,
                None,
            );
        });

        if place_flora {
            record_compute_barrier(device, &cmdbuf);
            record_timed_surface_pass!({
                self.prepare_active_surface_flora_dispatch_ppl.record(
                    &cmdbuf,
                    Extent3D::new(1, 1, 1),
                    None,
                );
            });
            record_compute_to_indirect_and_shader_barrier(device, &cmdbuf);
            record_timed_surface_pass!({
                self.active_surface_to_flora_ppl.record_indirect(
                    &cmdbuf,
                    &self.resources.active_surface_flora_dispatch_indirect,
                    None,
                );
            });
        }

        if pass_timing.is_some() {
            assert_eq!(timing_pass_index, timing_passes.len());
        }

        if let Some(scope) = gpu_scope {
            if let Some(profiler) = self.gpu_job_profiler.as_mut() {
                profiler.end_scope(&cmdbuf, scope, PipelineStage::COMPUTE_SHADER);
            }
        }

        cmdbuf.end();
        let record_elapsed = record_start.elapsed();

        let submit_start = Instant::now();
        let gpu_job =
            cmdbuf.submit_gpu_job(&self.vulkan_ctx.get_general_queue(), "surface.build")?;
        let submitted_at = Instant::now();
        let submit_elapsed = submit_start.elapsed();

        Ok(SurfaceBuildJob {
            chunk_id,
            place_flora,
            flora_chunk_idx,
            total_start,
            submitted_at,
            setup_elapsed,
            record_elapsed,
            submit_elapsed,
            _command_buffer: cmdbuf,
            gpu_job,
            gpu_scope,
        })
    }

    pub fn build_surface_ready(&self, job: &SurfaceBuildJob) -> Result<bool> {
        job.gpu_job
            .is_complete()
            .map_err(|err| anyhow::anyhow!("failed to poll surface build GPU job: {err}"))
    }

    pub fn wait_build_surface(&self, job: &SurfaceBuildJob) -> Result<()> {
        job.gpu_job.wait()?;
        Ok(())
    }

    pub fn finish_build_surface(&mut self, job: SurfaceBuildJob) -> Result<SurfaceBuildResult> {
        let gpu_completion_latency_elapsed = job.submitted_at.elapsed();
        let _completed_gpu_job = job.gpu_job.wait_complete()?;
        let readback_start = Instant::now();
        let make_surface_result = get_make_surface_result(&self.resources.make_surface_result);
        let active_voxel_len = make_surface_result.active_voxel_len;
        let active_brick_len = make_surface_result.active_brick_len;
        let solid_workgroup_len = make_surface_result.solid_workgroup_len;
        let readback_elapsed = readback_start.elapsed();

        if let Some(scope) = job.gpu_scope {
            if let Some(profiler) = self.gpu_job_profiler.as_mut() {
                match profiler.collect_completed_scope(scope) {
                    Ok(Some(result)) => {
                        log::info!(
                            "[PERF][GPU_JOB_SCOPE] name={} queue={:?} chunk {:?} duration={:.0}us",
                            result.name,
                            result.queue,
                            job.chunk_id,
                            result.duration_us(),
                        );
                    }
                    Ok(None) => {}
                    Err(err) => {
                        log::warn!(
                            "[PERF][GPU_JOB_SCOPE] surface.build chunk {:?} readback failed: {err}",
                            job.chunk_id,
                        );
                    }
                }
            }
        }

        if let Some(timing) = self.pass_timing.as_ref() {
            timing.collect_and_log(
                "SURFACE_BUILD_PASS_TIMING",
                "surface_pass_timed_total_gpu",
                job.chunk_id,
                surface_build_timing_passes(job.place_flora),
            );
        }

        let should_rebuild_flora = job.flora_chunk_idx.is_some() && active_voxel_len > 0;
        let flora_start = Instant::now();
        if let Some(chunk_idx) = job.flora_chunk_idx.filter(|_| active_voxel_len > 0) {
            let result = get_occupancy_to_instances_result(
                &self.resources.occupancy_to_instances_result,
                self.flora_species_count,
            );
            let chunk_resources_mut =
                &mut self.resources.instances.chunk_flora_instances[chunk_idx].1;
            for (species_idx, len) in result.flora_instance_len.iter().enumerate() {
                chunk_resources_mut.set_species_len(species_idx, *len);
            }
            self.sync_authored_flora_chunk_to_gpu(job.chunk_id)?;
        }
        let flora_elapsed = flora_start.elapsed();
        let total_elapsed = job.total_start.elapsed();

        log::debug!(
            "[PERF][SURFACE_BUILD] chunk {:?} total {:.2}ms setup {:.2}ms record {:.2}ms gpu_submit {:.2}ms gpu_completion_latency {:.2}ms readback {:.2}ms flora {:.2}ms active_voxels {} active_bricks {} solid_workgroups {} place_flora {} flora_rebuilt {}",
            job.chunk_id,
            total_elapsed.as_secs_f32() * 1000.0,
            job.setup_elapsed.as_secs_f32() * 1000.0,
            job.record_elapsed.as_secs_f32() * 1000.0,
            job.submit_elapsed.as_secs_f32() * 1000.0,
            gpu_completion_latency_elapsed.as_secs_f32() * 1000.0,
            readback_elapsed.as_secs_f32() * 1000.0,
            flora_elapsed.as_secs_f32() * 1000.0,
            active_voxel_len,
            active_brick_len,
            solid_workgroup_len,
            job.place_flora,
            should_rebuild_flora,
        );

        Ok(SurfaceBuildResult {
            chunk_id: job.chunk_id,
            active_voxel_len,
            active_brick_len,
            solid_workgroup_len,
            place_flora: job.place_flora,
            flora_rebuilt: should_rebuild_flora,
            setup_ms: job.setup_elapsed.as_secs_f64() * 1000.0,
            record_ms: job.record_elapsed.as_secs_f64() * 1000.0,
            gpu_submit_ms: job.submit_elapsed.as_secs_f64() * 1000.0,
            gpu_completion_latency_ms: gpu_completion_latency_elapsed.as_secs_f64() * 1000.0,
            readback_ms: readback_elapsed.as_secs_f64() * 1000.0,
            flora_ms: flora_elapsed.as_secs_f64() * 1000.0,
            total_ms: total_elapsed.as_secs_f64() * 1000.0,
        })
    }

    pub fn authored_flora_base_positions_for_species(&self, species_index: u32) -> Vec<UVec3> {
        self.authored_flora
            .instances_by_chunk
            .values()
            .flat_map(|instances| instances.iter())
            .filter(|instance| instance.species_index == species_index)
            .map(|instance| instance.base_world_vox)
            .collect()
    }

    pub fn try_add_authored_flora_instance_deferred(
        &mut self,
        species_index: u32,
        base_world_vox: UVec3,
        growth_progress: u32,
        spawn_start_ms: u32,
        seed: u32,
        dirty_chunks: &mut Vec<UVec3>,
    ) -> bool {
        if !species::is_authored_plant_species_index(species_index) {
            return false;
        }
        let chunk_id = self.chunk_id_for_world_voxel(base_world_vox);
        if !self.chunk_bound.in_bound(chunk_id) {
            return false;
        }
        let chunk_world_offset = chunk_id * self.voxel_dim_per_chunk;
        let local_pos = base_world_vox - chunk_world_offset;
        if local_pos.cmpge(self.voxel_dim_per_chunk).any() {
            return false;
        }

        let chunk_instances = self.authored_flora.instances_for_chunk_mut(chunk_id);
        if chunk_instances.iter().any(|instance| {
            instance.species_index == species_index && instance.base_world_vox == base_world_vox
        }) {
            return false;
        }

        chunk_instances.push(AuthoredFloraInstance {
            species_index,
            base_world_vox,
            growth_progress: growth_progress.min(0xff),
            spawn_start_ms,
            seed,
        });
        if !dirty_chunks.contains(&chunk_id) {
            dirty_chunks.push(chunk_id);
        }
        true
    }

    pub fn sync_authored_flora_dirty_chunks(&mut self, dirty_chunks: &[UVec3]) -> Result<()> {
        for &chunk_id in dirty_chunks {
            self.sync_authored_flora_chunk_to_gpu(chunk_id)?;
        }
        Ok(())
    }

    pub fn remove_authored_flora_for_brush(
        &mut self,
        chunk_id: UVec3,
        edit_start: Vec3,
        edit_end: Vec3,
        edit_radius: f32,
    ) -> Result<u32> {
        if !self.chunk_bound.in_bound(chunk_id) || edit_radius <= 0.0 {
            return Ok(0);
        }

        let Some(chunk_instances) = self.authored_flora.instances_by_chunk.get_mut(&chunk_id)
        else {
            return Ok(0);
        };

        let edit_start_vox = edit_start * 256.0;
        let edit_end_vox = edit_end * 256.0;
        let radius_sq = (edit_radius * 256.0).powi(2);
        let before = chunk_instances.len();
        chunk_instances.retain(|instance| {
            let base_center_vox = Vec3::new(
                instance.base_world_vox.x as f32 + 0.5,
                instance.base_world_vox.y as f32 + 0.5,
                instance.base_world_vox.z as f32 + 0.5,
            );
            distance_sq_to_segment(base_center_vox, edit_start_vox, edit_end_vox) > radius_sq
        });
        let removed = (before - chunk_instances.len()) as u32;
        if removed > 0 {
            self.sync_authored_flora_chunk_to_gpu(chunk_id)?;
        }
        Ok(removed)
    }

    fn chunk_id_for_world_voxel(&self, world_vox: UVec3) -> UVec3 {
        world_vox / self.voxel_dim_per_chunk
    }

    fn sync_authored_flora_chunk_to_gpu(&mut self, chunk_id: UVec3) -> Result<()> {
        let chunk_idx = self.get_chunk_resource_index(chunk_id)?;
        let chunk_world_offset = chunk_id * self.voxel_dim_per_chunk;
        let authored_instances = self.authored_flora.instances_for_chunk(chunk_id);
        let chunk_resources = &mut self.resources.instances.chunk_flora_instances[chunk_idx].1;

        for species_index in species::AUTHORED_PLANT_SPECIES_INDICES {
            let mut gpu_instances = Vec::new();
            for instance in authored_instances
                .iter()
                .filter(|instance| instance.species_index == species_index)
            {
                let local_pos = instance.base_world_vox - chunk_world_offset;
                if local_pos.cmpge(self.voxel_dim_per_chunk).any() {
                    continue;
                }
                gpu_instances.push(pack_manual_flora_instance(
                    local_pos,
                    instance.growth_progress,
                    instance.spawn_start_ms,
                ));
            }
            chunk_resources.write_species_instances(species_index as usize, &gpu_instances)?;
        }

        Ok(())
    }

    pub fn edit_flora_instances_for_brush(
        &mut self,
        chunk_id: UVec3,
        edit_start: Vec3,
        edit_end: Vec3,
        edit_radius: f32,
        flora_tick: u32,
        spawn_time_ms: u32,
    ) -> Result<()> {
        self.remove_authored_flora_for_brush(chunk_id, edit_start, edit_end, edit_radius)?;
        let _ = self.run_occupancy_edit(
            chunk_id,
            edit_start,
            edit_end,
            edit_radius,
            flora_tick,
            spawn_time_ms,
            0,
            species::FloraPaintSelection::GrassMix,
            0,
            species::GRASS_MIX_PAINT_BRUSH_SETTINGS,
            OccupancyEditMode::Remove,
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn regenerate_flora_instances_for_brush(
        &mut self,
        chunk_id: UVec3,
        edit_start: Vec3,
        edit_end: Vec3,
        edit_radius: f32,
        flora_tick: u32,
        spawn_time_ms: u32,
        paint_selection: species::FloraPaintSelection,
        paint_dab_serial: u32,
        paint_brush: species::FloraPaintBrushSettings,
    ) -> Result<FloraRegenStats> {
        self.run_occupancy_edit(
            chunk_id,
            edit_start,
            edit_end,
            edit_radius,
            flora_tick,
            spawn_time_ms,
            0,
            paint_selection,
            paint_dab_serial,
            paint_brush,
            OccupancyEditMode::Add,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn trim_flora_instances_for_brush(
        &mut self,
        chunk_id: UVec3,
        edit_start: Vec3,
        edit_end: Vec3,
        edit_radius: f32,
        flora_tick: u32,
        spawn_time_ms: u32,
        target_age: u32,
    ) -> Result<FloraRegenStats> {
        self.run_occupancy_edit(
            chunk_id,
            edit_start,
            edit_end,
            edit_radius,
            flora_tick,
            spawn_time_ms,
            target_age,
            species::FloraPaintSelection::GrassMix,
            0,
            species::GRASS_MIX_PAINT_BRUSH_SETTINGS,
            OccupancyEditMode::Trim,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn run_occupancy_edit(
        &mut self,
        chunk_id: UVec3,
        edit_start: Vec3,
        edit_end: Vec3,
        edit_radius: f32,
        flora_tick: u32,
        spawn_time_ms: u32,
        target_age: u32,
        paint_selection: species::FloraPaintSelection,
        paint_dab_serial: u32,
        paint_brush: species::FloraPaintBrushSettings,
        mode: OccupancyEditMode,
    ) -> Result<FloraRegenStats> {
        if !self.chunk_bound.in_bound(chunk_id) {
            return Err(anyhow::anyhow!("Chunk ID out of bounds"));
        }

        let chunk_idx = self.get_chunk_resource_index(chunk_id)?;
        let chunk_world_offset = chunk_id * self.voxel_dim_per_chunk;
        let edit_start_vox = edit_start * 256.0;
        let edit_end_vox = edit_end * 256.0;
        let edit_radius_vox = edit_radius * 256.0;

        let before_total = self.resources.instances.chunk_flora_instances[chunk_idx]
            .1
            .total_instance_len();

        let mut species_len = [0_u32; 4];
        let mut max_len = 0_u32;
        for (species_idx, species) in species_len
            .iter_mut()
            .enumerate()
            .take(self.flora_species_count.min(4))
        {
            let len = self.resources.instances.chunk_flora_instances[chunk_idx]
                .1
                .species_len(species_idx);
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
            spawn_time_ms,
        )?;
        update_edit_occupancy_info(
            &self.resources.edit_occupancy_info,
            edit_start_vox,
            edit_end_vox,
            edit_radius_vox,
            chunk_world_offset,
            self.voxel_dim_per_chunk,
            mode,
            flora_tick,
            target_age,
            paint_selection,
            paint_dab_serial,
            paint_brush,
        )?;
        update_occupancy_to_instances_info(
            &self.resources.occupancy_to_instances_info,
            chunk_world_offset,
            self.voxel_dim_per_chunk,
            spawn_time_ms,
        )?;
        cleanup_occupancy_to_instances_result(&self.resources.occupancy_to_instances_result)?;

        let chunk_resources = &self.resources.instances.chunk_flora_instances[chunk_idx].1;
        self.bind_manual_instance_buffer(&self.instances_to_occupancy_ppl, chunk_resources);
        self.bind_manual_instance_buffer(&self.occupancy_to_instances_ppl, chunk_resources);

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
        let _completed_gpu_job = cmdbuf
            .submit_gpu_job(&self.vulkan_ctx.get_general_queue(), "surface.flora_edit")?
            .wait_complete()?;

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
            chunk_resources_mut.set_species_len(species_idx, *len);
            after_total = after_total.saturating_add(*len);
        }
        self.sync_authored_flora_chunk_to_gpu(chunk_id)?;

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
                .species_len(species_idx);
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
            0,
        )?;
        cleanup_occupancy_to_instances_result(&self.resources.occupancy_to_instances_result)?;

        let chunk_resources = &self.resources.instances.chunk_flora_instances[chunk_idx].1;
        self.bind_manual_instance_buffer(&self.update_flora_growth_ppl, chunk_resources);

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
        let _completed_gpu_job = cmdbuf
            .submit_gpu_job(&self.vulkan_ctx.get_general_queue(), "surface.flora_growth")?
            .wait_complete()?;

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

    fn bind_manual_instance_buffer(
        &self,
        pipeline: &ComputePipeline,
        resources: &FloraInstanceResources,
    ) {
        pipeline.write_descriptor_set(
            1,
            WriteDescriptorSet::new_buffer_write(0, &resources.resource.instances_buf),
        );
    }

    pub fn get_resources(&self) -> &SurfaceResources {
        &self.resources
    }
}

fn pack_manual_flora_instance(
    local_pos: UVec3,
    growth_progress: u32,
    spawn_start_ms: u32,
) -> resources::Instance {
    resources::Instance {
        packed_local_pos: (local_pos.x & 0xff)
            | ((local_pos.y & 0xff) << 8)
            | ((local_pos.z & 0xff) << 16)
            | ((growth_progress & 0xff) << 24),
        spawn_start_ms,
    }
}

fn distance_sq_to_segment(point: Vec3, start: Vec3, end: Vec3) -> f32 {
    let segment = end - start;
    let segment_len_sq = segment.length_squared().max(1.0e-4);
    let t = ((point - start).dot(segment) / segment_len_sq).clamp(0.0, 1.0);
    let closest = start + segment * t;
    point.distance_squared(closest)
}

fn record_compute_barrier(device: &verdarium_vkn::Device, cmdbuf: &CommandBuffer) {
    PipelineBarrier::compute_shader_access().record_insert(device, cmdbuf);
}

fn record_compute_to_indirect_and_shader_barrier(
    device: &verdarium_vkn::Device,
    cmdbuf: &CommandBuffer,
) {
    PipelineBarrier::compute_to_indirect_and_shader_access().record_insert(device, cmdbuf);
}

fn record_clear_buffer_for_compute(
    device: &verdarium_vkn::Device,
    cmdbuf: &CommandBuffer,
    buffer: &Buffer,
) {
    buffer.record_fill(cmdbuf, 0, buffer.get_size_bytes(), 0);

    PipelineBarrier::transfer_to_compute_shader_access().record_insert(device, cmdbuf);
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
        total_u32 >= 3,
        "make_surface_result buffer too small: expected at least 3 u32s, got {}",
        total_u32
    );
    MakeSurfaceResultReadback {
        active_voxel_len: data[0],
        active_brick_len: data[1],
        solid_workgroup_len: data[2],
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
    spawn_time_ms: u32,
) -> Result<()> {
    instances_to_occupancy_info.fill_uniform(&InstancesToOccupancyInfo {
        chunk_world_offset: chunk_world_offset.to_array(),
        chunk_dim: chunk_dim.to_array(),
        species_instance_len,
        tick_delta,
        spawn_time_ms,
        ..InstancesToOccupancyInfo::zeroed()
    })
}

#[allow(clippy::too_many_arguments)]
fn update_edit_occupancy_info(
    edit_occupancy_info: &Buffer,
    edit_start_vox: Vec3,
    edit_end_vox: Vec3,
    edit_radius_vox: f32,
    chunk_world_offset: UVec3,
    chunk_dim: UVec3,
    mode: OccupancyEditMode,
    flora_tick: u32,
    target_age: u32,
    paint_selection: species::FloraPaintSelection,
    paint_dab_serial: u32,
    paint_brush: species::FloraPaintBrushSettings,
) -> Result<()> {
    edit_occupancy_info.fill_uniform(&EditOccupancyInfo {
        edit_segment_start_radius_vox: [
            edit_start_vox.x,
            edit_start_vox.y,
            edit_start_vox.z,
            edit_radius_vox,
        ],
        edit_segment_end_radius_vox: [
            edit_end_vox.x,
            edit_end_vox.y,
            edit_end_vox.z,
            edit_radius_vox,
        ],
        chunk_world_offset: chunk_world_offset.to_array(),
        chunk_dim: chunk_dim.to_array(),
        mode: mode as u32,
        flora_tick,
        target_age,
        paint_selection: paint_selection.shader_selection(),
        paint_config: [
            paint_dab_serial,
            paint_brush.soft_spacing_voxels,
            0,
            paint_brush.plants_per_release,
        ],
        ..EditOccupancyInfo::zeroed()
    })
}

fn update_occupancy_to_instances_info(
    occupancy_to_instances_info: &Buffer,
    chunk_world_offset: UVec3,
    chunk_dim: UVec3,
    spawn_time_ms: u32,
) -> Result<()> {
    occupancy_to_instances_info.fill_uniform(&OccupancyToInstancesInfo {
        chunk_world_offset: chunk_world_offset.to_array(),
        chunk_dim: chunk_dim.to_array(),
        spawn_time_ms,
        ..OccupancyToInstancesInfo::zeroed()
    })
}

fn cleanup_occupancy_to_instances_result(result: &Buffer) -> Result<()> {
    cleanup_layout_buffer(result)
}

fn cleanup_layout_buffer(buffer: &Buffer) -> Result<()> {
    let layout = buffer.get_layout().unwrap();
    let buffer_size = layout.root_member.get_size_bytes() as usize;
    let zeroed = vec![0u8; buffer_size];
    buffer.fill_with_raw_u8(&zeroed)?;
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
