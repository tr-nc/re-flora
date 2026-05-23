mod resources;
pub use resources::*;

use super::SurfaceResources;
use crate::generated::gpu_structs::ContreeBuildInfo;
use crate::util::AllocationStrategy;
use crate::util::FirstFitAllocator;
use crate::util::LatestChunkQueue;
use crate::util::ShaderCompiler;
use anyhow::Result;
use glam::{UVec3, Vec2, Vec3};
use petalsonic::{AcousticRay, BatchedAnyHitRayTracer};
use re_flora_vkn::vk;
use re_flora_vkn::Allocator;
use re_flora_vkn::Buffer;
use re_flora_vkn::BufferUsage;
use re_flora_vkn::CommandBuffer;
use re_flora_vkn::ComputePipeline;
use re_flora_vkn::DescriptorPool;
use re_flora_vkn::Extent3D;
use re_flora_vkn::Fence;
use re_flora_vkn::MemoryBarrier;
use re_flora_vkn::MemoryLocation;
use re_flora_vkn::PipelineBarrier;
use re_flora_vkn::ShaderModule;
use re_flora_vkn::TimestampQueryPool;
use re_flora_vkn::VulkanContext;
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    mpsc, Arc, RwLock,
};
use std::thread;
use std::time::{Duration, Instant};

const SIZE_OF_NODE_ELEMENT: u64 = 3 * std::mem::size_of::<u32>() as u64;
const SIZE_OF_LEAF_ELEMENT: u64 = std::mem::size_of::<u32>() as u64;
const MAX_NODE_BUFFER_SIZE_IN_BYTES: u64 = 10 * 1024 * 1024;
const MAX_LEAF_BUFFER_SIZE_IN_BYTES: u64 = 10 * 1024 * 1024;
const MAX_DDA_ITERATION: usize = 256;
const DDA_EPSILON: f32 = 1e-4;
const AUDIO_RAY_START_EPSILON: f32 = 0.05;
const AUDIO_RAY_ENDPOINT_EPSILON: f32 = 0.05;

pub struct ContreeBuilder {
    vulkan_ctx: VulkanContext,
    resources: ContreeBuilderResources,

    #[allow(dead_code)]
    contree_buffer_setup_ppl: ComputePipeline,
    #[allow(dead_code)]
    contree_leaf_write_ppl: ComputePipeline,
    #[allow(dead_code)]
    contree_tree_write_ppl: ComputePipeline,
    #[allow(dead_code)]
    contree_buffer_update_ppl: ComputePipeline,
    #[allow(dead_code)]
    contree_last_buffer_update_ppl: ComputePipeline,
    #[allow(dead_code)]
    contree_concat_ppl: ComputePipeline,

    #[allow(dead_code)]
    fixed_pool: DescriptorPool,

    /// Atlas offset <-> (node_alloc_id, leaf_alloc_id)
    chunk_offset_allocation_table: HashMap<UVec3, (u64, u64)>,

    pass_timing: Option<ContreePassTiming>,
    contree_cmdbuf: CommandBuffer,

    leaf_allocator: FirstFitAllocator,
    node_allocator: FirstFitAllocator,

    chunk_dim: UVec3,
    voxel_dim_per_chunk: UVec3,
    cpu_scene_chunks: Vec<Option<UVec3>>,
    cpu_chunk_caches: HashMap<UVec3, Arc<CpuChunkCache>>,
    cpu_chunk_cache_queue: LatestChunkQueue<CpuChunkCacheBuildSource>,
    cpu_chunk_source_revisions: HashMap<UVec3, u64>,
    cpu_chunk_source_updates: Vec<ContreeCpuChunkSourceUpdate>,
    cpu_chunk_readback_buffers: Option<CpuChunkReadbackBuffers>,
    active_cpu_chunk_cache_job: Option<CpuChunkCacheFenceJob>,
    cpu_chunk_cache_decode_inflight: bool,
    cpu_chunk_cache_job_tx: mpsc::Sender<CpuChunkCacheWorkerJob>,
    cpu_chunk_cache_result_rx: mpsc::Receiver<CpuChunkCacheWorkerResult>,
    shared_ray_query_state: Arc<RwLock<ContreeRayQueryState>>,
    audio_ray_tracer: Arc<ContreeAnyHitRayTracer>,
}

pub struct ContreeAnyHitRayTracer {
    enabled: Arc<AtomicBool>,
    shared_state: Arc<RwLock<ContreeRayQueryState>>,
    runtime_stats: Arc<ContreeRayTracingRuntimeStats>,
}

#[derive(Default)]
struct ContreeRayTracingRuntimeStats {
    update_count: AtomicUsize,
    updated_sources: AtomicUsize,
    occluded_sources: AtomicUsize,
    update_failures: AtomicUsize,
    total_update_time_us: AtomicU64,
}

struct ContreeRayQueryState {
    chunk_dim: UVec3,
    cpu_scene_chunks: Vec<Option<UVec3>>,
    cpu_chunk_caches: HashMap<UVec3, Arc<CpuChunkCache>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContreeCpuChunkSourceUpdate {
    pub chunk_idx: UVec3,
    pub revision: u64,
    pub is_present: bool,
}

#[allow(dead_code)]
pub struct ContreeCpuRayQuerySnapshot {
    chunk_dim: UVec3,
    cpu_scene_chunks: Vec<Option<UVec3>>,
    cpu_chunk_caches: HashMap<UVec3, Arc<CpuChunkCache>>,
}

#[allow(dead_code)]
impl ContreeCpuRayQuerySnapshot {
    pub fn query_terrain_ray_cpu(&self, origin: Vec3, direction: Vec3) -> Option<Vec3> {
        query_terrain_ray_against_state(
            self.chunk_dim,
            &self.cpu_scene_chunks,
            &self.cpu_chunk_caches,
            origin,
            direction,
        )
    }

    pub fn query_terrain_occupancy_cpu(&self, point: Vec3) -> bool {
        query_terrain_occupancy_against_state(
            self.chunk_dim,
            &self.cpu_scene_chunks,
            &self.cpu_chunk_caches,
            point,
        )
    }
}

#[derive(Clone, Copy, Debug)]
struct CpuContreeNode {
    packed_0: u32,
    child_mask_lo: u32,
    child_mask_hi: u32,
}

#[derive(Clone, Debug)]
struct CpuChunkCache {
    chunk_idx: UVec3,
    nodes: Vec<CpuContreeNode>,
    leaves: Vec<u32>,
}

#[derive(Clone, Copy, Debug)]
struct CpuChunkCacheBuildSource {
    node_alloc_offset: u64,
    leaf_alloc_offset: u64,
    node_size_in_bytes: u64,
    leaf_size_in_bytes: u64,
}

pub struct ContreeBuildJob {
    atlas_offset: UVec3,
    chunk_idx: UVec3,
    node_alloc_id: u64,
    leaf_alloc_id: u64,
    node_alloc_offset: u64,
    leaf_alloc_offset: u64,
    total_start: Instant,
    submitted_at: Instant,
    prealloc_elapsed: Duration,
    submit_elapsed: Duration,
    _command_buffer: CommandBuffer,
    fence: Fence,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub struct ContreeBuildResult {
    pub chunk_idx: UVec3,
    pub scene_offsets: Option<(u64, u64)>,
    pub source_revision: u64,
    pub prealloc_ms: f64,
    pub gpu_submit_ms: f64,
    pub fence_latency_ms: f64,
    pub size_ms: f64,
    pub confirm_ms: f64,
    pub total_ms: f64,
    pub node_bytes: u64,
    pub leaf_bytes: u64,
}

#[derive(Clone, Copy)]
struct ContreePassTimingPass {
    label: &'static str,
    bench_key: &'static str,
}

struct ContreePassTiming {
    query_pool: TimestampQueryPool,
    passes: Vec<ContreePassTimingPass>,
}

impl ContreePassTiming {
    fn maybe_new(vulkan_ctx: &VulkanContext, total_levels: u32) -> Option<Self> {
        if !log::log_enabled!(target: module_path!(), log::Level::Debug) {
            return None;
        }

        let tree_pass_count = total_levels.saturating_sub(2) as usize;
        if tree_pass_count > CONTREE_TREE_WRITE_TIMING_PASSES.len() {
            log::debug!(
                "[PERF][CONTREE_PASS_TIMING] disabled: total_levels={} exceeds timing label capacity {}",
                total_levels,
                CONTREE_TREE_WRITE_TIMING_PASSES.len(),
            );
            return None;
        }

        let passes = contree_pass_timing_passes(total_levels);
        let query_count = (passes.len() * 2) as u32;
        let query_pool =
            TimestampQueryPool::maybe_new(vulkan_ctx, query_count, "PERF][CONTREE_PASS_TIMING")?;

        log::debug!(
            "[PERF][CONTREE_PASS_TIMING] enabled passes={} queries={} timestamp_period_ns={:.3}",
            passes.len(),
            query_count,
            query_pool.timestamp_period_ns(),
        );

        Some(Self { query_pool, passes })
    }

    fn query_count(&self) -> u32 {
        (self.passes.len() * 2) as u32
    }

    fn record_reset(&self, cmdbuf: &CommandBuffer) {
        self.query_pool.record_reset(cmdbuf, self.query_count());
    }

    fn record_start(&self, cmdbuf: &CommandBuffer, pass_index: usize) {
        self.record_timestamp(cmdbuf, pass_index * 2);
    }

    fn record_end(&self, cmdbuf: &CommandBuffer, pass_index: usize) {
        self.record_timestamp(cmdbuf, pass_index * 2 + 1);
    }

