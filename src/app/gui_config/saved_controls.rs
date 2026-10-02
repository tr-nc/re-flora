//! The public custom-settings UI accepts selectors into the saved document, never
//! arbitrary mutable values. Non-capturing function pointers cannot bind App state.
use super::search::SearchFilter;
use crate::app::gui_config_model::SavedCustomSettings;

pub struct SavedControls<'a> {
    ui: &'a mut egui::Ui,
    settings: &'a mut SavedCustomSettings,
    filter: SearchFilter,
    path: String,
    matches: usize,
}

impl<'a> SavedControls<'a> {
    #[cfg(test)]
    pub(crate) fn for_test(ui: &'a mut egui::Ui, settings: &'a mut SavedCustomSettings) -> Self {
        Self::new(ui, settings)
    }
    pub(super) fn new(ui: &'a mut egui::Ui, settings: &'a mut SavedCustomSettings) -> Self {
        Self {
            ui,
            settings,
            filter: SearchFilter::default(),
            path: String::new(),
            matches: 0,
        }
    }

    pub(super) fn filtered(
        ui: &'a mut egui::Ui,
        settings: &'a mut SavedCustomSettings,
        filter: &SearchFilter,
        path: &str,
    ) -> Self {
        Self {
            ui,
            settings,
            filter: filter.clone(),
            path: path.to_owned(),
            matches: 0,
        }
    }

    pub(super) fn matches(&self) -> usize {
        self.matches
    }

    fn show(&mut self, label: &str) -> bool {
        if !self.filter.matches([self.path.as_str(), label]) {
            return false;
        }
        if self.filter.is_active() && self.matches == 0 {
            self.ui.weak(&self.path);
        }
        self.matches += 1;
        true
    }

    fn hidden_response(&self, label: &str) -> egui::Response {
        // Do not allocate layout space or bind a mutable value for a hidden control.
        self.ui.interact(
            egui::Rect::from_min_size(self.ui.cursor().min, egui::Vec2::ZERO),
            self.ui.make_persistent_id(("hidden_saved_control", label)),
            egui::Sense::hover(),
        )
    }

    pub fn slider(
        &mut self,
        field: fn(&mut SavedCustomSettings) -> &mut f32,
        range: std::ops::RangeInclusive<f32>,
        label: &str,
        step: f64,
        logarithmic: bool,
    ) -> egui::Response {
        if !self.show(label) {
            return self.hidden_response(label);
        }
        self.ui
            .push_id(("saved_control", label), |ui| {
                ui.add(
                    egui::Slider::new(field(self.settings), range)
                        .text(label)
                        .step_by(step)
                        .logarithmic(logarithmic),
                )
            })
            .inner
    }

    pub fn toggle<T: Copy + PartialEq>(
        &mut self,
        field: fn(&mut SavedCustomSettings) -> &mut T,
        checked: T,
        unchecked: T,
        label: &str,
    ) -> egui::Response {
        if !self.show(label) {
            return self.hidden_response(label);
        }
        let value = field(self.settings);
        let mut enabled = *value == checked;
        let response = self
            .ui
            .push_id(("saved_control", label), |ui| {
                ui.checkbox(&mut enabled, label)
            })
            .inner;
        if response.changed() {
            *value = if enabled { checked } else { unchecked };
        }
        response
    }

    pub fn read<T: Copy>(&self, field: fn(&SavedCustomSettings) -> &T) -> T {
        *field(self.settings)
    }
    pub fn small(&mut self, text: impl Into<egui::RichText>) {
        if !self.filter.is_active() {
            self.ui.small(text);
        }
    }
    pub fn label(&mut self, text: &str) {
        if !self.filter.is_active() {
            self.ui.label(text);
        }
    }
}

