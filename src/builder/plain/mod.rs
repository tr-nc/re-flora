mod resources;
use crate::generated::gpu_structs::{
    BvhNodes, ChunkModifyInfo, ChunkSolidSampleInfo, Cuboids, ModelVoxelizeInfo,
    PushConstantChunkModifySample, RegionInfo, RoundCones, Spheres,
};
use crate::geom::{BvhNode, Cuboid, RoundCone, Sphere, UAabb3};
use crate::util::ShaderCompiler;
use anyhow::Result;
use bytemuck::{Pod, Zeroable};
use glam::{IVec3, UVec3, Vec3};
pub use resources::*;
use std::collections::VecDeque;
use std::convert::TryInto;
use std::time::{Duration, Instant};
use verdarium_vkn::execute_one_time_command;
use verdarium_vkn::execute_one_time_gpu_job;
use verdarium_vkn::vk;
use verdarium_vkn::Allocator;
use verdarium_vkn::Buffer;
use verdarium_vkn::BufferUsage;
use verdarium_vkn::ClearValue;
use verdarium_vkn::ColorClearValue;
use verdarium_vkn::CommandBuffer;
use verdarium_vkn::ComputePipeline;
use verdarium_vkn::DescriptorPool;
use verdarium_vkn::Extent3D;
use verdarium_vkn::GpuJobToken;
use verdarium_vkn::MemoryLocation;
use verdarium_vkn::PipelineBarrier;
use verdarium_vkn::ShaderModule;
use verdarium_vkn::TextureLayout;
use verdarium_vkn::TextureRegion;
use verdarium_vkn::VulkanContext;

pub const VOXEL_TYPE_CHERRY_WOOD: u32 = 5;
pub const VOXEL_TYPE_OAK_WOOD: u32 = 6;
pub const VOXEL_TYPE_ROCK: u32 = 7;
pub const VOXEL_TYPE_EMPTY: u32 = 0;
pub const VOXEL_TYPE_DIRT: u32 = 2;
pub const VOXEL_TYPE_SAND: u32 = 3;
pub const VOXEL_TYPE_MASK: u8 = 0x0f;
pub const VOXEL_ATLAS_STATE_MASK: u8 = 0xf0;
// Moisture intentionally uses only two bits (4..5): 0=dry, 1..3=wetter.
// Bits 6..7 remain reserved for future packed soil state.
pub const VOXEL_MOISTURE_MASK: u8 = 0x30;
pub const VOXEL_MOISTURE_MAX: u8 = 0x03;
const PRIMITIVE_KIND_ROUND_CONE: u32 = 0;
const PRIMITIVE_KIND_CUBOID: u32 = 1;
const PRIMITIVE_KIND_SPHERE: u32 = 2;
pub const EDIT_STATS_VOXEL_TYPE_COUNT: usize = 8;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct TerrainMoistureDryPushConstants {
    offset: [u32; 4],
    dim: [u32; 4],
    dry_params: [f32; 4],
}

pub(crate) const EDIT_REMOVAL_CANDIDATE_CAPACITY: u64 = 65_536;
pub(crate) const CHUNK_SOLID_SAMPLE_CAPACITY: u64 = 65_536;
pub(crate) const TERRAIN_SMOOTH_MBO_HISTOGRAM_BINS: u32 = 1024;
pub(crate) const TERRAIN_SMOOTH_MBO_MAX_DIM: u32 = 224;
pub(crate) const TERRAIN_SMOOTH_MBO_CELL_CAPACITY: u64 = (TERRAIN_SMOOTH_MBO_MAX_DIM as u64)
    * (TERRAIN_SMOOTH_MBO_MAX_DIM as u64)
    * (TERRAIN_SMOOTH_MBO_MAX_DIM as u64);
const EDIT_REMOVAL_SAMPLE_COUNT: usize = 50;

fn voxel_type_from_atlas_byte(voxel_data: u8) -> u8 {
    voxel_data & VOXEL_TYPE_MASK
}

fn pack_voxel_atlas_byte(voxel_type: u8, moisture: u8) -> u8 {
    (voxel_type & VOXEL_TYPE_MASK) | (((moisture & VOXEL_MOISTURE_MAX) << 4) & VOXEL_MOISTURE_MASK)
}

fn pack_voxel_atlas_byte_for_fill(old_voxel_data: u8, fill_voxel_type: u8) -> u8 {
    if fill_voxel_type == VOXEL_TYPE_DIRT as u8 || fill_voxel_type == VOXEL_TYPE_SAND as u8 {
        (old_voxel_data & VOXEL_ATLAS_STATE_MASK) | (fill_voxel_type & VOXEL_TYPE_MASK)
    } else {
        pack_voxel_atlas_byte(fill_voxel_type, 0)
    }
}