    fn record_timestamp(&self, cmdbuf: &CommandBuffer, query_index: usize) {
        self.query_pool.record_timestamp(
            cmdbuf,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            query_index as u32,
        );
    }

    fn collect_and_log(&self, chunk_idx: UVec3) {
        let readback_start = Instant::now();
        let timestamps = match self.query_pool.read_u64(self.query_count()) {
            Ok(timestamps) => timestamps,
            Err(err) => {
                crate::util::BENCH
                    .lock()
                    .unwrap()
                    .record("contree_pass_timestamp_readback", readback_start.elapsed());
                log::warn!(
                    "[PERF][CONTREE_PASS_TIMING] chunk {:?} query readback failed: {err}",
                    chunk_idx
                );
                return;
            }
        };
        crate::util::BENCH
            .lock()
            .unwrap()
            .record("contree_pass_timestamp_readback", readback_start.elapsed());

        let mut parts = Vec::with_capacity(self.passes.len());
        let mut total_ms = 0.0;
        let mut bench = crate::util::BENCH.lock().unwrap();
        for (pass_index, pass) in self.passes.iter().enumerate() {
            let start = timestamps[pass_index * 2];
            let end = timestamps[pass_index * 2 + 1];
            if end < start {
                log::debug!(
                    "[PERF][CONTREE_PASS_TIMING] chunk {:?} pass {} timestamp wrapped or reordered start={} end={}",
                    chunk_idx,
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
        bench.record(
            "contree_pass_timed_total_gpu",
            Duration::from_secs_f64(total_ms / 1000.0),
        );
        drop(bench);

        log::debug!(
            "[PERF][CONTREE_PASS_TIMING] chunk {:?} pass_total={:.3}ms {}",
            chunk_idx,
            total_ms,
            parts.join(" "),
        );
    }
}

const CONTREE_TREE_WRITE_TIMING_PASSES: [ContreePassTimingPass; 8] = [
    ContreePassTimingPass {
        label: "tree_write_0",
        bench_key: "contree_pass_tree_write_0_gpu",
    },
    ContreePassTimingPass {
        label: "tree_write_1",
        bench_key: "contree_pass_tree_write_1_gpu",
    },
    ContreePassTimingPass {
        label: "tree_write_2",
        bench_key: "contree_pass_tree_write_2_gpu",
    },
    ContreePassTimingPass {
        label: "tree_write_3",
        bench_key: "contree_pass_tree_write_3_gpu",
    },
    ContreePassTimingPass {
        label: "tree_write_4",
        bench_key: "contree_pass_tree_write_4_gpu",
    },
    ContreePassTimingPass {
        label: "tree_write_5",
        bench_key: "contree_pass_tree_write_5_gpu",
    },
    ContreePassTimingPass {
        label: "tree_write_6",
        bench_key: "contree_pass_tree_write_6_gpu",
    },
    ContreePassTimingPass {
        label: "tree_write_7",
        bench_key: "contree_pass_tree_write_7_gpu",
    },
];

const CONTREE_BUFFER_UPDATE_TIMING_PASSES: [ContreePassTimingPass; 8] = [
    ContreePassTimingPass {
        label: "buffer_update_0",
        bench_key: "contree_pass_buffer_update_0_gpu",
    },
    ContreePassTimingPass {
        label: "buffer_update_1",
        bench_key: "contree_pass_buffer_update_1_gpu",
    },
    ContreePassTimingPass {
        label: "buffer_update_2",
        bench_key: "contree_pass_buffer_update_2_gpu",
    },
    ContreePassTimingPass {
        label: "buffer_update_3",
        bench_key: "contree_pass_buffer_update_3_gpu",
    },
    ContreePassTimingPass {
        label: "buffer_update_4",
        bench_key: "contree_pass_buffer_update_4_gpu",
    },
    ContreePassTimingPass {
        label: "buffer_update_5",
        bench_key: "contree_pass_buffer_update_5_gpu",
    },
    ContreePassTimingPass {
        label: "buffer_update_6",
        bench_key: "contree_pass_buffer_update_6_gpu",
    },
    ContreePassTimingPass {
        label: "buffer_update_7",
        bench_key: "contree_pass_buffer_update_7_gpu",
    },
];

fn contree_pass_timing_passes(total_levels: u32) -> Vec<ContreePassTimingPass> {
    let mut passes = vec![
        ContreePassTimingPass {
            label: "buffer_setup",
            bench_key: "contree_pass_buffer_setup_gpu",
        },
        ContreePassTimingPass {
            label: "leaf_write",
            bench_key: "contree_pass_leaf_write_gpu",
        },
        ContreePassTimingPass {
            label: "buffer_update_after_leaf",
            bench_key: "contree_pass_buffer_update_after_leaf_gpu",
        },
    ];

    let tree_pass_count = total_levels.saturating_sub(2) as usize;
    for pass_index in 0..tree_pass_count {
        if let Some(pass) = CONTREE_TREE_WRITE_TIMING_PASSES.get(pass_index) {
            passes.push(*pass);
        }

        if pass_index + 1 == tree_pass_count {
            passes.push(ContreePassTimingPass {
                label: "last_buffer_update",
                bench_key: "contree_pass_last_buffer_update_gpu",
            });
        } else if let Some(pass) = CONTREE_BUFFER_UPDATE_TIMING_PASSES.get(pass_index) {
            passes.push(*pass);
        }
    }

    passes.push(ContreePassTimingPass {
        label: "concat",
        bench_key: "contree_pass_concat_gpu",
    });
    passes
}

fn contree_level_node_count(level: u32) -> u64 {
    64_u64.pow(level)
}

fn contree_level_node_offset(level: u32) -> u64 {
    let mut offset = 0;
    for current_level in 0..level {
        offset += contree_level_node_count(current_level);
    }
    offset
}

fn record_clear_sparse_leaf_nodes(
    device: &re_flora_vkn::Device,
    cmdbuf: &CommandBuffer,
    sparse_nodes: &Buffer,
    total_levels: u32,
) {
    let leaf_node_level = total_levels.saturating_sub(2);
    let offset_bytes = contree_level_node_offset(leaf_node_level) * SIZE_OF_NODE_ELEMENT;
    let size_bytes = contree_level_node_count(leaf_node_level) * SIZE_OF_NODE_ELEMENT;

    sparse_nodes.record_fill(cmdbuf, offset_bytes, size_bytes, 0);

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

struct CpuChunkReadbackBuffers {
    node_readback: Buffer,
    leaf_readback: Buffer,
}

struct CpuChunkCacheFenceJob {
    _command_buffer: CommandBuffer,
    fence: Fence,
    chunk_idx: UVec3,
    revision: u64,
    source: CpuChunkCacheBuildSource,
    readback_buffers: CpuChunkReadbackBuffers,
}

struct CpuChunkCacheWorkerJob {
    chunk_idx: UVec3,
    revision: u64,
    source: CpuChunkCacheBuildSource,
    readback_buffers: CpuChunkReadbackBuffers,
}

struct CpuChunkCacheWorkerResult {
    chunk_idx: UVec3,
    revision: u64,
    cache: Arc<CpuChunkCache>,
    readback_buffers: CpuChunkReadbackBuffers,
}

impl ContreeAnyHitRayTracer {
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }
}

impl BatchedAnyHitRayTracer for ContreeAnyHitRayTracer {
    fn trace_any_hit_batch(
        &self,
        rays: &[AcousticRay],
        min_distances: &[f32],
        max_distances: &[f32],
    ) -> Vec<bool> {
        if !self.enabled.load(Ordering::Relaxed) {
            return vec![false; rays.len()];
        }

        let trace_start = Instant::now();
        let Ok(shared_state) = self.shared_state.read() else {
            self.runtime_stats
                .update_failures
                .fetch_add(1, Ordering::Relaxed);
            return vec![false; rays.len()];
        };

        let results = rays
            .iter()
            .zip(min_distances.iter().copied())
            .zip(max_distances.iter().copied())
            .map(|((ray, min_distance), max_distance)| {
                query_terrain_any_hit(
                    &shared_state,
                    Vec3::new(ray.origin.x, ray.origin.y, ray.origin.z),
                    Vec3::new(ray.direction.x, ray.direction.y, ray.direction.z),
                    min_distance,
                    max_distance,
                )
            })
            .collect::<Vec<_>>();

        let occluded_sources = results.iter().filter(|is_occluded| **is_occluded).count();
        self.runtime_stats
            .update_count
            .fetch_add(1, Ordering::Relaxed);
        self.runtime_stats
            .updated_sources
            .fetch_add(results.len(), Ordering::Relaxed);
        self.runtime_stats
            .occluded_sources
            .fetch_add(occluded_sources, Ordering::Relaxed);
        self.runtime_stats
            .total_update_time_us
            .fetch_add(trace_start.elapsed().as_micros() as u64, Ordering::Relaxed);

        results
    }
}

impl ContreeBuilder {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        vulkan_ctx: VulkanContext,
        allocator: Allocator,
        shader_compiler: &ShaderCompiler,
        surfacer_resources: &SurfaceResources,
        chunk_dim: UVec3,
        voxel_dim_per_chunk: UVec3,
        node_pool_size_in_bytes: u64,
        leaf_pool_size_in_bytes: u64,
    ) -> Self {
        assert!(
            voxel_dim_per_chunk.x == voxel_dim_per_chunk.y
                && voxel_dim_per_chunk.x == voxel_dim_per_chunk.z,
            "ContreeBuilder: voxel_dim_per_chunk must be a cube"
        );
        assert!(is_power_of_four(voxel_dim_per_chunk.x));

        let device = vulkan_ctx.device();

        let contree_buffer_setup_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/contree/buffer_setup.comp",
            "main",
        )
        .unwrap();
        let contree_leaf_write_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/contree/leaf_write.comp",
            "main",
        )
        .unwrap();
        let contree_tree_write_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/contree/tree_write.comp",
            "main",
        )
        .unwrap();
        let contree_buffer_update_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/contree/buffer_update.comp",
            "main",
        )
        .unwrap();
        let contree_last_buffer_update_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/contree/last_buffer_update.comp",
            "main",
        )
        .unwrap();
        let contree_concat_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/contree/concat.comp",
            "main",
        )
        .unwrap();

        let resources = ContreeBuilderResources::new(
            device.clone(),
            allocator.clone(),
            voxel_dim_per_chunk,
            node_pool_size_in_bytes,
            leaf_pool_size_in_bytes,
            &contree_buffer_setup_sm,
            &contree_leaf_write_sm,
            &contree_tree_write_sm,
            &contree_last_buffer_update_sm,
        );

        let fixed_pool = DescriptorPool::new(device).unwrap();

        let contree_buffer_setup_ppl = ComputePipeline::new(
            device,
            &contree_buffer_setup_sm,
            &fixed_pool,
            &[&resources, surfacer_resources],
        );
        let contree_leaf_write_ppl = ComputePipeline::new(
            device,
            &contree_leaf_write_sm,
            &fixed_pool,
            &[&resources, surfacer_resources],
        );
        let contree_tree_write_ppl =
            ComputePipeline::new(device, &contree_tree_write_sm, &fixed_pool, &[&resources]);
        let contree_buffer_update_ppl = ComputePipeline::new(
            device,
            &contree_buffer_update_sm,
            &fixed_pool,
            &[&resources],
        );
        let contree_last_buffer_update_ppl = ComputePipeline::new(
            device,
            &contree_last_buffer_update_sm,
            &fixed_pool,
            &[&resources],
        );
        let contree_concat_ppl =
            ComputePipeline::new(device, &contree_concat_sm, &fixed_pool, &[&resources]);

        // // --- Descriptor Sets ---
        // let alloc_set_fn = |ppl: &ComputePipeline| -> DescriptorSet {
        //     fixed_pool
        //         .allocate_set(&ppl.get_layout().get_descriptor_set_layouts()[&0])
        //         .unwrap()
        // };

        // let contree_buffer_setup_ds = alloc_set_fn(&contree_buffer_setup_ppl);
        // let contree_leaf_write_ds = alloc_set_fn(&contree_leaf_write_ppl);
        // let contree_tree_write_ds = alloc_set_fn(&contree_tree_write_ppl);
        // let contree_buffer_update_ds = alloc_set_fn(&contree_buffer_update_ppl);
        // let contree_last_buffer_update_ds = alloc_set_fn(&contree_last_buffer_update_ppl);
        // let contree_concat_ds = alloc_set_fn(&contree_concat_ppl);

        // Self::update_contree_buffer_setup_ds(&contree_buffer_setup_ds, &resources);
        // Self::update_contree_leaf_write_ds(&contree_leaf_write_ds, &resources, surfacer_resources);
        // Self::update_contree_tree_write_ds(&contree_tree_write_ds, &resources);
        // Self::update_contree_buffer_update_ds(&contree_buffer_update_ds, &resources);
        // Self::update_contree_last_buffer_update_ds(&contree_last_buffer_update_ds, &resources);
        // Self::update_contree_concat_ds(&contree_concat_ds, &resources);

        // contree_buffer_setup_ppl.set_descriptor_sets(vec![contree_buffer_setup_ds]);
        // contree_leaf_write_ppl.set_descriptor_sets(vec![contree_leaf_write_ds]);
        // contree_tree_write_ppl.set_descriptor_sets(vec![contree_tree_write_ds]);
        // contree_buffer_update_ppl.set_descriptor_sets(vec![contree_buffer_update_ds]);
        // contree_last_buffer_update_ppl.set_descriptor_sets(vec![contree_last_buffer_update_ds]);
        // contree_concat_ppl.set_descriptor_sets(vec![contree_concat_ds]);

        // --- Command Buffer Recording ---
        let total_levels = get_level(voxel_dim_per_chunk);
        let pass_timing = ContreePassTiming::maybe_new(&vulkan_ctx, total_levels);
        let contree_cmdbuf = Self::record_cmdbuf(
            &vulkan_ctx,
            &resources,
            total_levels,
            &contree_buffer_setup_ppl,
            &contree_leaf_write_ppl,
            &contree_tree_write_ppl,
            &contree_buffer_update_ppl,
            &contree_last_buffer_update_ppl,
            &contree_concat_ppl,
            pass_timing.as_ref(),
        );

        let node_allocator = FirstFitAllocator::new(node_pool_size_in_bytes);
        let leaf_allocator = FirstFitAllocator::new(leaf_pool_size_in_bytes);
        let (cpu_chunk_cache_job_tx, cpu_chunk_cache_result_rx) =
            Self::spawn_cpu_chunk_cache_workers();
        let cpu_chunk_readback_buffers = Some(CpuChunkReadbackBuffers::new(
            device.clone(),
            allocator.clone(),
        ));

        let shared_ray_query_state = Arc::new(RwLock::new(ContreeRayQueryState {
            chunk_dim,
            cpu_scene_chunks: vec![None; (chunk_dim.x * chunk_dim.y * chunk_dim.z) as usize],
            cpu_chunk_caches: HashMap::new(),
        }));
        let audio_ray_tracer = Arc::new(ContreeAnyHitRayTracer {
            enabled: Arc::new(AtomicBool::new(true)),
            shared_state: shared_ray_query_state.clone(),
            runtime_stats: Arc::new(ContreeRayTracingRuntimeStats::default()),
        });

        Self {
            vulkan_ctx,
            resources,
            contree_buffer_setup_ppl,
            contree_leaf_write_ppl,
            contree_tree_write_ppl,
            contree_buffer_update_ppl,
            contree_last_buffer_update_ppl,
            contree_concat_ppl,
            fixed_pool,
            chunk_offset_allocation_table: HashMap::new(),
            pass_timing,
            contree_cmdbuf,
            node_allocator,
            leaf_allocator,
            chunk_dim,
            voxel_dim_per_chunk,
            cpu_scene_chunks: vec![None; (chunk_dim.x * chunk_dim.y * chunk_dim.z) as usize],
            cpu_chunk_caches: HashMap::new(),
            cpu_chunk_cache_queue: LatestChunkQueue::default(),
            cpu_chunk_source_revisions: HashMap::new(),
            cpu_chunk_source_updates: Vec::new(),
            cpu_chunk_readback_buffers,
            active_cpu_chunk_cache_job: None,
            cpu_chunk_cache_decode_inflight: false,
            cpu_chunk_cache_job_tx,
            cpu_chunk_cache_result_rx,
            shared_ray_query_state,
            audio_ray_tracer,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn record_cmdbuf(
        vulkan_ctx: &VulkanContext,
        resources: &ContreeBuilderResources,
        total_levels: u32,
        contree_buffer_setup_ppl: &ComputePipeline,
        contree_leaf_write_ppl: &ComputePipeline,
        contree_tree_write_ppl: &ComputePipeline,
        contree_buffer_update_ppl: &ComputePipeline,
        contree_last_buffer_update_ppl: &ComputePipeline,
        contree_concat_ppl: &ComputePipeline,
        pass_timing: Option<&ContreePassTiming>,
    ) -> CommandBuffer {
        let shader_access_memory_barrier = MemoryBarrier::new_shader_access();
        let indirect_access_memory_barrier = MemoryBarrier::new_indirect_access();

        let shader_access_pipeline_barrier = PipelineBarrier::new(
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vec![shader_access_memory_barrier],
        );
        let indirect_access_pipeline_barrier = PipelineBarrier::new(
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::DRAW_INDIRECT | vk::PipelineStageFlags::COMPUTE_SHADER,
            vec![indirect_access_memory_barrier],
        );

        let device = vulkan_ctx.device();
        let cmdbuf = CommandBuffer::new(device, vulkan_ctx.command_pool());
        cmdbuf.begin(false);

        let dispatch_1x1x1 = Extent3D {
            width: 1,
            height: 1,
            depth: 1,
        };

        if let Some(timing) = pass_timing {
            timing.record_reset(&cmdbuf);
        }
        let mut timing_pass_index = 0usize;
        macro_rules! record_timed_pass {
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

        record_timed_pass!({
            contree_buffer_setup_ppl.record(&cmdbuf, dispatch_1x1x1, None);
        });

        shader_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);
        indirect_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);

        record_clear_sparse_leaf_nodes(
            vulkan_ctx.device(),
            &cmdbuf,
            &resources.sparse_nodes,
            total_levels,
        );

        record_timed_pass!({
            contree_leaf_write_ppl.record_indirect(
                &cmdbuf,
                &resources.level_dispatch_indirect,
                None,
            );
        });

        shader_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);

        record_timed_pass!({
            contree_buffer_update_ppl.record(&cmdbuf, dispatch_1x1x1, None);
        });

        shader_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);
        indirect_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);

        for i in 0..(total_levels - 2) {
            record_timed_pass!({
                contree_tree_write_ppl.record_indirect(
                    &cmdbuf,
                    &resources.level_dispatch_indirect,
                    None,
                );
            });

            shader_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);

            if i != total_levels - 3 {
                record_timed_pass!({
                    contree_buffer_update_ppl.record(&cmdbuf, dispatch_1x1x1, None);
                });
            } else {
                record_timed_pass!({
                    contree_last_buffer_update_ppl.record(&cmdbuf, dispatch_1x1x1, None);
                });
            }

            shader_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);
            indirect_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);
        }

        record_timed_pass!({
            contree_concat_ppl.record_indirect(&cmdbuf, &resources.concat_dispatch_indirect, None);
        });

        if let Some(timing) = pass_timing {
            assert_eq!(timing_pass_index, timing.passes.len());
        }

        cmdbuf.end();
        cmdbuf
    }

    /// Returns: (node_size_in_bytes, leaf_size_in_bytes)
    pub fn get_contree_size_info(&self, resources: &ContreeBuilderResources) -> (u64, u64) {
        let readback_start = Instant::now();
        let raw_data = resources.contree_build_result.read_back().unwrap();
        crate::util::BENCH
            .lock()
            .unwrap()
            .record("contree_size_readback", readback_start.elapsed());
        assert!(
            raw_data.len() >= 8,
            "contree_build_result buffer too small: expected at least 8 bytes, got {}",
            raw_data.len()
        );
        let node_len = u32::from_ne_bytes(raw_data[0..4].try_into().unwrap()) as u64;
        let leaf_len = u32::from_ne_bytes(raw_data[4..8].try_into().unwrap()) as u64;
        let leaf_size_in_bytes = leaf_len * SIZE_OF_LEAF_ELEMENT;
        let node_size_in_bytes = node_len * SIZE_OF_NODE_ELEMENT;

        (node_size_in_bytes, leaf_size_in_bytes)
    }

    pub fn get_resources(&self) -> &ContreeBuilderResources {
        &self.resources
    }

    pub fn cpu_cached_chunk_count(&self) -> usize {
        self.cpu_chunk_caches.len()
    }

    pub fn audio_ray_tracer(&self) -> Arc<ContreeAnyHitRayTracer> {
        self.audio_ray_tracer.clone()
    }

    pub fn poll_cpu_chunk_cache_jobs(&mut self, focus: Vec3, chunk_extent: UVec3) {
        self.dispatch_completed_cpu_chunk_cache_jobs();
        self.publish_completed_cpu_chunk_cache_jobs(focus, chunk_extent);
        self.try_submit_next_cpu_chunk_cache_job(focus, chunk_extent);
    }

    pub fn flush_cpu_chunk_cache_jobs(&mut self) {
        loop {
            self.poll_cpu_chunk_cache_jobs(Vec3::ZERO, self.voxel_dim_per_chunk);
            if self.cpu_chunk_cache_jobs_idle() {
                break;
            }

            if let Some(job) = self.active_cpu_chunk_cache_job.as_ref() {
                if let Err(err) = job.fence.wait() {
                    log::error!(
                        "Failed to wait for CPU cache fence for {:?}: {err}",
                        job.chunk_idx
                    );
                    break;
                }
            } else {
                thread::sleep(Duration::from_millis(1));
            }
        }
    }

    pub fn cpu_chunk_cache_jobs_idle(&self) -> bool {
        self.cpu_chunk_cache_jobs_are_idle()
    }

    #[allow(dead_code)]
    pub fn cpu_chunk_query_source_ready(&self, chunk_idx: UVec3) -> bool {
        !self.cpu_chunk_cache_queue.has_unfinished_work(chunk_idx)
    }

    #[allow(dead_code)]
    pub fn cpu_chunk_query_source_revision(&self, chunk_idx: UVec3) -> Option<u64> {
        self.cpu_chunk_source_revisions.get(&chunk_idx).copied()
    }

    pub fn take_cpu_chunk_source_updates(&mut self) -> Vec<ContreeCpuChunkSourceUpdate> {
        std::mem::take(&mut self.cpu_chunk_source_updates)
    }

    #[allow(dead_code)]
    pub fn cpu_ray_query_snapshot(&self) -> ContreeCpuRayQuerySnapshot {
        ContreeCpuRayQuerySnapshot {
            chunk_dim: self.chunk_dim,
            cpu_scene_chunks: self.cpu_scene_chunks.clone(),
            cpu_chunk_caches: self.cpu_chunk_caches.clone(),
        }
    }

    pub fn query_terrain_ray_cpu(&self, origin: Vec3, direction: Vec3) -> Option<Vec3> {
        query_terrain_ray_against_state(
            self.chunk_dim,
            &self.cpu_scene_chunks,
            &self.cpu_chunk_caches,
            origin,
            direction,
        )
    }

    #[allow(dead_code)]
    pub fn query_terrain_occupancy_cpu(&self, point: Vec3) -> bool {
        query_terrain_occupancy_against_state(
            self.chunk_dim,
            &self.cpu_scene_chunks,
            &self.cpu_chunk_caches,
            point,
        )
    }

    #[allow(dead_code)]
    pub fn has_cpu_chunk_cache(&self, chunk_idx: UVec3) -> bool {
        if chunk_idx.cmplt(self.chunk_dim).any() {
            scene_chunk_present_in_grid(self.chunk_dim, &self.cpu_scene_chunks, chunk_idx)
                && self.cpu_chunk_caches.contains_key(&chunk_idx)
        } else {
            false
        }
    }

    #[allow(dead_code)]
    fn build_contree(
        &mut self,
        contree_dim: UVec3,
        node_write_offset: u64,
        leaf_write_offset: u64,
    ) -> Result<()> {
        update_buffers(
            &self.resources.contree_build_info,
            contree_dim,
            get_level(contree_dim),
            node_write_offset as u32,
            leaf_write_offset as u32,
        )?;

        let cmdbuf = self.contree_cmdbuf.clone();
        let fence = Fence::new(self.vulkan_ctx.device(), false);
        let submit_start = Instant::now();
        cmdbuf.submit(&self.vulkan_ctx.get_general_queue(), Some(&fence));
        let wait_start = Instant::now();
        fence.wait().unwrap();
        let wait_ms = wait_start.elapsed().as_secs_f32() * 1000.0;
        log::debug!(
            "[QUEUE][CONTREE_BUILD] dim={:?} node_offset={} leaf_offset={} submit_ms={:.2} fence_wait_ms={:.2}",
            contree_dim,
            node_write_offset,
            leaf_write_offset,
            submit_start.elapsed().as_secs_f32() * 1000.0,
            wait_ms,
        );

        return Ok(());

        fn update_buffers(
            contree_build_info: &Buffer,
            contree_dim: UVec3,
            max_level: u32,
            node_write_offset: u32,
            leaf_write_offset: u32,
        ) -> Result<()> {
            contree_build_info.fill_uniform(&ContreeBuildInfo {
                dim: contree_dim.x,
                max_level,
                node_write_offset,
                leaf_write_offset,
            })
        }
    }

    /// Returns: (node_alloc_offset, leaf_alloc_offset)
    pub fn build_and_alloc(&mut self, atlas_offset: UVec3) -> Result<Option<(u64, u64)>> {
        let job = self.submit_build_and_alloc(atlas_offset)?;
        job.fence.wait()?;
        Ok(self.finish_build_and_alloc(job)?.scene_offsets)
    }

    pub fn clear_empty_surface_chunk(&mut self, atlas_offset: UVec3) -> ContreeBuildResult {
        let total_start = Instant::now();
        let chunk_idx = atlas_offset / self.voxel_dim_per_chunk;
        let source_revision = self.clear_empty_chunk_state(atlas_offset, chunk_idx);
        let total_elapsed = total_start.elapsed();
        crate::util::BENCH
            .lock()
            .unwrap()
            .record("contree_empty_surface_skip_total", total_elapsed);
        log::debug!(
            "[QUEUE][CONTREE_REBUILD] chunk {:?} empty source_rev={} total_ms={:.2} gpu_submit_ms=0.00 fence_latency_ms=0.00 size_ms=0.00 skipped_surface_empty=true",
            chunk_idx,
            source_revision,
            total_elapsed.as_secs_f32() * 1000.0,
        );

        ContreeBuildResult {
            chunk_idx,
            scene_offsets: None,
            source_revision,
            prealloc_ms: 0.0,
            gpu_submit_ms: 0.0,
            fence_latency_ms: 0.0,
            size_ms: 0.0,
            confirm_ms: 0.0,
            total_ms: total_elapsed.as_secs_f64() * 1000.0,
            node_bytes: 0,
            leaf_bytes: 0,
        }
    }

    pub fn submit_build_and_alloc(&mut self, atlas_offset: UVec3) -> Result<ContreeBuildJob> {
        let total_start = Instant::now();
        let atlas_dim = self.voxel_dim_per_chunk;
        let chunk_idx = atlas_offset / self.voxel_dim_per_chunk;

        let alloc_start = Instant::now();
        let node_allocation = self
            .node_allocator
            .allocate(MAX_NODE_BUFFER_SIZE_IN_BYTES)
            .map_err(|err| anyhow::anyhow!("failed to allocate contree node buffer: {err}"))?;
        let leaf_allocation = self
            .leaf_allocator
            .allocate(MAX_LEAF_BUFFER_SIZE_IN_BYTES)
            .map_err(|err| anyhow::anyhow!("failed to allocate contree leaf buffer: {err}"))?;
        let prealloc_elapsed = alloc_start.elapsed();
        crate::util::BENCH
            .lock()
            .unwrap()
            .record("contree_pre_allocate", prealloc_elapsed);

        let node_alloc_offset_in_bytes = node_allocation.offset;
        let leaf_alloc_offset_in_bytes = leaf_allocation.offset;
        let node_alloc_offset = node_alloc_offset_in_bytes / SIZE_OF_NODE_ELEMENT;
        let leaf_alloc_offset = leaf_alloc_offset_in_bytes / SIZE_OF_LEAF_ELEMENT;

        update_contree_build_info(
            &self.resources.contree_build_info,
            atlas_dim,
            get_level(atlas_dim),
            node_alloc_offset as u32,
            leaf_alloc_offset as u32,
        )?;

        let cmdbuf = self.contree_cmdbuf.clone();
        let fence = Fence::new(self.vulkan_ctx.device(), false);
        let submit_start = Instant::now();
        cmdbuf.submit(&self.vulkan_ctx.get_general_queue(), Some(&fence));
        let submitted_at = Instant::now();
        let submit_elapsed = submit_start.elapsed();
        crate::util::BENCH
            .lock()
            .unwrap()
            .record("contree_build_gpu_submit", submit_elapsed);
        log::debug!(
            "[QUEUE][CONTREE_BUILD] submit chunk {:?} node_offset={} leaf_offset={} prealloc_ms={:.2} submit_ms={:.2}",
            chunk_idx,
            node_alloc_offset,
            leaf_alloc_offset,
            prealloc_elapsed.as_secs_f32() * 1000.0,
            submit_elapsed.as_secs_f32() * 1000.0,
        );

        Ok(ContreeBuildJob {
            atlas_offset,
            chunk_idx,
            node_alloc_id: node_allocation.id,
            leaf_alloc_id: leaf_allocation.id,
            node_alloc_offset,
            leaf_alloc_offset,
            total_start,
            submitted_at,
            prealloc_elapsed,
            submit_elapsed,
            _command_buffer: cmdbuf,
            fence,
        })
    }

    pub fn build_and_alloc_ready(&self, job: &ContreeBuildJob) -> Result<bool> {
        job.fence
            .is_signaled()
            .map_err(|err| anyhow::anyhow!("failed to poll contree build fence: {err}"))
    }

    pub fn wait_build_and_alloc(&self, job: &ContreeBuildJob) -> Result<()> {
        job.fence.wait()?;
        Ok(())
    }

    pub fn discard_build_and_alloc(&mut self, job: ContreeBuildJob) {
        if let Err(err) = self.node_allocator.deallocate(job.node_alloc_id) {
            log::error!(
                "Failed to deallocate stale contree node allocation for {:?}: {}",
                job.chunk_idx,
                err,
            );
        }
        if let Err(err) = self.leaf_allocator.deallocate(job.leaf_alloc_id) {
            log::error!(
                "Failed to deallocate stale contree leaf allocation for {:?}: {}",
                job.chunk_idx,
                err,
            );
        }
    }

    pub fn finish_build_and_alloc(&mut self, job: ContreeBuildJob) -> Result<ContreeBuildResult> {
        let fence_latency_elapsed = job.submitted_at.elapsed();
        crate::util::BENCH.lock().unwrap().record(
            "contree_build_gpu",
            fence_latency_elapsed + job.submit_elapsed,
        );
        if let Some(pass_timing) = self.pass_timing.as_ref() {
            pass_timing.collect_and_log(job.chunk_idx);
        }

        let size_start = Instant::now();
        let (confirmed_node_buffer_size_in_bytes, confirmed_leaf_buffer_size_in_bytes) =
            self.get_contree_size_info(&self.resources);
        let size_elapsed = size_start.elapsed();
        crate::util::BENCH
            .lock()
            .unwrap()
            .record("contree_size_total", size_elapsed);

        if confirmed_node_buffer_size_in_bytes == 0 || confirmed_leaf_buffer_size_in_bytes == 0 {
            let chunk_idx = job.chunk_idx;
            let source_revision = self.clear_empty_chunk_state(job.atlas_offset, chunk_idx);
            let total_elapsed = job.total_start.elapsed();
            let prealloc_ms = job.prealloc_elapsed.as_secs_f64() * 1000.0;
            let gpu_submit_ms = job.submit_elapsed.as_secs_f64() * 1000.0;
            self.discard_build_and_alloc(job);
            crate::util::BENCH
                .lock()
                .unwrap()
                .record("contree_build_and_alloc_total", total_elapsed);
            log::debug!(
                "[QUEUE][CONTREE_REBUILD] chunk {:?} empty source_rev={} total_ms={:.2} gpu_submit_ms={:.2} fence_latency_ms={:.2} size_ms={:.2}",
                chunk_idx,
                source_revision,
                total_elapsed.as_secs_f32() * 1000.0,
                gpu_submit_ms,
                fence_latency_elapsed.as_secs_f32() * 1000.0,
                size_elapsed.as_secs_f32() * 1000.0,
            );

            return Ok(ContreeBuildResult {
                chunk_idx,
                scene_offsets: None,
                source_revision,
                prealloc_ms,
                gpu_submit_ms,
                fence_latency_ms: fence_latency_elapsed.as_secs_f64() * 1000.0,
                size_ms: size_elapsed.as_secs_f64() * 1000.0,
                confirm_ms: 0.0,
                total_ms: total_elapsed.as_secs_f64() * 1000.0,
                node_bytes: 0,
                leaf_bytes: 0,
            });
        }

        let confirm_start = Instant::now();
        self.deallocate_chunk_allocation(job.atlas_offset);
        self.chunk_offset_allocation_table
            .insert(job.atlas_offset, (job.node_alloc_id, job.leaf_alloc_id));
        self.confirm_allocation_of_chunk(
            confirmed_node_buffer_size_in_bytes,
            confirmed_leaf_buffer_size_in_bytes,
            job.atlas_offset,
        );
        let confirm_elapsed = confirm_start.elapsed();
        crate::util::BENCH
            .lock()
            .unwrap()
            .record("contree_confirm_allocation", confirm_elapsed);

        let cpu_cache_source = CpuChunkCacheBuildSource {
            node_alloc_offset: job.node_alloc_offset,
            leaf_alloc_offset: job.leaf_alloc_offset,
            node_size_in_bytes: confirmed_node_buffer_size_in_bytes,
            leaf_size_in_bytes: confirmed_leaf_buffer_size_in_bytes,
        };
        self.queue_chunk_cpu_cache_rebuild(job.chunk_idx, cpu_cache_source);
        self.set_scene_chunk(job.chunk_idx, Some(job.chunk_idx));
        let total_elapsed = job.total_start.elapsed();
        crate::util::BENCH
            .lock()
            .unwrap()
            .record("contree_build_and_alloc_total", total_elapsed);
        let source_revision = self
            .cpu_chunk_source_revisions
            .get(&job.chunk_idx)
            .copied()
            .unwrap_or(0);
        log::debug!(
            "[QUEUE][CONTREE_REBUILD] chunk {:?} total_ms={:.2} prealloc_ms={:.2} gpu_submit_ms={:.2} fence_latency_ms={:.2} size_ms={:.2} confirm_ms={:.2} node_bytes={} leaf_bytes={}",
            job.chunk_idx,
            total_elapsed.as_secs_f32() * 1000.0,
            job.prealloc_elapsed.as_secs_f32() * 1000.0,
            job.submit_elapsed.as_secs_f32() * 1000.0,
            fence_latency_elapsed.as_secs_f32() * 1000.0,
            size_elapsed.as_secs_f32() * 1000.0,
            confirm_elapsed.as_secs_f32() * 1000.0,
            confirmed_node_buffer_size_in_bytes,
            confirmed_leaf_buffer_size_in_bytes,
        );

        Ok(ContreeBuildResult {
            chunk_idx: job.chunk_idx,
            scene_offsets: Some((job.node_alloc_offset, job.leaf_alloc_offset)),
            source_revision,
            prealloc_ms: job.prealloc_elapsed.as_secs_f64() * 1000.0,
            gpu_submit_ms: job.submit_elapsed.as_secs_f64() * 1000.0,
            fence_latency_ms: fence_latency_elapsed.as_secs_f64() * 1000.0,
            size_ms: size_elapsed.as_secs_f64() * 1000.0,
            confirm_ms: confirm_elapsed.as_secs_f64() * 1000.0,
            total_ms: total_elapsed.as_secs_f64() * 1000.0,
            node_bytes: confirmed_node_buffer_size_in_bytes,
            leaf_bytes: confirmed_leaf_buffer_size_in_bytes,
        })
    }

    fn queue_chunk_cpu_cache_rebuild(
        &mut self,
        chunk_idx: UVec3,
        source: CpuChunkCacheBuildSource,
    ) {
        let revision = self.cpu_chunk_cache_queue.push(chunk_idx, source);
        log::debug!(
            "[QUEUE][CPU_CACHE] enqueue chunk {:?} revision {} pending={} active={} decode_inflight={}",
            chunk_idx,
            revision,
            self.cpu_chunk_cache_queue.len(),
            self.cpu_chunk_cache_queue.active_len(),
            self.cpu_chunk_cache_decode_inflight,
        );
        self.try_submit_next_cpu_chunk_cache_job(Vec3::ZERO, self.voxel_dim_per_chunk);
    }

    fn submit_chunk_cpu_cache_rebuild(
        &mut self,
        chunk_idx: UVec3,
        revision: u64,
        source: CpuChunkCacheBuildSource,
    ) {
        assert!(source.node_size_in_bytes <= MAX_NODE_BUFFER_SIZE_IN_BYTES);
        assert!(source.leaf_size_in_bytes <= MAX_LEAF_BUFFER_SIZE_IN_BYTES);

        let readback_buffers = self
            .cpu_chunk_readback_buffers
            .take()
            .expect("CPU chunk readback buffers should be available before submit");
        let command_buffer =
            CommandBuffer::new(self.vulkan_ctx.device(), self.vulkan_ctx.command_pool());
        let fence = Fence::new(self.vulkan_ctx.device(), false);

        let gpu_copy_start = Instant::now();
        command_buffer.begin(true);
        self.resources.contree_node_data.record_copy_to_buffer(
            &command_buffer,
            &readback_buffers.node_readback,
            source.node_size_in_bytes,
            source.node_alloc_offset * SIZE_OF_NODE_ELEMENT,
            0,
        );
        self.resources.contree_leaf_data.record_copy_to_buffer(
            &command_buffer,
            &readback_buffers.leaf_readback,
            source.leaf_size_in_bytes,
            source.leaf_alloc_offset * SIZE_OF_LEAF_ELEMENT,
            0,
        );
        command_buffer.end();
        command_buffer.submit(&self.vulkan_ctx.get_general_queue(), Some(&fence));
        let gpu_copy_elapsed = gpu_copy_start.elapsed();
        crate::util::BENCH
            .lock()
            .unwrap()
            .record("contree_cpu_cache_copy_to_readback", gpu_copy_elapsed);

        self.active_cpu_chunk_cache_job = Some(CpuChunkCacheFenceJob {
            _command_buffer: command_buffer,
            fence,
            chunk_idx,
            revision,
            source,
            readback_buffers,
        });
        log::debug!(
            "[QUEUE][CPU_CACHE] submit chunk {:?} revision {} pending={} gpu_copy_ms={:.2}",
            chunk_idx,
            revision,
            self.cpu_chunk_cache_queue.len(),
            gpu_copy_elapsed.as_secs_f32() * 1000.0,
        );
    }

    fn dispatch_completed_cpu_chunk_cache_jobs(&mut self) {
        let Some(job) = self.active_cpu_chunk_cache_job.as_ref() else {
            return;
        };

        let fence_done = match job.fence.is_signaled() {
            Ok(done) => done,
            Err(err) => {
                log::error!(
                    "Failed to poll CPU cache fence for {:?} rev {}: {err}",
                    job.chunk_idx,
                    job.revision,
                );
                return;
            }
        };

        if !fence_done {
            return;
        }

        let job = self
            .active_cpu_chunk_cache_job
            .take()
            .expect("active CPU chunk cache job disappeared after fence poll");
        log::debug!(
            "[QUEUE][CPU_CACHE] fence_ready chunk {:?} revision {} pending={}",
            job.chunk_idx,
            job.revision,
            self.cpu_chunk_cache_queue.len(),
        );
        self.cpu_chunk_cache_decode_inflight = true;
        let worker_job = CpuChunkCacheWorkerJob {
            chunk_idx: job.chunk_idx,
            revision: job.revision,
            source: job.source,
            readback_buffers: job.readback_buffers,
        };
        if let Err(err) = self.cpu_chunk_cache_job_tx.send(worker_job) {
            log::error!(
                "Failed to dispatch CPU cache worker job for {:?} rev {}: {err}",
                job.chunk_idx,
                job.revision,
            );
            self.cpu_chunk_cache_decode_inflight = false;
            let failed_job = err.0;
            self.cpu_chunk_readback_buffers = Some(failed_job.readback_buffers);
            self.cpu_chunk_cache_queue
                .complete(failed_job.chunk_idx, failed_job.revision);
        }
    }

    fn publish_completed_cpu_chunk_cache_jobs(&mut self, focus: Vec3, chunk_extent: UVec3) {
        while let Ok(result) = self.cpu_chunk_cache_result_rx.try_recv() {
            self.cpu_chunk_cache_decode_inflight = false;
            self.cpu_chunk_readback_buffers = Some(result.readback_buffers);
            let should_publish = self
                .cpu_chunk_cache_queue
                .is_latest_revision(result.chunk_idx, result.revision);

            let source_revision = if should_publish {
                self.cpu_chunk_caches
                    .insert(result.chunk_idx, result.cache.clone());
                self.publish_shared_chunk_cache(result.chunk_idx, result.cache);
                self.set_scene_chunk(result.chunk_idx, Some(result.chunk_idx));
                Some(self.record_cpu_chunk_source_update(result.chunk_idx, true))
            } else {
                None
            };

            self.cpu_chunk_cache_queue
                .complete(result.chunk_idx, result.revision);
            log::debug!(
                "[QUEUE][CPU_CACHE] publish chunk {:?} revision {} published={} source_rev={:?} pending={} cached={}",
                result.chunk_idx,
                result.revision,
                should_publish,
                source_revision,
                self.cpu_chunk_cache_queue.len(),
                self.cpu_chunk_caches.len(),
            );
            self.try_submit_next_cpu_chunk_cache_job(focus, chunk_extent);
        }
    }

    fn clear_empty_chunk_state(&mut self, atlas_offset: UVec3, chunk_idx: UVec3) -> u64 {
        self.cpu_chunk_caches.remove(&chunk_idx);
        self.remove_shared_chunk_cache(chunk_idx);
        self.cpu_chunk_cache_queue.clear(chunk_idx);
        self.set_scene_chunk(chunk_idx, None);
        let source_revision = self.record_cpu_chunk_source_update(chunk_idx, false);
        self.deallocate_chunk_allocation(atlas_offset);
        source_revision
    }

    fn deallocate_chunk_allocation(&mut self, atlas_offset: UVec3) {
        if let Some((node_alloc_id, leaf_alloc_id)) =
            self.chunk_offset_allocation_table.remove(&atlas_offset)
        {
            self.node_allocator.deallocate(node_alloc_id).unwrap();
            self.leaf_allocator.deallocate(leaf_alloc_id).unwrap();
        }
    }

    fn set_scene_chunk(&mut self, chunk_idx: UVec3, chunk: Option<UVec3>) {
        let index = self.scene_chunk_flat_index(chunk_idx);
        self.cpu_scene_chunks[index] = chunk;

        if let Ok(mut shared_state) = self.shared_ray_query_state.write() {
            shared_state.cpu_scene_chunks[index] = chunk;
        }
    }

    fn record_cpu_chunk_source_update(&mut self, chunk_idx: UVec3, is_present: bool) -> u64 {
        let revision = self
            .cpu_chunk_source_revisions
            .get(&chunk_idx)
            .copied()
            .unwrap_or(0)
            + 1;
        self.cpu_chunk_source_revisions.insert(chunk_idx, revision);
        self.cpu_chunk_source_updates
            .push(ContreeCpuChunkSourceUpdate {
                chunk_idx,
                revision,
                is_present,
            });
        revision
    }

    fn publish_shared_chunk_cache(&self, chunk_idx: UVec3, cache: Arc<CpuChunkCache>) {
        if let Ok(mut shared_state) = self.shared_ray_query_state.write() {
            shared_state.cpu_chunk_caches.insert(chunk_idx, cache);
        }
    }

    fn remove_shared_chunk_cache(&self, chunk_idx: UVec3) {
        if let Ok(mut shared_state) = self.shared_ray_query_state.write() {
            shared_state.cpu_chunk_caches.remove(&chunk_idx);
        }
    }

    fn scene_chunk_flat_index(&self, chunk_idx: UVec3) -> usize {
        scene_chunk_flat_index(self.chunk_dim, chunk_idx)
    }

    /// Allocate a chunk of data and store the allocation id in the offset_allocation_table.
    ///
    /// Returns: (node_alloc_offset_in_bytes, leaf_alloc_offset_in_bytes)
    /// If the chunk already exists, deallocate it first.
    #[allow(dead_code)]
    fn pre_allocate_chunk(
        &mut self,
        max_node_buffer_size_in_bytes: u64,
        max_leaf_buffer_size_in_bytes: u64,
        atlas_offset: UVec3,
    ) -> (u64, u64) {
        self.deallocate_chunk_allocation(atlas_offset);
        let node_allocation = self
            .node_allocator
            .allocate(max_node_buffer_size_in_bytes)
            .unwrap();
        let leaf_allocation = self
            .leaf_allocator
            .allocate(max_leaf_buffer_size_in_bytes)
            .unwrap();

        self.chunk_offset_allocation_table
            .insert(atlas_offset, (node_allocation.id, leaf_allocation.id));
        (node_allocation.offset, leaf_allocation.offset)
    }

    fn confirm_allocation_of_chunk(
        &mut self,
        confirmed_node_buffer_size_in_bytes: u64,
        confirmed_leaf_buffer_size_in_bytes: u64,
        atlas_offset: UVec3,
    ) {
        let (node_alloc_id, leaf_alloc_id) = self
            .chunk_offset_allocation_table
            .get(&atlas_offset)
            .expect("Chunk not found in allocation table");

        self.node_allocator
            .resize(*node_alloc_id, confirmed_node_buffer_size_in_bytes)
            .unwrap();
        self.leaf_allocator
            .resize(*leaf_alloc_id, confirmed_leaf_buffer_size_in_bytes)
            .unwrap();
    }

    fn cpu_chunk_cache_jobs_are_idle(&self) -> bool {
        self.cpu_chunk_cache_queue.is_idle()
            && self.active_cpu_chunk_cache_job.is_none()
            && !self.cpu_chunk_cache_decode_inflight
    }

    fn try_submit_next_cpu_chunk_cache_job(&mut self, focus: Vec3, chunk_extent: UVec3) {
        if self.active_cpu_chunk_cache_job.is_some()
            || self.cpu_chunk_cache_decode_inflight
            || self.cpu_chunk_readback_buffers.is_none()
        {
            return;
        }

        if let Some(work) = self
            .cpu_chunk_cache_queue
            .pop_nearest_to(focus, chunk_extent)
        {
            log::debug!(
                "[QUEUE][CPU_CACHE] pop_nearest chunk {:?} revision {} focus={:?} remaining={}",
                work.chunk_id,
                work.revision,
                focus,
                self.cpu_chunk_cache_queue.len(),
            );
            self.submit_chunk_cpu_cache_rebuild(work.chunk_id, work.revision, work.payload);
        }
    }

    fn spawn_cpu_chunk_cache_workers() -> (
        mpsc::Sender<CpuChunkCacheWorkerJob>,
        mpsc::Receiver<CpuChunkCacheWorkerResult>,
    ) {
        let (job_tx, job_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        thread::spawn(move || loop {
            let job = match job_rx.recv() {
                Ok(job) => job,
                Err(_) => break,
            };

            match decode_cpu_chunk_cache_job(job) {
                Ok(result) => {
                    if result_tx.send(result).is_err() {
                        break;
                    }
                }
                Err(err) => {
                    log::error!("Failed to decode CPU chunk cache job: {err}");
                }
            }
        });

        (job_tx, result_rx)
    }
}

