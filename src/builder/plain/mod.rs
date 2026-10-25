#![allow(clippy::items_after_test_module)]

mod atlas_readback;
mod resources;
use crate::generated::gpu_structs::{
    BvhNodes, ChunkModifyInfo, ChunkSolidSampleInfo, Cuboids, ModelVoxelizeInfo,
    PushConstantChunkHeightmap, PushConstantChunkModifySample, RegionInfo, RoundCones, Spheres,
    Toruses, VoxelPropertySampleInfo as VoxelPropertySampleInfoGpu,
    VoxelPropertySampleResult as VoxelPropertySampleResultGpu,
};
use crate::geom::{BvhNode, Cuboid, RoundCone, Sphere, Torus, UAabb3};
use anyhow::{Context, Result};
use bytemuck::{Pod, Zeroable};
use glam::{IVec3, UVec3, Vec3};
use re_flora_vkn::execute_one_time_command;
use re_flora_vkn::execute_one_time_gpu_job;
use re_flora_vkn::vk;
use re_flora_vkn::Allocator;
use re_flora_vkn::Buffer;
use re_flora_vkn::BufferUsage;
use re_flora_vkn::BufferUse;
use re_flora_vkn::ClearValue;
use re_flora_vkn::ColorClearValue;
use re_flora_vkn::CommandBuffer;
use re_flora_vkn::ComputePipeline;
use re_flora_vkn::DescriptorPool;
use re_flora_vkn::DescriptorUpdate;
use re_flora_vkn::Extent3D;
use re_flora_vkn::GpuJobToken;
use re_flora_vkn::MemoryLocation;
use re_flora_vkn::ShaderModule;
use re_flora_vkn::TextureLayout;
use re_flora_vkn::TextureRegion;
use re_flora_vkn::VulkanContext;
pub use resources::*;
use std::convert::TryInto;
use std::time::{Duration, Instant};

pub const VOXEL_TYPE_GLASS: u32 = 1;
pub const VOXEL_TYPE_ASPHALT: u32 = 12;
pub const VOXEL_TYPE_PAINTED_METAL: u32 = 13;
pub const VOXEL_TYPE_TERRACOTTA: u32 = 14;
pub const VOXEL_TYPE_CANOPY: u32 = 15;
pub const VOXEL_TYPE_CHERRY_WOOD: u32 = 5;
pub const VOXEL_TYPE_OAK_WOOD: u32 = 6;
pub const VOXEL_TYPE_ROCK: u32 = 7;
pub const VOXEL_TYPE_EMISSIVE: u32 = 8;
pub const VOXEL_TYPE_LIMESTONE: u32 = 9;
pub const VOXEL_TYPE_IVY: u32 = 10;
pub const VOXEL_TYPE_PETAL: u32 = 11;
pub const VOXEL_TYPE_EMPTY: u32 = 0;
pub const VOXEL_TYPE_DIRT: u32 = 2;
pub const VOXEL_TYPE_SAND: u32 = 3;
pub const VOXEL_TYPE_STUCCO: u32 = 4;
pub const VOXEL_TYPE_MASK: u8 = 0x0f;
pub const VOXEL_FIXED_SCENE_BIT: u8 = 0x80;
/// Derived ordinary-grass coverage on Dirt only; does not change its material.
pub const VOXEL_GRASS_COVER_BIT: u8 = 0x40;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TerrainGeneration {
    #[default]
    Procedural,
    Garden(u32),
    Flat,
}

pub(crate) const FLAT_TERRAIN_HEIGHT: f32 = 0.5;

#[derive(Clone, Copy)]
#[repr(u32)]
enum AtlasFillPolicy {
    PreserveState = 0,
    ClearState = 1,
    FixedScene = 2,
}

