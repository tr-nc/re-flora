//! Cached directory listing and the terrain save/load controls. No world or GPU access.
use super::*;
use std::path::PathBuf;

#[derive(Default)]
pub(super) struct SnapshotSelector {
    directory: Option<PathBuf>,
    paths: Vec<String>,
    error: Option<String>,
}

#[derive(Clone, Copy)]
pub(in crate::app::core) enum SnapshotAction {
    Save,
    Load,
    Delete,
}

fn snapshot_directory(path: &str) -> PathBuf {
    Path::new(path)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .to_owned()
}

fn list_snapshots(directory: &Path) -> Result<Vec<String>> {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error).context("read terrain save directory"),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|ext| ext == "rflterrain")
        {
            paths.push(entry.path().to_string_lossy().into_owned());
        }
    }
    paths.sort();
    Ok(paths)
}

impl TerrainPersistenceRuntime {
    fn refresh_snapshots(&mut self) {
        let directory = snapshot_directory(&self.snapshot_path);
        match list_snapshots(&directory) {
            Ok(paths) => {
                self.selector.paths = paths;
                self.selector.error = None;
            }
            Err(error) => {
                self.selector.paths.clear();
                self.selector.error = Some(error.to_string());
            }
        }
        self.selector.directory = Some(directory);
    }

    pub(in crate::app::core) fn snapshot_controls(
        &mut self,
        ui: &mut egui::Ui,
    ) -> Option<SnapshotAction> {
        if self.selector.directory.as_ref() != Some(&snapshot_directory(&self.snapshot_path)) {
            self.refresh_snapshots();
        }
        let mut action = None;
        ui.horizontal(|ui| {
            ui.label("Saved terrains");
            let label = if self.selector.paths.contains(&self.snapshot_path) {
                Path::new(&self.snapshot_path)
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            } else if self.selector.paths.is_empty() {
                "No saved terrains".to_owned()
            } else {
                "Select a terrain".to_owned()
            };
            egui::ComboBox::from_id_salt("terrain_snapshot_selector")
                .selected_text(label)
                .show_ui(ui, |ui| {
                    for path in &self.selector.paths {
                        let label = Path::new(path).file_name().unwrap().to_string_lossy();
                        ui.selectable_value(&mut self.snapshot_path, path.clone(), label.as_ref());
                    }
                });
            if ui.button("Refresh").clicked() {
                self.refresh_snapshots();
            }
        });
        ui.horizontal(|ui| {
            ui.label("Save path");
            ui.text_edit_singleline(&mut self.snapshot_path);
        });
        ui.small("Choose a saved terrain, or enter a new .rflterrain path to save another.");
        ui.horizontal(|ui| {
            let ready = self.can_start_operation();
            let selected = self.selector.paths.contains(&self.snapshot_path);
            if ui
                .add_enabled(
                    ready && !self.snapshot_path.trim().is_empty(),
                    egui::Button::new("Save terrain"),
                )
                .clicked()
            {
                action = Some(SnapshotAction::Save);
            }
            if ui
                .add_enabled(ready && selected, egui::Button::new("Load terrain"))
                .clicked()
            {
                action = Some(SnapshotAction::Load);
            }
            if ui
                .add_enabled(ready && selected, egui::Button::new("Delete terrain"))
                .clicked()
            {
                action = Some(SnapshotAction::Delete);
            }
            ui.label(self.status_label());
        });
        if let Some(error) = &self.selector.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
        action
    }

    pub(in crate::app::core) fn delete_selected_snapshot(&mut self) {
        if !self.can_start_operation() || !self.selector.paths.contains(&self.snapshot_path) {
            return;
        }
        let path = self.snapshot_path.clone();
        match std::fs::remove_file(&path) {
            Ok(()) => {
                log::info!("[TERRAIN_PERSISTENCE] deleted path={path}; live garden retained");
                self.refresh_snapshots();
                if let Some(next) = self.selector.paths.first() {
                    self.snapshot_path = next.clone();
                }
                self.status = TerrainPersistenceStatus::Ready;
            }
            Err(error) => {
                self.selector.error = Some(format!("Delete failed: {error}"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selector_lists_only_snapshot_files_and_deletes_only_the_selected_save() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.rflterrain");
        let b = dir.path().join("b.rflterrain");
        std::fs::write(&a, b"a").unwrap();
        std::fs::write(&b, b"b").unwrap();
        std::fs::write(dir.path().join("notes.txt"), b"keep").unwrap();
        std::fs::create_dir(dir.path().join("folder.rflterrain")).unwrap();
        let mut runtime =
            TerrainPersistenceRuntime::from_plan(&crate::TerrainPersistencePlan::default(), false)
                .unwrap();
        runtime.snapshot_path = b.to_string_lossy().into_owned();
        runtime.refresh_snapshots();
        assert_eq!(
            runtime.selector.paths,
            vec![
                a.to_string_lossy().into_owned(),
                b.to_string_lossy().into_owned()
            ]
        );
        runtime.delete_selected_snapshot();
        assert!(!b.exists());
        assert_eq!(std::fs::read(&a).unwrap(), b"a");
        assert_eq!(runtime.snapshot_path, a.to_string_lossy());
        assert!(dir.path().join("notes.txt").exists());
        runtime.delete_selected_snapshot();
        assert!(runtime.selector.paths.is_empty());
        assert!(runtime.allows_world_updates());
        assert!(list_snapshots(&dir.path().join("missing"))
            .unwrap()
            .is_empty());
    }
}
