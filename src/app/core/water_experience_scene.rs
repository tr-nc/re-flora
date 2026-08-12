use super::App;
use crate::app::world_edits::{VoxelEdit, WorldEditPlan};
use crate::builder::{VOXEL_TYPE_EMPTY, VOXEL_TYPE_ROCK};
use crate::geom::{build_bvh, Cuboid};
use anyhow::Result;
use glam::Vec3;
use re_flora_water::PondWaterConfig;

const VOXELS_PER_WORLD_UNIT: f32 = 256.0;
const BASIN_OUTER_MIN_WS: Vec3 = Vec3::new(0.32, 0.08, 0.32);
const BASIN_OUTER_MAX_WS: Vec3 = Vec3::new(1.68, 0.90, 1.68);
const BASIN_INNER_MIN_WS: Vec3 = Vec3::new(0.42, 0.28, 0.42);
const BASIN_INNER_MAX_WS: Vec3 = Vec3::new(1.58, 1.35, 1.58);
const INITIAL_FLUID_MIN_WS: Vec3 = Vec3::new(0.48, 0.32, 0.48);
const INITIAL_FLUID_MAX_WS: Vec3 = Vec3::new(1.52, 0.72, 1.52);
const CAMERA_POSITION_WS: Vec3 = Vec3::new(1.0, 1.85, 2.35);
const CAMERA_TARGET_WS: Vec3 = Vec3::new(1.0, 0.57, 1.0);
const EXPERIENCE_TIME_OF_DAY: f32 = 0.42;
const EXPERIENCE_PARTICLE_COUNT: usize = 10_000;
const EXPERIENCE_SUBSTEP_HZ: f32 = 60.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WaterExperiencePhase {
    WaitingForCompleteFrame,
    Ready,
}

#[derive(Debug)]
pub(super) struct WaterExperienceScene {
    expected_particle_count: usize,
    phase: WaterExperiencePhase,
}

impl WaterExperienceScene {
    pub(super) fn new(expected_particle_count: usize) -> Self {
        Self {
            expected_particle_count,
            phase: WaterExperiencePhase::WaitingForCompleteFrame,
        }
    }

    pub(super) fn configure_water(config: &mut PondWaterConfig) {
        *config = config
            .clone()
            .with_particle_count(EXPERIENCE_PARTICLE_COUNT)
            .with_initial_fluid_bounds(INITIAL_FLUID_MIN_WS, INITIAL_FLUID_MAX_WS)
            .with_substep_hz(EXPERIENCE_SUBSTEP_HZ)
            .with_terrain_collision_margin_cells(0.0)
            .with_linear_damping_per_sec(1.5);
    }

    fn is_waiting(&self) -> bool {
        self.phase == WaterExperiencePhase::WaitingForCompleteFrame
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

fn terrain_plan() -> Result<WorldEditPlan> {
    Ok(WorldEditPlan {
        // Establish a known solid basin first, then carve the open water volume.
        // Loading builds every terrain chunk after this plan, so no extra runtime
        // rebuild or transient old scene is needed.
        voxel_edits: vec![
            cuboid_edit(BASIN_OUTER_MIN_WS, BASIN_OUTER_MAX_WS, VOXEL_TYPE_ROCK)?,
            cuboid_edit(BASIN_INNER_MIN_WS, BASIN_INNER_MAX_WS, VOXEL_TYPE_EMPTY)?,
        ],
        build_edits: Vec::new(),
    })
}

impl App {
    pub(super) fn apply_water_experience_terrain(&mut self) -> Result<()> {
        self.execute_edit_plan(terrain_plan()?)?;
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
            self.water_sim.config.particle_count,
            INITIAL_FLUID_MIN_WS,
            INITIAL_FLUID_MAX_WS,
        );
        Ok(())
    }

    pub(super) fn process_water_experience_scene(&mut self) {
        let Some(expected_particle_count) = self
            .water_experience_scene
            .as_ref()
            .filter(|scene| scene.is_waiting())
            .map(|scene| scene.expected_particle_count)
        else {
            return;
        };
        if !self.water_terrain_status().is_ready() {
            return;
        }
        let Some(frame) = self.water_sim.latest_particle_frame() else {
            return;
        };
        if !complete_frame_is_ready(
            frame.particles().len(),
            expected_particle_count,
            frame.sim_time_seconds(),
        ) {
            return;
        }

        let revision = frame.revision();
        let sim_time_seconds = frame.sim_time_seconds();
        self.water_experience_scene
            .as_mut()
            .expect("water experience disappeared while becoming ready")
            .phase = WaterExperiencePhase::Ready;
        log::info!(
            "[WATER_EXPERIENCE] ready complete_frame_revision={} sim_time_seconds={:.6} particles={} terrain_cache=ready",
            revision,
            sim_time_seconds,
            expected_particle_count,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use re_flora_water::collider::WaterBoxCollider;

    fn assert_vec3_close(actual: Vec3, expected: Vec3) {
        assert!(
            actual.abs_diff_eq(expected, 1.0e-6),
            "expected {expected:?}, got {actual:?}"
        );
    }

    #[test]
    fn experience_water_config_is_deterministic_and_has_free_surface_headroom() {
        let mut config =
            PondWaterConfig::default().with_collider_bounds(Vec3::ZERO, Vec3::splat(2.0));

        WaterExperienceScene::configure_water(&mut config);

        assert_eq!(config.particle_count, EXPERIENCE_PARTICLE_COUNT);
        assert_eq!(config.substep_dt, EXPERIENCE_SUBSTEP_HZ.recip());
        assert_eq!(config.terrain_collision_margin_cells, 0.0);
        assert_eq!(config.linear_damping_per_sec, 1.5);
        assert_eq!(
            config.initial_fluid_bounds,
            Some(WaterBoxCollider::new(
                INITIAL_FLUID_MIN_WS,
                INITIAL_FLUID_MAX_WS
            ))
        );
        assert!(INITIAL_FLUID_MAX_WS.y < config.collider.max_ws.y);
    }

    #[test]
    fn experience_terrain_plan_builds_solid_basin_then_carves_open_volume() {
        let plan = terrain_plan().unwrap();

        assert!(plan.build_edits.is_empty());
        assert_eq!(plan.voxel_edits.len(), 2);
        let VoxelEdit::StampCuboids {
            cuboids,
            voxel_type,
            ..
        } = &plan.voxel_edits[0]
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
        } = &plan.voxel_edits[1]
        else {
            panic!("expected inner basin cuboid");
        };
        assert_eq!(*voxel_type, VOXEL_TYPE_EMPTY);
        assert_vec3_close(cuboids[0].min() / VOXELS_PER_WORLD_UNIT, BASIN_INNER_MIN_WS);
        assert_vec3_close(cuboids[0].max() / VOXELS_PER_WORLD_UNIT, BASIN_INNER_MAX_WS);
    }

    #[test]
    fn experience_camera_frames_the_fluid_volume_from_outside_the_basin() {
        assert!(CAMERA_POSITION_WS.z > BASIN_OUTER_MAX_WS.z);
        assert!(CAMERA_POSITION_WS.y > BASIN_OUTER_MAX_WS.y);
        assert!(CAMERA_TARGET_WS.x > INITIAL_FLUID_MIN_WS.x);
        assert!(CAMERA_TARGET_WS.x < INITIAL_FLUID_MAX_WS.x);
        assert!(CAMERA_TARGET_WS.z > INITIAL_FLUID_MIN_WS.z);
        assert!(CAMERA_TARGET_WS.z < INITIAL_FLUID_MAX_WS.z);
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
