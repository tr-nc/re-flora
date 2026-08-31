use super::water::{
    EXPERIENCE_INITIAL_FLUID_MAX_WS as INITIAL_FLUID_MAX_WS,
    EXPERIENCE_INITIAL_FLUID_MIN_WS as INITIAL_FLUID_MIN_WS,
};
use super::App;
use crate::app::world_edits::{VoxelEdit, WorldEditTransaction};
use crate::builder::{VOXEL_TYPE_EMPTY, VOXEL_TYPE_ROCK};
use crate::geom::{build_bvh, Cuboid};
use anyhow::Result;
use glam::Vec3;

const VOXELS_PER_WORLD_UNIT: f32 = 256.0;
const BASIN_OUTER_MIN_WS: Vec3 = Vec3::new(0.32, 0.08, 0.32);
const BASIN_OUTER_MAX_WS: Vec3 = Vec3::new(1.68, 0.90, 1.68);
const BASIN_INNER_MIN_WS: Vec3 = Vec3::new(0.42, 0.28, 0.42);
const BASIN_INNER_MAX_WS: Vec3 = Vec3::new(1.58, 1.35, 1.58);
const CAMERA_POSITION_WS: Vec3 = Vec3::new(1.0, 1.85, 2.35);
const CAMERA_TARGET_WS: Vec3 = Vec3::new(1.0, 0.57, 1.0);
const EXPERIENCE_TIME_OF_DAY: f32 = 0.42;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WaterExperiencePhase {
    PendingActivation,
    WaitingForCompleteFrame { expected_particle_count: usize },
    Ready,
}

pub(super) enum WaterExperienceFrameTxn {
    Inactive,
    PendingActivation,
    Waiting { expected_particle_count: usize },
    Ready,
}

pub(super) enum WaterExperienceFrameResult {
    NotReady,
    Ready {
        particle_count: usize,
        sim_time_seconds: f32,
        revision: u64,
    },
}

pub(super) struct WaterExperienceReadyReceipt {
    pub(super) particle_count: usize,
    pub(super) sim_time_seconds: f32,
    pub(super) revision: u64,
}

#[derive(Debug)]
pub(super) struct WaterExperienceScene {
    phase: WaterExperiencePhase,
}

impl WaterExperienceScene {
    pub(super) fn pending() -> Self {
        Self {
            phase: WaterExperiencePhase::PendingActivation,
        }
    }

    pub(super) fn activate(&mut self, expected_particle_count: usize) {
        match self.phase {
            WaterExperiencePhase::PendingActivation => {
                self.phase = WaterExperiencePhase::WaitingForCompleteFrame {
                    expected_particle_count,
                };
            }
            WaterExperiencePhase::WaitingForCompleteFrame { .. } | WaterExperiencePhase::Ready => {
                panic!("water experience was activated more than once")
            }
        }
    }

    pub(super) fn begin_frame(&self) -> WaterExperienceFrameTxn {
        match self.phase {
            WaterExperiencePhase::PendingActivation => WaterExperienceFrameTxn::PendingActivation,
            WaterExperiencePhase::WaitingForCompleteFrame {
                expected_particle_count,
            } => WaterExperienceFrameTxn::Waiting {
                expected_particle_count,
            },
            WaterExperiencePhase::Ready => WaterExperienceFrameTxn::Ready,
        }
    }

    pub(super) fn finish_frame(
        &mut self,
        transaction: WaterExperienceFrameTxn,
        result: WaterExperienceFrameResult,
    ) -> anyhow::Result<Option<WaterExperienceReadyReceipt>> {
        let WaterExperienceFrameTxn::Waiting {
            expected_particle_count,
        } = transaction
        else {
            anyhow::ensure!(
                matches!(result, WaterExperienceFrameResult::NotReady),
                "inactive water-experience frame received a ready result"
            );
            return Ok(None);
        };
        anyhow::ensure!(
            matches!(
                self.phase,
                WaterExperiencePhase::WaitingForCompleteFrame {
                    expected_particle_count: current
                } if current == expected_particle_count
            ),
            "stale water-experience frame transaction"
        );
        let WaterExperienceFrameResult::Ready {
            particle_count,
            sim_time_seconds,
            revision,
        } = result
        else {
            return Ok(None);
        };
        anyhow::ensure!(
            complete_frame_is_ready(particle_count, expected_particle_count, sim_time_seconds),
            "water-experience ready receipt does not describe a complete frame"
        );
        self.phase = WaterExperiencePhase::Ready;
        Ok(Some(WaterExperienceReadyReceipt {
            particle_count,
            sim_time_seconds,
            revision,
        }))
    }
}

