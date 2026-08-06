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
    DdgiVolumeStage, DdgiVolumes, DDGI_CONVERGENCE_POLICY,
};
pub(crate) use runtime::{DdgiRuntime, DdgiRuntimeVolumeTarget};
#[allow(unused_imports)]
pub use runtime::{DdgiRuntimeStatus, DdgiRuntimeTargetWork, DdgiRuntimeVolumeStatus};
#[allow(unused_imports)]
pub use scheduler::{
    DdgiFieldIdentity, DdgiFieldIdentityError, DdgiFieldKey, DdgiFieldStage, DdgiScheduledWorkKind,
};
pub(crate) use scheduler::{DdgiScheduledWork, DdgiSchedulerError, DdgiTransportScheduler};
pub(crate) use terrain_refresh::DdgiTerrainRefresh;
pub use terrain_refresh::{DdgiBuildKind, DdgiBuildToken, DdgiRefreshState};
pub use voxel_visibility::DdgiVoxelVisibility;

/// Experimental visibility terms used by runtime DDGI consumers.
///
/// Probe transport always uses the full visibility model. These modes only exist to isolate the
/// steady-state terrain/raster query cost before choosing an optimization.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum DdgiConsumerVisibility {
    #[default]
    Full = 0,
    MomentOnly = 1,
    ExactOnly = 2,
    None = 3,
}

impl DdgiConsumerVisibility {
    pub fn from_cli_value(value: &str) -> Option<Self> {
        match value {
            "full" => Some(Self::Full),
            "moment-only" => Some(Self::MomentOnly),
            "exact-only" => Some(Self::ExactOnly),
            "none" => Some(Self::None),
            _ => None,
        }
    }

    pub const fn as_u32(self) -> u32 {
        self as u32
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::MomentOnly => "moment-only",
            Self::ExactOnly => "exact-only",
            Self::None => "none",
        }
    }
}

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
    UnoccludedIrradiance = 12,
    EqualWeightIrradiance = 13,
    RawCageIrradiance = 14,
    SpatialWeightCurrent = 15,
    SpatialWeightNominal = 16,
    SpatialWeightWrap = 17,
    SpatialWeightNominalWrap = 18,
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
            "unoccluded-irradiance" => Some(Self::UnoccludedIrradiance),
            "equal-weight-irradiance" => Some(Self::EqualWeightIrradiance),
            "raw-cage-irradiance" => Some(Self::RawCageIrradiance),
            "spatial-weight-current" => Some(Self::SpatialWeightCurrent),
            "spatial-weight-nominal" => Some(Self::SpatialWeightNominal),
            "spatial-weight-wrap" => Some(Self::SpatialWeightWrap),
            "spatial-weight-nominal-wrap" => Some(Self::SpatialWeightNominalWrap),
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
            Self::UnoccludedIrradiance => "unoccluded-irradiance",
            Self::EqualWeightIrradiance => "equal-weight-irradiance",
            Self::RawCageIrradiance => "raw-cage-irradiance",
            Self::SpatialWeightCurrent => "spatial-weight-current",
            Self::SpatialWeightNominal => "spatial-weight-nominal",
            Self::SpatialWeightWrap => "spatial-weight-wrap",
            Self::SpatialWeightNominalWrap => "spatial-weight-nominal-wrap",
        }
    }
}