impl CpuChunkReadbackBuffers {
    fn new(device: re_flora_vkn::Device, allocator: Allocator) -> Self {
        Self {
            node_readback: Buffer::new_sized(
                device.clone(),
                allocator.clone(),
                BufferUsage::from_flags(vk::BufferUsageFlags::TRANSFER_DST),
                MemoryLocation::GpuToCpu,
                MAX_NODE_BUFFER_SIZE_IN_BYTES,
            ),
            leaf_readback: Buffer::new_sized(
                device,
                allocator,
                BufferUsage::from_flags(vk::BufferUsageFlags::TRANSFER_DST),
                MemoryLocation::GpuToCpu,
                MAX_LEAF_BUFFER_SIZE_IN_BYTES,
            ),
        }
    }
}

fn decode_cpu_chunk_cache_job(job: CpuChunkCacheWorkerJob) -> Result<CpuChunkCacheWorkerResult> {
    let readback_start = Instant::now();
    let node_bytes = job
        .readback_buffers
        .node_readback
        .read_back_range(0, job.source.node_size_in_bytes)?;
    let leaf_bytes = job
        .readback_buffers
        .leaf_readback
        .read_back_range(0, job.source.leaf_size_in_bytes)?;
    let readback_elapsed = readback_start.elapsed();
    crate::util::BENCH
        .lock()
        .unwrap()
        .record("contree_cpu_cache_readback", readback_elapsed);

    let decode_start = Instant::now();
    let nodes = node_bytes
        .chunks_exact(SIZE_OF_NODE_ELEMENT as usize)
        .map(|chunk| CpuContreeNode {
            packed_0: u32::from_ne_bytes(chunk[0..4].try_into().unwrap()),
            child_mask_lo: u32::from_ne_bytes(chunk[4..8].try_into().unwrap()),
            child_mask_hi: u32::from_ne_bytes(chunk[8..12].try_into().unwrap()),
        })
        .collect();
    let leaves = leaf_bytes
        .chunks_exact(SIZE_OF_LEAF_ELEMENT as usize)
        .map(|chunk| u32::from_ne_bytes(chunk[0..4].try_into().unwrap()))
        .collect();
    let decode_elapsed = decode_start.elapsed();
    crate::util::BENCH
        .lock()
        .unwrap()
        .record("contree_cpu_cache_decode", decode_elapsed);

    Ok(CpuChunkCacheWorkerResult {
        chunk_idx: job.chunk_idx,
        revision: job.revision,
        cache: Arc::new(CpuChunkCache {
            chunk_idx: job.chunk_idx,
            nodes,
            leaves,
        }),
        readback_buffers: job.readback_buffers,
    })
}