fn complete_frame_is_ready(
    particle_count: usize,
    expected_particle_count: usize,
    sim_time_seconds: f32,
) -> bool {
    particle_count == expected_particle_count
        && sim_time_seconds.is_finite()
        && sim_time_seconds > 0.0
}

fn cuboid_edit(min_ws: Vec3, max_ws: Vec3, voxel_type: u32) -> Result<VoxelEdit> {
    let cuboid = Cuboid::from_min_max(
        min_ws * VOXELS_PER_WORLD_UNIT,
        max_ws * VOXELS_PER_WORLD_UNIT,
    );
    let bvh_nodes = build_bvh(&[cuboid.aabb()], &[0]).map_err(anyhow::Error::msg)?;
    Ok(VoxelEdit::StampCuboids {
        bvh_nodes,
        cuboids: vec![cuboid],
        voxel_type,
    })
}

fn terrain_plan() -> Result<WorldEditTransaction> {
    Ok(WorldEditTransaction::during_loading(vec![
        // Establish a known solid basin first, then carve the open water volume.
        // Loading builds every terrain chunk after this plan, so no extra runtime
        // rebuild or transient old scene is needed.
        cuboid_edit(BASIN_OUTER_MIN_WS, BASIN_OUTER_MAX_WS, VOXEL_TYPE_ROCK)?,
        cuboid_edit(BASIN_INNER_MIN_WS, BASIN_INNER_MAX_WS, VOXEL_TYPE_EMPTY)?,
    ]))
}

impl App {
    pub(super) fn apply_water_experience_terrain(&mut self) -> Result<()> {
        self.execute_world_edit(terrain_plan()?)?;
        log::info!(
            "[WATER_EXPERIENCE] terrain basin outer={:?}..{:?} inner={:?}..{:?}",
            BASIN_OUTER_MIN_WS,
            BASIN_OUTER_MAX_WS,
            BASIN_INNER_MIN_WS,
            BASIN_INNER_MAX_WS,
        );
        Ok(())
    }

    pub(super) fn configure_water_experience_camera(&mut self) -> Result<()> {
        self.set_manual_time_of_day(EXPERIENCE_TIME_OF_DAY);
        self.debug_settings.adjustables.auto_daynight_cycle.value = false;
        self.camera_control.set_orbit_focus(CAMERA_TARGET_WS);
        anyhow::ensure!(
            self.tracer
                .set_camera_pose_looking_at(CAMERA_POSITION_WS, CAMERA_TARGET_WS),
            "failed to apply deterministic water experience camera"
        );
        self.tracer.invalidate_local_direct_sun_shadow_histories();
        log::info!(
            "[WATER_EXPERIENCE] configured camera={:?} target={:?} time_of_day={:.3} particle_backend_seed={} initial_fluid={:?}..{:?}",
            CAMERA_POSITION_WS,
            CAMERA_TARGET_WS,
            EXPERIENCE_TIME_OF_DAY,
            self.water.config().particle_count,
            INITIAL_FLUID_MIN_WS,
            INITIAL_FLUID_MAX_WS,
        );
        Ok(())
    }

