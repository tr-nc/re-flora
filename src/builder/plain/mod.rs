mod resources;
use crate::generated::gpu_structs::{
    BvhNodes, ChunkModifyInfo, ChunkSolidSampleInfo, Cuboids, ModelVoxelizeInfo,
    PushConstantChunkModifySample, RegionInfo, RoundCones, Spheres,
};
use crate::geom::{BvhNode, Cuboid, RoundCone, Sphere, UAabb3};
use crate::model::ModelTriangleGpu;
use crate::util::ShaderCompiler;
use anyhow::Result;
use bytemuck::{Pod, Zeroable};
use glam::{IVec3, UVec3, Vec3};
use re_flora_vkn::execute_one_time_command;
use re_flora_vkn::execute_one_time_command_with_fence;
use re_flora_vkn::vk;
use re_flora_vkn::Allocator;
use re_flora_vkn::Buffer;
use re_flora_vkn::BufferUsage;
use re_flora_vkn::ClearValue;
use re_flora_vkn::ColorClearValue;
use re_flora_vkn::CommandBuffer;
use re_flora_vkn::ComputePipeline;
use re_flora_vkn::DescriptorPool;
use re_flora_vkn::Extent3D;
use re_flora_vkn::Fence;
use re_flora_vkn::MemoryBarrier;
use re_flora_vkn::MemoryLocation;
use re_flora_vkn::PipelineBarrier;
use re_flora_vkn::ShaderModule;
use re_flora_vkn::Texture;
use re_flora_vkn::TextureRegion;
use re_flora_vkn::VulkanContext;
pub use resources::*;
use std::convert::TryInto;
use std::time::{Duration, Instant};

pub const VOXEL_TYPE_CHERRY_WOOD: u32 = 5;
pub const VOXEL_TYPE_OAK_WOOD: u32 = 6;
pub const VOXEL_TYPE_ROCK: u32 = 7;
pub const VOXEL_TYPE_EMPTY: u32 = 0;
pub const VOXEL_TYPE_DIRT: u32 = 2;
pub const VOXEL_TYPE_SAND: u32 = 3;
const PRIMITIVE_KIND_ROUND_CONE: u32 = 0;
const PRIMITIVE_KIND_CUBOID: u32 = 1;
const PRIMITIVE_KIND_SPHERE: u32 = 2;
pub const EDIT_STATS_VOXEL_TYPE_COUNT: usize = 8;
pub(crate) const EDIT_REMOVAL_CANDIDATE_CAPACITY: u64 = 65_536;
pub(crate) const CHUNK_SOLID_SAMPLE_CAPACITY: u64 = 65_536;
const EDIT_REMOVAL_SAMPLE_COUNT: usize = 50;

#[derive(Clone, Copy, Debug, Default)]
pub struct ChunkModifyStats {
    pub removed_counts: [u32; EDIT_STATS_VOXEL_TYPE_COUNT],
    pub added_counts: [u32; EDIT_STATS_VOXEL_TYPE_COUNT],
}

impl ChunkModifyStats {
    pub fn count_removed(&self, voxel_type: u32) -> u32 {
        self.removed_counts
            .get(voxel_type as usize)
            .copied()
            .unwrap_or(0)
    }

    pub fn count_added(&self, voxel_type: u32) -> u32 {
        self.added_counts
            .get(voxel_type as usize)
            .copied()
            .unwrap_or(0)
    }
}

#[derive(Debug, Default)]
pub struct ChunkModifyReadback {
    pub stats: ChunkModifyStats,
    #[allow(dead_code)]
    pub sampled_positions_world: Vec<Vec3>,
}

pub struct ChunkSolidSampleJob {
    atlas_offset: UVec3,
    atlas_dim: UVec3,
    sample_dim: UVec3,
    sample_count: u64,
    byte_count: u64,
    total_start: Instant,
    submitted_at: Instant,
    prepare_elapsed: Duration,
    submit_elapsed: Duration,
    _command_buffer: CommandBuffer,
    fence: Fence,
}

impl ChunkSolidSampleJob {
    pub fn atlas_offset(&self) -> UVec3 {
        self.atlas_offset
    }

    pub fn atlas_dim(&self) -> UVec3 {
        self.atlas_dim
    }

    pub fn sample_dim(&self) -> UVec3 {
        self.sample_dim
    }

    pub fn sample_count(&self) -> u64 {
        self.sample_count
    }

    pub fn byte_count(&self) -> u64 {
        self.byte_count
    }
}

#[derive(Debug)]
pub struct ChunkSolidSampleResult {
    pub atlas_offset: UVec3,
    pub atlas_dim: UVec3,
    pub sample_dim: UVec3,
    pub sample_count: u64,
    pub byte_count: u64,
    pub samples: Vec<u32>,
    pub prepare_ms: f64,
    pub gpu_submit_ms: f64,
    pub fence_latency_ms: f64,
    pub readback_ms: f64,
    pub convert_ms: f64,
    pub total_ms: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct EditRemovalSampleReadback {
    sample_count: u32,
    _pad0: [u32; 3],
    positions: [[f32; 4]; EDIT_REMOVAL_SAMPLE_COUNT],
}

pub struct PlainBuilder {
    vulkan_ctx: VulkanContext,
    resources: PlainBuilderResources,
    plain_atlas_dim: UVec3,

