use crate::resource::{Resource, ResourceContainer};
use anyhow::{ensure, Result};
use bytemuck::{Pod, Zeroable};
use glam::{UVec3, Vec3};
use re_flora_vkn::vk;
use re_flora_vkn::{Allocator, Buffer, BufferUsage, Device, MemoryLocation};

pub const SUPPORTED_ENVIRONMENT_PROBE_SPACINGS_VOXELS: [u32; 4] = [64, 32, 16, 8];
pub const DEFAULT_ENVIRONMENT_PROBE_SPACING_VOXELS: u32 = 32;
pub const ENVIRONMENT_PROBE_SH_COEFFICIENT_COUNT: usize = 9;
const ENVIRONMENT_PROBE_MARKER_INDEX_COUNT: u32 = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
#[allow(dead_code)]
pub enum EnvironmentProbeState {
    Inactive = 0,
    InsideSolid = 1,
    RelocationPending = 2,
    Valid = 3,
    Dirty = 4,
    Updating = 5,
    RelocationFailed = 6,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(dead_code)]
pub struct EnvironmentProbeInterpolationCell {
    pub base: UVec3,
    pub fraction: Vec3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnvironmentProbeGrid {
    world_extent_voxels: UVec3,
    spacing_voxels: u32,
    dimensions: UVec3,
    probe_count: u32,
}

impl EnvironmentProbeGrid {
    pub fn new(world_extent_voxels: UVec3, spacing_voxels: u32) -> Result<Self> {
        ensure!(
            SUPPORTED_ENVIRONMENT_PROBE_SPACINGS_VOXELS.contains(&spacing_voxels),
            "unsupported environment probe spacing {spacing_voxels}; supported values: {}",
            supported_environment_probe_spacings_label()
        );
        ensure!(
            world_extent_voxels.cmpgt(UVec3::ZERO).all(),
            "environment probe world extent must be non-zero"
        );
        ensure!(
            (world_extent_voxels % UVec3::splat(spacing_voxels)) == UVec3::ZERO,
            "environment probe spacing {spacing_voxels} must divide world extent {world_extent_voxels:?}"
        );

        let dimensions = world_extent_voxels / spacing_voxels + UVec3::ONE;
        let probe_count_u64 = dimensions.x as u64 * dimensions.y as u64 * dimensions.z as u64;
        ensure!(
            probe_count_u64 <= u32::MAX as u64,
            "environment probe count {probe_count_u64} exceeds u32"
        );

        Ok(Self {
            world_extent_voxels,
            spacing_voxels,
            dimensions,
            probe_count: probe_count_u64 as u32,
        })
    }

    pub fn spacing_voxels(self) -> u32 {
        self.spacing_voxels
    }

    pub fn dimensions(self) -> UVec3 {
        self.dimensions
    }

    pub fn probe_count(self) -> u32 {
        self.probe_count
    }

    pub fn flatten(self, coordinate: UVec3) -> Option<u32> {
        if coordinate.cmplt(self.dimensions).all() {
            Some(
                coordinate.x
                    + self.dimensions.x * (coordinate.y + self.dimensions.y * coordinate.z),
            )
        } else {
            None
        }
    }

    pub fn unflatten(self, index: u32) -> Option<UVec3> {
        if index >= self.probe_count {
            return None;
        }
        let xy = self.dimensions.x * self.dimensions.y;
        let z = index / xy;
        let remainder = index % xy;
        let y = remainder / self.dimensions.x;
        let x = remainder % self.dimensions.x;
        Some(UVec3::new(x, y, z))
    }

    pub fn grid_to_voxel_position(self, coordinate: UVec3) -> Option<Vec3> {
        self.flatten(coordinate)
            .map(|_| coordinate.as_vec3() * self.spacing_voxels as f32)
    }

    pub fn grid_to_world_position(
        self,
        coordinate: UVec3,
        voxels_per_world_unit: UVec3,
    ) -> Option<Vec3> {
        self.grid_to_voxel_position(coordinate)
            .map(|position| position / voxels_per_world_unit.as_vec3())
    }

