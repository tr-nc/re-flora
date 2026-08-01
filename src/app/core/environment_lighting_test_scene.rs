use super::App;
use crate::app::world_edits::{BuildEdit, VoxelEdit, WorldEditPlan};
use crate::builder::{VOXEL_TYPE_EMPTY, VOXEL_TYPE_ROCK, VOXEL_TYPE_SAND};
use crate::ddgi::{
    DdgiBuildKind, DdgiFieldIdentity, DdgiFieldStage, DdgiRefreshState, DdgiScheduledWorkKind,
    DdgiVolumeStage, DDGI_PROBE_BATCH_SIZE,
};
use crate::geom::{build_bvh, Cuboid, UAabb3};
use crate::EnvironmentLightingTestCase;
use anyhow::{Context, Result};
use egui::Color32;
use glam::{UVec3, Vec3};

const BUILD_DELAY_SECONDS: f32 = 0.5;
const SETTLE_FRAMES: u8 = 2;
const TEST_TIME_OF_DAY: f32 = 0.455_705;
const TEST_LATITUDE: f32 = -0.24;
const TEST_SEASON: f32 = 0.25;
const TEST_VOXEL_COLOR_VARIANCE: f32 = 0.0;
const VOXELS_PER_WORLD_UNIT: f32 = 256.0;
const RADIANCE_R2_ROCK_COLOR: Color32 = Color32::from_rgb(126, 125, 128);
const RADIANCE_R3_ROCK_COLOR: Color32 = Color32::from_rgb(130, 125, 128);
const RADIANCE_R4_ROCK_COLOR: Color32 = Color32::from_rgb(134, 125, 128);

pub(super) const STARTUP_TREE_POSITION: Vec3 = Vec3::new(1.72, 0.2, 0.62);