    #[allow(dead_code)]
    buffer_setup_ppl: ComputePipeline,
    #[allow(dead_code)]
    chunk_init_ppl: ComputePipeline,
    heightmap_ppl: ComputePipeline,
    chunk_modify_ppl: ComputePipeline,
    chunk_modify_sample_ppl: ComputePipeline,
    chunk_solid_sample_ppl: ComputePipeline,
    model_voxelize_ppl: ComputePipeline,

    #[allow(dead_code)]
    pool: DescriptorPool,

    build_cmdbuf: CommandBuffer,
    next_edit_sample_seed: u32,
    #[allow(dead_code)]
    chunk_atlas_readback_buffer: Option<Buffer>,
}

impl PlainBuilder {
    pub fn new(
        vulkan_ctx: VulkanContext,
        shader_compiler: &ShaderCompiler,
        allocator: Allocator,
        plain_atlas_dim: UVec3,
        free_atlas_dim: UVec3,
    ) -> Self {
        let device = vulkan_ctx.device();

        let buffer_setup_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/chunk_writer/buffer_setup.comp",
            "main",
        )
        .unwrap();
        let chunk_init_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/chunk_writer/chunk_init.comp",
            "main",
        )
        .unwrap();
        let chunk_modify_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/chunk_writer/chunk_modify.comp",
            "main",
        )
        .unwrap();
        let chunk_modify_sample_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/chunk_writer/chunk_modify_sample.comp",
            "main",
        )
        .unwrap();
        let chunk_solid_sample_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/chunk_writer/chunk_solid_sample.comp",
            "main",
        )
        .unwrap();
        let model_voxelize_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/chunk_writer/model_voxelize.comp",
            "main",
        )
        .unwrap();
        let heightmap_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/chunk_writer/chunk_heightmap.comp",
            "main",
        )
        .unwrap();

        let resources = PlainBuilderResources::new(
            device,
            allocator.clone(),
            plain_atlas_dim,
            free_atlas_dim,
            &buffer_setup_sm,
            &chunk_modify_sm,
            &chunk_modify_sample_sm,
            &chunk_solid_sample_sm,
            &model_voxelize_sm,
            &heightmap_sm,
        );

        let pool = DescriptorPool::new(device).unwrap();

        let buffer_setup_ppl = ComputePipeline::new(device, &buffer_setup_sm, &pool, &[&resources]);
        let chunk_init_ppl = ComputePipeline::new(device, &chunk_init_sm, &pool, &[&resources]);
        let heightmap_ppl = ComputePipeline::new(device, &heightmap_sm, &pool, &[&resources]);
        let chunk_modify_ppl = ComputePipeline::new(device, &chunk_modify_sm, &pool, &[&resources]);
        let chunk_modify_sample_ppl =
            ComputePipeline::new(device, &chunk_modify_sample_sm, &pool, &[&resources]);
        let chunk_solid_sample_ppl =
            ComputePipeline::new(device, &chunk_solid_sample_sm, &pool, &[&resources]);
        let model_voxelize_ppl =
            ComputePipeline::new(device, &model_voxelize_sm, &pool, &[&resources]);

        init_atlas_images(&vulkan_ctx, &resources);

        let build_cmdbuf = Self::record_build_cmdbuf(
            &vulkan_ctx,
            &resources.chunk_atlas,
            &resources.region_indirect,
            &heightmap_ppl,
            &buffer_setup_ppl,
            &chunk_init_ppl,
            plain_atlas_dim,
        );

        return Self {
            vulkan_ctx,
            resources,
            plain_atlas_dim,
            buffer_setup_ppl,
            chunk_init_ppl,
            heightmap_ppl,
            chunk_modify_ppl,
            chunk_modify_sample_ppl,
            chunk_solid_sample_ppl,
            model_voxelize_ppl,
            pool,
            build_cmdbuf,
            next_edit_sample_seed: 1,
            chunk_atlas_readback_buffer: None,
        };

        fn init_atlas_images(vulkan_context: &VulkanContext, resources: &PlainBuilderResources) {
            execute_one_time_command(
                vulkan_context.device(),
                vulkan_context.command_pool(),
                &vulkan_context.get_general_queue(),
                |cmdbuf| {
                    resources.chunk_atlas.get_image().record_clear(
                        cmdbuf,
                        Some(vk::ImageLayout::GENERAL),
                        0,
                        ClearValue::Color(ColorClearValue::UInt([0, 0, 0, 0])),
                    );
                    resources.free_atlas.get_image().record_clear(
                        cmdbuf,
                        Some(vk::ImageLayout::GENERAL),
                        0,
                        ClearValue::Color(ColorClearValue::UInt([0, 0, 0, 0])),
                    );
                    resources.solid_workgroup_flags.record_fill(
                        cmdbuf,
                        0,
                        resources.solid_workgroup_flags.get_size_bytes(),
                        0,
                    );
                },
            );
        }
    }

    fn record_build_cmdbuf(
        vulkan_ctx: &VulkanContext,
        chunk_atlas: &Texture,
        region_indirect: &Buffer,
        heightmap_ppl: &ComputePipeline,
        buffer_setup_ppl: &ComputePipeline,
        chunk_init_ppl: &ComputePipeline,
        dispatch_dim: UVec3,
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

        let cmdbuf = CommandBuffer::new(vulkan_ctx.device(), vulkan_ctx.command_pool());
        cmdbuf.begin(false);

        chunk_atlas
            .get_image()
            .record_transition_barrier(&cmdbuf, 0, vk::ImageLayout::GENERAL);

        heightmap_ppl.record(
            &cmdbuf,
            Extent3D {
                width: dispatch_dim.x,
                height: dispatch_dim.z,
                depth: 1,
            },
            None,
        );

        shader_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);

        buffer_setup_ppl.record(
            &cmdbuf,
            Extent3D {
                width: 1,
                height: 1,
                depth: 1,
            },
            None,
        );

        shader_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);
        indirect_access_pipeline_barrier.record_insert(vulkan_ctx.device(), &cmdbuf);

        chunk_init_ppl.record_indirect(&cmdbuf, region_indirect, None);

        cmdbuf.end();
        cmdbuf
    }

    pub fn get_resources(&self) -> &PlainBuilderResources {
        &self.resources
    }

    #[allow(dead_code)]
    fn ensure_chunk_atlas_readback_buffer(&mut self, byte_count: u64) {
        let current_size = self
            .chunk_atlas_readback_buffer
            .as_ref()
            .map(Buffer::get_size_bytes)
            .unwrap_or(0);
        if current_size >= byte_count {
            return;
        }

        self.chunk_atlas_readback_buffer = Some(Buffer::new_sized(
            self.vulkan_ctx.device().clone(),
            self.resources
                .chunk_atlas
                .get_image()
                .get_allocator()
                .clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::TRANSFER_DST),
            MemoryLocation::GpuToCpu,
            byte_count,
        ));
        log::info!(
            "[PLAIN_BUILDER] allocated reusable chunk atlas readback buffer size_bytes={} previous_size_bytes={}",
            byte_count,
            current_size,
        );
    }

    #[allow(dead_code)]
    pub fn read_chunk_atlas_region(
        &mut self,
        atlas_offset: UVec3,
        atlas_dim: UVec3,
    ) -> Result<Vec<u8>> {
        let atlas_extent = self.resources.chunk_atlas.get_image().get_desc().extent;
        let atlas_size = UVec3::new(atlas_extent.width, atlas_extent.height, atlas_extent.depth);
        if atlas_dim.x == 0 || atlas_dim.y == 0 || atlas_dim.z == 0 {
            return Ok(Vec::new());
        }
        if (atlas_offset + atlas_dim).cmpgt(atlas_size).any() {
            anyhow::bail!(
                "chunk atlas read outside bounds: offset={:?} dim={:?} atlas={:?}",
                atlas_offset,
                atlas_dim,
                atlas_size
            );
        }

        let byte_count = atlas_dim.x as u64 * atlas_dim.y as u64 * atlas_dim.z as u64;
        self.ensure_chunk_atlas_readback_buffer(byte_count);

        let queue = self.vulkan_ctx.get_general_queue();
        let command_pool = self.vulkan_ctx.command_pool();
        let chunk_atlas = self.resources.chunk_atlas.get_image();
        let buffer = self
            .chunk_atlas_readback_buffer
            .as_mut()
            .expect("chunk atlas readback buffer should be allocated");
        chunk_atlas.copy_image_to_buffer(
            buffer,
            &queue,
            command_pool,
            vk::ImageLayout::GENERAL,
            0,
            TextureRegion {
                offset: [
                    atlas_offset.x as i32,
                    atlas_offset.y as i32,
                    atlas_offset.z as i32,
                ],
                extent: Extent3D::new(atlas_dim.x, atlas_dim.y, atlas_dim.z),
            },
        );
        buffer.read_back_range(0, byte_count)
    }

    #[allow(dead_code)]
    pub fn sample_chunk_atlas_solid_grid(
        &mut self,
        atlas_offset: UVec3,
        atlas_dim: UVec3,
        sample_dim: UVec3,
    ) -> Result<Vec<u32>> {
        let job = self.submit_chunk_atlas_solid_grid_sample(atlas_offset, atlas_dim, sample_dim)?;
        let wait_start = Instant::now();
        job.fence.wait()?;
        let wait_elapsed = wait_start.elapsed();
        crate::util::BENCH.lock().unwrap().record(
            "chunk_solid_sample_gpu_dispatch",
            wait_elapsed + job.submit_elapsed,
        );
        let result = self.finish_chunk_atlas_solid_grid_sample(job)?;
        log::info!(
            "[PLAIN_BUILDER][CHUNK_SOLID_SAMPLE] atlas_offset={:?} source_dim={:?} sample_dim={:?} samples={} readback_bytes={} gpu_submit={:.3}ms fence_wait={:.3}ms readback={:.3}ms convert={:.3}ms total={:.3}ms",
            result.atlas_offset,
            result.atlas_dim,
            result.sample_dim,
            result.sample_count,
            result.byte_count,
            result.gpu_submit_ms,
            wait_elapsed.as_secs_f64() * 1000.0,
            result.readback_ms,
            result.convert_ms,
            result.total_ms,
        );
        Ok(result.samples)
    }

    pub fn submit_chunk_atlas_solid_grid_sample(
        &mut self,
        atlas_offset: UVec3,
        atlas_dim: UVec3,
        sample_dim: UVec3,
    ) -> Result<ChunkSolidSampleJob> {
        let total_start = Instant::now();
        let atlas_extent = self.resources.chunk_atlas.get_image().get_desc().extent;
        let atlas_size = UVec3::new(atlas_extent.width, atlas_extent.height, atlas_extent.depth);
        if atlas_dim.x == 0 || atlas_dim.y == 0 || atlas_dim.z == 0 {
            anyhow::bail!("chunk solid sample source dim must be non-zero: {atlas_dim:?}");
        }
        if sample_dim.x < 2 || sample_dim.y < 2 || sample_dim.z < 2 {
            anyhow::bail!(
                "chunk solid sample dim must be at least 2 in every axis: {sample_dim:?}"
            );
        }
        if (atlas_offset + atlas_dim).cmpgt(atlas_size).any() {
            anyhow::bail!(
                "chunk solid sample source outside atlas: offset={:?} dim={:?} atlas={:?}",
                atlas_offset,
                atlas_dim,
                atlas_size
            );
        }

        let sample_count = sample_dim.x as u64 * sample_dim.y as u64 * sample_dim.z as u64;
        if sample_count > CHUNK_SOLID_SAMPLE_CAPACITY {
            anyhow::bail!(
                "chunk solid sample dim {:?} has {} samples, but capacity is {}",
                sample_dim,
                sample_count,
                CHUNK_SOLID_SAMPLE_CAPACITY
            );
        }
        let byte_count = sample_count * std::mem::size_of::<u32>() as u64;

        let prepare_start = Instant::now();
        self.resources
            .chunk_solid_sample_info
            .fill_uniform(&ChunkSolidSampleInfo {
                atlas_offset: atlas_offset.to_array(),
                atlas_dim: atlas_dim.to_array(),
                sample_dim: sample_dim.to_array(),
                ..ChunkSolidSampleInfo::zeroed()
            })?;
        let prepare_elapsed = prepare_start.elapsed();
        crate::util::BENCH
            .lock()
            .unwrap()
            .record("chunk_solid_sample_prepare", prepare_elapsed);

        let host_read_barrier = PipelineBarrier::new(
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::HOST,
            vec![MemoryBarrier::new(
                vk::AccessFlags::SHADER_WRITE,
                vk::AccessFlags::HOST_READ,
            )],
        );
        let command_buffer =
            CommandBuffer::new(self.vulkan_ctx.device(), self.vulkan_ctx.command_pool());
        let fence = Fence::new(self.vulkan_ctx.device(), false);
        let submit_start = Instant::now();
        command_buffer.begin(true);
        self.chunk_solid_sample_ppl.record(
            &command_buffer,
            Extent3D::new(sample_dim.x, sample_dim.y, sample_dim.z),
            None,
        );
        host_read_barrier.record_insert(self.vulkan_ctx.device(), &command_buffer);
        command_buffer.end();
        command_buffer.submit(&self.vulkan_ctx.get_general_queue(), Some(&fence));
        let submitted_at = Instant::now();
        let submit_elapsed = submit_start.elapsed();
        crate::util::BENCH
            .lock()
            .unwrap()
            .record("chunk_solid_sample_gpu_submit", submit_elapsed);

        Ok(ChunkSolidSampleJob {
            atlas_offset,
            atlas_dim,
            sample_dim,
            sample_count,
            byte_count,
            total_start,
            submitted_at,
            prepare_elapsed,
            submit_elapsed,
            _command_buffer: command_buffer,
            fence,
        })
    }

    pub fn chunk_atlas_solid_grid_sample_ready(&self, job: &ChunkSolidSampleJob) -> Result<bool> {
        job.fence
            .is_signaled()
            .map_err(|err| anyhow::anyhow!("failed to poll chunk solid sample fence: {err}"))
    }

    pub fn finish_chunk_atlas_solid_grid_sample(
        &mut self,
        job: ChunkSolidSampleJob,
    ) -> Result<ChunkSolidSampleResult> {
        let fence_latency_elapsed = job.submitted_at.elapsed();
        let readback_start = Instant::now();
        let raw = self
            .resources
            .chunk_solid_samples
            .read_back_range(0, job.byte_count)?;
        let readback_elapsed = readback_start.elapsed();
        crate::util::BENCH
            .lock()
            .unwrap()
            .record("chunk_solid_sample_readback", readback_elapsed);

        let convert_start = Instant::now();
        let samples = raw
            .chunks_exact(std::mem::size_of::<u32>())
            .map(|chunk| u32::from_ne_bytes(chunk.try_into().unwrap()))
            .collect::<Vec<_>>();
        let convert_elapsed = convert_start.elapsed();
        crate::util::BENCH
            .lock()
            .unwrap()
            .record("chunk_solid_sample_convert", convert_elapsed);
        let total_elapsed = job.total_start.elapsed();
        crate::util::BENCH
            .lock()
            .unwrap()
            .record("chunk_solid_sample_total", total_elapsed);

        Ok(ChunkSolidSampleResult {
            atlas_offset: job.atlas_offset,
            atlas_dim: job.atlas_dim,
            sample_dim: job.sample_dim,
            sample_count: job.sample_count,
            byte_count: job.byte_count,
            samples,
            prepare_ms: job.prepare_elapsed.as_secs_f64() * 1000.0,
            gpu_submit_ms: job.submit_elapsed.as_secs_f64() * 1000.0,
            fence_latency_ms: fence_latency_elapsed.as_secs_f64() * 1000.0,
            readback_ms: readback_elapsed.as_secs_f64() * 1000.0,
            convert_ms: convert_elapsed.as_secs_f64() * 1000.0,
            total_ms: total_elapsed.as_secs_f64() * 1000.0,
        })
    }

    pub fn chunk_init(&mut self, atlas_offset: UVec3, atlas_dim: UVec3) -> Result<()> {
        if atlas_dim.x == 0 || atlas_dim.y == 0 || atlas_dim.z == 0 {
            return Ok(());
        }
        update_buffers(&self.resources, atlas_offset, atlas_dim)?;

        // re-record the command buffer with updated descriptor sets
        self.build_cmdbuf = Self::record_build_cmdbuf(
            &self.vulkan_ctx,
            &self.resources.chunk_atlas,
            &self.resources.region_indirect,
            &self.heightmap_ppl,
            &self.buffer_setup_ppl,
            &self.chunk_init_ppl,
            atlas_dim,
        );

        self.build_cmdbuf
            .submit(&self.vulkan_ctx.get_general_queue(), None);
        self.vulkan_ctx
            .device()
            .wait_queue_idle(&self.vulkan_ctx.get_general_queue());
        return Ok(());

        fn update_buffers(
            resources: &PlainBuilderResources,
            offset: UVec3,
            dim: UVec3,
        ) -> Result<()> {
            resources.region_info.fill_uniform(&RegionInfo {
                offset: offset.to_array(),
                dim: dim.to_array(),
                ..RegionInfo::zeroed()
            })
        }
    }

    pub fn chunk_modify(&mut self, bvh_nodes: &[BvhNode], round_cones: &[RoundCone]) -> Result<()> {
        self.chunk_modify_with_voxel_type(bvh_nodes, round_cones, VOXEL_TYPE_CHERRY_WOOD)
    }

    pub fn chunk_modify_with_voxel_type(
        &mut self,
        bvh_nodes: &[BvhNode],
        round_cones: &[RoundCone],
        fill_voxel_type: u32,
    ) -> Result<()> {
        self.chunk_modify_round_cones_with_voxel_type(bvh_nodes, round_cones, fill_voxel_type)
    }

    pub fn chunk_modify_cuboids(
        &mut self,
        bvh_nodes: &[BvhNode],
        cuboids: &[Cuboid],
    ) -> Result<()> {
        self.chunk_modify_cuboids_with_voxel_type(bvh_nodes, cuboids, VOXEL_TYPE_CHERRY_WOOD)
    }

    pub fn chunk_modify_cuboids_with_voxel_type(
        &mut self,
        bvh_nodes: &[BvhNode],
        cuboids: &[Cuboid],
        fill_voxel_type: u32,
    ) -> Result<()> {
        self.chunk_modify_cuboids_with_voxel_type_impl(bvh_nodes, cuboids, fill_voxel_type)
    }

    pub fn chunk_modify_surface_spheres_with_voxel_type(
        &mut self,
        bvh_nodes: &[BvhNode],
        spheres: &[Sphere],
        fill_voxel_type: u32,
        target_voxel_type: Option<u32>,
        max_write_count: Option<u32>,
        max_removed_counts: Option<[u32; EDIT_STATS_VOXEL_TYPE_COUNT]>,
    ) -> Result<ChunkModifyReadback> {
        let total_start = Instant::now();
        let atlas_dim = chunk_atlas_dim(&self.resources);
        let Some((offset, dim)) = calculate_clipped_offset_and_dim(bvh_nodes, atlas_dim) else {
            return Ok(ChunkModifyReadback::default());
        };
        let prep_start = Instant::now();
        clear_edit_stats(&self.resources)?;
        clear_edit_removal_candidates(&self.resources)?;
        update_chunk_modify_info(
            &self.resources,
            offset,
            dim,
            fill_voxel_type,
            target_voxel_type,
            PRIMITIVE_KIND_SPHERE,
            true,
            max_write_count,
            max_removed_counts,
        )?;
        update_spheres(&self.resources, spheres)?;
        update_trunk_bvh_nodes(&self.resources, bvh_nodes)?;
        crate::util::BENCH
            .lock()
            .unwrap()
            .record("terrain_edit_prepare_buffers", prep_start.elapsed());
        let sample_push = PushConstantChunkModifySample {
            edit_seed: self.next_edit_sample_seed,
            _pad0: [0; 12],
        };
        self.next_edit_sample_seed = self.next_edit_sample_seed.wrapping_add(1);
        let shader_access_pipeline_barrier = PipelineBarrier::new(
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vec![MemoryBarrier::new_shader_access()],
        );

        let gpu_start = Instant::now();
        execute_one_time_command_with_fence(
            self.vulkan_ctx.device(),
            self.vulkan_ctx.command_pool(),
            &self.vulkan_ctx.get_general_queue(),
            |cmdbuf| {
                self.chunk_modify_ppl.record(
                    cmdbuf,
                    Extent3D {
                        width: dim.x,
                        height: dim.y,
                        depth: dim.z,
                    },
                    None,
                );
                shader_access_pipeline_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
                self.chunk_modify_sample_ppl.record(
                    cmdbuf,
                    Extent3D::new(EDIT_REMOVAL_SAMPLE_COUNT as u32, 1, 1),
                    Some(bytemuck::bytes_of(&sample_push)),
                );
            },
        );
        crate::util::BENCH
            .lock()
            .unwrap()
            .record("terrain_edit_gpu_modify_and_sample", gpu_start.elapsed());

        let stats_start = Instant::now();
        let stats = read_edit_stats(&self.resources)?;
        crate::util::BENCH
            .lock()
            .unwrap()
            .record("terrain_edit_stats_readback", stats_start.elapsed());

        let sample_start = Instant::now();
        let sampled_positions_world = read_edit_removal_sample(&self.resources)?;
        crate::util::BENCH
            .lock()
            .unwrap()
            .record("terrain_edit_sample_readback", sample_start.elapsed());

        crate::util::BENCH
            .lock()
            .unwrap()
            .record("terrain_edit_plain_builder_total", total_start.elapsed());
        Ok(ChunkModifyReadback {
            stats,
            sampled_positions_world,
        })
    }

    pub fn voxelize_model(
        &mut self,
        triangles: &[ModelTriangleGpu],
        position: Vec3,
        fill_voxel_type: u32,
    ) -> Result<UAabb3> {
        let total_start = Instant::now();
        if triangles.is_empty() {
            return Err(anyhow::anyhow!("cannot voxelize a model with no triangles"));
        }

        let triangle_vec4s_len = triangles.len() * 3;
        let max_triangle_vec4s = self.resources.model_triangles.get_size_bytes() as usize
            / std::mem::size_of::<[f32; 4]>();
        if triangle_vec4s_len > max_triangle_vec4s {
            return Err(anyhow::anyhow!(
                "model has {} triangle vec4s, but the upload buffer only holds {}",
                triangle_vec4s_len,
                max_triangle_vec4s
            ));
        }

        let (offset, dim, rebuild_bound) = calculate_model_voxel_bounds(
            triangles,
            position,
            self.plain_atlas_dim,
            MODEL_VOXELIZE_SURFACE_THICKNESS_VOX,
        )?;
        let voxels = dim.x as u64 * dim.y as u64 * dim.z as u64;

        let upload_start = Instant::now();
        let mut triangle_vec4s = Vec::with_capacity(triangle_vec4s_len);
        for triangle in triangles {
            triangle_vec4s.push(triangle.a);
            triangle_vec4s.push(triangle.b);
            triangle_vec4s.push(triangle.c);
        }

        self.resources.model_triangles.fill(&triangle_vec4s)?;
        self.resources
            .model_voxelize_info
            .fill_uniform(&ModelVoxelizeInfo {
                offset: offset.to_array(),
                triangle_count: triangles.len() as u32,
                dim: dim.to_array(),
                fill_voxel_type,
                position_vox: (position * 256.0).to_array(),
                surface_thickness_vox: MODEL_VOXELIZE_SURFACE_THICKNESS_VOX,
            })?;
        let upload_elapsed = upload_start.elapsed();

        let shader_access_pipeline_barrier = PipelineBarrier::new(
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vec![MemoryBarrier::new_shader_access()],
        );

        let gpu_start = Instant::now();
        execute_one_time_command(
            self.vulkan_ctx.device(),
            self.vulkan_ctx.command_pool(),
            &self.vulkan_ctx.get_general_queue(),
            |cmdbuf| {
                self.model_voxelize_ppl
                    .record(cmdbuf, Extent3D::new(dim.x, dim.y, dim.z), None);
                shader_access_pipeline_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            },
        );
        let gpu_elapsed = gpu_start.elapsed();

        log::info!(
            "[MODEL_VOXELIZE] approach=winding triangles={} dim={:?} voxels={} upload={:.3}ms gpu_dispatch_wait={:.3}ms total={:.3}ms",
            triangles.len(),
            dim,
            voxels,
            upload_elapsed.as_secs_f64() * 1000.0,
            gpu_elapsed.as_secs_f64() * 1000.0,
            total_start.elapsed().as_secs_f64() * 1000.0,
        );

        Ok(rebuild_bound)
    }

    fn chunk_modify_round_cones_with_voxel_type(
        &mut self,
        bvh_nodes: &[BvhNode],
        round_cones: &[RoundCone],
        fill_voxel_type: u32,
    ) -> Result<()> {
        let atlas_dim = chunk_atlas_dim(&self.resources);
        let Some((offset, dim)) = calculate_clipped_offset_and_dim(bvh_nodes, atlas_dim) else {
            return Ok(());
        };
        update_chunk_modify_info(
            &self.resources,
            offset,
            dim,
            fill_voxel_type,
            None,
            PRIMITIVE_KIND_ROUND_CONE,
            false,
            None,
            None,
        )?;
        update_round_cones(&self.resources, round_cones)?;
        update_trunk_bvh_nodes(&self.resources, bvh_nodes)?;
        log::info!(
            "[TREE_DEBUG] round_cone_dispatch cones={} offset={:?} dim={:?} voxels={}",
            round_cones.len(),
            offset,
            dim,
            dim.x as u64 * dim.y as u64 * dim.z as u64,
        );

        execute_one_time_command(
            self.vulkan_ctx.device(),
            self.vulkan_ctx.command_pool(),
            &self.vulkan_ctx.get_general_queue(),
            |cmdbuf| {
                self.chunk_modify_ppl.record(
                    cmdbuf,
                    Extent3D {
                        width: dim.x,
                        height: dim.y,
                        depth: dim.z,
                    },
                    None,
                );
            },
        );
        Ok(())
    }

    fn chunk_modify_cuboids_with_voxel_type_impl(
        &mut self,
        bvh_nodes: &[BvhNode],
        cuboids: &[Cuboid],
        fill_voxel_type: u32,
    ) -> Result<()> {
        let atlas_dim = chunk_atlas_dim(&self.resources);
        let Some((offset, dim)) = calculate_clipped_offset_and_dim(bvh_nodes, atlas_dim) else {
            return Ok(());
        };
        update_chunk_modify_info(
            &self.resources,
            offset,
            dim,
            fill_voxel_type,
            None,
            PRIMITIVE_KIND_CUBOID,
            false,
            None,
            None,
        )?;
        update_cuboids(&self.resources, cuboids)?;
        update_trunk_bvh_nodes(&self.resources, bvh_nodes)?;

        execute_one_time_command(
            self.vulkan_ctx.device(),
            self.vulkan_ctx.command_pool(),
            &self.vulkan_ctx.get_general_queue(),
            |cmdbuf| {
                self.chunk_modify_ppl.record(
                    cmdbuf,
                    Extent3D {
                        width: dim.x,
                        height: dim.y,
                        depth: dim.z,
                    },
                    None,
                );
            },
        );
        Ok(())
    }
}

