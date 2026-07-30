use super::App;
use crate::app::world_edits::{BuildEdit, VoxelEdit, WorldEditPlan};
use crate::builder::{VOXEL_TYPE_EMPTY, VOXEL_TYPE_ROCK};
use crate::environment_probes::{
    EnvironmentProbeVisualizationFilter, EnvironmentProbeVisualizationMode,
};
use crate::geom::{build_bvh, Cuboid, UAabb3};
use crate::tracer::{append_box, GeometryPreviewMesh};
use anyhow::{Context, Result};
use glam::{UVec3, Vec3, Vec4};

const BUILD_DELAY_SECONDS: f32 = 0.5;
const SETTLE_FRAMES: u8 = 2;

const CAMERA_POSITION: Vec3 = Vec3::new(1.0, 0.875, 2.18);
const CAMERA_TARGET: Vec3 = Vec3::new(1.0, 0.875, 0.82);
const TEST_TIME_OF_DAY: f32 = 0.455_705;

const CLEAR_MIN: Vec3 = Vec3::new(104.0, 80.0, 160.0);
const CLEAR_MAX: Vec3 = Vec3::new(472.0, 360.0, 504.0);
const WALL_MIN: Vec3 = Vec3::new(256.0, 116.0, 270.0);
const WALL_MAX: Vec3 = Vec3::new(456.0, 326.0, 294.0);
const REBUILD_MIN: UVec3 = UVec3::new(96, 72, 152);
const REBUILD_MAX: UVec3 = UVec3::new(480, 368, 511);

const SENTINEL_MIN: Vec3 = Vec3::new(0.48, 0.46, 0.72);
const SENTINEL_MAX: Vec3 = Vec3::new(1.78, 1.29, 0.74);
const SENTINEL_STRIPE_COUNT: usize = 12;
const SENTINEL_RED: Vec4 = Vec4::new(1.0, 0.015, 0.03, 1.0);
const SENTINEL_BLUE: Vec4 = Vec4::new(0.015, 0.03, 1.0, 1.0);

pub(super) const STARTUP_TREE_POSITION: Vec3 = Vec3::new(1.26, 0.2, 0.54);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TestScenePhase {
    Pending,
    WaitingForRebuild,
    Settling { frames: u8, terrain_revision: u32 },
    WaitingForProbeField { terrain_revision: u32 },
    Ready,
    Failed,
}

#[derive(Debug)]
pub(super) struct HybridTransparencyTestScene {
    phase: TestScenePhase,
}

impl HybridTransparencyTestScene {
    pub(super) fn new() -> Self {
        Self {
            phase: TestScenePhase::Pending,
        }
    }

