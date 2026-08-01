use super::App;
use crate::app::world_edits::{BuildEdit, VoxelEdit, WorldEditPlan};
use crate::builder::{VOXEL_TYPE_EMPTY, VOXEL_TYPE_ROCK};
use crate::geom::{build_bvh, Cuboid, UAabb3};
use crate::EnvironmentLightingTestCase;
use anyhow::{Context, Result};
use glam::{UVec3, Vec3};

const BUILD_DELAY_SECONDS: f32 = 0.5;
const SETTLE_FRAMES: u8 = 2;
const TEST_TIME_OF_DAY: f32 = 0.455_705;
const TEST_VOXEL_COLOR_VARIANCE: f32 = 0.0;

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

    pub(super) fn edit_cycle_target_revision(&self) -> Option<u32> {
        if !is_terrain_edit_case(self.case) || self.is_ready() {
            return None;
        }
        match self.phase {
            TestScenePhase::WaitingForEditedTerrain {
                target_revision, ..
            }
            | TestScenePhase::WaitingForEditedProbeField {
                target_revision, ..
            } => Some(target_revision),
            TestScenePhase::WaitingForDensityRebuild { terrain_revision } => Some(terrain_revision),
            _ => Some(0),
        }
    }

    pub(super) fn phase_label(&self) -> &'static str {
        match self.phase {
            TestScenePhase::Pending => "pending",
            TestScenePhase::WaitingForRebuild => "waiting-for-initial-terrain",
            TestScenePhase::Settling { .. } => "settling-initial-terrain",
            TestScenePhase::WaitingForProbeField { .. } => "waiting-for-initial-probe-field",
            TestScenePhase::WaitingForEditedTerrain { .. } => "waiting-for-edited-terrain",
            TestScenePhase::WaitingForEditedProbeField { .. } => "waiting-for-edited-probe-field",
            TestScenePhase::WaitingForDensityRebuild { .. } => "waiting-for-density-rebuild",
            TestScenePhase::Ready => "ready",
            TestScenePhase::Failed => "failed",
        }
    }
}

struct TestSceneGeometry {
    cleared_startup_obstacle: Vec<Cuboid>,
    rock: Vec<Cuboid>,
    carved_empty: Vec<Cuboid>,
    startup_obstacle_rebuild_bound: UAabb3,
    test_rebuild_bound: UAabb3,
}

impl TestSceneGeometry {
    fn build(case: EnvironmentLightingTestCase) -> Self {
        let (startup_obstacle_min, startup_obstacle_max) = App::debug_startup_block_bounds();
        let startup_obstacle = Cuboid::from_min_max(startup_obstacle_min, startup_obstacle_max);
        let startup_obstacle_aabb = startup_obstacle.aabb();

        let (rock, carved_empty) = match case {
            EnvironmentLightingTestCase::Sealed => (
                vec![Cuboid::from_min_max(SHELL_MIN, SHELL_MAX)],
                vec![Cuboid::from_min_max(INTERIOR_MIN, INTERIOR_MAX)],
            ),
            EnvironmentLightingTestCase::Portal
            | EnvironmentLightingTestCase::TerrainEdits
            | EnvironmentLightingTestCase::TerrainEditsInflight
            | EnvironmentLightingTestCase::TerrainEditsClosed => (
                vec![Cuboid::from_min_max(SHELL_MIN, SHELL_MAX)],
                vec![
                    Cuboid::from_min_max(INTERIOR_MIN, INTERIOR_MAX),
                    Cuboid::from_min_max(SKYLIGHT_MIN, SKYLIGHT_MAX),
                ],
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
                (rock, Vec::new())
            }
        };

        Self {
            cleared_startup_obstacle: vec![startup_obstacle],
            rock,
            carved_empty,
            startup_obstacle_rebuild_bound: UAabb3::new(
                startup_obstacle_aabb.min_uvec3(),
                startup_obstacle_aabb.max_uvec3(),
            ),
            test_rebuild_bound: UAabb3::new(TEST_REBUILD_MIN, TEST_REBUILD_MAX),
        }
    }

    fn compile(self) -> Result<WorldEditPlan> {
        let mut voxel_edits = vec![stamp_cuboids(
            self.cleared_startup_obstacle,
            VOXEL_TYPE_EMPTY,
        )?];
        voxel_edits.push(stamp_cuboids(self.rock, VOXEL_TYPE_ROCK)?);
        if !self.carved_empty.is_empty() {
            voxel_edits.push(stamp_cuboids(self.carved_empty, VOXEL_TYPE_EMPTY)?);
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
        | EnvironmentLightingTestCase::TerrainEdits
        | EnvironmentLightingTestCase::TerrainEditsInflight
        | EnvironmentLightingTestCase::TerrainEditsClosed => {
            (Vec3::new(0.65, 0.52, 1.38), Vec3::new(0.65, 0.78, 1.10))
        }
        EnvironmentLightingTestCase::Walls => {
            (Vec3::new(1.00, 0.62, 1.76), Vec3::new(1.00, 0.58, 1.10))
        }
    }
}

impl App {
    pub(super) fn configure_environment_lighting_test_scene_camera(&mut self) {
        let case = self
            .environment_lighting_test_scene
            .as_ref()
            .expect("test scene camera requires test scene")
            .case;
        let (camera_position, camera_target) = camera_pose(case);
        self.current_time_of_day = TEST_TIME_OF_DAY;
        self.debug_settings.adjustables.time_of_day.value = TEST_TIME_OF_DAY;
        self.debug_settings.adjustables.auto_daynight_cycle.value = false;
        self.debug_settings.adjustables.voxel_color_variance.value = TEST_VOXEL_COLOR_VARIANCE;
        self.orbit_camera_focus = camera_target;
        if self
            .tracer
            .set_camera_pose_looking_at(camera_position, camera_target)
        {
            self.request_vsm_history_reset();
            log::info!(
                "[ENV_LIGHT_TEST] case={} camera position=({:.3},{:.3},{:.3}) target=({:.3},{:.3},{:.3}) time_of_day={:.6} auto_cycle=false voxel_color_variance={:.3}",
                case.label(),
                camera_position.x,
                camera_position.y,
                camera_position.z,
                camera_target.x,
                camera_target.y,
                camera_target.z,
                TEST_TIME_OF_DAY,
                TEST_VOXEL_COLOR_VARIANCE,
            );
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
                        log::info!(
                            "[ENV_LIGHT_TEST] static edits applied case={} rebuild_voxel_bound={:?}..{:?}",
                            case.label(),
                            TEST_REBUILD_MIN,
                            TEST_REBUILD_MAX,
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
                self.tracer
                    .notify_ddgi_initial_terrain_ready(terrain_revision);
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
                if is_terrain_edit_case(case) {
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
                if case == EnvironmentLightingTestCase::TerrainEditsInflight
                    && edit == TerrainEdit::CloseSkylight
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
            TestScenePhase::Ready | TestScenePhase::Failed => return,
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
            | EnvironmentLightingTestCase::TerrainEditsInflight
            | EnvironmentLightingTestCase::TerrainEditsClosed
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