const MODEL_VOXELIZE_SURFACE_THICKNESS_VOX: f32 = 0.75;

fn calculate_model_voxel_bounds(
    triangles: &[ModelTriangleGpu],
    position: Vec3,
    atlas_dim: UVec3,
    surface_thickness_vox: f32,
) -> Result<(UVec3, UVec3, UAabb3)> {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for triangle in triangles {
        for point in [triangle.a, triangle.b, triangle.c] {
            let world = Vec3::new(point[0], point[1], point[2]) + position;
            min = min.min(world);
            max = max.max(world);
        }
    }

    if !min.is_finite() || !max.is_finite() {
        return Err(anyhow::anyhow!("model bounds contain non-finite values"));
    }

    let pad = surface_thickness_vox + 1.0;
    let min_vox = (min * 256.0 - Vec3::splat(pad)).floor().as_ivec3();
    let max_vox_exclusive = (max * 256.0 + Vec3::splat(pad)).ceil().as_ivec3() + IVec3::ONE;
    let atlas_max = atlas_dim.as_ivec3();
    let clamped_min = min_vox.clamp(IVec3::ZERO, atlas_max);
    let clamped_max = max_vox_exclusive.clamp(IVec3::ZERO, atlas_max);

    if any_ivec3_less_equal(clamped_max, clamped_min) {
        return Err(anyhow::anyhow!(
            "model voxel bounds are outside the atlas: min={:?}, max={:?}, atlas={:?}",
            min_vox,
            max_vox_exclusive,
            atlas_dim
        ));
    }

    let offset = clamped_min.as_uvec3();
    let max_exclusive = clamped_max.as_uvec3();
    let dim = max_exclusive - offset;
    Ok((offset, dim, UAabb3::new(offset, max_exclusive)))
}

