//! Dynamic diffuse global illumination domain types.
//!
//! Atlas addressing and octahedral wrapping live here so callers can work in probe-space without
//! depending on the physical GPU texture layout.

mod atlas;
mod capture;
#[cfg_attr(not(test), allow(dead_code))]
mod octahedral;
mod resources;
mod runtime;
// This host-only seam is intentionally landed before its tracer/volume integration.
#[allow(dead_code)]
mod scheduler;
mod terrain_refresh;
mod voxel_visibility;

pub use atlas::{
    supported_ddgi_spacings_label, validate_ddgi_spacing, DdgiAtlasLayout, DdgiVolumeGrid,
    DDGI_GUTTER_WORKGROUP_SIZE, DDGI_IRRADIANCE_INTERIOR_SIDE, DDGI_IRRADIANCE_STORED_SIDE,
    DDGI_PROBE_BATCH_SIZE, DDGI_RAYS_PER_PROBE, DDGI_RELOCATION_WORKGROUP_SIZE,
    DDGI_TRACE_WORKGROUP_SIZE, DDGI_VISIBILITY_INTERIOR_SIDE, DEFAULT_DDGI_SPACING_VOXELS,
    SUPPORTED_DDGI_SPACINGS_VOXELS,
};
pub use capture::{DdgiCaptureCheckpoint, DdgiCapturePublication, DdgiCaptureTarget};
// These identities and diagnostics form the capture/analysis seam even when the game binary does
// not directly name every exported type in a particular build.
#[allow(unused_imports)]
pub use resources::{
    DdgiAtlasValidationStats, DdgiBatchOrder, DdgiConvergencePolicy, DdgiRayBatch,
    DdgiResourceBytes, DdgiValidatedIterationOutcome, DdgiVerifiedBatchOutcome, DdgiVolume,
    DdgiVolumeStage, DdgiVolumeStatus, DdgiVolumes, DDGI_CONVERGENCE_POLICY,
};
pub use runtime::DdgiRuntimeStatus;
#[allow(unused_imports)]
pub use scheduler::{
    DdgiFieldIdentity, DdgiFieldIdentityError, DdgiFieldKey, DdgiFieldStage, DdgiScheduledWork,
    DdgiScheduledWorkKind, DdgiSchedulerError, DdgiTransportScheduler,
};
pub use terrain_refresh::{DdgiBuildKind, DdgiBuildToken, DdgiRefreshState, DdgiTerrainRefresh};
pub use voxel_visibility::DdgiVoxelVisibility;

/// Terrain-only hard-visibility origin variants used to isolate voxel receiver self-occlusion.
/// The DDGI surface anchor and filtered moment query remain unchanged for every variant.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum DdgiTerrainHardOrigin {
    SurfaceQuarterVoxel = 0,
    CenterFixedWorld = 1,
    #[default]
    SurfaceFixedWorld = 2,
}

impl DdgiTerrainHardOrigin {
    pub fn from_cli_value(value: &str) -> Option<Self> {
        match value {
            "surface-quarter" => Some(Self::SurfaceQuarterVoxel),
            "center-fixed" => Some(Self::CenterFixedWorld),
            "surface-fixed" => Some(Self::SurfaceFixedWorld),
            _ => None,
        }
    }

    pub const fn as_u32(self) -> u32 {
        self as u32
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::SurfaceQuarterVoxel => "surface-quarter",
            Self::CenterFixedWorld => "center-fixed",
            Self::SurfaceFixedWorld => "surface-fixed",
        }
    }
}

/// Permanent DDGI diagnostics. Exact modes expose the packed-voxel visibility gate separately
/// from the filtered moment term used by the final query.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum DdgiDebugView {
    #[default]
    Final = 0,
    MomentVisibility = 1,
    ExactVisibility = 2,
    VisibilityError = 3,
    ExactIrradiance = 4,
    IrradianceError = 5,
    WeightSum = 6,
    DominantProbe = 7,
    ProbeState = 8,
    Relocation = 9,
    IrradianceAtlas = 10,
    VisibilityAtlas = 11,
}

impl DdgiDebugView {
    pub fn from_cli_value(value: &str) -> Option<Self> {
        match value {
            "final" => Some(Self::Final),
            "moment-visibility" => Some(Self::MomentVisibility),
            "exact-visibility" => Some(Self::ExactVisibility),
            "visibility-error" => Some(Self::VisibilityError),
            "exact-irradiance" => Some(Self::ExactIrradiance),
            "irradiance-error" => Some(Self::IrradianceError),
            "weight-sum" => Some(Self::WeightSum),
            "dominant-probe" => Some(Self::DominantProbe),
            "probe-state" => Some(Self::ProbeState),
            "relocation" => Some(Self::Relocation),
            "irradiance-atlas" => Some(Self::IrradianceAtlas),
            "visibility-atlas" => Some(Self::VisibilityAtlas),
            _ => None,
        }
    }

    pub const fn as_u32(self) -> u32 {
        self as u32
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Final => "final",
            Self::MomentVisibility => "moment-visibility",
            Self::ExactVisibility => "exact-visibility",
            Self::VisibilityError => "visibility-error",
            Self::ExactIrradiance => "exact-irradiance",
            Self::IrradianceError => "irradiance-error",
            Self::WeightSum => "weight-sum",
            Self::DominantProbe => "dominant-probe",
            Self::ProbeState => "probe-state",
            Self::Relocation => "relocation",
            Self::IrradianceAtlas => "irradiance-atlas",
            Self::VisibilityAtlas => "visibility-atlas",
        }
    }
}