fn query_terrain_any_hit(
    state: &ContreeRayQueryState,
    origin: Vec3,
    direction: Vec3,
    min_distance: f32,
    max_distance: f32,
) -> bool {
    if direction.length_squared() <= f32::EPSILON {
        return false;
    }

    let start_distance = (min_distance + AUDIO_RAY_START_EPSILON).max(0.0);
    let end_distance = (max_distance - AUDIO_RAY_ENDPOINT_EPSILON).max(start_distance);
    if end_distance <= start_distance {
        return false;
    }

    let normalized_dir = direction.normalize();
    let segment_origin = origin + normalized_dir * start_distance;
    let segment_length = end_distance - start_distance;

    query_terrain_ray_from_snapshot(state, segment_origin, normalized_dir)
        .is_some_and(|hit| hit.distance(segment_origin) <= segment_length)
}

fn query_terrain_ray_from_snapshot(
    state: &ContreeRayQueryState,
    origin: Vec3,
    direction: Vec3,
) -> Option<Vec3> {
    query_terrain_ray_against_state(
        state.chunk_dim,
        &state.cpu_scene_chunks,
        &state.cpu_chunk_caches,
        origin,
        direction,
    )
}

fn query_terrain_ray_against_state(
    chunk_dim: UVec3,
    cpu_scene_chunks: &[Option<UVec3>],
    cpu_chunk_caches: &HashMap<UVec3, Arc<CpuChunkCache>>,
    origin: Vec3,
    direction: Vec3,
) -> Option<Vec3> {
    if direction.length_squared() <= f32::EPSILON {
        return None;
    }

    let normalized_dir = direction.normalize();
    let inv_dir = reciprocal(normalized_dir);
    let marched_dir = sanitize_dda_direction(normalized_dir);

    let slab = slabs(Vec3::ZERO, chunk_dim.as_vec3(), origin, inv_dir);
    if slab.x > slab.y || slab.y < 0.0 {
        return None;
    }

    let march_extent = slab.x.max(0.0) + DDA_EPSILON;
    let marched_origin = origin + march_extent * marched_dir;
    let delta_dist = reciprocal(marched_dir.abs());
    let ray_step = marched_dir.signum().as_ivec3();
    let mut map_pos = marched_origin.floor().as_ivec3();
    let ray_sign = marched_dir.signum();
    let mut side_dist = (((ray_sign * 0.5) + Vec3::splat(0.5))
        + ray_sign * (map_pos.as_vec3() - marched_origin))
        * delta_dist;

    for _ in 0..MAX_DDA_ITERATION {
        let min_mask = [
            side_dist.x <= side_dist.y.min(side_dist.z),
            side_dist.y <= side_dist.z.min(side_dist.x),
            side_dist.z <= side_dist.x.min(side_dist.y),
        ];
        side_dist += Vec3::new(
            if min_mask[0] { delta_dist.x } else { 0.0 },
            if min_mask[1] { delta_dist.y } else { 0.0 },
            if min_mask[2] { delta_dist.z } else { 0.0 },
        );

        if !in_aabb_i(map_pos, glam::IVec3::ZERO, chunk_dim.as_ivec3()) {
            break;
        }

        let chunk_idx = map_pos.as_uvec3();
        if scene_chunk_present_in_grid(chunk_dim, cpu_scene_chunks, chunk_idx) {
            if let Some(cache) = cpu_chunk_caches.get(&chunk_idx) {
                if let Some(hit) =
                    query_cached_chunk_cpu_ray(cache.as_ref(), marched_origin, marched_dir)
                {
                    return Some(hit);
                }
            }
        }

        map_pos += glam::IVec3::new(
            if min_mask[0] { ray_step.x } else { 0 },
            if min_mask[1] { ray_step.y } else { 0 },
            if min_mask[2] { ray_step.z } else { 0 },
        );
    }

    None
}

