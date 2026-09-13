//! Shared presentation for snapshot libraries; storage and live-world actions stay with owners.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SnapshotAction {
    Save,
    Load,
    Delete,
}

pub(super) fn selector(
    ui: &mut egui::Ui,
    id: &str,
    noun: &str,
    selected: &mut String,
    entries: &[(String, String)],
) -> bool {
    let label = entries
        .iter()
        .find(|(key, _)| key == selected)
        .map(|(_, label)| label.clone())
        .unwrap_or_else(|| {
            if entries.is_empty() {
                format!("No saved {noun}")
            } else {
                format!("Select {noun}")
            }
        });
    ui.horizontal(|ui| {
        ui.label(format!("Saved {noun}"));
        egui::ComboBox::from_id_salt(id)
            .selected_text(label)
            .show_ui(ui, |ui| {
                for (key, label) in entries {
                    ui.selectable_value(selected, key.clone(), label);
                }
            });
        ui.button("Refresh").clicked()
    })
    .inner
}

pub(super) fn actions(
    ui: &mut egui::Ui,
    noun: &str,
    can_save: bool,
    can_load_delete: bool,
) -> Option<SnapshotAction> {
    ui.horizontal(|ui| {
        let mut action = None;
        for (kind, label, enabled) in [
            (SnapshotAction::Save, "Save", can_save),
            (SnapshotAction::Load, "Load", can_load_delete),
            (SnapshotAction::Delete, "Delete", can_load_delete),
        ] {
            if ui
                .add_enabled(enabled, egui::Button::new(format!("{label} {noun}")))
                .clicked()
            {
                action = Some(kind);
            }
        }
        action
    })
    .inner
}
