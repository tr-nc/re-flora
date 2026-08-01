use super::{
    DdgiAtlasLayout, DdgiBuildToken, DdgiVolumeGrid, DDGI_IRRADIANCE_INTERIOR_SIDE,
    DDGI_PROBE_BATCH_SIZE, DDGI_RAYS_PER_PROBE, DDGI_VISIBILITY_INTERIOR_SIDE,
};
use crate::environment_lighting::DdgiRadianceSnapshot;
use crate::generated::gpu_structs::{
    DdgiProbeMetadata, DdgiRadianceSun, DdgiRadianceVoxelPalette, DdgiTransportQueryInfo,
};
use crate::resource::{Resource, ResourceContainer};
use anyhow::{ensure, Context, Result};
use bytemuck::Zeroable;
use glam::UVec3;
use re_flora_vkn::vk;
use re_flora_vkn::{
    Allocator, Buffer, BufferUsage, Extent3D, ImageDesc, MemoryLocation, SamplerDesc, Texture,
    TextureLayout, VulkanContext,
};

const DDGI_IRRADIANCE_FORMAT: vk::Format = vk::Format::R32G32B32A32_SFLOAT;
const DDGI_VISIBILITY_FORMAT: vk::Format = vk::Format::R32G32_SFLOAT;
const DDGI_TRACE_STATS_COUNT: usize = 8;
const DDGI_ATLAS_REDUCTION_COUNT: usize = 5;

/// Conservative, centralized feedback stopping policy. The relative metric uses a symmetric
/// denominator `max(abs(source), abs(destination), relative_floor)` so near-black texels do not
/// prevent convergence forever. These are transport decisions only; HDR values are never clamped.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DdgiConvergencePolicy {
    pub absolute_threshold: f32,
    pub relative_threshold: f32,
    pub relative_floor: f32,
    pub consecutive_iterations: u32,
    pub hard_max_iteration: u32,
}

