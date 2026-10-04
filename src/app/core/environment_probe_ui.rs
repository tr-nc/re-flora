//! Session-only probe adjustment controls; runtime diagnostics belong in logs.
use crate::app::ui_text;
use crate::ddgi::SUPPORTED_DDGI_SPACINGS_VOXELS;
use crate::environment_probes::{
    EnvironmentProbeVisualizationFilter, EnvironmentProbeVisualizationMode,
    EnvironmentProbeVisualizationSettings,
};

pub(super) fn draw(
    ui: &mut egui::Ui,
    spacing: &mut u32,
    active_spacing: u32,
    visualization: &mut EnvironmentProbeVisualizationSettings,
) -> bool {
    ui_text::hint(
        ui,
        "Not saved — probe rebuild and visualization controls are session-only.",
    );
    egui::ComboBox::from_label("Spacing (voxels)")
        .selected_text(spacing.to_string())
        .show_ui(ui, |ui| {
            for value in SUPPORTED_DDGI_SPACINGS_VOXELS {
                ui.selectable_value(spacing, value, value.to_string());
            }
        });
    let rebuild = ui
        .add_enabled(
            *spacing != active_spacing,
            egui::Button::new("Apply / Rebuild"),
        )
        .clicked();
    ui.checkbox(&mut visualization.enabled, "Visualize probes");
    if visualization.enabled {
        egui::ComboBox::from_label("Display")
            .selected_text(visualization.mode.label())
            .show_ui(ui, |ui| {
                for mode in EnvironmentProbeVisualizationMode::ALL {
                    ui.selectable_value(&mut visualization.mode, mode, mode.label());
                }
            });
        egui::ComboBox::from_label("Filter")
            .selected_text(visualization.filter.label())
            .show_ui(ui, |ui| {
                for filter in EnvironmentProbeVisualizationFilter::ALL {
                    ui.selectable_value(&mut visualization.filter, filter, filter.label());
                }
            });
        ui.add(
            egui::Slider::new(&mut visualization.camera_radius_voxels, 0.0..=512.0)
                .text("Camera radius (vox; 0 = all)"),
        );
        ui.add(
            egui::Slider::new(&mut visualization.instance_stride, 1..=64).text("Instance stride"),
        );
        ui.add(
            egui::Slider::new(&mut visualization.marker_size_voxels, 0.5..=12.0)
                .text("Marker size (voxels)"),
        );
        ui.checkbox(&mut visualization.depth_tested, "Depth tested");
    }
    rebuild
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(shape: &egui::Shape, out: &mut String) {
        match shape {
            egui::Shape::Text(t) => {
                out.push_str(&t.galley.job.text);
                out.push('\n');
            }
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    text(shape, out);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn visualization_controls_are_hidden_not_disabled_and_diagnostics_are_absent() {
        let context = egui::Context::default();
        let mut settings = EnvironmentProbeVisualizationSettings::default();
        let mut spacing = 32;
        for enabled in [false, true, false] {
            settings.enabled = enabled;
            let output = context.run_ui(egui::RawInput::default(), |ui| {
                draw(ui, &mut spacing, 32, &mut settings);
            });
            let mut labels = String::new();
            for shape in &output.shapes {
                text(&shape.shape, &mut labels);
            }
            assert!(labels.contains("Spacing (voxels)"));
            assert!(labels.contains("Visualize probes"));
            for control in [
                "Display",
                "Filter",
                "Camera radius",
                "Instance stride",
                "Marker size",
                "Depth tested",
            ] {
                assert_eq!(labels.contains(control), enabled, "{control}: {labels}");
            }
            for diagnostic in [
                "Current",
                "Revisions",
                "Allocated",
                "Submitted instances",
                "active token",
            ] {
                assert!(!labels.contains(diagnostic));
            }
        }
    }
}
