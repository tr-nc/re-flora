use super::ui_style::GOLD_ACCENT;
use super::App;
use crate::app::camera_snapshots::{
    is_player_default_snapshot_name, CameraSnapshot, CameraSnapshotLibrary,
    PLAYER_DEFAULT_SNAPSHOT_NAME,
};
use crate::gameplay::CameraPose;
use anyhow::{anyhow, Result};
use egui::RichText;

const CAMERA_SNAPSHOT_DRAFT_BASE_NAME: &str = "snapshot";

impl App {
    pub(super) fn apply_startup_camera_snapshot(
        &mut self,
        requested_name: Option<&str>,
    ) -> Result<()> {
        let Some(requested_name) = requested_name else {
            log::info!("[CAMERA_SNAPSHOT] Using default player camera; no snapshot requested");
            return Ok(());
        };

        if is_player_default_snapshot_name(requested_name) {
            log::info!(
                "[CAMERA_SNAPSHOT] Using default player camera requested as '{}'",
                requested_name
            );
            return Ok(());
        }

        let snapshot = self
            .camera_snapshots
            .find(requested_name)
            .cloned()
            .ok_or_else(|| {
                anyhow!(
                    "camera snapshot '{}' not found. Available snapshots: {}. Run `re-flora --list-camera-snapshots` to list available camera snapshots.",
                    requested_name,
                    self.camera_snapshots.names_for_cli().join(", ")
                )
            })?;

        self.apply_camera_snapshot(&snapshot);
        log::info!(
            "[CAMERA_SNAPSHOT] Applied startup snapshot '{}' from {}",
            snapshot.name,
            self.camera_snapshots.path().display()
        );
        Ok(())
    }

    pub(super) fn apply_camera_snapshot(&mut self, snapshot: &CameraSnapshot) {
        self.camera_control_mode = if snapshot.fly_mode {
            super::CameraControlMode::FreeFly
        } else {
            super::CameraControlMode::Walk
        };
        self.sync_cursor_with_panels();
        self.accumulated_mouse_delta = glam::Vec2::ZERO;
        self.smoothed_mouse_delta = glam::Vec2::ZERO;
        self.player_tools.shovel_dig_held = false;
        self.stop_terrain_edit_loop_sound();
        self.tracer.apply_camera_pose(snapshot.pose());
        self.request_vsm_history_reset();
    }
}

pub(super) fn draw_camera_snapshots_ui(
    ui: &mut egui::Ui,
    camera_snapshots: &mut CameraSnapshotLibrary,
    draft_name: &mut String,
    draft_description: &mut String,
    status: &mut Option<String>,
    current_pose: CameraPose,
    is_fly_mode: bool,
) -> Option<CameraSnapshot> {
    ui.heading(
        RichText::new("Camera Snapshots")
            .size(16.0)
            .color(GOLD_ACCENT),
    );
    ui.label(format!("File: {}", camera_snapshots.path().display()));

    ui.monospace(format!(
        "pos [{:.3}, {:.3}, {:.3}]  yaw {:.2}  pitch {:.2}  fov {:.2}  fly {}",
        current_pose.position.x,
        current_pose.position.y,
        current_pose.position.z,
        current_pose.yaw_deg,
        current_pose.pitch_deg,
        current_pose.fov_deg,
        is_fly_mode
    ));

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label("Name");
        ui.text_edit_singleline(draft_name);
    });
    ui.horizontal(|ui| {
        ui.label("Description");
        ui.text_edit_singleline(draft_description);
    });

    if ui.button("Save Current Camera Snapshot").clicked() {
        let requested_name = draft_name.trim();
        let description = draft_description.trim().to_owned();
        match camera_snapshots.add_from_pose(requested_name, description, current_pose, is_fly_mode)
        {
            Ok(saved_name) => {
                *status = Some(format!(
                    "Saved '{}' to {}",
                    saved_name,
                    camera_snapshots.path().display()
                ));
                log::info!("[CAMERA_SNAPSHOT] Saved snapshot '{}'", saved_name);
                *draft_name = camera_snapshots.unique_name(CAMERA_SNAPSHOT_DRAFT_BASE_NAME);
                draft_description.clear();
            }
            Err(err) => {
                *status = Some(format!("Save failed: {err}"));
                log::error!("[CAMERA_SNAPSHOT] Save failed: {}", err);
            }
        }
    }

    if let Some(status) = status.as_ref() {
        ui.label(status);
    }

    ui.add_space(6.0);
    ui.label(format!(
        "{} saved snapshots (+ '{}' default option for CLI)",
        camera_snapshots.snapshots().len(),
        PLAYER_DEFAULT_SNAPSHOT_NAME
    ));

    if camera_snapshots.is_empty() {
        ui.label("No saved snapshots yet. The default player camera remains available.");
        return None;
    }

    #[derive(Clone)]
    enum SnapshotUiAction {
        Apply(String),
        Delete(String),
    }

    let mut action = None;
    for snapshot in camera_snapshots.snapshots() {
        ui.horizontal(|ui| {
            ui.monospace(&snapshot.name);
            if !snapshot.description.is_empty() {
                ui.label(&snapshot.description);
            }
            if ui.button("Apply").clicked() {
                action = Some(SnapshotUiAction::Apply(snapshot.name.clone()));
            }
            if ui.button("Delete").clicked() {
                action = Some(SnapshotUiAction::Delete(snapshot.name.clone()));
            }
        });
    }

    match action {
        Some(SnapshotUiAction::Apply(name)) => {
            let Some(snapshot) = camera_snapshots.find(&name).cloned() else {
                *status = Some(format!("Snapshot '{}' no longer exists", name));
                return None;
            };
            *status = Some(format!("Applied '{}'", snapshot.name));
            log::info!(
                "[CAMERA_SNAPSHOT] Applied snapshot '{}' from UI",
                snapshot.name
            );
            Some(snapshot)
        }
        Some(SnapshotUiAction::Delete(name)) => match camera_snapshots.remove(&name) {
            Ok(true) => {
                *status = Some(format!("Deleted '{}'", name));
                log::info!("[CAMERA_SNAPSHOT] Deleted snapshot '{}'", name);
                *draft_name = camera_snapshots.unique_name(CAMERA_SNAPSHOT_DRAFT_BASE_NAME);
                None
            }
            Ok(false) => {
                *status = Some(format!("Snapshot '{}' no longer exists", name));
                None
            }
            Err(err) => {
                *status = Some(format!("Delete failed: {err}"));
                log::error!("[CAMERA_SNAPSHOT] Delete '{}' failed: {}", name, err);
                None
            }
        },
        None => None,
    }
}