fn query_terrain_occupancy_against_state(
    chunk_dim: UVec3,
    cpu_scene_chunks: &[Option<UVec3>],
    cpu_chunk_caches: &HashMap<UVec3, Arc<CpuChunkCache>>,
    point: Vec3,
) -> bool {
    if !point.is_finite() {
        return false;
    }

    let chunk_pos = point.floor().as_ivec3();
    if !in_aabb_i(chunk_pos, glam::IVec3::ZERO, chunk_dim.as_ivec3()) {
        return false;
    }

    let chunk_idx = chunk_pos.as_uvec3();
    if !scene_chunk_present_in_grid(chunk_dim, cpu_scene_chunks, chunk_idx) {
        return false;
    }

    cpu_chunk_caches
        .get(&chunk_idx)
        .is_some_and(|cache| query_cached_chunk_cpu_occupancy(cache.as_ref(), point))
}

fn query_cached_chunk_cpu_occupancy(cache: &CpuChunkCache, point: Vec3) -> bool {
    if cache.nodes.is_empty() {
        return false;
    }

    let local_pos = point - cache.chunk_idx.as_vec3() + Vec3::ONE;
    if local_pos.cmplt(Vec3::ONE).any() || local_pos.cmpge(Vec3::splat(2.0)).any() {
        return false;
    }

    let mut scale_exp = 21i32;
    let mut node = cache.nodes[0];
    for _ in 0..16 {
        let Some(child_idx) = get_node_cell_index(local_pos, scale_exp).map(|idx| idx as u32)
        else {
            return false;
        };
        if !child_mask_test(node, child_idx) {
            return false;
        }

        let bits = child_mask_bitcount_below(node, child_idx);
        let child_addr = ((node.packed_0 >> 1) + bits) as usize;
        if is_leaf(node) {
            return cache
                .leaves
                .get(child_addr)
                .is_some_and(|voxel| *voxel != 0);
        }

        let Some(next_node) = cache.nodes.get(child_addr).copied() else {
            return false;
        };
        node = next_node;
        scale_exp -= 2;
    }

    false
}