/// Escape hatch for explicitly temporary experiments. No raw UI is provided until
/// the caller states a reason, which is also displayed to the player.
pub struct TemporaryControls<'a> {
    ui: &'a mut egui::Ui,
    filter: SearchFilter,
    path: String,
    matches: usize,
}
impl<'a> TemporaryControls<'a> {
    pub(super) fn new(ui: &'a mut egui::Ui) -> Self {
        Self {
            ui,
            filter: SearchFilter::default(),
            path: String::new(),
            matches: 0,
        }
    }
    pub(super) fn filtered(ui: &'a mut egui::Ui, filter: &SearchFilter, path: &str) -> Self {
        Self {
            ui,
            filter: filter.clone(),
            path: path.to_owned(),
            matches: 0,
        }
    }
    pub(super) fn matches(&self) -> usize {
        self.matches
    }

    pub fn not_saved(&mut self, reason: &'static str, draw: impl FnOnce(&mut egui::Ui)) {
        assert!(
            !reason.trim().is_empty(),
            "Temporary controls require a reason"
        );
        if !self.filter.matches([self.path.as_str(), reason]) {
            return;
        }
        if self.filter.is_active() {
            self.ui.weak(&self.path);
        }
        self.matches += 1;
        self.ui.small(format!("Not saved — {reason}"));
        draw(self.ui);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::gui_config::DebugSettings;
    use crate::app::gui_config_loader::GuiConfigLoader;

    #[test]
    fn custom_controls_search_their_labels_without_hidden_layout_or_mutation() {
        let mut settings = DebugSettings::load();
        let context = egui::Context::default();
        let filter = SearchFilter::new("clarity");
        let _ = context.run_ui(Default::default(), |ui| {
            let mut controls = SavedControls::filtered(
                ui,
                &mut settings.config.custom,
                &filter,
                "Future / Tuning",
            );
            let before = controls.ui.cursor();
            let hidden = controls.slider(
                |_| panic!("hidden field was accessed"),
                0.0..=1.0,
                "Unrelated",
                0.1,
                false,
            );
            assert!(!hidden.changed());
            assert_eq!(controls.ui.cursor(), before);
            controls.slider(
                |s| &mut s.future_control_fixture,
                0.0..=1.0,
                "Future tuning clarity",
                0.1,
                false,
            );
            assert_eq!(controls.matches(), 1);
        });
        let _ = context.run_ui(Default::default(), |ui| {
            let mut controls =
                TemporaryControls::filtered(ui, &SearchFilter::new("lighting"), "Wind");
            controls.not_saved("prototype", |_| panic!("unmatched temporary tool rendered"));
            assert_eq!(controls.matches(), 0);
        });
    }

    #[test]
    fn newly_declared_slider_needs_no_save_hook() {
        let mut settings = DebugSettings::load();
        let context = egui::Context::default();
        let mut rect = egui::Rect::NOTHING;
        let mut draw = |events| {
            let _ = context.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    rect = SavedControls::new(ui, &mut settings.config.custom)
                        .slider(
                            |s| &mut s.future_control_fixture,
                            0.0..=1.0,
                            "New setting",
                            0.125,
                            false,
                        )
                        .rect;
                },
            );
            rect
        };
        draw(Vec::new());
        let rect = draw(Vec::new());
        let pos = egui::pos2(
            rect.left() + context.global_style().spacing.slider_width * 0.75,
            rect.center().y,
        );
        for pressed in [true, false] {
            draw(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
        }
        assert_ne!(settings.future_control_fixture, 0.0);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("gui.toml");
        settings.save_to_path(&path).unwrap();
        let reloaded = DebugSettings::from_config(GuiConfigLoader::load_from_path(&path));
        assert_eq!(
            settings.future_control_fixture,
            reloaded.future_control_fixture
        );
    }

    #[test]
    fn temporary_controls_require_and_display_a_reason() {
        let context = egui::Context::default();
        let output = context.run_ui(Default::default(), |ui| {
            TemporaryControls::new(ui).not_saved("test experiment", |ui| {
                ui.label("temporary");
            });
        });
        let text = format!("{:?}", output.shapes);
        assert!(text.contains("Not saved"));
        assert!(text.contains("test experiment"));
    }
}
