use super::App;
use crate::app::world_edits::{BuildEdit, VoxelEdit, WorldEditPlan};
use crate::builder::{VOXEL_TYPE_EMPTY, VOXEL_TYPE_ROCK};
use crate::geom::{build_bvh, Cuboid, UAabb3};
use anyhow::{Context, Result};
use glam::{UVec3, Vec3};

const BUILD_DELAY_SECONDS: f32 = 0.5;
const SETTLE_FRAMES: u8 = 2;

const CAMERA_POSITION: Vec3 = Vec3::new(1.0, 0.78, 2.45);
const CAMERA_TARGET: Vec3 = Vec3::new(1.0, 0.55, 1.18);
const TEST_TIME_OF_DAY: f32 = 0.455_705;

pub(super) const STARTUP_TREE_POSITION: Vec3 = Vec3::new(1.72, 0.2, 0.62);

const ROOFED_SHELL_MIN: Vec3 = Vec3::new(96.0, 84.0, 216.0);
const ROOFED_SHELL_MAX: Vec3 = Vec3::new(238.0, 236.0, 392.0);
const ROOFED_INTERIOR_MIN: Vec3 = Vec3::new(112.0, 100.0, 242.0);
const ROOFED_INTERIOR_MAX: Vec3 = Vec3::new(222.0, 216.0, 400.0);

const OPEN_FLOOR_MIN: Vec3 = Vec3::new(274.0, 84.0, 216.0);
const OPEN_FLOOR_MAX: Vec3 = Vec3::new(416.0, 100.0, 392.0);
const OPEN_BACK_MIN: Vec3 = Vec3::new(274.0, 100.0, 216.0);
const OPEN_BACK_MAX: Vec3 = Vec3::new(416.0, 236.0, 242.0);
const OPEN_LEFT_WALL_MIN: Vec3 = Vec3::new(274.0, 100.0, 216.0);
const OPEN_LEFT_WALL_MAX: Vec3 = Vec3::new(290.0, 196.0, 392.0);
const OPEN_RIGHT_WALL_MIN: Vec3 = Vec3::new(400.0, 100.0, 216.0);
const OPEN_RIGHT_WALL_MAX: Vec3 = Vec3::new(416.0, 196.0, 392.0);

const ROOFED_PLINTH_MIN: Vec3 = Vec3::new(148.0, 100.0, 278.0);
const ROOFED_PLINTH_MAX: Vec3 = Vec3::new(184.0, 124.0, 326.0);
const OPEN_PLINTH_MIN: Vec3 = Vec3::new(326.0, 100.0, 278.0);
const OPEN_PLINTH_MAX: Vec3 = Vec3::new(362.0, 124.0, 326.0);

const GALLERY_REBUILD_MIN: UVec3 = UVec3::new(88, 76, 208);
const GALLERY_REBUILD_MAX: UVec3 = UVec3::new(424, 244, 408);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TestScenePhase {
    Pending,
    WaitingForRebuild,
    Settling(u8),
    Ready,
    Failed,
}

#[derive(Debug)]
pub(super) struct EnvironmentLightingTestScene {
    phase: TestScenePhase,
}

impl EnvironmentLightingTestScene {
    pub(super) fn new() -> Self {
        Self {
            phase: TestScenePhase::Pending,
        }
    }

    pub(super) fn is_ready(&self) -> bool {
        self.phase == TestScenePhase::Ready
    }
}

struct TestSceneGeometry {
    cleared_startup_obstacle: Vec<Cuboid>,
    base_rock: Vec<Cuboid>,
    carved_empty: Vec<Cuboid>,
    comparison_rock: Vec<Cuboid>,
    startup_obstacle_rebuild_bound: UAabb3,
    gallery_rebuild_bound: UAabb3,
}

impl TestSceneGeometry {
    fn build() -> Self {
        let (startup_obstacle_min, startup_obstacle_max) = App::debug_startup_block_bounds();
        let startup_obstacle = Cuboid::from_min_max(startup_obstacle_min, startup_obstacle_max);
        let startup_obstacle_aabb = startup_obstacle.aabb();
        Self {
            cleared_startup_obstacle: vec![startup_obstacle],
            base_rock: vec![
                Cuboid::from_min_max(ROOFED_SHELL_MIN, ROOFED_SHELL_MAX),
                Cuboid::from_min_max(OPEN_FLOOR_MIN, OPEN_FLOOR_MAX),
                Cuboid::from_min_max(OPEN_BACK_MIN, OPEN_BACK_MAX),
                Cuboid::from_min_max(OPEN_LEFT_WALL_MIN, OPEN_LEFT_WALL_MAX),
                Cuboid::from_min_max(OPEN_RIGHT_WALL_MIN, OPEN_RIGHT_WALL_MAX),
            ],
            carved_empty: vec![Cuboid::from_min_max(
                ROOFED_INTERIOR_MIN,
                ROOFED_INTERIOR_MAX,
            )],
            comparison_rock: vec![
                Cuboid::from_min_max(ROOFED_PLINTH_MIN, ROOFED_PLINTH_MAX),
                Cuboid::from_min_max(OPEN_PLINTH_MIN, OPEN_PLINTH_MAX),
            ],
            startup_obstacle_rebuild_bound: UAabb3::new(
                startup_obstacle_aabb.min_uvec3(),
                startup_obstacle_aabb.max_uvec3(),
            ),
            gallery_rebuild_bound: UAabb3::new(GALLERY_REBUILD_MIN, GALLERY_REBUILD_MAX),
        }
    }