pub fn voxel_is_fixed_scene(data: u8) -> bool {
    let ty = u32::from(data & VOXEL_TYPE_MASK);
    ty != VOXEL_TYPE_EMPTY
        && ty != VOXEL_TYPE_DIRT
        && ty != VOXEL_TYPE_SAND
        && data & VOXEL_FIXED_SCENE_BIT != 0
}
// Atlas bytes retain 4 type bits and 2 moisture bits. Bit 6 caches grass cover on Dirt;
// bit 7 remains reserved for soil;
// bit 7 is fixed-scene ownership on non-soil types only.
pub const VOXEL_MOISTURE_MASK: u8 = 0x30;
pub const VOXEL_MOISTURE_MAX: u8 = 0x03;
pub const VOXEL_PROPERTY_MOISTURE: u32 = 1;
pub const VOXEL_PROPERTY_SOIL_TARGET_MASK: u32 = (1 << VOXEL_TYPE_DIRT) | (1 << VOXEL_TYPE_SAND);
const PRIMITIVE_KIND_ROUND_CONE: u32 = 0;
const PRIMITIVE_KIND_CUBOID: u32 = 1;
const PRIMITIVE_KIND_SPHERE: u32 = 2;
const PRIMITIVE_KIND_TORUS: u32 = 3;
pub const EDIT_STATS_VOXEL_TYPE_COUNT: usize = 16;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct TerrainMoistureDryPushConstants {
    offset: [u32; 4],
    dim: [u32; 4],
    dry_params: [f32; 4],
    sun_dir_params: [f32; 4],
    shadow_params: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct TerrainMoistureSpreadPushConstants {
    offset: [u32; 4],
    dim: [u32; 4],
    spread_params: [f32; 4],
    phase_params: [u32; 4],
}

pub(crate) const EDIT_REMOVAL_CANDIDATE_CAPACITY: u64 = 65_536;
pub(crate) const CHUNK_SOLID_SAMPLE_CAPACITY: u64 = 65_536;
const EDIT_REMOVAL_SAMPLE_COUNT: usize = 50;

pub(crate) fn voxel_type_from_atlas_byte(voxel_data: u8) -> u8 {
    voxel_data & VOXEL_TYPE_MASK
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
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
struct TerrainSoilBrushPushConstants {
    offset: [u32; 4],
    dim: [u32; 4],
    start_radius: [f32; 4],
    end_amount: [f32; 4],
    wet_patch: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
pub struct VoxelPropertySampleRequest {
    pub center: Vec3,
    pub radius_world: f32,
    pub property_id: u32,
    pub target_voxel_mask: u32,
}

impl VoxelPropertySampleRequest {
    pub fn soil_moisture(center: Vec3, radius_world: f32) -> Self {
        Self {
            center,
            radius_world,
            property_id: VOXEL_PROPERTY_MOISTURE,
            target_voxel_mask: VOXEL_PROPERTY_SOIL_TARGET_MASK,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct VoxelPropertySampleStats {
    pub count: u32,
    pub sum: u32,
}

impl VoxelPropertySampleStats {
    pub fn average(&self) -> Option<f32> {
        (self.count > 0).then(|| self.sum as f32 / self.count as f32)
    }
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
    terrain_moisture_brush_ppl: ComputePipeline,
    terrain_soil_mix_ppl: ComputePipeline,
    terrain_moisture_dry_ppl: ComputePipeline,
    terrain_moisture_spread_ppl: ComputePipeline,
    voxel_property_sample_ppl: ComputePipeline,
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
    next_soil_mix_dither_seed: u32,
    #[allow(dead_code)]
    chunk_atlas_readback_buffer: Option<Buffer>,
    soil_sampling_hint: crate::terrain_deposition::DepositionSamplingHint,
}

impl PlainBuilder {
    pub fn new(
        vulkan_ctx: VulkanContext,
        allocator: Allocator,
        plain_atlas_dim: UVec3,
        free_atlas_dim: UVec3,
    ) -> Self {
        let device = vulkan_ctx.device();

        let buffer_setup_sm = ShaderModule::from_precompiled(
            device,
            "shader/builder/chunk_writer/buffer_setup.comp",
            "main",
        )
        .unwrap();
        let chunk_init_sm = ShaderModule::from_precompiled(
            device,
            "shader/builder/chunk_writer/chunk_init.comp",
            "main",
        )
        .unwrap();
        let chunk_modify_sm = ShaderModule::from_precompiled(
            device,
            "shader/builder/chunk_writer/chunk_modify.comp",
            "main",
        )
        .unwrap();
        let chunk_modify_sample_sm = ShaderModule::from_precompiled(
            device,
            "shader/builder/chunk_writer/chunk_modify_sample.comp",
            "main",
        )
        .unwrap();
        let chunk_solid_sample_sm = ShaderModule::from_precompiled(
            device,
            "shader/builder/chunk_writer/chunk_solid_sample.comp",
            "main",
        )
        .unwrap();
        let model_voxelize_sm = ShaderModule::from_precompiled(
            device,
            "shader/builder/chunk_writer/model_voxelize.comp",
            "main",
        )
        .unwrap();
        let heightmap_sm = ShaderModule::from_precompiled(
            device,
            "shader/builder/chunk_writer/chunk_heightmap.comp",
            "main",
        )
        .unwrap();
        let terrain_moisture_brush_sm = ShaderModule::from_precompiled(
            device,
            "shader/builder/chunk_writer/terrain_moisture_brush.comp",
            "main",
        )
        .unwrap();
        let terrain_soil_mix_sm = ShaderModule::from_precompiled(
            device,
            "shader/builder/chunk_writer/terrain_soil_mix.comp",
            "main",
        )
        .unwrap();
        let terrain_moisture_dry_sm = ShaderModule::from_precompiled(
            device,
            "shader/builder/chunk_writer/terrain_moisture_dry.comp",
            "main",
        )
        .unwrap();
        let terrain_moisture_spread_sm = ShaderModule::from_precompiled(
            device,
            "shader/builder/chunk_writer/terrain_moisture_spread.comp",
            "main",
        )
        .unwrap();
        let voxel_property_sample_sm = ShaderModule::from_precompiled(
            device,
            "shader/builder/chunk_writer/voxel_property_sample.comp",
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
            &voxel_property_sample_sm,
        );

        let pool = DescriptorPool::new(device).unwrap();

        let buffer_setup_ppl = ComputePipeline::new(device, &buffer_setup_sm, &pool, &[&resources]);
        let chunk_init_ppl = ComputePipeline::new(device, &chunk_init_sm, &pool, &[&resources]);
        let heightmap_ppl = ComputePipeline::new(device, &heightmap_sm, &pool, &[&resources]);
        let terrain_moisture_brush_ppl =
            ComputePipeline::new(device, &terrain_moisture_brush_sm, &pool, &[&resources]);
        let terrain_soil_mix_ppl =
            ComputePipeline::new(device, &terrain_soil_mix_sm, &pool, &[&resources]);
        let terrain_moisture_dry_ppl =
            ComputePipeline::new_uninitialized(device, &terrain_moisture_dry_sm, &pool);
        let terrain_moisture_spread_ppl =
            ComputePipeline::new(device, &terrain_moisture_spread_sm, &pool, &[&resources]);
        let voxel_property_sample_ppl =
            ComputePipeline::new(device, &voxel_property_sample_sm, &pool, &[&resources]);
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
            &resources.region_info,
            &resources.region_indirect,
            &heightmap_ppl,
            &buffer_setup_ppl,
            &chunk_init_ppl,
            plain_atlas_dim,
            TerrainGeneration::Procedural,
        );

        return Self {
            vulkan_ctx,
            resources,
            plain_atlas_dim,
            buffer_setup_ppl,
            chunk_init_ppl,
            heightmap_ppl,
            terrain_moisture_brush_ppl,
            terrain_soil_mix_ppl,
            terrain_moisture_dry_ppl,
            terrain_moisture_spread_ppl,
            voxel_property_sample_ppl,
            chunk_modify_ppl,
            chunk_modify_sample_ppl,
            chunk_solid_sample_ppl,
            model_voxelize_ppl,
            pool,
            build_cmdbuf,
            next_edit_sample_seed: 1,
            next_moisture_dither_seed: 1,
            next_soil_mix_dither_seed: 1,
            chunk_atlas_readback_buffer: None,
            soil_sampling_hint: Default::default(),
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
        region_info: &Buffer,
        region_indirect: &Buffer,
        heightmap_ppl: &ComputePipeline,
        buffer_setup_ppl: &ComputePipeline,
        chunk_init_ppl: &ComputePipeline,
        dispatch_dim: UVec3,
        terrain: TerrainGeneration,
    ) -> CommandBuffer {
        let cmdbuf = CommandBuffer::new(vulkan_ctx.device(), vulkan_ctx.command_pool());
        cmdbuf.begin(false);

        cmdbuf.use_buffer(region_info, BufferUse::HostWrite);

        heightmap_ppl.record(
            &cmdbuf,
            Extent3D {
                width: dispatch_dim.x,
                height: dispatch_dim.z,
                depth: 1,
            },
            Some(bytemuck::bytes_of(&PushConstantChunkHeightmap {
                flat_terrain: u32::from(terrain == TerrainGeneration::Flat),
                flat_height: FLAT_TERRAIN_HEIGHT,
                ..PushConstantChunkHeightmap::zeroed()
            })),
        );

        buffer_setup_ppl.record(
            &cmdbuf,
            Extent3D {
                width: 1,
                height: 1,
                depth: 1,
            },
            None,
        );

        chunk_init_ppl.record_indirect(&cmdbuf, region_indirect, None);

        cmdbuf.end();
        cmdbuf
    }

    pub fn voxel_dimensions(&self) -> UVec3 {
        chunk_atlas_dim(&self.resources)
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

    pub fn read_chunk_atlas_region(
        &mut self,
        atlas_offset: UVec3,
        atlas_dim: UVec3,
    ) -> Result<Vec<u8>> {
        self.read_chunk_atlas_regions(&[(atlas_offset, atlas_dim)])?
            .pop()
            .context("single atlas read must return one region")
    }

    /// Fresh authoritative snapshots in request order, with one GPU transfer/wait.
    /// All bounds are checked before submission; overlapping source regions are
    /// independent packed copies, never an enlarged protection/undo volume.
    pub fn read_chunk_atlas_regions(&mut self, regions: &[(UVec3, UVec3)]) -> Result<Vec<Vec<u8>>> {
        let atlas_extent = self.resources.chunk_atlas.get_image().get_desc().extent;
        let atlas_size = UVec3::new(atlas_extent.width, atlas_extent.height, atlas_extent.depth);
        let plan = atlas_readback::AtlasReadPlan::new(regions, atlas_size)?;
        if plan.copies.is_empty() {
            return plan.read_results(|_, _| unreachable!("empty atlas batch has no host reads"));
        }
        let byte_count = plan.byte_count;
        self.ensure_chunk_atlas_readback_buffer(byte_count);

        let queue = self.vulkan_ctx.get_general_queue();
        let command_pool = self.vulkan_ctx.command_pool();
        let chunk_atlas = self.resources.chunk_atlas.get_image();
        let buffer = self
            .chunk_atlas_readback_buffer
            .as_mut()
            .expect("chunk atlas readback buffer should be allocated");
        let transfer_start = Instant::now();
        chunk_atlas.copy_image_regions_to_buffer(
            buffer,
            &queue,
            command_pool,
            TextureLayout::GENERAL,
            0,
            &plan.copies,
        );
        let transfer_ms = transfer_start.elapsed().as_secs_f64() * 1000.;
        let copy_start = Instant::now();
        let data = plan.read_results(|offset, count| buffer.read_back_range(offset, count))?;
        let report = if std::env::var_os("RE_FLORA_SOIL_PERF_REPORT").is_some() {
            Some("SOIL_READ")
        } else if std::env::var_os("RE_FLORA_PAVEMENT_PERF_REPORT").is_some() {
            Some("ATLAS_READ")
        } else {
            None
        };
        if let Some(report) = report {
            log::info!("[PERF][{report}] transfer_wait_ms={transfer_ms:.3} host_copy_ms={:.3} bytes={byte_count} regions={}", copy_start.elapsed().as_secs_f64() * 1000., plan.copies.len());
        }
        Ok(data)
    }

    /// Coverage is derived from restored flora, not authoritative saved terrain.
    pub fn restore_chunk_atlas_region(
        &mut self,
        offset: UVec3,
        dim: UVec3,
        mut bytes: Vec<u8>,
    ) -> Result<()> {
        for byte in &mut bytes {
            if *byte & VOXEL_TYPE_MASK == VOXEL_TYPE_DIRT as u8 {
                *byte &= !VOXEL_GRASS_COVER_BIT;
            }
        }
        self.write_chunk_atlas_region(offset, dim, &bytes)
    }

    pub fn write_chunk_atlas_region(
        &mut self,
        atlas_offset: UVec3,
        atlas_dim: UVec3,
        data: &[u8],
    ) -> Result<()> {
        let atlas_extent = self.resources.chunk_atlas.get_image().get_desc().extent;
        let atlas_size = UVec3::new(atlas_extent.width, atlas_extent.height, atlas_extent.depth);
        if atlas_dim.x == 0 || atlas_dim.y == 0 || atlas_dim.z == 0 {
            anyhow::ensure!(data.is_empty(), "empty atlas region requires empty data");
            return Ok(());
        }
        if (atlas_offset + atlas_dim).cmpgt(atlas_size).any() {
            anyhow::bail!(
                "chunk atlas write outside bounds: offset={:?} dim={:?} atlas={:?}",
                atlas_offset,
                atlas_dim,
                atlas_size
            );
        }
        let expected_byte_count =
            atlas_dim.x as usize * atlas_dim.y as usize * atlas_dim.z as usize;
        anyhow::ensure!(
            data.len() == expected_byte_count,
            "chunk atlas write has {} bytes, expected {}",
            data.len(),
            expected_byte_count
        );

        let queue = self.vulkan_ctx.get_general_queue();
        let command_pool = self.vulkan_ctx.command_pool();
        self.resources.chunk_atlas.get_image().fill_with_raw_u8(
            &queue,
            command_pool,
            TextureRegion {
                offset: [
                    atlas_offset.x as i32,
                    atlas_offset.y as i32,
                    atlas_offset.z as i32,
                ],
                extent: Extent3D::new(atlas_dim.x, atlas_dim.y, atlas_dim.z),
            },
            data,
            0,
            Some(TextureLayout::GENERAL),
        )
    }

    /// Plan against authoritative 3D occupancy, then upload one bounded deposition batch.
    /// This does not replace any old material or soil properties.
    pub(crate) fn deposit_soil(
        &mut self,
        request: crate::terrain_deposition::DepositionRequest,
        legal: UAabb3,
        support_floor: Option<u32>,
        fast_path: bool,
    ) -> Result<crate::terrain_deposition::DepositionResult> {
        let start = Instant::now();
        let Some(mut sample) = request.sampling_bounds(legal) else {
            return Ok(crate::terrain_deposition::DepositionResult::default());
        };
        let mut halo = request.horizontal_halo();
        if fast_path {
            halo = self.soil_sampling_hint.initial_halo(request, legal);
            sample = request
                .sampling_bounds_with_halo(legal, halo)
                .context("soil reach hint produced invalid bounds")?;
        } else {
            self.soil_sampling_hint.reset();
        }
        let mut sampled_height = if fast_path {
            request.initial_sampling_height(legal)
        } else {
            legal.max().y - legal.min().y
        };
        sample = UAabb3::new(
            sample.min(),
            UVec3::new(
                sample.max().x,
                sample.min().y + sampled_height,
                sample.max().z,
            ),
        );
        let mut expansions = 0;
        let profile = std::env::var_os("RE_FLORA_SOIL_PERF_REPORT").is_some();
        let mut read_ms = 0.;
        let mut plan_ms = 0.;
        let mut crop_ms = 0.;
        let mut upload_ms = 0.;
        let mut dirty_ms = 0.;
        let mut read_bytes = 0_usize;
        let (data, dim, mut result) = loop {
            let dim = sample.max() - sample.min();
            let stage = Instant::now();
            let mut data = self.read_chunk_atlas_region(sample.min(), dim)?;
            read_ms += stage.elapsed().as_secs_f64() * 1000.;
            read_bytes += data.len();
            let stage = Instant::now();
            let plan = if fast_path {
                crate::terrain_deposition::deposit_in_bounds_fast
            } else {
                crate::terrain_deposition::deposit_in_bounds
            };
            let result = plan(&mut data, sample.min(), dim, support_floor, request, legal);
            plan_ms += stage.elapsed().as_secs_f64() * 1000.;
            if !result.needs_more_space && !result.needs_more_height {
                break (data, dim, result);
            }
            // No GPU write/inventory debit has happened. Re-read the original atlas and
            // replay the same seeded request with more space, preserving exact ownership.
            if result.needs_more_space {
                halo *= 2.;
            }
            if result.needs_more_height {
                sampled_height = (sampled_height * 2).min(legal.max().y - legal.min().y);
            }
            let horizontal = request
                .sampling_bounds_with_halo(legal, halo)
                .context("soil sampling expansion produced invalid bounds")?;
            let next = UAabb3::new(
                horizontal.min(),
                UVec3::new(
                    horizontal.max().x,
                    horizontal.min().y + sampled_height,
                    horizontal.max().z,
                ),
            );
            anyhow::ensure!(
                next != sample,
                "soil requested space outside fully sampled legal bounds"
            );
            sample = next;
            expansions += 1;
        };
        result.sampling_halo = halo;
        if fast_path {
            self.soil_sampling_hint.remember(request, legal, halo);
        }
        if let Some(changed) = result.changed {
            let write_dim = changed.max() - changed.min() + UVec3::ONE;
            let stage = Instant::now();
            let write_data =
                extract_volume_region_u8(&data, dim, changed.min() - sample.min(), write_dim);
            crop_ms = stage.elapsed().as_secs_f64() * 1000.;
            let stage = Instant::now();
            self.write_chunk_atlas_region(changed.min(), write_dim, &write_data)?;
            upload_ms = stage.elapsed().as_secs_f64() * 1000.;
            let stage = Instant::now();
            self.mark_all_solid_workgroups_dirty();
            dirty_ms = stage.elapsed().as_secs_f64() * 1000.;
        }
        if profile {
            log::info!("[PERF][SOIL_STAGE] read_ms={read_ms:.3} plan_ms={plan_ms:.3} crop_ms={crop_ms:.3} upload_ms={upload_ms:.3} dirty_ms={dirty_ms:.3} read_bytes={read_bytes} expansions={expansions}");
        }
        log::info!(
            "[SOIL_DEPOSITION] requested={} added={} sample={:?} expanded_regions={} changed={:?} total={:.2}ms",
            request.volume,
            result.added,
            dim,
            expansions,
            result.changed,
            start.elapsed().as_secs_f64() * 1000.0
        );
        Ok(result)
    }

    pub fn mark_all_solid_workgroups_dirty(&self) {
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

    fn prepare_terrain_moisture_brush(
        &mut self,
        start: Vec3,
        end: Vec3,
        radius_world: f32,
        amount: f32,
        directional_pair: bool,
        wet_patch_size_voxels: f32,
    ) -> Option<(TerrainSoilBrushPushConstants, UVec3, UAabb3)> {
        let atlas_dim = chunk_atlas_dim(&self.resources);
        let start_vox = start * 256.0;
        let end_vox = end * 256.0;
        let radius_vox = (radius_world * 256.0).max(0.0);
        let patch_size = if directional_pair {
            if wet_patch_size_voxels.is_finite() {
                wet_patch_size_voxels.clamp(2.0, 32.0)
            } else {
                12.0
            }
        } else {
            0.0
        };
        let amount = if directional_pair {
            amount.max(0.0)
        } else {
            amount.clamp(0.0, 1.0)
        };
        if radius_vox <= 0.0
            || amount <= 0.0
            || !amount.is_finite()
            || !start_vox.is_finite()
            || !end_vox.is_finite()
        {
            return None;
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
            return None;
        }

        let offset = clamped_min.as_uvec3();
        let dim = (clamped_max - clamped_min).as_uvec3();
        let dither_seed = self.next_moisture_dither_seed;
        self.next_moisture_dither_seed = self
            .next_moisture_dither_seed
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223)
            .max(1);
        let push_constants = TerrainSoilBrushPushConstants {
            offset: [offset.x, offset.y, offset.z, dither_seed],
            dim: [dim.x, dim.y, dim.z, u32::from(directional_pair)],
            start_radius: [start_vox.x, start_vox.y, start_vox.z, radius_vox],
            end_amount: [end_vox.x, end_vox.y, end_vox.z, amount],
            wet_patch: [patch_size, 0.0, 0.0, 0.0],
        };
        let changed_bound = UAabb3::new(offset, offset + dim - UVec3::ONE);
        Some((push_constants, dim, changed_bound))
    }

    fn record_prepared_terrain_moisture_brush(
        &self,
        cmdbuf: &CommandBuffer,
        push_constants: &TerrainSoilBrushPushConstants,
        dim: UVec3,
    ) {
        self.terrain_moisture_brush_ppl.record(
            cmdbuf,
            Extent3D::new(dim.x, dim.y, dim.z),
            Some(bytemuck::bytes_of(push_constants)),
        );
    }

    #[allow(dead_code)]
    pub fn record_terrain_moisture_brush(
        &mut self,
        cmdbuf: &CommandBuffer,
        start: Vec3,
        end: Vec3,
        radius_world: f32,
        amount: f32,
    ) -> Option<UAabb3> {
        let (push_constants, dim, changed_bound) =
            self.prepare_terrain_moisture_brush(start, end, radius_world, amount, false, 0.0)?;
        self.record_prepared_terrain_moisture_brush(cmdbuf, &push_constants, dim);
        Some(changed_bound)
    }

    pub fn record_sprinkler_moisture_brush(
        &mut self,
        cmdbuf: &CommandBuffer,
        center: Vec3,
        axis: Vec3,
        range_world: f32,
        amount: f32,
        wet_patch_size_voxels: f32,
    ) -> Option<UAabb3> {
        let direction = Vec3::new(axis.x, 0.0, axis.z).normalize_or_zero();
        if direction == Vec3::ZERO {
            return None;
        }
        let end = center + direction * range_world;
        let (push_constants, dim, changed_bound) = self.prepare_terrain_moisture_brush(
            center,
            end,
            range_world,
            amount,
            true,
            wet_patch_size_voxels,
        )?;
        self.record_prepared_terrain_moisture_brush(cmdbuf, &push_constants, dim);
        Some(changed_bound)
    }

    #[allow(dead_code)]
    pub fn apply_terrain_moisture_brush(
        &mut self,
        start: Vec3,
        end: Vec3,
        radius_world: f32,
        amount: f32,
    ) -> Result<Option<UAabb3>> {
        let Some((push_constants, dim, changed_bound)) =
            self.prepare_terrain_moisture_brush(start, end, radius_world, amount, false, 0.0)
        else {
            return Ok(None);
        };

        execute_one_time_command(
            self.vulkan_ctx.device(),
            self.vulkan_ctx.command_pool(),
            &self.vulkan_ctx.get_general_queue(),
            |cmdbuf| {
                self.record_prepared_terrain_moisture_brush(cmdbuf, &push_constants, dim);
            },
        );

        Ok(Some(changed_bound))
    }

    fn prepare_terrain_soil_mix(
        &mut self,
        start: Vec3,
        end: Vec3,
        radius_world: f32,
        strength: f32,
    ) -> Option<(TerrainSoilBrushPushConstants, UVec3, UAabb3)> {
        let atlas_dim = chunk_atlas_dim(&self.resources);
        let start_vox = start * 256.0;
        let end_vox = end * 256.0;
        let radius_vox = (radius_world * 256.0).max(0.0);
        let strength = strength.clamp(0.0, 1.0);
        if radius_vox <= 0.0 || strength <= 0.0 || !start_vox.is_finite() || !end_vox.is_finite() {
            return None;
        }

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
            return None;
        }

        let offset = clamped_min.as_uvec3();
        let dim = (clamped_max - clamped_min).as_uvec3();
        let dither_seed = self.next_soil_mix_dither_seed;
        self.next_soil_mix_dither_seed = self
            .next_soil_mix_dither_seed
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223)
            .max(1);
        let push_constants = TerrainSoilBrushPushConstants {
            offset: [offset.x, offset.y, offset.z, dither_seed],
            dim: [dim.x, dim.y, dim.z, 0],
            start_radius: [start_vox.x, start_vox.y, start_vox.z, radius_vox],
            end_amount: [end_vox.x, end_vox.y, end_vox.z, strength],
            wet_patch: [0.0; 4],
        };
        let changed_bound = UAabb3::new(offset, offset + dim - UVec3::ONE);
        Some((push_constants, dim, changed_bound))
    }

    fn record_prepared_terrain_soil_mix(
        &self,
        cmdbuf: &CommandBuffer,
        push_constants: &TerrainSoilBrushPushConstants,
        dim: UVec3,
    ) {
        let mut phase_push_constants = *push_constants;
        for axis in 0..3 {
            for pair_parity in 0..2 {
                phase_push_constants.dim[3] = axis | (pair_parity << 2);
                self.terrain_soil_mix_ppl.record(
                    cmdbuf,
                    Extent3D::new(dim.x, dim.y, dim.z),
                    Some(bytemuck::bytes_of(&phase_push_constants)),
                );
            }
        }
    }

    #[allow(dead_code)]
    pub fn apply_terrain_soil_mix(
        &mut self,
        start: Vec3,
        end: Vec3,
        radius_world: f32,
        strength: f32,
    ) -> Result<Option<UAabb3>> {
        let Some((push_constants, dim, changed_bound)) =
            self.prepare_terrain_soil_mix(start, end, radius_world, strength)
        else {
            return Ok(None);
        };

        execute_one_time_command(
            self.vulkan_ctx.device(),
            self.vulkan_ctx.command_pool(),
            &self.vulkan_ctx.get_general_queue(),
            |cmdbuf| {
                self.record_prepared_terrain_soil_mix(cmdbuf, &push_constants, dim);
            },
        );

        Ok(Some(changed_bound))
    }

    /// Initializes the terrain moisture/dry descriptor set during app construction.
    ///
    /// This is intentionally a creation-time operation; runtime frame code must not rewrite this
    /// pipeline's descriptor generation in place.
    pub fn initialize_terrain_moisture_dry_resources(
        &self,
        daylight: &dyn re_flora_vkn::ResourceContainer,
        surface: &crate::builder::ContreeBuilderResources,
    ) -> Result<()> {
        self.terrain_moisture_dry_ppl
            .initialize_descriptors(DescriptorUpdate::All(&[daylight, &self.resources, surface]))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_terrain_moisture_dry_region(
        &mut self,
        cmdbuf: &CommandBuffer,
        atlas_offset: UVec3,
        atlas_dim: UVec3,
        dry_probability: f32,
        sun_dir: Vec3,
        sunlit_probability_multiplier: f32,
        residual_probability_multiplier: f32,
        voxels_per_world_unit: f32,
        direct_shadow_source_mask: u32,
        direct_shadow_available_mask: u32,
        surface_leaf_count: u32,
        surface_leaf_chunk_info_index: u32,
    ) -> bool {
        let chunk_atlas_dim = chunk_atlas_dim(&self.resources);
        let dry_probability = dry_probability.clamp(0.0, 1.0);
        if dry_probability <= 0.0
            || surface_leaf_count == 0
            || atlas_dim == UVec3::ZERO
            || chunk_atlas_dim == UVec3::ZERO
        {
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

        let sun_dir = if sun_dir.is_finite() && sun_dir.length_squared() > 0.0 {
            sun_dir.normalize()
        } else {
            Vec3::ZERO
        };
        let sunlit_probability_multiplier = sunlit_probability_multiplier.max(1.0);
        let residual_probability_multiplier = residual_probability_multiplier.max(1.0);

        let dither_seed = self.next_moisture_dither_seed;
        self.next_moisture_dither_seed = self
            .next_moisture_dither_seed
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223)
            .max(1);
        let push_constants = TerrainMoistureDryPushConstants {
            offset: [atlas_offset.x, atlas_offset.y, atlas_offset.z, dither_seed],
            dim: [surface_leaf_count, surface_leaf_chunk_info_index, 0, 0],
            dry_params: [
                dry_probability,
                residual_probability_multiplier,
                voxels_per_world_unit.max(1.0),
                0.0,
            ],
            sun_dir_params: [
                sun_dir.x,
                sun_dir.y,
                sun_dir.z,
                sunlit_probability_multiplier,
            ],
            shadow_params: [
                direct_shadow_source_mask,
                direct_shadow_available_mask,
                0,
                0,
            ],
        };

        self.terrain_moisture_dry_ppl.record(
            cmdbuf,
            Extent3D::new(surface_leaf_count, 1, 1),
            Some(bytemuck::bytes_of(&push_constants)),
        );

        true
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_terrain_moisture_spread_region(
        &mut self,
        cmdbuf: &CommandBuffer,
        atlas_offset: UVec3,
        atlas_dim: UVec3,
        spread_probability: f32,
        mobility_exponent: f32,
        downward_multiplier: f32,
        upward_multiplier: f32,
        axis: u32,
        pair_parity: u32,
    ) -> bool {
        let chunk_atlas_dim = chunk_atlas_dim(&self.resources);
        let spread_probability = spread_probability.clamp(0.0, 1.0);
        let mobility_exponent = mobility_exponent.max(1.0);
        let downward_multiplier = downward_multiplier.max(0.0);
        let upward_multiplier = upward_multiplier.max(0.0);
        if spread_probability <= 0.0 || atlas_dim == UVec3::ZERO || chunk_atlas_dim == UVec3::ZERO {
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
                "Skipping terrain moisture spread region outside atlas: offset={:?} dim={:?} atlas={:?}",
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
        let push_constants = TerrainMoistureSpreadPushConstants {
            offset: [atlas_offset.x, atlas_offset.y, atlas_offset.z, dither_seed],
            dim: [atlas_dim.x, atlas_dim.y, atlas_dim.z, 0],
            spread_params: [
                spread_probability,
                mobility_exponent,
                downward_multiplier,
                upward_multiplier,
            ],
            phase_params: [axis.min(2), pair_parity & 1, 0, 0],
        };

        self.terrain_moisture_spread_ppl.record(
            cmdbuf,
            Extent3D::new(atlas_dim.x, atlas_dim.y, atlas_dim.z),
            Some(bytemuck::bytes_of(&push_constants)),
        );

        true
    }

    pub fn sample_voxel_property_sphere(
        &mut self,
        request: VoxelPropertySampleRequest,
    ) -> Result<VoxelPropertySampleStats> {
        let atlas_dim = chunk_atlas_dim(&self.resources);
        let center_vox = request.center * 256.0;
        let radius_vox = (request.radius_world * 256.0).max(0.0);
        if radius_vox <= 0.0
            || !center_vox.is_finite()
            || request.property_id == 0
            || request.target_voxel_mask == 0
        {
            return Ok(VoxelPropertySampleStats::default());
        }

        let Some((offset, dim)) = clipped_voxel_box(center_vox, radius_vox, atlas_dim) else {
            return Ok(VoxelPropertySampleStats::default());
        };

        self.resources
            .voxel_property_sample_info
            .fill_uniform(&VoxelPropertySampleInfoGpu {
                atlas_offset_property_id: [offset.x, offset.y, offset.z, request.property_id],
                atlas_dim_target_mask: [dim.x, dim.y, dim.z, request.target_voxel_mask],
                center_radius: [center_vox.x, center_vox.y, center_vox.z, radius_vox],
                options: [0, 0, 0, 0],
            })?;

        let command_buffer =
            CommandBuffer::new(self.vulkan_ctx.device(), self.vulkan_ctx.command_pool());
        command_buffer.begin(true);
        command_buffer.use_buffer(
            &self.resources.voxel_property_sample_info,
            BufferUse::HostWrite,
        );
        self.resources.voxel_property_sample_result.record_fill(
            &command_buffer,
            0,
            self.resources.voxel_property_sample_result.get_size_bytes(),
            0,
        );
        self.voxel_property_sample_ppl.record(
            &command_buffer,
            Extent3D::new(dim.x, dim.y, dim.z),
            None,
        );
        command_buffer.use_buffer(
            &self.resources.voxel_property_sample_result,
            BufferUse::HostRead,
        );
        command_buffer.end();
        command_buffer
            .submit_gpu_job(
                &self.vulkan_ctx.get_general_queue(),
                "plain.voxel_property_sample",
            )?
            .wait_complete()?;

        let raw = self.resources.voxel_property_sample_result.read_back()?;
        let result = *bytemuck::from_bytes::<VoxelPropertySampleResultGpu>(&raw);
        let count = result.stats[0];
        Ok(VoxelPropertySampleStats {
            count,
            sum: result.stats[1],
        })
    }

    pub fn sample_soil_moisture_sphere(
        &mut self,
        center: Vec3,
        radius_world: f32,
    ) -> Result<VoxelPropertySampleStats> {
        self.sample_voxel_property_sphere(VoxelPropertySampleRequest::soil_moisture(
            center,
            radius_world,
        ))
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

        let command_buffer =
            CommandBuffer::new(self.vulkan_ctx.device(), self.vulkan_ctx.command_pool());
        let submit_start = Instant::now();
        command_buffer.begin(true);
        command_buffer.use_buffer(
            &self.resources.chunk_solid_sample_info,
            BufferUse::HostWrite,
        );
        self.chunk_solid_sample_ppl.record(
            &command_buffer,
            Extent3D::new(sample_dim.x, sample_dim.y, sample_dim.z),
            None,
        );
        command_buffer.use_buffer(&self.resources.chunk_solid_samples, BufferUse::HostRead);
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
            gpu_job,
        })
    }

    pub fn chunk_atlas_solid_grid_sample_ready(&self, job: &ChunkSolidSampleJob) -> Result<bool> {
        job.gpu_job
            .is_complete()
            .map_err(|err| anyhow::anyhow!("failed to poll chunk solid sample GPU job: {err}"))
    }

    /// Consumes a submitted solid-grid sample without reading or publishing its
    /// result.  This is the only legal shutdown path for an in-flight sample.
    pub fn discard_chunk_atlas_solid_grid_sample(
        &mut self,
        job: ChunkSolidSampleJob,
    ) -> Result<()> {
        let _completed_gpu_job = job.gpu_job.wait_complete()?;
        Ok(())
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
        self.chunk_init_with_terrain(atlas_offset, atlas_dim, TerrainGeneration::Procedural)
    }

    pub fn chunk_init_with_terrain(
        &mut self,
        atlas_offset: UVec3,
        atlas_dim: UVec3,
        terrain: TerrainGeneration,
    ) -> Result<()> {
        if atlas_dim.x == 0 || atlas_dim.y == 0 || atlas_dim.z == 0 {
            return Ok(());
        }
        if let TerrainGeneration::Garden(seed) = terrain {
            let extent = self.resources.chunk_atlas.get_image().get_desc().extent;
            let bytes = crate::garden_layout::terrain_chunk(
                atlas_offset,
                atlas_dim,
                UVec3::new(extent.width, extent.height, extent.depth),
                seed,
            );
            return self.write_chunk_atlas_region(atlas_offset, atlas_dim, &bytes);
        }
        update_buffers(&self.resources, atlas_offset, atlas_dim)?;

        // re-record the command buffer with updated descriptor sets
        self.build_cmdbuf = Self::record_build_cmdbuf(
            &self.vulkan_ctx,
            &self.resources.region_info,
            &self.resources.region_indirect,
            &self.heightmap_ppl,
            &self.buffer_setup_ppl,
            &self.chunk_init_ppl,
            atlas_dim,
            terrain,
        );

        let _completed_gpu_job = self
            .build_cmdbuf
            .clone()
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

    pub fn chunk_modify(&mut self, bvh_nodes: &[BvhNode], round_cones: &[RoundCone]) -> Result<()> {
        self.chunk_modify_with_voxel_type(bvh_nodes, round_cones, VOXEL_TYPE_CHERRY_WOOD)
    }

    pub fn chunk_modify_with_voxel_type(
        &mut self,
        bvh_nodes: &[BvhNode],
        round_cones: &[RoundCone],
        fill_voxel_type: u32,
    ) -> Result<()> {
        self.chunk_modify_round_cones_with_voxel_type(bvh_nodes, round_cones, fill_voxel_type, None)
    }

    pub fn chunk_replace_voxel_type_in_round_cones(
        &mut self,
        bvh_nodes: &[BvhNode],
        round_cones: &[RoundCone],
        target_voxel_type: u32,
        fill_voxel_type: u32,
    ) -> Result<()> {
        self.chunk_modify_round_cones_with_voxel_type(
            bvh_nodes,
            round_cones,
            fill_voxel_type,
            Some(target_voxel_type),
        )
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
        self.chunk_modify_cuboids_with_voxel_type_and_state(
            bvh_nodes,
            cuboids,
            fill_voxel_type,
            false,
        )
    }

    pub fn chunk_modify_cuboids_with_voxel_type_and_state(
        &mut self,
        bvh_nodes: &[BvhNode],
        cuboids: &[Cuboid],
        fill_voxel_type: u32,
        clear_fill_voxel_state: bool,
    ) -> Result<()> {
        self.chunk_modify_cuboids_with_voxel_type_impl(
            bvh_nodes,
            cuboids,
            fill_voxel_type,
            if clear_fill_voxel_state {
                AtlasFillPolicy::ClearState
            } else {
                AtlasFillPolicy::PreserveState
            },
        )
    }

    /// Scene authoring is the sole capability allowed to write/clear immutable non-soil cells.
    pub fn chunk_modify_fixed_scene_cuboids(
        &mut self,
        bvh_nodes: &[BvhNode],
        cuboids: &[Cuboid],
        material: u32,
    ) -> Result<()> {
        anyhow::ensure!(
            material != VOXEL_TYPE_DIRT && material != VOXEL_TYPE_SAND,
            "fixed scene cannot own soil state bits"
        );
        self.chunk_modify_cuboids_with_voxel_type_impl(
            bvh_nodes,
            cuboids,
            material,
            AtlasFillPolicy::FixedScene,
        )
    }

    pub fn chunk_modify_spheres_with_voxel_type(
        &mut self,
        bvh_nodes: &[BvhNode],
        spheres: &[Sphere],
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
            PRIMITIVE_KIND_SPHERE,
            false,
            AtlasFillPolicy::PreserveState,
            None,
            None,
            None,
        )?;
        update_spheres(&self.resources, spheres)?;
        update_trunk_bvh_nodes(&self.resources, bvh_nodes)?;

        execute_one_time_command(
            self.vulkan_ctx.device(),
            self.vulkan_ctx.command_pool(),
            &self.vulkan_ctx.get_general_queue(),
            |cmdbuf| {
                cmdbuf.use_buffer(&self.resources.chunk_modify_info, BufferUse::HostWrite);
                cmdbuf.use_buffer(&self.resources.trunk_bvh_nodes, BufferUse::HostWrite);
                cmdbuf.use_buffer(&self.resources.spheres, BufferUse::HostWrite);
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

    pub fn chunk_modify_toruses_with_voxel_type(
        &mut self,
        bvh_nodes: &[BvhNode],
        toruses: &[Torus],
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
            PRIMITIVE_KIND_TORUS,
            false,
            AtlasFillPolicy::PreserveState,
            None,
            None,
            None,
        )?;
        update_toruses(&self.resources, toruses)?;
        update_trunk_bvh_nodes(&self.resources, bvh_nodes)?;

        execute_one_time_command(
            self.vulkan_ctx.device(),
            self.vulkan_ctx.command_pool(),
            &self.vulkan_ctx.get_general_queue(),
            |cmdbuf| {
                cmdbuf.use_buffer(&self.resources.chunk_modify_info, BufferUse::HostWrite);
                cmdbuf.use_buffer(&self.resources.trunk_bvh_nodes, BufferUse::HostWrite);
                cmdbuf.use_buffer(&self.resources.toruses, BufferUse::HostWrite);
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

    /// Exact capsule with the spherical brush's readback, inventory limits and legal bounds.
    #[allow(clippy::too_many_arguments)]
    pub fn chunk_modify_surface_capsule(
        &mut self,
        bvh_nodes: &[BvhNode],
        capsule: &RoundCone,
        fill_voxel_type: u32,
        target_voxel_type: Option<u32>,
        max_write_count: Option<u32>,
        max_removed_counts: Option<[u32; EDIT_STATS_VOXEL_TYPE_COUNT]>,
        bound: Option<UAabb3>,
    ) -> Result<ChunkModifyReadback> {
        self.modify_surface_with_readback(
            bvh_nodes,
            &[],
            Some(capsule),
            fill_voxel_type,
            target_voxel_type,
            max_write_count,
            max_removed_counts,
            true,
            bound,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn modify_surface_with_readback(
        &mut self,
        bvh_nodes: &[BvhNode],
        spheres: &[Sphere],
        capsule: Option<&RoundCone>,
        fill_voxel_type: u32,
        target_voxel_type: Option<u32>,
        max_write_count: Option<u32>,
        max_removed_counts: Option<[u32; EDIT_STATS_VOXEL_TYPE_COUNT]>,
        surface_only: bool,
        bound: Option<UAabb3>,
    ) -> Result<ChunkModifyReadback> {
        let total_start = Instant::now();
        let atlas_dim = chunk_atlas_dim(&self.resources);
        let Some((mut offset, mut dim)) = calculate_clipped_offset_and_dim(bvh_nodes, atlas_dim)
        else {
            return Ok(ChunkModifyReadback::default());
        };
        if let Some(bound) = bound {
            let max = (offset + dim).min(bound.max());
            offset = offset.max(bound.min());
            if offset.cmpge(max).any() {
                return Ok(ChunkModifyReadback::default());
            }
            dim = max - offset;
        }
        let prep_start = Instant::now();
        clear_edit_stats(&self.resources)?;
        clear_edit_removal_candidates(&self.resources)?;
        update_chunk_modify_info(
            &self.resources,
            offset,
            dim,
            fill_voxel_type,
            target_voxel_type,
            if capsule.is_some() {
                PRIMITIVE_KIND_ROUND_CONE
            } else {
                PRIMITIVE_KIND_SPHERE
            },
            surface_only,
            AtlasFillPolicy::PreserveState,
            max_write_count,
            max_removed_counts,
            bound.map(|b| b.min().y),
        )?;
        if let Some(capsule) = capsule {
            update_round_cones(&self.resources, std::slice::from_ref(capsule))?;
        } else {
            update_spheres(&self.resources, spheres)?;
        }
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
        let gpu_start = Instant::now();
        execute_one_time_gpu_job(
            self.vulkan_ctx.device(),
            self.vulkan_ctx.command_pool(),
            &self.vulkan_ctx.get_general_queue(),
            |cmdbuf| {
                cmdbuf.use_buffer(&self.resources.chunk_modify_info, BufferUse::HostWrite);
                cmdbuf.use_buffer(&self.resources.trunk_bvh_nodes, BufferUse::HostWrite);
                cmdbuf.use_buffer(&self.resources.round_cones, BufferUse::HostWrite);
                cmdbuf.use_buffer(&self.resources.cuboids, BufferUse::HostWrite);
                cmdbuf.use_buffer(&self.resources.spheres, BufferUse::HostWrite);
                cmdbuf.use_buffer(&self.resources.edit_stats, BufferUse::HostWrite);
                cmdbuf.use_buffer(
                    &self.resources.edit_removal_candidates,
                    BufferUse::HostWrite,
                );
                self.chunk_modify_ppl.record(
                    cmdbuf,
                    Extent3D {
                        width: dim.x,
                        height: dim.y,
                        depth: dim.z,
                    },
                    None,
                );
                self.chunk_modify_sample_ppl.record(
                    cmdbuf,
                    Extent3D::new(EDIT_REMOVAL_SAMPLE_COUNT as u32, 1, 1),
                    Some(bytemuck::bytes_of(&sample_push)),
                );
                cmdbuf.use_buffer(&self.resources.edit_stats, BufferUse::HostRead);
                cmdbuf.use_buffer(&self.resources.edit_removal_sample, BufferUse::HostRead);
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

        let gpu_start = Instant::now();
        execute_one_time_command(
            self.vulkan_ctx.device(),
            self.vulkan_ctx.command_pool(),
            &self.vulkan_ctx.get_general_queue(),
            |cmdbuf| {
                cmdbuf.use_buffer(&self.resources.model_voxelize_info, BufferUse::HostWrite);
                cmdbuf.use_buffer(&self.resources.model_triangles, BufferUse::HostWrite);
                self.model_voxelize_ppl
                    .record(cmdbuf, Extent3D::new(dim.x, dim.y, dim.z), None);
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
        target_voxel_type: Option<u32>,
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
            target_voxel_type,
            PRIMITIVE_KIND_ROUND_CONE,
            false,
            AtlasFillPolicy::PreserveState,
            None,
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
                cmdbuf.use_buffer(&self.resources.chunk_modify_info, BufferUse::HostWrite);
                cmdbuf.use_buffer(&self.resources.trunk_bvh_nodes, BufferUse::HostWrite);
                cmdbuf.use_buffer(&self.resources.round_cones, BufferUse::HostWrite);
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
        fill_policy: AtlasFillPolicy,
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
            fill_policy,
            None,
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
                cmdbuf.use_buffer(&self.resources.chunk_modify_info, BufferUse::HostWrite);
                cmdbuf.use_buffer(&self.resources.trunk_bvh_nodes, BufferUse::HostWrite);
                cmdbuf.use_buffer(&self.resources.cuboids, BufferUse::HostWrite);
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

fn volume_index(local: UVec3, dim: UVec3) -> usize {
    ((local.z * dim.y + local.y) * dim.x + local.x) as usize
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emissive_material_fits_the_existing_four_bit_schema_and_edit_stats() {
        assert!(VOXEL_TYPE_EMISSIVE <= u32::from(VOXEL_TYPE_MASK));
        assert_eq!(EDIT_STATS_VOXEL_TYPE_COUNT, VOXEL_TYPE_MASK as usize + 1);
        assert_eq!(
            std::mem::size_of::<crate::generated::gpu_structs::EditStats>(),
            EDIT_STATS_VOXEL_TYPE_COUNT * 2 * std::mem::size_of::<u32>()
        );
        let modify = crate::generated::gpu_structs::ChunkModifyInfo::zeroed();
        assert_eq!(modify.max_removed_counts_8_11.len(), 4);
        assert_eq!(modify.max_removed_counts_12_15.len(), 4);
        for data in 0u8..=u8::MAX {
            let ty = u32::from(data & VOXEL_TYPE_MASK);
            if [VOXEL_TYPE_EMPTY, VOXEL_TYPE_DIRT, VOXEL_TYPE_SAND].contains(&ty) {
                assert!(!voxel_is_fixed_scene(data));
            }
        }
        assert!(voxel_is_fixed_scene(
            VOXEL_TYPE_GLASS as u8 | VOXEL_FIXED_SCENE_BIT
        ));
        let writer_types = include_str!("../../../shader/slang/chunk_writer_types.slang");
        assert!(writer_types.contains("EDIT_STATS_VOXEL_TYPE_COUNT = 16u"));
        assert!(writer_types.contains("max_removed_counts_8_11"));
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

#[allow(clippy::too_many_arguments)]
fn update_chunk_modify_info(
    resources: &PlainBuilderResources,
    offset: UVec3,
    dim: UVec3,
    fill_voxel_type: u32,
    target_voxel_type: Option<u32>,
    primitive_kind: u32,
    surface_only: bool,
    fill_policy: AtlasFillPolicy,
    max_write_count: Option<u32>,
    max_removed_counts: Option<[u32; EDIT_STATS_VOXEL_TYPE_COUNT]>,
    surface_support_y: Option<u32>,
) -> Result<()> {
    let max_removed_counts = max_removed_counts.unwrap_or([u32::MAX; EDIT_STATS_VOXEL_TYPE_COUNT]);
    resources.chunk_modify_info.fill_uniform(&ChunkModifyInfo {
        offset: offset.to_array(),
        dim: dim.to_array(),
        fill_voxel_type,
        target_voxel_type: target_voxel_type.unwrap_or(u32::MAX),
        primitive_kind,
        surface_only: if surface_only { 1 } else { 0 },
        surface_support_y_plus_one: surface_support_y.map_or(0, |y| y + 1),
        max_write_count: max_write_count.unwrap_or(0),
        fill_policy: fill_policy as u32,
        max_removed_counts_0_3: max_removed_counts[..4].try_into().unwrap(),
        max_removed_counts_4_7: max_removed_counts[4..8].try_into().unwrap(),
        max_removed_counts_8_11: max_removed_counts[8..12].try_into().unwrap(),
        max_removed_counts_12_15: max_removed_counts[12..16].try_into().unwrap(),
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
            center: cuboid.center().to_array(),
            half_size: cuboid.half_size().to_array(),
            inverse_rotation: cuboid.rotation().conjugate().to_array(),
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

fn update_toruses(resources: &PlainBuilderResources, toruses: &[Torus]) -> Result<()> {
    for (i, torus) in toruses.iter().enumerate() {
        let data = Toruses {
            center: torus.center().to_array(),
            major_radius: torus.major_radius(),
            inverse_rotation: torus.rotation().conjugate().to_array(),
            tube_radius: torus.tube_radius(),
        };
        resources
            .toruses
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
