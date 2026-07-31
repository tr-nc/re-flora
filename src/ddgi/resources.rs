use super::{
    DdgiAtlasLayout, DdgiVolumeGrid, DDGI_IRRADIANCE_INTERIOR_SIDE, DDGI_PROBE_BATCH_SIZE,
    DDGI_RAYS_PER_PROBE, DDGI_VISIBILITY_INTERIOR_SIDE,
};
use crate::resource::{Resource, ResourceContainer};
use anyhow::{ensure, Result};
use bytemuck::{Pod, Zeroable};
use glam::UVec3;
use re_flora_vkn::vk;
use re_flora_vkn::{
    Allocator, Buffer, BufferUsage, Extent3D, ImageDesc, MemoryLocation, SamplerDesc, Texture,
    TextureLayout, VulkanContext,
};

const DDGI_IRRADIANCE_FORMAT: vk::Format = vk::Format::R32G32B32A32_SFLOAT;
const DDGI_VISIBILITY_FORMAT: vk::Format = vk::Format::R32G32_SFLOAT;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct DdgiProbeMetadataGpu {
    pub nominal_position_and_min_clearance: [f32; 4],
    pub actual_position_and_clearance: [f32; 4],
    pub state_and_reserved: [u32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DdgiResourceBytes {
    pub irradiance_atlas: u64,
    pub visibility_atlas: u64,
    pub global_sky_irradiance: u64,
    pub probe_metadata: u64,
    pub transient_ray_data: u64,
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
                * std::mem::size_of::<DdgiProbeMetadataGpu>() as u64,
            transient_ray_data: DDGI_PROBE_BATCH_SIZE as u64
                * DDGI_RAYS_PER_PROBE as u64
                * std::mem::size_of::<[f32; 4]>() as u64,
        }
    }

    pub fn total(self) -> u64 {
        self.irradiance_atlas
            + self.visibility_atlas
            + self.global_sky_irradiance
            + self.probe_metadata
            + self.transient_ray_data
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub enum DdgiVolumeStage {
    #[default]
    Allocated,
    GlobalSkyReady,
    Relocated,
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
    pub ddgi_probe_metadata: Resource<Buffer>,
    pub ddgi_transient_ray_data: Resource<Buffer>,
    pub ddgi_irradiance_atlas: Resource<Texture>,
    pub ddgi_visibility_atlas: Resource<Texture>,
    pub ddgi_global_sky_irradiance: Resource<Texture>,
}

impl ResourceContainer for DdgiVolume {
    fn get_buffer(&self, name: &str) -> Option<&Buffer> {
        match name {
            "ddgi_probe_metadata" => Some(&self.ddgi_probe_metadata),
            "ddgi_transient_ray_data" => Some(&self.ddgi_transient_ray_data),
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
            "[DDGI] allocated stage=allocated spacing_voxels={} grid={}x{}x{} probes={} irradiance={}x{} RGBA32F visibility={}x{} RG32F ray_batch={}x{} metadata_bytes={} irradiance_bytes={} visibility_bytes={} ray_bytes={} global_sky_bytes={} total_mib={:.2}",
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
            resource_bytes.global_sky_irradiance,
            resource_bytes.total() as f64 / (1024.0 * 1024.0),
        );

        Ok(Self {
            grid,
            irradiance_layout,
            visibility_layout,
            resource_bytes,
            stage: DdgiVolumeStage::Allocated,
            ddgi_probe_metadata: Resource::new(probe_metadata),
            ddgi_transient_ray_data: Resource::new(transient_ray_data),
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
        }
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
        };
        assert!(!status.is_ready());
    }
}