/// GPU-friendly triangle vertex for model voxelization.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ModelTriangleGpu {
    pub a: [f32; 4],
    pub b: [f32; 4],
    pub c: [f32; 4],
}

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
    gpu_job: GpuJobToken,
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
    pub gpu_completion_latency_ms: f64,
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

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct TerrainSmoothInfoGpu {
    offset: [u32; 3],
    _pad0: u32,
    dim: [u32; 3],
    _pad1: u32,
    center_xz_vox: [f32; 2],
    brush_radius_vox: f32,
    kernel_radius_vox: f32,
    strength: f32,
    max_delta_vox: f32,
    deadband_vox: f32,
    _pad2: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct TerrainSmoothMboInfoGpu {
    offset: [u32; 4],
    dim: [u32; 4],
    center_radius: [f32; 4],
    params: [f32; 4],
    threshold: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
struct TerrainSmoothMboResultGpu {
    counts: [u32; 4],
    counts_extra: [u32; 4],
    changed_min: [u32; 4],
    changed_max: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
struct TerrainMoistureBrushInfoGpu {
    offset: [u32; 4],
    dim: [u32; 4],
    start_radius: [f32; 4],
    end_amount: [f32; 4],
}

pub struct PlainBuilder {
    vulkan_ctx: VulkanContext,
    resources: PlainBuilderResources,
    #[allow(dead_code)]
    plain_atlas_dim: UVec3,

    #[allow(dead_code)]
    buffer_setup_ppl: ComputePipeline,
    #[allow(dead_code)]
    chunk_init_ppl: ComputePipeline,
    heightmap_ppl: ComputePipeline,
    #[allow(dead_code)]
    terrain_smooth_heights_ppl: ComputePipeline,
    #[allow(dead_code)]
    terrain_smooth_target_ppl: ComputePipeline,
    #[allow(dead_code)]
    terrain_smooth_apply_ppl: ComputePipeline,
    terrain_smooth_mbo_init_ppl: ComputePipeline,
    terrain_smooth_mbo_diffuse_ab_ppl: ComputePipeline,
    terrain_smooth_mbo_diffuse_ba_ppl: ComputePipeline,
    terrain_smooth_mbo_score_ppl: ComputePipeline,
    terrain_smooth_mbo_apply_ppl: ComputePipeline,
    terrain_moisture_brush_ppl: ComputePipeline,
    terrain_moisture_dry_ppl: ComputePipeline,
    chunk_modify_ppl: ComputePipeline,
    chunk_modify_sample_ppl: ComputePipeline,
    chunk_solid_sample_ppl: ComputePipeline,
    #[allow(dead_code)]
    model_voxelize_ppl: ComputePipeline,

    #[allow(dead_code)]
    pool: DescriptorPool,

    build_cmdbuf: CommandBuffer,
    next_edit_sample_seed: u32,
    next_moisture_dither_seed: u32,
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
        let terrain_smooth_heights_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/chunk_writer/terrain_smooth_heights.comp",
            "main",
        )
        .unwrap();
        let terrain_smooth_target_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/chunk_writer/terrain_smooth_target.comp",
            "main",
        )
        .unwrap();
        let terrain_smooth_apply_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/chunk_writer/terrain_smooth_apply.comp",
            "main",
        )
        .unwrap();
        let terrain_smooth_mbo_init_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/chunk_writer/terrain_smooth_mbo_init.comp",
            "main",
        )
        .unwrap();
        let terrain_smooth_mbo_diffuse_ab_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/chunk_writer/terrain_smooth_mbo_diffuse_ab.comp",
            "main",
        )
        .unwrap();
        let terrain_smooth_mbo_diffuse_ba_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/chunk_writer/terrain_smooth_mbo_diffuse_ba.comp",
            "main",
        )
        .unwrap();
        let terrain_smooth_mbo_score_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/chunk_writer/terrain_smooth_mbo_score.comp",
            "main",
        )
        .unwrap();
        let terrain_smooth_mbo_apply_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/chunk_writer/terrain_smooth_mbo_apply.comp",
            "main",
        )
        .unwrap();
        let terrain_moisture_brush_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/chunk_writer/terrain_moisture_brush.comp",
            "main",
        )
        .unwrap();
        let terrain_moisture_dry_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/builder/chunk_writer/terrain_moisture_dry.comp",
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
            &terrain_smooth_heights_sm,
            &terrain_smooth_apply_sm,
        );

        let pool = DescriptorPool::new(device).unwrap();

        let buffer_setup_ppl = ComputePipeline::new(device, &buffer_setup_sm, &pool, &[&resources]);
        let chunk_init_ppl = ComputePipeline::new(device, &chunk_init_sm, &pool, &[&resources]);
        let heightmap_ppl = ComputePipeline::new(device, &heightmap_sm, &pool, &[&resources]);
        let terrain_smooth_heights_ppl =
            ComputePipeline::new(device, &terrain_smooth_heights_sm, &pool, &[&resources]);
        let terrain_smooth_target_ppl =
            ComputePipeline::new(device, &terrain_smooth_target_sm, &pool, &[&resources]);
        let terrain_smooth_apply_ppl =
            ComputePipeline::new(device, &terrain_smooth_apply_sm, &pool, &[&resources]);
        let terrain_smooth_mbo_init_ppl =
            ComputePipeline::new(device, &terrain_smooth_mbo_init_sm, &pool, &[&resources]);
        let terrain_smooth_mbo_diffuse_ab_ppl = ComputePipeline::new(
            device,
            &terrain_smooth_mbo_diffuse_ab_sm,
            &pool,
            &[&resources],
        );
        let terrain_smooth_mbo_diffuse_ba_ppl = ComputePipeline::new(
            device,
            &terrain_smooth_mbo_diffuse_ba_sm,
            &pool,
            &[&resources],
        );
        let terrain_smooth_mbo_score_ppl =
            ComputePipeline::new(device, &terrain_smooth_mbo_score_sm, &pool, &[&resources]);
        let terrain_smooth_mbo_apply_ppl =
            ComputePipeline::new(device, &terrain_smooth_mbo_apply_sm, &pool, &[&resources]);
        let terrain_moisture_brush_ppl =
            ComputePipeline::new(device, &terrain_moisture_brush_sm, &pool, &[&resources]);
        let terrain_moisture_dry_ppl =
            ComputePipeline::new(device, &terrain_moisture_dry_sm, &pool, &[&resources]);
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
            terrain_smooth_heights_ppl,
            terrain_smooth_target_ppl,
            terrain_smooth_apply_ppl,
            terrain_smooth_mbo_init_ppl,
            terrain_smooth_mbo_diffuse_ab_ppl,
            terrain_smooth_mbo_diffuse_ba_ppl,
            terrain_smooth_mbo_score_ppl,
            terrain_smooth_mbo_apply_ppl,
            terrain_moisture_brush_ppl,
            terrain_moisture_dry_ppl,
            chunk_modify_ppl,
            chunk_modify_sample_ppl,
            chunk_solid_sample_ppl,
            model_voxelize_ppl,
            pool,
            build_cmdbuf,
            next_edit_sample_seed: 1,
            next_moisture_dither_seed: 1,
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
                        Some(TextureLayout::GENERAL),
                        0,
                        ClearValue::Color(ColorClearValue::UInt([0, 0, 0, 0])),
                    );
                    resources.free_atlas.get_image().record_clear(
                        cmdbuf,
                        Some(TextureLayout::GENERAL),
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
        region_indirect: &Buffer,
        heightmap_ppl: &ComputePipeline,
        buffer_setup_ppl: &ComputePipeline,
        chunk_init_ppl: &ComputePipeline,
        dispatch_dim: UVec3,
    ) -> CommandBuffer {
        let shader_access_pipeline_barrier = PipelineBarrier::compute_shader_access();
        let indirect_access_pipeline_barrier = PipelineBarrier::compute_to_indirect_access();

        let cmdbuf = CommandBuffer::new(vulkan_ctx.device(), vulkan_ctx.command_pool());
        cmdbuf.begin(false);

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
            TextureLayout::GENERAL,
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

    fn mark_all_solid_workgroups_dirty(&self) {
        execute_one_time_command(
            self.vulkan_ctx.device(),
            self.vulkan_ctx.command_pool(),
            &self.vulkan_ctx.get_general_queue(),
            |cmdbuf| {
                self.resources.solid_workgroup_flags.record_fill(
                    cmdbuf,
                    0,
                    self.resources.solid_workgroup_flags.get_size_bytes(),
                    u32::MAX,
                );
            },
        );
    }

    #[allow(dead_code)]
    pub fn apply_terrain_moisture_brush(
        &mut self,
        start: Vec3,
        end: Vec3,
        radius_world: f32,
        amount: f32,
    ) -> Result<Option<UAabb3>> {
        let atlas_dim = chunk_atlas_dim(&self.resources);
        let start_vox = start * 256.0;
        let end_vox = end * 256.0;
        let radius_vox = (radius_world * 256.0).max(0.0);
        let amount = amount.clamp(0.0, 1.0);
        if radius_vox <= 0.0 || amount <= 0.0 || !start_vox.is_finite() || !end_vox.is_finite() {
            return Ok(None);
        }

        // The water dab is a swept sphere in voxel space, not a whole vertical column over the XZ
        // footprint. The shader still restricts writes to surface dirt/sand voxels.
        let atlas_dim_i = atlas_dim.as_ivec3();
        let min_vox = start_vox.min(end_vox);
        let max_vox = start_vox.max(end_vox);
        let min = IVec3::new(
            (min_vox.x - radius_vox).floor() as i32,
            (min_vox.y - radius_vox).floor() as i32,
            (min_vox.z - radius_vox).floor() as i32,
        );
        let max_exclusive = IVec3::new(
            (max_vox.x + radius_vox).ceil() as i32,
            (max_vox.y + radius_vox).ceil() as i32,
            (max_vox.z + radius_vox).ceil() as i32,
        );
        let clamped_min = min.clamp(IVec3::ZERO, atlas_dim_i);
        let clamped_max = max_exclusive.clamp(IVec3::ZERO, atlas_dim_i);
        if any_ivec3_less_equal(clamped_max, clamped_min) {
            return Ok(None);
        }

        let offset = clamped_min.as_uvec3();
        let dim = (clamped_max - clamped_min).as_uvec3();
        let dither_seed = self.next_moisture_dither_seed;
        self.next_moisture_dither_seed = self
            .next_moisture_dither_seed
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223)
            .max(1);
        self.resources
            .terrain_moisture_brush_info
            .fill_uniform(&TerrainMoistureBrushInfoGpu {
                offset: [offset.x, offset.y, offset.z, dither_seed],
                dim: [dim.x, dim.y, dim.z, 0],
                start_radius: [start_vox.x, start_vox.y, start_vox.z, radius_vox],
                end_amount: [end_vox.x, end_vox.y, end_vox.z, amount],
            })?;

        let shader_access_pipeline_barrier = PipelineBarrier::compute_shader_access();
        execute_one_time_command(
            self.vulkan_ctx.device(),
            self.vulkan_ctx.command_pool(),
            &self.vulkan_ctx.get_general_queue(),
            |cmdbuf| {
                self.terrain_moisture_brush_ppl.record(
                    cmdbuf,
                    Extent3D::new(dim.x, dim.y, dim.z),
                    None,
                );
                shader_access_pipeline_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            },
        );

        Ok(Some(UAabb3::new(offset, offset + dim - UVec3::ONE)))
    }

    pub fn record_terrain_moisture_dry_region(
        &mut self,
        cmdbuf: &CommandBuffer,
        atlas_offset: UVec3,
        atlas_dim: UVec3,
        dry_probability: f32,
    ) -> bool {
        let chunk_atlas_dim = chunk_atlas_dim(&self.resources);
        let dry_probability = dry_probability.clamp(0.0, 1.0);
        if dry_probability <= 0.0 || atlas_dim == UVec3::ZERO || chunk_atlas_dim == UVec3::ZERO {
            return false;
        }
        if atlas_offset.x > chunk_atlas_dim.x
            || atlas_offset.y > chunk_atlas_dim.y
            || atlas_offset.z > chunk_atlas_dim.z
            || atlas_dim.x > chunk_atlas_dim.x - atlas_offset.x
            || atlas_dim.y > chunk_atlas_dim.y - atlas_offset.y
            || atlas_dim.z > chunk_atlas_dim.z - atlas_offset.z
        {
            log::warn!(
                "Skipping terrain moisture dry region outside atlas: offset={:?} dim={:?} atlas={:?}",
                atlas_offset,
                atlas_dim,
                chunk_atlas_dim,
            );
            return false;
        }

        let dither_seed = self.next_moisture_dither_seed;
        self.next_moisture_dither_seed = self
            .next_moisture_dither_seed
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223)
            .max(1);
        let push_constants = TerrainMoistureDryPushConstants {
            offset: [atlas_offset.x, atlas_offset.y, atlas_offset.z, dither_seed],
            dim: [atlas_dim.x, atlas_dim.y, atlas_dim.z, 0],
            dry_params: [dry_probability, 0.0, 0.0, 0.0],
        };

        self.terrain_moisture_dry_ppl.record(
            cmdbuf,
            Extent3D::new(atlas_dim.x, atlas_dim.y, atlas_dim.z),
            Some(bytemuck::bytes_of(&push_constants)),
        );
        PipelineBarrier::compute_shader_access().record_insert(self.vulkan_ctx.device(), cmdbuf);

        true
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
        job.gpu_job.wait()?;
        let wait_elapsed = wait_start.elapsed();
        crate::util::BENCH.lock().unwrap().record(
            "chunk_solid_sample_gpu_dispatch",
            wait_elapsed + job.submit_elapsed,
        );
        let result = self.finish_chunk_atlas_solid_grid_sample(job)?;
        log::info!(
            "[PLAIN_BUILDER][CHUNK_SOLID_SAMPLE] atlas_offset={:?} source_dim={:?} sample_dim={:?} samples={} readback_bytes={} gpu_submit={:.3}ms gpu_wait={:.3}ms readback={:.3}ms convert={:.3}ms total={:.3}ms",
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

        let host_read_barrier = PipelineBarrier::compute_to_host_read();
        let command_buffer =
            CommandBuffer::new(self.vulkan_ctx.device(), self.vulkan_ctx.command_pool());
        let submit_start = Instant::now();
        command_buffer.begin(true);
        self.chunk_solid_sample_ppl.record(
            &command_buffer,
            Extent3D::new(sample_dim.x, sample_dim.y, sample_dim.z),
            None,
        );
        host_read_barrier.record_insert(self.vulkan_ctx.device(), &command_buffer);
        command_buffer.end();
        let gpu_job = command_buffer.submit_gpu_job(
            &self.vulkan_ctx.get_general_queue(),
            "plain.chunk_solid_sample",
        )?;
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
            gpu_job,
        })
    }

    pub fn chunk_atlas_solid_grid_sample_ready(&self, job: &ChunkSolidSampleJob) -> Result<bool> {
        job.gpu_job
            .is_complete()
            .map_err(|err| anyhow::anyhow!("failed to poll chunk solid sample GPU job: {err}"))
    }

    pub fn finish_chunk_atlas_solid_grid_sample(
        &mut self,
        job: ChunkSolidSampleJob,
    ) -> Result<ChunkSolidSampleResult> {
        let gpu_completion_latency_elapsed = job.submitted_at.elapsed();
        let _completed_gpu_job = job.gpu_job.wait_complete()?;
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
            gpu_completion_latency_ms: gpu_completion_latency_elapsed.as_secs_f64() * 1000.0,
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
            &self.resources.region_indirect,
            &self.heightmap_ppl,
            &self.buffer_setup_ppl,
            &self.chunk_init_ppl,
            atlas_dim,
        );

        let _completed_gpu_job = self
            .build_cmdbuf
            .submit_gpu_job(&self.vulkan_ctx.get_general_queue(), "plain.chunk_init")?
            .wait_complete()?;
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

    pub fn smooth_terrain_dirt(
        &mut self,
        center: Vec3,
        brush_radius_world: f32,
        strength: f32,
        max_delta_world: f32,
        deadband_world: f32,
    ) -> Result<Option<UAabb3>> {
        let atlas_dim = chunk_atlas_dim(&self.resources);
        let center_vox = center * 256.0;
        let brush_radius_vox = (brush_radius_world * 256.0).max(0.0);
        if brush_radius_vox <= 0.0 || !center_vox.is_finite() {
            return Ok(None);
        }
        let max_delta_vox = (max_delta_world * 256.0).max(1.0);
        let sample_radius_vox = brush_radius_vox + max_delta_vox.ceil().clamp(2.0, 10.0) + 3.0;
        let Some((_offset, dim)) = clipped_voxel_box(center_vox, sample_radius_vox, atlas_dim)
        else {
            return Ok(None);
        };

        let cell_count = volume_cell_count(dim)? as u64;
        if dim.max_element() > TERRAIN_SMOOTH_MBO_MAX_DIM
            || cell_count > TERRAIN_SMOOTH_MBO_CELL_CAPACITY
        {
            log::warn!(
                "[TERRAIN_SMOOTH_MBO] sample dim {:?} exceeds GPU capacity dim={} cells={}; falling back to CPU smoother",
                dim,
                TERRAIN_SMOOTH_MBO_MAX_DIM,
                TERRAIN_SMOOTH_MBO_CELL_CAPACITY,
            );
            return self.smooth_terrain_dirt_cpu(
                center,
                brush_radius_world,
                strength,
                max_delta_world,
                deadband_world,
            );
        }

        self.smooth_terrain_mbo_gpu(
            center,
            brush_radius_world,
            strength,
            max_delta_world,
            deadband_world,
        )
    }

    fn smooth_terrain_mbo_gpu(
        &mut self,
        center: Vec3,
        brush_radius_world: f32,
        strength: f32,
        max_delta_world: f32,
        deadband_world: f32,
    ) -> Result<Option<UAabb3>> {
        let total_start = Instant::now();
        let atlas_dim = chunk_atlas_dim(&self.resources);
        let center_vox = center * 256.0;
        let brush_radius_vox = (brush_radius_world * 256.0).max(0.0);
        if brush_radius_vox <= 0.0 || !center_vox.is_finite() {
            return Ok(None);
        }

        let strength = strength.clamp(0.0, 1.0);
        let deadband_vox = (deadband_world * 256.0).max(0.0);
        let kernel_radius_vox = (brush_radius_vox * 0.35).clamp(2.0, 12.0);
        let band_radius = ((max_delta_world * 256.0).max(1.0).ceil() as u32).clamp(2, 10);
        let sample_radius_vox = brush_radius_vox + band_radius as f32 + 3.0;
        let Some((offset, dim)) = clipped_voxel_box(center_vox, sample_radius_vox, atlas_dim)
        else {
            return Ok(None);
        };
        let cell_count = volume_cell_count(dim)? as u64;
        if dim.max_element() > TERRAIN_SMOOTH_MBO_MAX_DIM
            || cell_count > TERRAIN_SMOOTH_MBO_CELL_CAPACITY
        {
            anyhow::bail!(
                "terrain smooth MBO sample dim {:?} cells={} exceeds capacity dim={} cells={}",
                dim,
                cell_count,
                TERRAIN_SMOOTH_MBO_MAX_DIM,
                TERRAIN_SMOOTH_MBO_CELL_CAPACITY,
            );
        }

        let mut iteration_count = ((kernel_radius_vox / 2.0).ceil() as u32).clamp(2, 6);
        if iteration_count % 2 != 0 {
            iteration_count += 1;
        }

        let mut info = TerrainSmoothMboInfoGpu {
            offset: [offset.x, offset.y, offset.z, 0],
            dim: [dim.x, dim.y, dim.z, TERRAIN_SMOOTH_MBO_HISTOGRAM_BINS],
            center_radius: [center_vox.x, center_vox.y, center_vox.z, brush_radius_vox],
            params: [strength, deadband_vox, 0.0, 0.0],
            threshold: [0, 0, 0, 0],
        };

        let prepare_start = Instant::now();
        self.resources.terrain_smooth_mbo_info.fill_uniform(&info)?;
        let prepare_ms = prepare_start.elapsed().as_secs_f64() * 1000.0;

        let transfer_to_compute_barrier = PipelineBarrier::transfer_to_compute_shader_access();
        let shader_access_barrier = PipelineBarrier::compute_shader_access();
        let host_read_barrier = PipelineBarrier::compute_to_host_read();

        let score_gpu_start = Instant::now();
        let command_buffer =
            CommandBuffer::new(self.vulkan_ctx.device(), self.vulkan_ctx.command_pool());
        command_buffer.begin(true);
        self.resources.terrain_smooth_mbo_histogram.record_fill(
            &command_buffer,
            0,
            self.resources.terrain_smooth_mbo_histogram.get_size_bytes(),
            0,
        );
        self.resources.terrain_smooth_mbo_result.record_fill(
            &command_buffer,
            0,
            self.resources.terrain_smooth_mbo_result.get_size_bytes(),
            0,
        );
        transfer_to_compute_barrier.record_insert(self.vulkan_ctx.device(), &command_buffer);
        self.terrain_smooth_mbo_init_ppl.record(
            &command_buffer,
            Extent3D::new(dim.x, dim.y, dim.z),
            None,
        );
        shader_access_barrier.record_insert(self.vulkan_ctx.device(), &command_buffer);
        for _ in 0..(iteration_count / 2) {
            self.terrain_smooth_mbo_diffuse_ab_ppl.record(
                &command_buffer,
                Extent3D::new(dim.x, dim.y, dim.z),
                None,
            );
            shader_access_barrier.record_insert(self.vulkan_ctx.device(), &command_buffer);
            self.terrain_smooth_mbo_diffuse_ba_ppl.record(
                &command_buffer,
                Extent3D::new(dim.x, dim.y, dim.z),
                None,
            );
            shader_access_barrier.record_insert(self.vulkan_ctx.device(), &command_buffer);
        }
        self.terrain_smooth_mbo_score_ppl.record(
            &command_buffer,
            Extent3D::new(dim.x, dim.y, dim.z),
            None,
        );
        host_read_barrier.record_insert(self.vulkan_ctx.device(), &command_buffer);
        command_buffer.end();
        command_buffer
            .submit_gpu_job(
                &self.vulkan_ctx.get_general_queue(),
                "plain.terrain_smooth_mbo_score",
            )?
            .wait_complete()?;
        let score_gpu_ms = score_gpu_start.elapsed().as_secs_f64() * 1000.0;

        let read_start = Instant::now();
        let histogram_raw = self.resources.terrain_smooth_mbo_histogram.read_back()?;
        let result_raw = self.resources.terrain_smooth_mbo_result.read_back()?;
        let histogram = histogram_raw
            .chunks_exact(std::mem::size_of::<u32>())
            .map(|chunk| u32::from_ne_bytes(chunk.try_into().unwrap()))
            .collect::<Vec<_>>();
        let score_result = *bytemuck::from_bytes::<TerrainSmoothMboResultGpu>(&result_raw);
        let read_ms = read_start.elapsed().as_secs_f64() * 1000.0;
        let candidate_count = score_result.counts[0];
        let target_solid_count = score_result.counts[1];
        if candidate_count == 0 || target_solid_count == 0 || target_solid_count == candidate_count
        {
            log::info!(
                "[TERRAIN_SMOOTH_MBO] no-op candidates={} target_solid={} dim={:?} iters={} prepare={:.2}ms score_gpu={:.2}ms read={:.2}ms total={:.2}ms",
                candidate_count,
                target_solid_count,
                dim,
                iteration_count,
                prepare_ms,
                score_gpu_ms,
                read_ms,
                total_start.elapsed().as_secs_f64() * 1000.0,
            );
            return Ok(None);
        }

        let Some(threshold) = terrain_smooth_mbo_threshold(&histogram, target_solid_count) else {
            return Ok(None);
        };
        info.threshold = [threshold.bin, threshold.tie_hash_limit, 0, 0];
        self.resources.terrain_smooth_mbo_info.fill_uniform(&info)?;

        let apply_gpu_start = Instant::now();
        let command_buffer =
            CommandBuffer::new(self.vulkan_ctx.device(), self.vulkan_ctx.command_pool());
        command_buffer.begin(true);
        self.resources.terrain_smooth_mbo_result.record_fill(
            &command_buffer,
            0,
            self.resources.terrain_smooth_mbo_result.get_size_bytes(),
            0,
        );
        let changed_min_offset = 2 * std::mem::size_of::<[u32; 4]>() as u64;
        self.resources.terrain_smooth_mbo_result.record_fill(
            &command_buffer,
            changed_min_offset,
            std::mem::size_of::<[u32; 4]>() as u64,
            u32::MAX,
        );
        transfer_to_compute_barrier.record_insert(self.vulkan_ctx.device(), &command_buffer);
        self.terrain_smooth_mbo_apply_ppl.record(
            &command_buffer,
            Extent3D::new(dim.x, dim.y, dim.z),
            None,
        );
        host_read_barrier.record_insert(self.vulkan_ctx.device(), &command_buffer);
        command_buffer.end();
        command_buffer
            .submit_gpu_job(
                &self.vulkan_ctx.get_general_queue(),
                "plain.terrain_smooth_mbo_apply",
            )?
            .wait_complete()?;
        let apply_gpu_ms = apply_gpu_start.elapsed().as_secs_f64() * 1000.0;

        let apply_read_start = Instant::now();
        let apply_raw = self.resources.terrain_smooth_mbo_result.read_back()?;
        let apply_result = *bytemuck::from_bytes::<TerrainSmoothMboResultGpu>(&apply_raw);
        let apply_read_ms = apply_read_start.elapsed().as_secs_f64() * 1000.0;
        let changed_count = apply_result.counts[2];
        if changed_count == 0 || apply_result.changed_min[0] == u32::MAX {
            log::info!(
                "[TERRAIN_SMOOTH_MBO] no-op stable candidates={} target_solid={} threshold_bin={} tie_keep={}/{} dim={:?} iters={} score_gpu={:.2}ms apply_gpu={:.2}ms total={:.2}ms",
                candidate_count,
                target_solid_count,
                threshold.bin,
                threshold.tie_keep_count,
                threshold.tie_bin_count,
                dim,
                iteration_count,
                score_gpu_ms,
                apply_gpu_ms,
                total_start.elapsed().as_secs_f64() * 1000.0,
            );
            return Ok(None);
        }

        let changed_min = UVec3::new(
            apply_result.changed_min[0],
            apply_result.changed_min[1],
            apply_result.changed_min[2],
        );
        let changed_max = UVec3::new(
            apply_result.changed_max[0],
            apply_result.changed_max[1],
            apply_result.changed_max[2],
        );
        let volume_delta = apply_result.counts[3] as i64 - apply_result.counts_extra[0] as i64;
        log::info!(
            "[TERRAIN_SMOOTH_MBO] changed={} added={} removed={} volume_delta={} candidates={} target_solid={} threshold_bin={} tie_keep={}/{} dim={:?} changed_min={:?} changed_max={:?} iters={} prepare={:.2}ms score_gpu={:.2}ms read={:.2}ms apply_gpu={:.2}ms apply_read={:.2}ms total={:.2}ms",
            changed_count,
            apply_result.counts[3],
            apply_result.counts_extra[0],
            volume_delta,
            candidate_count,
            target_solid_count,
            threshold.bin,
            threshold.tie_keep_count,
            threshold.tie_bin_count,
            dim,
            changed_min,
            changed_max,
            iteration_count,
            prepare_ms,
            score_gpu_ms,
            read_ms,
            apply_gpu_ms,
            apply_read_ms,
            total_start.elapsed().as_secs_f64() * 1000.0,
        );

        Ok(Some(UAabb3::new(
            changed_min.saturating_sub(UVec3::ONE),
            (changed_max + UVec3::ONE).min(atlas_dim - UVec3::ONE),
        )))
    }

    fn smooth_terrain_dirt_cpu(
        &mut self,
        center: Vec3,
        brush_radius_world: f32,
        strength: f32,
        max_delta_world: f32,
        deadband_world: f32,
    ) -> Result<Option<UAabb3>> {
        let total_start = Instant::now();
        let atlas_dim = chunk_atlas_dim(&self.resources);
        let center_vox = center * 256.0;
        let brush_radius_vox = (brush_radius_world * 256.0).max(0.0);
        if brush_radius_vox <= 0.0 || !center_vox.is_finite() {
            return Ok(None);
        }

        let strength = strength.clamp(0.0, 1.0);
        let deadband_vox = (deadband_world * 256.0).max(0.0);
        let kernel_radius_vox = (brush_radius_vox * 0.35).clamp(2.0, 12.0);
        let band_radius = ((max_delta_world * 256.0).max(1.0).ceil() as u32).clamp(2, 10);
        let sample_radius_vox = brush_radius_vox + band_radius as f32 + 3.0;
        let Some((offset, dim)) = clipped_voxel_box(center_vox, sample_radius_vox, atlas_dim)
        else {
            return Ok(None);
        };

        let cell_count = volume_cell_count(dim)?;
        let read_start = Instant::now();
        let mut atlas_data = self.read_chunk_atlas_region(offset, dim)?;
        let read_ms = read_start.elapsed().as_secs_f64() * 1000.0;
        if atlas_data.len() != cell_count {
            anyhow::bail!(
                "terrain smooth atlas readback size mismatch: got {}, expected {}",
                atlas_data.len(),
                cell_count
            );
        }

        let solid: Vec<bool> = atlas_data
            .iter()
            .map(|voxel_data| is_terrain_voxel(voxel_type_from_atlas_byte(*voxel_data) as u32))
            .collect();
        let mutable: Vec<bool> = atlas_data
            .iter()
            .map(|voxel_data| {
                is_terrain_smooth_mutable_voxel(voxel_type_from_atlas_byte(*voxel_data) as u32)
            })
            .collect();

        let classify_start = Instant::now();
        let Some(surface_seed) = find_nearest_smooth_surface_seed(
            &solid,
            &mutable,
            offset,
            dim,
            center_vox,
            brush_radius_vox,
        ) else {
            return Ok(None);
        };
        let surface_component = collect_smooth_surface_component(
            &solid,
            &mutable,
            offset,
            dim,
            center_vox,
            brush_radius_vox,
            surface_seed,
        );
        let surface_voxel_count = surface_component
            .iter()
            .filter(|selected| **selected)
            .count();
        if surface_voxel_count == 0 {
            return Ok(None);
        }

        let band_distance = collect_smooth_mutation_band(
            &mutable,
            dim,
            &surface_component,
            band_radius.saturating_add(2),
        );
        let band_indices: Vec<usize> = band_distance
            .iter()
            .enumerate()
            .filter_map(|(idx, distance)| {
                (u32::from(*distance) <= band_radius.saturating_add(2)).then_some(idx)
            })
            .collect();
        if band_indices.is_empty() {
            return Ok(None);
        }
        let classify_ms = classify_start.elapsed().as_secs_f64() * 1000.0;

        let blur_start = Instant::now();
        let iteration_count = ((kernel_radius_vox / 2.0).ceil() as usize).clamp(3, 6);
        let mut density_a: Vec<f32> = solid
            .iter()
            .map(|is_solid| if *is_solid { 1.0 } else { 0.0 })
            .collect();
        let mut density_b = density_a.clone();
        for _ in 0..iteration_count {
            diffuse_smooth_density(
                &density_a,
                &mut density_b,
                &band_distance,
                dim,
                band_radius + 2,
                &band_indices,
            );
            std::mem::swap(&mut density_a, &mut density_b);
        }
        let blur_ms = blur_start.elapsed().as_secs_f64() * 1000.0;

        let rank_start = Instant::now();
        let mut candidates = Vec::new();
        let mut target_solid_count = 0usize;
        candidates.reserve(band_indices.len().min(cell_count));
        for &idx in &band_indices {
            if !mutable[idx] || u32::from(band_distance[idx]) > band_radius {
                continue;
            }

            let local = volume_local_pos(idx, dim);
            let world_center = offset.as_vec3() + local.as_vec3() + Vec3::splat(0.5);
            let center_dist = world_center.distance(center_vox);
            if center_dist > brush_radius_vox {
                continue;
            }

            if solid[idx] {
                target_solid_count += 1;
            }

            let brush_falloff = 1.0 - smoothstep01(center_dist / brush_radius_vox);
            let local_strength = (strength * brush_falloff * 2.25).clamp(0.0, 1.0);
            let original_density = if solid[idx] { 1.0 } else { 0.0 };
            let mut score =
                original_density * (1.0 - local_strength) + density_a[idx] * local_strength;
            if (score - original_density).abs() <= deadband_vox / 256.0 {
                score = original_density;
            }
            candidates.push(TerrainSmoothCandidate {
                index: idx,
                score,
                tie_breaker: smooth_candidate_hash(offset + local),
            });
        }

        if candidates.is_empty()
            || target_solid_count == 0
            || target_solid_count == candidates.len()
        {
            log::info!(
                "[TERRAIN_SMOOTH_3D] no-op candidates={} target_solid={} surface={} band={} dim={:?} read={:.2}ms classify={:.2}ms blur={:.2}ms rank={:.2}ms total={:.2}ms",
                candidates.len(),
                target_solid_count,
                surface_voxel_count,
                band_indices.len(),
                dim,
                read_ms,
                classify_ms,
                blur_ms,
                rank_start.elapsed().as_secs_f64() * 1000.0,
                total_start.elapsed().as_secs_f64() * 1000.0,
            );
            return Ok(None);
        }

        let candidate_count = candidates.len();
        candidates.sort_unstable_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| b.tie_breaker.cmp(&a.tie_breaker))
        });

        let rank_ms = rank_start.elapsed().as_secs_f64() * 1000.0;

        let apply_start = Instant::now();
        let mut changes = Vec::new();
        let mut changed_min = UVec3::splat(u32::MAX);
        let mut changed_max = UVec3::ZERO;
        for (rank, candidate) in candidates.into_iter().enumerate() {
            let idx = candidate.index;
            let wants_solid = rank < target_solid_count;
            if wants_solid == solid[idx] {
                continue;
            }

            let local = volume_local_pos(idx, dim);
            let world = offset + local;
            let new_voxel_type = if wants_solid {
                choose_smooth_fill_voxel_type(&atlas_data, &solid, idx, dim)
            } else {
                VOXEL_TYPE_EMPTY as u8
            };
            if voxel_type_from_atlas_byte(atlas_data[idx]) == new_voxel_type {
                continue;
            }

            let new_voxel = pack_voxel_atlas_byte_for_fill(atlas_data[idx], new_voxel_type);
            changes.push((idx, new_voxel));
            changed_min = changed_min.min(world);
            changed_max = changed_max.max(world);
        }

        if changes.is_empty() {
            log::info!(
                "[TERRAIN_SMOOTH_3D] no-op stable target_solid={} candidates={} surface={} band={} dim={:?} read={:.2}ms classify={:.2}ms blur={:.2}ms rank={:.2}ms apply={:.2}ms total={:.2}ms",
                target_solid_count,
                candidate_count,
                surface_voxel_count,
                band_indices.len(),
                dim,
                read_ms,
                classify_ms,
                blur_ms,
                rank_ms,
                apply_start.elapsed().as_secs_f64() * 1000.0,
                total_start.elapsed().as_secs_f64() * 1000.0,
            );
            return Ok(None);
        }

        for (idx, new_voxel) in &changes {
            atlas_data[*idx] = *new_voxel;
        }
        let changed_count = changes.len() as u32;
        let write_offset = changed_min;
        let write_dim = changed_max - changed_min + UVec3::ONE;
        let write_data =
            extract_volume_region_u8(&atlas_data, dim, write_offset - offset, write_dim);

        let queue = self.vulkan_ctx.get_general_queue();
        let command_pool = self.vulkan_ctx.command_pool();
        self.resources.chunk_atlas.get_image().fill_with_raw_u8(
            &queue,
            command_pool,
            TextureRegion {
                offset: [
                    write_offset.x as i32,
                    write_offset.y as i32,
                    write_offset.z as i32,
                ],
                extent: Extent3D::new(write_dim.x, write_dim.y, write_dim.z),
            },
            &write_data,
            0,
            Some(TextureLayout::GENERAL),
        )?;
        self.mark_all_solid_workgroups_dirty();
        let apply_ms = apply_start.elapsed().as_secs_f64() * 1000.0;

        log::info!(
            "[TERRAIN_SMOOTH_3D] changed={} target_solid={} candidates={} surface={} band={} dim={:?} write_dim={:?} iters={} read={:.2}ms classify={:.2}ms blur={:.2}ms rank={:.2}ms apply={:.2}ms total={:.2}ms",
            changed_count,
            target_solid_count,
            candidate_count,
            surface_voxel_count,
            band_indices.len(),
            dim,
            write_dim,
            iteration_count,
            read_ms,
            classify_ms,
            blur_ms,
            rank_ms,
            apply_ms,
            total_start.elapsed().as_secs_f64() * 1000.0,
        );

        Ok(Some(UAabb3::new(
            changed_min.saturating_sub(UVec3::ONE),
            (changed_max + UVec3::ONE).min(atlas_dim - UVec3::ONE),
        )))
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
        let shader_access_pipeline_barrier = PipelineBarrier::compute_shader_access();

        let gpu_start = Instant::now();
        execute_one_time_gpu_job(
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

    #[allow(dead_code)]
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

        let shader_access_pipeline_barrier = PipelineBarrier::compute_shader_access();

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

#[allow(dead_code)]
const MODEL_VOXELIZE_SURFACE_THICKNESS_VOX: f32 = 0.75;

#[allow(dead_code)]
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

#[derive(Clone, Copy, Debug)]
struct TerrainSmoothCandidate {
    index: usize,
    score: f32,
    tie_breaker: u32,
}

#[derive(Clone, Copy, Debug)]
struct TerrainSmoothMboThreshold {
    bin: u32,
    tie_keep_count: u32,
    tie_bin_count: u32,
    tie_hash_limit: u32,
}

const TERRAIN_SMOOTH_UNREACHED: u16 = u16::MAX;

fn is_terrain_voxel(voxel_type: u32) -> bool {
    voxel_type == VOXEL_TYPE_DIRT || voxel_type == VOXEL_TYPE_SAND || voxel_type == VOXEL_TYPE_ROCK
}

fn is_terrain_smooth_mutable_voxel(voxel_type: u32) -> bool {
    voxel_type == VOXEL_TYPE_EMPTY || is_terrain_voxel(voxel_type)
}

fn volume_cell_count(dim: UVec3) -> Result<usize> {
    let count = dim
        .x
        .checked_mul(dim.y)
        .and_then(|count| count.checked_mul(dim.z))
        .ok_or_else(|| anyhow::anyhow!("terrain smooth volume too large: {:?}", dim))?;
    Ok(count as usize)
}

fn volume_index(local: UVec3, dim: UVec3) -> usize {
    ((local.z * dim.y + local.y) * dim.x + local.x) as usize
}

fn volume_local_pos(index: usize, dim: UVec3) -> UVec3 {
    let index = index as u32;
    let x = index % dim.x;
    let yz = index / dim.x;
    let y = yz % dim.y;
    let z = yz / dim.y;
    UVec3::new(x, y, z)
}

fn offset_volume_index(local: UVec3, offset: IVec3, dim: UVec3) -> Option<usize> {
    let next = local.as_ivec3() + offset;
    if next.cmplt(IVec3::ZERO).any() || next.cmpge(dim.as_ivec3()).any() {
        return None;
    }
    Some(volume_index(next.as_uvec3(), dim))
}

fn smoothstep01(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn smooth_candidate_hash(pos: UVec3) -> u32 {
    let mut x = pos.x.wrapping_mul(0x9E37_79B9)
        ^ pos.y.wrapping_mul(0x85EB_CA6B)
        ^ pos.z.wrapping_mul(0xC2B2_AE35);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^ (x >> 16)
}

fn terrain_smooth_mbo_threshold(
    histogram: &[u32],
    target_solid_count: u32,
) -> Option<TerrainSmoothMboThreshold> {
    if target_solid_count == 0 || histogram.is_empty() {
        return None;
    }

    let mut remaining = target_solid_count;
    for (bin, count) in histogram.iter().enumerate().rev() {
        if *count == 0 {
            continue;
        }
        if remaining > *count {
            remaining -= *count;
            continue;
        }

        let tie_keep_count = remaining;
        let tie_hash_limit = if tie_keep_count >= *count {
            u32::MAX
        } else {
            ((tie_keep_count as f64 / *count as f64) * u32::MAX as f64).floor() as u32
        };
        return Some(TerrainSmoothMboThreshold {
            bin: bin as u32,
            tie_keep_count,
            tie_bin_count: *count,
            tie_hash_limit,
        });
    }

    None
}

fn clipped_voxel_box(
    center_vox: Vec3,
    radius_vox: f32,
    atlas_dim: UVec3,
) -> Option<(UVec3, UVec3)> {
    let atlas_dim_i = atlas_dim.as_ivec3();
    let min = (center_vox - Vec3::splat(radius_vox)).floor().as_ivec3();
    let max_exclusive = (center_vox + Vec3::splat(radius_vox)).ceil().as_ivec3();
    let clamped_min = min.clamp(IVec3::ZERO, atlas_dim_i);
    let clamped_max = max_exclusive.clamp(IVec3::ZERO, atlas_dim_i);
    if clamped_min.x >= clamped_max.x
        || clamped_min.y >= clamped_max.y
        || clamped_min.z >= clamped_max.z
    {
        return None;
    }

    Some((
        clamped_min.as_uvec3(),
        (clamped_max - clamped_min).as_uvec3(),
    ))
}

fn is_smooth_surface_solid(solid: &[bool], idx: usize, dim: UVec3) -> bool {
    if !solid[idx] {
        return false;
    }

    let local = volume_local_pos(idx, dim);
    for offset in [
        IVec3::X,
        IVec3::NEG_X,
        IVec3::Y,
        IVec3::NEG_Y,
        IVec3::Z,
        IVec3::NEG_Z,
    ] {
        if offset_volume_index(local, offset, dim).is_none_or(|neighbor| !solid[neighbor]) {
            return true;
        }
    }
    false
}

fn smooth_surface_normal(solid: &[bool], idx: usize, dim: UVec3) -> Vec3 {
    let local = volume_local_pos(idx, dim);
    let sample = |offset: IVec3| -> f32 {
        offset_volume_index(local, offset, dim)
            .map(|neighbor| if solid[neighbor] { 1.0 } else { 0.0 })
            .unwrap_or(0.0)
    };
    Vec3::new(
        sample(IVec3::NEG_X) - sample(IVec3::X),
        sample(IVec3::NEG_Y) - sample(IVec3::Y),
        sample(IVec3::NEG_Z) - sample(IVec3::Z),
    )
    .normalize_or_zero()
}

fn find_nearest_smooth_surface_seed(
    solid: &[bool],
    mutable: &[bool],
    offset: UVec3,
    dim: UVec3,
    center_vox: Vec3,
    brush_radius_vox: f32,
) -> Option<usize> {
    let mut best = None;
    let mut best_dist_sq = f32::INFINITY;
    let seed_radius = brush_radius_vox.min(10.0) + 3.0;
    let seed_radius_sq = seed_radius * seed_radius;
    let local_center = center_vox - offset.as_vec3();
    let local_min = (local_center - Vec3::splat(seed_radius))
        .floor()
        .as_ivec3()
        .clamp(IVec3::ZERO, dim.as_ivec3());
    let local_max = (local_center + Vec3::splat(seed_radius))
        .ceil()
        .as_ivec3()
        .clamp(IVec3::ZERO, dim.as_ivec3());

    for z in local_min.z..local_max.z {
        for y in local_min.y..local_max.y {
            for x in local_min.x..local_max.x {
                let local = UVec3::new(x as u32, y as u32, z as u32);
                let idx = volume_index(local, dim);
                if !mutable[idx] || !is_smooth_surface_solid(solid, idx, dim) {
                    continue;
                }
                let world_center = offset.as_vec3() + local.as_vec3() + Vec3::splat(0.5);
                let dist_sq = world_center.distance_squared(center_vox);
                if dist_sq <= seed_radius_sq && dist_sq < best_dist_sq {
                    best = Some(idx);
                    best_dist_sq = dist_sq;
                }
            }
        }
    }
    best
}

fn collect_smooth_surface_component(
    solid: &[bool],
    mutable: &[bool],
    offset: UVec3,
    dim: UVec3,
    center_vox: Vec3,
    brush_radius_vox: f32,
    seed: usize,
) -> Vec<bool> {
    let mut selected = vec![false; solid.len()];
    let seed_normal = smooth_surface_normal(solid, seed, dim);
    let use_normal_filter = seed_normal.length_squared() > 0.01;
    let max_dist = brush_radius_vox + 2.0;
    let max_dist_sq = max_dist * max_dist;
    let mut queue = VecDeque::new();
    selected[seed] = true;
    queue.push_back(seed);

    while let Some(idx) = queue.pop_front() {
        let local = volume_local_pos(idx, dim);
        for dz in -1..=1 {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if dx == 0 && dy == 0 && dz == 0 {
                        continue;
                    }
                    let Some(neighbor) = offset_volume_index(local, IVec3::new(dx, dy, dz), dim)
                    else {
                        continue;
                    };
                    if selected[neighbor]
                        || !mutable[neighbor]
                        || !is_smooth_surface_solid(solid, neighbor, dim)
                    {
                        continue;
                    }
                    let neighbor_local = volume_local_pos(neighbor, dim);
                    let world_center =
                        offset.as_vec3() + neighbor_local.as_vec3() + Vec3::splat(0.5);
                    if world_center.distance_squared(center_vox) > max_dist_sq {
                        continue;
                    }
                    if use_normal_filter {
                        let normal = smooth_surface_normal(solid, neighbor, dim);
                        if normal.length_squared() > 0.01 && normal.dot(seed_normal) < 0.15 {
                            continue;
                        }
                    }
                    selected[neighbor] = true;
                    queue.push_back(neighbor);
                }
            }
        }
    }

    selected
}

