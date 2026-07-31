//! Debug visualization support for the DDGI probe volume.
//!
//! The production probe representation lives in `crate::ddgi`. This module only owns the small
//! billboard mesh and user-facing visualization controls.

use crate::resource::Resource;
use bytemuck::{Pod, Zeroable};
use glam::UVec3;
use re_flora_vkn::vk;
use re_flora_vkn::{Allocator, Buffer, BufferUsage, Device, MemoryLocation};

const ENVIRONMENT_PROBE_MARKER_INDEX_COUNT: u32 = 6;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum EnvironmentProbeVisualizationMode {
    #[default]
    State = 0,
    Irradiance = 1,
    Relocation = 2,
}

impl EnvironmentProbeVisualizationMode {
    pub const ALL: [Self; 3] = [Self::State, Self::Irradiance, Self::Relocation];

    pub fn label(self) -> &'static str {
        match self {
            Self::State => "State",
            Self::Irradiance => "Irradiance",
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
}

impl EnvironmentProbeVisualizationFilter {
    pub const ALL: [Self; 3] = [Self::All, Self::Valid, Self::Invalid];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Valid => "Valid only",
            Self::Invalid => "Invalid only",
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
        }
    }
}

impl EnvironmentProbeVisualizationSettings {
    pub fn sanitized(mut self) -> Self {
        self.camera_radius_voxels = self.camera_radius_voxels.clamp(0.0, 2048.0);
        self.instance_stride = self.instance_stride.clamp(1, 256);
        self.marker_size_voxels = self.marker_size_voxels.clamp(0.25, 32.0);
        self
    }

    pub fn submitted_instance_count(self, probe_count: u32) -> u32 {
        if !self.enabled || probe_count == 0 {
            return 0;
        }
        let strided_count = probe_count.div_ceil(self.instance_stride.max(1));
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
    pub _padding: [f32; 2],
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
            _padding: [0.0; 2],
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
