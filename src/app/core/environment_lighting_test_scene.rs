use super::placeables::{SprinklerPlacementTarget, SPRINKLER_HEAD_EMITTER_PART};
use super::App;
use crate::app::world_edits::{BuildEdit, TerrainRemovalEdit, VoxelEdit, WorldEditPlan};
use crate::builder::{VOXEL_TYPE_EMISSIVE, VOXEL_TYPE_EMPTY, VOXEL_TYPE_ROCK, VOXEL_TYPE_SAND};
use crate::ddgi::{
    DdgiBuildKind, DdgiFieldIdentity, DdgiFieldState, DdgiProbePriorityReason, DdgiRefreshState,
    DdgiScheduledWorkKind, DdgiVolumeStage, DDGI_PROBE_BATCH_SIZE,
};
use crate::geom::{build_bvh, Cuboid, UAabb3};
use crate::lighting::{
    EmissiveVoxelProvider, LightId, LocalLight, LocalLightRecord, LocalLightSnapshot, PointLight,
    RasterEmitterComponent, RasterEmitterKey, RasterEntityId, SpotLight,
    AUTHORED_LOCAL_LIGHT_PROVIDER_ID, EMISSIVE_VOXEL_PROVIDER_ID, LOCAL_LIGHT_GPU_CAPACITY,
    RASTER_ENTITY_LIGHT_PROVIDER_ID,
};
use crate::EnvironmentLightingTestCase;
use anyhow::{Context, Result};
use egui::Color32;
use glam::{UVec3, Vec3};

mod local_light_scaling;
use local_light_scaling::{LocalLightScalingSample, LocalLightScalingState};