fn query_cached_chunk_cpu_ray(
    cache: &CpuChunkCache,
    origin: Vec3,
    direction: Vec3,
) -> Option<Vec3> {
    if direction.length_squared() <= f32::EPSILON || cache.nodes.is_empty() {
        return None;
    }

    let local_origin = origin - cache.chunk_idx.as_vec3() + Vec3::ONE;
    let local_hit = march_contree_cpu(local_origin, direction, &cache.nodes, &cache.leaves)?;
    Some(local_hit + cache.chunk_idx.as_vec3() - Vec3::ONE)
}

fn scene_chunk_present_in_grid(
    chunk_dim: UVec3,
    cpu_scene_chunks: &[Option<UVec3>],
    chunk_idx: UVec3,
) -> bool {
    cpu_scene_chunks[scene_chunk_flat_index(chunk_dim, chunk_idx)].is_some()
}

fn scene_chunk_flat_index(chunk_dim: UVec3, chunk_idx: UVec3) -> usize {
    (chunk_idx.x + chunk_idx.z * chunk_dim.x + chunk_idx.y * chunk_dim.x * chunk_dim.z) as usize
}

/// Returns true if `n` is a power of four (1, 4, 16, 64, …).
///
/// Uses two bit-tricks:
/// 1. `n & (n - 1) == 0` ensures `n` is a power of two (only one bit set).
/// 2. `0x5555_5555` has 1s in all even bit positions (0,2,4,…).
///    Masking with it ensures the single bit of `n` is in an even position.
fn is_power_of_four(n: u32) -> bool {
    n != 0
        && (n & (n - 1)) == 0         // power of two?
        && (n & 0x5555_5555) != 0 // bit in an even position?
}