    #[allow(dead_code)]
    pub fn interpolation_cell_voxels(
        self,
        voxel_position: Vec3,
    ) -> EnvironmentProbeInterpolationCell {
        let grid_max = (self.dimensions - UVec3::ONE).as_vec3();
        let grid_position =
            (voxel_position / self.spacing_voxels as f32).clamp(Vec3::ZERO, grid_max);
        let base = grid_position
            .floor()
            .as_uvec3()
            .min(self.dimensions - UVec3::splat(2));
        EnvironmentProbeInterpolationCell {
            base,
            fraction: grid_position - base.as_vec3(),
        }
    }
}

pub fn validate_environment_probe_spacing(spacing_voxels: u32) -> Result<u32, String> {
    if SUPPORTED_ENVIRONMENT_PROBE_SPACINGS_VOXELS.contains(&spacing_voxels) {
        Ok(spacing_voxels)
    } else {
        Err(format!(
            "Unsupported --environment-probe-spacing-voxels '{spacing_voxels}'. Supported values: {}",
            supported_environment_probe_spacings_label()
        ))
    }
}

pub fn supported_environment_probe_spacings_label() -> String {
    SUPPORTED_ENVIRONMENT_PROBE_SPACINGS_VOXELS
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct EnvironmentProbeSummaryGpu {
    pub original_position_visibility: [f32; 4],
    pub sample_position_confidence: [f32; 4],
    pub revisions_state: [u32; 4],
    pub update_info: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct EnvironmentProbeCoefficientsGpu {
    pub coefficients: [[f32; 4]; ENVIRONMENT_PROBE_SH_COEFFICIENT_COUNT],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnvironmentProbeResourceBytes {
    pub coefficients: u64,
    pub summaries: u64,
}

impl EnvironmentProbeResourceBytes {
    pub fn for_grid(grid: EnvironmentProbeGrid) -> Self {
        Self {
            coefficients: std::mem::size_of::<EnvironmentProbeCoefficientsGpu>() as u64
                * grid.probe_count() as u64,
            summaries: std::mem::size_of::<EnvironmentProbeSummaryGpu>() as u64
                * grid.probe_count() as u64,
        }
    }

    pub fn total(self) -> u64 {
        self.coefficients + self.summaries
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnvironmentProbeVolumeStatus {
    pub grid: EnvironmentProbeGrid,
    pub valid_probe_count: u32,
    pub resource_bytes: EnvironmentProbeResourceBytes,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum EnvironmentProbeVisualizationMode {
    #[default]
    State = 0,
    SkyVisibility = 1,
    Irradiance = 2,
    AgeRevision = 3,
    Relocation = 4,
}

impl EnvironmentProbeVisualizationMode {
    pub const ALL: [Self; 5] = [
        Self::State,
        Self::SkyVisibility,
        Self::Irradiance,
        Self::AgeRevision,
        Self::Relocation,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::State => "State",
            Self::SkyVisibility => "Sky visibility",
            Self::Irradiance => "Irradiance",
            Self::AgeRevision => "Age / revision",
            Self::Relocation => "Relocation",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum EnvironmentProbeVisualizationFilter {
    #[default]
    All = 0,
    Valid = 1,
    Invalid = 2,
    DirtyOrUpdating = 3,
}

impl EnvironmentProbeVisualizationFilter {
    pub const ALL: [Self; 4] = [Self::All, Self::Valid, Self::Invalid, Self::DirtyOrUpdating];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Valid => "Valid only",
            Self::Invalid => "Invalid only",
            Self::DirtyOrUpdating => "Dirty / updating",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvironmentProbeVisualizationSettings {
    pub enabled: bool,
    pub mode: EnvironmentProbeVisualizationMode,
    pub filter: EnvironmentProbeVisualizationFilter,
    pub camera_radius_voxels: f32,
    pub instance_stride: u32,
    pub marker_size_voxels: f32,
    pub depth_tested: bool,
    pub age_range_frames: u32,
}

impl Default for EnvironmentProbeVisualizationSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: EnvironmentProbeVisualizationMode::State,
            filter: EnvironmentProbeVisualizationFilter::All,
            camera_radius_voxels: 0.0,
            instance_stride: 1,
            marker_size_voxels: 2.0,
            depth_tested: true,
            age_range_frames: 120,
        }
    }
}

impl EnvironmentProbeVisualizationSettings {
    pub fn sanitized(mut self) -> Self {
        self.camera_radius_voxels = self.camera_radius_voxels.clamp(0.0, 2048.0);
        self.instance_stride = self.instance_stride.clamp(1, 256);
        self.marker_size_voxels = self.marker_size_voxels.clamp(0.25, 32.0);
        self.age_range_frames = self.age_range_frames.clamp(1, 1_000_000);
        self
    }

    pub fn submitted_instance_count(self, probe_count: u32) -> u32 {
        if !self.enabled || probe_count == 0 {
            return 0;
        }
        let stride = self.instance_stride.max(1);
        let strided_count = probe_count.div_ceil(stride);
        if self.mode == EnvironmentProbeVisualizationMode::Relocation {
            strided_count.saturating_mul(2)
        } else {
            strided_count
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct EnvironmentProbeVisualizationPushConstants {
    pub display_mode: u32,
    pub filter: u32,
    pub instance_stride: u32,
    pub always_visible: u32,
    pub marker_size_world: f32,
    pub camera_radius_world: f32,
    pub inverse_age_range_frames: f32,
    pub _padding: f32,
}

impl EnvironmentProbeVisualizationPushConstants {
    pub fn new(
        settings: EnvironmentProbeVisualizationSettings,
        voxels_per_world_unit: UVec3,
    ) -> Self {
        let settings = settings.sanitized();
        let world_scale = 1.0 / voxels_per_world_unit.x.max(1) as f32;
        Self {
            display_mode: settings.mode as u32,
            filter: settings.filter as u32,
            instance_stride: settings.instance_stride,
            always_visible: u32::from(!settings.depth_tested),
            marker_size_world: settings.marker_size_voxels * world_scale,
            camera_radius_world: settings.camera_radius_voxels * world_scale,
            inverse_age_range_frames: 1.0 / settings.age_range_frames as f32,
            _padding: 0.0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct EnvironmentProbeMarkerVertex {
    pub local_position: [f32; 2],
}

pub struct EnvironmentProbeVisualizationResources {
    pub marker_vertices: Resource<Buffer>,
    pub marker_indices: Resource<Buffer>,
}

impl EnvironmentProbeVisualizationResources {
    pub fn new(device: Device, allocator: Allocator) -> Self {
        let vertices = [
            EnvironmentProbeMarkerVertex {
                local_position: [-1.0, 0.0],
            },
            EnvironmentProbeMarkerVertex {
                local_position: [0.0, -1.0],
            },
            EnvironmentProbeMarkerVertex {
                local_position: [1.0, 0.0],
            },
            EnvironmentProbeMarkerVertex {
                local_position: [0.0, 1.0],
            },
        ];
        let indices = [0_u32, 1, 2, 0, 2, 3];
        let marker_vertices = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::VERTEX_BUFFER),
            MemoryLocation::CpuToGpu,
            std::mem::size_of_val(&vertices) as u64,
        );
        marker_vertices
            .fill(&vertices)
            .expect("environment probe marker vertex upload must fit");
        let marker_indices = Buffer::new_sized(
            device,
            allocator,
            BufferUsage::from_flags(vk::BufferUsageFlags::INDEX_BUFFER),
            MemoryLocation::CpuToGpu,
            std::mem::size_of_val(&indices) as u64,
        );
        marker_indices
            .fill(&indices)
            .expect("environment probe marker index upload must fit");
        Self {
            marker_vertices: Resource::new(marker_vertices),
            marker_indices: Resource::new(marker_indices),
        }
    }

    pub fn index_count(&self) -> u32 {
        ENVIRONMENT_PROBE_MARKER_INDEX_COUNT
    }
}

pub struct EnvironmentProbeVolume {
    grid: EnvironmentProbeGrid,
    valid_probe_count: u32,
    resource_bytes: EnvironmentProbeResourceBytes,
    pub environment_probe_coefficients: Resource<Buffer>,
    pub environment_probe_summaries: Resource<Buffer>,
}

impl ResourceContainer for EnvironmentProbeVolume {
    fn get_buffer(&self, name: &str) -> Option<&Buffer> {
        match name {
            "environment_probe_coefficients" => Some(&self.environment_probe_coefficients),
            "environment_probe_summaries" => Some(&self.environment_probe_summaries),
            _ => None,
        }
    }

    fn get_texture(&self, _name: &str) -> Option<&re_flora_vkn::Texture> {
        None
    }

    fn get_resource_names(&self) -> Vec<&'static str> {
        vec![
            "environment_probe_coefficients",
            "environment_probe_summaries",
        ]
    }
}

impl EnvironmentProbeVolume {
    pub fn new(
        device: Device,
        allocator: Allocator,
        world_extent_voxels: UVec3,
        voxels_per_world_unit: UVec3,
        spacing_voxels: u32,
    ) -> Result<Self> {
        ensure!(
            voxels_per_world_unit.cmpgt(UVec3::ZERO).all(),
            "voxels per world unit must be non-zero"
        );
        let grid = EnvironmentProbeGrid::new(world_extent_voxels, spacing_voxels)?;
        let probe_count = grid.probe_count() as usize;
        let resource_bytes = EnvironmentProbeResourceBytes::for_grid(grid);

        let coefficients = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            MemoryLocation::CpuToGpu,
            resource_bytes.coefficients,
        );
        coefficients.fill(&vec![
            EnvironmentProbeCoefficientsGpu::zeroed();
            probe_count
        ])?;

        let summaries = Buffer::new_sized(
            device,
            allocator,
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            MemoryLocation::CpuToGpu,
            resource_bytes.summaries,
        );
        let summary_data = (0..grid.probe_count())
            .map(|index| {
                let coordinate = grid
                    .unflatten(index)
                    .expect("probe index generated from grid count must be valid");
                let world_position = grid
                    .grid_to_world_position(coordinate, voxels_per_world_unit)
                    .expect("probe coordinate generated from grid dimensions must be valid");
                EnvironmentProbeSummaryGpu {
                    original_position_visibility: [
                        world_position.x,
                        world_position.y,
                        world_position.z,
                        0.0,
                    ],
                    sample_position_confidence: [
                        world_position.x,
                        world_position.y,
                        world_position.z,
                        0.0,
                    ],
                    revisions_state: [0, 0, EnvironmentProbeState::Inactive as u32, index],
                    update_info: [0; 4],
                }
            })
            .collect::<Vec<_>>();
        summaries.fill(&summary_data)?;

        log::info!(
            "[ENV_PROBES] allocated spacing_voxels={} grid={}x{}x{} probes={} valid=0 coefficients_bytes={} summaries_bytes={} total_bytes={} total_mib={:.2}",
            spacing_voxels,
            grid.dimensions().x,
            grid.dimensions().y,
            grid.dimensions().z,
            grid.probe_count(),
            resource_bytes.coefficients,
            resource_bytes.summaries,
            resource_bytes.total(),
            resource_bytes.total() as f64 / (1024.0 * 1024.0),
        );

        Ok(Self {
            grid,
            valid_probe_count: 0,
            resource_bytes,
            environment_probe_coefficients: Resource::new(coefficients),
            environment_probe_summaries: Resource::new(summaries),
        })
    }

    pub fn status(&self) -> EnvironmentProbeVolumeStatus {
        EnvironmentProbeVolumeStatus {
            grid: self.grid,
            valid_probe_count: self.valid_probe_count,
            resource_bytes: self.resource_bytes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORLD_EXTENT: UVec3 = UVec3::splat(512);

    #[test]
    fn supported_spacings_cover_world_endpoints() {
        for spacing in SUPPORTED_ENVIRONMENT_PROBE_SPACINGS_VOXELS {
            let grid = EnvironmentProbeGrid::new(WORLD_EXTENT, spacing).unwrap();
            let last = grid.dimensions() - UVec3::ONE;
            assert_eq!(grid.grid_to_voxel_position(UVec3::ZERO), Some(Vec3::ZERO));
            assert_eq!(
                grid.grid_to_voxel_position(last),
                Some(WORLD_EXTENT.as_vec3())
            );
            assert_eq!(
                grid.dimensions(),
                UVec3::splat(WORLD_EXTENT.x / spacing + 1)
            );
        }
    }

    #[test]
    fn flattening_round_trips_every_probe() {
        let grid = EnvironmentProbeGrid::new(WORLD_EXTENT, 32).unwrap();
        for index in 0..grid.probe_count() {
            let coordinate = grid.unflatten(index).unwrap();
            assert_eq!(grid.flatten(coordinate), Some(index));
        }
        assert_eq!(grid.flatten(grid.dimensions()), None);
        assert_eq!(grid.unflatten(grid.probe_count()), None);
    }

    #[test]
    fn interpolation_coordinates_clamp_at_world_edges() {
        let grid = EnvironmentProbeGrid::new(WORLD_EXTENT, 32).unwrap();
        assert_eq!(
            grid.interpolation_cell_voxels(Vec3::splat(-4.0)),
            EnvironmentProbeInterpolationCell {
                base: UVec3::ZERO,
                fraction: Vec3::ZERO,
            }
        );
        assert_eq!(
            grid.interpolation_cell_voxels(Vec3::splat(16.0)),
            EnvironmentProbeInterpolationCell {
                base: UVec3::ZERO,
                fraction: Vec3::splat(0.5),
            }
        );
        assert_eq!(
            grid.interpolation_cell_voxels(WORLD_EXTENT.as_vec3()),
            EnvironmentProbeInterpolationCell {
                base: grid.dimensions() - UVec3::splat(2),
                fraction: Vec3::ONE,
            }
        );
    }

    #[test]
    fn invalid_spacing_is_rejected() {
        assert!(EnvironmentProbeGrid::new(WORLD_EXTENT, 24).is_err());
        assert!(validate_environment_probe_spacing(24).is_err());
        assert_eq!(validate_environment_probe_spacing(16), Ok(16));
    }

    #[test]
    fn resource_layout_matches_documented_lower_bounds() {
        assert_eq!(
            std::mem::size_of::<EnvironmentProbeCoefficientsGpu>(),
            9 * 16
        );
        assert_eq!(std::mem::size_of::<EnvironmentProbeSummaryGpu>(), 4 * 16);

        let grid = EnvironmentProbeGrid::new(WORLD_EXTENT, 16).unwrap();
        assert_eq!(grid.dimensions(), UVec3::splat(33));
        assert_eq!(grid.probe_count(), 35_937);
    }

    #[test]
    fn visualization_stride_and_relocation_control_submitted_instances() {
        let settings = EnvironmentProbeVisualizationSettings {
            enabled: true,
            instance_stride: 4,
            ..Default::default()
        };
        assert_eq!(settings.submitted_instance_count(10), 3);
        assert_eq!(
            EnvironmentProbeVisualizationSettings {
                mode: EnvironmentProbeVisualizationMode::Relocation,
                ..settings
            }
            .submitted_instance_count(10),
            6
        );
        assert_eq!(
            EnvironmentProbeVisualizationSettings {
                enabled: false,
                ..settings
            }
            .submitted_instance_count(10),
            0
        );
    }
}