    pub(super) fn is_ready(&self) -> bool {
        self.phase == TestScenePhase::Ready
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

fn scene_plan() -> Result<WorldEditPlan> {
    Ok(WorldEditPlan {
        voxel_edits: vec![
            stamp_cuboids(
                vec![Cuboid::from_min_max(CLEAR_MIN, CLEAR_MAX)],
                VOXEL_TYPE_EMPTY,
            )?,
            stamp_cuboids(
                vec![Cuboid::from_min_max(WALL_MIN, WALL_MAX)],
                VOXEL_TYPE_ROCK,
            )?,
        ],
        build_edits: vec![BuildEdit::RebuildMesh(UAabb3::new(
            REBUILD_MIN,
            REBUILD_MAX,
        ))],
    })
}

fn sentinel_mesh() -> GeometryPreviewMesh {
    let mut mesh = GeometryPreviewMesh::default();
    let stripe_width = (SENTINEL_MAX.x - SENTINEL_MIN.x) / SENTINEL_STRIPE_COUNT as f32;
    for stripe in 0..SENTINEL_STRIPE_COUNT {
        let min = Vec3::new(
            SENTINEL_MIN.x + stripe as f32 * stripe_width,
            SENTINEL_MIN.y,
            SENTINEL_MIN.z,
        );
        let max = Vec3::new(
            SENTINEL_MIN.x + (stripe + 1) as f32 * stripe_width,
            SENTINEL_MAX.y,
            SENTINEL_MAX.z,
        );
        let color = if stripe % 2 == 0 {
            SENTINEL_RED
        } else {
            SENTINEL_BLUE
        };
        append_box(&mut mesh, min, max, color);
    }
    mesh
}

impl App {
    pub(super) fn configure_hybrid_transparency_test_scene(&mut self) -> Result<()> {
        self.current_time_of_day = TEST_TIME_OF_DAY;
        self.debug_settings.adjustables.time_of_day.value = TEST_TIME_OF_DAY;
        self.debug_settings.adjustables.auto_daynight_cycle.value = false;
        self.orbit_camera_focus = CAMERA_TARGET;
        if !self
            .tracer
            .set_camera_pose_looking_at(CAMERA_POSITION, CAMERA_TARGET)
        {
            anyhow::bail!("failed to apply deterministic camera pose");
        }

        let mut visualization = self.tracer.environment_probe_visualization_settings();
        visualization.enabled = true;
        visualization.mode = EnvironmentProbeVisualizationMode::State;
        visualization.filter = EnvironmentProbeVisualizationFilter::Valid;
        visualization.camera_radius_voxels = 88.0;
        visualization.instance_stride = 1;
        visualization.marker_size_voxels = 6.0;
        visualization.depth_tested = true;
        self.tracer
            .set_environment_probe_visualization_settings(visualization);

        self.tracer
            .upload_debug_geometry_preview(&sentinel_mesh(), Vec3::ZERO, Vec4::ONE)?;
        self.request_vsm_history_reset();
        log::info!(
            "[HYBRID_ALPHA_TEST] camera position=({:.3},{:.3},{:.3}) target=({:.3},{:.3},{:.3}) left=control_no_terrain right=rock_occlusion sentinel=opaque_red_blue_stripes probes=valid_depth_tested",
            CAMERA_POSITION.x,
            CAMERA_POSITION.y,
            CAMERA_POSITION.z,
            CAMERA_TARGET.x,
            CAMERA_TARGET.y,
            CAMERA_TARGET.z,
        );
        Ok(())
    }

    pub(super) fn process_hybrid_transparency_test_scene(&mut self) {
        let Some(phase) = self
            .hybrid_transparency_test_scene
            .as_ref()
            .map(|scene| scene.phase)
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
                    "[HYBRID_ALPHA_TEST] constructing empty viewing corridor and right-side rock occluder"
                );
                match scene_plan()
                    .context("compile deterministic hybrid transparency test scene")
                    .and_then(|plan| self.execute_edit_plan(plan))
                {
                    Ok(()) => TestScenePhase::WaitingForRebuild,
                    Err(err) => {
                        log::error!("[HYBRID_ALPHA_TEST] construction failed: {err:#}");
                        TestScenePhase::Failed
                    }
                }
            }
            TestScenePhase::WaitingForRebuild => {
                if !self.deferred_chunk_rebuilds_idle() {
                    return;
                }
                let terrain_revision = self.tracer.environment_probe_terrain_revision();
                log::info!(
                    "[HYBRID_ALPHA_TEST] terrain rebuild complete revision={}; settling {} frames",
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
                    .environment_probe_terrain_revision_ready(terrain_revision)
                {
                    return;
                }
                log::info!(
                    "[HYBRID_ALPHA_TEST] ready revision={} expected_current_bug=red_blue_stripes_visible_through_right_probe_despite_rock_wall",
                    terrain_revision,
                );
                TestScenePhase::Ready
            }
            TestScenePhase::Ready | TestScenePhase::Failed => return,
        };

        self.hybrid_transparency_test_scene
            .as_mut()
            .expect("test scene state disappeared")
            .phase = next_phase;
    }
}