fn any_ivec3_less_equal(a: IVec3, b: IVec3) -> bool {
    a.x <= b.x || a.y <= b.y || a.z <= b.z
}

fn chunk_atlas_dim(resources: &PlainBuilderResources) -> UVec3 {
    let extent = resources.chunk_atlas.get_image().get_desc().extent;
    UVec3::new(extent.width, extent.height, extent.depth)
}

fn calculate_clipped_offset_and_dim(
    bvh_nodes: &[BvhNode],
    atlas_dim: UVec3,
) -> Option<(UVec3, UVec3)> {
    let root_node = &bvh_nodes[0];
    let atlas_max = atlas_dim.as_ivec3();
    let min_vox = root_node.aabb.min().floor().as_ivec3();
    let max_vox = root_node.aabb.max().ceil().as_ivec3();
    let clamped_min = min_vox.clamp(IVec3::ZERO, atlas_max);
    let clamped_max = max_vox.clamp(IVec3::ZERO, atlas_max);

    if min_vox != clamped_min || max_vox != clamped_max {
        log::warn!(
            "voxel edit bounds clipped to atlas: requested min={:?} max={:?}, clipped min={:?} max={:?}, atlas_dim={:?}",
            min_vox,
            max_vox,
            clamped_min,
            clamped_max,
            atlas_dim,
        );
    }

    if any_ivec3_less_equal(clamped_max, clamped_min) {
        log::warn!(
            "voxel edit skipped outside atlas: requested min={:?} max={:?}, atlas_dim={:?}",
            min_vox,
            max_vox,
            atlas_dim,
        );
        return None;
    }

    let offset = clamped_min.as_uvec3();
    let dim = clamped_max.as_uvec3() - offset;
    Some((offset, dim))
}

