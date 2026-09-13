use super::App;
use crate::app::camera_snapshots::{
    is_player_default_snapshot_name, CameraSnapshot, CameraSnapshotLibrary,
};
use crate::gameplay::CameraPose;
use anyhow::{anyhow, Result};

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
            self.tracer
                .apply_camera_pose(crate::app::camera_snapshots::player_default_camera_pose());
            self.sync_orbit_focus_from_current_view();
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
        self.camera_control.apply_snapshot_mode(snapshot.fly_mode);
        self.sync_cursor_with_panels();
        self.player_tools.cancel_continuous_hold();
        self.stop_terrain_edit_loop_sound();
        self.tracer.apply_camera_pose(snapshot.pose());
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
    use super::snapshot_controls::{self, SnapshotAction};
    ui.small(format!("File: {}", camera_snapshots.path().display()));
    let entries: Vec<_> = camera_snapshots
        .snapshots()
        .iter()
        .map(|s| (s.name.clone(), s.name.clone()))
        .collect();
    if camera_snapshots.is_empty() {
        ui.small("No saved cameras; the authored startup view is retained independently.");
    }
    let before = draft_name.clone();
    if snapshot_controls::selector(
        ui,
        "camera_snapshot_selector",
        "cameras",
        draft_name,
        &entries,
    ) {
        match CameraSnapshotLibrary::load(camera_snapshots.path().to_owned()) {
            Ok(reloaded) => {
                *camera_snapshots = reloaded;
                *status = Some("Refreshed cameras".into());
            }
            Err(error) => *status = Some(format!("Refresh failed: {error}")),
        }
    }
    if *draft_name != before {
        if let Some(snapshot) = camera_snapshots.find(draft_name) {
            *draft_description = snapshot.description.clone();
        }
    }
    ui.horizontal(|ui| {
        ui.label("Save name");
        ui.text_edit_singleline(draft_name);
    });
    ui.horizontal(|ui| {
        ui.label("Description");
        ui.text_edit_singleline(draft_description);
    });
    ui.small("Choose a saved camera to load, update or delete; enter a new name to save another. Delete keeps the current view.");
    let selected = camera_snapshots.find(draft_name).is_some();
    let action = snapshot_controls::actions(ui, "camera", !draft_name.trim().is_empty(), selected);
    let mut applied = None;
    match action {
        Some(SnapshotAction::Save) => match camera_snapshots.save_from_pose(
            draft_name,
            draft_description.clone(),
            current_pose,
            is_fly_mode,
        ) {
            Ok(name) => {
                *status = Some(format!("Saved '{name}'"));
                *draft_name = name;
            }
            Err(error) => *status = Some(format!("Save failed: {error}")),
        },
        Some(SnapshotAction::Load) => {
            applied = camera_snapshots.find(draft_name).cloned();
            if let Some(snapshot) = &applied {
                *status = Some(format!("Loaded '{}'", snapshot.name));
            }
        }
        Some(SnapshotAction::Delete) => {
            let name = draft_name.clone();
            match camera_snapshots.remove(&name) {
                Ok(true) => {
                    *status = Some(format!("Deleted '{name}'; current view retained"));
                    *draft_name = camera_snapshots
                        .snapshots()
                        .first()
                        .map(|s| s.name.clone())
                        .unwrap_or_else(|| CAMERA_SNAPSHOT_DRAFT_BASE_NAME.to_owned());
                    *draft_description = camera_snapshots
                        .find(draft_name)
                        .map(|s| s.description.clone())
                        .unwrap_or_default();
                }
                Ok(false) => *status = Some(format!("Camera '{name}' no longer exists")),
                Err(error) => *status = Some(format!("Delete failed: {error}")),
            }
        }
        None => {}
    }
    if let Some(status) = status.as_ref() {
        ui.label(status);
    }
    ui.collapsing("Current camera pose", |ui| {
        ui.monospace(format!(
            "pos [{:.3}, {:.3}, {:.3}] yaw {:.2} pitch {:.2} fov {:.2}",
            current_pose.position.x,
            current_pose.position.y,
            current_pose.position.z,
            current_pose.yaw_deg,
            current_pose.pitch_deg,
            current_pose.fov_deg
        ));
    });
    applied
}