    fn compile(self) -> Result<WorldEditPlan> {
        Ok(WorldEditPlan {
            voxel_edits: vec![
                stamp_cuboids(self.cleared_startup_obstacle, VOXEL_TYPE_EMPTY)?,
                stamp_cuboids(self.base_rock, VOXEL_TYPE_ROCK)?,
                stamp_cuboids(self.carved_empty, VOXEL_TYPE_EMPTY)?,
                stamp_cuboids(self.comparison_rock, VOXEL_TYPE_ROCK)?,
            ],
            build_edits: vec![
                BuildEdit::RebuildMesh(self.startup_obstacle_rebuild_bound),
                BuildEdit::RebuildMesh(self.gallery_rebuild_bound),
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

impl App {
    pub(super) fn configure_environment_lighting_test_scene_camera(&mut self) {
        self.current_time_of_day = TEST_TIME_OF_DAY;
        self.debug_settings.adjustables.time_of_day.value = TEST_TIME_OF_DAY;
        self.debug_settings.adjustables.auto_daynight_cycle.value = false;
        self.orbit_camera_focus = CAMERA_TARGET;
        if self
            .tracer
            .set_camera_pose_looking_at(CAMERA_POSITION, CAMERA_TARGET)
        {
            self.request_vsm_history_reset();
            log::info!(
                "[ENV_LIGHT_TEST] camera position=({:.3},{:.3},{:.3}) target=({:.3},{:.3},{:.3}) time_of_day={:.6} auto_cycle=false",
                CAMERA_POSITION.x,
                CAMERA_POSITION.y,
                CAMERA_POSITION.z,
                CAMERA_TARGET.x,
                CAMERA_TARGET.y,
                CAMERA_TARGET.z,
                TEST_TIME_OF_DAY,
            );
        } else {
            log::error!("[ENV_LIGHT_TEST] failed to apply deterministic camera pose");
        }
    }

    pub(super) fn process_environment_lighting_test_scene(&mut self) {
        let Some(phase) = self
            .environment_lighting_test_scene
            .as_ref()
            .map(|scene| scene.phase)
        else {
            return;
        };

        match phase {
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
                    "[ENV_LIGHT_TEST] constructing roofed and open terrain bays with voxel edits"
                );
                let result = TestSceneGeometry::build()
                    .compile()
                    .context("compile deterministic environment-lighting test scene")
                    .and_then(|plan| self.execute_edit_plan(plan));
                let next_phase = match result {
                    Ok(()) => {
                        log::info!(
                            "[ENV_LIGHT_TEST] edits applied gallery_rebuild_voxel_bound={:?}..{:?}",
                            GALLERY_REBUILD_MIN,
                            GALLERY_REBUILD_MAX,
                        );
                        TestScenePhase::WaitingForRebuild
                    }
                    Err(err) => {
                        log::error!("[ENV_LIGHT_TEST] construction failed: {err:#}");
                        TestScenePhase::Failed
                    }
                };
                self.environment_lighting_test_scene
                    .as_mut()
                    .expect("test scene state disappeared")
                    .phase = next_phase;
            }
            TestScenePhase::WaitingForRebuild => {
                if self.deferred_chunk_rebuilds_idle() {
                    self.tracer.request_environment_probe_classification();
                    log::info!(
                        "[ENV_LIGHT_TEST] terrain rebuild complete; requested probe classification; settling {} frames",
                        SETTLE_FRAMES
                    );
                    self.environment_lighting_test_scene
                        .as_mut()
                        .expect("test scene state disappeared")
                        .phase = TestScenePhase::Settling(SETTLE_FRAMES);
                }
            }
            TestScenePhase::Settling(frames) => {
                let next_phase = if frames > 1 {
                    TestScenePhase::Settling(frames - 1)
                } else {
                    log::info!(
                        "[ENV_LIGHT_TEST] ready roofed_sample_ws=(0.648,0.438,1.180) open_sample_ws=(1.344,0.438,1.180)"
                    );
                    TestScenePhase::Ready
                };
                self.environment_lighting_test_scene
                    .as_mut()
                    .expect("test scene state disappeared")
                    .phase = next_phase;
            }
            TestScenePhase::Ready | TestScenePhase::Failed => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roofed_carve_stays_inside_shell_except_for_open_front() {
        assert!(ROOFED_INTERIOR_MIN.cmpgt(ROOFED_SHELL_MIN).all());
        assert!(ROOFED_INTERIOR_MAX.x < ROOFED_SHELL_MAX.x);
        assert!(ROOFED_INTERIOR_MAX.y < ROOFED_SHELL_MAX.y);
        assert!(ROOFED_INTERIOR_MAX.z > ROOFED_SHELL_MAX.z);
    }

    #[test]
    fn open_and_roofed_plinths_have_matching_dimensions_and_height() {
        assert_eq!(
            ROOFED_PLINTH_MAX - ROOFED_PLINTH_MIN,
            OPEN_PLINTH_MAX - OPEN_PLINTH_MIN
        );
        assert_eq!(ROOFED_PLINTH_MIN.y, OPEN_PLINTH_MIN.y);
        assert_eq!(ROOFED_PLINTH_MAX.y, OPEN_PLINTH_MAX.y);
    }

    #[test]
    fn test_scene_plan_is_bounded_and_uses_focused_rebuilds() {
        let plan = TestSceneGeometry::build().compile().unwrap();

        assert_eq!(plan.voxel_edits.len(), 4);
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