#[allow(clippy::too_many_arguments)]
fn update_chunk_modify_info(
    resources: &PlainBuilderResources,
    offset: UVec3,
    dim: UVec3,
    fill_voxel_type: u32,
    target_voxel_type: Option<u32>,
    primitive_kind: u32,
    surface_only: bool,
    max_write_count: Option<u32>,
    max_removed_counts: Option<[u32; EDIT_STATS_VOXEL_TYPE_COUNT]>,
) -> Result<()> {
    let max_removed_counts = max_removed_counts.unwrap_or([u32::MAX; EDIT_STATS_VOXEL_TYPE_COUNT]);
    resources.chunk_modify_info.fill_uniform(&ChunkModifyInfo {
        offset: offset.to_array(),
        dim: dim.to_array(),
        fill_voxel_type,
        target_voxel_type: target_voxel_type.unwrap_or(u32::MAX),
        primitive_kind,
        surface_only: if surface_only { 1 } else { 0 },
        max_write_count: max_write_count.unwrap_or(0),
        max_removed_counts_0_3: max_removed_counts[..4].try_into().unwrap(),
        max_removed_counts_4_7: max_removed_counts[4..].try_into().unwrap(),
        ..ChunkModifyInfo::zeroed()
    })
}

fn clear_edit_stats(resources: &PlainBuilderResources) -> Result<()> {
    resources
        .edit_stats
        .fill_with_raw_u32(&[0; EDIT_STATS_VOXEL_TYPE_COUNT * 2])
}

