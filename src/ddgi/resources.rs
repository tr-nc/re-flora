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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DdgiRayBatch {
    pub first_probe_index: u32,
    pub probe_count: u32,
    pub terrain_revision: u32,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DdgiResourceBytes {
    pub irradiance_atlas: u64,
    pub transport_source_irradiance_atlas: u64,
    pub visibility_atlas: u64,
    pub global_sky_irradiance: u64,
    pub probe_metadata: u64,
    pub transient_ray_data: u64,
    pub trace_stats: u64,
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
            + self.radiance_sun
            + self.radiance_voxel_palette
            + self.transport_query_info
    }
}

/// The last complete transport state represented by a volume's irradiance atlas.
///
/// Probe batches are deliberately absent: a partial full-volume iteration is never a transport
/// state and must never become consumer-visible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum DdgiTransportStage {
    SeedSky,
    SingleBounce,
    Feedback { iteration: u32 },
    Converged,
    NonConverged,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DdgiVolumeStatus {
    pub build_token: Option<DdgiBuildToken>,
    pub grid: DdgiVolumeGrid,
    pub irradiance_layout: DdgiAtlasLayout,
    pub visibility_layout: DdgiAtlasLayout,
    pub resource_bytes: DdgiResourceBytes,
    pub stage: DdgiVolumeStage,
    pub transport_stage: Option<DdgiTransportStage>,
    pub global_sky_revision: u32,
    pub radiance_revision: Option<u32>,
    pub relocated_terrain_revision: Option<u32>,
    pub active_ray_batch: Option<DdgiRayBatch>,
    pub filtered_probe_count: u32,
}

impl DdgiVolumeStatus {
    pub fn is_ready(self) -> bool {
        self.stage == DdgiVolumeStage::Ready
    }
}

/// The consumer-visible DDGI volume and an optional volume being built for a later promotion.
///
/// Callers can inspect revisions and readiness without learning which atlas or ray batch the
/// builder currently owns. Consumers must use `active`; builder passes must use `builder`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    pub ddgi_irradiance_atlas: Resource<Texture>,
    pub ddgi_transport_source_irradiance_atlas: Resource<Texture>,
    pub ddgi_visibility_atlas: Resource<Texture>,
    pub ddgi_global_sky_irradiance: Resource<Texture>,
    pub ddgi_radiance_sun: Resource<Buffer>,
    pub ddgi_radiance_voxel_palette: Resource<Buffer>,
    pub ddgi_transport_query_info: Resource<Buffer>,
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
        transport_query_info.fill_uniform(&DdgiTransportQueryInfo {
            grid_dimensions: grid.dimensions().to_array(),
            visibility_bias_world: 2.0 / voxels_per_world_unit.min_element().max(1) as f32,
            world_to_grid_scale: (voxels_per_world_unit.as_vec3() / spacing_voxels as f32)
                .to_array(),
            source_ready: 0,
            irradiance_tile_columns: irradiance_layout.tile_grid().x,
            visibility_tile_columns: visibility_layout.tile_grid().x,
            padding: [0; 2],
        })?;
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
            "[DDGI] allocated stage=allocated spacing_voxels={} grid={}x{}x{} probes={} irradiance={}x{} RGBA32F visibility={}x{} RG32F ray_batch={}x{} metadata_bytes={} irradiance_bytes={} transport_source_irradiance_bytes={} visibility_bytes={} ray_bytes={} trace_stats_bytes={} global_sky_bytes={} snapshot_uniform_bytes={} transport_query_bytes={} total_mib={:.2}",
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
            ddgi_irradiance_atlas: Resource::new(irradiance_atlas),
            ddgi_transport_source_irradiance_atlas: Resource::new(
                transport_source_irradiance_atlas,
            ),
            ddgi_visibility_atlas: Resource::new(visibility_atlas),
            ddgi_global_sky_irradiance: Resource::new(global_sky_irradiance),
            ddgi_radiance_sun: Resource::new(radiance_sun),
            ddgi_radiance_voxel_palette: Resource::new(radiance_voxel_palette),
            ddgi_transport_query_info: Resource::new(transport_query_info),
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

    pub fn mark_global_sky_ready(&mut self, environment_revision: u32) {
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
        }
        self.stage = stage_after_global_sky_update(self.stage);
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
        self.stage = DdgiVolumeStage::RelocationPending;
        true
    }

    pub fn pending_relocation_terrain_revision(&self) -> Option<u32> {
        (self.stage == DdgiVolumeStage::RelocationPending)
            .then_some(self.requested_terrain_revision)
            .flatten()
    }

    pub fn mark_relocated(&mut self, terrain_revision: u32) {
        assert_eq!(self.requested_terrain_revision, Some(terrain_revision));
        self.relocated_terrain_revision = Some(terrain_revision);
        self.next_probe_index = 0;
        self.stage = DdgiVolumeStage::Relocated;
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
        Some(DdgiRayBatch {
            first_probe_index: self.next_probe_index,
            probe_count: (self.grid.probe_count() - self.next_probe_index)
                .min(DDGI_PROBE_BATCH_SIZE),
            terrain_revision: self.relocated_terrain_revision?,
        })
    }

    pub fn mark_ray_batch_ready(&mut self, batch: DdgiRayBatch) {
        assert_eq!(self.next_ray_batch_to_trace(), Some(batch));
        self.active_ray_batch = Some(batch);
        self.stage = DdgiVolumeStage::RayBatchReady;
    }

    pub fn mark_ray_batch_filtered(&mut self, batch: DdgiRayBatch) {
        assert_eq!(self.stage, DdgiVolumeStage::RayBatchReady);
        assert_eq!(self.active_ray_batch, Some(batch));
        self.next_probe_index = batch.first_probe_index + batch.probe_count;
        self.active_ray_batch = None;
        self.stage = if self.next_probe_index == self.grid.probe_count() {
            DdgiVolumeStage::AtlasReady
        } else {
            DdgiVolumeStage::Rebuilding
        };
    }

    pub fn mark_ready(&mut self) {
        assert_eq!(self.stage, DdgiVolumeStage::AtlasReady);
        assert_eq!(self.next_probe_index, self.grid.probe_count());
        self.transport_stage
            .get_or_insert(DdgiTransportStage::SeedSky);
        self.stage = DdgiVolumeStage::Ready;
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
        let status_for = |stage, terrain_revision| DdgiVolumeStatus {
            build_token: None,
            grid,
            irradiance_layout,
            visibility_layout,
            resource_bytes: DdgiResourceBytes::new(grid, irradiance_layout, visibility_layout),
            stage,
            transport_stage: (stage == DdgiVolumeStage::Ready)
                .then_some(DdgiTransportStage::SeedSky),
            global_sky_revision: 3,
            radiance_revision: Some(3),
            relocated_terrain_revision: Some(terrain_revision),
            active_ray_batch: None,
            filtered_probe_count: 0,
        };

        let active = status_for(DdgiVolumeStage::Ready, 7);
        let staging = status_for(DdgiVolumeStage::Rebuilding, 8);
        let status = DdgiStatus {
            active,
            staging: Some(staging),
        };

        assert_eq!(status.active(), active);
        assert_eq!(status.builder(), staging);
        assert!(!status.staging_is_ready());

        let ready_staging = status_for(DdgiVolumeStage::Ready, 8);
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
}
