use super::App;
use crate::{
    app::{world_edits::TerrainBrushEdit, DebugSettings},
    tree_gen::TreeDesc,
};
use anyhow::Result;
use glam::{Vec2, Vec3};

const TREE_SEED: u64 = 122;
const TIME_OF_DAY: f32 = 0.47;
const CAMERA_POSITION: Vec3 = Vec3::new(0.46, 1.08, 1.42);
const CAMERA_TARGET: Vec3 = Vec3::new(0.78, 0.41, 0.98);
const RECEIVER_CENTER_XZ: Vec2 = Vec2::new(0.76, 0.98);
const RECEIVER_RADIUS: f32 = 0.34;

pub(super) fn configure_tree(debug_settings: &mut DebugSettings) {
    let mut tree_desc = TreeDesc::default();
    tree_desc.branching.seed = TREE_SEED;
    debug_settings.tree.desc = tree_desc;
    debug_settings.adjustables.tree_age.value = 1.0;
}

impl App {
    pub(super) fn configure_foliage_shadow_bench_camera(&mut self) -> Result<()> {
        self.set_manual_time_of_day(TIME_OF_DAY);
        self.debug_settings.adjustables.auto_daynight_cycle.value = false;
        self.camera_control.apply_snapshot_mode(true);
        self.camera_control.set_orbit_focus(CAMERA_TARGET);
        if !self
            .tracer
            .set_camera_pose_looking_at(CAMERA_POSITION, CAMERA_TARGET)
        {
            anyhow::bail!("failed to apply foliage shadow benchmark camera pose");
        }
        self.tracer.invalidate_local_direct_sun_shadow_histories();
        log::info!(
            "[FOLIAGE_SHADOW_BENCH] tree_seed={} tree_age=1.0 time_of_day={:.3} fixed_animation_hz=60 camera=({:.3},{:.3},{:.3}) target=({:.3},{:.3},{:.3})",
            TREE_SEED,
            TIME_OF_DAY,
            CAMERA_POSITION.x,
            CAMERA_POSITION.y,
            CAMERA_POSITION.z,
            CAMERA_TARGET.x,
            CAMERA_TARGET.y,
            CAMERA_TARGET.z,
        );
        Ok(())
    }

    pub(super) fn configure_foliage_shadow_bench_receiver(&mut self) -> Result<()> {
        self.player_tools.flora_paint_selection_index = 0;
        let center = Vec3::new(
            RECEIVER_CENTER_XZ.x,
            self.query_terrain_height_cpu(RECEIVER_CENTER_XZ),
            RECEIVER_CENTER_XZ.y,
        );
        self.apply_surface_flora_regeneration(
            TerrainBrushEdit {
                start: center,
                end: center,
                radius: RECEIVER_RADIUS,
            },
            1,
            true,
        )?;
        log::info!(
            "[FOLIAGE_SHADOW_BENCH] receiver=grass-mix center=({:.3},{:.3},{:.3}) radius={:.3}",
            center.x,
            center.y,
            center.z,
            RECEIVER_RADIUS,
        );
        Ok(())
    }
}