const BUILD_DELAY_SECONDS: f32 = 0.5;
const SETTLE_FRAMES: u8 = 2;
const TEST_TIME_OF_DAY: f32 = 0.455_705;
const TEST_LATITUDE: f32 = -0.24;
const TEST_SEASON: f32 = 0.25;
const PATT_SEAM_TIME_OF_DAY: f32 = 0.49;
const PATT_SEAM_LATITUDE: f32 = -0.07;
const PATT_SEAM_SEASON: f32 = 0.29;
const TEST_VOXEL_COLOR_VARIANCE: f32 = 0.0;
const VOXELS_PER_WORLD_UNIT: f32 = 256.0;
const RADIANCE_R1_SUN_COLOR: Color32 = Color32::from_rgb(255, 241, 224);
const RADIANCE_R1_SUN_LUMINANCE: f32 = 1.65;
const RADIANCE_R2_TIME_OF_DAY: f32 = 0.465_705;
const RADIANCE_R2_SUN_COLOR: Color32 = Color32::from_rgb(255, 180, 128);
const RADIANCE_R2_SUN_LUMINANCE: f32 = 3.3;
const RADIANCE_R3_TIME_OF_DAY: f32 = 0.475_705;
const RADIANCE_R3_SUN_COLOR: Color32 = Color32::from_rgb(180, 220, 255);
const RADIANCE_R3_SUN_LUMINANCE: f32 = 2.2;
const RADIANCE_R4_TIME_OF_DAY: f32 = 0.485_705;
const RADIANCE_R4_SUN_COLOR: Color32 = Color32::from_rgb(128, 180, 255);
const RADIANCE_R4_SUN_LUMINANCE: f32 = 0.8;
const RADIANCE_R2_ROCK_COLOR: Color32 = Color32::from_rgb(126, 125, 128);
const RADIANCE_R3_ROCK_COLOR: Color32 = Color32::from_rgb(130, 125, 128);
const RADIANCE_R4_ROCK_COLOR: Color32 = Color32::from_rgb(134, 125, 128);
const POINT_LIGHT_ADD_POSITION: Vec3 = Vec3::new(0.66, 0.68, 1.18);
const POINT_LIGHT_MOVED_POSITION: Vec3 = Vec3::new(0.82, 0.62, 1.06);
const POINT_LIGHT_RANGE_WORLD: f32 = 0.55;
const POINT_LIGHT_SOURCE_RADIUS_WORLD: f32 = 0.03;
const POINT_LIGHT_FIXED_RECEIVER_WORLD: Vec3 = Vec3::new(0.66, 101.0 / VOXELS_PER_WORLD_UNIT, 1.18);
const POINT_LIGHT_FIXED_RECEIVER_NORMAL: Vec3 = Vec3::Y;
const POINT_LIGHT_FIXED_RAY_ORIGIN_OFFSET_WORLD: f32 = 0.001;
const POINT_LIGHT_BLOCKER_MIN: Vec3 = Vec3::new(164.0, 128.0, 298.0);
const POINT_LIGHT_BLOCKER_MAX: Vec3 = Vec3::new(174.0, 152.0, 307.0);
const POINT_LIGHT_EMISSIVE_MIN: Vec3 = Vec3::new(196.0, 100.0, 344.0);
const POINT_LIGHT_EMISSIVE_MAX: Vec3 = Vec3::new(212.0, 116.0, 360.0);
const VOXEL_EMISSIVE_PRIMARY_MIN: UVec3 = UVec3::new(164, 170, 296);
const VOXEL_EMISSIVE_PRIMARY_MAX: UVec3 = UVec3::new(170, 176, 302);
const VOXEL_EMISSIVE_SECONDARY_MIN: UVec3 = UVec3::new(170, 170, 296);
const VOXEL_EMISSIVE_SECONDARY_MAX: UVec3 = UVec3::new(176, 176, 302);
const VOXEL_EMISSIVE_MOVED_MIN: UVec3 = UVec3::new(176, 170, 296);
const VOXEL_EMISSIVE_MOVED_MAX: UVec3 = UVec3::new(188, 176, 302);
const RASTER_EMITTER_ADD_BASE_POSITION: Vec3 = Vec3::new(
    POINT_LIGHT_ADD_POSITION.x,
    POINT_LIGHT_ADD_POSITION.y - 4.0 / VOXELS_PER_WORLD_UNIT,
    POINT_LIGHT_ADD_POSITION.z,
);
const RASTER_EMITTER_MOVED_BASE_POSITION: Vec3 = Vec3::new(
    POINT_LIGHT_MOVED_POSITION.x,
    POINT_LIGHT_MOVED_POSITION.y - 4.0 / VOXELS_PER_WORLD_UNIT,
    POINT_LIGHT_MOVED_POSITION.z,
);
const MULTI_SOURCE_AUTHORED_COLOR: Vec3 = Vec3::new(1.0, 0.12, 0.04);
const MULTI_SOURCE_AUTHORED_INTENSITY: f32 = 0.045;
const MULTI_SOURCE_RASTER_COLOR: Vec3 = Vec3::new(0.04, 0.22, 1.0);
const MULTI_SOURCE_RASTER_INTENSITY: f32 = 0.065;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
enum PointLightTestStage {
    AwaitBaseline,
    AwaitAddLive,
    AwaitVisibleDiagnostic,
    AwaitBlockerSettled,
    AwaitBlockedDiagnostic,
    AwaitRestoreSettled,
    AwaitRestoredDiagnostic,
    AwaitOverflowDiagnostic,
    AwaitDiagnosticCleanupLive,
    AwaitRemovedDiagnostic,
    AwaitPointOnDdgiPublication,
    AwaitMoveLive,
    AwaitMoveMidflight,
    AwaitPhotometricUpdateLive,
    AwaitRemoveLive,
    AwaitFinalPublication,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
enum VoxelEmissiveTestStage {
    AwaitBaseline,
    AwaitAddRegistry,
    AwaitAddLive,
    AwaitVisibleDiagnostic,
    AwaitBlockerSettled,
    AwaitBlockedDiagnostic,
    AwaitRestoreSettled,
    AwaitRestoredDiagnostic,
    AwaitAggregateRegistry,
    AwaitAggregateLive,
    AwaitAggregateDiagnostic,
    AwaitMoveRegistry,
    AwaitMoveLive,
    AwaitMovedDdgiPublication,
    AwaitRemoveRegistry,
    AwaitRemoveLive,
    AwaitRemovedDiagnostic,
    AwaitFinalPublication,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct VoxelEmissiveLifecycleState {
    terrain_revision: u32,
    baseline: Option<DdgiFieldIdentity>,
    light_id: Option<LightId>,
    stage: VoxelEmissiveTestStage,
    expected_source_revision: u64,
    expected_registry_revision: u64,
    mutation_frame: u64,
    visible_luma_q8: u32,
    primary_intensity_bits: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
enum RasterEmitterTestStage {
    AwaitBaseline,
    AwaitSpawnLive,
    AwaitVisibleDiagnostic,
    AwaitDdgiPublication,
    AwaitNoopStable,
    AwaitMoveLive,
    AwaitMoveMidflight,
    AwaitPhotometricLive,
    AwaitRemoveLive,
    AwaitRemovedDiagnostic,
    AwaitFinalPublication,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RasterEmitterLifecycleState {
    terrain_revision: u32,
    baseline: Option<DdgiFieldIdentity>,
    entity: Option<RasterEntityId>,
    light_id: Option<LightId>,
    in_flight: Option<DdgiFieldIdentity>,
    in_flight_source_revision: u64,
    stage: RasterEmitterTestStage,
    expected_source_revision: u64,
    expected_registry_revision: u64,
    expected_provider_revision: u64,
    expected_sprinkler_revision: u64,
    mutation_frame: u64,
    visible_luma_q8: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
enum MultiSourceTestStage {
    AwaitBaseline,
    AwaitVoxelRegistry,
    AwaitThreeLive,
    AwaitAuthoredDiagnostic,
    AwaitVoxelDiagnostic,
    AwaitRasterDiagnostic,
    AwaitAggregateDiagnostic,
    AwaitThreeDdgiPublication,
    AwaitSwapLive,
    AwaitSwappedAggregateDiagnostic,
    AwaitSwappedAuthoredDiagnostic,
    AwaitAuthoredRemoveLive,
    AwaitAfterRemoveAggregateDiagnostic,
    AwaitRemovedAuthoredDiagnostic,
    AwaitVoxelMoveRegistry,
    AwaitVoxelMoveLive,
    AwaitMovedVoxelStaleDiagnostic,
    AwaitOverflowLive,
    AwaitFinalRegistry,
    AwaitFinalLive,
    AwaitFinalStaleDiagnostic,
    AwaitFinalPublication,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct MultiSourceLifecycleState {
    terrain_revision: u32,
    baseline: Option<DdgiFieldIdentity>,
    authored_id: Option<LightId>,
    voxel_id: Option<LightId>,
    stale_voxel_id: Option<LightId>,
    raster_id: Option<LightId>,
    raster_entity: Option<RasterEntityId>,
    stage: MultiSourceTestStage,
    expected_source_revision: u64,
    expected_registry_revision: u64,
    mutation_frame: u64,
    authored_irradiance: Vec3,
    voxel_irradiance: Vec3,
    raster_irradiance: Vec3,
    aggregate_irradiance: Vec3,
    swapped_authored_irradiance: Vec3,
}

pub(super) const STARTUP_TREE_POSITION: Vec3 = Vec3::new(1.72, 0.2, 0.62);

const SHELL_MIN: Vec3 = Vec3::new(96.0, 84.0, 216.0);
const SHELL_MAX: Vec3 = Vec3::new(238.0, 236.0, 392.0);
const INTERIOR_MIN: Vec3 = Vec3::new(112.0, 100.0, 242.0);
const INTERIOR_MAX: Vec3 = Vec3::new(222.0, 216.0, 376.0);
const SKYLIGHT_MIN: Vec3 = Vec3::new(144.0, 216.0, 270.0);
const SKYLIGHT_MAX: Vec3 = Vec3::new(192.0, 244.0, 334.0);

const PATT_SEAM_DIG_CENTERS: [Vec3; 2] = [
    Vec3::new(0.58, 226.0 / 256.0, 1.10),
    Vec3::new(0.52, 226.0 / 256.0, 1.20),
];
const PATT_SEAM_DIG_PASSES: usize = 24;

const WALLS_FLOOR_MIN: Vec3 = Vec3::new(80.0, 84.0, 208.0);
const WALLS_FLOOR_MAX: Vec3 = Vec3::new(432.0, 100.0, 408.0);
const WALLS_BACK_MIN: Vec3 = Vec3::new(80.0, 100.0, 208.0);
const WALLS_BACK_MAX: Vec3 = Vec3::new(432.0, 236.0, 224.0);
const ONE_VOXEL_WALL_MIN: Vec3 = Vec3::new(96.0, 100.0, 300.0);
const ONE_VOXEL_WALL_MAX: Vec3 = Vec3::new(192.0, 196.0, 301.0);
const TWO_VOXEL_WALL_MIN: Vec3 = Vec3::new(208.0, 100.0, 300.0);
const TWO_VOXEL_WALL_MAX: Vec3 = Vec3::new(304.0, 196.0, 302.0);

const DONOR_CLEAR_MIN: Vec3 = Vec3::new(72.0, 100.0, 200.0);
const DONOR_CLEAR_MAX: Vec3 = Vec3::new(440.0, 244.0, 416.0);
const DONOR_FLOOR_MIN: Vec3 = Vec3::new(80.0, 84.0, 208.0);
const DONOR_FLOOR_MAX: Vec3 = Vec3::new(432.0, 100.0, 408.0);
const DONOR_BACK_MIN: Vec3 = Vec3::new(80.0, 100.0, 208.0);
const DONOR_BACK_MAX: Vec3 = Vec3::new(432.0, 164.0, 240.0);
const DONOR_LEFT_ROOF_MIN: Vec3 = Vec3::new(80.0, 164.0, 208.0);
const DONOR_LEFT_ROOF_MAX: Vec3 = Vec3::new(224.0, 228.0, 320.0);
const DONOR_RIGHT_ROOF_MIN: Vec3 = Vec3::new(288.0, 164.0, 208.0);
const DONOR_RIGHT_ROOF_MAX: Vec3 = Vec3::new(432.0, 228.0, 320.0);
const DONOR_DIVIDER_MIN: Vec3 = Vec3::new(224.0, 100.0, 208.0);
const DONOR_DIVIDER_MAX: Vec3 = Vec3::new(288.0, 228.0, 408.0);
const DONOR_SLAB_MIN: Vec3 = Vec3::new(104.0, 100.0, 336.0);
const DONOR_SLAB_MAX: Vec3 = Vec3::new(208.0, 116.0, 392.0);
const DONOR_CONTROL_SLAB_MIN: Vec3 = Vec3::new(304.0, 100.0, 336.0);
const DONOR_CONTROL_SLAB_MAX: Vec3 = Vec3::new(408.0, 116.0, 392.0);

const DONOR_RECEIVER_ROI_MIN: Vec3 = Vec3::new(136.0, 112.0, 240.0);
const DONOR_RECEIVER_ROI_MAX: Vec3 = Vec3::new(208.0, 152.0, 240.0);
const DONOR_CONTROL_RECEIVER_ROI_MIN: Vec3 = Vec3::new(336.0, 112.0, 240.0);
const DONOR_CONTROL_RECEIVER_ROI_MAX: Vec3 = Vec3::new(408.0, 152.0, 240.0);
const DONOR_SURFACE_ROI_MIN: Vec3 = Vec3::new(112.0, 116.0, 344.0);
const DONOR_SURFACE_ROI_MAX: Vec3 = Vec3::new(200.0, 116.0, 384.0);

// A solid block is carved into a low, roofed L-shaped light trap. The first reflector sits in
// the open light well, the second is the east wall of the first tunnel leg, and the final receiver
// is the north wall after the turn. Straight paths first->second and second->receiver are open;
// first->receiver crosses uncarved rock at the inner corner.
const DOGLEG_CLEAR_MIN: Vec3 = Vec3::new(48.0, 72.0, 96.0);
const DOGLEG_CLEAR_MAX: Vec3 = Vec3::new(464.0, 256.0, 448.0);
const DOGLEG_BLOCK_MIN: Vec3 = Vec3::new(64.0, 80.0, 96.0);
const DOGLEG_BLOCK_MAX: Vec3 = Vec3::new(448.0, 240.0, 432.0);
const DOGLEG_LIGHT_WELL_MIN: Vec3 = Vec3::new(80.0, 96.0, 288.0);
const DOGLEG_LIGHT_WELL_MAX: Vec3 = Vec3::new(192.0, 240.0, 416.0);
const DOGLEG_FIRST_LEG_MIN: Vec3 = Vec3::new(192.0, 96.0, 288.0);
const DOGLEG_FIRST_LEG_MAX: Vec3 = Vec3::new(352.0, 176.0, 368.0);
const DOGLEG_SECOND_LEG_MIN: Vec3 = Vec3::new(272.0, 96.0, 144.0);
const DOGLEG_SECOND_LEG_MAX: Vec3 = Vec3::new(352.0, 176.0, 288.0);
const DOGLEG_RECEIVER_CHAMBER_MIN: Vec3 = Vec3::new(256.0, 96.0, 128.0);
const DOGLEG_RECEIVER_CHAMBER_MAX: Vec3 = Vec3::new(400.0, 176.0, 192.0);
const DOGLEG_FIRST_REFLECTOR_MIN: Vec3 = Vec3::new(112.0, 144.0, 320.0);
const DOGLEG_FIRST_REFLECTOR_MAX: Vec3 = Vec3::new(192.0, 160.0, 384.0);

const DOGLEG_FIRST_REFLECTOR_ROI_MIN: Vec3 = Vec3::new(144.0, 160.0, 336.0);
const DOGLEG_FIRST_REFLECTOR_ROI_MAX: Vec3 = Vec3::new(176.0, 160.0, 368.0);
const DOGLEG_SECOND_REFLECTOR_ROI_MIN: Vec3 = Vec3::new(352.0, 112.0, 304.0);
const DOGLEG_SECOND_REFLECTOR_ROI_MAX: Vec3 = Vec3::new(352.0, 160.0, 352.0);
const DOGLEG_FINAL_RECEIVER_ROI_MIN: Vec3 = Vec3::new(288.0, 112.0, 128.0);
const DOGLEG_FINAL_RECEIVER_ROI_MAX: Vec3 = Vec3::new(336.0, 160.0, 128.0);

const TEST_REBUILD_MIN: UVec3 = UVec3::new(72, 76, 200);
const TEST_REBUILD_MAX: UVec3 = UVec3::new(440, 244, 416);

#[derive(Clone, Copy, Debug, PartialEq)]
enum TestScenePhase {
    Pending,
    TerrainPublished,
    Settling {
        frames: u8,
        terrain_revision: u32,
    },
    WaitingForProbeField {
        terrain_revision: u32,
    },
    PattSeamTerrainPublished {
        target_revision: u32,
    },
    WaitingForPattSeamProbeField {
        target_revision: u32,
    },
    WaitingForRadianceBaseline {
        terrain_revision: u32,
    },
    PointLightLifecycle {
        terrain_revision: u32,
        baseline: Option<DdgiFieldIdentity>,
        light_id: Option<LightId>,
        in_flight: Option<DdgiFieldIdentity>,
        in_flight_source_revision: u64,
        stage: PointLightTestStage,
        expected_source_revision: u64,
        mutation_frame: u64,
    },
    VoxelEmissiveLifecycle(VoxelEmissiveLifecycleState),
    RasterEmitterLifecycle(RasterEmitterLifecycleState),
    MultiSourceLifecycle(MultiSourceLifecycleState),
    LocalLightScaling(LocalLightScalingState),
    CapturingRadianceBaseline {
        r1: DdgiFieldIdentity,
    },
    MutatingRadianceR2 {
        r1: DdgiFieldIdentity,
    },
    CapturingRadianceR2NextFrame {
        r1: DdgiFieldIdentity,
        mutation_frame: u64,
    },
    WaitingForRadianceR2Midflight {
        r1: DdgiFieldIdentity,
    },
    WaitingForRadianceR3Observed {
        r1: DdgiFieldIdentity,
        r2: DdgiFieldIdentity,
    },
    MutatingRadianceR4 {
        r1: DdgiFieldIdentity,
        r2: DdgiFieldIdentity,
    },
    CapturingRadianceR4NextFrame {
        r1: DdgiFieldIdentity,
        r2: DdgiFieldIdentity,
        mutation_frame: u64,
    },
    WaitingForRadianceR4Midflight {
        r1: DdgiFieldIdentity,
        r2: DdgiFieldIdentity,
    },
    WaitingForRadianceR4Published {
        r1: DdgiFieldIdentity,
        r2: DdgiFieldIdentity,
        r4: DdgiFieldIdentity,
    },
    CapturingRadianceR4Published {
        r1: DdgiFieldIdentity,
        r2: DdgiFieldIdentity,
        r4: DdgiFieldIdentity,
    },
    WaitingForDensityMidflight {
        baseline: DdgiFieldIdentity,
    },
    WaitingForDensityGeometryReplacement {
        baseline: DdgiFieldIdentity,
        obsolete_density_token_serial: u64,
        obsolete_density_field: DdgiFieldIdentity,
        target_revision: u32,
    },
    WaitingForDensityGeometryPublished {
        baseline: DdgiFieldIdentity,
        obsolete_density_token_serial: u64,
        obsolete_density_field: DdgiFieldIdentity,
        terrain_token_serial: u64,
        target_revision: u32,
    },
    WaitingForDensityRetryMidflight {
        geometry_field: DdgiFieldIdentity,
        obsolete_density_token_serial: u64,
        terrain_token_serial: u64,
    },
    WaitingForDensityFinalPublished {
        geometry_field: DdgiFieldIdentity,
        obsolete_density_token_serial: u64,
        terrain_token_serial: u64,
        density_token_serial: u64,
        density_field: DdgiFieldIdentity,
    },
    TerrainEditPublished {
        edit: TerrainEdit,
        target_revision: u32,
    },
    WaitingForEditedProbeField {
        edit: TerrainEdit,
        target_revision: u32,
    },
    WaitingForDensityRebuild {
        terrain_revision: u32,
    },
    CapturingInflightStaleActive {
        target_revision: u32,
    },
    Ready,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RadianceCaptureCheckpoint {
    Baseline,
    R2NextFrame,
    R4NextFrame,
    Final,
}

impl RadianceCaptureCheckpoint {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::R2NextFrame => "r2-next-frame",
            Self::R4NextFrame => "r4-next-frame",
            Self::Final => "final",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RadianceCaptureRequest {
    pub checkpoint: RadianceCaptureCheckpoint,
    pub mutation_frame: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerrainEdit {
    CloseSkylight,
    ReopenSkylight,
}

impl TerrainEdit {
    fn label(self) -> &'static str {
        match self {
            Self::CloseSkylight => "close-skylight",
            Self::ReopenSkylight => "reopen-skylight",
        }
    }

    fn voxel_type(self) -> u32 {
        match self {
            Self::CloseSkylight => VOXEL_TYPE_ROCK,
            Self::ReopenSkylight => VOXEL_TYPE_EMPTY,
        }
    }
}

fn next_nonzero_revision(revision: u32) -> u32 {
    revision.wrapping_add(1).max(1)
}

fn is_converged_field(field: DdgiFieldIdentity) -> bool {
    field.field().state() == DdgiFieldState::Converged
}

fn assert_radiance_epoch_zero(
    field: DdgiFieldIdentity,
    source: DdgiFieldIdentity,
    radiance_revision: u32,
) {
    let key = field.field();
    let source_key = source.field();
    assert_eq!(key.state(), DdgiFieldState::Converging);
    assert_eq!(key.update_epoch(), 0);
    assert_eq!(key.geometry_revision(), source_key.geometry_revision());
    assert_eq!(key.spacing_voxels(), source_key.spacing_voxels());
    assert_eq!(key.radiance_revision(), radiance_revision);
    assert_eq!(field.source(), Some(source_key));
}

fn assert_initial_epoch_zero(
    field: DdgiFieldIdentity,
    geometry_revision: u32,
    radiance_revision: u32,
    spacing_voxels: u32,
) {
    let key = field.field();
    assert_eq!(key.geometry_revision(), geometry_revision);
    assert_eq!(key.radiance_revision(), radiance_revision);
    assert_eq!(key.spacing_voxels(), spacing_voxels);
    assert_eq!(key.state(), DdgiFieldState::Converging);
    assert_eq!(key.update_epoch(), 0);
    assert_eq!(field.source(), None);
}

fn assert_geometry_epoch_zero(
    field: DdgiFieldIdentity,
    source: DdgiFieldIdentity,
    geometry_revision: u32,
) {
    let key = field.field();
    let source_key = source.field();
    assert_eq!(key.geometry_revision(), geometry_revision);
    assert_eq!(key.radiance_revision(), source_key.radiance_revision());
    assert_eq!(key.spacing_voxels(), source_key.spacing_voxels());
    assert_eq!(key.state(), DdgiFieldState::Converging);
    assert_eq!(key.update_epoch(), 0);
    assert_eq!(field.source(), Some(source_key));
}

fn log_acceptance_field(group: &str, checkpoint: &str, field: DdgiFieldIdentity) {
    let key = field.field();
    let source = field.source();
    log::info!(
        "[DDGI_ACCEPT][{}] checkpoint={} field_serial={} geometry_revision={} radiance_revision={} spacing_voxels={} state={:?} update_epoch={} source_field_serial={} source_radiance_revision={} source_state={} source_update_epoch={}",
        group,
        checkpoint,
        key.serial(),
        key.geometry_revision(),
        key.radiance_revision(),
        key.spacing_voxels(),
        key.state(),
        key.update_epoch(),
        source.map_or(0, |source| source.serial()),
        source.map_or(0, |source| source.radiance_revision()),
        source.map_or_else(|| "none".to_owned(), |source| format!("{:?}", source.state())),
        source.map_or(0, |source| source.update_epoch()),
    );
}

#[derive(Debug)]
pub(super) struct EnvironmentLightingTestScene {
    case: EnvironmentLightingTestCase,
    phase: TestScenePhase,
    point_light_fixed_gpu_request_serial: u32,
    point_light_fixed_gpu_visible_luma_q8: u32,
    point_light_diagnostic_selected_decoy_id: Option<LightId>,
    point_light_diagnostic_overflow_id: Option<LightId>,
    point_light_diagnostic_supplemental_ids: Vec<LightId>,
    point_light_expected_registry_revision: u64,
    multi_source_overflow_authored_ids: Vec<LightId>,
    local_light_scaling_ids: Vec<LightId>,
    local_light_scaling_samples: Vec<LocalLightScalingSample>,
}

impl EnvironmentLightingTestScene {
    pub(super) fn new(case: EnvironmentLightingTestCase) -> Self {
        Self {
            case,
            phase: TestScenePhase::Pending,
            point_light_fixed_gpu_request_serial: 0,
            point_light_fixed_gpu_visible_luma_q8: 0,
            point_light_diagnostic_selected_decoy_id: None,
            point_light_diagnostic_overflow_id: None,
            point_light_diagnostic_supplemental_ids: Vec::new(),
            point_light_expected_registry_revision: 0,
            multi_source_overflow_authored_ids: Vec::new(),
            local_light_scaling_ids: Vec::new(),
            local_light_scaling_samples: Vec::new(),
        }
    }

    pub(super) fn is_ready(&self) -> bool {
        self.phase == TestScenePhase::Ready
    }

    pub(super) fn hides_terrain_edit_preview(&self) -> bool {
        self.case == EnvironmentLightingTestCase::PattSeam
    }

    pub(super) fn is_capture_ready(&self) -> bool {
        self.is_ready()
            || matches!(
                self.phase,
                TestScenePhase::CapturingInflightStaleActive { .. }
                    | TestScenePhase::CapturingRadianceBaseline { .. }
                    | TestScenePhase::CapturingRadianceR2NextFrame { .. }
                    | TestScenePhase::CapturingRadianceR4NextFrame { .. }
                    | TestScenePhase::CapturingRadianceR4Published { .. }
            )
    }

    pub(super) fn radiance_capture_request(&self) -> Option<RadianceCaptureRequest> {
        let (checkpoint, mutation_frame) = match self.phase {
            TestScenePhase::CapturingRadianceBaseline { .. } => {
                (RadianceCaptureCheckpoint::Baseline, None)
            }
            TestScenePhase::CapturingRadianceR2NextFrame { mutation_frame, .. } => {
                (RadianceCaptureCheckpoint::R2NextFrame, Some(mutation_frame))
            }
            TestScenePhase::CapturingRadianceR4NextFrame { mutation_frame, .. } => {
                (RadianceCaptureCheckpoint::R4NextFrame, Some(mutation_frame))
            }
            TestScenePhase::CapturingRadianceR4Published { .. } => {
                (RadianceCaptureCheckpoint::Final, None)
            }
            _ => return None,
        };
        Some(RadianceCaptureRequest {
            checkpoint,
            mutation_frame,
        })
    }

    pub(super) fn complete_radiance_capture(
        &mut self,
        checkpoint: RadianceCaptureCheckpoint,
    ) -> bool {
        self.phase = match (self.phase, checkpoint) {
            (
                TestScenePhase::CapturingRadianceBaseline { r1 },
                RadianceCaptureCheckpoint::Baseline,
            ) => TestScenePhase::MutatingRadianceR2 { r1 },
            (
                TestScenePhase::CapturingRadianceR2NextFrame { r1, .. },
                RadianceCaptureCheckpoint::R2NextFrame,
            ) => TestScenePhase::WaitingForRadianceR2Midflight { r1 },
            (
                TestScenePhase::CapturingRadianceR4NextFrame { r1, r2, .. },
                RadianceCaptureCheckpoint::R4NextFrame,
            ) => TestScenePhase::WaitingForRadianceR4Midflight { r1, r2 },
            (
                TestScenePhase::CapturingRadianceR4Published { .. },
                RadianceCaptureCheckpoint::Final,
            ) => TestScenePhase::Ready,
            (phase, actual) => {
                panic!("radiance capture completion mismatch phase={phase:?} checkpoint={actual:?}")
            }
        };
        self.is_ready()
    }

    pub(super) fn inflight_capture_target_revision(&self) -> Option<u32> {
        match self.phase {
            TestScenePhase::CapturingInflightStaleActive { target_revision } => {
                Some(target_revision)
            }
            _ => None,
        }
    }

    pub(super) fn edit_cycle_target_revision(&self) -> Option<u32> {
        if !(is_terrain_edit_case(self.case)
            || self.case == EnvironmentLightingTestCase::RadianceChanges
            || self.case == EnvironmentLightingTestCase::PointLightChanges
            || self.case == EnvironmentLightingTestCase::VoxelEmissiveChanges
            || self.case == EnvironmentLightingTestCase::RasterEmitterChanges
            || self.case == EnvironmentLightingTestCase::MultiSourceStress
            || self.case == EnvironmentLightingTestCase::LocalLightScaling
            || self.case == EnvironmentLightingTestCase::PattSeam)
            || self.is_ready()
        {
            return None;
        }
        match self.phase {
            TestScenePhase::TerrainEditPublished {
                target_revision, ..
            }
            | TestScenePhase::WaitingForEditedProbeField {
                target_revision, ..
            }
            | TestScenePhase::PattSeamTerrainPublished { target_revision }
            | TestScenePhase::WaitingForPattSeamProbeField { target_revision }
            | TestScenePhase::CapturingInflightStaleActive { target_revision } => {
                Some(target_revision)
            }
            TestScenePhase::WaitingForDensityRebuild { terrain_revision } => Some(terrain_revision),
            TestScenePhase::WaitingForRadianceBaseline { terrain_revision } => {
                Some(terrain_revision)
            }
            TestScenePhase::PointLightLifecycle {
                terrain_revision, ..
            } => Some(terrain_revision),
            TestScenePhase::VoxelEmissiveLifecycle(state) => Some(state.terrain_revision),
            TestScenePhase::RasterEmitterLifecycle(state) => Some(state.terrain_revision),
            TestScenePhase::MultiSourceLifecycle(state) => Some(state.terrain_revision),
            TestScenePhase::LocalLightScaling(state) => Some(state.terrain_revision),
            TestScenePhase::CapturingRadianceBaseline { r1 }
            | TestScenePhase::MutatingRadianceR2 { r1 }
            | TestScenePhase::CapturingRadianceR2NextFrame { r1, .. }
            | TestScenePhase::WaitingForRadianceR2Midflight { r1 }
            | TestScenePhase::WaitingForRadianceR3Observed { r1, .. }
            | TestScenePhase::MutatingRadianceR4 { r1, .. }
            | TestScenePhase::CapturingRadianceR4NextFrame { r1, .. }
            | TestScenePhase::WaitingForRadianceR4Midflight { r1, .. }
            | TestScenePhase::WaitingForRadianceR4Published { r1, .. }
            | TestScenePhase::CapturingRadianceR4Published { r1, .. } => {
                Some(r1.field().geometry_revision())
            }
            TestScenePhase::WaitingForDensityMidflight { baseline } => {
                Some(baseline.field().geometry_revision())
            }
            TestScenePhase::WaitingForDensityGeometryReplacement {
                target_revision, ..
            }
            | TestScenePhase::WaitingForDensityGeometryPublished {
                target_revision, ..
            } => Some(target_revision),
            TestScenePhase::WaitingForDensityRetryMidflight { geometry_field, .. }
            | TestScenePhase::WaitingForDensityFinalPublished { geometry_field, .. } => {
                Some(geometry_field.field().geometry_revision())
            }
            _ => Some(0),
        }
    }

    pub(super) fn phase_label(&self) -> &'static str {
        match self.phase {
            TestScenePhase::Pending => "pending",
            TestScenePhase::TerrainPublished => "terrain-published",
            TestScenePhase::Settling { .. } => "settling-initial-terrain",
            TestScenePhase::WaitingForProbeField { .. } => "waiting-for-initial-probe-field",
            TestScenePhase::PattSeamTerrainPublished { .. } => "patt-seam-terrain-published",
            TestScenePhase::WaitingForPattSeamProbeField { .. } => {
                "waiting-for-patt-seam-probe-field"
            }
            TestScenePhase::WaitingForRadianceBaseline { .. } => "waiting-for-radiance-baseline",
            TestScenePhase::PointLightLifecycle { stage, .. } => match stage {
                PointLightTestStage::AwaitBaseline => "waiting-for-point-light-baseline",
                PointLightTestStage::AwaitAddLive => "waiting-for-point-light-add-live",
                PointLightTestStage::AwaitVisibleDiagnostic => {
                    "waiting-for-point-light-visible-diagnostic"
                }
                PointLightTestStage::AwaitBlockerSettled => {
                    "waiting-for-point-light-blocker-settle"
                }
                PointLightTestStage::AwaitBlockedDiagnostic => {
                    "waiting-for-point-light-blocked-diagnostic"
                }
                PointLightTestStage::AwaitRestoreSettled => {
                    "waiting-for-point-light-restore-settle"
                }
                PointLightTestStage::AwaitRestoredDiagnostic => {
                    "waiting-for-point-light-restored-diagnostic"
                }
                PointLightTestStage::AwaitOverflowDiagnostic => {
                    "waiting-for-point-light-overflow-diagnostic"
                }
                PointLightTestStage::AwaitDiagnosticCleanupLive => {
                    "waiting-for-point-light-diagnostic-cleanup-live"
                }
                PointLightTestStage::AwaitRemovedDiagnostic => {
                    "waiting-for-point-light-removed-diagnostic"
                }
                PointLightTestStage::AwaitPointOnDdgiPublication => {
                    "waiting-for-point-light-on-ddgi-publication"
                }
                PointLightTestStage::AwaitMoveLive => "waiting-for-point-light-move-live",
                PointLightTestStage::AwaitMoveMidflight => "waiting-for-point-light-move-midflight",
                PointLightTestStage::AwaitPhotometricUpdateLive => {
                    "waiting-for-point-light-photometric-update-live"
                }
                PointLightTestStage::AwaitRemoveLive => "waiting-for-point-light-remove-live",
                PointLightTestStage::AwaitFinalPublication => {
                    "waiting-for-point-light-final-publication"
                }
            },
            TestScenePhase::VoxelEmissiveLifecycle(state) => match state.stage {
                VoxelEmissiveTestStage::AwaitBaseline => "waiting-for-voxel-emissive-baseline",
                VoxelEmissiveTestStage::AwaitAddRegistry => {
                    "waiting-for-voxel-emissive-add-registry"
                }
                VoxelEmissiveTestStage::AwaitAddLive => "waiting-for-voxel-emissive-add-live",
                VoxelEmissiveTestStage::AwaitVisibleDiagnostic => {
                    "waiting-for-voxel-emissive-visible-diagnostic"
                }
                VoxelEmissiveTestStage::AwaitBlockerSettled => {
                    "waiting-for-voxel-emissive-blocker-settle"
                }
                VoxelEmissiveTestStage::AwaitBlockedDiagnostic => {
                    "waiting-for-voxel-emissive-blocked-diagnostic"
                }
                VoxelEmissiveTestStage::AwaitRestoreSettled => {
                    "waiting-for-voxel-emissive-restore-settle"
                }
                VoxelEmissiveTestStage::AwaitRestoredDiagnostic => {
                    "waiting-for-voxel-emissive-restored-diagnostic"
                }
                VoxelEmissiveTestStage::AwaitAggregateRegistry => {
                    "waiting-for-voxel-emissive-aggregate-registry"
                }
                VoxelEmissiveTestStage::AwaitAggregateLive => {
                    "waiting-for-voxel-emissive-aggregate-live"
                }
                VoxelEmissiveTestStage::AwaitAggregateDiagnostic => {
                    "waiting-for-voxel-emissive-aggregate-diagnostic"
                }
                VoxelEmissiveTestStage::AwaitMoveRegistry => {
                    "waiting-for-voxel-emissive-move-registry"
                }
                VoxelEmissiveTestStage::AwaitMoveLive => "waiting-for-voxel-emissive-move-live",
                VoxelEmissiveTestStage::AwaitMovedDdgiPublication => {
                    "waiting-for-voxel-emissive-ddgi-publication"
                }
                VoxelEmissiveTestStage::AwaitRemoveRegistry => {
                    "waiting-for-voxel-emissive-remove-registry"
                }
                VoxelEmissiveTestStage::AwaitRemoveLive => "waiting-for-voxel-emissive-remove-live",
                VoxelEmissiveTestStage::AwaitRemovedDiagnostic => {
                    "waiting-for-voxel-emissive-removed-diagnostic"
                }
                VoxelEmissiveTestStage::AwaitFinalPublication => {
                    "waiting-for-voxel-emissive-final-publication"
                }
            },
            TestScenePhase::RasterEmitterLifecycle(state) => match state.stage {
                RasterEmitterTestStage::AwaitBaseline => "waiting-for-raster-emitter-baseline",
                RasterEmitterTestStage::AwaitSpawnLive => "waiting-for-raster-emitter-spawn-live",
                RasterEmitterTestStage::AwaitVisibleDiagnostic => {
                    "waiting-for-raster-emitter-visible-diagnostic"
                }
                RasterEmitterTestStage::AwaitDdgiPublication => {
                    "waiting-for-raster-emitter-ddgi-publication"
                }
                RasterEmitterTestStage::AwaitNoopStable => "waiting-for-raster-emitter-noop-stable",
                RasterEmitterTestStage::AwaitMoveLive => "waiting-for-raster-emitter-move-live",
                RasterEmitterTestStage::AwaitMoveMidflight => {
                    "waiting-for-raster-emitter-move-midflight"
                }
                RasterEmitterTestStage::AwaitPhotometricLive => {
                    "waiting-for-raster-emitter-photometric-live"
                }
                RasterEmitterTestStage::AwaitRemoveLive => "waiting-for-raster-emitter-remove-live",
                RasterEmitterTestStage::AwaitRemovedDiagnostic => {
                    "waiting-for-raster-emitter-removed-diagnostic"
                }
                RasterEmitterTestStage::AwaitFinalPublication => {
                    "waiting-for-raster-emitter-final-publication"
                }
            },
            TestScenePhase::MultiSourceLifecycle(state) => match state.stage {
                MultiSourceTestStage::AwaitBaseline => "waiting-for-multi-source-baseline",
                MultiSourceTestStage::AwaitVoxelRegistry => {
                    "waiting-for-multi-source-voxel-registry"
                }
                MultiSourceTestStage::AwaitThreeLive => "waiting-for-multi-source-three-live",
                MultiSourceTestStage::AwaitAuthoredDiagnostic => {
                    "waiting-for-multi-source-authored-diagnostic"
                }
                MultiSourceTestStage::AwaitVoxelDiagnostic => {
                    "waiting-for-multi-source-voxel-diagnostic"
                }
                MultiSourceTestStage::AwaitRasterDiagnostic => {
                    "waiting-for-multi-source-raster-diagnostic"
                }
                MultiSourceTestStage::AwaitAggregateDiagnostic => {
                    "waiting-for-multi-source-aggregate-diagnostic"
                }
                MultiSourceTestStage::AwaitThreeDdgiPublication => {
                    "waiting-for-multi-source-ddgi-publication"
                }
                MultiSourceTestStage::AwaitSwapLive => "waiting-for-multi-source-swap-live",
                MultiSourceTestStage::AwaitSwappedAggregateDiagnostic => {
                    "waiting-for-multi-source-swapped-aggregate-diagnostic"
                }
                MultiSourceTestStage::AwaitSwappedAuthoredDiagnostic => {
                    "waiting-for-multi-source-swapped-authored-diagnostic"
                }
                MultiSourceTestStage::AwaitAuthoredRemoveLive => {
                    "waiting-for-multi-source-authored-remove-live"
                }
                MultiSourceTestStage::AwaitAfterRemoveAggregateDiagnostic => {
                    "waiting-for-multi-source-after-remove-aggregate"
                }
                MultiSourceTestStage::AwaitRemovedAuthoredDiagnostic => {
                    "waiting-for-multi-source-removed-authored-diagnostic"
                }
                MultiSourceTestStage::AwaitVoxelMoveRegistry => {
                    "waiting-for-multi-source-voxel-move-registry"
                }
                MultiSourceTestStage::AwaitVoxelMoveLive => {
                    "waiting-for-multi-source-voxel-move-live"
                }
                MultiSourceTestStage::AwaitMovedVoxelStaleDiagnostic => {
                    "waiting-for-multi-source-moved-voxel-stale"
                }
                MultiSourceTestStage::AwaitOverflowLive => "waiting-for-multi-source-overflow-live",
                MultiSourceTestStage::AwaitFinalRegistry => {
                    "waiting-for-multi-source-final-registry"
                }
                MultiSourceTestStage::AwaitFinalLive => "waiting-for-multi-source-final-live",
                MultiSourceTestStage::AwaitFinalStaleDiagnostic => {
                    "waiting-for-multi-source-final-stale"
                }
                MultiSourceTestStage::AwaitFinalPublication => {
                    "waiting-for-multi-source-final-publication"
                }
            },
            TestScenePhase::LocalLightScaling(state) => state.phase_label(),
            TestScenePhase::CapturingRadianceBaseline { .. } => "capturing-radiance-baseline",
            TestScenePhase::MutatingRadianceR2 { .. } => "mutating-radiance-r2",
            TestScenePhase::CapturingRadianceR2NextFrame { .. } => {
                "capturing-radiance-r2-next-frame"
            }
            TestScenePhase::WaitingForRadianceR2Midflight { .. } => {
                "waiting-for-radiance-r2-midflight"
            }
            TestScenePhase::WaitingForRadianceR3Observed { .. } => {
                "waiting-for-radiance-r3-observed"
            }
            TestScenePhase::MutatingRadianceR4 { .. } => "mutating-radiance-r4",
            TestScenePhase::CapturingRadianceR4NextFrame { .. } => {
                "capturing-radiance-r4-next-frame"
            }
            TestScenePhase::WaitingForRadianceR4Midflight { .. } => {
                "waiting-for-radiance-r4-midflight"
            }
            TestScenePhase::WaitingForRadianceR4Published { .. } => {
                "waiting-for-radiance-r4-published"
            }
            TestScenePhase::CapturingRadianceR4Published { .. } => {
                "capturing-radiance-r4-published"
            }
            TestScenePhase::WaitingForDensityMidflight { .. } => "waiting-for-density-midflight",
            TestScenePhase::WaitingForDensityGeometryReplacement { .. } => {
                "waiting-for-density-geometry-replacement"
            }
            TestScenePhase::WaitingForDensityGeometryPublished { .. } => {
                "waiting-for-density-geometry-published"
            }
            TestScenePhase::WaitingForDensityRetryMidflight { .. } => {
                "waiting-for-density-retry-midflight"
            }
            TestScenePhase::WaitingForDensityFinalPublished { .. } => {
                "waiting-for-density-final-published"
            }
            TestScenePhase::TerrainEditPublished { .. } => "terrain-edit-published",
            TestScenePhase::WaitingForEditedProbeField { .. } => "waiting-for-edited-probe-field",
            TestScenePhase::WaitingForDensityRebuild { .. } => "waiting-for-density-rebuild",
            TestScenePhase::CapturingInflightStaleActive { .. } => {
                "capturing-inflight-stale-active"
            }
            TestScenePhase::Ready => "ready",
            TestScenePhase::Failed => "failed",
        }
    }
}

struct TestSceneGeometry {
    cleared_test_scene: Vec<Cuboid>,
    rock: Vec<Cuboid>,
    carved_empty: Vec<Cuboid>,
    sand: Vec<Cuboid>,
    emissive: Vec<Cuboid>,
    test_rebuild_bound: UAabb3,
}

fn test_rebuild_bound(case: EnvironmentLightingTestCase) -> UAabb3 {
    match case {
        EnvironmentLightingTestCase::Dogleg => {
            UAabb3::new(DOGLEG_CLEAR_MIN.as_uvec3(), DOGLEG_CLEAR_MAX.as_uvec3())
        }
        _ => UAabb3::new(TEST_REBUILD_MIN, TEST_REBUILD_MAX),
    }
}

impl TestSceneGeometry {
    fn build(case: EnvironmentLightingTestCase) -> Self {
        let test_rebuild_bound = test_rebuild_bound(case);
        let (cleared_test_scene, rock, carved_empty, sand) = match case {
            EnvironmentLightingTestCase::Sealed | EnvironmentLightingTestCase::PattSeam => (
                Vec::new(),
                vec![Cuboid::from_min_max(SHELL_MIN, SHELL_MAX)],
                vec![Cuboid::from_min_max(INTERIOR_MIN, INTERIOR_MAX)],
                Vec::new(),
            ),
            EnvironmentLightingTestCase::Portal
            | EnvironmentLightingTestCase::RadianceChanges
            | EnvironmentLightingTestCase::PointLightChanges
            | EnvironmentLightingTestCase::VoxelEmissiveChanges
            | EnvironmentLightingTestCase::RasterEmitterChanges
            | EnvironmentLightingTestCase::MultiSourceStress
            | EnvironmentLightingTestCase::LocalLightScaling
            | EnvironmentLightingTestCase::DensityChanges
            | EnvironmentLightingTestCase::TerrainEdits
            | EnvironmentLightingTestCase::TerrainEditsInflight
            | EnvironmentLightingTestCase::TerrainEditsInflightCapture
            | EnvironmentLightingTestCase::TerrainEditsClosed => (
                Vec::new(),
                vec![Cuboid::from_min_max(SHELL_MIN, SHELL_MAX)],
                vec![
                    Cuboid::from_min_max(INTERIOR_MIN, INTERIOR_MAX),
                    Cuboid::from_min_max(SKYLIGHT_MIN, SKYLIGHT_MAX),
                ],
                Vec::new(),
            ),
            EnvironmentLightingTestCase::Walls => {
                let mut rock = vec![
                    Cuboid::from_min_max(WALLS_FLOOR_MIN, WALLS_FLOOR_MAX),
                    Cuboid::from_min_max(WALLS_BACK_MIN, WALLS_BACK_MAX),
                    Cuboid::from_min_max(ONE_VOXEL_WALL_MIN, ONE_VOXEL_WALL_MAX),
                    Cuboid::from_min_max(TWO_VOXEL_WALL_MIN, TWO_VOXEL_WALL_MAX),
                ];
                for step in 0..12 {
                    let x = 320.0 + step as f32 * 8.0;
                    let z = 276.0 + step as f32 * 3.0;
                    rock.push(Cuboid::from_min_max(
                        Vec3::new(x, 100.0, z),
                        Vec3::new(x + 8.0, 196.0, z + 1.0),
                    ));
                }
                (Vec::new(), rock, Vec::new(), Vec::new())
            }
            EnvironmentLightingTestCase::Donor => (
                vec![Cuboid::from_min_max(DONOR_CLEAR_MIN, DONOR_CLEAR_MAX)],
                vec![
                    Cuboid::from_min_max(DONOR_FLOOR_MIN, DONOR_FLOOR_MAX),
                    Cuboid::from_min_max(DONOR_BACK_MIN, DONOR_BACK_MAX),
                    Cuboid::from_min_max(DONOR_LEFT_ROOF_MIN, DONOR_LEFT_ROOF_MAX),
                    Cuboid::from_min_max(DONOR_RIGHT_ROOF_MIN, DONOR_RIGHT_ROOF_MAX),
                    Cuboid::from_min_max(DONOR_DIVIDER_MIN, DONOR_DIVIDER_MAX),
                    Cuboid::from_min_max(DONOR_CONTROL_SLAB_MIN, DONOR_CONTROL_SLAB_MAX),
                ],
                Vec::new(),
                vec![Cuboid::from_min_max(DONOR_SLAB_MIN, DONOR_SLAB_MAX)],
            ),
            EnvironmentLightingTestCase::Dogleg => (
                vec![Cuboid::from_min_max(DOGLEG_CLEAR_MIN, DOGLEG_CLEAR_MAX)],
                vec![Cuboid::from_min_max(DOGLEG_BLOCK_MIN, DOGLEG_BLOCK_MAX)],
                vec![
                    Cuboid::from_min_max(DOGLEG_LIGHT_WELL_MIN, DOGLEG_LIGHT_WELL_MAX),
                    Cuboid::from_min_max(DOGLEG_FIRST_LEG_MIN, DOGLEG_FIRST_LEG_MAX),
                    Cuboid::from_min_max(DOGLEG_SECOND_LEG_MIN, DOGLEG_SECOND_LEG_MAX),
                    Cuboid::from_min_max(DOGLEG_RECEIVER_CHAMBER_MIN, DOGLEG_RECEIVER_CHAMBER_MAX),
                ],
                vec![Cuboid::from_min_max(
                    DOGLEG_FIRST_REFLECTOR_MIN,
                    DOGLEG_FIRST_REFLECTOR_MAX,
                )],
            ),
        };

        let emissive = if case == EnvironmentLightingTestCase::PointLightChanges {
            vec![Cuboid::from_min_max(
                POINT_LIGHT_EMISSIVE_MIN,
                POINT_LIGHT_EMISSIVE_MAX,
            )]
        } else {
            Vec::new()
        };
        Self {
            cleared_test_scene,
            rock,
            carved_empty,
            sand,
            emissive,
            test_rebuild_bound,
        }
    }

    fn compile(self) -> Result<WorldEditPlan> {
        let mut voxel_edits = Vec::new();
        if !self.cleared_test_scene.is_empty() {
            voxel_edits.push(stamp_cuboids(self.cleared_test_scene, VOXEL_TYPE_EMPTY)?);
        }
        voxel_edits.push(stamp_cuboids(self.rock, VOXEL_TYPE_ROCK)?);
        if !self.carved_empty.is_empty() {
            voxel_edits.push(stamp_cuboids(self.carved_empty, VOXEL_TYPE_EMPTY)?);
        }
        if !self.sand.is_empty() {
            voxel_edits.push(stamp_cuboids(self.sand, VOXEL_TYPE_SAND)?);
        }
        if !self.emissive.is_empty() {
            voxel_edits.push(stamp_cuboids(self.emissive, VOXEL_TYPE_EMISSIVE)?);
        }
        let build_edits = vec![BuildEdit::RebuildMesh(self.test_rebuild_bound)];
        Ok(WorldEditPlan {
            voxel_edits,
            build_edits,
        })
    }
}

fn stamp_cuboids(cuboids: Vec<Cuboid>, voxel_type: u32) -> Result<VoxelEdit> {
    let aabbs = cuboids.iter().map(Cuboid::aabb).collect::<Vec<_>>();
    let leaves = (0..cuboids.len() as u32).collect::<Vec<_>>();
    let bvh_nodes = build_bvh(&aabbs, &leaves).map_err(anyhow::Error::msg)?;
    Ok(VoxelEdit::StampCuboids {
        bvh_nodes,
        cuboids,
        voxel_type,
        atlas_state_write: Default::default(),
    })
}

fn skylight_edit_plan(edit: TerrainEdit) -> Result<WorldEditPlan> {
    Ok(WorldEditPlan {
        voxel_edits: vec![stamp_cuboids(
            vec![Cuboid::from_min_max(SKYLIGHT_MIN, SKYLIGHT_MAX)],
            edit.voxel_type(),
        )?],
        build_edits: vec![BuildEdit::RebuildMesh(UAabb3::new(
            SKYLIGHT_MIN.as_uvec3(),
            SKYLIGHT_MAX.as_uvec3(),
        ))],
    })
}

fn point_light_blocker_edit_plan(voxel_type: u32) -> Result<WorldEditPlan> {
    Ok(WorldEditPlan {
        voxel_edits: vec![stamp_cuboids(
            vec![Cuboid::from_min_max(
                POINT_LIGHT_BLOCKER_MIN,
                POINT_LIGHT_BLOCKER_MAX,
            )],
            voxel_type,
        )?],
        build_edits: vec![BuildEdit::RebuildMesh(UAabb3::new(
            POINT_LIGHT_BLOCKER_MIN.as_uvec3(),
            POINT_LIGHT_BLOCKER_MAX.as_uvec3(),
        ))],
    })
}

fn voxel_emissive_edit_plan(edits: &[(UVec3, UVec3, u32)]) -> Result<WorldEditPlan> {
    let min = edits
        .iter()
        .map(|(min, _, _)| *min)
        .reduce(UVec3::min)
        .context("voxel-emissive edit plan requires at least one voxel")?;
    let max = edits
        .iter()
        .map(|(_, max, _)| *max)
        .reduce(UVec3::max)
        .expect("non-empty edit plan has a maximum");
    let voxel_edits = edits
        .iter()
        .map(|(min, max, voxel_type)| {
            stamp_cuboids(
                vec![Cuboid::from_min_max(min.as_vec3(), max.as_vec3())],
                *voxel_type,
            )
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(WorldEditPlan {
        voxel_edits,
        build_edits: vec![BuildEdit::RebuildMesh(UAabb3::new(min, max))],
    })
}

fn voxel_emissive_record(snapshot: &LocalLightSnapshot, voxel: UVec3) -> Option<LocalLightRecord> {
    let key = EmissiveVoxelProvider::source_key_for_voxel(voxel);
    snapshot.lights().iter().copied().find(|record| {
        record.source().provider() == EMISSIVE_VOXEL_PROVIDER_ID && record.source().key() == key
    })
}

fn voxel_emissive_source_count(snapshot: &LocalLightSnapshot) -> usize {
    snapshot
        .lights()
        .iter()
        .filter(|record| record.source().provider() == EMISSIVE_VOXEL_PROVIDER_ID)
        .count()
}

fn provider_source_count(
    snapshot: &LocalLightSnapshot,
    provider: crate::lighting::ProviderId,
) -> usize {
    snapshot
        .lights()
        .iter()
        .filter(|record| record.source().provider() == provider)
        .count()
}

fn multi_source_authored_light() -> LocalLight {
    LocalLight::Point(
        PointLight::new(
            POINT_LIGHT_ADD_POSITION,
            MULTI_SOURCE_AUTHORED_COLOR,
            MULTI_SOURCE_AUTHORED_INTENSITY,
            POINT_LIGHT_SOURCE_RADIUS_WORLD,
            POINT_LIGHT_RANGE_WORLD,
        )
        .expect("multi-source authored point must be valid"),
    )
}

fn multi_source_raster_component() -> RasterEmitterComponent {
    raster_emitter_component(
        POINT_LIGHT_MOVED_POSITION,
        MULTI_SOURCE_RASTER_COLOR,
        MULTI_SOURCE_RASTER_INTENSITY,
    )
}

fn vec3_near(actual: Vec3, expected: Vec3, relative_epsilon: f32) -> bool {
    let scale = actual.abs().max(expected.abs()).max(Vec3::ONE);
    (actual - expected)
        .abs()
        .cmple(scale * relative_epsilon)
        .all()
}

fn expected_emissive_voxel_intensity(voxel_count: u32) -> f32 {
    // Isotropic 256 voxels/world-unit: mean projected area is 1.5 voxel-face areas.
    crate::lighting::EMISSIVE_VOXEL_SURFACE_RADIANCE
        * (1.5 / VOXELS_PER_WORLD_UNIT.powi(2))
        * voxel_count as f32
}

fn volume_voxels(min: UVec3, max: UVec3) -> u32 {
    (max - min).element_product()
}

fn raster_emitter_component(position: Vec3, color: Vec3, intensity: f32) -> RasterEmitterComponent {
    RasterEmitterComponent::new(LocalLight::Point(
        PointLight::new(
            position,
            color,
            intensity,
            POINT_LIGHT_SOURCE_RADIUS_WORLD,
            POINT_LIGHT_RANGE_WORLD,
        )
        .expect("raster-emitter lifecycle point light must be valid"),
    ))
}

fn camera_pose(case: EnvironmentLightingTestCase) -> (Vec3, Vec3) {
    match case {
        EnvironmentLightingTestCase::Sealed | EnvironmentLightingTestCase::PattSeam => {
            (Vec3::new(0.65, 0.58, 1.38), Vec3::new(0.65, 0.64, 1.02))
        }
        EnvironmentLightingTestCase::Portal
        | EnvironmentLightingTestCase::RadianceChanges
        | EnvironmentLightingTestCase::PointLightChanges
        | EnvironmentLightingTestCase::VoxelEmissiveChanges
        | EnvironmentLightingTestCase::RasterEmitterChanges
        | EnvironmentLightingTestCase::MultiSourceStress
        | EnvironmentLightingTestCase::LocalLightScaling
        | EnvironmentLightingTestCase::DensityChanges
        | EnvironmentLightingTestCase::TerrainEdits
        | EnvironmentLightingTestCase::TerrainEditsInflight
        | EnvironmentLightingTestCase::TerrainEditsInflightCapture
        | EnvironmentLightingTestCase::TerrainEditsClosed => {
            (Vec3::new(0.65, 0.52, 1.38), Vec3::new(0.65, 0.78, 1.10))
        }
        EnvironmentLightingTestCase::Walls => {
            (Vec3::new(1.00, 0.62, 1.76), Vec3::new(1.00, 0.58, 1.10))
        }
        EnvironmentLightingTestCase::Donor => {
            (Vec3::new(0.67, 0.50, 1.32), Vec3::new(0.67, 0.50, 0.56))
        }
        EnvironmentLightingTestCase::Dogleg => {
            (Vec3::new(1.52, 0.55, 0.58), Vec3::new(1.22, 0.52, 0.50))
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TestVoxelPalette {
    dirt: Color32,
    sand: Color32,
    cherry_wood: Color32,
    oak_wood: Color32,
    rock: Color32,
}

fn voxel_palette(case: EnvironmentLightingTestCase) -> TestVoxelPalette {
    TestVoxelPalette {
        dirt: Color32::from_rgb(95, 95, 95),
        sand: if matches!(
            case,
            EnvironmentLightingTestCase::Donor | EnvironmentLightingTestCase::Dogleg
        ) {
            Color32::from_rgb(224, 48, 32)
        } else {
            Color32::from_rgb(194, 176, 115)
        },
        cherry_wood: Color32::from_rgb(202, 176, 92),
        oak_wood: Color32::from_rgb(166, 144, 75),
        rock: Color32::from_rgb(122, 125, 128),
    }
}

fn test_lighting(case: EnvironmentLightingTestCase) -> (f32, f32, f32) {
    match case {
        EnvironmentLightingTestCase::PattSeam => {
            (PATT_SEAM_TIME_OF_DAY, PATT_SEAM_LATITUDE, PATT_SEAM_SEASON)
        }
        _ => (TEST_TIME_OF_DAY, TEST_LATITUDE, TEST_SEASON),
    }
}

fn voxel_roi_to_world(min_voxel: Vec3, max_voxel: Vec3) -> (Vec3, Vec3) {
    (
        min_voxel / VOXELS_PER_WORLD_UNIT,
        max_voxel / VOXELS_PER_WORLD_UNIT,
    )
}

impl App {
    pub(super) fn configure_environment_lighting_test_scene_camera(&mut self) {
        let case = self
            .environment_lighting_test_scene
            .as_ref()
            .expect("test scene camera requires test scene")
            .case;
        let (camera_position, camera_target) = camera_pose(case);
        let palette = voxel_palette(case);
        let (time_of_day, latitude, season) = test_lighting(case);
        let sun_luminance = RADIANCE_R1_SUN_LUMINANCE;
        self.set_manual_time_of_day(time_of_day);
        self.debug_settings.adjustables.latitude.value = latitude;
        self.debug_settings.adjustables.season.value = season;
        self.debug_settings.adjustables.auto_daynight_cycle.value = false;
        self.debug_settings.adjustables.sun_color.value = RADIANCE_R1_SUN_COLOR;
        self.debug_settings.adjustables.sun_luminance.value = sun_luminance;
        self.debug_settings.adjustables.voxel_dirt_color.value = palette.dirt;
        self.debug_settings.adjustables.voxel_sand_color.value = palette.sand;
        self.debug_settings
            .adjustables
            .voxel_cherry_wood_color
            .value = palette.cherry_wood;
        self.debug_settings.adjustables.voxel_oak_wood_color.value = palette.oak_wood;
        self.debug_settings.adjustables.voxel_rock_color.value = palette.rock;
        self.debug_settings.adjustables.voxel_color_variance.value = TEST_VOXEL_COLOR_VARIANCE;
        self.camera_control.set_orbit_focus(camera_target);
        if self
            .tracer
            .set_camera_pose_looking_at(camera_position, camera_target)
        {
            let effective_time_of_day = self.debug_settings.adjustables.time_of_day.value;
            let effective_latitude = self.debug_settings.adjustables.latitude.value;
            let effective_season = self.debug_settings.adjustables.season.value;
            let effective_sun_luminance = self.debug_settings.adjustables.sun_luminance.value;
            let effective_auto_cycle = self.debug_settings.adjustables.auto_daynight_cycle.value;
            log::info!(
                "[ENV_LIGHT_TEST] case={} camera position=({:.3},{:.3},{:.3}) target=({:.3},{:.3},{:.3}) sky_settings_source={} time_of_day={:.6} latitude={:.3} season={:.3} sun_luminance={:.3} auto_cycle={} voxel_color_variance={:.3}",
                case.label(),
                camera_position.x,
                camera_position.y,
                camera_position.z,
                camera_target.x,
                camera_target.y,
                camera_target.z,
                "scene-preset",
                effective_time_of_day,
                effective_latitude,
                effective_season,
                effective_sun_luminance,
                effective_auto_cycle,
                TEST_VOXEL_COLOR_VARIANCE,
            );
            if case == EnvironmentLightingTestCase::Donor {
                let (donor_receiver_min, donor_receiver_max) =
                    voxel_roi_to_world(DONOR_RECEIVER_ROI_MIN, DONOR_RECEIVER_ROI_MAX);
                let (control_receiver_min, control_receiver_max) = voxel_roi_to_world(
                    DONOR_CONTROL_RECEIVER_ROI_MIN,
                    DONOR_CONTROL_RECEIVER_ROI_MAX,
                );
                let (donor_surface_min, donor_surface_max) =
                    voxel_roi_to_world(DONOR_SURFACE_ROI_MIN, DONOR_SURFACE_ROI_MAX);
                log::info!(
                    "[ENV_LIGHT_TEST_ROI] case=donor donor_receiver_world={:?}..{:?} control_receiver_world={:?}..{:?} donor_surface_world={:?}..{:?}",
                    donor_receiver_min,
                    donor_receiver_max,
                    control_receiver_min,
                    control_receiver_max,
                    donor_surface_min,
                    donor_surface_max,
                );
            } else if case == EnvironmentLightingTestCase::Dogleg {
                let (first_min, first_max) = voxel_roi_to_world(
                    DOGLEG_FIRST_REFLECTOR_ROI_MIN,
                    DOGLEG_FIRST_REFLECTOR_ROI_MAX,
                );
                let (second_min, second_max) = voxel_roi_to_world(
                    DOGLEG_SECOND_REFLECTOR_ROI_MIN,
                    DOGLEG_SECOND_REFLECTOR_ROI_MAX,
                );
                let (receiver_min, receiver_max) = voxel_roi_to_world(
                    DOGLEG_FINAL_RECEIVER_ROI_MIN,
                    DOGLEG_FINAL_RECEIVER_ROI_MAX,
                );
                log::info!(
                    "[ENV_LIGHT_TEST_ROI] case=dogleg first_reflector_world={:?}..{:?} second_reflector_world={:?}..{:?} final_receiver_world={:?}..{:?} expected_first_signal_epoch=1 final_receiver_direct_sun=occluded",
                    first_min,
                    first_max,
                    second_min,
                    second_max,
                    receiver_min,
                    receiver_max,
                );
            }
        } else {
            log::error!("[ENV_LIGHT_TEST] failed to apply deterministic camera pose");
        }
    }

    fn apply_radiance_test_mutation(
        &mut self,
        time_of_day: f32,
        sun_color: Color32,
        sun_luminance: f32,
        rock_color: Color32,
    ) {
        self.set_manual_time_of_day(time_of_day);
        self.debug_settings.adjustables.sun_color.value = sun_color;
        self.debug_settings.adjustables.sun_luminance.value = sun_luminance;
        self.debug_settings.adjustables.voxel_rock_color.value = rock_color;
        self.tracer.invalidate_local_direct_sun_shadow_histories();
    }

    pub(super) fn process_radiance_test_mutation_after_render(&mut self) {
        let Some(phase) = self
            .environment_lighting_test_scene
            .as_ref()
            .map(|scene| scene.phase)
        else {
            return;
        };
        let mutation_frame = self.time_info.total_frame_count();
        let next_phase = match phase {
            TestScenePhase::MutatingRadianceR2 { r1 } => {
                self.apply_radiance_test_mutation(
                    RADIANCE_R2_TIME_OF_DAY,
                    RADIANCE_R2_SUN_COLOR,
                    RADIANCE_R2_SUN_LUMINANCE,
                    RADIANCE_R2_ROCK_COLOR,
                );
                log::info!(
                    "[DDGI_ACCEPT][RADIANCE] mutation=r2 after_render_frame={} first_affected_render_frame={} time_of_day={} sun_rgb={},{},{} sun_luminance={} rock_rgb={},{},{} expected_radiance_revision={}",
                    mutation_frame,
                    mutation_frame + 1,
                    RADIANCE_R2_TIME_OF_DAY,
                    RADIANCE_R2_SUN_COLOR.r(),
                    RADIANCE_R2_SUN_COLOR.g(),
                    RADIANCE_R2_SUN_COLOR.b(),
                    RADIANCE_R2_SUN_LUMINANCE,
                    RADIANCE_R2_ROCK_COLOR.r(),
                    RADIANCE_R2_ROCK_COLOR.g(),
                    RADIANCE_R2_ROCK_COLOR.b(),
                    next_nonzero_revision(r1.field().radiance_revision()),
                );
                TestScenePhase::CapturingRadianceR2NextFrame { r1, mutation_frame }
            }
            TestScenePhase::MutatingRadianceR4 { r1, r2 } => {
                let r4_revision =
                    next_nonzero_revision(next_nonzero_revision(r2.field().radiance_revision()));
                let active = self.tracer.ddgi_runtime_status().active();
                assert_eq!(active.published_field, Some(r1));
                assert_eq!(active.building_field, Some(r2));
                assert_eq!(
                    active.radiance_revision,
                    Some(r2.field().radiance_revision())
                );
                self.apply_radiance_test_mutation(
                    RADIANCE_R4_TIME_OF_DAY,
                    RADIANCE_R4_SUN_COLOR,
                    RADIANCE_R4_SUN_LUMINANCE,
                    RADIANCE_R4_ROCK_COLOR,
                );
                log::info!(
                    "[DDGI_ACCEPT][RADIANCE] mutation=r4 after_render_frame={} first_affected_render_frame={} time_of_day={} sun_rgb={},{},{} sun_luminance={} rock_rgb={},{},{} expected_radiance_revision={} inflight_field_serial={} immutable_inflight_radiance_revision={} latest_coalescing_pending=true",
                    mutation_frame,
                    mutation_frame + 1,
                    RADIANCE_R4_TIME_OF_DAY,
                    RADIANCE_R4_SUN_COLOR.r(),
                    RADIANCE_R4_SUN_COLOR.g(),
                    RADIANCE_R4_SUN_COLOR.b(),
                    RADIANCE_R4_SUN_LUMINANCE,
                    RADIANCE_R4_ROCK_COLOR.r(),
                    RADIANCE_R4_ROCK_COLOR.g(),
                    RADIANCE_R4_ROCK_COLOR.b(),
                    r4_revision,
                    r2.field().serial(),
                    r2.field().radiance_revision(),
                );
                TestScenePhase::CapturingRadianceR4NextFrame {
                    r1,
                    r2,
                    mutation_frame,
                }
            }
            _ => return,
        };
        self.environment_lighting_test_scene
            .as_mut()
            .expect("radiance mutation lost test scene")
            .phase = next_phase;
    }

    fn advance_multi_source_lifecycle(
        &mut self,
        mut state: MultiSourceLifecycleState,
        fixed_gpu_request_serial: u32,
    ) -> Option<TestScenePhase> {
        let next = match state.stage {
            MultiSourceTestStage::AwaitBaseline => {
                let status = self.tracer.ddgi_runtime_status();
                let active = status.active();
                let baseline = active.published_field?;
                if !is_converged_field(baseline)
                    || active.stage != DdgiVolumeStage::Ready
                    || active.building_field.is_some()
                    || status.staging().is_some()
                {
                    return None;
                }
                assert_eq!(baseline.field().geometry_revision(), state.terrain_revision);
                assert_eq!(self.local_lights.snapshot().lights().len(), 0);

                let authored_id = self.local_lights.add(multi_source_authored_light());
                let (raster_entity, raster_id) = self
                    .apply_emissive_sprinkler_placement(
                        SprinklerPlacementTarget::Terrain(RASTER_EMITTER_MOVED_BASE_POSITION),
                        multi_source_raster_component(),
                    )
                    .expect("multi-source raster emitter spawn must succeed");
                let terrain_revision = self
                    .apply_voxel_emissive_edits(
                        "multi-source-add",
                        &[(
                            VOXEL_EMISSIVE_PRIMARY_MIN,
                            VOXEL_EMISSIVE_PRIMARY_MAX,
                            VOXEL_TYPE_EMISSIVE,
                        )],
                        state.terrain_revision,
                    )
                    .expect("multi-source voxel emitter add must succeed");
                let snapshot = self.local_lights.snapshot();
                state.terrain_revision = terrain_revision;
                state.baseline = Some(baseline);
                state.authored_id = Some(authored_id);
                state.raster_id = Some(raster_id);
                state.raster_entity = Some(raster_entity);
                state.expected_source_revision = snapshot.source_revision();
                state.expected_registry_revision = snapshot.registry_revision();
                state.mutation_frame = self.time_info.total_frame_count();
                state.stage = MultiSourceTestStage::AwaitVoxelRegistry;
                log_acceptance_field("MULTI_SOURCE", "baseline", baseline);
                log::info!(
                    "[MULTI_SOURCE_ACCEPT] action=add authored_slot={} authored_generation={} raster_slot={} raster_generation={} raster_entity={:?} source_revision={} registry_revision={} target_geometry_revision={} provider_counts=authored:1,voxel:0,raster:1 renderer_instances={} surface_emissive_pixels=false",
                    authored_id.slot(),
                    authored_id.generation(),
                    raster_id.slot(),
                    raster_id.generation(),
                    raster_entity,
                    state.expected_source_revision,
                    state.expected_registry_revision,
                    terrain_revision,
                    self.tracer.sprinkler_instance_count(),
                );
                state
            }
            MultiSourceTestStage::AwaitVoxelRegistry => {
                let snapshot = self.local_lights.snapshot();
                let voxel_record = voxel_emissive_record(&snapshot, VOXEL_EMISSIVE_PRIMARY_MIN)?;
                if provider_source_count(&snapshot, AUTHORED_LOCAL_LIGHT_PROVIDER_ID) != 1
                    || provider_source_count(&snapshot, EMISSIVE_VOXEL_PROVIDER_ID) != 1
                    || provider_source_count(&snapshot, RASTER_ENTITY_LIGHT_PROVIDER_ID) != 1
                {
                    return None;
                }
                assert_eq!(snapshot.lights().len(), 3);
                state.voxel_id = Some(voxel_record.id());
                state.expected_source_revision = snapshot.source_revision();
                state.expected_registry_revision = snapshot.registry_revision();
                state.mutation_frame = self.time_info.total_frame_count();
                state.stage = MultiSourceTestStage::AwaitThreeLive;
                log::info!(
                    "[MULTI_SOURCE_ACCEPT] checkpoint=providers-authoritative source_revision={} registry_revision={} provider_counts=authored:1,voxel:1,raster:1 voxel_slot={} voxel_generation={} raster_instances={} no_duplicates=true",
                    state.expected_source_revision,
                    state.expected_registry_revision,
                    voxel_record.id().slot(),
                    voxel_record.id().generation(),
                    self.tracer.sprinkler_instance_count(),
                );
                state
            }
            MultiSourceTestStage::AwaitThreeLive => {
                if self.tracer.local_light_live_state() != (Some(state.expected_source_revision), 3)
                    || self.time_info.total_frame_count() <= state.mutation_frame
                {
                    return None;
                }
                assert_eq!(
                    self.tracer.local_light_revision_observability().1,
                    Some(state.expected_registry_revision)
                );
                assert!(
                    !self
                        .tracer
                        .ddgi_lighting_diagnostics()
                        .has_mixed_in_flight_revision
                );
                let request = self
                    .tracer
                    .request_local_light_visibility_diagnostic(
                        state.terrain_revision,
                        state.expected_source_revision,
                        state.authored_id.unwrap(),
                        POINT_LIGHT_FIXED_RECEIVER_WORLD,
                        POINT_LIGHT_FIXED_RECEIVER_NORMAL,
                        POINT_LIGHT_FIXED_RAY_ORIGIN_OFFSET_WORLD,
                    )
                    .expect("multi-source authored diagnostic request must succeed");
                self.environment_lighting_test_scene
                    .as_mut()
                    .unwrap()
                    .point_light_fixed_gpu_request_serial = request;
                state.stage = MultiSourceTestStage::AwaitAuthoredDiagnostic;
                state
            }
            MultiSourceTestStage::AwaitAuthoredDiagnostic => {
                let evidence = self.tracer.local_light_visibility_diagnostic_evidence()?;
                if evidence.request.request_serial != fixed_gpu_request_serial {
                    return None;
                }
                assert_eq!(evidence.request.target.light_id(), state.authored_id);
                assert!(evidence.identity_matches && evidence.irradiance_luma_q8 > 0);
                assert_eq!((evidence.candidates, evidence.visible), (1, 1));
                state.authored_irradiance = evidence.irradiance;
                let request = self
                    .tracer
                    .request_local_light_visibility_diagnostic(
                        state.terrain_revision,
                        state.expected_source_revision,
                        state.voxel_id.unwrap(),
                        POINT_LIGHT_FIXED_RECEIVER_WORLD,
                        POINT_LIGHT_FIXED_RECEIVER_NORMAL,
                        POINT_LIGHT_FIXED_RAY_ORIGIN_OFFSET_WORLD,
                    )
                    .expect("multi-source voxel diagnostic request must succeed");
                self.environment_lighting_test_scene
                    .as_mut()
                    .unwrap()
                    .point_light_fixed_gpu_request_serial = request;
                state.stage = MultiSourceTestStage::AwaitVoxelDiagnostic;
                state
            }
            MultiSourceTestStage::AwaitVoxelDiagnostic => {
                let evidence = self.tracer.local_light_visibility_diagnostic_evidence()?;
                if evidence.request.request_serial != fixed_gpu_request_serial {
                    return None;
                }
                assert_eq!(evidence.request.target.light_id(), state.voxel_id);
                assert!(evidence.identity_matches && evidence.irradiance_luma_q8 > 0);
                assert_eq!((evidence.candidates, evidence.visible), (1, 1));
                state.voxel_irradiance = evidence.irradiance;
                let request = self
                    .tracer
                    .request_local_light_visibility_diagnostic(
                        state.terrain_revision,
                        state.expected_source_revision,
                        state.raster_id.unwrap(),
                        POINT_LIGHT_FIXED_RECEIVER_WORLD,
                        POINT_LIGHT_FIXED_RECEIVER_NORMAL,
                        POINT_LIGHT_FIXED_RAY_ORIGIN_OFFSET_WORLD,
                    )
                    .expect("multi-source raster diagnostic request must succeed");
                self.environment_lighting_test_scene
                    .as_mut()
                    .unwrap()
                    .point_light_fixed_gpu_request_serial = request;
                state.stage = MultiSourceTestStage::AwaitRasterDiagnostic;
                state
            }
            MultiSourceTestStage::AwaitRasterDiagnostic => {
                let evidence = self.tracer.local_light_visibility_diagnostic_evidence()?;
                if evidence.request.request_serial != fixed_gpu_request_serial {
                    return None;
                }
                assert_eq!(evidence.request.target.light_id(), state.raster_id);
                assert!(evidence.identity_matches && evidence.irradiance_luma_q8 > 0);
                assert_eq!((evidence.candidates, evidence.visible), (1, 1));
                state.raster_irradiance = evidence.irradiance;
                let request = self
                    .tracer
                    .request_local_light_aggregate_visibility_diagnostic(
                        state.terrain_revision,
                        state.expected_source_revision,
                        POINT_LIGHT_FIXED_RECEIVER_WORLD,
                        POINT_LIGHT_FIXED_RECEIVER_NORMAL,
                        POINT_LIGHT_FIXED_RAY_ORIGIN_OFFSET_WORLD,
                    )
                    .expect("multi-source aggregate diagnostic request must succeed");
                self.environment_lighting_test_scene
                    .as_mut()
                    .unwrap()
                    .point_light_fixed_gpu_request_serial = request;
                state.stage = MultiSourceTestStage::AwaitAggregateDiagnostic;
                state
            }
            MultiSourceTestStage::AwaitAggregateDiagnostic => {
                let evidence = self.tracer.local_light_visibility_diagnostic_evidence()?;
                if evidence.request.request_serial != fixed_gpu_request_serial {
                    return None;
                }
                assert!(evidence.request.target.light_id().is_none());
                assert!(evidence.identity_matches);
                assert_eq!(
                    (evidence.candidates, evidence.visible, evidence.occluded),
                    (3, 3, 0)
                );
                let expected =
                    state.authored_irradiance + state.voxel_irradiance + state.raster_irradiance;
                assert!(vec3_near(evidence.irradiance, expected, 2.0e-5));
                state.aggregate_irradiance = evidence.irradiance;
                state.stage = MultiSourceTestStage::AwaitThreeDdgiPublication;
                log::info!(
                    "[MULTI_SOURCE_ACCEPT] checkpoint=gpu-additivity providers=authored+voxel+raster selected_count=3 individual_sum={:?} aggregate={:?} additive=true visibility_per_light=true",
                    expected,
                    evidence.irradiance,
                );
                state
            }
            MultiSourceTestStage::AwaitThreeDdgiPublication => {
                let transport = self.tracer.ddgi_live_radiance_snapshot()?;
                if transport.local_lights.source_revision() != state.expected_source_revision
                    || transport.local_lights.count() != 3
                {
                    return None;
                }
                let status = self.tracer.ddgi_runtime_status();
                let active = status.active();
                let field = active.published_field?;
                if field.field().radiance_revision()
                    != transport.local_lights.info.transport_revision
                    || field.field().geometry_revision() != state.terrain_revision
                    || !is_converged_field(field)
                    || active.stage != DdgiVolumeStage::Ready
                    || active.building_field.is_some()
                    || status.staging().is_some()
                {
                    return None;
                }
                let gpu = self.tracer.ddgi_local_light_gpu_evidence()?;
                if !gpu.matches_classified_field(field)
                    || gpu.local_source_revision != state.expected_source_revision
                    || gpu.local_light_count != 3
                    || !gpu.is_complete()
                {
                    return None;
                }
                assert_eq!(gpu.sampled_probe_count, active.grid.probe_count());
                assert!(gpu.totals.candidates > 0 && gpu.totals.irradiance_luma_q8 > 0);
                assert!(
                    !self
                        .tracer
                        .ddgi_lighting_diagnostics()
                        .has_mixed_in_flight_revision
                );

                self.local_lights
                    .update(
                        state.authored_id.unwrap(),
                        LocalLight::Point(
                            PointLight::new(
                                POINT_LIGHT_MOVED_POSITION,
                                MULTI_SOURCE_RASTER_COLOR,
                                MULTI_SOURCE_RASTER_INTENSITY,
                                POINT_LIGHT_SOURCE_RADIUS_WORLD,
                                POINT_LIGHT_RANGE_WORLD,
                            )
                            .unwrap(),
                        ),
                    )
                    .expect("multi-source authored swap update must retain stable id");
                let raster_id = self
                    .update_emissive_sprinkler(
                        state.raster_entity.unwrap(),
                        RASTER_EMITTER_ADD_BASE_POSITION,
                        raster_emitter_component(
                            POINT_LIGHT_ADD_POSITION,
                            MULTI_SOURCE_AUTHORED_COLOR,
                            MULTI_SOURCE_AUTHORED_INTENSITY,
                        ),
                    )
                    .expect("multi-source raster swap update must succeed");
                assert_eq!(raster_id, state.raster_id.unwrap());
                let snapshot = self.local_lights.snapshot();
                state.expected_source_revision = snapshot.source_revision();
                state.expected_registry_revision = snapshot.registry_revision();
                state.mutation_frame = self.time_info.total_frame_count();
                state.stage = MultiSourceTestStage::AwaitSwapLive;
                log::info!(
                    "[MULTI_SOURCE_ACCEPT] checkpoint=ddgi-three field_serial={} transport_revision={} source_revision={} probes={} candidates={} visible={} occluded={} luma_q8={} full_sweep=true no_starvation=true mixed_in_flight=false action=swap-authored-raster-descriptors stable_ids=true photometric_update=true move=true next_source_revision={}",
                    field.field().serial(),
                    field.field().radiance_revision(),
                    gpu.local_source_revision,
                    gpu.sampled_probe_count,
                    gpu.totals.candidates,
                    gpu.totals.visible,
                    gpu.totals.occluded,
                    gpu.totals.irradiance_luma_q8,
                    state.expected_source_revision,
                );
                state
            }
            MultiSourceTestStage::AwaitSwapLive => {
                if self.tracer.local_light_live_state() != (Some(state.expected_source_revision), 3)
                    || self.time_info.total_frame_count() <= state.mutation_frame
                {
                    return None;
                }
                assert_eq!(
                    self.tracer.local_light_revision_observability().1,
                    Some(state.expected_registry_revision)
                );
                let request = self
                    .tracer
                    .request_local_light_aggregate_visibility_diagnostic(
                        state.terrain_revision,
                        state.expected_source_revision,
                        POINT_LIGHT_FIXED_RECEIVER_WORLD,
                        POINT_LIGHT_FIXED_RECEIVER_NORMAL,
                        POINT_LIGHT_FIXED_RAY_ORIGIN_OFFSET_WORLD,
                    )
                    .expect("swapped aggregate diagnostic request must succeed");
                self.environment_lighting_test_scene
                    .as_mut()
                    .unwrap()
                    .point_light_fixed_gpu_request_serial = request;
                state.stage = MultiSourceTestStage::AwaitSwappedAggregateDiagnostic;
                state
            }
            MultiSourceTestStage::AwaitSwappedAggregateDiagnostic => {
                let evidence = self.tracer.local_light_visibility_diagnostic_evidence()?;
                if evidence.request.request_serial != fixed_gpu_request_serial {
                    return None;
                }
                assert_eq!((evidence.candidates, evidence.visible), (3, 3));
                assert!(vec3_near(
                    evidence.irradiance,
                    state.aggregate_irradiance,
                    2.0e-5
                ));
                let request = self
                    .tracer
                    .request_local_light_visibility_diagnostic(
                        state.terrain_revision,
                        state.expected_source_revision,
                        state.authored_id.unwrap(),
                        POINT_LIGHT_FIXED_RECEIVER_WORLD,
                        POINT_LIGHT_FIXED_RECEIVER_NORMAL,
                        POINT_LIGHT_FIXED_RAY_ORIGIN_OFFSET_WORLD,
                    )
                    .expect("swapped authored diagnostic request must succeed");
                self.environment_lighting_test_scene
                    .as_mut()
                    .unwrap()
                    .point_light_fixed_gpu_request_serial = request;
                state.stage = MultiSourceTestStage::AwaitSwappedAuthoredDiagnostic;
                log::info!(
                    "[MULTI_SOURCE_ACCEPT] checkpoint=gpu-order-independence before={:?} after={:?} descriptor_multiset_same=true provider_order_stable=true order_independent=true",
                    state.aggregate_irradiance,
                    evidence.irradiance,
                );
                state
            }
            MultiSourceTestStage::AwaitSwappedAuthoredDiagnostic => {
                let evidence = self.tracer.local_light_visibility_diagnostic_evidence()?;
                if evidence.request.request_serial != fixed_gpu_request_serial {
                    return None;
                }
                assert_eq!(evidence.request.target.light_id(), state.authored_id);
                assert!(evidence.identity_matches && evidence.irradiance_luma_q8 > 0);
                state.swapped_authored_irradiance = evidence.irradiance;
                self.local_lights
                    .remove(state.authored_id.unwrap())
                    .expect("multi-source authored removal must succeed");
                let snapshot = self.local_lights.snapshot();
                state.expected_source_revision = snapshot.source_revision();
                state.expected_registry_revision = snapshot.registry_revision();
                state.mutation_frame = self.time_info.total_frame_count();
                state.stage = MultiSourceTestStage::AwaitAuthoredRemoveLive;
                state
            }
            MultiSourceTestStage::AwaitAuthoredRemoveLive => {
                if self.tracer.local_light_live_state() != (Some(state.expected_source_revision), 2)
                    || self.time_info.total_frame_count() <= state.mutation_frame
                {
                    return None;
                }
                let request = self
                    .tracer
                    .request_local_light_aggregate_visibility_diagnostic(
                        state.terrain_revision,
                        state.expected_source_revision,
                        POINT_LIGHT_FIXED_RECEIVER_WORLD,
                        POINT_LIGHT_FIXED_RECEIVER_NORMAL,
                        POINT_LIGHT_FIXED_RAY_ORIGIN_OFFSET_WORLD,
                    )
                    .expect("post-removal aggregate diagnostic request must succeed");
                self.environment_lighting_test_scene
                    .as_mut()
                    .unwrap()
                    .point_light_fixed_gpu_request_serial = request;
                state.stage = MultiSourceTestStage::AwaitAfterRemoveAggregateDiagnostic;
                state
            }
            MultiSourceTestStage::AwaitAfterRemoveAggregateDiagnostic => {
                let evidence = self.tracer.local_light_visibility_diagnostic_evidence()?;
                if evidence.request.request_serial != fixed_gpu_request_serial {
                    return None;
                }
                assert_eq!((evidence.candidates, evidence.visible), (2, 2));
                let expected = state.aggregate_irradiance - state.swapped_authored_irradiance;
                assert!(vec3_near(evidence.irradiance, expected, 3.0e-5));
                let request = self
                    .tracer
                    .request_local_light_visibility_diagnostic(
                        state.terrain_revision,
                        state.expected_source_revision,
                        state.authored_id.unwrap(),
                        POINT_LIGHT_FIXED_RECEIVER_WORLD,
                        POINT_LIGHT_FIXED_RECEIVER_NORMAL,
                        POINT_LIGHT_FIXED_RAY_ORIGIN_OFFSET_WORLD,
                    )
                    .expect("removed authored diagnostic request must succeed");
                self.environment_lighting_test_scene
                    .as_mut()
                    .unwrap()
                    .point_light_fixed_gpu_request_serial = request;
                state.stage = MultiSourceTestStage::AwaitRemovedAuthoredDiagnostic;
                log::info!(
                    "[MULTI_SOURCE_ACCEPT] checkpoint=gpu-remove-one before={:?} removed={:?} after={:?} exact_provider_delta=true remaining_provider_count=2",
                    state.aggregate_irradiance,
                    state.swapped_authored_irradiance,
                    evidence.irradiance,
                );
                state
            }
            MultiSourceTestStage::AwaitRemovedAuthoredDiagnostic => {
                let evidence = self.tracer.local_light_visibility_diagnostic_evidence()?;
                if evidence.request.request_serial != fixed_gpu_request_serial {
                    return None;
                }
                assert!(!evidence.identity_matches);
                assert_eq!(
                    (evidence.candidates, evidence.visible, evidence.occluded),
                    (0, 0, 0)
                );
                assert_eq!(evidence.irradiance, Vec3::ZERO);
                let terrain_revision = self
                    .apply_voxel_emissive_edits(
                        "multi-source-move",
                        &[
                            (
                                VOXEL_EMISSIVE_PRIMARY_MIN,
                                VOXEL_EMISSIVE_PRIMARY_MAX,
                                VOXEL_TYPE_EMPTY,
                            ),
                            (
                                VOXEL_EMISSIVE_MOVED_MIN,
                                VOXEL_EMISSIVE_MOVED_MAX,
                                VOXEL_TYPE_EMISSIVE,
                            ),
                        ],
                        state.terrain_revision,
                    )
                    .expect("multi-source voxel move must succeed");
                state.terrain_revision = terrain_revision;
                state.stage = MultiSourceTestStage::AwaitVoxelMoveRegistry;
                state
            }
            MultiSourceTestStage::AwaitVoxelMoveRegistry => {
                let snapshot = self.local_lights.snapshot();
                if voxel_emissive_record(&snapshot, VOXEL_EMISSIVE_PRIMARY_MIN).is_some() {
                    return None;
                }
                let moved = voxel_emissive_record(&snapshot, VOXEL_EMISSIVE_MOVED_MIN)?;
                assert_ne!(moved.id(), state.voxel_id.unwrap());
                assert_eq!(
                    provider_source_count(&snapshot, EMISSIVE_VOXEL_PROVIDER_ID),
                    1
                );
                assert_eq!(
                    provider_source_count(&snapshot, RASTER_ENTITY_LIGHT_PROVIDER_ID),
                    1
                );
                let stale_voxel_id = state.voxel_id.replace(moved.id()).unwrap();
                state.stale_voxel_id = Some(stale_voxel_id);
                state.expected_source_revision = snapshot.source_revision();
                state.expected_registry_revision = snapshot.registry_revision();
                state.mutation_frame = self.time_info.total_frame_count();
                state.stage = MultiSourceTestStage::AwaitVoxelMoveLive;
                log::info!(
                    "[MULTI_SOURCE_ACCEPT] action=move-voxel old_slot={} old_generation={} new_slot={} new_generation={} geometry_revision={} source_revision={} registry_revision={} affected_old_plus_new=true stale_registry_source=false",
                    stale_voxel_id.slot(),
                    stale_voxel_id.generation(),
                    moved.id().slot(),
                    moved.id().generation(),
                    state.terrain_revision,
                    state.expected_source_revision,
                    state.expected_registry_revision,
                );
                state
            }
            MultiSourceTestStage::AwaitVoxelMoveLive => {
                if self.tracer.local_light_live_state() != (Some(state.expected_source_revision), 2)
                    || self.time_info.total_frame_count() <= state.mutation_frame
                {
                    return None;
                }
                let request = self
                    .tracer
                    .request_local_light_visibility_diagnostic(
                        state.terrain_revision,
                        state.expected_source_revision,
                        state.stale_voxel_id.unwrap(),
                        POINT_LIGHT_FIXED_RECEIVER_WORLD,
                        POINT_LIGHT_FIXED_RECEIVER_NORMAL,
                        POINT_LIGHT_FIXED_RAY_ORIGIN_OFFSET_WORLD,
                    )
                    .expect("stale multi-source identity request must succeed");
                self.environment_lighting_test_scene
                    .as_mut()
                    .unwrap()
                    .point_light_fixed_gpu_request_serial = request;
                state.stage = MultiSourceTestStage::AwaitMovedVoxelStaleDiagnostic;
                state
            }
            MultiSourceTestStage::AwaitMovedVoxelStaleDiagnostic => {
                let evidence = self.tracer.local_light_visibility_diagnostic_evidence()?;
                if evidence.request.request_serial != fixed_gpu_request_serial {
                    return None;
                }
                assert!(!evidence.identity_matches);
                assert_eq!(evidence.irradiance, Vec3::ZERO);

                let mut overflow_ids = Vec::with_capacity(LOCAL_LIGHT_GPU_CAPACITY + 1);
                for index in 0..LOCAL_LIGHT_GPU_CAPACITY {
                    overflow_ids.push(
                        self.local_lights.add(LocalLight::Point(
                            PointLight::new(
                                POINT_LIGHT_ADD_POSITION + Vec3::X * index as f32 * 0.002,
                                Vec3::ONE,
                                0.0,
                                POINT_LIGHT_SOURCE_RADIUS_WORLD,
                                POINT_LIGHT_RANGE_WORLD,
                            )
                            .unwrap(),
                        )),
                    );
                }
                overflow_ids.push(
                    self.local_lights.add(LocalLight::Spot(
                        SpotLight::new(
                            POINT_LIGHT_ADD_POSITION,
                            -Vec3::Y,
                            Vec3::ONE,
                            0.1,
                            POINT_LIGHT_SOURCE_RADIUS_WORLD,
                            POINT_LIGHT_RANGE_WORLD,
                            0.1,
                            0.2,
                        )
                        .unwrap(),
                    )),
                );
                self.environment_lighting_test_scene
                    .as_mut()
                    .unwrap()
                    .multi_source_overflow_authored_ids = overflow_ids;
                let snapshot = self.local_lights.snapshot();
                assert_eq!(snapshot.lights().len(), LOCAL_LIGHT_GPU_CAPACITY + 3);
                state.expected_source_revision = snapshot.source_revision();
                state.expected_registry_revision = snapshot.registry_revision();
                state.mutation_frame = self.time_info.total_frame_count();
                state.stage = MultiSourceTestStage::AwaitOverflowLive;
                state
            }
            MultiSourceTestStage::AwaitOverflowLive => {
                if self.tracer.local_light_live_state()
                    != (
                        Some(state.expected_source_revision),
                        LOCAL_LIGHT_GPU_CAPACITY as u32,
                    )
                    || self.time_info.total_frame_count() <= state.mutation_frame
                {
                    return None;
                }
                let overflow = self.tracer.local_light_overflow_evidence();
                assert_eq!(overflow.len(), 3);
                assert_eq!(
                    overflow
                        .iter()
                        .filter(|item| {
                            item.source.provider() == AUTHORED_LOCAL_LIGHT_PROVIDER_ID
                                && item.reason
                                    == crate::lighting::LocalLightOverflowReason::UnsupportedKind
                        })
                        .count(),
                    1
                );
                assert_eq!(
                    overflow
                        .iter()
                        .filter(|item| {
                            item.source.provider() == EMISSIVE_VOXEL_PROVIDER_ID
                                && item.reason
                                    == crate::lighting::LocalLightOverflowReason::Capacity
                        })
                        .count(),
                    1
                );
                assert_eq!(
                    overflow
                        .iter()
                        .filter(|item| {
                            item.source.provider() == RASTER_ENTITY_LIGHT_PROVIDER_ID
                                && item.reason
                                    == crate::lighting::LocalLightOverflowReason::Capacity
                        })
                        .count(),
                    1
                );
                log::info!(
                    "[MULTI_SOURCE_ACCEPT] checkpoint=overcapacity authoritative_count={} accepted_count={} overflow_count=3 overflow_by_provider_reason=authored:UnsupportedKind:1,voxel:Capacity:1,raster:Capacity:1 deterministic_selection=true silent_drop=false",
                    LOCAL_LIGHT_GPU_CAPACITY + 3,
                    LOCAL_LIGHT_GPU_CAPACITY,
                );

                let authored_ids = std::mem::take(
                    &mut self
                        .environment_lighting_test_scene
                        .as_mut()
                        .unwrap()
                        .multi_source_overflow_authored_ids,
                );
                for id in authored_ids {
                    self.local_lights
                        .remove(id)
                        .expect("multi-source overflow authored cleanup must succeed");
                }
                let removed_raster = self
                    .remove_emissive_sprinkler(state.raster_entity.unwrap())
                    .expect("multi-source raster cleanup must succeed");
                assert_eq!(removed_raster, state.raster_id.unwrap());
                let terrain_revision = self
                    .apply_voxel_emissive_edits(
                        "multi-source-remove",
                        &[(
                            VOXEL_EMISSIVE_MOVED_MIN,
                            VOXEL_EMISSIVE_MOVED_MAX,
                            VOXEL_TYPE_EMPTY,
                        )],
                        state.terrain_revision,
                    )
                    .expect("multi-source voxel cleanup must succeed");
                state.terrain_revision = terrain_revision;
                state.stage = MultiSourceTestStage::AwaitFinalRegistry;
                state
            }
            MultiSourceTestStage::AwaitFinalRegistry => {
                let snapshot = self.local_lights.snapshot();
                if !snapshot.lights().is_empty()
                    || self.raster_entity_emitters.source_count() != 0
                    || !self.sprinklers.is_empty()
                    || voxel_emissive_source_count(&snapshot) != 0
                {
                    return None;
                }
                assert_eq!(self.tracer.sprinkler_instance_count(), 0);
                assert!(self
                    .local_lights
                    .light_id(
                        RASTER_ENTITY_LIGHT_PROVIDER_ID,
                        RasterEmitterKey::new(
                            state.raster_entity.unwrap(),
                            SPRINKLER_HEAD_EMITTER_PART,
                        )
                        .source_key(),
                    )
                    .is_none());
                state.expected_source_revision = snapshot.source_revision();
                state.expected_registry_revision = snapshot.registry_revision();
                state.mutation_frame = self.time_info.total_frame_count();
                state.stage = MultiSourceTestStage::AwaitFinalLive;
                state
            }
            MultiSourceTestStage::AwaitFinalLive => {
                if self.tracer.local_light_live_state() != (Some(state.expected_source_revision), 0)
                    || self.time_info.total_frame_count() <= state.mutation_frame
                {
                    return None;
                }
                assert!(self.tracer.local_light_overflow_evidence().is_empty());
                let request = self
                    .tracer
                    .request_local_light_visibility_diagnostic(
                        state.terrain_revision,
                        state.expected_source_revision,
                        state.raster_id.unwrap(),
                        POINT_LIGHT_FIXED_RECEIVER_WORLD,
                        POINT_LIGHT_FIXED_RECEIVER_NORMAL,
                        POINT_LIGHT_FIXED_RAY_ORIGIN_OFFSET_WORLD,
                    )
                    .expect("final stale raster diagnostic request must succeed");
                self.environment_lighting_test_scene
                    .as_mut()
                    .unwrap()
                    .point_light_fixed_gpu_request_serial = request;
                state.stage = MultiSourceTestStage::AwaitFinalStaleDiagnostic;
                state
            }
            MultiSourceTestStage::AwaitFinalStaleDiagnostic => {
                let evidence = self.tracer.local_light_visibility_diagnostic_evidence()?;
                if evidence.request.request_serial != fixed_gpu_request_serial {
                    return None;
                }
                assert!(!evidence.identity_matches);
                assert_eq!(
                    (evidence.candidates, evidence.visible, evidence.occluded),
                    (0, 0, 0)
                );
                assert_eq!(evidence.irradiance, Vec3::ZERO);
                state.stage = MultiSourceTestStage::AwaitFinalPublication;
                state
            }
            MultiSourceTestStage::AwaitFinalPublication => {
                let transport = self.tracer.ddgi_live_radiance_snapshot()?;
                if transport.local_lights.source_revision() != state.expected_source_revision
                    || transport.local_lights.count() != 0
                {
                    return None;
                }
                let status = self.tracer.ddgi_runtime_status();
                let active = status.active();
                let field = active.published_field?;
                if field.field().radiance_revision()
                    != transport.local_lights.info.transport_revision
                    || field.field().geometry_revision() != state.terrain_revision
                    || !is_converged_field(field)
                    || active.stage != DdgiVolumeStage::Ready
                    || active.building_field.is_some()
                    || status.staging().is_some()
                {
                    return None;
                }
                let gpu = self.tracer.ddgi_local_light_gpu_evidence()?;
                if !gpu.matches_classified_field(field)
                    || gpu.local_source_revision != state.expected_source_revision
                    || gpu.local_light_count != 0
                    || !gpu.is_complete()
                {
                    return None;
                }
                assert_eq!(gpu.sampled_probe_count, active.grid.probe_count());
                assert_eq!(gpu.totals.candidates, 0);
                assert_eq!(gpu.totals.visible, 0);
                assert_eq!(gpu.totals.occluded, 0);
                assert_eq!(gpu.totals.irradiance_luma_q8, 0);
                let diagnostics = self.tracer.ddgi_lighting_diagnostics();
                assert!(!diagnostics.has_mixed_in_flight_revision);
                assert_eq!(diagnostics.in_flight_revision, None);
                let atlas = active
                    .last_atlas_validation
                    .expect("multi-source final field must have atlas evidence");
                assert_eq!(atlas.non_finite_count, 0);
                assert!(atlas.max_rgb_value > 0.0);
                let (lag, coalesced) = self.tracer.local_light_transport_observability();
                assert_eq!(lag, 0);
                log_acceptance_field("MULTI_SOURCE", "complete", field);
                log::info!(
                    "[MULTI_SOURCE_ACCEPT] complete production_providers=authored+voxel+raster add=true move=true photometric=true remove=true overcapacity=true final_zero=true stable_ids=true registry_revision={} source_revision={} transport_revision={} revision_lag={} coalesced_live_revisions={} full_sweep_probes={} ddgi_candidates=0 ddgi_luma_q8=0 stale_direct=false stale_ddgi=false mixed_in_flight=false atlas_finite=true atlas_nonblack=true atlas_max_rgb={:.8} surface_emissive_pixels=false",
                    state.expected_registry_revision,
                    state.expected_source_revision,
                    field.field().radiance_revision(),
                    lag,
                    coalesced,
                    gpu.sampled_probe_count,
                    atlas.max_rgb_value,
                );
                return Some(TestScenePhase::Ready);
            }
        };
        Some(TestScenePhase::MultiSourceLifecycle(next))
    }

    pub(super) fn process_environment_lighting_test_scene(&mut self) {
        let Some((case, phase, fixed_gpu_request_serial, fixed_gpu_visible_luma_q8)) =
            self.environment_lighting_test_scene.as_ref().map(|scene| {
                (
                    scene.case,
                    scene.phase,
                    scene.point_light_fixed_gpu_request_serial,
                    scene.point_light_fixed_gpu_visible_luma_q8,
                )
            })
        else {
            return;
        };

        let next_phase = match phase {
            TestScenePhase::Pending => {
                let Some(render_start) = self.render_start_time else {
                    return;
                };
                if render_start.elapsed().as_secs_f32() < BUILD_DELAY_SECONDS {
                    return;
                }

                log::info!(
                    "[ENV_LIGHT_TEST] constructing static case={} before probe initialization",
                    case.label(),
                );
                match TestSceneGeometry::build(case)
                    .compile()
                    .context("compile deterministic environment-lighting test scene")
                    .and_then(|plan| self.execute_edit_plan(plan))
                {
                    Ok(()) => {
                        let rebuild_bound = test_rebuild_bound(case);
                        log::info!(
                            "[ENV_LIGHT_TEST] static edits applied case={} rebuild_voxel_bound={:?}..{:?}",
                            case.label(),
                            rebuild_bound.min(),
                            rebuild_bound.max(),
                        );
                        TestScenePhase::TerrainPublished
                    }
                    Err(err) => {
                        log::error!("[ENV_LIGHT_TEST] construction failed: {err:#}");
                        TestScenePhase::Failed
                    }
                }
            }
            TestScenePhase::TerrainPublished => {
                let terrain_revision = self
                    .observe_initial_published_terrain_for_ddgi()
                    .unwrap_or_else(|err| {
                        panic!("[ENV_LIGHT_TEST] DDGI visibility publication failed: {err:#}")
                    });
                log::info!(
                    "[ENV_LIGHT_TEST] static terrain ready case={} terrain_revision={} settling_frames={}",
                    case.label(),
                    terrain_revision,
                    SETTLE_FRAMES,
                );
                TestScenePhase::Settling {
                    frames: SETTLE_FRAMES,
                    terrain_revision,
                }
            }
            TestScenePhase::Settling {
                frames,
                terrain_revision,
            } => {
                if frames > 1 {
                    TestScenePhase::Settling {
                        frames: frames - 1,
                        terrain_revision,
                    }
                } else {
                    TestScenePhase::WaitingForProbeField { terrain_revision }
                }
            }
            TestScenePhase::WaitingForProbeField { terrain_revision } => {
                if !self
                    .tracer
                    .ddgi_ready_for_terrain_revision(terrain_revision)
                {
                    return;
                }
                if case == EnvironmentLightingTestCase::RadianceChanges {
                    TestScenePhase::WaitingForRadianceBaseline { terrain_revision }
                } else if case == EnvironmentLightingTestCase::PointLightChanges {
                    TestScenePhase::PointLightLifecycle {
                        terrain_revision,
                        baseline: None,
                        light_id: None,
                        in_flight: None,
                        in_flight_source_revision: 0,
                        stage: PointLightTestStage::AwaitBaseline,
                        expected_source_revision: 0,
                        mutation_frame: 0,
                    }
                } else if case == EnvironmentLightingTestCase::VoxelEmissiveChanges {
                    let snapshot = self.local_lights.snapshot();
                    TestScenePhase::VoxelEmissiveLifecycle(VoxelEmissiveLifecycleState {
                        terrain_revision,
                        baseline: None,
                        light_id: None,
                        stage: VoxelEmissiveTestStage::AwaitBaseline,
                        expected_source_revision: snapshot.source_revision(),
                        expected_registry_revision: snapshot.registry_revision(),
                        mutation_frame: 0,
                        visible_luma_q8: 0,
                        primary_intensity_bits: 0,
                    })
                } else if case == EnvironmentLightingTestCase::RasterEmitterChanges {
                    let snapshot = self.local_lights.snapshot();
                    TestScenePhase::RasterEmitterLifecycle(RasterEmitterLifecycleState {
                        terrain_revision,
                        baseline: None,
                        entity: None,
                        light_id: None,
                        in_flight: None,
                        in_flight_source_revision: 0,
                        stage: RasterEmitterTestStage::AwaitBaseline,
                        expected_source_revision: snapshot.source_revision(),
                        expected_registry_revision: snapshot.registry_revision(),
                        expected_provider_revision: self
                            .raster_entity_emitters
                            .snapshot()
                            .source_revision(),
                        expected_sprinkler_revision: self.sprinklers.revision(),
                        mutation_frame: 0,
                        visible_luma_q8: 0,
                    })
                } else if case == EnvironmentLightingTestCase::MultiSourceStress {
                    let snapshot = self.local_lights.snapshot();
                    TestScenePhase::MultiSourceLifecycle(MultiSourceLifecycleState {
                        terrain_revision,
                        baseline: None,
                        authored_id: None,
                        voxel_id: None,
                        stale_voxel_id: None,
                        raster_id: None,
                        raster_entity: None,
                        stage: MultiSourceTestStage::AwaitBaseline,
                        expected_source_revision: snapshot.source_revision(),
                        expected_registry_revision: snapshot.registry_revision(),
                        mutation_frame: 0,
                        authored_irradiance: Vec3::ZERO,
                        voxel_irradiance: Vec3::ZERO,
                        raster_irradiance: Vec3::ZERO,
                        aggregate_irradiance: Vec3::ZERO,
                        swapped_authored_irradiance: Vec3::ZERO,
                    })
                } else if case == EnvironmentLightingTestCase::LocalLightScaling {
                    TestScenePhase::LocalLightScaling(LocalLightScalingState::new(terrain_revision))
                } else if case == EnvironmentLightingTestCase::DensityChanges {
                    let runtime = self.tracer.ddgi_runtime_status();
                    let baseline = runtime
                        .active()
                        .published_field
                        .expect("density lifecycle requires an initial published field");
                    assert_eq!(baseline.field().geometry_revision(), terrain_revision);
                    assert_eq!(runtime.active().grid.spacing_voxels(), 32);
                    assert!(runtime.active_consumers_are_available());
                    log_acceptance_field("DENSITY", "baseline", baseline);
                    self.tracer.rebuild_environment_probes(16);
                    TestScenePhase::WaitingForDensityMidflight { baseline }
                } else if case == EnvironmentLightingTestCase::PattSeam {
                    log::info!(
                        "[DDGI_SEAM_REPRO] initial probe field ready terrain_revision={}",
                        terrain_revision,
                    );
                    match self.apply_patt_seam_dig(terrain_revision) {
                        Ok(target_revision) => {
                            TestScenePhase::PattSeamTerrainPublished { target_revision }
                        }
                        Err(err) => {
                            log::error!("[DDGI_SEAM_REPRO] shovel replay failed: {err:#}");
                            TestScenePhase::Failed
                        }
                    }
                } else if is_terrain_edit_case(case) {
                    log::info!(
                        "[ENV_LIGHT_EDIT_CYCLE] initial probe field ready terrain_revision={}",
                        terrain_revision,
                    );
                    match self.apply_environment_lighting_terrain_edit(
                        TerrainEdit::CloseSkylight,
                        terrain_revision,
                    ) {
                        Ok(target_revision) => TestScenePhase::TerrainEditPublished {
                            edit: TerrainEdit::CloseSkylight,
                            target_revision,
                        },
                        Err(err) => {
                            log::error!("[ENV_LIGHT_EDIT_CYCLE] close edit failed: {err:#}");
                            TestScenePhase::Failed
                        }
                    }
                } else {
                    log::info!(
                        "[ENV_LIGHT_TEST] ready case={} backend={} terrain_revision={} geometry=static",
                        case.label(),
                        "ddgi",
                        terrain_revision,
                    );
                    TestScenePhase::Ready
                }
            }
            TestScenePhase::WaitingForRadianceBaseline { terrain_revision } => {
                let status = self.tracer.ddgi_runtime_status();
                let active = status.active();
                let Some(r1) = active.published_field else {
                    return;
                };
                if !is_converged_field(r1)
                    || active.stage != DdgiVolumeStage::Ready
                    || active.building_field.is_some()
                    || status.staging().is_some()
                {
                    return;
                }
                assert_eq!(r1.field().geometry_revision(), terrain_revision);
                assert_eq!(
                    self.tracer.ddgi_latest_radiance_revision(),
                    Some(r1.field().radiance_revision())
                );
                log_acceptance_field("RADIANCE", "r1-terminal", r1);
                if self.environment_irradiance_capture.is_enabled() {
                    assert_eq!(
                        self.tracer.ddgi_capture_target(),
                        crate::ddgi::DdgiCaptureTarget::Published,
                        "radiance lifecycle capture requires target=published"
                    );
                    TestScenePhase::CapturingRadianceBaseline { r1 }
                } else {
                    self.apply_radiance_test_mutation(
                        RADIANCE_R2_TIME_OF_DAY,
                        RADIANCE_R2_SUN_COLOR,
                        RADIANCE_R2_SUN_LUMINANCE,
                        RADIANCE_R2_ROCK_COLOR,
                    );
                    log::info!(
                        "[DDGI_ACCEPT][RADIANCE] mutation=r2 frame={} time_of_day={} sun_rgb={},{},{} sun_luminance={} rock_rgb={},{},{} expected_radiance_revision={}",
                        self.time_info.total_frame_count(),
                        RADIANCE_R2_TIME_OF_DAY,
                        RADIANCE_R2_SUN_COLOR.r(),
                        RADIANCE_R2_SUN_COLOR.g(),
                        RADIANCE_R2_SUN_COLOR.b(),
                        RADIANCE_R2_SUN_LUMINANCE,
                        RADIANCE_R2_ROCK_COLOR.r(),
                        RADIANCE_R2_ROCK_COLOR.g(),
                        RADIANCE_R2_ROCK_COLOR.b(),
                        next_nonzero_revision(r1.field().radiance_revision()),
                    );
                    TestScenePhase::WaitingForRadianceR2Midflight { r1 }
                }
            }
            TestScenePhase::PointLightLifecycle {
                terrain_revision,
                baseline,
                light_id,
                in_flight,
                in_flight_source_revision,
                stage,
                expected_source_revision,
                mutation_frame,
            } => match stage {
                PointLightTestStage::AwaitBaseline => {
                    let status = self.tracer.ddgi_runtime_status();
                    let active = status.active();
                    let Some(baseline) = active.published_field else {
                        return;
                    };
                    if !is_converged_field(baseline)
                        || active.stage != DdgiVolumeStage::Ready
                        || active.building_field.is_some()
                        || status.staging().is_some()
                    {
                        return;
                    }
                    assert_eq!(baseline.field().geometry_revision(), terrain_revision);
                    assert_eq!(active.relocated_terrain_revision, Some(terrain_revision));
                    let selected_decoy_id = self.local_lights.add(LocalLight::Point(
                        PointLight::new(
                            POINT_LIGHT_ADD_POSITION,
                            Vec3::new(1.0, 0.0, 0.0),
                            8.0,
                            POINT_LIGHT_SOURCE_RADIUS_WORLD,
                            POINT_LIGHT_RANGE_WORLD,
                        )
                        .expect("point-light diagnostic decoy must be valid"),
                    ));
                    let id = self.local_lights.add(LocalLight::Point(
                        PointLight::new(
                            POINT_LIGHT_ADD_POSITION,
                            Vec3::new(1.0, 0.45, 0.20),
                            0.08,
                            POINT_LIGHT_SOURCE_RADIUS_WORLD,
                            POINT_LIGHT_RANGE_WORLD,
                        )
                        .expect("point-light test add must be valid"),
                    ));
                    let mut supplemental_ids = vec![selected_decoy_id];
                    for index in 0..7 {
                        supplemental_ids.push(
                            self.local_lights.add(LocalLight::Point(
                                PointLight::new(
                                    POINT_LIGHT_ADD_POSITION
                                        + Vec3::X * (index as f32 + 1.0) * 0.01,
                                    Vec3::ONE,
                                    0.0,
                                    POINT_LIGHT_SOURCE_RADIUS_WORLD,
                                    POINT_LIGHT_RANGE_WORLD,
                                )
                                .expect("zero-energy diagnostic capacity source must be valid"),
                            )),
                        );
                    }
                    let overflow_id = *supplemental_ids
                        .last()
                        .expect("diagnostic overflow source must exist");
                    let scene = self
                        .environment_lighting_test_scene
                        .as_mut()
                        .expect("point-light test scene must exist");
                    scene.point_light_diagnostic_selected_decoy_id = Some(selected_decoy_id);
                    scene.point_light_diagnostic_overflow_id = Some(overflow_id);
                    scene.point_light_diagnostic_supplemental_ids = supplemental_ids;
                    let source_revision = self.local_lights.snapshot().source_revision();
                    let frame = self.time_info.total_frame_count();
                    log_acceptance_field("POINT_LIGHT", "baseline", baseline);
                    log::info!(
                        "[POINT_LIGHT_ACCEPT] action=add-small-n frame={} target_slot={} target_generation={} target_selected_index=1 source_revision={} authoritative_count=9 gpu_capacity=8 expected_gpu_count=8 expected_overflow=1 position_world={:?} color={:?} intensity={} source_radius_world={} range_world={} geometry_revision={} direct_expected=next_render",
                        frame,
                        id.slot(),
                        id.generation(),
                        source_revision,
                        POINT_LIGHT_ADD_POSITION,
                        Vec3::new(1.0, 0.45, 0.20),
                        0.08,
                        POINT_LIGHT_SOURCE_RADIUS_WORLD,
                        POINT_LIGHT_RANGE_WORLD,
                        terrain_revision,
                    );
                    TestScenePhase::PointLightLifecycle {
                        terrain_revision,
                        baseline: Some(baseline),
                        light_id: Some(id),
                        in_flight: None,
                        in_flight_source_revision: 0,
                        stage: PointLightTestStage::AwaitAddLive,
                        expected_source_revision: source_revision,
                        mutation_frame: frame,
                    }
                }
                PointLightTestStage::AwaitAddLive => {
                    let (live_revision, live_count) = self.tracer.local_light_live_state();
                    if live_revision != Some(expected_source_revision) {
                        return;
                    }
                    assert_eq!(live_count, 8);
                    assert!(self.time_info.total_frame_count() > mutation_frame);
                    let (revision_lag, coalesced) =
                        self.tracer.local_light_transport_observability();
                    log::info!(
                        "[POINT_LIGHT_ACCEPT] checkpoint=add-live direct_immediate=true source_revision={} live_count={} mutation_frame={} observed_frame={} transport_revision_lag={} coalesced_live_revisions={} terrain_direct=true raster_direct=true shared_visibility=exact-voxel-segment",
                        expected_source_revision,
                        live_count,
                        mutation_frame,
                        self.time_info.total_frame_count(),
                        revision_lag,
                        coalesced,
                    );
                    let id = light_id.expect("point-light id must be retained");
                    let request_serial = self
                        .tracer
                        .request_local_light_visibility_diagnostic(
                            terrain_revision,
                            expected_source_revision,
                            id,
                            POINT_LIGHT_FIXED_RECEIVER_WORLD,
                            POINT_LIGHT_FIXED_RECEIVER_NORMAL,
                            POINT_LIGHT_FIXED_RAY_ORIGIN_OFFSET_WORLD,
                        )
                        .expect("visible fixed-receiver diagnostic request must succeed");
                    self.environment_lighting_test_scene
                        .as_mut()
                        .expect("point-light test scene must exist")
                        .point_light_fixed_gpu_request_serial = request_serial;
                    TestScenePhase::PointLightLifecycle {
                        terrain_revision,
                        baseline,
                        light_id,
                        in_flight,
                        in_flight_source_revision,
                        stage: PointLightTestStage::AwaitVisibleDiagnostic,
                        expected_source_revision,
                        mutation_frame: self.time_info.total_frame_count(),
                    }
                }
                PointLightTestStage::AwaitVisibleDiagnostic => {
                    let Some(evidence) = self.tracer.local_light_visibility_diagnostic_evidence()
                    else {
                        return;
                    };
                    if evidence.request.request_serial != fixed_gpu_request_serial {
                        return;
                    }
                    let id = light_id.expect("point-light id must be retained");
                    assert!(evidence.identity_matches);
                    assert_eq!(evidence.selected_light_index, Some(1));
                    assert_eq!(evidence.request.geometry_revision, terrain_revision);
                    assert_eq!(evidence.request.source_revision, expected_source_revision);
                    assert_eq!(evidence.request.target.light_id(), Some(id));
                    assert_eq!(
                        evidence.request.receiver_position,
                        POINT_LIGHT_FIXED_RECEIVER_WORLD
                    );
                    assert_eq!(
                        evidence.request.receiver_normal,
                        POINT_LIGHT_FIXED_RECEIVER_NORMAL
                    );
                    assert_eq!(evidence.candidates, 1);
                    assert_eq!(evidence.visible, 1);
                    assert_eq!(evidence.occluded, 0);
                    assert!(evidence.irradiance_luma_q8 > 0);
                    assert!(evidence.irradiance.min_element() > 0.0);
                    self.environment_lighting_test_scene
                        .as_mut()
                        .expect("point-light test scene must exist")
                        .point_light_fixed_gpu_visible_luma_q8 = evidence.irradiance_luma_q8;
                    let terrain_revision = self
                        .apply_point_light_blocker(VOXEL_TYPE_ROCK, terrain_revision)
                        .expect("point-light blocker add must succeed");
                    TestScenePhase::PointLightLifecycle {
                        terrain_revision,
                        baseline,
                        light_id,
                        in_flight,
                        in_flight_source_revision,
                        stage: PointLightTestStage::AwaitBlockerSettled,
                        expected_source_revision,
                        mutation_frame: self.time_info.total_frame_count(),
                    }
                }
                PointLightTestStage::AwaitBlockerSettled => {
                    if self.time_info.total_frame_count() <= mutation_frame {
                        return;
                    }
                    assert_eq!(
                        self.tracer.local_light_live_state(),
                        (Some(expected_source_revision), 8)
                    );
                    let request_serial = self
                        .tracer
                        .request_local_light_visibility_diagnostic(
                            terrain_revision,
                            expected_source_revision,
                            light_id.expect("point-light id must be retained"),
                            POINT_LIGHT_FIXED_RECEIVER_WORLD,
                            POINT_LIGHT_FIXED_RECEIVER_NORMAL,
                            POINT_LIGHT_FIXED_RAY_ORIGIN_OFFSET_WORLD,
                        )
                        .expect("blocked fixed-receiver diagnostic request must succeed");
                    self.environment_lighting_test_scene
                        .as_mut()
                        .expect("point-light test scene must exist")
                        .point_light_fixed_gpu_request_serial = request_serial;
                    TestScenePhase::PointLightLifecycle {
                        terrain_revision,
                        baseline,
                        light_id,
                        in_flight,
                        in_flight_source_revision,
                        stage: PointLightTestStage::AwaitBlockedDiagnostic,
                        expected_source_revision,
                        mutation_frame: self.time_info.total_frame_count(),
                    }
                }
                PointLightTestStage::AwaitBlockedDiagnostic => {
                    let Some(evidence) = self.tracer.local_light_visibility_diagnostic_evidence()
                    else {
                        return;
                    };
                    if evidence.request.request_serial != fixed_gpu_request_serial {
                        return;
                    }
                    assert!(evidence.identity_matches);
                    assert_eq!(evidence.selected_light_index, Some(1));
                    assert_eq!(evidence.request.geometry_revision, terrain_revision);
                    assert_eq!(evidence.request.source_revision, expected_source_revision);
                    assert_eq!(evidence.request.target.light_id(), light_id);
                    assert_eq!(
                        evidence.request.receiver_position,
                        POINT_LIGHT_FIXED_RECEIVER_WORLD
                    );
                    assert_eq!(
                        evidence.request.receiver_normal,
                        POINT_LIGHT_FIXED_RECEIVER_NORMAL
                    );
                    assert_eq!(evidence.candidates, 1);
                    assert_eq!(evidence.visible, 0);
                    assert_eq!(evidence.occluded, 1);
                    assert_eq!(evidence.irradiance_luma_q8, 0);
                    assert_eq!(evidence.irradiance, Vec3::ZERO);
                    let terrain_revision = self
                        .apply_point_light_blocker(VOXEL_TYPE_EMPTY, terrain_revision)
                        .expect("point-light blocker removal must succeed");
                    TestScenePhase::PointLightLifecycle {
                        terrain_revision,
                        baseline,
                        light_id,
                        in_flight,
                        in_flight_source_revision,
                        stage: PointLightTestStage::AwaitRestoreSettled,
                        expected_source_revision,
                        mutation_frame: self.time_info.total_frame_count(),
                    }
                }
                PointLightTestStage::AwaitRestoreSettled => {
                    if self.time_info.total_frame_count() <= mutation_frame {
                        return;
                    }
                    assert_eq!(
                        self.tracer.local_light_live_state(),
                        (Some(expected_source_revision), 8)
                    );
                    let request_serial = self
                        .tracer
                        .request_local_light_visibility_diagnostic(
                            terrain_revision,
                            expected_source_revision,
                            light_id.expect("point-light id must be retained"),
                            POINT_LIGHT_FIXED_RECEIVER_WORLD,
                            POINT_LIGHT_FIXED_RECEIVER_NORMAL,
                            POINT_LIGHT_FIXED_RAY_ORIGIN_OFFSET_WORLD,
                        )
                        .expect("restored fixed-receiver diagnostic request must succeed");
                    self.environment_lighting_test_scene
                        .as_mut()
                        .expect("point-light test scene must exist")
                        .point_light_fixed_gpu_request_serial = request_serial;
                    TestScenePhase::PointLightLifecycle {
                        terrain_revision,
                        baseline,
                        light_id,
                        in_flight,
                        in_flight_source_revision,
                        stage: PointLightTestStage::AwaitRestoredDiagnostic,
                        expected_source_revision,
                        mutation_frame: self.time_info.total_frame_count(),
                    }
                }
                PointLightTestStage::AwaitRestoredDiagnostic => {
                    let Some(evidence) = self.tracer.local_light_visibility_diagnostic_evidence()
                    else {
                        return;
                    };
                    if evidence.request.request_serial != fixed_gpu_request_serial {
                        return;
                    }
                    assert!(evidence.identity_matches);
                    assert_eq!(evidence.selected_light_index, Some(1));
                    assert_eq!(evidence.request.geometry_revision, terrain_revision);
                    assert_eq!(evidence.request.source_revision, expected_source_revision);
                    assert_eq!(evidence.request.target.light_id(), light_id);
                    assert_eq!(
                        evidence.request.receiver_position,
                        POINT_LIGHT_FIXED_RECEIVER_WORLD
                    );
                    assert_eq!(
                        evidence.request.receiver_normal,
                        POINT_LIGHT_FIXED_RECEIVER_NORMAL
                    );
                    assert_eq!(evidence.candidates, 1);
                    assert_eq!(evidence.visible, 1);
                    assert_eq!(evidence.occluded, 0);
                    assert_eq!(
                        evidence.irradiance_luma_q8, fixed_gpu_visible_luma_q8,
                        "removing the blocker must restore the exact fixed-sample energy"
                    );
                    log::info!(
                        "[POINT_LIGHT_ACCEPT] checkpoint=fixed-gpu-apples-to-apples receiver_world={:?} receiver_normal={:?} light_slot={} light_generation={} source_revision={} visible_luma_q8={} blocked_luma_q8=0 restored_luma_q8={} blocker_visible=0 blocker_occluded=1 restored_exact=true",
                        POINT_LIGHT_FIXED_RECEIVER_WORLD,
                        POINT_LIGHT_FIXED_RECEIVER_NORMAL,
                        light_id.unwrap().slot(),
                        light_id.unwrap().generation(),
                        expected_source_revision,
                        fixed_gpu_visible_luma_q8,
                        evidence.irradiance_luma_q8,
                    );
                    let overflow_id = self
                        .environment_lighting_test_scene
                        .as_ref()
                        .and_then(|scene| scene.point_light_diagnostic_overflow_id)
                        .expect("point-light overflow diagnostic id must exist");
                    let request_serial = self
                        .tracer
                        .request_local_light_visibility_diagnostic(
                            terrain_revision,
                            expected_source_revision,
                            overflow_id,
                            POINT_LIGHT_FIXED_RECEIVER_WORLD,
                            POINT_LIGHT_FIXED_RECEIVER_NORMAL,
                            POINT_LIGHT_FIXED_RAY_ORIGIN_OFFSET_WORLD,
                        )
                        .expect("overflow identity diagnostic request must succeed");
                    self.environment_lighting_test_scene
                        .as_mut()
                        .expect("point-light test scene must exist")
                        .point_light_fixed_gpu_request_serial = request_serial;
                    TestScenePhase::PointLightLifecycle {
                        terrain_revision,
                        baseline,
                        light_id,
                        in_flight,
                        in_flight_source_revision,
                        stage: PointLightTestStage::AwaitOverflowDiagnostic,
                        expected_source_revision,
                        mutation_frame,
                    }
                }
                PointLightTestStage::AwaitOverflowDiagnostic => {
                    let Some(evidence) = self.tracer.local_light_visibility_diagnostic_evidence()
                    else {
                        return;
                    };
                    if evidence.request.request_serial != fixed_gpu_request_serial {
                        return;
                    }
                    let (selected_decoy_id, overflow_id, supplemental_ids) = {
                        let scene = self
                            .environment_lighting_test_scene
                            .as_mut()
                            .expect("point-light test scene must exist");
                        (
                            scene
                                .point_light_diagnostic_selected_decoy_id
                                .expect("selected diagnostic decoy must exist"),
                            scene
                                .point_light_diagnostic_overflow_id
                                .expect("overflow diagnostic id must exist"),
                            std::mem::take(&mut scene.point_light_diagnostic_supplemental_ids),
                        )
                    };
                    assert_eq!(evidence.request.target.light_id(), Some(overflow_id));
                    assert_eq!(evidence.request.source_revision, expected_source_revision);
                    assert!(!evidence.identity_matches);
                    assert_eq!(evidence.selected_light_index, None);
                    assert_eq!(evidence.candidates, 0);
                    assert_eq!(evidence.visible, 0);
                    assert_eq!(evidence.occluded, 0);
                    assert_eq!(evidence.irradiance_luma_q8, 0);
                    assert_eq!(evidence.irradiance, Vec3::ZERO);
                    for id in supplemental_ids {
                        self.local_lights
                            .remove(id)
                            .expect("diagnostic supplemental light must remain live until cleanup");
                    }
                    let snapshot = self.local_lights.snapshot();
                    let source_revision = snapshot.source_revision();
                    self.environment_lighting_test_scene
                        .as_mut()
                        .expect("point-light test scene must exist")
                        .point_light_expected_registry_revision = snapshot.registry_revision();
                    log::info!(
                        "[POINT_LIGHT_ACCEPT] checkpoint=n-diagnostic-overflow identity_matches=false selected_index=none overflow_slot={} overflow_generation={} source_revision_before={} cleanup_source_revision={} authoritative_count_after=1 selected_decoy_slot={} selected_decoy_generation={} stale_direct_expected=false",
                        overflow_id.slot(),
                        overflow_id.generation(),
                        expected_source_revision,
                        source_revision,
                        selected_decoy_id.slot(),
                        selected_decoy_id.generation(),
                    );
                    TestScenePhase::PointLightLifecycle {
                        terrain_revision,
                        baseline,
                        light_id,
                        in_flight,
                        in_flight_source_revision,
                        stage: PointLightTestStage::AwaitDiagnosticCleanupLive,
                        expected_source_revision: source_revision,
                        mutation_frame: self.time_info.total_frame_count(),
                    }
                }
                PointLightTestStage::AwaitDiagnosticCleanupLive => {
                    if self.tracer.local_light_live_state() != (Some(expected_source_revision), 1) {
                        return;
                    }
                    assert!(self.time_info.total_frame_count() > mutation_frame);
                    let selected_decoy_id = self
                        .environment_lighting_test_scene
                        .as_ref()
                        .and_then(|scene| scene.point_light_diagnostic_selected_decoy_id)
                        .expect("removed selected diagnostic decoy id must be retained");
                    let request_serial = self
                        .tracer
                        .request_local_light_visibility_diagnostic(
                            terrain_revision,
                            expected_source_revision,
                            selected_decoy_id,
                            POINT_LIGHT_FIXED_RECEIVER_WORLD,
                            POINT_LIGHT_FIXED_RECEIVER_NORMAL,
                            POINT_LIGHT_FIXED_RAY_ORIGIN_OFFSET_WORLD,
                        )
                        .expect("removed identity diagnostic request must succeed");
                    self.environment_lighting_test_scene
                        .as_mut()
                        .expect("point-light test scene must exist")
                        .point_light_fixed_gpu_request_serial = request_serial;
                    TestScenePhase::PointLightLifecycle {
                        terrain_revision,
                        baseline,
                        light_id,
                        in_flight,
                        in_flight_source_revision,
                        stage: PointLightTestStage::AwaitRemovedDiagnostic,
                        expected_source_revision,
                        mutation_frame,
                    }
                }
                PointLightTestStage::AwaitRemovedDiagnostic => {
                    let Some(evidence) = self.tracer.local_light_visibility_diagnostic_evidence()
                    else {
                        return;
                    };
                    if evidence.request.request_serial != fixed_gpu_request_serial {
                        return;
                    }
                    let selected_decoy_id = self
                        .environment_lighting_test_scene
                        .as_ref()
                        .and_then(|scene| scene.point_light_diagnostic_selected_decoy_id)
                        .expect("removed selected diagnostic decoy id must be retained");
                    assert_eq!(evidence.request.target.light_id(), Some(selected_decoy_id));
                    assert_eq!(evidence.request.source_revision, expected_source_revision);
                    assert!(!evidence.identity_matches);
                    assert_eq!(evidence.selected_light_index, None);
                    assert_eq!(evidence.candidates, 0);
                    assert_eq!(evidence.visible, 0);
                    assert_eq!(evidence.occluded, 0);
                    assert_eq!(evidence.irradiance_luma_q8, 0);
                    assert_eq!(evidence.irradiance, Vec3::ZERO);
                    let expected_registry_revision = self
                        .environment_lighting_test_scene
                        .as_ref()
                        .expect("point-light test scene must exist")
                        .point_light_expected_registry_revision;
                    let revisions = self.tracer.local_light_revision_observability();
                    assert_eq!(revisions.0, Some(expected_source_revision));
                    assert_eq!(revisions.1, Some(expected_registry_revision));
                    log::info!(
                        "[POINT_LIGHT_ACCEPT] checkpoint=n-diagnostic-complete target_selected_index=1 target_energy_isolated=true overflow_identity_matches=false removed_selected_identity_matches=false source_revision={} registry_revision={:?} live_gpu_revision={:?} gpu_count=1 authoritative_count=1 mixed_in_flight=false",
                        expected_source_revision,
                        revisions.1,
                        revisions.2,
                    );
                    TestScenePhase::PointLightLifecycle {
                        terrain_revision,
                        baseline,
                        light_id,
                        in_flight,
                        in_flight_source_revision,
                        stage: PointLightTestStage::AwaitPointOnDdgiPublication,
                        expected_source_revision,
                        mutation_frame,
                    }
                }
                PointLightTestStage::AwaitPointOnDdgiPublication => {
                    let baseline = baseline.expect("point-light baseline must be retained");
                    let id = light_id.expect("point-light id must be retained");
                    let Some(transport) = self.tracer.ddgi_live_radiance_snapshot() else {
                        return;
                    };
                    if transport.local_lights.source_revision() != expected_source_revision
                        || transport.local_lights.count() != 1
                    {
                        return;
                    }
                    let status = self.tracer.ddgi_runtime_status();
                    if status.staging().is_some() {
                        return;
                    }
                    let active = status.active();
                    let Some(point_on_field) = active.published_field else {
                        return;
                    };
                    if point_on_field.field().radiance_revision()
                        != transport.local_lights.info.transport_revision
                        || point_on_field.field().geometry_revision() != terrain_revision
                        || !is_converged_field(point_on_field)
                        || active.stage != DdgiVolumeStage::Ready
                        || active.building_field.is_some()
                    {
                        return;
                    }
                    let builder = self
                        .tracer
                        .ddgi_builder_radiance_snapshot()
                        .expect("point-light DDGI builder must latch an immutable snapshot");
                    assert_eq!(
                        builder.local_lights.source_revision(),
                        expected_source_revision
                    );
                    assert_eq!(builder.local_lights.count(), 1);
                    assert_eq!(
                        builder.local_lights.info.transport_revision,
                        point_on_field.field().radiance_revision()
                    );
                    assert_eq!(active.relocated_terrain_revision, Some(terrain_revision));
                    let Some(gpu_evidence) = self.tracer.ddgi_local_light_gpu_evidence() else {
                        return;
                    };
                    if !gpu_evidence.matches_classified_field(point_on_field)
                        || gpu_evidence.local_source_revision != expected_source_revision
                        || gpu_evidence.local_light_count != 1
                    {
                        return;
                    }
                    assert!(gpu_evidence.is_complete());
                    assert!(gpu_evidence.totals.visible > 0);
                    assert!(gpu_evidence.totals.irradiance_luma_q8 > 0);
                    assert!(gpu_evidence.emissive_surface_hits > 0);
                    assert!(gpu_evidence.emissive_surface_radiance_luma_q8 > 0);
                    let diagnostics = self.tracer.ddgi_lighting_diagnostics();
                    assert!(!diagnostics.has_mixed_in_flight_revision);
                    assert_eq!(diagnostics.in_flight_revision, None);
                    self.local_lights
                        .update(
                            id,
                            LocalLight::Point(
                                PointLight::new(
                                    POINT_LIGHT_MOVED_POSITION,
                                    Vec3::new(1.0, 0.45, 0.20),
                                    0.08,
                                    POINT_LIGHT_SOURCE_RADIUS_WORLD,
                                    POINT_LIGHT_RANGE_WORLD,
                                )
                                .expect("point-light test move must be valid"),
                            ),
                        )
                        .expect("point-light test id must stay live during move");
                    let source_revision = self.local_lights.snapshot().source_revision();
                    let frame = self.time_info.total_frame_count();
                    log::info!(
                        "[POINT_LIGHT_ACCEPT] checkpoint=point-on-ddgi-gpu positive=true emissive_surface_hit=true source_revision={} geometry_revision={} field_serial={} probes={} candidates={} visible={} occluded={} irradiance_luma_q8={} emissive_surface_hits={} emissive_surface_radiance_luma_q8={} action=move frame={} next_source_revision={} from_world={:?} to_world={:?} mixed_in_flight=false",
                        expected_source_revision,
                        terrain_revision,
                        point_on_field.field().serial(),
                        gpu_evidence.sampled_probe_count,
                        gpu_evidence.totals.candidates,
                        gpu_evidence.totals.visible,
                        gpu_evidence.totals.occluded,
                        gpu_evidence.totals.irradiance_luma_q8,
                        gpu_evidence.emissive_surface_hits,
                        gpu_evidence.emissive_surface_radiance_luma_q8,
                        frame,
                        source_revision,
                        POINT_LIGHT_ADD_POSITION,
                        POINT_LIGHT_MOVED_POSITION,
                    );
                    TestScenePhase::PointLightLifecycle {
                        terrain_revision,
                        baseline: Some(baseline),
                        light_id: Some(id),
                        in_flight: None,
                        in_flight_source_revision: 0,
                        stage: PointLightTestStage::AwaitMoveLive,
                        expected_source_revision: source_revision,
                        mutation_frame: frame,
                    }
                }
                PointLightTestStage::AwaitMoveLive => {
                    let (live_revision, live_count) = self.tracer.local_light_live_state();
                    if live_revision != Some(expected_source_revision) {
                        return;
                    }
                    assert_eq!(live_count, 1);
                    assert!(self.time_info.total_frame_count() > mutation_frame);
                    assert!(
                        !self
                            .tracer
                            .ddgi_lighting_diagnostics()
                            .has_mixed_in_flight_revision
                    );
                    log::info!(
                        "[POINT_LIGHT_ACCEPT] checkpoint=move-live direct_immediate=true source_revision={} live_count=1 mutation_frame={} observed_frame={} mixed_in_flight=false",
                        expected_source_revision,
                        mutation_frame,
                        self.time_info.total_frame_count(),
                    );
                    TestScenePhase::PointLightLifecycle {
                        terrain_revision,
                        baseline,
                        light_id,
                        in_flight,
                        in_flight_source_revision,
                        stage: PointLightTestStage::AwaitMoveMidflight,
                        expected_source_revision,
                        mutation_frame,
                    }
                }
                PointLightTestStage::AwaitMoveMidflight => {
                    let status = self.tracer.ddgi_runtime_status();
                    assert!(status.staging().is_none());
                    let active = status.active();
                    let Some(in_flight) = active.building_field else {
                        return;
                    };
                    if in_flight.field().geometry_revision() != terrain_revision {
                        return;
                    }
                    let work = active
                        .target_work
                        .expect("moving point-light build must retain scheduled work");
                    assert_eq!(work.kind(), DdgiScheduledWorkKind::RadianceUpdate);
                    assert_eq!(work.destination(), in_flight);
                    let probe_priority = active
                        .probe_priority
                        .expect("moving point light must prioritize its impact bound");
                    assert_eq!(
                        probe_priority.reason(),
                        DdgiProbePriorityReason::LightingImpact
                    );
                    let remaining = active
                        .grid
                        .probe_count()
                        .saturating_sub(active.filtered_probe_count);
                    if active.filtered_probe_count == 0 || remaining <= 3 * DDGI_PROBE_BATCH_SIZE {
                        return;
                    }
                    let builder = self
                        .tracer
                        .ddgi_builder_radiance_snapshot()
                        .expect("moving point light must latch an immutable DDGI snapshot");
                    assert_eq!(
                        builder.local_lights.source_revision(),
                        expected_source_revision
                    );
                    assert_eq!(builder.local_lights.count(), 1);
                    assert_eq!(
                        builder.local_lights.info.transport_revision,
                        in_flight.field().radiance_revision()
                    );
                    let diagnostics = self.tracer.ddgi_lighting_diagnostics();
                    assert!(!diagnostics.has_mixed_in_flight_revision);
                    assert_eq!(
                        diagnostics.in_flight_revision,
                        Some(in_flight.field().radiance_revision())
                    );
                    let id = light_id.expect("point-light id must survive move");
                    self.local_lights
                        .update(
                            id,
                            LocalLight::Point(
                                PointLight::new(
                                    POINT_LIGHT_MOVED_POSITION,
                                    Vec3::new(0.20, 0.55, 1.0),
                                    0.14,
                                    POINT_LIGHT_SOURCE_RADIUS_WORLD,
                                    POINT_LIGHT_RANGE_WORLD,
                                )
                                .expect("point-light photometric update must be valid"),
                            ),
                        )
                        .expect("point-light id must stay live during photometric update");
                    let source_revision = self.local_lights.snapshot().source_revision();
                    let frame = self.time_info.total_frame_count();
                    log::info!(
                        "[POINT_LIGHT_ACCEPT] checkpoint=move-midflight action=update-color-intensity frame={} source_revision={} color={:?} intensity={} ddgi_in_flight_revision={} ddgi_frozen_source_revision={} progress={}/{} priority_reason={:?} priority_voxel_min={:?} priority_voxel_max={:?} full_sweep_probes={} mixed_in_flight=false",
                        frame,
                        source_revision,
                        Vec3::new(0.20, 0.55, 1.0),
                        0.14,
                        in_flight.field().radiance_revision(),
                        expected_source_revision,
                        active.filtered_probe_count,
                        active.grid.probe_count(),
                        probe_priority.reason(),
                        probe_priority.voxel_bound().min(),
                        probe_priority.voxel_bound().max(),
                        active.grid.probe_count(),
                    );
                    TestScenePhase::PointLightLifecycle {
                        terrain_revision,
                        baseline,
                        light_id,
                        in_flight: Some(in_flight),
                        in_flight_source_revision: expected_source_revision,
                        stage: PointLightTestStage::AwaitPhotometricUpdateLive,
                        expected_source_revision: source_revision,
                        mutation_frame: frame,
                    }
                }
                PointLightTestStage::AwaitPhotometricUpdateLive => {
                    let (live_revision, live_count) = self.tracer.local_light_live_state();
                    if live_revision != Some(expected_source_revision) {
                        return;
                    }
                    assert_eq!(live_count, 1);
                    assert!(self.time_info.total_frame_count() > mutation_frame);
                    assert!(
                        !self
                            .tracer
                            .ddgi_lighting_diagnostics()
                            .has_mixed_in_flight_revision
                    );
                    let id = light_id.expect("point-light id must survive photometric update");
                    self.local_lights
                        .remove(id)
                        .expect("point-light removal must use the current stable id");
                    let source_revision = self.local_lights.snapshot().source_revision();
                    let frame = self.time_info.total_frame_count();
                    log::info!(
                        "[POINT_LIGHT_ACCEPT] checkpoint=photometric-update-live direct_immediate=true action=remove frame={} source_revision={} live_count_before_remove={} ddgi_frozen_source_revision={} mixed_in_flight=false",
                        frame,
                        source_revision,
                        live_count,
                        in_flight_source_revision,
                    );
                    TestScenePhase::PointLightLifecycle {
                        terrain_revision,
                        baseline,
                        light_id,
                        in_flight,
                        in_flight_source_revision,
                        stage: PointLightTestStage::AwaitRemoveLive,
                        expected_source_revision: source_revision,
                        mutation_frame: frame,
                    }
                }
                PointLightTestStage::AwaitRemoveLive => {
                    let (live_revision, live_count) = self.tracer.local_light_live_state();
                    if live_revision != Some(expected_source_revision) {
                        return;
                    }
                    assert_eq!(live_count, 0);
                    assert!(self.time_info.total_frame_count() > mutation_frame);
                    let diagnostics = self.tracer.ddgi_lighting_diagnostics();
                    assert!(!diagnostics.has_mixed_in_flight_revision);
                    let (revision_lag, coalesced) =
                        self.tracer.local_light_transport_observability();
                    log::info!(
                        "[POINT_LIGHT_ACCEPT] checkpoint=remove-live direct_immediate=true stale_direct=false source_revision={} live_count=0 transport_revision_lag={} coalesced_live_revisions={} mixed_in_flight=false",
                        expected_source_revision,
                        revision_lag,
                        coalesced,
                    );
                    TestScenePhase::PointLightLifecycle {
                        terrain_revision,
                        baseline,
                        light_id,
                        in_flight,
                        in_flight_source_revision,
                        stage: PointLightTestStage::AwaitFinalPublication,
                        expected_source_revision,
                        mutation_frame,
                    }
                }
                PointLightTestStage::AwaitFinalPublication => {
                    let Some(transport) = self.tracer.ddgi_live_radiance_snapshot() else {
                        return;
                    };
                    if transport.local_lights.source_revision() != expected_source_revision
                        || transport.local_lights.count() != 0
                    {
                        return;
                    }
                    let status = self.tracer.ddgi_runtime_status();
                    let active = status.active();
                    let Some(final_field) = active.published_field else {
                        return;
                    };
                    if final_field.field().radiance_revision()
                        != transport.local_lights.info.transport_revision
                        || !is_converged_field(final_field)
                        || active.stage != DdgiVolumeStage::Ready
                        || active.building_field.is_some()
                    {
                        return;
                    }
                    let baseline = baseline.expect("point-light baseline must be retained");
                    assert_ne!(
                        baseline.field().geometry_revision(),
                        terrain_revision,
                        "blocker add/remove must have advanced geometry independently"
                    );
                    assert_eq!(
                        final_field.field().geometry_revision(),
                        terrain_revision,
                        "light-only move/update/remove must retain restored geometry"
                    );
                    assert_eq!(active.relocated_terrain_revision, Some(terrain_revision));
                    let builder = self
                        .tracer
                        .ddgi_builder_radiance_snapshot()
                        .expect("final DDGI field must retain its immutable snapshot");
                    assert_eq!(
                        builder.local_lights.source_revision(),
                        expected_source_revision
                    );
                    assert_eq!(builder.local_lights.count(), 0);
                    assert_eq!(
                        builder.local_lights.info.transport_revision,
                        final_field.field().radiance_revision()
                    );
                    let diagnostics = self.tracer.ddgi_lighting_diagnostics();
                    assert!(!diagnostics.has_mixed_in_flight_revision);
                    assert_eq!(diagnostics.in_flight_revision, None);
                    assert_eq!(
                        diagnostics.scheduler_published_revision,
                        Some(final_field.field().radiance_revision())
                    );
                    let Some(gpu_evidence) = self.tracer.ddgi_local_light_gpu_evidence() else {
                        return;
                    };
                    if !gpu_evidence.matches_classified_field(final_field)
                        || gpu_evidence.local_source_revision != expected_source_revision
                        || gpu_evidence.local_light_count != 0
                    {
                        return;
                    }
                    assert!(gpu_evidence.is_complete());
                    assert_eq!(gpu_evidence.totals.candidates, 0);
                    assert_eq!(gpu_evidence.totals.visible, 0);
                    assert_eq!(gpu_evidence.totals.occluded, 0);
                    assert_eq!(gpu_evidence.totals.irradiance_luma_q8, 0);
                    let atlas = active
                        .last_atlas_validation
                        .expect("final point-light field must have atlas validation evidence");
                    assert_eq!(atlas.non_finite_count, 0);
                    assert!(
                        atlas.max_rgb_value > 0.0,
                        "final DDGI atlas must not be black"
                    );
                    let (revision_lag, coalesced) =
                        self.tracer.local_light_transport_observability();
                    assert_eq!(revision_lag, 0);
                    assert!(coalesced >= 1);
                    log_acceptance_field("POINT_LIGHT", "complete", final_field);
                    log::info!(
                        "[POINT_LIGHT_ACCEPT] complete latest_wins=true direct_removed=true stale_contribution=false mixed_in_flight=false ddgi_off_candidates=0 ddgi_off_visible=0 ddgi_off_occluded=0 ddgi_off_luma_q8=0 atlas_nonblack=true atlas_max_rgb={:.8} local_source_revision={} transport_revision={} revision_lag={} coalesced_live_revisions={} geometry_revision={} visibility_relocation_revision={} terrain_direct=true raster_direct=true ddgi_injection=true shared_units=world shared_visibility=exact-voxel-segment",
                        atlas.max_rgb_value,
                        expected_source_revision,
                        final_field.field().radiance_revision(),
                        revision_lag,
                        coalesced,
                        final_field.field().geometry_revision(),
                        active.relocated_terrain_revision.unwrap_or_default(),
                    );
                    TestScenePhase::Ready
                }
            },
            TestScenePhase::VoxelEmissiveLifecycle(mut state) => match state.stage {
                VoxelEmissiveTestStage::AwaitBaseline => {
                    let status = self.tracer.ddgi_runtime_status();
                    let active = status.active();
                    let Some(baseline) = active.published_field else {
                        return;
                    };
                    if !is_converged_field(baseline)
                        || active.stage != DdgiVolumeStage::Ready
                        || active.building_field.is_some()
                        || status.staging().is_some()
                    {
                        return;
                    }
                    let snapshot = self.local_lights.snapshot();
                    assert_eq!(voxel_emissive_source_count(&snapshot), 0);
                    assert!(voxel_emissive_record(&snapshot, VOXEL_EMISSIVE_PRIMARY_MIN).is_none());
                    state.baseline = Some(baseline);
                    state.expected_source_revision = snapshot.source_revision();
                    state.expected_registry_revision = snapshot.registry_revision();
                    state.terrain_revision = self
                        .apply_voxel_emissive_edits(
                            "add-primary",
                            &[(
                                VOXEL_EMISSIVE_PRIMARY_MIN,
                                VOXEL_EMISSIVE_PRIMARY_MAX,
                                VOXEL_TYPE_EMISSIVE,
                            )],
                            state.terrain_revision,
                        )
                        .expect("primary emissive voxel edit must succeed");
                    state.mutation_frame = self.time_info.total_frame_count();
                    state.stage = VoxelEmissiveTestStage::AwaitAddRegistry;
                    TestScenePhase::VoxelEmissiveLifecycle(state)
                }
                VoxelEmissiveTestStage::AwaitAddRegistry => {
                    let snapshot = self.local_lights.snapshot();
                    let Some(record) = voxel_emissive_record(&snapshot, VOXEL_EMISSIVE_PRIMARY_MIN)
                    else {
                        return;
                    };
                    if snapshot.source_revision() == state.expected_source_revision {
                        return;
                    }
                    assert_eq!(voxel_emissive_source_count(&snapshot), 1);
                    assert!(snapshot.registry_revision() > state.expected_registry_revision);
                    let LocalLight::Point(point) = record.light() else {
                        panic!("voxel provider must publish a point aggregate")
                    };
                    let expected_position = (VOXEL_EMISSIVE_PRIMARY_MIN.as_vec3()
                        + VOXEL_EMISSIVE_PRIMARY_MAX.as_vec3())
                        * (0.5 / VOXELS_PER_WORLD_UNIT);
                    assert!(point.position.abs_diff_eq(expected_position, 1.0e-6));
                    let one_voxel_intensity = expected_emissive_voxel_intensity(1);
                    let emitter_voxels = (point.intensity / one_voxel_intensity).round() as u32;
                    assert!(emitter_voxels > 1);
                    assert!(
                        emitter_voxels
                            <= volume_voxels(
                                VOXEL_EMISSIVE_PRIMARY_MIN,
                                VOXEL_EMISSIVE_PRIMARY_MAX,
                            )
                    );
                    assert!(
                        (point.intensity - expected_emissive_voxel_intensity(emitter_voxels)).abs()
                            <= 1.0e-6
                    );
                    let source_half_extent =
                        (VOXEL_EMISSIVE_PRIMARY_MAX - VOXEL_EMISSIVE_PRIMARY_MIN).as_vec3()
                            * (0.5 / VOXELS_PER_WORLD_UNIT);
                    assert!(point.source_radius + 1.0e-6 >= source_half_extent.length());
                    assert!(point.range >= 0.35);
                    state.light_id = Some(record.id());
                    state.primary_intensity_bits = point.intensity.to_bits();
                    state.expected_source_revision = snapshot.source_revision();
                    state.expected_registry_revision = snapshot.registry_revision();
                    state.stage = VoxelEmissiveTestStage::AwaitAddLive;
                    log::info!(
                        "[VOXEL_EMISSIVE_ACCEPT] checkpoint=add-registry provider=voxel source_count=1 light_slot={} light_generation={} source_revision={} registry_revision={} emitter_voxels={} intensity={} position_world={:?} source_radius_world={} range_world={} edit_to_registry_frames={}",
                        record.id().slot(),
                        record.id().generation(),
                        state.expected_source_revision,
                        state.expected_registry_revision,
                        emitter_voxels,
                        point.intensity,
                        point.position,
                        point.source_radius,
                        point.range,
                        self.time_info.total_frame_count().saturating_sub(state.mutation_frame),
                    );
                    TestScenePhase::VoxelEmissiveLifecycle(state)
                }
                VoxelEmissiveTestStage::AwaitAddLive => {
                    if self.tracer.local_light_live_state()
                        != (Some(state.expected_source_revision), 1)
                    {
                        return;
                    }
                    assert_eq!(
                        self.tracer.local_light_revision_observability().1,
                        Some(state.expected_registry_revision)
                    );
                    assert!(
                        !self
                            .tracer
                            .ddgi_lighting_diagnostics()
                            .has_mixed_in_flight_revision
                    );
                    let request_serial = self
                        .tracer
                        .request_local_light_visibility_diagnostic(
                            state.terrain_revision,
                            state.expected_source_revision,
                            state.light_id.expect("voxel aggregate id must exist"),
                            POINT_LIGHT_FIXED_RECEIVER_WORLD,
                            POINT_LIGHT_FIXED_RECEIVER_NORMAL,
                            POINT_LIGHT_FIXED_RAY_ORIGIN_OFFSET_WORLD,
                        )
                        .expect("voxel emitter visibility diagnostic request must succeed");
                    self.environment_lighting_test_scene
                        .as_mut()
                        .expect("voxel-emissive test scene must exist")
                        .point_light_fixed_gpu_request_serial = request_serial;
                    state.stage = VoxelEmissiveTestStage::AwaitVisibleDiagnostic;
                    TestScenePhase::VoxelEmissiveLifecycle(state)
                }
                VoxelEmissiveTestStage::AwaitVisibleDiagnostic => {
                    let Some(evidence) = self.tracer.local_light_visibility_diagnostic_evidence()
                    else {
                        return;
                    };
                    if evidence.request.request_serial != fixed_gpu_request_serial {
                        return;
                    }
                    assert!(evidence.identity_matches);
                    assert_eq!(evidence.selected_light_index, Some(0));
                    assert_eq!(evidence.request.geometry_revision, state.terrain_revision);
                    assert_eq!(
                        evidence.request.source_revision,
                        state.expected_source_revision
                    );
                    assert_eq!(evidence.request.target.light_id(), state.light_id);
                    assert_eq!(
                        (evidence.candidates, evidence.visible, evidence.occluded),
                        (1, 1, 0)
                    );
                    assert!(evidence.irradiance_luma_q8 > 0);
                    state.visible_luma_q8 = evidence.irradiance_luma_q8;
                    log::info!(
                        "[VOXEL_EMISSIVE_ACCEPT] checkpoint=emitter-endpoint-visible self_occluded=false candidates=1 visible=1 occluded=0 luma_q8={} light_slot={} light_generation={} source_radius_endpoint_volume=true",
                        state.visible_luma_q8,
                        state.light_id.unwrap().slot(),
                        state.light_id.unwrap().generation(),
                    );
                    state.terrain_revision = self
                        .apply_point_light_blocker(VOXEL_TYPE_ROCK, state.terrain_revision)
                        .expect("voxel-emitter blocker add must succeed");
                    state.mutation_frame = self.time_info.total_frame_count();
                    state.stage = VoxelEmissiveTestStage::AwaitBlockerSettled;
                    TestScenePhase::VoxelEmissiveLifecycle(state)
                }
                VoxelEmissiveTestStage::AwaitBlockerSettled => {
                    if self.time_info.total_frame_count() <= state.mutation_frame {
                        return;
                    }
                    let request_serial = self
                        .tracer
                        .request_local_light_visibility_diagnostic(
                            state.terrain_revision,
                            state.expected_source_revision,
                            state.light_id.unwrap(),
                            POINT_LIGHT_FIXED_RECEIVER_WORLD,
                            POINT_LIGHT_FIXED_RECEIVER_NORMAL,
                            POINT_LIGHT_FIXED_RAY_ORIGIN_OFFSET_WORLD,
                        )
                        .expect("blocked voxel-emitter diagnostic request must succeed");
                    self.environment_lighting_test_scene
                        .as_mut()
                        .unwrap()
                        .point_light_fixed_gpu_request_serial = request_serial;
                    state.stage = VoxelEmissiveTestStage::AwaitBlockedDiagnostic;
                    TestScenePhase::VoxelEmissiveLifecycle(state)
                }
                VoxelEmissiveTestStage::AwaitBlockedDiagnostic => {
                    let Some(evidence) = self.tracer.local_light_visibility_diagnostic_evidence()
                    else {
                        return;
                    };
                    if evidence.request.request_serial != fixed_gpu_request_serial {
                        return;
                    }
                    assert!(evidence.identity_matches);
                    assert_eq!(
                        (evidence.candidates, evidence.visible, evidence.occluded),
                        (1, 0, 1)
                    );
                    assert_eq!(evidence.irradiance_luma_q8, 0);
                    log::info!(
                        "[VOXEL_EMISSIVE_ACCEPT] checkpoint=independent-blocker candidates=1 visible=0 occluded=1 luma_q8=0 same_receiver=true same_light_id=true"
                    );
                    state.terrain_revision = self
                        .apply_point_light_blocker(VOXEL_TYPE_EMPTY, state.terrain_revision)
                        .expect("voxel-emitter blocker removal must succeed");
                    state.mutation_frame = self.time_info.total_frame_count();
                    state.stage = VoxelEmissiveTestStage::AwaitRestoreSettled;
                    TestScenePhase::VoxelEmissiveLifecycle(state)
                }
                VoxelEmissiveTestStage::AwaitRestoreSettled => {
                    if self.time_info.total_frame_count() <= state.mutation_frame {
                        return;
                    }
                    let request_serial = self
                        .tracer
                        .request_local_light_visibility_diagnostic(
                            state.terrain_revision,
                            state.expected_source_revision,
                            state.light_id.unwrap(),
                            POINT_LIGHT_FIXED_RECEIVER_WORLD,
                            POINT_LIGHT_FIXED_RECEIVER_NORMAL,
                            POINT_LIGHT_FIXED_RAY_ORIGIN_OFFSET_WORLD,
                        )
                        .expect("restored voxel-emitter diagnostic request must succeed");
                    self.environment_lighting_test_scene
                        .as_mut()
                        .unwrap()
                        .point_light_fixed_gpu_request_serial = request_serial;
                    state.stage = VoxelEmissiveTestStage::AwaitRestoredDiagnostic;
                    TestScenePhase::VoxelEmissiveLifecycle(state)
                }
                VoxelEmissiveTestStage::AwaitRestoredDiagnostic => {
                    let Some(evidence) = self.tracer.local_light_visibility_diagnostic_evidence()
                    else {
                        return;
                    };
                    if evidence.request.request_serial != fixed_gpu_request_serial {
                        return;
                    }
                    assert!(evidence.identity_matches);
                    assert_eq!(
                        (evidence.candidates, evidence.visible, evidence.occluded),
                        (1, 1, 0)
                    );
                    assert_eq!(evidence.irradiance_luma_q8, state.visible_luma_q8);
                    let old_source_revision = state.expected_source_revision;
                    let old_registry_revision = state.expected_registry_revision;
                    state.terrain_revision = self
                        .apply_voxel_emissive_edits(
                            "add-secondary-same-cluster",
                            &[(
                                VOXEL_EMISSIVE_SECONDARY_MIN,
                                VOXEL_EMISSIVE_SECONDARY_MAX,
                                VOXEL_TYPE_EMISSIVE,
                            )],
                            state.terrain_revision,
                        )
                        .expect("secondary emissive voxel edit must succeed");
                    state.expected_source_revision = old_source_revision;
                    state.expected_registry_revision = old_registry_revision;
                    state.mutation_frame = self.time_info.total_frame_count();
                    state.stage = VoxelEmissiveTestStage::AwaitAggregateRegistry;
                    TestScenePhase::VoxelEmissiveLifecycle(state)
                }
                VoxelEmissiveTestStage::AwaitAggregateRegistry => {
                    let snapshot = self.local_lights.snapshot();
                    let Some(record) = voxel_emissive_record(&snapshot, VOXEL_EMISSIVE_PRIMARY_MIN)
                    else {
                        return;
                    };
                    if snapshot.source_revision() == state.expected_source_revision {
                        return;
                    }
                    assert_eq!(voxel_emissive_source_count(&snapshot), 1);
                    assert_eq!(record.id(), state.light_id.unwrap());
                    assert!(snapshot.registry_revision() > state.expected_registry_revision);
                    let LocalLight::Point(point) = record.light() else {
                        panic!("voxel provider must publish point aggregates")
                    };
                    let primary_intensity = f32::from_bits(state.primary_intensity_bits);
                    assert!(point.intensity > primary_intensity);
                    let one_voxel_intensity = expected_emissive_voxel_intensity(1);
                    let combined_count = (point.intensity / one_voxel_intensity).round() as u32;
                    assert!(combined_count > 1);
                    assert!(
                        combined_count
                            <= volume_voxels(
                                VOXEL_EMISSIVE_PRIMARY_MIN,
                                VOXEL_EMISSIVE_SECONDARY_MAX,
                            )
                    );
                    assert!(
                        (point.intensity - expected_emissive_voxel_intensity(combined_count)).abs()
                            <= 1.0e-6
                    );
                    let combined_half_extent =
                        (VOXEL_EMISSIVE_SECONDARY_MAX - VOXEL_EMISSIVE_PRIMARY_MIN).as_vec3()
                            * (0.5 / VOXELS_PER_WORLD_UNIT);
                    assert!(point.source_radius + 1.0e-6 >= combined_half_extent.length());
                    state.expected_source_revision = snapshot.source_revision();
                    state.expected_registry_revision = snapshot.registry_revision();
                    state.stage = VoxelEmissiveTestStage::AwaitAggregateLive;
                    log::info!(
                        "[VOXEL_EMISSIVE_ACCEPT] checkpoint=aggregate-update stable_identity=true light_slot={} light_generation={} emitter_voxels={} intensity={} source_radius_world={} source_revision={} registry_revision={} edit_to_registry_frames={}",
                        record.id().slot(),
                        record.id().generation(),
                        combined_count,
                        point.intensity,
                        point.source_radius,
                        state.expected_source_revision,
                        state.expected_registry_revision,
                        self.time_info.total_frame_count().saturating_sub(state.mutation_frame),
                    );
                    TestScenePhase::VoxelEmissiveLifecycle(state)
                }
                VoxelEmissiveTestStage::AwaitAggregateLive => {
                    if self.tracer.local_light_live_state()
                        != (Some(state.expected_source_revision), 1)
                    {
                        return;
                    }
                    let request_serial = self
                        .tracer
                        .request_local_light_visibility_diagnostic(
                            state.terrain_revision,
                            state.expected_source_revision,
                            state.light_id.unwrap(),
                            POINT_LIGHT_FIXED_RECEIVER_WORLD,
                            POINT_LIGHT_FIXED_RECEIVER_NORMAL,
                            POINT_LIGHT_FIXED_RAY_ORIGIN_OFFSET_WORLD,
                        )
                        .expect("updated aggregate diagnostic request must succeed");
                    self.environment_lighting_test_scene
                        .as_mut()
                        .unwrap()
                        .point_light_fixed_gpu_request_serial = request_serial;
                    state.stage = VoxelEmissiveTestStage::AwaitAggregateDiagnostic;
                    TestScenePhase::VoxelEmissiveLifecycle(state)
                }
                VoxelEmissiveTestStage::AwaitAggregateDiagnostic => {
                    let Some(evidence) = self.tracer.local_light_visibility_diagnostic_evidence()
                    else {
                        return;
                    };
                    if evidence.request.request_serial != fixed_gpu_request_serial {
                        return;
                    }
                    assert!(evidence.identity_matches);
                    assert_eq!(
                        (evidence.candidates, evidence.visible, evidence.occluded),
                        (1, 1, 0)
                    );
                    assert!(evidence.irradiance_luma_q8 > state.visible_luma_q8);
                    let before_source_revision = state.expected_source_revision;
                    let before_registry_revision = state.expected_registry_revision;
                    state.terrain_revision = self
                        .apply_voxel_emissive_edits(
                            "move-cluster",
                            &[
                                (
                                    VOXEL_EMISSIVE_PRIMARY_MIN,
                                    VOXEL_EMISSIVE_SECONDARY_MAX,
                                    VOXEL_TYPE_EMPTY,
                                ),
                                (
                                    VOXEL_EMISSIVE_MOVED_MIN,
                                    VOXEL_EMISSIVE_MOVED_MAX,
                                    VOXEL_TYPE_EMISSIVE,
                                ),
                            ],
                            state.terrain_revision,
                        )
                        .expect("emissive voxel move edit must succeed");
                    state.expected_source_revision = before_source_revision;
                    state.expected_registry_revision = before_registry_revision;
                    state.mutation_frame = self.time_info.total_frame_count();
                    state.stage = VoxelEmissiveTestStage::AwaitMoveRegistry;
                    TestScenePhase::VoxelEmissiveLifecycle(state)
                }
                VoxelEmissiveTestStage::AwaitMoveRegistry => {
                    let snapshot = self.local_lights.snapshot();
                    let Some(record) = voxel_emissive_record(&snapshot, VOXEL_EMISSIVE_MOVED_MIN)
                    else {
                        return;
                    };
                    if voxel_emissive_record(&snapshot, VOXEL_EMISSIVE_PRIMARY_MIN).is_some()
                        || snapshot.source_revision() == state.expected_source_revision
                    {
                        return;
                    }
                    assert_eq!(voxel_emissive_source_count(&snapshot), 1);
                    let old_id = state
                        .light_id
                        .expect("old aggregate identity must be retained");
                    assert_ne!(record.id(), old_id);
                    assert!(snapshot.lights().iter().all(|entry| entry.id() != old_id));
                    assert!(snapshot.registry_revision() > state.expected_registry_revision);
                    let LocalLight::Point(point) = record.light() else {
                        panic!("moved voxel aggregate must be a point")
                    };
                    let moved_count =
                        (point.intensity / expected_emissive_voxel_intensity(1)).round() as u32;
                    assert!(moved_count > 1);
                    assert!(
                        moved_count
                            <= volume_voxels(VOXEL_EMISSIVE_MOVED_MIN, VOXEL_EMISSIVE_MOVED_MAX,)
                    );
                    assert!(
                        (point.intensity - expected_emissive_voxel_intensity(moved_count)).abs()
                            <= 1.0e-6
                    );
                    state.light_id = Some(record.id());
                    state.expected_source_revision = snapshot.source_revision();
                    state.expected_registry_revision = snapshot.registry_revision();
                    state.stage = VoxelEmissiveTestStage::AwaitMoveLive;
                    log::info!(
                        "[VOXEL_EMISSIVE_ACCEPT] checkpoint=move-registry old_slot={} old_generation={} new_slot={} new_generation={} stale_old=false provider_source_count=1 source_revision={} registry_revision={} edit_to_registry_frames={}",
                        old_id.slot(),
                        old_id.generation(),
                        record.id().slot(),
                        record.id().generation(),
                        state.expected_source_revision,
                        state.expected_registry_revision,
                        self.time_info.total_frame_count().saturating_sub(state.mutation_frame),
                    );
                    TestScenePhase::VoxelEmissiveLifecycle(state)
                }
                VoxelEmissiveTestStage::AwaitMoveLive => {
                    if self.tracer.local_light_live_state()
                        != (Some(state.expected_source_revision), 1)
                    {
                        return;
                    }
                    assert_eq!(
                        self.tracer.local_light_revision_observability().1,
                        Some(state.expected_registry_revision)
                    );
                    assert!(
                        !self
                            .tracer
                            .ddgi_lighting_diagnostics()
                            .has_mixed_in_flight_revision
                    );
                    state.stage = VoxelEmissiveTestStage::AwaitMovedDdgiPublication;
                    TestScenePhase::VoxelEmissiveLifecycle(state)
                }
                VoxelEmissiveTestStage::AwaitMovedDdgiPublication => {
                    let Some(transport) = self.tracer.ddgi_live_radiance_snapshot() else {
                        return;
                    };
                    if transport.local_lights.source_revision() != state.expected_source_revision
                        || transport.local_lights.count() != 1
                    {
                        return;
                    }
                    let status = self.tracer.ddgi_runtime_status();
                    if status.staging().is_some() {
                        return;
                    }
                    let active = status.active();
                    let Some(field) = active.published_field else {
                        return;
                    };
                    if field.field().radiance_revision()
                        != transport.local_lights.info.transport_revision
                        || field.field().geometry_revision() != state.terrain_revision
                        || !is_converged_field(field)
                        || active.stage != DdgiVolumeStage::Ready
                        || active.building_field.is_some()
                    {
                        return;
                    }
                    let builder = self
                        .tracer
                        .ddgi_builder_radiance_snapshot()
                        .expect("voxel DDGI builder must retain immutable local lights");
                    assert_eq!(
                        builder.local_lights.source_revision(),
                        state.expected_source_revision
                    );
                    assert_eq!(builder.local_lights.count(), 1);
                    let Some(gpu) = self.tracer.ddgi_local_light_gpu_evidence() else {
                        return;
                    };
                    if !gpu.matches_classified_field(field)
                        || gpu.local_source_revision != state.expected_source_revision
                        || gpu.local_light_count != 1
                    {
                        return;
                    }
                    assert!(gpu.is_complete());
                    assert!(gpu.totals.visible > 0);
                    assert!(gpu.totals.irradiance_luma_q8 > 0);
                    assert!(gpu.emissive_surface_hits > 0);
                    assert!(gpu.emissive_surface_radiance_luma_q8 > 0);
                    assert!(
                        !self
                            .tracer
                            .ddgi_lighting_diagnostics()
                            .has_mixed_in_flight_revision
                    );
                    log::info!(
                        "[VOXEL_EMISSIVE_ACCEPT] checkpoint=moved-ddgi-gpu local_light_positive=true emissive_material_hit=true source_revision={} registry_revision={} transport_revision={} geometry_revision={} candidates={} visible={} occluded={} luma_q8={} emissive_surface_hits={} emissive_surface_luma_q8={} mixed_in_flight=false",
                        state.expected_source_revision,
                        state.expected_registry_revision,
                        field.field().radiance_revision(),
                        state.terrain_revision,
                        gpu.totals.candidates,
                        gpu.totals.visible,
                        gpu.totals.occluded,
                        gpu.totals.irradiance_luma_q8,
                        gpu.emissive_surface_hits,
                        gpu.emissive_surface_radiance_luma_q8,
                    );
                    state.terrain_revision = self
                        .apply_voxel_emissive_edits(
                            "remove-moved",
                            &[(
                                VOXEL_EMISSIVE_MOVED_MIN,
                                VOXEL_EMISSIVE_MOVED_MAX,
                                VOXEL_TYPE_EMPTY,
                            )],
                            state.terrain_revision,
                        )
                        .expect("moved emissive voxel removal must succeed");
                    state.mutation_frame = self.time_info.total_frame_count();
                    state.stage = VoxelEmissiveTestStage::AwaitRemoveRegistry;
                    TestScenePhase::VoxelEmissiveLifecycle(state)
                }
                VoxelEmissiveTestStage::AwaitRemoveRegistry => {
                    let snapshot = self.local_lights.snapshot();
                    if snapshot.source_revision() == state.expected_source_revision
                        || voxel_emissive_source_count(&snapshot) != 0
                    {
                        return;
                    }
                    let removed_id = state
                        .light_id
                        .expect("removed id must be retained for stale check");
                    assert!(snapshot
                        .lights()
                        .iter()
                        .all(|record| record.id() != removed_id));
                    assert!(snapshot.registry_revision() > state.expected_registry_revision);
                    state.expected_source_revision = snapshot.source_revision();
                    state.expected_registry_revision = snapshot.registry_revision();
                    state.stage = VoxelEmissiveTestStage::AwaitRemoveLive;
                    log::info!(
                        "[VOXEL_EMISSIVE_ACCEPT] checkpoint=remove-registry provider_source_count=0 removed_slot={} removed_generation={} stale_registry=false source_revision={} registry_revision={} edit_to_registry_frames={}",
                        removed_id.slot(),
                        removed_id.generation(),
                        state.expected_source_revision,
                        state.expected_registry_revision,
                        self.time_info.total_frame_count().saturating_sub(state.mutation_frame),
                    );
                    TestScenePhase::VoxelEmissiveLifecycle(state)
                }
                VoxelEmissiveTestStage::AwaitRemoveLive => {
                    if self.tracer.local_light_live_state()
                        != (Some(state.expected_source_revision), 0)
                    {
                        return;
                    }
                    let request_serial = self
                        .tracer
                        .request_local_light_visibility_diagnostic(
                            state.terrain_revision,
                            state.expected_source_revision,
                            state.light_id.unwrap(),
                            POINT_LIGHT_FIXED_RECEIVER_WORLD,
                            POINT_LIGHT_FIXED_RECEIVER_NORMAL,
                            POINT_LIGHT_FIXED_RAY_ORIGIN_OFFSET_WORLD,
                        )
                        .expect("removed voxel-emitter diagnostic request must succeed");
                    self.environment_lighting_test_scene
                        .as_mut()
                        .unwrap()
                        .point_light_fixed_gpu_request_serial = request_serial;
                    state.stage = VoxelEmissiveTestStage::AwaitRemovedDiagnostic;
                    TestScenePhase::VoxelEmissiveLifecycle(state)
                }
                VoxelEmissiveTestStage::AwaitRemovedDiagnostic => {
                    let Some(evidence) = self.tracer.local_light_visibility_diagnostic_evidence()
                    else {
                        return;
                    };
                    if evidence.request.request_serial != fixed_gpu_request_serial {
                        return;
                    }
                    assert!(!evidence.identity_matches);
                    assert_eq!(evidence.selected_light_index, None);
                    assert_eq!(
                        (evidence.candidates, evidence.visible, evidence.occluded),
                        (0, 0, 0)
                    );
                    assert_eq!(evidence.irradiance_luma_q8, 0);
                    state.stage = VoxelEmissiveTestStage::AwaitFinalPublication;
                    TestScenePhase::VoxelEmissiveLifecycle(state)
                }
                VoxelEmissiveTestStage::AwaitFinalPublication => {
                    let Some(transport) = self.tracer.ddgi_live_radiance_snapshot() else {
                        return;
                    };
                    if transport.local_lights.source_revision() != state.expected_source_revision
                        || transport.local_lights.count() != 0
                    {
                        return;
                    }
                    let status = self.tracer.ddgi_runtime_status();
                    let active = status.active();
                    let Some(field) = active.published_field else {
                        return;
                    };
                    if field.field().radiance_revision()
                        != transport.local_lights.info.transport_revision
                        || field.field().geometry_revision() != state.terrain_revision
                        || !is_converged_field(field)
                        || active.stage != DdgiVolumeStage::Ready
                        || active.building_field.is_some()
                        || status.staging().is_some()
                    {
                        return;
                    }
                    let diagnostics = self.tracer.ddgi_lighting_diagnostics();
                    assert!(!diagnostics.has_mixed_in_flight_revision);
                    assert_eq!(diagnostics.in_flight_revision, None);
                    let Some(gpu) = self.tracer.ddgi_local_light_gpu_evidence() else {
                        return;
                    };
                    if !gpu.matches_classified_field(field)
                        || gpu.local_source_revision != state.expected_source_revision
                        || gpu.local_light_count != 0
                    {
                        return;
                    }
                    assert!(gpu.is_complete());
                    assert_eq!(gpu.totals.candidates, 0);
                    assert_eq!(gpu.totals.visible, 0);
                    assert_eq!(gpu.totals.occluded, 0);
                    assert_eq!(gpu.totals.irradiance_luma_q8, 0);
                    let atlas = active
                        .last_atlas_validation
                        .expect("final voxel-emissive field must have atlas evidence");
                    assert_eq!(atlas.non_finite_count, 0);
                    assert!(atlas.max_rgb_value > 0.0);
                    let (lag, coalesced) = self.tracer.local_light_transport_observability();
                    assert_eq!(lag, 0);
                    log_acceptance_field("VOXEL_EMISSIVE", "complete", field);
                    log::info!(
                        "[VOXEL_EMISSIVE_ACCEPT] complete provider_lifecycle=true stable_same_cluster_id=true moved_generation=true direct_removed=true stale_contribution=false mixed_in_flight=false ddgi_off_candidates=0 ddgi_off_luma_q8=0 atlas_nonblack=true atlas_max_rgb={:.8} source_revision={} registry_revision={} transport_revision={} revision_lag={} coalesced_live_revisions={} geometry_revision={} terrain_direct=true raster_direct=true ddgi_injection=true emissive_material_transport=true shared_units=world emitter_endpoint_volume=true",
                        atlas.max_rgb_value,
                        state.expected_source_revision,
                        state.expected_registry_revision,
                        field.field().radiance_revision(),
                        lag,
                        coalesced,
                        state.terrain_revision,
                    );
                    TestScenePhase::Ready
                }
            },
            TestScenePhase::MultiSourceLifecycle(state) => {
                let Some(next) =
                    self.advance_multi_source_lifecycle(state, fixed_gpu_request_serial)
                else {
                    return;
                };
                next
            }
            TestScenePhase::LocalLightScaling(state) => {
                let Some(next) = self.advance_local_light_scaling(state) else {
                    return;
                };
                next
            }
            TestScenePhase::RasterEmitterLifecycle(mut state) => match state.stage {
                RasterEmitterTestStage::AwaitBaseline => {
                    let status = self.tracer.ddgi_runtime_status();
                    let active = status.active();
                    let Some(baseline) = active.published_field else {
                        return;
                    };
                    if !is_converged_field(baseline)
                        || active.stage != DdgiVolumeStage::Ready
                        || active.building_field.is_some()
                        || status.staging().is_some()
                    {
                        return;
                    }
                    assert_eq!(baseline.field().geometry_revision(), state.terrain_revision);
                    assert_eq!(
                        active.relocated_terrain_revision,
                        Some(state.terrain_revision)
                    );
                    assert_eq!(self.tracer.local_light_live_state(), (Some(0), 0));
                    assert_eq!(self.sprinklers.len(), 0);
                    assert_eq!(self.tracer.sprinkler_instance_count(), 0);
                    assert_eq!(self.raster_entity_emitters.source_count(), 0);

                    let component = raster_emitter_component(
                        POINT_LIGHT_ADD_POSITION,
                        Vec3::new(1.0, 0.45, 0.20),
                        0.08,
                    );
                    let (entity, light_id) = self
                        .apply_emissive_sprinkler_placement(
                            SprinklerPlacementTarget::Terrain(RASTER_EMITTER_ADD_BASE_POSITION),
                            component,
                        )
                        .expect("production raster-emitter spawn must succeed");
                    let key = RasterEmitterKey::new(entity, SPRINKLER_HEAD_EMITTER_PART);
                    assert_eq!(
                        self.local_lights
                            .light_id(RASTER_ENTITY_LIGHT_PROVIDER_ID, key.source_key(),),
                        Some(light_id)
                    );
                    let snapshot = self.local_lights.snapshot();
                    state.baseline = Some(baseline);
                    state.entity = Some(entity);
                    state.light_id = Some(light_id);
                    state.expected_source_revision = snapshot.source_revision();
                    state.expected_registry_revision = snapshot.registry_revision();
                    state.expected_provider_revision =
                        self.raster_entity_emitters.snapshot().source_revision();
                    state.expected_sprinkler_revision = self.sprinklers.revision();
                    state.mutation_frame = self.time_info.total_frame_count();
                    state.stage = RasterEmitterTestStage::AwaitSpawnLive;
                    log_acceptance_field("RASTER_EMITTER", "baseline", baseline);
                    log::info!(
                        "[RASTER_EMITTER_ACCEPT] action=spawn entity={:?} part={} light_slot={} light_generation={} base_world={:?} emitter_world={:?} provider_source_revision={} registry_revision={} source_revision={} sprinkler_revision={} renderer_instances={} surface_emissive_pixels=false surface_lighting_contract=sun-plus-environment local_direct_self_injection=false",
                        entity,
                        SPRINKLER_HEAD_EMITTER_PART.get(),
                        light_id.slot(),
                        light_id.generation(),
                        RASTER_EMITTER_ADD_BASE_POSITION,
                        POINT_LIGHT_ADD_POSITION,
                        state.expected_provider_revision,
                        state.expected_registry_revision,
                        state.expected_source_revision,
                        state.expected_sprinkler_revision,
                        self.tracer.sprinkler_instance_count(),
                    );
                    TestScenePhase::RasterEmitterLifecycle(state)
                }
                RasterEmitterTestStage::AwaitSpawnLive => {
                    if self.tracer.local_light_live_state()
                        != (Some(state.expected_source_revision), 1)
                    {
                        return;
                    }
                    if self.time_info.total_frame_count() <= state.mutation_frame {
                        return;
                    }
                    assert_eq!(self.sprinklers.len(), 1);
                    assert_eq!(self.tracer.sprinkler_instance_count(), 1);
                    assert_eq!(self.raster_entity_emitters.source_count(), 1);
                    assert_eq!(
                        self.tracer.local_light_revision_observability().1,
                        Some(state.expected_registry_revision)
                    );
                    assert!(
                        !self
                            .tracer
                            .ddgi_lighting_diagnostics()
                            .has_mixed_in_flight_revision
                    );
                    let request_serial = self
                        .tracer
                        .request_local_light_visibility_diagnostic(
                            state.terrain_revision,
                            state.expected_source_revision,
                            state.light_id.unwrap(),
                            POINT_LIGHT_FIXED_RECEIVER_WORLD,
                            POINT_LIGHT_FIXED_RECEIVER_NORMAL,
                            POINT_LIGHT_FIXED_RAY_ORIGIN_OFFSET_WORLD,
                        )
                        .expect("raster-emitter fixed GPU diagnostic request must succeed");
                    self.environment_lighting_test_scene
                        .as_mut()
                        .unwrap()
                        .point_light_fixed_gpu_request_serial = request_serial;
                    state.stage = RasterEmitterTestStage::AwaitVisibleDiagnostic;
                    TestScenePhase::RasterEmitterLifecycle(state)
                }
                RasterEmitterTestStage::AwaitVisibleDiagnostic => {
                    let Some(evidence) = self.tracer.local_light_visibility_diagnostic_evidence()
                    else {
                        return;
                    };
                    if evidence.request.request_serial != fixed_gpu_request_serial {
                        return;
                    }
                    assert!(evidence.identity_matches);
                    assert_eq!(evidence.selected_light_index, Some(0));
                    assert_eq!(evidence.request.target.light_id(), state.light_id);
                    assert_eq!(
                        evidence.request.source_revision,
                        state.expected_source_revision
                    );
                    assert_eq!(
                        (evidence.candidates, evidence.visible, evidence.occluded),
                        (1, 1, 0)
                    );
                    assert!(evidence.irradiance_luma_q8 > 0);
                    state.visible_luma_q8 = evidence.irradiance_luma_q8;
                    state.stage = RasterEmitterTestStage::AwaitDdgiPublication;
                    log::info!(
                        "[RASTER_EMITTER_ACCEPT] checkpoint=spawn-gpu-direct identity_matches=true selected_index=0 candidates=1 visible=1 occluded=0 luma_q8={} real_renderer_instances=1 direct_immediate=true",
                        state.visible_luma_q8,
                    );
                    TestScenePhase::RasterEmitterLifecycle(state)
                }
                RasterEmitterTestStage::AwaitDdgiPublication => {
                    let Some(transport) = self.tracer.ddgi_live_radiance_snapshot() else {
                        return;
                    };
                    if transport.local_lights.source_revision() != state.expected_source_revision
                        || transport.local_lights.count() != 1
                    {
                        return;
                    }
                    let status = self.tracer.ddgi_runtime_status();
                    let active = status.active();
                    let Some(field) = active.published_field else {
                        return;
                    };
                    if field.field().radiance_revision()
                        != transport.local_lights.info.transport_revision
                        || field.field().geometry_revision() != state.terrain_revision
                        || !is_converged_field(field)
                        || active.stage != DdgiVolumeStage::Ready
                        || active.building_field.is_some()
                        || status.staging().is_some()
                    {
                        return;
                    }
                    let Some(gpu) = self.tracer.ddgi_local_light_gpu_evidence() else {
                        return;
                    };
                    if !gpu.matches_classified_field(field)
                        || gpu.local_source_revision != state.expected_source_revision
                        || gpu.local_light_count != 1
                    {
                        return;
                    }
                    assert!(gpu.is_complete());
                    assert!(gpu.totals.visible > 0);
                    assert!(gpu.totals.irradiance_luma_q8 > 0);
                    assert!(
                        !self
                            .tracer
                            .ddgi_lighting_diagnostics()
                            .has_mixed_in_flight_revision
                    );

                    let revisions_before = (
                        self.raster_entity_emitters.snapshot().source_revision(),
                        self.local_lights.snapshot().registry_revision(),
                        self.local_lights.snapshot().source_revision(),
                        self.sprinklers.revision(),
                    );
                    let light_id = self
                        .update_emissive_sprinkler(
                            state.entity.unwrap(),
                            RASTER_EMITTER_ADD_BASE_POSITION,
                            raster_emitter_component(
                                POINT_LIGHT_ADD_POSITION,
                                Vec3::new(1.0, 0.45, 0.20),
                                0.08,
                            ),
                        )
                        .expect("identical raster-emitter publication must succeed");
                    assert_eq!(light_id, state.light_id.unwrap());
                    assert_eq!(
                        (
                            self.raster_entity_emitters.snapshot().source_revision(),
                            self.local_lights.snapshot().registry_revision(),
                            self.local_lights.snapshot().source_revision(),
                            self.sprinklers.revision(),
                        ),
                        revisions_before,
                        "identical entity/component republish must be a total no-op"
                    );
                    state.mutation_frame = self.time_info.total_frame_count();
                    state.stage = RasterEmitterTestStage::AwaitNoopStable;
                    log::info!(
                        "[RASTER_EMITTER_ACCEPT] checkpoint=ddgi-on provider_source_revision={} source_revision={} transport_revision={} field_serial={} probes={} candidates={} visible={} occluded={} luma_q8={} mixed_in_flight=false action=noop-republish revisions_unchanged=true light_id_unchanged=true",
                        state.expected_provider_revision,
                        state.expected_source_revision,
                        field.field().radiance_revision(),
                        field.field().serial(),
                        gpu.sampled_probe_count,
                        gpu.totals.candidates,
                        gpu.totals.visible,
                        gpu.totals.occluded,
                        gpu.totals.irradiance_luma_q8,
                    );
                    TestScenePhase::RasterEmitterLifecycle(state)
                }
                RasterEmitterTestStage::AwaitNoopStable => {
                    if self.time_info.total_frame_count() <= state.mutation_frame {
                        return;
                    }
                    assert_eq!(
                        self.tracer.local_light_live_state(),
                        (Some(state.expected_source_revision), 1)
                    );
                    assert_eq!(
                        self.raster_entity_emitters.snapshot().source_revision(),
                        state.expected_provider_revision
                    );
                    assert_eq!(
                        self.local_lights.snapshot().registry_revision(),
                        state.expected_registry_revision
                    );
                    assert_eq!(
                        self.sprinklers.revision(),
                        state.expected_sprinkler_revision
                    );

                    let light_id = self
                        .update_emissive_sprinkler(
                            state.entity.unwrap(),
                            RASTER_EMITTER_MOVED_BASE_POSITION,
                            raster_emitter_component(
                                POINT_LIGHT_MOVED_POSITION,
                                Vec3::new(1.0, 0.45, 0.20),
                                0.08,
                            ),
                        )
                        .expect("production raster-emitter move must succeed");
                    assert_eq!(light_id, state.light_id.unwrap());
                    let snapshot = self.local_lights.snapshot();
                    assert!(snapshot.source_revision() > state.expected_source_revision);
                    assert!(snapshot.registry_revision() > state.expected_registry_revision);
                    state.expected_source_revision = snapshot.source_revision();
                    state.expected_registry_revision = snapshot.registry_revision();
                    state.expected_provider_revision =
                        self.raster_entity_emitters.snapshot().source_revision();
                    state.expected_sprinkler_revision = self.sprinklers.revision();
                    state.mutation_frame = self.time_info.total_frame_count();
                    state.stage = RasterEmitterTestStage::AwaitMoveLive;
                    TestScenePhase::RasterEmitterLifecycle(state)
                }
                RasterEmitterTestStage::AwaitMoveLive => {
                    if self.tracer.local_light_live_state()
                        != (Some(state.expected_source_revision), 1)
                    {
                        return;
                    }
                    if self.time_info.total_frame_count() <= state.mutation_frame {
                        return;
                    }
                    let key =
                        RasterEmitterKey::new(state.entity.unwrap(), SPRINKLER_HEAD_EMITTER_PART);
                    assert_eq!(
                        self.local_lights
                            .light_id(RASTER_ENTITY_LIGHT_PROVIDER_ID, key.source_key(),),
                        state.light_id
                    );
                    assert_eq!(self.sprinklers.len(), 1);
                    assert_eq!(self.tracer.sprinkler_instance_count(), 1);
                    assert!(
                        !self
                            .tracer
                            .ddgi_lighting_diagnostics()
                            .has_mixed_in_flight_revision
                    );
                    log::info!(
                        "[RASTER_EMITTER_ACCEPT] checkpoint=move-live stable_light_id=true entity={:?} light_slot={} light_generation={} provider_source_revision={} registry_revision={} source_revision={} renderer_instances=1 direct_immediate=true",
                        state.entity.unwrap(),
                        state.light_id.unwrap().slot(),
                        state.light_id.unwrap().generation(),
                        state.expected_provider_revision,
                        state.expected_registry_revision,
                        state.expected_source_revision,
                    );
                    state.stage = RasterEmitterTestStage::AwaitMoveMidflight;
                    TestScenePhase::RasterEmitterLifecycle(state)
                }
                RasterEmitterTestStage::AwaitMoveMidflight => {
                    let status = self.tracer.ddgi_runtime_status();
                    assert!(status.staging().is_none());
                    let active = status.active();
                    let Some(in_flight) = active.building_field else {
                        return;
                    };
                    let work = active
                        .target_work
                        .expect("raster-emitter move build must retain work");
                    assert_eq!(work.kind(), DdgiScheduledWorkKind::RadianceUpdate);
                    assert_eq!(work.destination(), in_flight);
                    let priority = active
                        .probe_priority
                        .expect("raster-emitter move must carry lighting impact priority");
                    assert_eq!(priority.reason(), DdgiProbePriorityReason::LightingImpact);
                    let remaining = active
                        .grid
                        .probe_count()
                        .saturating_sub(active.filtered_probe_count);
                    if active.filtered_probe_count == 0 || remaining <= 3 * DDGI_PROBE_BATCH_SIZE {
                        return;
                    }
                    let builder = self
                        .tracer
                        .ddgi_builder_radiance_snapshot()
                        .expect("raster-emitter DDGI build must freeze its local-light snapshot");
                    assert_eq!(
                        builder.local_lights.source_revision(),
                        state.expected_source_revision
                    );
                    assert_eq!(builder.local_lights.count(), 1);
                    assert_eq!(
                        builder.local_lights.info.transport_revision,
                        in_flight.field().radiance_revision()
                    );
                    assert!(
                        !self
                            .tracer
                            .ddgi_lighting_diagnostics()
                            .has_mixed_in_flight_revision
                    );
                    let frozen_source_revision = state.expected_source_revision;
                    let light_id = self
                        .update_emissive_sprinkler(
                            state.entity.unwrap(),
                            RASTER_EMITTER_MOVED_BASE_POSITION,
                            raster_emitter_component(
                                POINT_LIGHT_MOVED_POSITION,
                                Vec3::new(0.20, 0.55, 1.0),
                                0.14,
                            ),
                        )
                        .expect("raster-emitter photometric update must succeed");
                    assert_eq!(light_id, state.light_id.unwrap());
                    let snapshot = self.local_lights.snapshot();
                    state.in_flight = Some(in_flight);
                    state.in_flight_source_revision = frozen_source_revision;
                    state.expected_source_revision = snapshot.source_revision();
                    state.expected_registry_revision = snapshot.registry_revision();
                    state.expected_provider_revision =
                        self.raster_entity_emitters.snapshot().source_revision();
                    state.expected_sprinkler_revision = self.sprinklers.revision();
                    state.mutation_frame = self.time_info.total_frame_count();
                    state.stage = RasterEmitterTestStage::AwaitPhotometricLive;
                    log::info!(
                        "[RASTER_EMITTER_ACCEPT] checkpoint=move-midflight action=photometric-update stable_light_id=true frozen_source_revision={} live_source_revision={} ddgi_in_flight_revision={} progress={}/{} priority_reason={:?} full_sweep_probes={} mixed_in_flight=false",
                        frozen_source_revision,
                        state.expected_source_revision,
                        in_flight.field().radiance_revision(),
                        active.filtered_probe_count,
                        active.grid.probe_count(),
                        priority.reason(),
                        active.grid.probe_count(),
                    );
                    TestScenePhase::RasterEmitterLifecycle(state)
                }
                RasterEmitterTestStage::AwaitPhotometricLive => {
                    if self.tracer.local_light_live_state()
                        != (Some(state.expected_source_revision), 1)
                    {
                        return;
                    }
                    if self.time_info.total_frame_count() <= state.mutation_frame {
                        return;
                    }
                    assert!(
                        !self
                            .tracer
                            .ddgi_lighting_diagnostics()
                            .has_mixed_in_flight_revision
                    );
                    let removed = self
                        .remove_emissive_sprinkler(state.entity.unwrap())
                        .expect("production raster-emitter despawn must succeed");
                    assert_eq!(removed, state.light_id.unwrap());
                    let key =
                        RasterEmitterKey::new(state.entity.unwrap(), SPRINKLER_HEAD_EMITTER_PART);
                    assert!(self
                        .local_lights
                        .light_id(RASTER_ENTITY_LIGHT_PROVIDER_ID, key.source_key())
                        .is_none());
                    let snapshot = self.local_lights.snapshot();
                    state.expected_source_revision = snapshot.source_revision();
                    state.expected_registry_revision = snapshot.registry_revision();
                    state.expected_provider_revision =
                        self.raster_entity_emitters.snapshot().source_revision();
                    state.expected_sprinkler_revision = self.sprinklers.revision();
                    state.mutation_frame = self.time_info.total_frame_count();
                    state.stage = RasterEmitterTestStage::AwaitRemoveLive;
                    log::info!(
                        "[RASTER_EMITTER_ACCEPT] checkpoint=photometric-live action=despawn stable_light_id=true removed_slot={} removed_generation={} source_revision={} provider_source_revision={} registry_revision={} frozen_in_flight_source_revision={} stale_registry_source=false surface_emissive_pixels=false",
                        removed.slot(),
                        removed.generation(),
                        state.expected_source_revision,
                        state.expected_provider_revision,
                        state.expected_registry_revision,
                        state.in_flight_source_revision,
                    );
                    TestScenePhase::RasterEmitterLifecycle(state)
                }
                RasterEmitterTestStage::AwaitRemoveLive => {
                    if self.tracer.local_light_live_state()
                        != (Some(state.expected_source_revision), 0)
                    {
                        return;
                    }
                    if self.time_info.total_frame_count() <= state.mutation_frame {
                        return;
                    }
                    assert!(self.sprinklers.is_empty());
                    assert_eq!(self.tracer.sprinkler_instance_count(), 0);
                    assert_eq!(self.raster_entity_emitters.source_count(), 0);
                    assert!(
                        !self
                            .tracer
                            .ddgi_lighting_diagnostics()
                            .has_mixed_in_flight_revision
                    );
                    let request_serial = self
                        .tracer
                        .request_local_light_visibility_diagnostic(
                            state.terrain_revision,
                            state.expected_source_revision,
                            state.light_id.unwrap(),
                            POINT_LIGHT_FIXED_RECEIVER_WORLD,
                            POINT_LIGHT_FIXED_RECEIVER_NORMAL,
                            POINT_LIGHT_FIXED_RAY_ORIGIN_OFFSET_WORLD,
                        )
                        .expect("removed raster-emitter diagnostic request must succeed");
                    self.environment_lighting_test_scene
                        .as_mut()
                        .unwrap()
                        .point_light_fixed_gpu_request_serial = request_serial;
                    state.stage = RasterEmitterTestStage::AwaitRemovedDiagnostic;
                    TestScenePhase::RasterEmitterLifecycle(state)
                }
                RasterEmitterTestStage::AwaitRemovedDiagnostic => {
                    let Some(evidence) = self.tracer.local_light_visibility_diagnostic_evidence()
                    else {
                        return;
                    };
                    if evidence.request.request_serial != fixed_gpu_request_serial {
                        return;
                    }
                    assert!(!evidence.identity_matches);
                    assert_eq!(evidence.selected_light_index, None);
                    assert_eq!(
                        (evidence.candidates, evidence.visible, evidence.occluded),
                        (0, 0, 0)
                    );
                    assert_eq!(evidence.irradiance_luma_q8, 0);
                    state.stage = RasterEmitterTestStage::AwaitFinalPublication;
                    TestScenePhase::RasterEmitterLifecycle(state)
                }
                RasterEmitterTestStage::AwaitFinalPublication => {
                    let Some(transport) = self.tracer.ddgi_live_radiance_snapshot() else {
                        return;
                    };
                    if transport.local_lights.source_revision() != state.expected_source_revision
                        || transport.local_lights.count() != 0
                    {
                        return;
                    }
                    let status = self.tracer.ddgi_runtime_status();
                    let active = status.active();
                    let Some(field) = active.published_field else {
                        return;
                    };
                    if field.field().radiance_revision()
                        != transport.local_lights.info.transport_revision
                        || field.field().geometry_revision() != state.terrain_revision
                        || !is_converged_field(field)
                        || active.stage != DdgiVolumeStage::Ready
                        || active.building_field.is_some()
                        || status.staging().is_some()
                    {
                        return;
                    }
                    let diagnostics = self.tracer.ddgi_lighting_diagnostics();
                    assert!(!diagnostics.has_mixed_in_flight_revision);
                    assert_eq!(diagnostics.in_flight_revision, None);
                    let Some(gpu) = self.tracer.ddgi_local_light_gpu_evidence() else {
                        return;
                    };
                    if !gpu.matches_classified_field(field)
                        || gpu.local_source_revision != state.expected_source_revision
                        || gpu.local_light_count != 0
                    {
                        return;
                    }
                    assert!(gpu.is_complete());
                    assert_eq!(gpu.totals.candidates, 0);
                    assert_eq!(gpu.totals.visible, 0);
                    assert_eq!(gpu.totals.occluded, 0);
                    assert_eq!(gpu.totals.irradiance_luma_q8, 0);
                    let atlas = active
                        .last_atlas_validation
                        .expect("final raster-emitter field must have atlas evidence");
                    assert_eq!(atlas.non_finite_count, 0);
                    assert!(atlas.max_rgb_value > 0.0);
                    let baseline = state.baseline.unwrap();
                    assert_eq!(
                        baseline.field().geometry_revision(),
                        field.field().geometry_revision(),
                        "raster emitter lifecycle must not invalidate geometry visibility"
                    );
                    assert_eq!(
                        active.relocated_terrain_revision,
                        Some(state.terrain_revision)
                    );
                    let (lag, coalesced) = self.tracer.local_light_transport_observability();
                    assert_eq!(lag, 0);
                    log_acceptance_field("RASTER_EMITTER", "complete", field);
                    log::info!(
                        "[RASTER_EMITTER_ACCEPT] complete production_hook=true spawn=true noop=true move=true photometric=true despawn=true stable_entity_part_identity=true stable_light_id=true provider_source_revision={} registry_revision={} source_revision={} transport_revision={} revision_lag={} coalesced_live_revisions={} renderer_instances=0 stale_direct=false stale_ddgi=false mixed_in_flight=false ddgi_on_luma_positive=true fixed_direct_luma_q8={} ddgi_off_candidates=0 ddgi_off_luma_q8=0 atlas_nonblack=true atlas_max_rgb={:.8} geometry_visibility_preserved=true surface_emissive_pixels=false surface_lighting_contract=sun-plus-environment local_direct_self_injection=false",
                        state.expected_provider_revision,
                        state.expected_registry_revision,
                        state.expected_source_revision,
                        field.field().radiance_revision(),
                        lag,
                        coalesced,
                        state.visible_luma_q8,
                        atlas.max_rgb_value,
                    );
                    TestScenePhase::Ready
                }
            },
            TestScenePhase::CapturingRadianceBaseline { .. } => return,
            TestScenePhase::MutatingRadianceR2 { .. } => return,
            TestScenePhase::CapturingRadianceR2NextFrame { .. } => return,
            TestScenePhase::WaitingForRadianceR2Midflight { r1 } => {
                let status = self.tracer.ddgi_runtime_status();
                assert!(status.staging().is_none());
                let active = status.active();
                assert_eq!(active.published_field, Some(r1));
                let Some(r2) = active.building_field else {
                    return;
                };
                let r2_revision = next_nonzero_revision(r1.field().radiance_revision());
                assert_radiance_epoch_zero(r2, r1, r2_revision);
                assert_eq!(r2.field().serial(), r1.field().serial() + 1);
                let work = active
                    .target_work
                    .expect("r2 building field must retain scheduled work");
                assert_eq!(work.kind(), DdgiScheduledWorkKind::RadianceUpdate);
                assert_eq!(work.destination(), r2);
                let remaining = active
                    .grid
                    .probe_count()
                    .saturating_sub(active.filtered_probe_count);
                if active.filtered_probe_count == 0 || remaining <= 3 * DDGI_PROBE_BATCH_SIZE {
                    return;
                }
                assert!(self
                    .tracer
                    .ddgi_runtime_status()
                    .active_consumers_are_available());
                log::info!(
                    "[DDGI_ACCEPT][RADIANCE] checkpoint=r2-midflight active_field_serial={} active_radiance_revision={} building_field_serial={} building_radiance_revision={} building_update_epoch={} source_field_serial={} progress={}/{} old_field_visible=true",
                    r1.field().serial(),
                    r1.field().radiance_revision(),
                    r2.field().serial(),
                    r2.field().radiance_revision(),
                    r2.field().update_epoch(),
                    r2.source().expect("r2 must have source").serial(),
                    active.filtered_probe_count,
                    active.grid.probe_count(),
                );
                self.apply_radiance_test_mutation(
                    RADIANCE_R3_TIME_OF_DAY,
                    RADIANCE_R3_SUN_COLOR,
                    RADIANCE_R3_SUN_LUMINANCE,
                    RADIANCE_R3_ROCK_COLOR,
                );
                log::info!(
                    "[DDGI_ACCEPT][RADIANCE] mutation=r3 frame={} time_of_day={} sun_rgb={},{},{} sun_luminance={} rock_rgb={},{},{} expected_radiance_revision={}",
                    self.time_info.total_frame_count(),
                    RADIANCE_R3_TIME_OF_DAY,
                    RADIANCE_R3_SUN_COLOR.r(),
                    RADIANCE_R3_SUN_COLOR.g(),
                    RADIANCE_R3_SUN_COLOR.b(),
                    RADIANCE_R3_SUN_LUMINANCE,
                    RADIANCE_R3_ROCK_COLOR.r(),
                    RADIANCE_R3_ROCK_COLOR.g(),
                    RADIANCE_R3_ROCK_COLOR.b(),
                    next_nonzero_revision(r2_revision),
                );
                TestScenePhase::WaitingForRadianceR3Observed { r1, r2 }
            }
            TestScenePhase::WaitingForRadianceR3Observed { r1, r2 } => {
                let active = self.tracer.ddgi_runtime_status().active();
                assert_eq!(active.published_field, Some(r1));
                assert_eq!(active.building_field, Some(r2));
                let r3_revision = next_nonzero_revision(r2.field().radiance_revision());
                if self.tracer.ddgi_latest_radiance_revision() != Some(r3_revision) {
                    return;
                }
                log::info!(
                    "[DDGI_ACCEPT][RADIANCE] checkpoint=r3-observed latest_radiance_revision={} inflight_field_serial={} inflight_radiance_revision={} field_serial_allocated=false",
                    r3_revision,
                    r2.field().serial(),
                    r2.field().radiance_revision(),
                );
                TestScenePhase::MutatingRadianceR4 { r1, r2 }
            }
            TestScenePhase::MutatingRadianceR4 { .. } => return,
            TestScenePhase::CapturingRadianceR4NextFrame { .. } => return,
            TestScenePhase::WaitingForRadianceR4Midflight { r1, r2 } => {
                let active = self.tracer.ddgi_runtime_status().active();
                let r4_revision =
                    next_nonzero_revision(next_nonzero_revision(r2.field().radiance_revision()));
                if self.tracer.ddgi_latest_radiance_revision() != Some(r4_revision) {
                    return;
                }
                match active.published_field {
                    Some(published) if published == r1 => {
                        assert_eq!(active.building_field, Some(r2));
                        return;
                    }
                    Some(published) => assert_eq!(published, r2),
                    None => panic!("radiance coalescing lost the published r1 field"),
                }
                let Some(r4) = active.building_field else {
                    return;
                };
                assert_radiance_epoch_zero(r4, r2, r4_revision);
                assert_eq!(
                    r4.field().serial(),
                    r2.field().serial() + 1,
                    "r3 must not claim work or allocate a field serial"
                );
                let work = active
                    .target_work
                    .expect("r4 building field must retain scheduled work");
                assert_eq!(work.kind(), DdgiScheduledWorkKind::RadianceUpdate);
                assert_eq!(work.destination(), r4);
                if active.filtered_probe_count == 0 {
                    return;
                }
                log::info!(
                    "[DDGI_ACCEPT][RADIANCE] checkpoint=r4-midflight active_field_serial={} active_radiance_revision={} building_field_serial={} building_radiance_revision={} building_update_epoch={} source_field_serial={} progress={}/{} r3_coalesced=true old_field_visible=true",
                    r2.field().serial(),
                    r2.field().radiance_revision(),
                    r4.field().serial(),
                    r4.field().radiance_revision(),
                    r4.field().update_epoch(),
                    r4.source().expect("r4 must have source").serial(),
                    active.filtered_probe_count,
                    active.grid.probe_count(),
                );
                TestScenePhase::WaitingForRadianceR4Published { r1, r2, r4 }
            }
            TestScenePhase::WaitingForRadianceR4Published { r1, r2, r4 } => {
                let active = self.tracer.ddgi_runtime_status().active();
                if active.published_field != Some(r4) {
                    assert_eq!(active.published_field, Some(r2));
                    assert_eq!(active.building_field, Some(r4));
                    return;
                }
                assert_eq!(
                    r1.field().geometry_revision(),
                    r4.field().geometry_revision()
                );
                assert_eq!(r1.field().spacing_voxels(), r4.field().spacing_voxels());
                assert_eq!(r4.source(), Some(r2.field()));
                assert_eq!(r4.field().serial(), r2.field().serial() + 1);
                log_acceptance_field("RADIANCE", "complete", r4);
                log::info!(
                    "[DDGI_ACCEPT][RADIANCE] complete r3_coalesced=true field_serial_gap_r2_to_r4=1 geometry_unchanged=true spacing_unchanged=true"
                );
                if self.environment_irradiance_capture.is_enabled() {
                    TestScenePhase::CapturingRadianceR4Published { r1, r2, r4 }
                } else {
                    TestScenePhase::Ready
                }
            }
            TestScenePhase::CapturingRadianceR4Published { .. } => return,
            TestScenePhase::WaitingForDensityMidflight { baseline } => {
                let runtime = self.tracer.ddgi_runtime_status();
                let active = runtime.active();
                assert_eq!(active.published_field, Some(baseline));
                assert_eq!(active.grid.spacing_voxels(), 32);
                assert!(runtime.active_consumers_are_available());
                let Some(staging) = runtime.staging() else {
                    return;
                };
                let token = staging
                    .build_token
                    .expect("density staging must have a build token");
                assert_eq!(token.kind(), DdgiBuildKind::Density);
                assert_eq!(token.spacing_voxels(), 16);
                assert_eq!(
                    token.terrain_revision(),
                    baseline.field().geometry_revision()
                );
                assert_eq!(runtime.deferred_density_spacing_voxels(), None);
                assert!(matches!(
                    runtime.coordinator(),
                    DdgiRefreshState::BuildingDensity {
                        candidate,
                        queued_spacing_voxels: None,
                    } if candidate == token
                ));
                let Some(work) = staging.target_work else {
                    return;
                };
                assert_eq!(work.kind(), DdgiScheduledWorkKind::DensityUpdate);
                let density_field = work.destination();
                assert_initial_epoch_zero(
                    density_field,
                    baseline.field().geometry_revision(),
                    baseline.field().radiance_revision(),
                    16,
                );
                if staging.building_field != Some(density_field) {
                    return;
                }
                let remaining = staging
                    .grid
                    .probe_count()
                    .saturating_sub(staging.filtered_probe_count);
                if staging.filtered_probe_count == 0 || remaining <= 3 * DDGI_PROBE_BATCH_SIZE {
                    return;
                }
                log::info!(
                    "[DDGI_ACCEPT][DENSITY] checkpoint=density-midflight active_token_serial={} active_field_serial={} active_geometry_revision={} active_spacing_voxels=32 obsolete_density_token_serial={} obsolete_density_field_serial={} obsolete_density_spacing_voxels=16 progress={}/{} old_field_visible=true active_available=true",
                    runtime
                        .active_token_serial()
                        .expect("initial active token missing"),
                    baseline.field().serial(),
                    baseline.field().geometry_revision(),
                    token.serial(),
                    density_field.field().serial(),
                    staging.filtered_probe_count,
                    staging.grid.probe_count(),
                );
                match self.apply_environment_lighting_terrain_edit(
                    TerrainEdit::CloseSkylight,
                    baseline.field().geometry_revision(),
                ) {
                    Ok(target_revision) => {
                        assert_eq!(
                            target_revision,
                            next_nonzero_revision(baseline.field().geometry_revision())
                        );
                        TestScenePhase::WaitingForDensityGeometryReplacement {
                            baseline,
                            obsolete_density_token_serial: token.serial(),
                            obsolete_density_field: density_field,
                            target_revision,
                        }
                    }
                    Err(err) => {
                        log::error!("[DDGI_ACCEPT][DENSITY] terrain edit failed: {err:#}");
                        TestScenePhase::Failed
                    }
                }
            }
            TestScenePhase::WaitingForDensityGeometryReplacement {
                baseline,
                obsolete_density_token_serial,
                obsolete_density_field,
                target_revision,
            } => {
                let runtime = self.tracer.ddgi_runtime_status();
                let active = runtime.active();
                assert_eq!(active.published_field, Some(baseline));
                assert_eq!(active.grid.spacing_voxels(), 32);
                assert_ne!(
                    runtime.active_token_serial(),
                    Some(obsolete_density_token_serial)
                );
                assert!(runtime.active_consumers_are_available());
                assert_eq!(runtime.target_terrain_revision(), Some(target_revision));
                assert_eq!(runtime.deferred_density_spacing_voxels(), Some(16));
                let Some(staging) = runtime.staging() else {
                    return;
                };
                let token = staging
                    .build_token
                    .expect("replacement staging must have a build token");
                if token.serial() == obsolete_density_token_serial {
                    assert_eq!(token.kind(), DdgiBuildKind::Density);
                    assert_eq!(staging.grid.spacing_voxels(), 16);
                    assert!(matches!(
                        runtime.coordinator(),
                        DdgiRefreshState::AwaitingTerrain {
                            latest_terrain_revision,
                        } if latest_terrain_revision == target_revision
                    ));
                    return;
                }
                assert!(token.serial() > obsolete_density_token_serial);
                assert_eq!(token.kind(), DdgiBuildKind::Terrain);
                assert_eq!(token.terrain_revision(), target_revision);
                assert_eq!(token.spacing_voxels(), 32);
                assert_eq!(staging.grid.spacing_voxels(), 32);
                assert!(matches!(
                    runtime.coordinator(),
                    DdgiRefreshState::BuildingTerrain {
                        candidate,
                        latest_terrain_revision,
                    } if candidate == token && latest_terrain_revision == target_revision
                ));
                log::info!(
                    "[DDGI_ACCEPT][DENSITY] checkpoint=geometry-preempted-density obsolete_density_token_serial={} obsolete_density_field_serial={} terrain_token_serial={} target_geometry_revision={} terrain_spacing_voxels=32 queued_density_spacing_voxels=16 obsolete_density_consumer_visible=false active_available=true",
                    obsolete_density_token_serial,
                    obsolete_density_field.field().serial(),
                    token.serial(),
                    target_revision,
                );
                TestScenePhase::WaitingForDensityGeometryPublished {
                    baseline,
                    obsolete_density_token_serial,
                    obsolete_density_field,
                    terrain_token_serial: token.serial(),
                    target_revision,
                }
            }
            TestScenePhase::WaitingForDensityGeometryPublished {
                baseline,
                obsolete_density_token_serial,
                obsolete_density_field,
                terrain_token_serial,
                target_revision,
            } => {
                let runtime = self.tracer.ddgi_runtime_status();
                assert_ne!(
                    runtime.active_token_serial(),
                    Some(obsolete_density_token_serial)
                );
                assert_ne!(
                    runtime.active().published_field,
                    Some(obsolete_density_field),
                    "obsolete density field became consumer-visible"
                );
                if runtime.active_token_serial() != Some(terrain_token_serial) {
                    assert_eq!(runtime.active().published_field, Some(baseline));
                    assert_eq!(runtime.active().grid.spacing_voxels(), 32);
                    assert!(runtime.active_consumers_are_available());
                    return;
                }
                let geometry_field = runtime
                    .active()
                    .published_field
                    .expect("terrain epoch zero must be published before promotion");
                assert_geometry_epoch_zero(geometry_field, baseline, target_revision);
                assert!(runtime.active_consumers_are_available());
                assert_eq!(runtime.deferred_density_spacing_voxels(), Some(16));
                log_acceptance_field("DENSITY", "geometry-e0-published", geometry_field);
                log::info!(
                    "[DDGI_ACCEPT][DENSITY] checkpoint=geometry-e0-published terrain_token_serial={} obsolete_density_token_serial={} geometry_revision={} active_spacing_voxels=32 queued_density_spacing_voxels=16 obsolete_density_consumer_visible=false active_available=true",
                    terrain_token_serial,
                    obsolete_density_token_serial,
                    target_revision,
                );
                TestScenePhase::WaitingForDensityRetryMidflight {
                    geometry_field,
                    obsolete_density_token_serial,
                    terrain_token_serial,
                }
            }
            TestScenePhase::WaitingForDensityRetryMidflight {
                geometry_field,
                obsolete_density_token_serial,
                terrain_token_serial,
            } => {
                let runtime = self.tracer.ddgi_runtime_status();
                let active = runtime.active();
                assert_eq!(active.published_field, Some(geometry_field));
                assert_eq!(runtime.active_token_serial(), Some(terrain_token_serial));
                assert_eq!(active.grid.spacing_voxels(), 32);
                assert!(runtime.active_consumers_are_available());
                assert_ne!(
                    runtime.active_token_serial(),
                    Some(obsolete_density_token_serial)
                );
                let Some(staging) = runtime.staging() else {
                    return;
                };
                let token = staging
                    .build_token
                    .expect("retried density staging must have a build token");
                assert!(token.serial() > terrain_token_serial);
                assert_ne!(token.serial(), obsolete_density_token_serial);
                assert_eq!(token.kind(), DdgiBuildKind::Density);
                assert_eq!(
                    token.terrain_revision(),
                    geometry_field.field().geometry_revision()
                );
                assert_eq!(token.spacing_voxels(), 16);
                assert_eq!(staging.grid.spacing_voxels(), 16);
                assert!(matches!(
                    runtime.coordinator(),
                    DdgiRefreshState::BuildingDensity {
                        candidate,
                        queued_spacing_voxels: None,
                    } if candidate == token
                ));
                let Some(work) = staging.target_work else {
                    return;
                };
                assert_eq!(work.kind(), DdgiScheduledWorkKind::DensityUpdate);
                let density_field = work.destination();
                assert_initial_epoch_zero(
                    density_field,
                    geometry_field.field().geometry_revision(),
                    geometry_field.field().radiance_revision(),
                    16,
                );
                if staging.building_field != Some(density_field) {
                    return;
                }
                if staging.filtered_probe_count == 0 {
                    return;
                }
                log::info!(
                    "[DDGI_ACCEPT][DENSITY] checkpoint=density-retry-midflight active_token_serial={} active_field_serial={} active_geometry_revision={} active_spacing_voxels=32 density_token_serial={} density_field_serial={} density_spacing_voxels=16 progress={}/{} old_field_visible=true active_available=true",
                    terrain_token_serial,
                    geometry_field.field().serial(),
                    geometry_field.field().geometry_revision(),
                    token.serial(),
                    density_field.field().serial(),
                    staging.filtered_probe_count,
                    staging.grid.probe_count(),
                );
                TestScenePhase::WaitingForDensityFinalPublished {
                    geometry_field,
                    obsolete_density_token_serial,
                    terrain_token_serial,
                    density_token_serial: token.serial(),
                    density_field,
                }
            }
            TestScenePhase::WaitingForDensityFinalPublished {
                geometry_field,
                obsolete_density_token_serial,
                terrain_token_serial,
                density_token_serial,
                density_field,
            } => {
                let runtime = self.tracer.ddgi_runtime_status();
                assert_ne!(
                    runtime.active_token_serial(),
                    Some(obsolete_density_token_serial)
                );
                if runtime.active_token_serial() != Some(density_token_serial) {
                    assert_eq!(runtime.active_token_serial(), Some(terrain_token_serial));
                    assert_eq!(runtime.active().published_field, Some(geometry_field));
                    assert_eq!(runtime.active().grid.spacing_voxels(), 32);
                    if let Some(staging_token) =
                        runtime.staging().and_then(|staging| staging.build_token)
                    {
                        assert_eq!(
                            staging_token.serial(),
                            density_token_serial,
                            "only the retried density token may remain staged"
                        );
                    }
                    return;
                }
                let active = runtime.active();
                assert_eq!(active.published_field, Some(density_field));
                assert_eq!(active.grid.spacing_voxels(), 16);
                assert_initial_epoch_zero(
                    density_field,
                    geometry_field.field().geometry_revision(),
                    geometry_field.field().radiance_revision(),
                    16,
                );
                assert!(obsolete_density_token_serial < terrain_token_serial);
                assert!(terrain_token_serial < density_token_serial);
                assert!(runtime.active_consumers_are_available());
                log_acceptance_field("DENSITY", "complete", density_field);
                log::info!(
                    "[DDGI_ACCEPT][DENSITY] complete obsolete_density_token_serial={} terrain_token_serial={} density_token_serial={} obsolete_density_consumer_visible=false first_consumer_visible_16_epoch=0 geometry_revision={} spacing_voxels=16",
                    obsolete_density_token_serial,
                    terrain_token_serial,
                    density_token_serial,
                    density_field.field().geometry_revision(),
                );
                TestScenePhase::Ready
            }
            TestScenePhase::PattSeamTerrainPublished { target_revision } => {
                log::info!(
                    "[DDGI_SEAM_REPRO] visible terrain publication complete target_revision={}",
                    target_revision,
                );
                TestScenePhase::WaitingForPattSeamProbeField { target_revision }
            }
            TestScenePhase::WaitingForPattSeamProbeField { target_revision } => {
                let runtime = self.tracer.ddgi_runtime_status();
                let active = runtime.active();
                let Some(field) = active.published_field else {
                    return;
                };
                if field.field().geometry_revision() != target_revision
                    || !is_converged_field(field)
                    || active.stage != DdgiVolumeStage::Ready
                    || active.building_field.is_some()
                    || runtime.staging().is_some()
                {
                    return;
                }
                log::info!(
                    "[DDGI_SEAM_REPRO] ready target_revision={} state={:?} update_epoch={} opening=shovel-sphere",
                    target_revision,
                    field.field().state(),
                    field.field().update_epoch(),
                );
                TestScenePhase::Ready
            }
            TestScenePhase::TerrainEditPublished {
                edit,
                target_revision,
            } => {
                log::info!(
                    "[ENV_LIGHT_EDIT_CYCLE] visible terrain publication complete edit={} target_revision={}",
                    edit.label(),
                    target_revision,
                );
                TestScenePhase::WaitingForEditedProbeField {
                    edit,
                    target_revision,
                }
            }
            TestScenePhase::WaitingForEditedProbeField {
                edit,
                target_revision,
            } => {
                if matches!(
                    case,
                    EnvironmentLightingTestCase::TerrainEditsInflight
                        | EnvironmentLightingTestCase::TerrainEditsInflightCapture
                ) && edit == TerrainEdit::CloseSkylight
                {
                    let status = self.tracer.ddgi_runtime_status();
                    let Some(staging) = status.staging().filter(|staging| !staging.is_ready())
                    else {
                        return;
                    };
                    log::info!(
                        "[ENV_LIGHT_EDIT_INFLIGHT] obsolete candidate observed terrain_revision={} stage={:?} active_terrain_revision={:?} token={:?}",
                        target_revision,
                        staging.stage,
                        status.active().relocated_terrain_revision,
                        staging.build_token,
                    );
                    let phase = match self.apply_environment_lighting_terrain_edit(
                        TerrainEdit::ReopenSkylight,
                        target_revision,
                    ) {
                        Ok(reopen_revision) => TestScenePhase::TerrainEditPublished {
                            edit: TerrainEdit::ReopenSkylight,
                            target_revision: reopen_revision,
                        },
                        Err(err) => {
                            log::error!("[ENV_LIGHT_EDIT_INFLIGHT] reopen edit failed: {err:#}");
                            TestScenePhase::Failed
                        }
                    };
                    self.environment_lighting_test_scene
                        .as_mut()
                        .expect("test scene state disappeared")
                        .phase = phase;
                    return;
                } else if case == EnvironmentLightingTestCase::TerrainEditsInflightCapture
                    && edit == TerrainEdit::ReopenSkylight
                {
                    let runtime = self.tracer.ddgi_runtime_status();
                    let Some(staging) = runtime.staging() else {
                        return;
                    };
                    let latest_is_building = matches!(
                        runtime.coordinator(),
                        crate::ddgi::DdgiRefreshState::BuildingTerrain {
                            candidate,
                            latest_terrain_revision,
                        } if candidate.terrain_revision() == target_revision
                            && latest_terrain_revision == target_revision
                    );
                    if !latest_is_building
                        || !runtime.active_consumers_are_available()
                        || staging.stage == crate::ddgi::DdgiVolumeStage::Ready
                    {
                        return;
                    }
                    log::info!(
                        "[ENV_LIGHT_EDIT_INFLIGHT_CAPTURE] armed active_terrain_revision={:?} target_terrain_revision={} staging_token_serial={:?} staging_stage={:?} staging_progress={}/{} coordinator={:?} invalidation=stale-active",
                        runtime.active().relocated_terrain_revision,
                        target_revision,
                        runtime.staging_token().map(crate::ddgi::DdgiBuildToken::serial),
                        staging.stage,
                        staging.filtered_probe_count,
                        staging.grid.probe_count(),
                        runtime.coordinator(),
                    );
                    self.environment_lighting_test_scene
                        .as_mut()
                        .expect("test scene state disappeared")
                        .phase = TestScenePhase::CapturingInflightStaleActive { target_revision };
                    return;
                } else if !self.tracer.ddgi_ready_for_terrain_revision(target_revision) {
                    return;
                }
                log::info!(
                    "[ENV_LIGHT_EDIT_CYCLE] edited probe field ready edit={} terrain_revision={}",
                    edit.label(),
                    target_revision,
                );
                match edit {
                    TerrainEdit::CloseSkylight => {
                        if case == EnvironmentLightingTestCase::TerrainEditsClosed {
                            log::info!(
                                "[ENV_LIGHT_EDIT_CYCLE] complete mode=closed final_terrain_revision={}",
                                target_revision,
                            );
                            TestScenePhase::Ready
                        } else {
                            match self.apply_environment_lighting_terrain_edit(
                                TerrainEdit::ReopenSkylight,
                                target_revision,
                            ) {
                                Ok(reopen_revision) => TestScenePhase::TerrainEditPublished {
                                    edit: TerrainEdit::ReopenSkylight,
                                    target_revision: reopen_revision,
                                },
                                Err(err) => {
                                    log::error!(
                                        "[ENV_LIGHT_EDIT_CYCLE] reopen edit failed: {err:#}"
                                    );
                                    TestScenePhase::Failed
                                }
                            }
                        }
                    }
                    TerrainEdit::ReopenSkylight => {
                        let spacing_voxels = self
                            .tracer
                            .ddgi_runtime_status()
                            .active()
                            .grid
                            .spacing_voxels();
                        self.tracer.rebuild_environment_probes(spacing_voxels);
                        log::info!(
                            "[ENV_LIGHT_EDIT_CYCLE] requested density rebuild terrain_revision={} spacing_voxels={}",
                            target_revision,
                            spacing_voxels,
                        );
                        TestScenePhase::WaitingForDensityRebuild {
                            terrain_revision: target_revision,
                        }
                    }
                }
            }
            TestScenePhase::WaitingForDensityRebuild { terrain_revision } => {
                let status = self.tracer.ddgi_runtime_status();
                if status.staging().is_some()
                    || !self
                        .tracer
                        .ddgi_ready_for_terrain_revision(terrain_revision)
                {
                    return;
                }
                log::info!(
                    "[ENV_LIGHT_EDIT_CYCLE] density rebuild ready terrain_revision={}",
                    terrain_revision,
                );
                log::info!(
                    "[ENV_LIGHT_EDIT_CYCLE] complete mode=reopened final_terrain_revision={}",
                    terrain_revision,
                );
                TestScenePhase::Ready
            }
            TestScenePhase::CapturingInflightStaleActive { .. }
            | TestScenePhase::Ready
            | TestScenePhase::Failed => return,
        };

        self.environment_lighting_test_scene
            .as_mut()
            .expect("test scene state disappeared")
            .phase = next_phase;
    }

    fn apply_environment_lighting_terrain_edit(
        &mut self,
        edit: TerrainEdit,
        source_revision: u32,
    ) -> Result<u32> {
        self.execute_edit_plan(
            skylight_edit_plan(edit).with_context(|| format!("compile {} edit", edit.label()))?,
        )?;
        let target_revision = self.visible_terrain_revision;
        anyhow::ensure!(
            target_revision != source_revision,
            "{} did not advance terrain revision from {}",
            edit.label(),
            source_revision,
        );
        log::info!(
            "[ENV_LIGHT_EDIT_CYCLE] requested edit={} source_revision={} target_revision={} voxel_bound={:?}..{:?}",
            edit.label(),
            source_revision,
            target_revision,
            SKYLIGHT_MIN.as_uvec3(),
            SKYLIGHT_MAX.as_uvec3(),
        );
        Ok(target_revision)
    }

    fn apply_point_light_blocker(&mut self, voxel_type: u32, source_revision: u32) -> Result<u32> {
        self.execute_edit_plan(point_light_blocker_edit_plan(voxel_type)?)?;
        let target_revision = self.visible_terrain_revision;
        anyhow::ensure!(
            target_revision != source_revision,
            "point-light blocker edit did not advance terrain revision from {}",
            source_revision,
        );
        log::info!(
            "[POINT_LIGHT_ACCEPT] action={} source_geometry_revision={} target_geometry_revision={} blocker_voxel_min={:?} blocker_voxel_max={:?} fixed_receiver_world={:?} fixed_receiver_normal={:?}",
            if voxel_type == VOXEL_TYPE_EMPTY {
                "remove-fixed-receiver-blocker"
            } else {
                "add-fixed-receiver-blocker"
            },
            source_revision,
            target_revision,
            POINT_LIGHT_BLOCKER_MIN.as_uvec3(),
            POINT_LIGHT_BLOCKER_MAX.as_uvec3(),
            POINT_LIGHT_FIXED_RECEIVER_WORLD,
            POINT_LIGHT_FIXED_RECEIVER_NORMAL,
        );
        Ok(target_revision)
    }

    fn apply_voxel_emissive_edits(
        &mut self,
        action: &str,
        edits: &[(UVec3, UVec3, u32)],
        source_revision: u32,
    ) -> Result<u32> {
        let plan = voxel_emissive_edit_plan(edits)?;
        let bound = match plan.build_edits[0] {
            BuildEdit::RebuildMesh(bound) => bound,
            _ => unreachable!("voxel-emissive edit helper only emits mesh rebuilds"),
        };
        self.execute_edit_plan(plan)?;
        let target_revision = self.visible_terrain_revision;
        anyhow::ensure!(
            target_revision != source_revision,
            "voxel-emissive {action} did not advance terrain revision from {source_revision}"
        );
        log::info!(
            "[VOXEL_EMISSIVE_ACCEPT] action={} source_geometry_revision={} target_geometry_revision={} trusted_voxel_min={:?} trusted_voxel_max={:?}",
            action,
            source_revision,
            target_revision,
            bound.min(),
            bound.max(),
        );
        Ok(target_revision)
    }

    fn apply_patt_seam_dig(&mut self, source_revision: u32) -> Result<u32> {
        let radius = super::TERRAIN_EDIT_DEFAULT_RADIUS;
        let mut removed_voxels = 0_u32;
        let mut productive_strokes = 0_usize;
        for _pass in 0..PATT_SEAM_DIG_PASSES {
            for center in PATT_SEAM_DIG_CENTERS {
                let readback = self.apply_surface_terrain_removal(
                    TerrainRemovalEdit { center, radius },
                    Some(VOXEL_TYPE_ROCK),
                    None,
                    None,
                )?;
                let stroke_removed: u32 = readback.stats.removed_counts.iter().sum();
                productive_strokes += usize::from(stroke_removed > 0);
                removed_voxels += stroke_removed;
            }
        }
        anyhow::ensure!(
            productive_strokes >= PATT_SEAM_DIG_CENTERS.len(),
            "patt seam shovel replay did not produce a complete pass",
        );
        let target_revision = self.visible_terrain_revision;
        anyhow::ensure!(
            target_revision != source_revision,
            "patt seam shovel replay did not advance terrain revision from {}",
            source_revision,
        );
        let center_min = PATT_SEAM_DIG_CENTERS
            .into_iter()
            .reduce(Vec3::min)
            .expect("patt seam replay must contain strokes");
        let center_max = PATT_SEAM_DIG_CENTERS
            .into_iter()
            .reduce(Vec3::max)
            .expect("patt seam replay must contain strokes");
        log::info!(
            "[DDGI_SEAM_REPRO] applied operation=apply_surface_terrain_removal target_voxel=rock passes={} strokes={} productive_strokes={} center_bounds=({:.3},{:.6},{:.3})..({:.3},{:.6},{:.3}) radius={:.6} removed_voxels={} source_revision={} target_revision={}",
            PATT_SEAM_DIG_PASSES,
            PATT_SEAM_DIG_PASSES * PATT_SEAM_DIG_CENTERS.len(),
            productive_strokes,
            center_min.x,
            center_min.y,
            center_min.z,
            center_max.x,
            center_max.y,
            center_max.z,
            radius,
            removed_voxels,
            source_revision,
            target_revision,
        );
        Ok(target_revision)
    }
}

fn is_terrain_edit_case(case: EnvironmentLightingTestCase) -> bool {
    matches!(
        case,
        EnvironmentLightingTestCase::TerrainEdits
            | EnvironmentLightingTestCase::DensityChanges
            | EnvironmentLightingTestCase::TerrainEditsInflight
            | EnvironmentLightingTestCase::TerrainEditsInflightCapture
            | EnvironmentLightingTestCase::TerrainEditsClosed
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ddgi::DdgiFieldKey;

    #[test]
    fn radiance_epochs_use_exact_distinct_palette_values() {
        let baseline = voxel_palette(EnvironmentLightingTestCase::RadianceChanges).rock;
        assert_ne!(baseline, RADIANCE_R2_ROCK_COLOR);
        assert_ne!(RADIANCE_R2_ROCK_COLOR, RADIANCE_R3_ROCK_COLOR);
        assert_ne!(RADIANCE_R3_ROCK_COLOR, RADIANCE_R4_ROCK_COLOR);
    }

    #[test]
    fn radiance_epoch_zero_restarts_from_the_exact_previous_field() {
        let r1_source = DdgiFieldKey::new(9, 1, 1, 32, DdgiFieldState::Converging, 3).unwrap();
        let r1 = DdgiFieldIdentity::new(
            DdgiFieldKey::new(10, 1, 1, 32, DdgiFieldState::Converged, 4).unwrap(),
            Some(r1_source),
        )
        .unwrap();
        let r2 = DdgiFieldIdentity::new(
            DdgiFieldKey::new(11, 1, 2, 32, DdgiFieldState::Converging, 0).unwrap(),
            Some(r1.field()),
        )
        .unwrap();

        assert_radiance_epoch_zero(r2, r1, 2);
        assert_eq!(r2.field().serial(), r1.field().serial() + 1);
    }

    #[test]
    fn density_initial_epoch_has_no_old_geometry_source() {
        let epoch_zero = DdgiFieldIdentity::new(
            DdgiFieldKey::new(20, 2, 1, 16, DdgiFieldState::Converging, 0).unwrap(),
            None,
        )
        .unwrap();

        assert_initial_epoch_zero(epoch_zero, 2, 1, 16);
    }

    #[test]
    fn inflight_stale_active_checkpoint_is_capture_ready_without_becoming_final_ready() {
        let mut scene = EnvironmentLightingTestScene::new(
            EnvironmentLightingTestCase::TerrainEditsInflightCapture,
        );
        scene.phase = TestScenePhase::CapturingInflightStaleActive { target_revision: 3 };

        assert!(scene.is_capture_ready());
        assert!(!scene.is_ready());
        assert_eq!(scene.inflight_capture_target_revision(), Some(3));
        assert_eq!(scene.edit_cycle_target_revision(), Some(3));
    }

    #[test]
    fn sealed_interior_stays_inside_shell() {
        assert!(INTERIOR_MIN.cmpgt(SHELL_MIN).all());
        assert!(INTERIOR_MAX.cmplt(SHELL_MAX).all());
    }

    #[test]
    fn portal_only_breaks_through_the_roof() {
        const { assert!(SKYLIGHT_MIN.x > INTERIOR_MIN.x) };
        const { assert!(SKYLIGHT_MAX.x < INTERIOR_MAX.x) };
        assert_eq!(SKYLIGHT_MIN.y, INTERIOR_MAX.y);
        const { assert!(SKYLIGHT_MAX.y > SHELL_MAX.y) };
        const { assert!(SKYLIGHT_MIN.z > INTERIOR_MIN.z) };
        const { assert!(SKYLIGHT_MAX.z < INTERIOR_MAX.z) };
    }

    #[test]
    fn patt_seam_replay_uses_the_saved_snapshot_and_only_punches_the_roof() {
        let snapshots = crate::app::camera_snapshots::CameraSnapshotLibrary::load(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("config/camera_snapshots.toml"),
        )
        .unwrap();
        assert_eq!(snapshots.snapshots().len(), 1);
        let snapshot = snapshots
            .find("snapshot")
            .expect("saved snapshot must exist");
        let camera_position = Vec3::from_array(snapshot.position);
        assert_eq!(
            PATT_SEAM_DIG_CENTERS,
            [
                Vec3::new(0.58, 226.0 / 256.0, 1.10),
                Vec3::new(0.52, 226.0 / 256.0, 1.20),
            ]
        );
        assert_eq!(
            test_lighting(EnvironmentLightingTestCase::PattSeam),
            (0.49, -0.07, 0.29)
        );
        assert_eq!(RADIANCE_R1_SUN_LUMINANCE, 1.65);
        assert!(
            PATT_SEAM_DIG_PASSES > (SHELL_MAX.y - INTERIOR_MAX.y) as usize,
            "surface-only shovel replay needs more passes than roof voxel layers",
        );
        let radius = super::super::TERRAIN_EDIT_DEFAULT_RADIUS;
        for center in PATT_SEAM_DIG_CENTERS {
            let camera_to_dig = center - camera_position;
            assert!(camera_to_dig.y > 0.0);

            let dig_min = center - Vec3::splat(radius);
            let dig_max = center + Vec3::splat(radius);
            assert!(dig_min.y * VOXELS_PER_WORLD_UNIT < INTERIOR_MAX.y);
            assert!(dig_max.y * VOXELS_PER_WORLD_UNIT > SHELL_MAX.y);
            assert!(dig_min.x * VOXELS_PER_WORLD_UNIT >= INTERIOR_MIN.x);
            assert!(dig_max.x * VOXELS_PER_WORLD_UNIT <= INTERIOR_MAX.x);
            assert!(dig_min.z * VOXELS_PER_WORLD_UNIT >= INTERIOR_MIN.z);
            assert!(dig_max.z * VOXELS_PER_WORLD_UNIT <= INTERIOR_MAX.z);
        }
    }

    #[test]
    fn patt_seam_sun_projects_through_the_opening_into_the_visible_interior() {
        let (time_of_day, latitude, season) = test_lighting(EnvironmentLightingTestCase::PattSeam);
        let (sun_altitude, sun_azimuth) =
            crate::app::environment::calculate_sun_position(time_of_day, latitude, season);
        let incoming =
            -crate::util::get_sun_dir(sun_altitude.asin().to_degrees(), sun_azimuth * 360.0);
        let floor_y = INTERIOR_MIN.y / VOXELS_PER_WORLD_UNIT;
        let floor_hits = PATT_SEAM_DIG_CENTERS.map(|center| {
            let distance = (floor_y - center.y) / incoming.y;
            let floor_hit = center + incoming * distance;

            assert!(distance > 0.0);
            assert!((floor_hit.y - floor_y).abs() < 1.0e-5);
            assert!(floor_hit.x * VOXELS_PER_WORLD_UNIT >= INTERIOR_MIN.x);
            assert!(floor_hit.x * VOXELS_PER_WORLD_UNIT <= INTERIOR_MAX.x);
            assert!(floor_hit.z * VOXELS_PER_WORLD_UNIT >= INTERIOR_MIN.z);
            assert!(floor_hit.z * VOXELS_PER_WORLD_UNIT <= INTERIOR_MAX.z);
            floor_hit
        });
        assert!(floor_hits[0].x > floor_hits[1].x);
        assert!(floor_hits[0].z < floor_hits[1].z);

        let sun_direction = -incoming;
        let radius = super::super::TERRAIN_EDIT_DEFAULT_RADIUS;
        for ray_anchor in PATT_SEAM_DIG_CENTERS {
            for step in 0..=256 {
                let roof_y = (INTERIOR_MAX.y
                    + (SHELL_MAX.y - INTERIOR_MAX.y) * step as f32 / 256.0)
                    / VOXELS_PER_WORLD_UNIT;
                let distance = (roof_y - ray_anchor.y) / sun_direction.y;
                let roof_point = ray_anchor + sun_direction * distance;
                assert!(
                    PATT_SEAM_DIG_CENTERS
                        .iter()
                        .any(|center| roof_point.distance(*center) <= radius + 1.0e-5),
                    "sun ray leaves the carved roof tunnel at {roof_point:?}",
                );
            }
        }
    }

    #[test]
    fn thin_wall_cases_have_exact_voxel_thicknesses() {
        assert_eq!(ONE_VOXEL_WALL_MAX.z - ONE_VOXEL_WALL_MIN.z, 1.0);
        assert_eq!(TWO_VOXEL_WALL_MAX.z - TWO_VOXEL_WALL_MIN.z, 2.0);
    }

    #[test]
    fn point_light_blocker_crosses_only_the_fixed_receiver_segment() {
        let receiver_voxel = POINT_LIGHT_FIXED_RECEIVER_WORLD * VOXELS_PER_WORLD_UNIT;
        let light_voxel = POINT_LIGHT_ADD_POSITION * VOXELS_PER_WORLD_UNIT;
        assert!(receiver_voxel.y < POINT_LIGHT_BLOCKER_MIN.y);
        assert!(POINT_LIGHT_BLOCKER_MAX.y < light_voxel.y);
        assert!(
            receiver_voxel.x >= POINT_LIGHT_BLOCKER_MIN.x
                && receiver_voxel.x <= POINT_LIGHT_BLOCKER_MAX.x
        );
        assert!(
            receiver_voxel.z >= POINT_LIGHT_BLOCKER_MIN.z
                && receiver_voxel.z <= POINT_LIGHT_BLOCKER_MAX.z
        );
        for voxel_type in [VOXEL_TYPE_ROCK, VOXEL_TYPE_EMPTY] {
            let plan = point_light_blocker_edit_plan(voxel_type).unwrap();
            assert_eq!(plan.voxel_edits.len(), 1);
            assert_eq!(plan.build_edits.len(), 1);
            assert!(matches!(
                plan.build_edits[0],
                BuildEdit::RebuildMesh(bound)
                    if bound.min() == POINT_LIGHT_BLOCKER_MIN.as_uvec3()
                        && bound.max() == POINT_LIGHT_BLOCKER_MAX.as_uvec3()
            ));
        }
    }

    #[test]
    fn all_test_scene_plans_are_static_and_bounded() {
        for case in [
            EnvironmentLightingTestCase::Sealed,
            EnvironmentLightingTestCase::PattSeam,
            EnvironmentLightingTestCase::Portal,
            EnvironmentLightingTestCase::Walls,
            EnvironmentLightingTestCase::Donor,
            EnvironmentLightingTestCase::Dogleg,
            EnvironmentLightingTestCase::RadianceChanges,
            EnvironmentLightingTestCase::PointLightChanges,
            EnvironmentLightingTestCase::VoxelEmissiveChanges,
            EnvironmentLightingTestCase::RasterEmitterChanges,
            EnvironmentLightingTestCase::MultiSourceStress,
            EnvironmentLightingTestCase::LocalLightScaling,
            EnvironmentLightingTestCase::DensityChanges,
            EnvironmentLightingTestCase::TerrainEdits,
            EnvironmentLightingTestCase::TerrainEditsInflight,
            EnvironmentLightingTestCase::TerrainEditsClosed,
        ] {
            let plan = TestSceneGeometry::build(case).compile().unwrap();
            assert!(!plan.voxel_edits.is_empty());
            assert_eq!(plan.build_edits.len(), 1);
            assert!(plan.build_edits.iter().all(|edit| match edit {
                BuildEdit::RebuildMesh(bound) => bound.max().cmple(UVec3::splat(512)).all(),
                _ => false,
            }));
        }
    }

    #[test]
    fn donor_scene_has_saturated_terrain_donor_and_neutral_control_materials() {
        let palette = voxel_palette(EnvironmentLightingTestCase::Donor);
        assert!(palette.sand.r() > palette.sand.g() * 4);
        assert!(palette.sand.r() > palette.sand.b() * 4);
        assert!((palette.rock.r() as i16 - palette.rock.b() as i16).abs() <= 6);

        let geometry = TestSceneGeometry::build(EnvironmentLightingTestCase::Donor);
        assert_eq!(geometry.sand.len(), 1);
        assert_eq!(geometry.sand[0].aabb().min(), DONOR_SLAB_MIN);
        assert_eq!(geometry.sand[0].aabb().max(), DONOR_SLAB_MAX);
    }

    #[test]
    fn donor_receiver_bays_are_symmetric_and_spacing_32_robust() {
        let donor_receiver_size = DONOR_RECEIVER_ROI_MAX - DONOR_RECEIVER_ROI_MIN;
        let control_receiver_size = DONOR_CONTROL_RECEIVER_ROI_MAX - DONOR_CONTROL_RECEIVER_ROI_MIN;
        assert_eq!(donor_receiver_size, control_receiver_size);
        assert_eq!(
            DONOR_SLAB_MAX - DONOR_SLAB_MIN,
            DONOR_CONTROL_SLAB_MAX - DONOR_CONTROL_SLAB_MIN
        );
        assert_eq!(
            DONOR_LEFT_ROOF_MAX - DONOR_LEFT_ROOF_MIN,
            DONOR_RIGHT_ROOF_MAX - DONOR_RIGHT_ROOF_MIN
        );
        const { assert!(DONOR_LEFT_ROOF_MAX.y - DONOR_LEFT_ROOF_MIN.y >= 64.0) };
        const { assert!(DONOR_DIVIDER_MAX.x - DONOR_DIVIDER_MIN.x >= 64.0) };
        const { assert!(DONOR_BACK_MAX.z - DONOR_BACK_MIN.z >= 32.0) };
    }

    #[test]
    fn donor_receivers_are_sun_occluded_while_donor_top_is_exposed() {
        let (sun_altitude, sun_azimuth) = crate::app::environment::calculate_sun_position(
            TEST_TIME_OF_DAY,
            TEST_LATITUDE,
            TEST_SEASON,
        );
        let sun_dir =
            crate::util::get_sun_dir(sun_altitude.asin().to_degrees(), sun_azimuth * 360.0);
        assert!(sun_dir.y > 0.0);

        let at_roof_height = |surface_point: Vec3| {
            let distance = (DONOR_LEFT_ROOF_MIN.y + 1.0 - surface_point.y) / sun_dir.y;
            surface_point + sun_dir * distance
        };
        for (receiver_min, receiver_max, roof_min, roof_max) in [
            (
                DONOR_RECEIVER_ROI_MIN,
                DONOR_RECEIVER_ROI_MAX,
                DONOR_LEFT_ROOF_MIN,
                DONOR_LEFT_ROOF_MAX,
            ),
            (
                DONOR_CONTROL_RECEIVER_ROI_MIN,
                DONOR_CONTROL_RECEIVER_ROI_MAX,
                DONOR_RIGHT_ROOF_MIN,
                DONOR_RIGHT_ROOF_MAX,
            ),
        ] {
            for x in [receiver_min.x, receiver_max.x] {
                for y in [receiver_min.y, receiver_max.y] {
                    let sun_ray = at_roof_height(Vec3::new(x, y, receiver_min.z));
                    assert!(sun_ray.cmpge(roof_min).all());
                    assert!(sun_ray.cmple(roof_max).all());
                }
            }
        }

        for x in [DONOR_SURFACE_ROI_MIN.x, DONOR_SURFACE_ROI_MAX.x] {
            for z in [DONOR_SURFACE_ROI_MIN.z, DONOR_SURFACE_ROI_MAX.z] {
                let donor_top_sun_ray = at_roof_height(Vec3::new(x, DONOR_SURFACE_ROI_MIN.y, z));
                assert!(donor_top_sun_ray.z > DONOR_LEFT_ROOF_MAX.z);
            }
        }
    }

    #[test]
    fn donor_capture_regions_stay_on_authored_surfaces() {
        for point in [DONOR_RECEIVER_ROI_MIN, DONOR_RECEIVER_ROI_MAX] {
            assert!(point.x >= DONOR_BACK_MIN.x && point.x <= DONOR_BACK_MAX.x);
            assert!(point.y >= DONOR_BACK_MIN.y && point.y <= DONOR_BACK_MAX.y);
            assert_eq!(point.z, DONOR_BACK_MAX.z);
        }
        for point in [
            DONOR_CONTROL_RECEIVER_ROI_MIN,
            DONOR_CONTROL_RECEIVER_ROI_MAX,
        ] {
            assert!(point.x >= DONOR_BACK_MIN.x && point.x <= DONOR_BACK_MAX.x);
            assert!(point.y >= DONOR_BACK_MIN.y && point.y <= DONOR_BACK_MAX.y);
            assert_eq!(point.z, DONOR_BACK_MAX.z);
        }
        for point in [DONOR_SURFACE_ROI_MIN, DONOR_SURFACE_ROI_MAX] {
            assert!(point.x >= DONOR_SLAB_MIN.x && point.x <= DONOR_SLAB_MAX.x);
            assert_eq!(point.y, DONOR_SLAB_MAX.y);
            assert!(point.z >= DONOR_SLAB_MIN.z && point.z <= DONOR_SLAB_MAX.z);
        }
    }

    #[test]
    fn donor_camera_center_ray_crosses_the_receiver_roi() {
        let (position, target) = camera_pose(EnvironmentLightingTestCase::Donor);
        let receiver_plane_z = DONOR_RECEIVER_ROI_MIN.z / VOXELS_PER_WORLD_UNIT;
        let distance_fraction = (receiver_plane_z - position.z) / (target.z - position.z);
        let receiver_plane_point = position + distance_fraction * (target - position);
        let (receiver_min, receiver_max) =
            voxel_roi_to_world(DONOR_RECEIVER_ROI_MIN, DONOR_RECEIVER_ROI_MAX);

        assert!((0.0..=1.0).contains(&distance_fraction));
        assert!(receiver_plane_point.x >= receiver_min.x);
        assert!(receiver_plane_point.x <= receiver_max.x);
        assert!(receiver_plane_point.y >= receiver_min.y);
        assert!(receiver_plane_point.y <= receiver_max.y);
    }

    #[test]
    fn donor_capture_regions_have_explicit_world_space_contract() {
        for (min_voxel, max_voxel) in [
            (DONOR_RECEIVER_ROI_MIN, DONOR_RECEIVER_ROI_MAX),
            (
                DONOR_CONTROL_RECEIVER_ROI_MIN,
                DONOR_CONTROL_RECEIVER_ROI_MAX,
            ),
            (DONOR_SURFACE_ROI_MIN, DONOR_SURFACE_ROI_MAX),
        ] {
            let (min_world, max_world) = voxel_roi_to_world(min_voxel, max_voxel);
            assert!(min_world.cmpge(Vec3::ZERO).all());
            assert!(max_world.cmple(Vec3::splat(2.0)).all());
            assert_eq!(min_world * VOXELS_PER_WORLD_UNIT, min_voxel);
            assert_eq!(max_world * VOXELS_PER_WORLD_UNIT, max_voxel);
        }
    }

    fn dogleg_point_is_carved(point: Vec3) -> bool {
        [
            (DOGLEG_LIGHT_WELL_MIN, DOGLEG_LIGHT_WELL_MAX),
            (DOGLEG_FIRST_LEG_MIN, DOGLEG_FIRST_LEG_MAX),
            (DOGLEG_SECOND_LEG_MIN, DOGLEG_SECOND_LEG_MAX),
            (DOGLEG_RECEIVER_CHAMBER_MIN, DOGLEG_RECEIVER_CHAMBER_MAX),
        ]
        .into_iter()
        .any(|(min, max)| point.cmpge(min).all() && point.cmple(max).all())
    }

    fn dogleg_segment_stays_in_carved_space(start: Vec3, end: Vec3) -> bool {
        (1..2_048).all(|step| {
            let fraction = step as f32 / 2_048.0;
            dogleg_point_is_carved(start.lerp(end, fraction))
        })
    }

    #[test]
    fn dogleg_scene_has_one_saturated_source_and_neutral_later_surfaces() {
        let palette = voxel_palette(EnvironmentLightingTestCase::Dogleg);
        assert!(palette.sand.r() > palette.sand.g() * 4);
        assert!(palette.sand.r() > palette.sand.b() * 4);
        assert!((palette.rock.r() as i16 - palette.rock.b() as i16).abs() <= 6);

        let geometry = TestSceneGeometry::build(EnvironmentLightingTestCase::Dogleg);
        assert_eq!(geometry.sand.len(), 1);
        assert_eq!(geometry.sand[0].aabb().min(), DOGLEG_FIRST_REFLECTOR_MIN);
        assert_eq!(geometry.sand[0].aabb().max(), DOGLEG_FIRST_REFLECTOR_MAX);
    }

    #[test]
    fn dogleg_blockers_are_spacing_16_and_spacing_32_robust() {
        const { assert!(DOGLEG_BLOCK_MAX.y - DOGLEG_FIRST_LEG_MAX.y >= 64.0) };
        const { assert!(DOGLEG_RECEIVER_CHAMBER_MIN.z - DOGLEG_BLOCK_MIN.z >= 32.0) };
        const { assert!(DOGLEG_SECOND_LEG_MIN.x - DOGLEG_FIRST_LEG_MIN.x >= 64.0) };
        const { assert!(DOGLEG_BLOCK_MAX.x - DOGLEG_FIRST_LEG_MAX.x >= 64.0) };
    }

    #[test]
    fn dogleg_requires_two_ordered_straight_transport_segments() {
        let first =
            (DOGLEG_FIRST_REFLECTOR_ROI_MIN + DOGLEG_FIRST_REFLECTOR_ROI_MAX) * 0.5 + Vec3::Y;
        let second =
            (DOGLEG_SECOND_REFLECTOR_ROI_MIN + DOGLEG_SECOND_REFLECTOR_ROI_MAX) * 0.5 - Vec3::X;
        let receiver =
            (DOGLEG_FINAL_RECEIVER_ROI_MIN + DOGLEG_FINAL_RECEIVER_ROI_MAX) * 0.5 + Vec3::Z;

        assert!(dogleg_segment_stays_in_carved_space(first, second));
        assert!(dogleg_segment_stays_in_carved_space(second, receiver));
        assert!(!dogleg_segment_stays_in_carved_space(first, receiver));
    }

    #[test]
    fn dogleg_first_reflector_is_sun_exposed_but_later_surfaces_are_roofed() {
        let (sun_altitude, sun_azimuth) = crate::app::environment::calculate_sun_position(
            TEST_TIME_OF_DAY,
            TEST_LATITUDE,
            TEST_SEASON,
        );
        let sun_dir =
            crate::util::get_sun_dir(sun_altitude.asin().to_degrees(), sun_azimuth * 360.0);
        assert!(sun_dir.y > 0.0);
        let at_height =
            |point: Vec3, height: f32| point + sun_dir * ((height - point.y) / sun_dir.y);

        let first = (DOGLEG_FIRST_REFLECTOR_ROI_MIN + DOGLEG_FIRST_REFLECTOR_ROI_MAX) * 0.5;
        let first_at_top = at_height(first, DOGLEG_BLOCK_MAX.y + 1.0);
        assert!(first_at_top.x >= DOGLEG_LIGHT_WELL_MIN.x);
        assert!(first_at_top.x <= DOGLEG_LIGHT_WELL_MAX.x);
        assert!(first_at_top.z >= DOGLEG_LIGHT_WELL_MIN.z);
        assert!(first_at_top.z <= DOGLEG_LIGHT_WELL_MAX.z);

        for (surface, roof_height) in [
            (
                (DOGLEG_SECOND_REFLECTOR_ROI_MIN + DOGLEG_SECOND_REFLECTOR_ROI_MAX) * 0.5 - Vec3::X,
                DOGLEG_FIRST_LEG_MAX.y + 1.0,
            ),
            (
                (DOGLEG_FINAL_RECEIVER_ROI_MIN + DOGLEG_FINAL_RECEIVER_ROI_MAX) * 0.5 + Vec3::Z,
                DOGLEG_RECEIVER_CHAMBER_MAX.y + 1.0,
            ),
        ] {
            let roof_hit = at_height(surface, roof_height);
            assert!(roof_hit.cmpge(DOGLEG_BLOCK_MIN).all());
            assert!(roof_hit.cmple(DOGLEG_BLOCK_MAX).all());
            assert!(!dogleg_point_is_carved(roof_hit));
        }
    }

    #[test]
    fn dogleg_capture_regions_have_explicit_world_space_contract() {
        for (min_voxel, max_voxel) in [
            (
                DOGLEG_FIRST_REFLECTOR_ROI_MIN,
                DOGLEG_FIRST_REFLECTOR_ROI_MAX,
            ),
            (
                DOGLEG_SECOND_REFLECTOR_ROI_MIN,
                DOGLEG_SECOND_REFLECTOR_ROI_MAX,
            ),
            (DOGLEG_FINAL_RECEIVER_ROI_MIN, DOGLEG_FINAL_RECEIVER_ROI_MAX),
        ] {
            let (min_world, max_world) = voxel_roi_to_world(min_voxel, max_voxel);
            assert!(min_world.cmpge(Vec3::ZERO).all());
            assert!(max_world.cmple(Vec3::splat(2.0)).all());
            assert_eq!(min_world * VOXELS_PER_WORLD_UNIT, min_voxel);
            assert_eq!(max_world * VOXELS_PER_WORLD_UNIT, max_voxel);
        }
    }

    #[test]
    fn terrain_edit_cycle_closes_and_reopens_the_authored_skylight() {
        let close = skylight_edit_plan(TerrainEdit::CloseSkylight).unwrap();
        let reopen = skylight_edit_plan(TerrainEdit::ReopenSkylight).unwrap();

        assert_eq!(close.voxel_edits.len(), 1);
        assert_eq!(reopen.voxel_edits.len(), 1);
        for plan in [close, reopen] {
            assert_eq!(plan.build_edits.len(), 1);
            assert!(matches!(
                plan.build_edits[0],
                BuildEdit::RebuildMesh(bound)
                    if bound.min() == SKYLIGHT_MIN.as_uvec3()
                        && bound.max() == SKYLIGHT_MAX.as_uvec3()
            ));
        }
        assert_eq!(TerrainEdit::CloseSkylight.voxel_type(), VOXEL_TYPE_ROCK);
        assert_eq!(TerrainEdit::ReopenSkylight.voxel_type(), VOXEL_TYPE_EMPTY);
    }

    #[test]
    fn point_light_scene_contains_a_real_emissive_surface_for_gpu_transport_evidence() {
        let plan = TestSceneGeometry::build(EnvironmentLightingTestCase::PointLightChanges)
            .compile()
            .unwrap();
        assert!(plan.voxel_edits.iter().any(|edit| matches!(
            edit,
            VoxelEdit::StampCuboids { voxel_type, .. }
                if *voxel_type == crate::builder::VOXEL_TYPE_EMISSIVE
        )));
    }

    #[test]
    fn voxel_emissive_lifecycle_edits_use_exact_trusted_bounds() {
        let add = voxel_emissive_edit_plan(&[(
            VOXEL_EMISSIVE_PRIMARY_MIN,
            VOXEL_EMISSIVE_PRIMARY_MAX,
            VOXEL_TYPE_EMISSIVE,
        )])
        .expect("bounded voxel add plan must compile");
        assert!(matches!(
            add.build_edits.as_slice(),
            [BuildEdit::RebuildMesh(bound)]
                if bound.min() == VOXEL_EMISSIVE_PRIMARY_MIN
                    && bound.max() == VOXEL_EMISSIVE_PRIMARY_MAX
        ));

        let moved = voxel_emissive_edit_plan(&[
            (
                VOXEL_EMISSIVE_PRIMARY_MIN,
                VOXEL_EMISSIVE_SECONDARY_MAX,
                VOXEL_TYPE_EMPTY,
            ),
            (
                VOXEL_EMISSIVE_MOVED_MIN,
                VOXEL_EMISSIVE_MOVED_MAX,
                VOXEL_TYPE_EMISSIVE,
            ),
        ])
        .expect("move plan must compile");
        assert!(matches!(
            moved.build_edits.as_slice(),
            [BuildEdit::RebuildMesh(bound)]
                if bound.min() == VOXEL_EMISSIVE_PRIMARY_MIN
                    && bound.max() == VOXEL_EMISSIVE_MOVED_MAX
        ));
        assert_eq!(moved.voxel_edits.len(), 2);
    }

    #[test]
    fn multi_source_scene_uses_one_fixed_world_space_receiver_and_swappable_point_contracts() {
        let authored = multi_source_authored_light();
        let LocalLight::Point(authored) = authored else {
            panic!("authored multi-source fixture must be a point light")
        };
        let raster = multi_source_raster_component();
        let LocalLight::Point(raster) = raster.light() else {
            panic!("raster multi-source fixture must be a point light")
        };

        for point in [authored, raster] {
            let to_light = point.position - POINT_LIGHT_FIXED_RECEIVER_WORLD;
            assert!(to_light.dot(POINT_LIGHT_FIXED_RECEIVER_NORMAL) > 0.0);
            assert!(to_light.length() < point.range);
            assert_eq!(point.source_radius, POINT_LIGHT_SOURCE_RADIUS_WORLD);
            assert_eq!(point.range, POINT_LIGHT_RANGE_WORLD);
        }
        assert_ne!(authored.position, raster.position);
        assert_ne!(authored.color, raster.color);
        assert_ne!(authored.intensity, raster.intensity);
        assert!(vec3_near(
            authored.color * authored.intensity + raster.color * raster.intensity,
            raster.color * raster.intensity + authored.color * authored.intensity,
            f32::EPSILON,
        ));
    }
}
