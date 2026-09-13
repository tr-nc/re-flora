//! The public custom-settings UI accepts selectors into the saved document, never
//! arbitrary mutable values. Non-capturing function pointers cannot bind App state.
use crate::app::gui_config_model::SavedCustomSettings;

pub struct SavedControls<'a> {
    ui: &'a mut egui::Ui,
    settings: &'a mut SavedCustomSettings,
}

impl<'a> SavedControls<'a> {
    #[cfg(test)]
    pub(crate) fn for_test(ui: &'a mut egui::Ui, settings: &'a mut SavedCustomSettings) -> Self {
        Self::new(ui, settings)
    }
    pub(super) fn new(ui: &'a mut egui::Ui, settings: &'a mut SavedCustomSettings) -> Self {
        Self { ui, settings }
    }

    pub fn slider(
        &mut self,
        field: fn(&mut SavedCustomSettings) -> &mut f32,
        range: std::ops::RangeInclusive<f32>,
        label: &str,
        step: f64,
        logarithmic: bool,
    ) -> egui::Response {
        self.ui.add(
            egui::Slider::new(field(self.settings), range)
                .text(label)
                .step_by(step)
                .logarithmic(logarithmic),
        )
    }

    pub fn toggle<T: Copy + PartialEq>(
        &mut self,
        field: fn(&mut SavedCustomSettings) -> &mut T,
        checked: T,
        unchecked: T,
        label: &str,
    ) -> egui::Response {
        let value = field(self.settings);
        let mut enabled = *value == checked;
        let response = self.ui.checkbox(&mut enabled, label);
        if response.changed() {
            *value = if enabled { checked } else { unchecked };
        }
        response
    }

    pub fn read<T: Copy>(&self, field: fn(&SavedCustomSettings) -> &T) -> T {
        *field(self.settings)
    }
    pub fn small(&mut self, text: impl Into<egui::RichText>) {
        self.ui.small(text);
    }
    pub fn label(&mut self, text: &str) {
        self.ui.label(text);
    }
    pub fn enabled(&mut self, enabled: bool, draw: impl FnOnce(&mut SavedControls<'_>)) {
        self.ui.add_enabled_ui(enabled, |ui| {
            draw(&mut SavedControls::new(ui, self.settings))
        });
    }
}

/// Escape hatch for explicitly temporary experiments. No raw UI is provided until
/// the caller states a reason, which is also displayed to the player.
pub struct TemporaryControls<'a> {
    ui: &'a mut egui::Ui,
}
impl<'a> TemporaryControls<'a> {
    pub(super) fn new(ui: &'a mut egui::Ui) -> Self {
        Self { ui }
    }
    pub fn not_saved(&mut self, reason: &'static str, draw: impl FnOnce(&mut egui::Ui)) {
        assert!(
            !reason.trim().is_empty(),
            "Temporary controls require a reason"
        );
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
            rect.left() + context.style().spacing.slider_width * 0.75,
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