fn collect_smooth_mutation_band(
    mutable: &[bool],
    dim: UVec3,
    surface_component: &[bool],
    max_band: u32,
) -> Vec<u16> {
    let mut distance = vec![TERRAIN_SMOOTH_UNREACHED; mutable.len()];
    let mut queue = VecDeque::new();
    for (idx, selected) in surface_component.iter().enumerate() {
        if *selected {
            distance[idx] = 0;
            queue.push_back(idx);
        }
    }

    while let Some(idx) = queue.pop_front() {
        let next_distance = distance[idx].saturating_add(1);
        if u32::from(next_distance) > max_band {
            continue;
        }
        let local = volume_local_pos(idx, dim);
        for offset in [
            IVec3::X,
            IVec3::NEG_X,
            IVec3::Y,
            IVec3::NEG_Y,
            IVec3::Z,
            IVec3::NEG_Z,
        ] {
            let Some(neighbor) = offset_volume_index(local, offset, dim) else {
                continue;
            };
            if !mutable[neighbor] || distance[neighbor] <= next_distance {
                continue;
            }
            distance[neighbor] = next_distance;
            queue.push_back(neighbor);
        }
    }

    distance
}

fn diffuse_smooth_density(
    input: &[f32],
    output: &mut [f32],
    band_distance: &[u16],
    dim: UVec3,
    max_band: u32,
    active_indices: &[usize],
) {
    for &idx in active_indices {
        if u32::from(band_distance[idx]) > max_band {
            continue;
        }
        let local = volume_local_pos(idx, dim);
        let mut weighted_sum = 0.0;
        let mut total_weight = 0.0;
        for dz in -1..=1 {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let Some(neighbor) = offset_volume_index(local, IVec3::new(dx, dy, dz), dim)
                    else {
                        continue;
                    };
                    if u32::from(band_distance[neighbor]) > max_band {
                        continue;
                    }
                    let dist_sq = (dx * dx + dy * dy + dz * dz) as f32;
                    let weight = match dist_sq as i32 {
                        0 => 4.0,
                        1 => 2.0,
                        2 => 1.0,
                        _ => 0.5,
                    };
                    weighted_sum += input[neighbor] * weight;
                    total_weight += weight;
                }
            }
        }
        if total_weight > 0.0 {
            output[idx] = weighted_sum / total_weight;
        }
    }
}

