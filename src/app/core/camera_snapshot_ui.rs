use super::App;
use crate::app::camera_snapshots::{
    is_player_default_snapshot_name, CameraSnapshot, CameraSnapshotLibrary,
};
use crate::app::ui_text;
use crate::gameplay::CameraPose;
use anyhow::{anyhow, Result};

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
    error: &mut Option<String>,
    current_pose: CameraPose,
    is_fly_mode: bool,
) -> Option<CameraSnapshot> {
    ui_text::section(ui, "Add camera");
    ui.horizontal(|ui| {
        ui_text::label(ui, "Name");
        ui.add(egui::TextEdit::singleline(draft_name).desired_width(ui.available_width()));
    });
    ui.horizontal(|ui| {
        ui_text::label(ui, "Description");
        ui.add(egui::TextEdit::singleline(draft_description).desired_width(ui.available_width()));
    });
    if ui
        .add_enabled(
            !draft_name.trim().is_empty(),
            egui::Button::new("Add current camera"),
        )
        .clicked()
    {
        match camera_snapshots.add_from_pose(
            draft_name,
            draft_description.clone(),
            current_pose,
            is_fly_mode,
        ) {
            Ok(name) => {
                log::info!("[CAMERA_SNAPSHOT] Added '{name}'");
                *draft_name = camera_snapshots.unique_name(draft_name);
                draft_description.clear();
                *error = None;
            }
            Err(err) => record_error(error, format!("Add failed: {err}")),
        }
    }

    let mut applied = None;
    let mut action = None;
    if !camera_snapshots.is_empty() {
        ui_text::section(ui, "Saved cameras");
    }
    for snapshot in camera_snapshots.snapshots() {
        ui.push_id(&snapshot.name, |ui| {
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Delete").clicked() {
                        action = Some((snapshot.name.clone(), RowAction::Delete));
                    }
                    if ui
                        .button("Update")
                        .on_hover_text("Replace this snapshot with the current camera view")
                        .clicked()
                    {
                        action = Some((snapshot.name.clone(), RowAction::Update));
                    }
                    if ui
                        .button("Apply")
                        .on_hover_text(&snapshot.description)
                        .clicked()
                    {
                        applied = Some(snapshot.clone());
                        *error = None;
                        log::info!("[CAMERA_SNAPSHOT] Applied '{}'", snapshot.name);
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui_text::label(ui, &snapshot.name);
                    });
                });
            });
        });
    }
    if let Some((name, action)) = action {
        let result = match action {
            RowAction::Delete => camera_snapshots.remove(&name).map(|_| ()),
            RowAction::Update => {
                let description = camera_snapshots.find(&name).unwrap().description.clone();
                camera_snapshots
                    .save_from_pose(&name, description, current_pose, is_fly_mode)
                    .map(|_| ())
            }
        };
        match result {
            Ok(()) => {
                log::info!("[CAMERA_SNAPSHOT] {action:?} '{name}'");
                *error = None;
            }
            Err(err) => record_error(error, format!("{action:?} failed: {err}")),
        }
    }
    if let Some(error) = error.as_ref() {
        ui_text::warning(ui, error);
    }
    applied
}

#[derive(Debug, Clone, Copy)]
enum RowAction {
    Update,
    Delete,
}

fn record_error(error: &mut Option<String>, message: String) {
    log::error!("[CAMERA_SNAPSHOT] {message}");
    *error = Some(message);
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, PointerButton, Pos2, Rect, Vec2};

    struct Fixture {
        _directory: tempfile::TempDir,
        context: egui::Context,
        library: CameraSnapshotLibrary,
        name: String,
        description: String,
        error: Option<String>,
        pose: CameraPose,
    }

    impl Fixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            Self {
                library: CameraSnapshotLibrary::load(directory.path().join("cameras.toml"))
                    .unwrap(),
                _directory: directory,
                context: egui::Context::default(),
                name: "snapshot".into(),
                description: String::new(),
                error: None,
                pose: crate::app::camera_snapshots::player_default_camera_pose(),
            }
        }
        fn frame(&mut self, events: Vec<Event>) -> (egui::FullOutput, Option<CameraSnapshot>) {
            let mut applied = None;
            let output = self.context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(480., 1000.))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    applied = draw_camera_snapshots_ui(
                        ui,
                        &mut self.library,
                        &mut self.name,
                        &mut self.description,
                        &mut self.error,
                        self.pose,
                        true,
                    );
                },
            );
            (output, applied)
        }
        fn click(&mut self, label: &str, index: usize) -> Option<CameraSnapshot> {
            self.frame(vec![]);
            let (output, _) = self.frame(vec![]);
            let mut labels = Vec::new();
            for shape in &output.shapes {
                collect(&shape.shape, &mut labels);
            }
            let position = labels
                .iter()
                .filter(|(text, _)| text == label)
                .nth(index)
                .unwrap_or_else(|| panic!("missing button {label} at {index}: {labels:?}"))
                .1;
            self.frame(vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
            self.frame(vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ])
            .1
        }
    }
    fn collect(shape: &egui::Shape, labels: &mut Vec<(String, Pos2)>) {
        match shape {
            egui::Shape::Text(text) => labels.push((
                text.galley.job.text.clone(),
                text.pos + text.galley.size() * 0.5,
            )),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, labels);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn camera_rows_add_apply_update_and_delete_without_manual_refresh() {
        let mut fixture = Fixture::new();
        fixture.description = "First camera".into();
        fixture.click("Add current camera", 0);
        assert_eq!(fixture.library.snapshots().len(), 1);
        let first = fixture.library.snapshots()[0].name.clone();
        fixture.pose.yaw_deg = 75.;
        fixture.click("Add current camera", 0);
        assert_eq!(fixture.library.snapshots().len(), 2);
        let second = fixture.library.snapshots()[1].name.clone();
        let applied = fixture.click("Apply", 1).unwrap();
        assert_eq!(applied.name, second);
        assert_eq!(applied.yaw_deg, 75.);
        assert!(applied.fly_mode);
        fixture.pose.yaw_deg = 120.;
        fixture.click("Update", 0);
        assert_eq!(fixture.library.find(&first).unwrap().yaw_deg, 120.);
        assert_eq!(
            fixture.library.find(&first).unwrap().description,
            "First camera"
        );
        fixture.click("Delete", 1);
        assert_eq!(fixture.library.snapshots().len(), 1);
        assert!(fixture.library.find(&second).is_none());
        let reloaded = CameraSnapshotLibrary::load(fixture.library.path()).unwrap();
        assert_eq!(reloaded.snapshots().len(), 1);
        assert_eq!(reloaded.find(&first).unwrap().yaw_deg, 120.);
        assert!(fixture.error.is_none());
    }

    #[test]
    fn camera_snapshot_ui_has_no_file_refresh_pose_or_instructional_hints() {
        let mut fixture = Fixture::new();
        fixture.frame(vec![]);
        let (output, _) = fixture.frame(vec![]);
        let mut labels = Vec::new();
        for shape in &output.shapes {
            collect(&shape.shape, &mut labels);
        }
        let text = labels
            .iter()
            .map(|(text, _)| text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        for removed in [
            "File:",
            "Refresh",
            "Current camera pose",
            "No saved cameras",
            "Choose a saved camera",
        ] {
            assert!(!text.contains(removed), "{text}");
        }
        assert!(text.contains("Add current camera"));
    }
}