fn log_4(n: u32) -> u32 {
    // trailing_zeros gives 2*k, so divide by 2:
    n.trailing_zeros() / 2
}

fn get_level(contree_dim: UVec3) -> u32 {
    log_4(contree_dim.x) + 1
}

fn update_contree_build_info(
    contree_build_info: &Buffer,
    contree_dim: UVec3,
    max_level: u32,
    node_write_offset: u32,
    leaf_write_offset: u32,
) -> Result<()> {
    contree_build_info.fill_uniform(&ContreeBuildInfo {
        dim: contree_dim.x,
        max_level,
        node_write_offset,
        leaf_write_offset,
    })
}

fn march_contree_cpu(
    origin: Vec3,
    dir: Vec3,
    nodes: &[CpuContreeNode],
    leaves: &[u32],
) -> Option<Vec3> {
    if nodes.is_empty() {
        return None;
    }

    let mut stack = [0u32; 11];
    let mut scale_exp = 21i32;
    let mut node_idx = 0u32;
    let mut node = *nodes.get(node_idx as usize)?;

    let slab = slabs(Vec3::ONE, Vec3::splat(1.999_999_9), origin, reciprocal(dir));
    if slab.x > slab.y || slab.y < 0.0 {
        return None;
    }
    let origin = origin + dir * slab.x.max(0.0);

    let mut mirror_mask = 0u32;
    if dir.x > 0.0 {
        mirror_mask |= 3u32;
    }
    if dir.y > 0.0 {
        mirror_mask |= 3u32 << 4;
    }
    if dir.z > 0.0 {
        mirror_mask |= 3u32 << 2;
    }

    let origin = get_mirrored_pos(origin, dir, true);
    let mut pos = origin.clamp(Vec3::ONE, Vec3::splat(1.999_999_9));
    let inv_dir = -reciprocal(dir.abs());

    for _ in 0..1024 {
        let mut child_idx = (get_node_cell_index(pos, scale_exp)? as u32) ^ mirror_mask;

        while child_mask_test(node, child_idx) && !is_leaf(node) {
            stack[(scale_exp >> 1) as usize] = node_idx;

            let bits = child_mask_bitcount_below(node, child_idx);
            node_idx = (node.packed_0 >> 1) + bits;
            node = *nodes.get(node_idx as usize)?;

            scale_exp -= 2;
            child_idx = (get_node_cell_index(pos, scale_exp)? as u32) ^ mirror_mask;
        }

        if child_mask_test(node, child_idx) && is_leaf(node) {
            let pos = get_mirrored_pos(pos, dir, false);
            let child_idx = get_node_cell_index(pos, scale_exp)? as u32;
            let bits = child_mask_bitcount_below(node, child_idx);
            let voxel_addr = ((node.packed_0 >> 1) + bits) as usize;
            if voxel_addr < leaves.len() {
                return Some(pos);
            }
            return None;
        }

        let mut adv_scale_exp = scale_exp;
        let shifted_idx = child_idx & 0x2A;
        let has_neighbor = if shifted_idx < 32 {
            ((node.child_mask_lo >> shifted_idx) & 0x0033_0033) != 0
        } else {
            ((node.child_mask_hi >> (shifted_idx - 32)) & 0x0033_0033) != 0
        };
        if !has_neighbor {
            adv_scale_exp += 1;
        }

        let cell_min = floor_scale(pos, adv_scale_exp);
        let side_dist = (cell_min - origin) * inv_dir;
        let tmax = side_dist.x.min(side_dist.y.min(side_dist.z));

        let side_mask = [
            tmax >= side_dist.x,
            tmax >= side_dist.y,
            tmax >= side_dist.z,
        ];
        let base = [
            cell_min.x.to_bits() as i32,
            cell_min.y.to_bits() as i32,
            cell_min.z.to_bits() as i32,
        ];
        let off = (1 << adv_scale_exp) - 1;
        let neighbor_max = [
            base[0] + if side_mask[0] { -1 } else { off },
            base[1] + if side_mask[1] { -1 } else { off },
            base[2] + if side_mask[2] { -1 } else { off },
        ];

        pos = (origin - dir.abs() * tmax).min(Vec3::new(
            f32::from_bits(neighbor_max[0] as u32),
            f32::from_bits(neighbor_max[1] as u32),
            f32::from_bits(neighbor_max[2] as u32),
        ));

        let combined = ((pos.x.to_bits() ^ cell_min.x.to_bits())
            | (pos.y.to_bits() ^ cell_min.y.to_bits())
            | (pos.z.to_bits() ^ cell_min.z.to_bits()))
            & 0xFFAA_AAAA;
        let diff_exp = find_msb(combined);
        if diff_exp > scale_exp {
            scale_exp = diff_exp;
            if diff_exp > 21 {
                break;
            }
            node_idx = stack[(scale_exp >> 1) as usize];
            node = *nodes.get(node_idx as usize)?;
        }
    }

    None
}

