use super::{
    DdgiAtlasLayout, DdgiVolumeGrid, DDGI_IRRADIANCE_INTERIOR_SIDE, DDGI_PROBE_BATCH_SIZE,
    DDGI_RAYS_PER_PROBE, DDGI_VISIBILITY_INTERIOR_SIDE,
};
use crate::generated::gpu_structs::DdgiProbeMetadata;
use crate::resource::{Resource, ResourceContainer};
use anyhow::{ensure, Result};
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
    pub visibility_atlas: u64,
    pub global_sky_irradiance: u64,
    pub probe_metadata: u64,
    pub transient_ray_data: u64,
    pub trace_stats: u64,
}

impl DdgiResourceBytes {
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
        }
    }

    pub fn total(self) -> u64 {
        self.irradiance_atlas
            + self.visibility_atlas
            + self.global_sky_irradiance
            + self.probe_metadata
            + self.transient_ray_data
            + self.trace_stats
    }
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
    pub grid: DdgiVolumeGrid,
    pub irradiance_layout: DdgiAtlasLayout,
    pub visibility_layout: DdgiAtlasLayout,
    pub resource_bytes: DdgiResourceBytes,
    pub stage: DdgiVolumeStage,
    pub global_sky_revision: u32,
    pub relocated_terrain_revision: Option<u32>,
    pub active_ray_batch: Option<DdgiRayBatch>,
    pub filtered_probe_count: u32,
}

impl DdgiVolumeStatus {
    pub fn is_ready(self) -> bool {
        self.stage == DdgiVolumeStage::Ready
    }
}

pub struct DdgiVolume {
    grid: DdgiVolumeGrid,
    irradiance_layout: DdgiAtlasLayout,
    visibility_layout: DdgiAtlasLayout,
    resource_bytes: DdgiResourceBytes,
    stage: DdgiVolumeStage,
    global_sky_revision: u32,
    requested_terrain_revision: Option<u32>,
    relocated_terrain_revision: Option<u32>,
    active_ray_batch: Option<DdgiRayBatch>,
    next_probe_index: u32,
    pub ddgi_probe_metadata: Resource<Buffer>,
    pub ddgi_transient_ray_data: Resource<Buffer>,
    pub ddgi_trace_stats: Resource<Buffer>,
    ddgi_trace_stats_readback: Buffer,
    pub ddgi_irradiance_atlas: Resource<Texture>,
    pub ddgi_visibility_atlas: Resource<Texture>,
    pub ddgi_global_sky_irradiance: Resource<Texture>,
}

impl ResourceContainer for DdgiVolume {
    fn get_buffer(&self, name: &str) -> Option<&Buffer> {
        match name {
            "ddgi_probe_metadata" => Some(&self.ddgi_probe_metadata),
            "ddgi_transient_ray_data" => Some(&self.ddgi_transient_ray_data),
            "ddgi_trace_stats" => Some(&self.ddgi_trace_stats),
            _ => None,
        }
    }

    fn get_texture(&self, name: &str) -> Option<&Texture> {
        match name {
            "ddgi_irradiance_atlas" => Some(&self.ddgi_irradiance_atlas),
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
            "ddgi_irradiance_atlas",
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
        let irradiance_atlas = Texture::new(
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
            "[DDGI] allocated stage=allocated spacing_voxels={} grid={}x{}x{} probes={} irradiance={}x{} RGBA32F visibility={}x{} RG32F ray_batch={}x{} metadata_bytes={} irradiance_bytes={} visibility_bytes={} ray_bytes={} trace_stats_bytes={} global_sky_bytes={} total_mib={:.2}",
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
            resource_bytes.visibility_atlas,
            resource_bytes.transient_ray_data,
            resource_bytes.trace_stats,
            resource_bytes.global_sky_irradiance,
            resource_bytes.total() as f64 / (1024.0 * 1024.0),
        );

        Ok(Self {
            grid,
            irradiance_layout,
            visibility_layout,
            resource_bytes,
            stage: DdgiVolumeStage::Allocated,
            global_sky_revision: 0,
            requested_terrain_revision: None,
            relocated_terrain_revision: None,
            active_ray_batch: None,
            next_probe_index: 0,
            ddgi_probe_metadata: Resource::new(probe_metadata),
            ddgi_transient_ray_data: Resource::new(transient_ray_data),
            ddgi_trace_stats: Resource::new(trace_stats),
            ddgi_trace_stats_readback: trace_stats_readback,
            ddgi_irradiance_atlas: Resource::new(irradiance_atlas),
            ddgi_visibility_atlas: Resource::new(visibility_atlas),
            ddgi_global_sky_irradiance: Resource::new(global_sky_irradiance),
        })
    }

    pub fn status(&self) -> DdgiVolumeStatus {
        DdgiVolumeStatus {
            grid: self.grid,
            irradiance_layout: self.irradiance_layout,
            visibility_layout: self.visibility_layout,
            resource_bytes: self.resource_bytes,
            stage: self.stage,
            global_sky_revision: self.global_sky_revision,
            relocated_terrain_revision: self.relocated_terrain_revision,
            active_ray_batch: self.active_ray_batch,
            filtered_probe_count: self.next_probe_index,
        }
    }

    pub fn global_sky_needs_update(&self, environment_revision: u32) -> bool {
        self.global_sky_revision != environment_revision
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
        assert_eq!(bytes.visibility_atlas, 12_882_240);
        assert_eq!(bytes.probe_metadata, 235_824);
        assert_eq!(bytes.transient_ray_data, 524_288);
        assert_eq!(bytes.trace_stats, 32);
        assert_eq!(bytes.global_sky_irradiance, 1_600);
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
            global_sky_revision: 0,
            relocated_terrain_revision: None,
            active_ray_batch: None,
            filtered_probe_count: 0,
        };
        assert!(!status.is_ready());
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