const SHELL_MIN: Vec3 = Vec3::new(96.0, 84.0, 216.0);
const SHELL_MAX: Vec3 = Vec3::new(238.0, 236.0, 392.0);
const INTERIOR_MIN: Vec3 = Vec3::new(112.0, 100.0, 242.0);
const INTERIOR_MAX: Vec3 = Vec3::new(222.0, 216.0, 376.0);
const SKYLIGHT_MIN: Vec3 = Vec3::new(144.0, 216.0, 270.0);
const SKYLIGHT_MAX: Vec3 = Vec3::new(192.0, 244.0, 334.0);

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TestScenePhase {
    Pending,
    WaitingForRebuild,
    Settling {
        frames: u8,
        terrain_revision: u32,
    },
    WaitingForProbeField {
        terrain_revision: u32,
    },
    WaitingForRadianceBaseline {
        terrain_revision: u32,
    },
    WaitingForRadianceR2Midflight {
        r1: DdgiFieldIdentity,
    },
    WaitingForRadianceR3Observed {
        r1: DdgiFieldIdentity,
        r2: DdgiFieldIdentity,
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
    WaitingForEditedTerrain {
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
    CapturingInflightFailClosed {
        target_revision: u32,
    },
    Ready,
    Failed,
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

fn is_terminal_field(field: DdgiFieldIdentity) -> bool {
    matches!(
        field.field().stage(),
        DdgiFieldStage::Converged | DdgiFieldStage::NonConverged
    )
}

fn assert_radiance_s2(field: DdgiFieldIdentity, source: DdgiFieldIdentity, radiance_revision: u32) {
    let key = field.field();
    let source_key = source.field();
    assert_eq!(key.stage(), DdgiFieldStage::Feedback);
    assert_eq!(key.iteration(), 2);
    assert_eq!(key.geometry_revision(), source_key.geometry_revision());
    assert_eq!(key.spacing_voxels(), source_key.spacing_voxels());
    assert_eq!(key.radiance_revision(), radiance_revision);
    assert_eq!(field.source(), Some(source_key));
}

fn assert_bootstrap_s1(
    field: DdgiFieldIdentity,
    geometry_revision: u32,
    radiance_revision: u32,
    spacing_voxels: u32,
) {
    let key = field.field();
    let source = field.source().expect("S1 must consume an S0 field");
    assert_eq!(key.geometry_revision(), geometry_revision);
    assert_eq!(key.radiance_revision(), radiance_revision);
    assert_eq!(key.spacing_voxels(), spacing_voxels);
    assert_eq!(key.stage(), DdgiFieldStage::SingleBounce);
    assert_eq!(key.iteration(), 1);
    assert_eq!(source.geometry_revision(), geometry_revision);
    assert_eq!(source.radiance_revision(), radiance_revision);
    assert_eq!(source.spacing_voxels(), spacing_voxels);
    assert_eq!(source.stage(), DdgiFieldStage::SeedSky);
    assert_eq!(source.iteration(), 0);
    assert_eq!(key.serial(), source.serial() + 1);
}

fn log_acceptance_field(group: &str, checkpoint: &str, field: DdgiFieldIdentity) {
    let key = field.field();
    let source = field.source();
    log::info!(
        "[DDGI_ACCEPT][{}] checkpoint={} field_serial={} geometry_revision={} radiance_revision={} spacing_voxels={} transport={:?} iteration={} source_field_serial={} source_radiance_revision={} source_transport={} source_iteration={}",
        group,
        checkpoint,
        key.serial(),
        key.geometry_revision(),
        key.radiance_revision(),
        key.spacing_voxels(),
        key.stage(),
        key.iteration(),
        source.map_or(0, |source| source.serial()),
        source.map_or(0, |source| source.radiance_revision()),
        source.map_or_else(|| "none".to_owned(), |source| format!("{:?}", source.stage())),
        source.map_or(0, |source| source.iteration()),
    );
}

#[derive(Debug)]
pub(super) struct EnvironmentLightingTestScene {
    case: EnvironmentLightingTestCase,
    phase: TestScenePhase,
}

impl EnvironmentLightingTestScene {
    pub(super) fn new(case: EnvironmentLightingTestCase) -> Self {
        Self {
            case,
            phase: TestScenePhase::Pending,
        }
    }

    pub(super) fn is_ready(&self) -> bool {
        self.phase == TestScenePhase::Ready
    }

    pub(super) fn is_capture_ready(&self) -> bool {
        self.is_ready()
            || matches!(
                self.phase,
                TestScenePhase::CapturingInflightFailClosed { .. }
            )
    }

    pub(super) fn inflight_capture_target_revision(&self) -> Option<u32> {
        match self.phase {
            TestScenePhase::CapturingInflightFailClosed { target_revision } => {
                Some(target_revision)
            }
            _ => None,
        }
    }

    pub(super) fn edit_cycle_target_revision(&self) -> Option<u32> {
        if !(is_terrain_edit_case(self.case)
            || self.case == EnvironmentLightingTestCase::RadianceChanges)
            || self.is_ready()
        {
            return None;
        }
        match self.phase {
            TestScenePhase::WaitingForEditedTerrain {
                target_revision, ..
            }
            | TestScenePhase::WaitingForEditedProbeField {
                target_revision, ..
            }
            | TestScenePhase::CapturingInflightFailClosed { target_revision } => {
                Some(target_revision)
            }
            TestScenePhase::WaitingForDensityRebuild { terrain_revision } => Some(terrain_revision),
            TestScenePhase::WaitingForRadianceBaseline { terrain_revision } => {
                Some(terrain_revision)
            }
            TestScenePhase::WaitingForRadianceR2Midflight { r1 }
            | TestScenePhase::WaitingForRadianceR3Observed { r1, .. }
            | TestScenePhase::WaitingForRadianceR4Midflight { r1, .. }
            | TestScenePhase::WaitingForRadianceR4Published { r1, .. } => {
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
            TestScenePhase::WaitingForRebuild => "waiting-for-initial-terrain",
            TestScenePhase::Settling { .. } => "settling-initial-terrain",
            TestScenePhase::WaitingForProbeField { .. } => "waiting-for-initial-probe-field",
            TestScenePhase::WaitingForRadianceBaseline { .. } => "waiting-for-radiance-baseline",
            TestScenePhase::WaitingForRadianceR2Midflight { .. } => {
                "waiting-for-radiance-r2-midflight"
            }
            TestScenePhase::WaitingForRadianceR3Observed { .. } => {
                "waiting-for-radiance-r3-observed"
            }
            TestScenePhase::WaitingForRadianceR4Midflight { .. } => {
                "waiting-for-radiance-r4-midflight"
            }
            TestScenePhase::WaitingForRadianceR4Published { .. } => {
                "waiting-for-radiance-r4-published"
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
            TestScenePhase::WaitingForEditedTerrain { .. } => "waiting-for-edited-terrain",
            TestScenePhase::WaitingForEditedProbeField { .. } => "waiting-for-edited-probe-field",
            TestScenePhase::WaitingForDensityRebuild { .. } => "waiting-for-density-rebuild",
            TestScenePhase::CapturingInflightFailClosed { .. } => "capturing-inflight-fail-closed",
            TestScenePhase::Ready => "ready",
            TestScenePhase::Failed => "failed",
        }
    }
}

struct TestSceneGeometry {
    cleared_startup_obstacle: Vec<Cuboid>,
    cleared_test_scene: Vec<Cuboid>,
    rock: Vec<Cuboid>,
    carved_empty: Vec<Cuboid>,
    sand: Vec<Cuboid>,
    startup_obstacle_rebuild_bound: UAabb3,
    test_rebuild_bound: UAabb3,
}

fn test_rebuild_bound(case: EnvironmentLightingTestCase) -> UAabb3 {
    if case == EnvironmentLightingTestCase::Dogleg {
        UAabb3::new(DOGLEG_CLEAR_MIN.as_uvec3(), DOGLEG_CLEAR_MAX.as_uvec3())
    } else {
        UAabb3::new(TEST_REBUILD_MIN, TEST_REBUILD_MAX)
    }
}

impl TestSceneGeometry {
    fn build(case: EnvironmentLightingTestCase) -> Self {
        let (startup_obstacle_min, startup_obstacle_max) = App::debug_startup_block_bounds();
        let startup_obstacle = Cuboid::from_min_max(startup_obstacle_min, startup_obstacle_max);
        let startup_obstacle_aabb = startup_obstacle.aabb();

        let test_rebuild_bound = test_rebuild_bound(case);
        let (cleared_test_scene, rock, carved_empty, sand) = match case {
            EnvironmentLightingTestCase::Sealed => (
                Vec::new(),
                vec![Cuboid::from_min_max(SHELL_MIN, SHELL_MAX)],
                vec![Cuboid::from_min_max(INTERIOR_MIN, INTERIOR_MAX)],
                Vec::new(),
            ),
            EnvironmentLightingTestCase::Portal
            | EnvironmentLightingTestCase::RadianceChanges
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

        Self {
            cleared_startup_obstacle: vec![startup_obstacle],
            cleared_test_scene,
            rock,
            carved_empty,
            sand,
            startup_obstacle_rebuild_bound: UAabb3::new(
                startup_obstacle_aabb.min_uvec3(),
                startup_obstacle_aabb.max_uvec3(),
            ),
            test_rebuild_bound,
        }
    }

    fn compile(self) -> Result<WorldEditPlan> {
        let mut voxel_edits = vec![stamp_cuboids(
            self.cleared_startup_obstacle,
            VOXEL_TYPE_EMPTY,
        )?];
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
        Ok(WorldEditPlan {
            voxel_edits,
            build_edits: vec![
                BuildEdit::RebuildMesh(self.startup_obstacle_rebuild_bound),
                BuildEdit::RebuildMesh(self.test_rebuild_bound),
            ],
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

fn camera_pose(case: EnvironmentLightingTestCase) -> (Vec3, Vec3) {
    match case {
        EnvironmentLightingTestCase::Sealed => {
            (Vec3::new(0.65, 0.58, 1.38), Vec3::new(0.65, 0.64, 1.02))
        }
        EnvironmentLightingTestCase::Portal
        | EnvironmentLightingTestCase::RadianceChanges
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
            (Vec3::new(0.50, 0.29, 1.32), Vec3::new(0.50, 0.26, 0.56))
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
        self.current_time_of_day = TEST_TIME_OF_DAY;
        self.debug_settings.adjustables.time_of_day.value = TEST_TIME_OF_DAY;
        self.debug_settings.adjustables.latitude.value = TEST_LATITUDE;
        self.debug_settings.adjustables.season.value = TEST_SEASON;
        self.debug_settings.adjustables.auto_daynight_cycle.value = false;
        self.debug_settings.adjustables.voxel_dirt_color.value = palette.dirt;
        self.debug_settings.adjustables.voxel_sand_color.value = palette.sand;
        self.debug_settings
            .adjustables
            .voxel_cherry_wood_color
            .value = palette.cherry_wood;
        self.debug_settings.adjustables.voxel_oak_wood_color.value = palette.oak_wood;
        self.debug_settings.adjustables.voxel_rock_color.value = palette.rock;
        self.debug_settings.adjustables.voxel_color_variance.value = TEST_VOXEL_COLOR_VARIANCE;
        self.orbit_camera_focus = camera_target;
        if self
            .tracer
            .set_camera_pose_looking_at(camera_position, camera_target)
        {
            self.request_vsm_history_reset();
            log::info!(
                "[ENV_LIGHT_TEST] case={} camera position=({:.3},{:.3},{:.3}) target=({:.3},{:.3},{:.3}) time_of_day={:.6} latitude={:.3} season={:.3} auto_cycle=false voxel_color_variance={:.3}",
                case.label(),
                camera_position.x,
                camera_position.y,
                camera_position.z,
                camera_target.x,
                camera_target.y,
                camera_target.z,
                TEST_TIME_OF_DAY,
                TEST_LATITUDE,
                TEST_SEASON,
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
                    "[ENV_LIGHT_TEST_ROI] case=dogleg first_reflector_world={:?}..{:?} second_reflector_world={:?}..{:?} final_receiver_world={:?}..{:?} expected_first_signal_stage=S2 final_receiver_direct_sun=occluded",
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

    pub(super) fn process_environment_lighting_test_scene(&mut self) {
        let Some((case, phase)) = self
            .environment_lighting_test_scene
            .as_ref()
            .map(|scene| (scene.case, scene.phase))
        else {
            return;
        };

        let next_phase = match phase {
            TestScenePhase::Pending => {
                let Some(render_start) = self.render_start_time else {
                    return;
                };
                if render_start.elapsed().as_secs_f32() < BUILD_DELAY_SECONDS
                    || !self.deferred_chunk_rebuilds_idle()
                {
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
                        TestScenePhase::WaitingForRebuild
                    }
                    Err(err) => {
                        log::error!("[ENV_LIGHT_TEST] construction failed: {err:#}");
                        TestScenePhase::Failed
                    }
                }
            }
            TestScenePhase::WaitingForRebuild => {
                if !self.deferred_chunk_rebuilds_idle() {
                    return;
                }
                let terrain_revision = self.tracer.environment_probe_terrain_revision();
                match self
                    .tracer
                    .notify_ddgi_initial_terrain_ready(terrain_revision)
                {
                    Ok(()) => {
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
                    Err(err) => {
                        log::error!("[ENV_LIGHT_TEST] DDGI visibility publication failed: {err:#}");
                        TestScenePhase::Failed
                    }
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
                } else if case == EnvironmentLightingTestCase::DensityChanges {
                    let runtime = self.tracer.ddgi_runtime_status();
                    let baseline = runtime
                        .active_published_field
                        .expect("density lifecycle requires an initial published field");
                    assert_eq!(baseline.field().geometry_revision(), terrain_revision);
                    assert_eq!(runtime.active_spacing_voxels, 32);
                    assert!(!runtime.full_domain_invalidation_fail_closed);
                    log_acceptance_field("DENSITY", "baseline", baseline);
                    TestScenePhase::WaitingForDensityMidflight { baseline }
                } else if is_terrain_edit_case(case) {
                    log::info!(
                        "[ENV_LIGHT_EDIT_CYCLE] initial probe field ready terrain_revision={}",
                        terrain_revision,
                    );
                    match self.apply_environment_lighting_terrain_edit(
                        TerrainEdit::CloseSkylight,
                        terrain_revision,
                    ) {
                        Ok(target_revision) => TestScenePhase::WaitingForEditedTerrain {
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
                let status = self.tracer.ddgi_status();
                let active = status.active();
                let Some(r1) = active.published_field else {
                    return;
                };
                if !is_terminal_field(r1)
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
                self.debug_settings.adjustables.voxel_rock_color.value = RADIANCE_R2_ROCK_COLOR;
                log::info!(
                    "[DDGI_ACCEPT][RADIANCE] mutation=r2 rock_rgb={},{},{} expected_radiance_revision={}",
                    RADIANCE_R2_ROCK_COLOR.r(),
                    RADIANCE_R2_ROCK_COLOR.g(),
                    RADIANCE_R2_ROCK_COLOR.b(),
                    next_nonzero_revision(r1.field().radiance_revision()),
                );
                TestScenePhase::WaitingForRadianceR2Midflight { r1 }
            }
            TestScenePhase::WaitingForRadianceR2Midflight { r1 } => {
                let status = self.tracer.ddgi_status();
                assert!(status.staging().is_none());
                let active = status.active();
                assert_eq!(active.published_field, Some(r1));
                let Some(r2) = active.building_field else {
                    return;
                };
                let r2_revision = next_nonzero_revision(r1.field().radiance_revision());
                assert_radiance_s2(r2, r1, r2_revision);
                assert_eq!(r2.field().serial(), r1.field().serial() + 1);
                let work = active
                    .scheduled_work
                    .expect("r2 building field must retain scheduled work");
                assert_eq!(work.kind(), DdgiScheduledWorkKind::RadianceFeedback);
                assert_eq!(work.destination(), r2);
                let remaining = active
                    .grid
                    .probe_count()
                    .saturating_sub(active.filtered_probe_count);
                if active.filtered_probe_count == 0 || remaining <= 3 * DDGI_PROBE_BATCH_SIZE {
                    return;
                }
                assert!(
                    !self
                        .tracer
                        .ddgi_runtime_status()
                        .full_domain_invalidation_fail_closed
                );
                log::info!(
                    "[DDGI_ACCEPT][RADIANCE] checkpoint=r2-midflight active_field_serial={} active_radiance_revision={} building_field_serial={} building_radiance_revision={} building_iteration={} source_field_serial={} progress={}/{} old_field_visible=true",
                    r1.field().serial(),
                    r1.field().radiance_revision(),
                    r2.field().serial(),
                    r2.field().radiance_revision(),
                    r2.field().iteration(),
                    r2.source().expect("r2 must have source").serial(),
                    active.filtered_probe_count,
                    active.grid.probe_count(),
                );
                self.debug_settings.adjustables.voxel_rock_color.value = RADIANCE_R3_ROCK_COLOR;
                log::info!(
                    "[DDGI_ACCEPT][RADIANCE] mutation=r3 rock_rgb={},{},{} expected_radiance_revision={}",
                    RADIANCE_R3_ROCK_COLOR.r(),
                    RADIANCE_R3_ROCK_COLOR.g(),
                    RADIANCE_R3_ROCK_COLOR.b(),
                    next_nonzero_revision(r2_revision),
                );
                TestScenePhase::WaitingForRadianceR3Observed { r1, r2 }
            }
            TestScenePhase::WaitingForRadianceR3Observed { r1, r2 } => {
                let active = self.tracer.ddgi_status().active();
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
                self.debug_settings.adjustables.voxel_rock_color.value = RADIANCE_R4_ROCK_COLOR;
                log::info!(
                    "[DDGI_ACCEPT][RADIANCE] mutation=r4 rock_rgb={},{},{} expected_radiance_revision={}",
                    RADIANCE_R4_ROCK_COLOR.r(),
                    RADIANCE_R4_ROCK_COLOR.g(),
                    RADIANCE_R4_ROCK_COLOR.b(),
                    next_nonzero_revision(r3_revision),
                );
                TestScenePhase::WaitingForRadianceR4Midflight { r1, r2 }
            }
            TestScenePhase::WaitingForRadianceR4Midflight { r1, r2 } => {
                let active = self.tracer.ddgi_status().active();
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
                assert_radiance_s2(r4, r2, r4_revision);
                assert_eq!(
                    r4.field().serial(),
                    r2.field().serial() + 1,
                    "r3 must not claim work or allocate a field serial"
                );
                let work = active
                    .scheduled_work
                    .expect("r4 building field must retain scheduled work");
                assert_eq!(work.kind(), DdgiScheduledWorkKind::RadianceFeedback);
                assert_eq!(work.destination(), r4);
                if active.filtered_probe_count == 0 {
                    return;
                }
                log::info!(
                    "[DDGI_ACCEPT][RADIANCE] checkpoint=r4-midflight active_field_serial={} active_radiance_revision={} building_field_serial={} building_radiance_revision={} building_iteration={} source_field_serial={} progress={}/{} r3_coalesced=true old_field_visible=true",
                    r2.field().serial(),
                    r2.field().radiance_revision(),
                    r4.field().serial(),
                    r4.field().radiance_revision(),
                    r4.field().iteration(),
                    r4.source().expect("r4 must have source").serial(),
                    active.filtered_probe_count,
                    active.grid.probe_count(),
                );
                TestScenePhase::WaitingForRadianceR4Published { r1, r2, r4 }
            }
            TestScenePhase::WaitingForRadianceR4Published { r1, r2, r4 } => {
                let active = self.tracer.ddgi_status().active();
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
                TestScenePhase::Ready
            }
            TestScenePhase::WaitingForDensityMidflight { baseline } => {
                let status = self.tracer.ddgi_status();
                let runtime = self.tracer.ddgi_runtime_status();
                let active = status.active();
                assert_eq!(active.published_field, Some(baseline));
                assert_eq!(active.grid.spacing_voxels(), 32);
                assert!(!runtime.full_domain_invalidation_fail_closed);
                let Some(staging) = status.staging() else {
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
                assert_eq!(runtime.queued_density_spacing_voxels, None);
                assert!(matches!(
                    runtime.coordinator_state,
                    DdgiRefreshState::BuildingDensity {
                        candidate,
                        queued_spacing_voxels: None,
                    } if candidate == token
                ));
                let Some(work) = staging.scheduled_work else {
                    return;
                };
                assert_eq!(work.kind(), DdgiScheduledWorkKind::DensityBootstrap);
                let density_field = work.destination();
                assert_bootstrap_s1(
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
                    "[DDGI_ACCEPT][DENSITY] checkpoint=density-midflight active_token_serial={} active_field_serial={} active_geometry_revision={} active_spacing_voxels=32 obsolete_density_token_serial={} obsolete_density_field_serial={} obsolete_density_spacing_voxels=16 progress={}/{} old_field_visible=true invalidation=false",
                    runtime.active_token_serial.expect("initial active token missing"),
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
                let status = self.tracer.ddgi_status();
                let runtime = self.tracer.ddgi_runtime_status();
                let active = status.active();
                assert_eq!(active.published_field, Some(baseline));
                assert_eq!(active.grid.spacing_voxels(), 32);
                assert_ne!(
                    runtime.active_token_serial,
                    Some(obsolete_density_token_serial)
                );
                assert!(runtime.full_domain_invalidation_fail_closed);
                assert_eq!(runtime.target_terrain_revision, Some(target_revision));
                assert_eq!(runtime.queued_density_spacing_voxels, Some(16));
                let Some(staging) = status.staging() else {
                    return;
                };
                let token = staging
                    .build_token
                    .expect("replacement staging must have a build token");
                if token.serial() == obsolete_density_token_serial {
                    assert_eq!(token.kind(), DdgiBuildKind::Density);
                    assert_eq!(staging.grid.spacing_voxels(), 16);
                    assert!(matches!(
                        runtime.coordinator_state,
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
                    runtime.coordinator_state,
                    DdgiRefreshState::BuildingTerrain {
                        candidate,
                        latest_terrain_revision,
                    } if candidate == token && latest_terrain_revision == target_revision
                ));
                log::info!(
                    "[DDGI_ACCEPT][DENSITY] checkpoint=geometry-preempted-density obsolete_density_token_serial={} obsolete_density_field_serial={} terrain_token_serial={} target_geometry_revision={} terrain_spacing_voxels=32 queued_density_spacing_voxels=16 obsolete_density_consumer_visible=false invalidation=true",
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
                let status = self.tracer.ddgi_status();
                let runtime = self.tracer.ddgi_runtime_status();
                assert_ne!(
                    runtime.active_token_serial,
                    Some(obsolete_density_token_serial)
                );
                assert_ne!(
                    runtime.active_published_field,
                    Some(obsolete_density_field),
                    "obsolete density field became consumer-visible"
                );
                if runtime.active_token_serial != Some(terrain_token_serial) {
                    assert_eq!(status.active().published_field, Some(baseline));
                    assert_eq!(status.active().grid.spacing_voxels(), 32);
                    assert!(runtime.full_domain_invalidation_fail_closed);
                    return;
                }
                let geometry_field = status
                    .active()
                    .published_field
                    .expect("terrain S1 must be published before promotion");
                assert_bootstrap_s1(
                    geometry_field,
                    target_revision,
                    baseline.field().radiance_revision(),
                    32,
                );
                assert!(!runtime.full_domain_invalidation_fail_closed);
                assert_eq!(runtime.queued_density_spacing_voxels, Some(16));
                log_acceptance_field("DENSITY", "geometry-s1-published", geometry_field);
                log::info!(
                    "[DDGI_ACCEPT][DENSITY] checkpoint=geometry-s1-published terrain_token_serial={} obsolete_density_token_serial={} geometry_revision={} active_spacing_voxels=32 queued_density_spacing_voxels=16 obsolete_density_consumer_visible=false invalidation=false",
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
                let status = self.tracer.ddgi_status();
                let runtime = self.tracer.ddgi_runtime_status();
                let active = status.active();
                assert_eq!(active.published_field, Some(geometry_field));
                assert_eq!(runtime.active_token_serial, Some(terrain_token_serial));
                assert_eq!(active.grid.spacing_voxels(), 32);
                assert!(!runtime.full_domain_invalidation_fail_closed);
                assert_ne!(
                    runtime.active_token_serial,
                    Some(obsolete_density_token_serial)
                );
                let Some(staging) = status.staging() else {
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
                    runtime.coordinator_state,
                    DdgiRefreshState::BuildingDensity {
                        candidate,
                        queued_spacing_voxels: None,
                    } if candidate == token
                ));
                let Some(work) = staging.scheduled_work else {
                    return;
                };
                assert_eq!(work.kind(), DdgiScheduledWorkKind::DensityBootstrap);
                let density_field = work.destination();
                assert_bootstrap_s1(
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
                    "[DDGI_ACCEPT][DENSITY] checkpoint=density-retry-midflight active_token_serial={} active_field_serial={} active_geometry_revision={} active_spacing_voxels=32 density_token_serial={} density_field_serial={} density_spacing_voxels=16 progress={}/{} old_field_visible=true invalidation=false",
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
                let status = self.tracer.ddgi_status();
                let runtime = self.tracer.ddgi_runtime_status();
                assert_ne!(
                    runtime.active_token_serial,
                    Some(obsolete_density_token_serial)
                );
                if runtime.active_token_serial != Some(density_token_serial) {
                    assert_eq!(runtime.active_token_serial, Some(terrain_token_serial));
                    assert_eq!(status.active().published_field, Some(geometry_field));
                    assert_eq!(status.active().grid.spacing_voxels(), 32);
                    if let Some(staging_token) =
                        status.staging().and_then(|staging| staging.build_token)
                    {
                        assert_eq!(
                            staging_token.serial(),
                            density_token_serial,
                            "only the retried density token may remain staged"
                        );
                    }
                    return;
                }
                let active = status.active();
                assert_eq!(active.published_field, Some(density_field));
                assert_eq!(active.grid.spacing_voxels(), 16);
                assert_bootstrap_s1(
                    density_field,
                    geometry_field.field().geometry_revision(),
                    geometry_field.field().radiance_revision(),
                    16,
                );
                assert!(obsolete_density_token_serial < terrain_token_serial);
                assert!(terrain_token_serial < density_token_serial);
                assert!(!runtime.full_domain_invalidation_fail_closed);
                log_acceptance_field("DENSITY", "complete", density_field);
                log::info!(
                    "[DDGI_ACCEPT][DENSITY] complete obsolete_density_token_serial={} terrain_token_serial={} density_token_serial={} obsolete_density_consumer_visible=false first_consumer_visible_16_stage=S1 geometry_revision={} spacing_voxels=16",
                    obsolete_density_token_serial,
                    terrain_token_serial,
                    density_token_serial,
                    density_field.field().geometry_revision(),
                );
                TestScenePhase::Ready
            }
            TestScenePhase::WaitingForEditedTerrain {
                edit,
                target_revision,
            } => {
                if !self.deferred_chunk_rebuilds_idle() {
                    return;
                }
                log::info!(
                    "[ENV_LIGHT_EDIT_CYCLE] edited terrain ready edit={} target_revision={}",
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
                    let status = self.tracer.ddgi_status();
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
                        Ok(reopen_revision) => TestScenePhase::WaitingForEditedTerrain {
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
                    let latest_is_building = matches!(
                        runtime.coordinator_state,
                        crate::ddgi::DdgiRefreshState::BuildingTerrain {
                            candidate,
                            latest_terrain_revision,
                        } if candidate.terrain_revision() == target_revision
                            && latest_terrain_revision == target_revision
                    );
                    if !latest_is_building
                        || !runtime.full_domain_invalidation_fail_closed
                        || runtime
                            .staging_stage
                            .is_none_or(|stage| stage == crate::ddgi::DdgiVolumeStage::Ready)
                    {
                        return;
                    }
                    log::info!(
                        "[ENV_LIGHT_EDIT_INFLIGHT_CAPTURE] armed active_terrain_revision={:?} target_terrain_revision={} staging_token_serial={:?} staging_stage={:?} staging_progress={}/{} coordinator={:?} invalidation=full-domain-fail-closed",
                        runtime.active_terrain_revision,
                        target_revision,
                        runtime.staging_token_serial,
                        runtime.staging_stage,
                        runtime.staging_filtered_probe_count,
                        runtime.staging_probe_count,
                        runtime.coordinator_state,
                    );
                    self.environment_lighting_test_scene
                        .as_mut()
                        .expect("test scene state disappeared")
                        .phase = TestScenePhase::CapturingInflightFailClosed { target_revision };
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
                                Ok(reopen_revision) => TestScenePhase::WaitingForEditedTerrain {
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
                        let spacing_voxels =
                            self.tracer.environment_probe_status().grid.spacing_voxels();
                        match self.tracer.rebuild_environment_probes(spacing_voxels) {
                            Ok(()) => {
                                log::info!(
                                    "[ENV_LIGHT_EDIT_CYCLE] requested density rebuild terrain_revision={} spacing_voxels={}",
                                    target_revision,
                                    spacing_voxels,
                                );
                                TestScenePhase::WaitingForDensityRebuild {
                                    terrain_revision: target_revision,
                                }
                            }
                            Err(err) => {
                                log::error!(
                                    "[ENV_LIGHT_EDIT_CYCLE] post-edit density rebuild failed: {err:#}"
                                );
                                TestScenePhase::Failed
                            }
                        }
                    }
                }
            }
            TestScenePhase::WaitingForDensityRebuild { terrain_revision } => {
                let status = self.tracer.ddgi_status();
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
            TestScenePhase::CapturingInflightFailClosed { .. }
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
        let target_revision = self.tracer.environment_probe_terrain_revision();
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
    fn radiance_s2_identity_restarts_from_the_exact_previous_field() {
        let r1_source = DdgiFieldKey::new(9, 1, 1, 32, DdgiFieldStage::Feedback, 3).unwrap();
        let r1 = DdgiFieldIdentity::new(
            DdgiFieldKey::new(10, 1, 1, 32, DdgiFieldStage::Converged, 4).unwrap(),
            Some(r1_source),
        )
        .unwrap();
        let r2 = DdgiFieldIdentity::new(
            DdgiFieldKey::new(11, 1, 2, 32, DdgiFieldStage::Feedback, 2).unwrap(),
            Some(r1.field()),
        )
        .unwrap();

        assert_radiance_s2(r2, r1, 2);
        assert_eq!(r2.field().serial(), r1.field().serial() + 1);
    }

    #[test]
    fn density_bootstrap_s1_consumes_matching_s0() {
        let seed = DdgiFieldKey::new(20, 2, 1, 16, DdgiFieldStage::SeedSky, 0).unwrap();
        let s1 = DdgiFieldIdentity::new(
            DdgiFieldKey::new(21, 2, 1, 16, DdgiFieldStage::SingleBounce, 1).unwrap(),
            Some(seed),
        )
        .unwrap();

        assert_bootstrap_s1(s1, 2, 1, 16);
    }

    #[test]
    fn inflight_checkpoint_is_capture_ready_without_becoming_final_ready() {
        let scene = EnvironmentLightingTestScene {
            case: EnvironmentLightingTestCase::TerrainEditsInflightCapture,
            phase: TestScenePhase::CapturingInflightFailClosed { target_revision: 3 },
        };

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
        assert!(SKYLIGHT_MIN.x > INTERIOR_MIN.x);
        assert!(SKYLIGHT_MAX.x < INTERIOR_MAX.x);
        assert_eq!(SKYLIGHT_MIN.y, INTERIOR_MAX.y);
        assert!(SKYLIGHT_MAX.y > SHELL_MAX.y);
        assert!(SKYLIGHT_MIN.z > INTERIOR_MIN.z);
        assert!(SKYLIGHT_MAX.z < INTERIOR_MAX.z);
    }

    #[test]
    fn thin_wall_cases_have_exact_voxel_thicknesses() {
        assert_eq!(ONE_VOXEL_WALL_MAX.z - ONE_VOXEL_WALL_MIN.z, 1.0);
        assert_eq!(TWO_VOXEL_WALL_MAX.z - TWO_VOXEL_WALL_MIN.z, 2.0);
    }

    #[test]
    fn all_test_scene_plans_are_static_and_bounded() {
        for case in [
            EnvironmentLightingTestCase::Sealed,
            EnvironmentLightingTestCase::Portal,
            EnvironmentLightingTestCase::Walls,
            EnvironmentLightingTestCase::Donor,
            EnvironmentLightingTestCase::Dogleg,
            EnvironmentLightingTestCase::RadianceChanges,
            EnvironmentLightingTestCase::DensityChanges,
            EnvironmentLightingTestCase::TerrainEdits,
            EnvironmentLightingTestCase::TerrainEditsInflight,
            EnvironmentLightingTestCase::TerrainEditsClosed,
        ] {
            let plan = TestSceneGeometry::build(case).compile().unwrap();
            assert!(plan.voxel_edits.len() >= 2);
            assert_eq!(plan.build_edits.len(), 2);
            assert!(plan.build_edits.iter().all(|edit| {
                matches!(
                    edit,
                    BuildEdit::RebuildMesh(bound)
                        if bound.max().cmple(UVec3::splat(512)).all()
                )
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
        assert!(DONOR_LEFT_ROOF_MAX.y - DONOR_LEFT_ROOF_MIN.y >= 64.0);
        assert!(DONOR_DIVIDER_MAX.x - DONOR_DIVIDER_MIN.x >= 64.0);
        assert!(DONOR_BACK_MAX.z - DONOR_BACK_MIN.z >= 32.0);
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
        assert!(DOGLEG_BLOCK_MAX.y - DOGLEG_FIRST_LEG_MAX.y >= 64.0);
        assert!(DOGLEG_RECEIVER_CHAMBER_MIN.z - DOGLEG_BLOCK_MIN.z >= 32.0);
        assert!(DOGLEG_SECOND_LEG_MIN.x - DOGLEG_FIRST_LEG_MIN.x >= 64.0);
        assert!(DOGLEG_BLOCK_MAX.x - DOGLEG_FIRST_LEG_MAX.x >= 64.0);
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
}