pub const DDGI_CONVERGENCE_POLICY: DdgiConvergencePolicy = DdgiConvergencePolicy {
    absolute_threshold: 0.0025,
    relative_threshold: 0.02,
    relative_floor: 0.05,
    consecutive_iterations: 2,
    hard_max_iteration: 8,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum DdgiIrradianceSlot {
    Atlas0 = 0,
    Atlas1 = 1,
}

impl DdgiIrradianceSlot {
    pub fn label(self) -> &'static str {
        match self {
            Self::Atlas0 => "atlas0",
            Self::Atlas1 => "atlas1",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DdgiTransportFieldIdentity {
    pub build_token: Option<DdgiBuildToken>,
    pub geometry_revision: u32,
    pub radiance_revision: u32,
    pub spacing_voxels: u32,
    pub stage: DdgiTransportStage,
    pub iteration: u32,
    pub slot: DdgiIrradianceSlot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DdgiTransportIterationIdentity {
    pub build_token: Option<DdgiBuildToken>,
    pub geometry_revision: u32,
    pub radiance_revision: u32,
    pub spacing_voxels: u32,
    pub stage: DdgiTransportStage,
    pub iteration: u32,
    pub source: Option<DdgiTransportFieldIdentity>,
    pub destination: DdgiTransportFieldIdentity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DdgiRayBatch {
    pub first_probe_index: u32,
    pub probe_count: u32,
    pub transport: DdgiTransportIterationIdentity,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DdgiTraceStats {
    pub ray_records: u32,
    pub valid_probe_rays: u32,
    pub misses: u32,
    pub frontface_hits: u32,
    pub backface_hits: u32,
    pub non_finite_records: u32,
    pub invalid_probe_rays: u32,
}

impl DdgiTraceStats {
    fn from_array(values: [u32; DDGI_TRACE_STATS_COUNT]) -> Self {
        Self {
            ray_records: values[0],
            valid_probe_rays: values[1],
            misses: values[2],
            frontface_hits: values[3],
            backface_hits: values[4],
            non_finite_records: values[5],
            invalid_probe_rays: values[6],
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DdgiAtlasValidationStats {
    pub max_absolute_rgb_delta: f32,
    pub max_relative_rgb_delta: f32,
    pub non_finite_count: u32,
    /// Valid 8x8 interior texels included in convergence deltas.
    pub valid_texel_count: u32,
    /// Valid 10x10 stored texels checked for finite values, including gutters.
    pub scanned_stored_texel_count: u32,
}

impl DdgiAtlasValidationStats {
    fn from_array(values: [u32; DDGI_ATLAS_REDUCTION_COUNT]) -> Self {
        Self {
            max_absolute_rgb_delta: f32::from_bits(values[0]),
            max_relative_rgb_delta: f32::from_bits(values[1]),
            non_finite_count: values[2],
            valid_texel_count: values[3],
            scanned_stored_texel_count: values[4],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DdgiResourceBytes {
    pub irradiance_atlas: u64,
    pub transport_source_irradiance_atlas: u64,
    pub visibility_atlas: u64,
    pub global_sky_irradiance: u64,
    pub probe_metadata: u64,
    pub transient_ray_data: u64,
    pub trace_stats: u64,
    pub atlas_reduction: u64,
    pub radiance_sun: u64,
    pub radiance_voxel_palette: u64,
    pub transport_query_info: u64,
}

impl DdgiResourceBytes {
    pub fn for_grid(grid: DdgiVolumeGrid) -> Result<Self> {
        let irradiance_layout =
            DdgiAtlasLayout::new(grid.probe_count(), DDGI_IRRADIANCE_INTERIOR_SIDE)?;
        let visibility_layout =
            DdgiAtlasLayout::new(grid.probe_count(), DDGI_VISIBILITY_INTERIOR_SIDE)?;
        Ok(Self::new(grid, irradiance_layout, visibility_layout))
    }

    fn new(
        grid: DdgiVolumeGrid,
        irradiance_layout: DdgiAtlasLayout,
        visibility_layout: DdgiAtlasLayout,
    ) -> Self {
        let irradiance_extent = irradiance_layout.extent();
        let visibility_extent = visibility_layout.extent();
        Self {
            irradiance_atlas: irradiance_extent.x as u64
                * irradiance_extent.y as u64
                * std::mem::size_of::<[f32; 4]>() as u64,
            transport_source_irradiance_atlas: irradiance_extent.x as u64
                * irradiance_extent.y as u64
                * std::mem::size_of::<[f32; 4]>() as u64,
            visibility_atlas: visibility_extent.x as u64
                * visibility_extent.y as u64
                * std::mem::size_of::<[f32; 2]>() as u64,
            global_sky_irradiance: super::DDGI_IRRADIANCE_STORED_SIDE as u64
                * super::DDGI_IRRADIANCE_STORED_SIDE as u64
                * std::mem::size_of::<[f32; 4]>() as u64,
            probe_metadata: grid.probe_count() as u64
                * std::mem::size_of::<DdgiProbeMetadata>() as u64,
            transient_ray_data: DDGI_PROBE_BATCH_SIZE as u64
                * DDGI_RAYS_PER_PROBE as u64
                * std::mem::size_of::<[f32; 4]>() as u64,
            trace_stats: (DDGI_TRACE_STATS_COUNT * std::mem::size_of::<u32>()) as u64,
            atlas_reduction: (DDGI_ATLAS_REDUCTION_COUNT * std::mem::size_of::<u32>()) as u64,
            radiance_sun: std::mem::size_of::<DdgiRadianceSun>() as u64,
            radiance_voxel_palette: std::mem::size_of::<DdgiRadianceVoxelPalette>() as u64,
            transport_query_info: std::mem::size_of::<DdgiTransportQueryInfo>() as u64,
        }
    }

    pub fn total(self) -> u64 {
        self.irradiance_atlas
            + self.transport_source_irradiance_atlas
            + self.visibility_atlas
            + self.global_sky_irradiance
            + self.probe_metadata
            + self.transient_ray_data
            + self.trace_stats
            + self.atlas_reduction
            + self.radiance_sun
            + self.radiance_voxel_palette
            + self.transport_query_info
    }
}

/// The last complete transport state owned by a volume.
///
/// SeedSky lives in the builder's transport-source atlas while SingleBounce lives in the consumer
/// atlas. Probe batches are deliberately absent: a partial full-volume iteration is never a
/// transport state and must never become consumer-visible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum DdgiTransportStage {
    SeedSky,
    SingleBounce,
    Feedback { iteration: u32 },
    Converged { iteration: u32 },
    NonConverged { iteration: u32 },
}

impl DdgiTransportStage {
    pub fn iteration(self) -> u32 {
        match self {
            Self::SeedSky => 0,
            Self::SingleBounce => 1,
            Self::Feedback { iteration } => iteration,
            Self::Converged { iteration } | Self::NonConverged { iteration } => iteration,
        }
    }

    pub fn immutable_source(self) -> Option<Self> {
        match self {
            Self::SeedSky => None,
            Self::SingleBounce => Some(Self::SeedSky),
            Self::Feedback { iteration: 2 } => Some(Self::SingleBounce),
            Self::Feedback { iteration } if iteration > 2 => Some(Self::Feedback {
                iteration: iteration - 1,
            }),
            Self::Feedback { .. } | Self::Converged { .. } | Self::NonConverged { .. } => None,
        }
    }

    pub fn destination_slot(self) -> Option<DdgiIrradianceSlot> {
        match self {
            Self::SeedSky => Some(DdgiIrradianceSlot::Atlas1),
            Self::SingleBounce => Some(DdgiIrradianceSlot::Atlas0),
            Self::Feedback { iteration } => Some(if iteration % 2 == 0 {
                DdgiIrradianceSlot::Atlas1
            } else {
                DdgiIrradianceSlot::Atlas0
            }),
            Self::Converged { .. } | Self::NonConverged { .. } => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DdgiVerifiedBatchOutcome {
    Continue,
    AwaitingAtlasValidation(DdgiTransportIterationIdentity),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DdgiValidatedIterationOutcome {
    SeedSkyComplete,
    Published {
        field: DdgiTransportFieldIdentity,
        next: DdgiTransportStage,
        consecutive_below_threshold: u32,
    },
    Converged {
        field: DdgiTransportFieldIdentity,
        iteration: u32,
    },
    NonConverged {
        field: DdgiTransportFieldIdentity,
        iteration: u32,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub enum DdgiVolumeStage {
    #[default]
    Allocated,
    GlobalSkyReady,
    RelocationPending,
    Relocated,
    RayBatchReady,
    AtlasReady,
    Rebuilding,
    Ready,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DdgiVolumeStatus {
    pub build_token: Option<DdgiBuildToken>,
    pub grid: DdgiVolumeGrid,
    pub irradiance_layout: DdgiAtlasLayout,
    pub visibility_layout: DdgiAtlasLayout,
    pub resource_bytes: DdgiResourceBytes,
    pub stage: DdgiVolumeStage,
    pub transport_stage: Option<DdgiTransportStage>,
    pub building_transport_stage: Option<DdgiTransportStage>,
    pub complete_field: Option<DdgiTransportFieldIdentity>,
    pub published_field: Option<DdgiTransportFieldIdentity>,
    pub consecutive_below_threshold: u32,
    pub last_atlas_validation: Option<DdgiAtlasValidationStats>,
    pub global_sky_revision: u32,
    pub radiance_revision: Option<u32>,
    pub relocated_terrain_revision: Option<u32>,
    pub active_ray_batch: Option<DdgiRayBatch>,
    pub filtered_probe_count: u32,
}

impl DdgiVolumeStatus {
    pub fn is_ready(self) -> bool {
        self.published_field.is_some()
    }
}

/// The consumer-visible DDGI volume and an optional volume being built for a later promotion.
///
/// Callers can inspect revisions and readiness without learning which atlas or ray batch the
/// builder currently owns. Consumers must use `active`; builder passes must use `builder`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DdgiStatus {
    active: DdgiVolumeStatus,
    staging: Option<DdgiVolumeStatus>,
}

impl DdgiStatus {
    pub fn active(self) -> DdgiVolumeStatus {
        self.active
    }

    pub fn staging(self) -> Option<DdgiVolumeStatus> {
        self.staging
    }

    pub fn builder(self) -> DdgiVolumeStatus {
        self.staging.unwrap_or(self.active)
    }

    pub fn staging_is_ready(self) -> bool {
        self.staging().is_some_and(DdgiVolumeStatus::is_ready)
    }
}

pub struct DdgiVolume {
    build_token: Option<DdgiBuildToken>,
    grid: DdgiVolumeGrid,
    irradiance_layout: DdgiAtlasLayout,
    visibility_layout: DdgiAtlasLayout,
    resource_bytes: DdgiResourceBytes,
    stage: DdgiVolumeStage,
    transport_stage: Option<DdgiTransportStage>,
    building_transport_stage: Option<DdgiTransportStage>,
    complete_field: Option<DdgiTransportFieldIdentity>,
    published_field: Option<DdgiTransportFieldIdentity>,
    consecutive_below_threshold: u32,
    last_atlas_validation: Option<DdgiAtlasValidationStats>,
    global_sky_revision: u32,
    radiance_revision: Option<u32>,
    requested_terrain_revision: Option<u32>,
    relocated_terrain_revision: Option<u32>,
    active_ray_batch: Option<DdgiRayBatch>,
    next_probe_index: u32,
    pub ddgi_probe_metadata: Resource<Buffer>,
    pub ddgi_transient_ray_data: Resource<Buffer>,
    pub ddgi_trace_stats: Resource<Buffer>,
    ddgi_trace_stats_readback: Buffer,
    pub ddgi_atlas_reduction: Resource<Buffer>,
    ddgi_atlas_reduction_readback: Buffer,
    pub ddgi_irradiance_atlas: Resource<Texture>,
    pub ddgi_transport_source_irradiance_atlas: Resource<Texture>,
    pub ddgi_visibility_atlas: Resource<Texture>,
    pub ddgi_global_sky_irradiance: Resource<Texture>,
    pub ddgi_radiance_sun: Resource<Buffer>,
    pub ddgi_radiance_voxel_palette: Resource<Buffer>,
    pub ddgi_transport_query_info: Resource<Buffer>,
    transport_query_snapshot: DdgiTransportQueryInfo,
}

/// Owns the DDGI active/staging lifecycle.
///
/// A staging volume is never returned by [`Self::active`]. Promotion is the only operation that
/// can make it consumer-visible, and promotion rejects incomplete volumes.
pub struct DdgiVolumes {
    active: DdgiVolume,
    staging: Option<DdgiVolume>,
}

impl DdgiVolumes {
    pub fn new(active: DdgiVolume) -> Self {
        Self {
            active,
            staging: None,
        }
    }

    pub fn status(&self) -> DdgiStatus {
        DdgiStatus {
            active: self.active.status(),
            staging: self.staging.as_ref().map(DdgiVolume::status),
        }
    }

    pub fn active(&self) -> &DdgiVolume {
        &self.active
    }

    pub fn builder(&self) -> &DdgiVolume {
        self.staging.as_ref().unwrap_or(&self.active)
    }

    pub fn builder_mut(&mut self) -> &mut DdgiVolume {
        self.staging.as_mut().unwrap_or(&mut self.active)
    }

    pub fn builder_is_active(&self) -> bool {
        self.staging.is_none()
    }

    /// Installs a new builder target while returning the previous staging volume, if any.
    /// The caller must rebind builder descriptors before dropping the returned volume.
    pub fn prepare_staging(&mut self, staging: DdgiVolume) -> Option<DdgiVolume> {
        self.staging.replace(staging)
    }

    /// Promotes a complete staging volume and returns the previous active volume.
    /// The caller must rebind consumer descriptors before dropping the returned volume.
    pub fn promote_staging(&mut self, expected_token: DdgiBuildToken) -> Result<DdgiVolume> {
        let staging = self
            .staging
            .as_ref()
            .context("cannot promote DDGI staging volume: no staging volume exists")?;
        ensure!(
            staging.status().is_ready(),
            "cannot promote DDGI staging volume before it is ready (stage={:?})",
            staging.status().stage,
        );
        ensure!(
            staging.status().build_token == Some(expected_token),
            "cannot promote DDGI staging volume with token {:?}; expected {:?}",
            staging.status().build_token,
            expected_token,
        );
        let staging = self.staging.take().expect("staging presence checked above");
        Ok(std::mem::replace(&mut self.active, staging))
    }
}

impl ResourceContainer for DdgiVolume {
    fn get_buffer(&self, name: &str) -> Option<&Buffer> {
        match name {
            "ddgi_probe_metadata" => Some(&self.ddgi_probe_metadata),
            "ddgi_transient_ray_data" => Some(&self.ddgi_transient_ray_data),
            "ddgi_trace_stats" => Some(&self.ddgi_trace_stats),
            "ddgi_atlas_reduction" => Some(&self.ddgi_atlas_reduction),
            "ddgi_radiance_sun" => Some(&self.ddgi_radiance_sun),
            "ddgi_radiance_voxel_palette" => Some(&self.ddgi_radiance_voxel_palette),
            "ddgi_transport_query_info" => Some(&self.ddgi_transport_query_info),
            _ => None,
        }
    }

    fn get_texture(&self, name: &str) -> Option<&Texture> {
        match name {
            "ddgi_irradiance_atlas" => Some(&self.ddgi_irradiance_atlas),
            "ddgi_transport_source_irradiance_atlas" => {
                Some(&self.ddgi_transport_source_irradiance_atlas)
            }
            "ddgi_visibility_atlas" => Some(&self.ddgi_visibility_atlas),
            "ddgi_global_sky_irradiance" => Some(&self.ddgi_global_sky_irradiance),
            _ => None,
        }
    }

    fn get_resource_names(&self) -> Vec<&'static str> {
        vec![
            "ddgi_probe_metadata",
            "ddgi_transient_ray_data",
            "ddgi_trace_stats",
            "ddgi_atlas_reduction",
            "ddgi_radiance_sun",
            "ddgi_radiance_voxel_palette",
            "ddgi_transport_query_info",
            "ddgi_irradiance_atlas",
            "ddgi_transport_source_irradiance_atlas",
            "ddgi_visibility_atlas",
            "ddgi_global_sky_irradiance",
        ]
    }
}

impl DdgiVolume {
    pub fn new(
        vulkan_ctx: &VulkanContext,
        allocator: Allocator,
        world_extent_voxels: UVec3,
        spacing_voxels: u32,
        voxels_per_world_unit: UVec3,
    ) -> Result<Self> {
        let grid = DdgiVolumeGrid::new(world_extent_voxels, spacing_voxels)?;
        let irradiance_layout =
            DdgiAtlasLayout::new(grid.probe_count(), DDGI_IRRADIANCE_INTERIOR_SIDE)?;
        let visibility_layout =
            DdgiAtlasLayout::new(grid.probe_count(), DDGI_VISIBILITY_INTERIOR_SIDE)?;
        debug_assert_eq!(
            visibility_layout.stored_side(),
            super::atlas::DDGI_VISIBILITY_STORED_SIDE
        );
        let resource_bytes = DdgiResourceBytes::new(grid, irradiance_layout, visibility_layout);

        let physical_device_properties = unsafe {
            vulkan_ctx
                .instance()
                .as_raw()
                .get_physical_device_properties(vulkan_ctx.physical_device().as_raw())
        };
        let max_image_dimension = physical_device_properties.limits.max_image_dimension2_d;
        for (name, extent) in [
            ("irradiance", irradiance_layout.extent()),
            ("visibility", visibility_layout.extent()),
        ] {
            ensure!(
                extent.max_element() <= max_image_dimension,
                "DDGI {name} atlas {}x{} exceeds device 2D texture limit {max_image_dimension}",
                extent.x,
                extent.y
            );
        }

        let device = vulkan_ctx.device().clone();
        let sampled_storage_usage = vk::ImageUsageFlags::SAMPLED
            | vk::ImageUsageFlags::STORAGE
            | vk::ImageUsageFlags::TRANSFER_DST;
        let sampler_desc = SamplerDesc {
            mag_filter: vk::Filter::LINEAR,
            min_filter: vk::Filter::LINEAR,
            ..Default::default()
        };

        let irradiance_desc = atlas_image_desc(
            irradiance_layout,
            DDGI_IRRADIANCE_FORMAT,
            sampled_storage_usage,
        );
        let visibility_desc = atlas_image_desc(
            visibility_layout,
            DDGI_VISIBILITY_FORMAT,
            sampled_storage_usage,
        );
        let global_sky_layout = DdgiAtlasLayout::new(1, DDGI_IRRADIANCE_INTERIOR_SIDE)?;
        let global_sky_desc = atlas_image_desc(
            global_sky_layout,
            DDGI_IRRADIANCE_FORMAT,
            sampled_storage_usage,
        );

        let probe_metadata = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            ),
            MemoryLocation::GpuOnly,
            resource_bytes.probe_metadata,
        );
        let transient_ray_data = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            ),
            MemoryLocation::GpuOnly,
            resource_bytes.transient_ray_data,
        );
        let trace_stats = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::TRANSFER_SRC
                    | vk::BufferUsageFlags::TRANSFER_DST,
            ),
            MemoryLocation::GpuOnly,
            resource_bytes.trace_stats,
        );
        let trace_stats_readback = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::TRANSFER_DST),
            MemoryLocation::GpuToCpu,
            resource_bytes.trace_stats,
        );
        let atlas_reduction = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::TRANSFER_SRC
                    | vk::BufferUsageFlags::TRANSFER_DST,
            ),
            MemoryLocation::GpuOnly,
            resource_bytes.atlas_reduction,
        );
        let atlas_reduction_readback = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::TRANSFER_DST),
            MemoryLocation::GpuToCpu,
            resource_bytes.atlas_reduction,
        );
        let uniform_buffer = |size| {
            Buffer::new_sized(
                device.clone(),
                allocator.clone(),
                BufferUsage::from_flags(vk::BufferUsageFlags::UNIFORM_BUFFER),
                MemoryLocation::CpuToGpu,
                size,
            )
        };
        let radiance_sun = uniform_buffer(resource_bytes.radiance_sun);
        radiance_sun.fill_uniform(&DdgiRadianceSun::zeroed())?;
        let radiance_voxel_palette = uniform_buffer(resource_bytes.radiance_voxel_palette);
        radiance_voxel_palette.fill_uniform(&DdgiRadianceVoxelPalette::zeroed())?;
        let transport_query_info = uniform_buffer(resource_bytes.transport_query_info);
        let transport_query_snapshot = DdgiTransportQueryInfo {
            grid_dimensions: grid.dimensions().to_array(),
            visibility_bias_world: 2.0 / voxels_per_world_unit.min_element().max(1) as f32,
            world_to_grid_scale: (voxels_per_world_unit.as_vec3() / spacing_voxels as f32)
                .to_array(),
            source_ready: 0,
            irradiance_tile_columns: irradiance_layout.tile_grid().x,
            visibility_tile_columns: visibility_layout.tile_grid().x,
            padding: [0; 2],
        };
        transport_query_info.fill_uniform(&transport_query_snapshot)?;
        let irradiance_atlas = Texture::new(
            device.clone(),
            allocator.clone(),
            &irradiance_desc,
            &sampler_desc,
        );
        let transport_source_irradiance_atlas = Texture::new(
            device.clone(),
            allocator.clone(),
            &irradiance_desc,
            &sampler_desc,
        );
        let visibility_atlas = Texture::new(
            device.clone(),
            allocator.clone(),
            &visibility_desc,
            &sampler_desc,
        );
        let global_sky_irradiance =
            Texture::new(device, allocator, &global_sky_desc, &sampler_desc);

        log::info!(
            "[DDGI] allocated stage=allocated spacing_voxels={} grid={}x{}x{} probes={} irradiance={}x{} RGBA32F visibility={}x{} RG32F ray_batch={}x{} metadata_bytes={} irradiance_bytes={} transport_source_irradiance_bytes={} visibility_bytes={} ray_bytes={} trace_stats_bytes={} atlas_reduction_bytes={} global_sky_bytes={} snapshot_uniform_bytes={} transport_query_bytes={} total_mib={:.2}",
            spacing_voxels,
            grid.dimensions().x,
            grid.dimensions().y,
            grid.dimensions().z,
            grid.probe_count(),
            irradiance_layout.extent().x,
            irradiance_layout.extent().y,
            visibility_layout.extent().x,
            visibility_layout.extent().y,
            DDGI_PROBE_BATCH_SIZE,
            DDGI_RAYS_PER_PROBE,
            resource_bytes.probe_metadata,
            resource_bytes.irradiance_atlas,
            resource_bytes.transport_source_irradiance_atlas,
            resource_bytes.visibility_atlas,
            resource_bytes.transient_ray_data,
            resource_bytes.trace_stats,
            resource_bytes.atlas_reduction,
            resource_bytes.global_sky_irradiance,
            resource_bytes.radiance_sun + resource_bytes.radiance_voxel_palette,
            resource_bytes.transport_query_info,
            resource_bytes.total() as f64 / (1024.0 * 1024.0),
        );

        Ok(Self {
            build_token: None,
            grid,
            irradiance_layout,
            visibility_layout,
            resource_bytes,
            stage: DdgiVolumeStage::Allocated,
            transport_stage: None,
            building_transport_stage: None,
            complete_field: None,
            published_field: None,
            consecutive_below_threshold: 0,
            last_atlas_validation: None,
            global_sky_revision: 0,
            radiance_revision: None,
            requested_terrain_revision: None,
            relocated_terrain_revision: None,
            active_ray_batch: None,
            next_probe_index: 0,
            ddgi_probe_metadata: Resource::new(probe_metadata),
            ddgi_transient_ray_data: Resource::new(transient_ray_data),
            ddgi_trace_stats: Resource::new(trace_stats),
            ddgi_trace_stats_readback: trace_stats_readback,
            ddgi_atlas_reduction: Resource::new(atlas_reduction),
            ddgi_atlas_reduction_readback: atlas_reduction_readback,
            ddgi_irradiance_atlas: Resource::new(irradiance_atlas),
            ddgi_transport_source_irradiance_atlas: Resource::new(
                transport_source_irradiance_atlas,
            ),
            ddgi_visibility_atlas: Resource::new(visibility_atlas),
            ddgi_global_sky_irradiance: Resource::new(global_sky_irradiance),
            ddgi_radiance_sun: Resource::new(radiance_sun),
            ddgi_radiance_voxel_palette: Resource::new(radiance_voxel_palette),
            ddgi_transport_query_info: Resource::new(transport_query_info),
            transport_query_snapshot,
        })
    }

    pub fn status(&self) -> DdgiVolumeStatus {
        DdgiVolumeStatus {
            build_token: self.build_token,
            grid: self.grid,
            irradiance_layout: self.irradiance_layout,
            visibility_layout: self.visibility_layout,
            resource_bytes: self.resource_bytes,
            stage: self.stage,
            transport_stage: self.transport_stage,
            building_transport_stage: self.building_transport_stage,
            complete_field: self.complete_field,
            published_field: self.published_field,
            consecutive_below_threshold: self.consecutive_below_threshold,
            last_atlas_validation: self.last_atlas_validation,
            global_sky_revision: self.global_sky_revision,
            radiance_revision: self.radiance_revision,
            relocated_terrain_revision: self.relocated_terrain_revision,
            active_ray_batch: self.active_ray_batch,
            filtered_probe_count: self.next_probe_index,
        }
    }

    pub fn assign_build_token(&mut self, build_token: DdgiBuildToken) {
        assert!(
            self.build_token.is_none(),
            "DDGI build token may only be assigned once"
        );
        self.build_token = Some(build_token);
    }

    pub fn should_latch_radiance_snapshot(&self, latest_revision: u32) -> bool {
        radiance_snapshot_should_latch(self.stage, self.radiance_revision, latest_revision)
    }

    pub fn latch_radiance_snapshot(
        &mut self,
        revision: u32,
        snapshot: DdgiRadianceSnapshot,
    ) -> Result<()> {
        ensure!(
            self.should_latch_radiance_snapshot(revision),
            "cannot replace DDGI radiance revision {:?} with {revision} while stage {:?} is in flight",
            self.radiance_revision,
            self.stage,
        );
        self.ddgi_radiance_sun.fill_uniform(&DdgiRadianceSun {
            direction: snapshot.sun_direction.to_array(),
            padding: 0.0,
            color: snapshot.sun_color.to_array(),
            luminance: snapshot.sun_luminance,
        })?;
        self.ddgi_radiance_voxel_palette
            .fill_uniform(&DdgiRadianceVoxelPalette {
                dirt_color: snapshot.voxel_palette.dirt_color.to_array(),
                sand_color: snapshot.voxel_palette.sand_color.to_array(),
                cherry_wood_color: snapshot.voxel_palette.cherry_wood_color.to_array(),
                oak_wood_color: snapshot.voxel_palette.oak_wood_color.to_array(),
                rock_color: snapshot.voxel_palette.rock_color.to_array(),
                hash_color_variance: snapshot.voxel_palette.hash_color_variance,
                ..DdgiRadianceVoxelPalette::zeroed()
            })?;
        self.radiance_revision = Some(revision);
        Ok(())
    }

    pub fn global_sky_needs_update(&self) -> bool {
        self.radiance_revision
            .is_some_and(|revision| self.global_sky_revision != revision)
    }

    pub fn mark_global_sky_ready(&mut self, environment_revision: u32) -> Result<()> {
        self.global_sky_revision = environment_revision;
        if matches!(
            self.stage,
            DdgiVolumeStage::Relocated
                | DdgiVolumeStage::RayBatchReady
                | DdgiVolumeStage::AtlasReady
                | DdgiVolumeStage::Rebuilding
                | DdgiVolumeStage::Ready
        ) {
            self.next_probe_index = 0;
            self.active_ray_batch = None;
            self.transport_stage = None;
            self.building_transport_stage = Some(DdgiTransportStage::SeedSky);
            self.complete_field = None;
            self.published_field = None;
            self.consecutive_below_threshold = 0;
            self.last_atlas_validation = None;
            self.set_transport_source_ready(false)?;
        }
        self.stage = stage_after_global_sky_update(self.stage);
        Ok(())
    }

    pub fn request_initialization(&mut self, terrain_revision: u32) -> bool {
        if initialization_request_is_duplicate(
            self.stage,
            self.requested_terrain_revision,
            terrain_revision,
        ) {
            return false;
        }

        self.requested_terrain_revision = Some(terrain_revision);
        self.relocated_terrain_revision = None;
        self.active_ray_batch = None;
        self.next_probe_index = 0;
        self.transport_stage = None;
        self.building_transport_stage = None;
        self.complete_field = None;
        self.published_field = None;
        self.consecutive_below_threshold = 0;
        self.last_atlas_validation = None;
        self.stage = DdgiVolumeStage::RelocationPending;
        true
    }

    pub fn pending_relocation_terrain_revision(&self) -> Option<u32> {
        (self.stage == DdgiVolumeStage::RelocationPending)
            .then_some(self.requested_terrain_revision)
            .flatten()
    }

    pub fn mark_relocated(&mut self, terrain_revision: u32) -> Result<()> {
        assert_eq!(self.requested_terrain_revision, Some(terrain_revision));
        self.relocated_terrain_revision = Some(terrain_revision);
        self.next_probe_index = 0;
        self.transport_stage = None;
        self.building_transport_stage = Some(DdgiTransportStage::SeedSky);
        self.complete_field = None;
        self.published_field = None;
        self.consecutive_below_threshold = 0;
        self.last_atlas_validation = None;
        self.set_transport_source_ready(false)?;
        self.stage = DdgiVolumeStage::Relocated;
        Ok(())
    }

    pub fn next_ray_batch_to_trace(&self) -> Option<DdgiRayBatch> {
        if !matches!(
            self.stage,
            DdgiVolumeStage::Relocated | DdgiVolumeStage::Rebuilding
        ) || self.active_ray_batch.is_some()
            || self.next_probe_index >= self.grid.probe_count()
        {
            return None;
        }
        let transport = self.current_iteration_identity()?;
        Some(DdgiRayBatch {
            first_probe_index: self.next_probe_index,
            probe_count: (self.grid.probe_count() - self.next_probe_index)
                .min(DDGI_PROBE_BATCH_SIZE),
            transport,
        })
    }

    fn current_iteration_identity(&self) -> Option<DdgiTransportIterationIdentity> {
        let stage = self.building_transport_stage?;
        let destination_slot = stage.destination_slot()?;
        let geometry_revision = self.relocated_terrain_revision?;
        let radiance_revision = self.radiance_revision?;
        let destination = DdgiTransportFieldIdentity {
            build_token: self.build_token,
            geometry_revision,
            radiance_revision,
            spacing_voxels: self.grid.spacing_voxels(),
            stage,
            iteration: stage.iteration(),
            slot: destination_slot,
        };
        let source = match stage.immutable_source() {
            None => None,
            Some(expected_stage) => {
                let source = self.complete_field?;
                if source.stage != expected_stage
                    || source.iteration != expected_stage.iteration()
                    || source.slot == destination_slot
                {
                    return None;
                }
                Some(source)
            }
        };
        Some(DdgiTransportIterationIdentity {
            build_token: self.build_token,
            geometry_revision,
            radiance_revision,
            spacing_voxels: self.grid.spacing_voxels(),
            stage,
            iteration: stage.iteration(),
            source,
            destination,
        })
    }

    pub fn iteration_will_complete(&self, batch: DdgiRayBatch) -> bool {
        assert_eq!(self.active_ray_batch, Some(batch));
        self.next_probe_index + batch.probe_count == self.grid.probe_count()
    }

    pub fn mark_ray_batch_ready(&mut self, batch: DdgiRayBatch) {
        assert_eq!(self.next_ray_batch_to_trace(), Some(batch));
        self.active_ray_batch = Some(batch);
        self.stage = DdgiVolumeStage::RayBatchReady;
    }

    pub fn mark_ray_batch_filtered(&mut self, batch: DdgiRayBatch) {
        assert_eq!(self.stage, DdgiVolumeStage::RayBatchReady);
        assert_eq!(self.active_ray_batch, Some(batch));
        self.next_probe_index += batch.probe_count;
        // Keep the exact batch identity live until the later-frame trace-stat readback has been
        // validated. This prevents the next batch, iteration advance, and publication from
        // overtaking GPU validation.
        self.stage = if self.next_probe_index == self.grid.probe_count() {
            DdgiVolumeStage::AtlasReady
        } else {
            DdgiVolumeStage::Rebuilding
        };
    }

    pub fn pending_trace_stats_batch_is(&self, batch: DdgiRayBatch) -> bool {
        pending_trace_stats_batch_matches(self.active_ray_batch, self.stage, batch)
    }

    pub fn mark_trace_stats_verified(
        &mut self,
        batch: DdgiRayBatch,
    ) -> Result<DdgiVerifiedBatchOutcome> {
        ensure!(
            self.pending_trace_stats_batch_is(batch),
            "stale DDGI trace-stat readback identity {batch:?}; current batch={:?} stage={:?}",
            self.active_ray_batch,
            self.stage,
        );
        ensure!(
            self.next_probe_index <= self.grid.probe_count(),
            "DDGI iteration filtered {}/{} probes",
            self.next_probe_index,
            self.grid.probe_count(),
        );
        if self.next_probe_index < self.grid.probe_count() {
            self.active_ray_batch = None;
            self.stage = DdgiVolumeStage::Rebuilding;
            return Ok(DdgiVerifiedBatchOutcome::Continue);
        }
        let identity = self
            .current_iteration_identity()
            .context("DDGI full iteration lost its transport identity")?;
        ensure!(
            identity == batch.transport,
            "DDGI full iteration identity changed"
        );
        Ok(DdgiVerifiedBatchOutcome::AwaitingAtlasValidation(identity))
    }

    pub fn mark_atlas_validated(
        &mut self,
        identity: DdgiTransportIterationIdentity,
        stats: DdgiAtlasValidationStats,
        policy: DdgiConvergencePolicy,
    ) -> Result<DdgiValidatedIterationOutcome> {
        ensure!(
            self.active_ray_batch
                .is_some_and(|batch| batch.transport == identity)
                && self.stage == DdgiVolumeStage::AtlasReady
                && self.next_probe_index == self.grid.probe_count(),
            "stale DDGI atlas validation identity {identity:?}; batch={:?} stage={:?} filtered={}/{}",
            self.active_ray_batch,
            self.stage,
            self.next_probe_index,
            self.grid.probe_count(),
        );
        ensure!(
            self.current_iteration_identity() == Some(identity),
            "DDGI atlas validation no longer matches the builder iteration"
        );
        ensure!(
            stats.non_finite_count == 0,
            "DDGI full-atlas validation found non-finite stored texels: {stats:?}"
        );
        ensure!(
            stats.valid_texel_count > 0 && stats.scanned_stored_texel_count > 0,
            "DDGI full-atlas validation found no valid probe texels: {stats:?}"
        );

        let previous_complete = self.complete_field;
        self.active_ray_batch = None;
        self.complete_field = Some(identity.destination);
        self.last_atlas_validation = Some(stats);
        self.next_probe_index = 0;

        match identity.stage {
            DdgiTransportStage::SeedSky => {
                ensure!(
                    identity.source.is_none(),
                    "DDGI S0 must not have a source field"
                );
                self.transport_stage = Some(DdgiTransportStage::SeedSky);
                self.building_transport_stage = Some(DdgiTransportStage::SingleBounce);
                self.set_transport_source_ready(true)?;
                self.stage = DdgiVolumeStage::Rebuilding;
                Ok(DdgiValidatedIterationOutcome::SeedSkyComplete)
            }
            DdgiTransportStage::SingleBounce => {
                ensure!(
                    identity.source == previous_complete,
                    "DDGI S1 did not consume the immutable S0 field"
                );
                self.transport_stage = Some(DdgiTransportStage::SingleBounce);
                self.published_field = Some(identity.destination);
                self.consecutive_below_threshold = 0;
                let next = DdgiTransportStage::Feedback { iteration: 2 };
                self.building_transport_stage = Some(next);
                self.stage = DdgiVolumeStage::Rebuilding;
                Ok(DdgiValidatedIterationOutcome::Published {
                    field: identity.destination,
                    next,
                    consecutive_below_threshold: 0,
                })
            }
            DdgiTransportStage::Feedback { iteration } => {
                ensure!(
                    identity.source == previous_complete,
                    "DDGI feedback iteration S{iteration} did not consume the previous field"
                );
                self.published_field = Some(identity.destination);
                match classify_feedback_iteration(
                    policy,
                    iteration,
                    self.consecutive_below_threshold,
                    stats,
                ) {
                    DdgiFeedbackDecision::Continue {
                        consecutive_below_threshold,
                    } => {
                        self.transport_stage = Some(identity.stage);
                        self.consecutive_below_threshold = consecutive_below_threshold;
                        let next = DdgiTransportStage::Feedback {
                            iteration: iteration + 1,
                        };
                        self.building_transport_stage = Some(next);
                        self.stage = DdgiVolumeStage::Rebuilding;
                        Ok(DdgiValidatedIterationOutcome::Published {
                            field: identity.destination,
                            next,
                            consecutive_below_threshold,
                        })
                    }
                    DdgiFeedbackDecision::Converged => {
                        self.transport_stage = Some(DdgiTransportStage::Converged { iteration });
                        self.building_transport_stage = None;
                        self.stage = DdgiVolumeStage::Ready;
                        Ok(DdgiValidatedIterationOutcome::Converged {
                            field: identity.destination,
                            iteration,
                        })
                    }
                    DdgiFeedbackDecision::NonConverged => {
                        self.transport_stage = Some(DdgiTransportStage::NonConverged { iteration });
                        self.building_transport_stage = None;
                        self.stage = DdgiVolumeStage::Ready;
                        Ok(DdgiValidatedIterationOutcome::NonConverged {
                            field: identity.destination,
                            iteration,
                        })
                    }
                }
            }
            DdgiTransportStage::Converged { .. } | DdgiTransportStage::NonConverged { .. } => {
                anyhow::bail!("terminal DDGI stage cannot be built: {:?}", identity.stage)
            }
        }
    }

    pub fn irradiance_atlas(&self, slot: DdgiIrradianceSlot) -> &Resource<Texture> {
        match slot {
            DdgiIrradianceSlot::Atlas0 => &self.ddgi_irradiance_atlas,
            DdgiIrradianceSlot::Atlas1 => &self.ddgi_transport_source_irradiance_atlas,
        }
    }

    pub fn published_irradiance_atlas(&self) -> Option<&Resource<Texture>> {
        self.published_field
            .map(|field| self.irradiance_atlas(field.slot))
    }

    fn set_transport_source_ready(&mut self, ready: bool) -> Result<()> {
        self.transport_query_snapshot.source_ready = u32::from(ready);
        self.ddgi_transport_query_info
            .fill_uniform(&self.transport_query_snapshot)
    }

    pub fn record_trace_stats_readback(&self, cmdbuf: &re_flora_vkn::CommandBuffer) {
        self.ddgi_trace_stats.record_copy_to_buffer(
            cmdbuf,
            &self.ddgi_trace_stats_readback,
            self.resource_bytes.trace_stats,
            0,
            0,
        );
    }

    pub fn record_atlas_reduction_readback(&self, cmdbuf: &re_flora_vkn::CommandBuffer) {
        self.ddgi_atlas_reduction.record_copy_to_buffer(
            cmdbuf,
            &self.ddgi_atlas_reduction_readback,
            self.resource_bytes.atlas_reduction,
            0,
            0,
        );
    }

    pub fn update_atlas_validation_from_readback(&self) -> Result<DdgiAtlasValidationStats> {
        let bytes = self.ddgi_atlas_reduction_readback.read_back()?;
        ensure!(
            bytes.len() == self.resource_bytes.atlas_reduction as usize,
            "DDGI atlas reduction readback returned {} bytes, expected {}",
            bytes.len(),
            self.resource_bytes.atlas_reduction,
        );
        let mut values = [0_u32; DDGI_ATLAS_REDUCTION_COUNT];
        for (value, bytes) in values.iter_mut().zip(bytes.chunks_exact(4)) {
            *value = u32::from_ne_bytes(bytes.try_into().expect("u32-sized chunk"));
        }
        Ok(DdgiAtlasValidationStats::from_array(values))
    }

    pub fn update_trace_stats_from_readback(&self) -> Result<DdgiTraceStats> {
        let bytes = self.ddgi_trace_stats_readback.read_back()?;
        ensure!(
            bytes.len() == self.resource_bytes.trace_stats as usize,
            "DDGI trace stats readback returned {} bytes, expected {}",
            bytes.len(),
            self.resource_bytes.trace_stats,
        );
        let mut values = [0_u32; DDGI_TRACE_STATS_COUNT];
        for (value, bytes) in values.iter_mut().zip(bytes.chunks_exact(4)) {
            *value = u32::from_ne_bytes(bytes.try_into().expect("u32-sized chunk"));
        }
        let stats = DdgiTraceStats::from_array(values);
        ensure!(
            stats.ray_records
                == stats
                    .valid_probe_rays
                    .saturating_add(stats.invalid_probe_rays),
            "DDGI trace stats ray partition is inconsistent: {stats:?}",
        );
        ensure!(
            stats.valid_probe_rays
                == stats
                    .misses
                    .saturating_add(stats.frontface_hits)
                    .saturating_add(stats.backface_hits),
            "DDGI trace stats hit partition is inconsistent: {stats:?}",
        );
        Ok(stats)
    }
}

fn pending_trace_stats_batch_matches(
    active_batch: Option<DdgiRayBatch>,
    stage: DdgiVolumeStage,
    candidate: DdgiRayBatch,
) -> bool {
    active_batch == Some(candidate)
        && matches!(
            stage,
            DdgiVolumeStage::Rebuilding | DdgiVolumeStage::AtlasReady
        )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DdgiFeedbackDecision {
    Continue { consecutive_below_threshold: u32 },
    Converged,
    NonConverged,
}

fn classify_feedback_iteration(
    policy: DdgiConvergencePolicy,
    iteration: u32,
    previous_consecutive_below_threshold: u32,
    stats: DdgiAtlasValidationStats,
) -> DdgiFeedbackDecision {
    debug_assert!(iteration >= 2);
    let below = stats.max_absolute_rgb_delta <= policy.absolute_threshold
        && stats.max_relative_rgb_delta <= policy.relative_threshold;
    let consecutive_below_threshold = if below {
        previous_consecutive_below_threshold + 1
    } else {
        0
    };
    if consecutive_below_threshold >= policy.consecutive_iterations {
        DdgiFeedbackDecision::Converged
    } else if iteration >= policy.hard_max_iteration {
        DdgiFeedbackDecision::NonConverged
    } else {
        DdgiFeedbackDecision::Continue {
            consecutive_below_threshold,
        }
    }
}

fn initialization_request_is_duplicate(
    stage: DdgiVolumeStage,
    requested_terrain_revision: Option<u32>,
    terrain_revision: u32,
) -> bool {
    requested_terrain_revision == Some(terrain_revision)
        && matches!(
            stage,
            DdgiVolumeStage::RelocationPending
                | DdgiVolumeStage::Relocated
                | DdgiVolumeStage::RayBatchReady
                | DdgiVolumeStage::AtlasReady
                | DdgiVolumeStage::Ready
        )
}

fn radiance_snapshot_should_latch(
    stage: DdgiVolumeStage,
    latched_revision: Option<u32>,
    latest_revision: u32,
) -> bool {
    if latest_revision == 0 || latched_revision == Some(latest_revision) {
        return false;
    }
    latched_revision.is_none() || stage == DdgiVolumeStage::Ready
}

fn stage_after_global_sky_update(stage: DdgiVolumeStage) -> DdgiVolumeStage {
    match stage {
        DdgiVolumeStage::Allocated | DdgiVolumeStage::GlobalSkyReady => {
            DdgiVolumeStage::GlobalSkyReady
        }
        DdgiVolumeStage::RelocationPending => DdgiVolumeStage::RelocationPending,
        DdgiVolumeStage::Relocated
        | DdgiVolumeStage::RayBatchReady
        | DdgiVolumeStage::AtlasReady
        | DdgiVolumeStage::Rebuilding
        | DdgiVolumeStage::Ready => DdgiVolumeStage::Rebuilding,
    }
}

fn atlas_image_desc(
    layout: DdgiAtlasLayout,
    format: vk::Format,
    usage: vk::ImageUsageFlags,
) -> ImageDesc {
    let extent = layout.extent();
    ImageDesc {
        extent: Extent3D::new(extent.x, extent.y, 1),
        format,
        usage,
        initial_layout: TextureLayout::UNDEFINED,
        aspect: vk::ImageAspectFlags::COLOR,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spacing_32_resource_contract_is_full_precision_and_batch_bounded() {
        let grid = DdgiVolumeGrid::new(UVec3::splat(512), 32).unwrap();
        let irradiance =
            DdgiAtlasLayout::new(grid.probe_count(), DDGI_IRRADIANCE_INTERIOR_SIDE).unwrap();
        let visibility =
            DdgiAtlasLayout::new(grid.probe_count(), DDGI_VISIBILITY_INTERIOR_SIDE).unwrap();
        let bytes = DdgiResourceBytes::new(grid, irradiance, visibility);

        assert_eq!(irradiance.extent(), glam::UVec2::new(710, 700));
        assert_eq!(visibility.extent(), glam::UVec2::new(1_278, 1_260));
        assert_eq!(bytes.irradiance_atlas, 7_952_000);
        assert_eq!(bytes.transport_source_irradiance_atlas, 7_952_000);
        assert_eq!(bytes.visibility_atlas, 12_882_240);
        assert_eq!(bytes.probe_metadata, 235_824);
        assert_eq!(bytes.transient_ray_data, 524_288);
        assert_eq!(bytes.trace_stats, 32);
        assert_eq!(bytes.atlas_reduction, 20);
        assert_eq!(bytes.global_sky_irradiance, 1_600);
        assert_eq!(bytes.radiance_sun, 32);
        assert_eq!(bytes.radiance_voxel_palette, 80);
        assert_eq!(bytes.transport_query_info, 48);
    }

    #[test]
    fn texture_descriptors_use_required_oracle_formats() {
        let irradiance = DdgiAtlasLayout::new(1, DDGI_IRRADIANCE_INTERIOR_SIDE).unwrap();
        let visibility = DdgiAtlasLayout::new(1, DDGI_VISIBILITY_INTERIOR_SIDE).unwrap();
        let usage = vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::STORAGE;
        let irradiance_desc = atlas_image_desc(irradiance, DDGI_IRRADIANCE_FORMAT, usage);
        let visibility_desc = atlas_image_desc(visibility, DDGI_VISIBILITY_FORMAT, usage);

        assert_eq!(irradiance_desc.format, vk::Format::R32G32B32A32_SFLOAT);
        assert_eq!(visibility_desc.format, vk::Format::R32G32_SFLOAT);
        assert_eq!(irradiance_desc.extent, Extent3D::new(10, 10, 1));
        assert_eq!(visibility_desc.extent, Extent3D::new(18, 18, 1));
        assert!(irradiance_desc.usage.contains(vk::ImageUsageFlags::STORAGE));
        assert!(visibility_desc.usage.contains(vk::ImageUsageFlags::SAMPLED));
    }

    #[test]
    fn volume_is_not_ready_when_resources_are_only_allocated() {
        let grid = DdgiVolumeGrid::new(UVec3::splat(512), 32).unwrap();
        let status = DdgiVolumeStatus {
            build_token: None,
            grid,
            irradiance_layout: DdgiAtlasLayout::new(
                grid.probe_count(),
                DDGI_IRRADIANCE_INTERIOR_SIDE,
            )
            .unwrap(),
            visibility_layout: DdgiAtlasLayout::new(
                grid.probe_count(),
                DDGI_VISIBILITY_INTERIOR_SIDE,
            )
            .unwrap(),
            resource_bytes: DdgiResourceBytes::new(
                grid,
                DdgiAtlasLayout::new(grid.probe_count(), DDGI_IRRADIANCE_INTERIOR_SIDE).unwrap(),
                DdgiAtlasLayout::new(grid.probe_count(), DDGI_VISIBILITY_INTERIOR_SIDE).unwrap(),
            ),
            stage: DdgiVolumeStage::Allocated,
            transport_stage: None,
            building_transport_stage: None,
            complete_field: None,
            published_field: None,
            consecutive_below_threshold: 0,
            last_atlas_validation: None,
            global_sky_revision: 0,
            radiance_revision: None,
            relocated_terrain_revision: None,
            active_ray_batch: None,
            filtered_probe_count: 0,
        };
        assert!(!status.is_ready());
    }

    #[test]
    fn runtime_status_keeps_staging_out_of_the_consumer_view_until_ready() {
        let grid = DdgiVolumeGrid::new(UVec3::splat(512), 32).unwrap();
        let irradiance_layout =
            DdgiAtlasLayout::new(grid.probe_count(), DDGI_IRRADIANCE_INTERIOR_SIDE).unwrap();
        let visibility_layout =
            DdgiAtlasLayout::new(grid.probe_count(), DDGI_VISIBILITY_INTERIOR_SIDE).unwrap();
        let field_for = |terrain_revision| DdgiTransportFieldIdentity {
            build_token: None,
            geometry_revision: terrain_revision,
            radiance_revision: 3,
            spacing_voxels: 32,
            stage: DdgiTransportStage::SingleBounce,
            iteration: 1,
            slot: DdgiIrradianceSlot::Atlas0,
        };
        let status_for = |stage: DdgiVolumeStage,
                          terrain_revision: u32,
                          published: bool|
         -> DdgiVolumeStatus {
            DdgiVolumeStatus {
                build_token: None,
                grid,
                irradiance_layout,
                visibility_layout,
                resource_bytes: DdgiResourceBytes::new(grid, irradiance_layout, visibility_layout),
                stage,
                transport_stage: published.then_some(DdgiTransportStage::SingleBounce),
                building_transport_stage: (stage == DdgiVolumeStage::Rebuilding)
                    .then_some(DdgiTransportStage::Feedback { iteration: 2 }),
                complete_field: published.then(|| field_for(terrain_revision)),
                published_field: published.then(|| field_for(terrain_revision)),
                consecutive_below_threshold: 0,
                last_atlas_validation: None,
                global_sky_revision: 3,
                radiance_revision: Some(3),
                relocated_terrain_revision: Some(terrain_revision),
                active_ray_batch: None,
                filtered_probe_count: 0,
            }
        };

        let active = status_for(DdgiVolumeStage::Rebuilding, 7, true);
        let staging = status_for(DdgiVolumeStage::Rebuilding, 8, false);
        let status = DdgiStatus {
            active,
            staging: Some(staging),
        };

        assert_eq!(status.active(), active);
        assert_eq!(status.builder(), staging);
        assert!(!status.staging_is_ready());

        // A complete finite S1 can promote while partial S2 writes the other slot.
        let ready_staging = status_for(DdgiVolumeStage::Rebuilding, 8, true);
        let status = DdgiStatus {
            active,
            staging: Some(ready_staging),
        };
        assert_eq!(status.active().relocated_terrain_revision, Some(7));
        assert_eq!(status.builder().relocated_terrain_revision, Some(8));
        assert!(status.staging_is_ready());
    }

    #[test]
    fn sky_update_preserves_initialization_but_invalidates_a_complete_volume() {
        assert_eq!(
            stage_after_global_sky_update(DdgiVolumeStage::Allocated),
            DdgiVolumeStage::GlobalSkyReady
        );
        assert_eq!(
            stage_after_global_sky_update(DdgiVolumeStage::Ready),
            DdgiVolumeStage::Rebuilding
        );
        assert_eq!(
            stage_after_global_sky_update(DdgiVolumeStage::RelocationPending),
            DdgiVolumeStage::RelocationPending
        );
    }

    #[test]
    fn radiance_snapshot_is_immutable_until_the_full_volume_is_ready() {
        assert!(radiance_snapshot_should_latch(
            DdgiVolumeStage::RelocationPending,
            None,
            1,
        ));
        for stage in [
            DdgiVolumeStage::GlobalSkyReady,
            DdgiVolumeStage::RelocationPending,
            DdgiVolumeStage::Relocated,
            DdgiVolumeStage::RayBatchReady,
            DdgiVolumeStage::AtlasReady,
            DdgiVolumeStage::Rebuilding,
        ] {
            assert!(!radiance_snapshot_should_latch(stage, Some(1), 2));
        }
        assert!(radiance_snapshot_should_latch(
            DdgiVolumeStage::Ready,
            Some(1),
            2,
        ));
        assert!(!radiance_snapshot_should_latch(
            DdgiVolumeStage::Ready,
            Some(2),
            2,
        ));
    }

    #[test]
    fn initialization_request_is_idempotent_for_the_same_terrain_revision() {
        assert!(!initialization_request_is_duplicate(
            DdgiVolumeStage::Allocated,
            None,
            7,
        ));
        assert!(initialization_request_is_duplicate(
            DdgiVolumeStage::RelocationPending,
            Some(7),
            7,
        ));
        assert!(!initialization_request_is_duplicate(
            DdgiVolumeStage::RelocationPending,
            Some(7),
            8,
        ));
    }

    #[test]
    fn transport_slots_strictly_ping_pong_previous_field_to_distinct_destination() {
        assert_eq!(
            DdgiTransportStage::SeedSky.destination_slot(),
            Some(DdgiIrradianceSlot::Atlas1)
        );
        assert_eq!(
            DdgiTransportStage::SingleBounce.destination_slot(),
            Some(DdgiIrradianceSlot::Atlas0)
        );
        for iteration in 2..=8 {
            let destination = DdgiTransportStage::Feedback { iteration }
                .destination_slot()
                .unwrap();
            let previous = if iteration == 2 {
                DdgiIrradianceSlot::Atlas0
            } else {
                DdgiTransportStage::Feedback {
                    iteration: iteration - 1,
                }
                .destination_slot()
                .unwrap()
            };
            assert_ne!(destination, previous);
        }
    }

    #[test]
    fn convergence_requires_two_consecutive_iterations_below_both_thresholds() {
        let low = DdgiAtlasValidationStats {
            max_absolute_rgb_delta: DDGI_CONVERGENCE_POLICY.absolute_threshold,
            max_relative_rgb_delta: DDGI_CONVERGENCE_POLICY.relative_threshold,
            ..Default::default()
        };
        assert_eq!(
            classify_feedback_iteration(DDGI_CONVERGENCE_POLICY, 2, 0, low),
            DdgiFeedbackDecision::Continue {
                consecutive_below_threshold: 1
            }
        );
        assert_eq!(
            classify_feedback_iteration(DDGI_CONVERGENCE_POLICY, 3, 1, low),
            DdgiFeedbackDecision::Converged
        );

        let high_relative = DdgiAtlasValidationStats {
            max_absolute_rgb_delta: 0.0,
            max_relative_rgb_delta: DDGI_CONVERGENCE_POLICY.relative_threshold * 2.0,
            ..Default::default()
        };
        assert_eq!(
            classify_feedback_iteration(DDGI_CONVERGENCE_POLICY, 3, 1, high_relative),
            DdgiFeedbackDecision::Continue {
                consecutive_below_threshold: 0
            }
        );
    }

    #[test]
    fn hard_max_classifies_latest_finite_field_as_non_converged() {
        let high = DdgiAtlasValidationStats {
            max_absolute_rgb_delta: 1.0,
            max_relative_rgb_delta: 1.0,
            ..Default::default()
        };
        assert_eq!(
            classify_feedback_iteration(
                DDGI_CONVERGENCE_POLICY,
                DDGI_CONVERGENCE_POLICY.hard_max_iteration,
                0,
                high,
            ),
            DdgiFeedbackDecision::NonConverged
        );
    }

    #[test]
    fn trace_stat_readback_requires_exact_batch_and_iteration_identity() {
        let batch = DdgiRayBatch {
            first_probe_index: 64,
            probe_count: 64,
            transport: DdgiTransportIterationIdentity {
                build_token: None,
                geometry_revision: 7,
                radiance_revision: 3,
                spacing_voxels: 32,
                stage: DdgiTransportStage::SeedSky,
                iteration: 0,
                source: None,
                destination: DdgiTransportFieldIdentity {
                    build_token: None,
                    geometry_revision: 7,
                    radiance_revision: 3,
                    spacing_voxels: 32,
                    stage: DdgiTransportStage::SeedSky,
                    iteration: 0,
                    slot: DdgiIrradianceSlot::Atlas1,
                },
            },
        };
        assert!(pending_trace_stats_batch_matches(
            Some(batch),
            DdgiVolumeStage::Rebuilding,
            batch,
        ));
        assert!(!pending_trace_stats_batch_matches(
            Some(batch),
            DdgiVolumeStage::RayBatchReady,
            batch,
        ));
        assert!(!pending_trace_stats_batch_matches(
            Some(batch),
            DdgiVolumeStage::Rebuilding,
            DdgiRayBatch {
                transport: DdgiTransportIterationIdentity {
                    stage: DdgiTransportStage::SingleBounce,
                    iteration: 1,
                    ..batch.transport
                },
                ..batch
            },
        ));
        assert!(!pending_trace_stats_batch_matches(
            None,
            DdgiVolumeStage::AtlasReady,
            batch,
        ));
    }
}