fn clear_edit_removal_candidates(resources: &PlainBuilderResources) -> Result<()> {
    resources.edit_removal_candidates.fill_with_raw_u8(&vec![
        0;
        resources.edit_removal_candidates.get_size_bytes()
            as usize
    ])
}

fn read_edit_stats(resources: &PlainBuilderResources) -> Result<ChunkModifyStats> {
    let raw = resources.edit_stats.read_back()?;
    let expected_len = EDIT_STATS_VOXEL_TYPE_COUNT * 2 * std::mem::size_of::<u32>();
    if raw.len() < expected_len {
        return Err(anyhow::anyhow!(
            "Edit stats buffer too small: got {}, need {}",
            raw.len(),
            expected_len
        ));
    }

    let mut values = [0u32; EDIT_STATS_VOXEL_TYPE_COUNT * 2];
    for (idx, chunk) in raw
        .chunks_exact(std::mem::size_of::<u32>())
        .take(EDIT_STATS_VOXEL_TYPE_COUNT * 2)
        .enumerate()
    {
        values[idx] = u32::from_ne_bytes(chunk.try_into().unwrap());
    }

    let mut removed_counts = [0u32; EDIT_STATS_VOXEL_TYPE_COUNT];
    removed_counts.copy_from_slice(&values[..EDIT_STATS_VOXEL_TYPE_COUNT]);
    let mut added_counts = [0u32; EDIT_STATS_VOXEL_TYPE_COUNT];
    added_counts.copy_from_slice(&values[EDIT_STATS_VOXEL_TYPE_COUNT..]);

    Ok(ChunkModifyStats {
        removed_counts,
        added_counts,
    })
}