fn extract_volume_region_u8(
    source: &[u8],
    source_dim: UVec3,
    region_offset: UVec3,
    region_dim: UVec3,
) -> Vec<u8> {
    let mut out = vec![0; (region_dim.x * region_dim.y * region_dim.z) as usize];
    for z in 0..region_dim.z {
        for y in 0..region_dim.y {
            let src_start = volume_index(region_offset + UVec3::new(0, y, z), source_dim);
            let dst_start = ((z * region_dim.y + y) * region_dim.x) as usize;
            let len = region_dim.x as usize;
            out[dst_start..dst_start + len].copy_from_slice(&source[src_start..src_start + len]);
        }
    }
    out
}

fn choose_smooth_fill_voxel_type(atlas_data: &[u8], solid: &[bool], idx: usize, dim: UVec3) -> u8 {
    let local = volume_local_pos(idx, dim);
    let mut best_type = VOXEL_TYPE_DIRT as u8;
    let mut best_count = 0u32;
    for radius in 1..=3 {
        let mut counts = [0u32; EDIT_STATS_VOXEL_TYPE_COUNT];
        for dz in -radius..=radius {
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    let Some(neighbor) = offset_volume_index(local, IVec3::new(dx, dy, dz), dim)
                    else {
                        continue;
                    };
                    if !solid[neighbor] {
                        continue;
                    }
                    let voxel_type = voxel_type_from_atlas_byte(atlas_data[neighbor]) as usize;
                    if voxel_type < counts.len() {
                        counts[voxel_type] += 1;
                    }
                }
            }
        }

        for voxel_type in [VOXEL_TYPE_DIRT, VOXEL_TYPE_SAND, VOXEL_TYPE_ROCK] {
            let count = counts[voxel_type as usize];
            if count > best_count {
                best_count = count;
                best_type = voxel_type as u8;
            }
        }
        if best_count > 0 {
            return best_type;
        }
    }

    best_type
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atlas_pack_uses_two_moisture_bits_and_preserves_reserved_state() {
        let packed = pack_voxel_atlas_byte(VOXEL_TYPE_DIRT as u8, 7);
        assert_eq!(voxel_type_from_atlas_byte(packed), VOXEL_TYPE_DIRT as u8);
        assert_eq!(packed & VOXEL_MOISTURE_MASK, 0x30);
        assert_eq!(packed & 0xc0, 0x00);

        let old_with_reserved_state = 0b1101_0010u8;
        let refilled =
            pack_voxel_atlas_byte_for_fill(old_with_reserved_state, VOXEL_TYPE_SAND as u8);
        assert_eq!(refilled, 0b1101_0011u8);

        let cleared =
            pack_voxel_atlas_byte_for_fill(old_with_reserved_state, VOXEL_TYPE_ROCK as u8);
        assert_eq!(cleared, VOXEL_TYPE_ROCK as u8);
    }

    #[test]
    fn mbo_threshold_keeps_top_histogram_bins() {
        let histogram = [2, 3, 5, 7];
        let threshold = terrain_smooth_mbo_threshold(&histogram, 9).unwrap();
        assert_eq!(threshold.bin, 2);
        assert_eq!(threshold.tie_keep_count, 2);
        assert_eq!(threshold.tie_bin_count, 5);
    }

    #[test]
    fn mbo_threshold_handles_full_top_bin() {
        let histogram = [0, 4, 0, 6];
        let threshold = terrain_smooth_mbo_threshold(&histogram, 6).unwrap();
        assert_eq!(threshold.bin, 3);
        assert_eq!(threshold.tie_keep_count, 6);
        assert_eq!(threshold.tie_hash_limit, u32::MAX);
    }
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
