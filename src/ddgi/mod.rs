//! Dynamic diffuse global illumination domain types.
//!
//! Atlas addressing and octahedral wrapping live here so callers can work in probe-space without
//! depending on the physical GPU texture layout.

mod atlas;
mod capture;
mod config;
#[cfg_attr(not(test), allow(dead_code))]
mod octahedral;
mod resources;
mod runtime;
// This host-only seam is intentionally landed before its tracer/volume integration.
#[allow(dead_code)]
mod scheduler;
mod terrain_refresh;
mod voxel_visibility;

/// Fixed-layout diagnostic readback for the saved-terrain DDGI seam fixture.
///
/// The shader writes six receiver records, each containing one receiver float4, eight probe
/// records with six float4s each, and four aggregate result float4s. Keep this layout explicit so
/// the text writer and shader cannot silently disagree about the evidence being inspected.
pub(crate) const DDGI_SPATIAL_WEIGHT_READBACK_RECEIVER_COUNT: usize = 6;
pub(crate) const DDGI_SPATIAL_WEIGHT_READBACK_PROBE_COUNT: usize = 8;
pub(crate) const DDGI_SPATIAL_WEIGHT_READBACK_FLOAT4S_PER_PROBE: usize = 6;
pub(crate) const DDGI_SPATIAL_WEIGHT_READBACK_FLOAT4S_PER_RECEIVER: usize = 1
    + DDGI_SPATIAL_WEIGHT_READBACK_PROBE_COUNT * DDGI_SPATIAL_WEIGHT_READBACK_FLOAT4S_PER_PROBE
    + 4;
pub(crate) const DDGI_SPATIAL_WEIGHT_READBACK_BYTE_COUNT: usize =
    DDGI_SPATIAL_WEIGHT_READBACK_RECEIVER_COUNT
        * DDGI_SPATIAL_WEIGHT_READBACK_FLOAT4S_PER_RECEIVER
        * std::mem::size_of::<[f32; 4]>();

/// Half-resolution tracer pixels corresponding to full-resolution pixels from the fixed 2880x1620
/// crop used by the saved-terrain repro. The tracer renders at a 0.5 scaling factor before the
/// screen-output upscale.
pub(crate) const DDGI_SPATIAL_WEIGHT_READBACK_PIXELS: [[u32; 2];
    DDGI_SPATIAL_WEIGHT_READBACK_RECEIVER_COUNT] = [
    [824, 370],
    [824, 376],
    [824, 382],
    [824, 395],
    [824, 401],
    [824, 407],
];

pub use atlas::{
    supported_ddgi_spacings_label, validate_ddgi_spacing, DdgiAtlasLayout, DdgiVolumeGrid,
    DDGI_GUTTER_WORKGROUP_SIZE, DDGI_IRRADIANCE_INTERIOR_SIDE, DDGI_IRRADIANCE_STORED_SIDE,
    DDGI_RELOCATION_WORKGROUP_SIZE, DDGI_TRACE_WORKGROUP_SIZE, DDGI_VISIBILITY_INTERIOR_SIDE,
    DEFAULT_DDGI_SPACING_VOXELS, SUPPORTED_DDGI_SPACINGS_VOXELS,
};
pub use capture::{DdgiCaptureCheckpoint, DdgiCapturePublication, DdgiCaptureTarget};
pub use config::{
    DDGI_LOCAL_RECOVERY_MAX_ABSOLUTE_DELTA, DDGI_LOCAL_RECOVERY_MIN_EPOCH,
    DDGI_LOCAL_RECOVERY_STABLE_EPOCHS, DDGI_PROBE_BATCH_SIZE, DDGI_RAYS_PER_PROBE,
    DDGI_RAY_BUDGET_PER_FRAME, DDGI_TOPOLOGY_RECOVERY_HISTORY_RETENTION,
};
// These identities and diagnostics form the capture/analysis seam even when the game binary does
// not directly name every exported type in a particular build.
#[allow(unused_imports)]
pub use resources::{
    DdgiAtlasValidationStats, DdgiBatchOrder, DdgiConvergencePolicy, DdgiConvergenceReason,
    DdgiLocalLightTraceTotals, DdgiProbePriority, DdgiProbePriorityReason, DdgiRayBatch,
    DdgiResourceBytes, DdgiTraceStats, DdgiValidatedIterationOutcome, DdgiVerifiedBatchOutcome,
    DdgiVolume, DdgiVolumeStage, DdgiVolumes, DDGI_CONVERGENCE_POLICY,
};
pub(crate) use runtime::{
    DdgiLightingDiagnostics, DdgiRuntime, DdgiRuntimeVolumeBuild, DdgiRuntimeVolumeTarget,
    DdgiVolumePublication,
};
#[allow(unused_imports)]
pub use runtime::{DdgiRuntimeStatus, DdgiRuntimeTargetWork, DdgiRuntimeVolumeStatus};
#[allow(unused_imports)]
pub use scheduler::{
    DdgiFieldIdentity, DdgiFieldIdentityError, DdgiFieldKey, DdgiFieldState, DdgiScheduledWorkKind,
};
pub(crate) use scheduler::{DdgiScheduledWork, DdgiSchedulerError, DdgiTransportScheduler};
pub(crate) use terrain_refresh::DdgiTerrainRefresh;
pub use terrain_refresh::{DdgiBuildKind, DdgiBuildToken, DdgiRefreshState};
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
    UnoccludedIrradiance = 12,
    EqualWeightIrradiance = 13,
    RawCageIrradiance = 14,
    SpatialWeightCurrent = 15,
    SpatialWeightNominal = 16,
    SpatialWeightWrap = 17,
    SpatialWeightNominalWrap = 18,
    SpatialWeightReadback = 19,
    SpatialWeightCurrentNoSurface = 20,
    SpatialWeightNominalNoSurface = 21,
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
            "spatial-weight-readback" => Some(Self::SpatialWeightReadback),
            "spatial-weight-current-no-surface" => Some(Self::SpatialWeightCurrentNoSurface),
            "spatial-weight-nominal-no-surface" => Some(Self::SpatialWeightNominalNoSurface),
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
            Self::SpatialWeightReadback => "spatial-weight-readback",
            Self::SpatialWeightCurrentNoSurface => "spatial-weight-current-no-surface",
            Self::SpatialWeightNominalNoSurface => "spatial-weight-nominal-no-surface",
        }
    }
}