fn read_edit_removal_sample(resources: &PlainBuilderResources) -> Result<Vec<Vec3>> {
    let raw = resources.edit_removal_sample.read_back()?;
    let readback = bytemuck::try_from_bytes::<EditRemovalSampleReadback>(&raw)
        .map_err(|err| anyhow::anyhow!("invalid edit removal sample readback: {err}"))?;
    let sample_count = (readback.sample_count as usize).min(EDIT_REMOVAL_SAMPLE_COUNT);
    let mut positions = Vec::with_capacity(sample_count);
    for item in readback.positions.iter().take(sample_count) {
        positions.push(Vec3::new(item[0], item[1], item[2]));
    }
    Ok(positions)
}

fn update_round_cones(resources: &PlainBuilderResources, round_cones: &[RoundCone]) -> Result<()> {
    for (i, round_cone) in round_cones.iter().enumerate() {
        let data = RoundCones {
            center_a: round_cone.center_a().to_array(),
            center_b: round_cone.center_b().to_array(),
            radius_a: round_cone.radius_a(),
            radius_b: round_cone.radius_b(),
        };
        resources
            .round_cones
            .fill_element_with_raw_u8(bytemuck::bytes_of(&data), i as u64)?;
    }
    Ok(())
}

fn update_cuboids(resources: &PlainBuilderResources, cuboids: &[Cuboid]) -> Result<()> {
    for (i, cuboid) in cuboids.iter().enumerate() {
        let data = Cuboids {
            min_corner: cuboid.min().to_array(),
            max_corner: cuboid.max().to_array(),
            ..Cuboids::zeroed()
        };
        resources
            .cuboids
            .fill_element_with_raw_u8(bytemuck::bytes_of(&data), i as u64)?;
    }
    Ok(())
}

fn update_spheres(resources: &PlainBuilderResources, spheres: &[Sphere]) -> Result<()> {
    for (i, sphere) in spheres.iter().enumerate() {
        let data = Spheres {
            center: sphere.center().to_array(),
            radius: sphere.radius(),
        };
        resources
            .spheres
            .fill_element_with_raw_u8(bytemuck::bytes_of(&data), i as u64)?;
    }
    Ok(())
}

fn update_trunk_bvh_nodes(resources: &PlainBuilderResources, bvh_nodes: &[BvhNode]) -> Result<()> {
    for (i, bvh_node) in bvh_nodes.iter().enumerate() {
        let combined_offset: u32 = if bvh_node.is_leaf {
            let primitive_idx = bvh_node.data_offset;
            0x8000_0000 | primitive_idx
        } else {
            bvh_node.left
        };
        let data = BvhNodes {
            aabb_min: bvh_node.aabb.min().to_array(),
            aabb_max: bvh_node.aabb.max().to_array(),
            offset: combined_offset,
            ..BvhNodes::zeroed()
        };
        resources
            .trunk_bvh_nodes
            .fill_element_with_raw_u8(bytemuck::bytes_of(&data), i as u64)?;
    }
    Ok(())
}