fn slabs(min_bound: Vec3, max_bound: Vec3, origin: Vec3, inv_dir: Vec3) -> Vec2 {
    let t0 = (min_bound - origin) * inv_dir;
    let t1 = (max_bound - origin) * inv_dir;
    let tmin = t0.min(t1);
    let tmax = t0.max(t1);
    Vec2::new(tmin.max_element(), tmax.min_element())
}

fn reciprocal(v: Vec3) -> Vec3 {
    Vec3::new(1.0 / v.x, 1.0 / v.y, 1.0 / v.z)
}

fn sanitize_dda_direction(direction: Vec3) -> Vec3 {
    let abs_dir = direction.abs().max(Vec3::splat(DDA_EPSILON));
    abs_dir * direction.signum()
}

fn in_aabb_i(point: glam::IVec3, box_min: glam::IVec3, box_max: glam::IVec3) -> bool {
    point.cmpge(box_min).all() && point.cmplt(box_max).all()
}

fn get_mirrored_pos(pos: Vec3, dir: Vec3, range_check: bool) -> Vec3 {
    let mirrored = Vec3::new(
        f32::from_bits(pos.x.to_bits() ^ 0x007F_FFFF),
        f32::from_bits(pos.y.to_bits() ^ 0x007F_FFFF),
        f32::from_bits(pos.z.to_bits() ^ 0x007F_FFFF),
    );

    let mirrored =
        if range_check && (pos.cmplt(Vec3::ONE).any() || pos.cmpge(Vec3::splat(2.0)).any()) {
            Vec3::splat(3.0) - pos
        } else {
            mirrored
        };

    Vec3::new(
        if dir.x > 0.0 { mirrored.x } else { pos.x },
        if dir.y > 0.0 { mirrored.y } else { pos.y },
        if dir.z > 0.0 { mirrored.z } else { pos.z },
    )
}

fn get_node_cell_index(pos: Vec3, scale_exp: i32) -> Option<i32> {
    let shift = u32::try_from(scale_exp).ok()?;
    let px = (pos.x.to_bits() >> shift) & 3;
    let py = (pos.y.to_bits() >> shift) & 3;
    let pz = (pos.z.to_bits() >> shift) & 3;
    Some((px + pz * 4 + py * 16) as i32)
}

fn floor_scale(pos: Vec3, scale_exp: i32) -> Vec3 {
    let mask = !0u32 << (scale_exp as u32);
    Vec3::new(
        f32::from_bits(pos.x.to_bits() & mask),
        f32::from_bits(pos.y.to_bits() & mask),
        f32::from_bits(pos.z.to_bits() & mask),
    )
}

fn is_leaf(node: CpuContreeNode) -> bool {
    (node.packed_0 & 1) != 0
}

fn child_mask_test(node: CpuContreeNode, idx: u32) -> bool {
    if idx < 32 {
        (node.child_mask_lo & (1 << idx)) != 0
    } else {
        (node.child_mask_hi & (1 << (idx - 32))) != 0
    }
}

fn child_mask_bitcount_below(node: CpuContreeNode, idx: u32) -> u32 {
    if idx < 32 {
        (node.child_mask_lo & ((1 << idx) - 1)).count_ones()
    } else {
        node.child_mask_lo.count_ones()
            + (node.child_mask_hi & ((1 << (idx - 32)) - 1)).count_ones()
    }
}

fn find_msb(value: u32) -> i32 {
    if value == 0 {
        -1
    } else {
        31 - value.leading_zeros() as i32
    }
}