    pub(super) fn process_water_experience_scene(&mut self) {
        let transaction = self.launch_owners.begin_water_experience_frame();
        let super::launch_owners::WaterExperienceFrameTxn::Waiting {
            expected_particle_count,
        } = &transaction
        else {
            return;
        };
        if !self.water.terrain_status().is_ready() {
            self.launch_owners
                .finish_water_experience_frame(
                    transaction,
                    super::launch_owners::WaterExperienceFrameResult::NotReady,
                )
                .expect("water-experience wait transaction must remain current");
            return;
        }
        let Some(frame) = self.water.latest_particle_frame() else {
            self.launch_owners
                .finish_water_experience_frame(
                    transaction,
                    super::launch_owners::WaterExperienceFrameResult::NotReady,
                )
                .expect("water-experience wait transaction must remain current");
            return;
        };
        if !complete_frame_is_ready(
            frame.particles().len(),
            *expected_particle_count,
            frame.sim_time_seconds(),
        ) {
            self.launch_owners
                .finish_water_experience_frame(
                    transaction,
                    super::launch_owners::WaterExperienceFrameResult::NotReady,
                )
                .expect("water-experience wait transaction must remain current");
            return;
        }

        let receipt = self
            .launch_owners
            .finish_water_experience_frame(
                transaction,
                super::launch_owners::WaterExperienceFrameResult::Ready {
                    particle_count: frame.particles().len(),
                    sim_time_seconds: frame.sim_time_seconds(),
                    revision: frame.revision(),
                },
            )
            .unwrap_or_else(|error| {
                panic!("[WATER_EXPERIENCE] failed to commit ready frame: {error:#}")
            })
            .expect("ready water-experience frame must produce a receipt");
        log::info!(
            "[WATER_EXPERIENCE] ready complete_frame_revision={} sim_time_seconds={:.6} particles={} terrain_cache=ready",
            receipt.revision,
            receipt.sim_time_seconds,
            receipt.particle_count,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::core::water::EXPERIENCE_PARTICLE_COUNT;

    fn assert_vec3_close(actual: Vec3, expected: Vec3) {
        assert!(
            actual.abs_diff_eq(expected, 1.0e-6),
            "expected {expected:?}, got {actual:?}"
        );
    }

    #[test]
    fn experience_terrain_plan_builds_solid_basin_then_carves_open_volume() {
        let plan = terrain_plan().unwrap();

        assert!(plan
            .affected_voxels(crate::app::core::VOXEL_DIM_PER_CHUNK)
            .unwrap()
            .is_none());
        assert_eq!(plan.voxel_edits().len(), 2);
        let VoxelEdit::StampCuboids {
            cuboids,
            voxel_type,
            ..
        } = &plan.voxel_edits()[0]
        else {
            panic!("expected outer basin cuboid");
        };
        assert_eq!(*voxel_type, VOXEL_TYPE_ROCK);
        assert_vec3_close(cuboids[0].min() / VOXELS_PER_WORLD_UNIT, BASIN_OUTER_MIN_WS);
        assert_vec3_close(cuboids[0].max() / VOXELS_PER_WORLD_UNIT, BASIN_OUTER_MAX_WS);

        let VoxelEdit::StampCuboids {
            cuboids,
            voxel_type,
            ..
        } = &plan.voxel_edits()[1]
        else {
            panic!("expected inner basin cuboid");
        };
        assert_eq!(*voxel_type, VOXEL_TYPE_EMPTY);
        assert_vec3_close(cuboids[0].min() / VOXELS_PER_WORLD_UNIT, BASIN_INNER_MIN_WS);
        assert_vec3_close(cuboids[0].max() / VOXELS_PER_WORLD_UNIT, BASIN_INNER_MAX_WS);
    }

    #[test]
    fn experience_camera_frames_the_fluid_volume_from_outside_the_basin() {
        const { assert!(CAMERA_POSITION_WS.z > BASIN_OUTER_MAX_WS.z) };
        const { assert!(CAMERA_POSITION_WS.y > BASIN_OUTER_MAX_WS.y) };
        const { assert!(CAMERA_TARGET_WS.x > INITIAL_FLUID_MIN_WS.x) };
        const { assert!(CAMERA_TARGET_WS.x < INITIAL_FLUID_MAX_WS.x) };
        const { assert!(CAMERA_TARGET_WS.z > INITIAL_FLUID_MIN_WS.z) };
        const { assert!(CAMERA_TARGET_WS.z < INITIAL_FLUID_MAX_WS.z) };
    }

    #[test]
    fn experience_waits_for_a_complete_advanced_frame() {
        assert!(!complete_frame_is_ready(
            EXPERIENCE_PARTICLE_COUNT - 1,
            EXPERIENCE_PARTICLE_COUNT,
            1.0 / 60.0,
        ));
        assert!(!complete_frame_is_ready(
            EXPERIENCE_PARTICLE_COUNT,
            EXPERIENCE_PARTICLE_COUNT,
            0.0,
        ));
        assert!(!complete_frame_is_ready(
            EXPERIENCE_PARTICLE_COUNT,
            EXPERIENCE_PARTICLE_COUNT,
            f32::NAN,
        ));
        assert!(complete_frame_is_ready(
            EXPERIENCE_PARTICLE_COUNT,
            EXPERIENCE_PARTICLE_COUNT,
            1.0 / 60.0,
        ));
    }
}
