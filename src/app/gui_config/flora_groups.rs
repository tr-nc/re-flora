//! Flora's display hierarchy. Stored sections, parameter IDs and values remain config-owned.
//! Branches contain only categories; controls live in the terminal categories.
use super::{
    debug_groups, draw_leaf_curve_previews, enforce_flora_natural_bend_order,
    enforce_leaf_curve_order, render_gui_param_from_config, GuiAdjustables,
};
use crate::app::gui_config_model::GuiSection;

const DISTRIBUTION: &[&str] = &[
    "special_flora_plants_per_release",
    "special_flora_cluster_radius_voxels",
    "special_flora_min_spacing_voxels",
    "special_flora_cluster_bias",
    "special_flora_outlier_chance",
];
const GROUND_MOTION: &[&str] = &[
    "grass_natural_bend_min_voxels",
    "grass_natural_bend_max_voxels",
    "flora_bend_height_power",
    "grass_vibration_amplitude_voxels",
    "grass_vibration_primary_speed",
    "grass_vibration_secondary_speed",
];
const GRASS_COLORS: &[&str] = &[
    "grass_bottom_dark_color",
    "grass_bottom_light_color",
    "grass_tip_dark_color",
    "grass_tip_light_color",
];
const LEAF_MOTION: &[&str] = &[
    "leaf_paddle_amplitude_voxels",
    "leaf_paddle_primary_speed",
    "leaf_paddle_secondary_speed",
];
const LEAF_RESPONSE: &[&str] = &["leaf_global_offset_scale"];
const LEAF_AMPLITUDE: &[&str] = &["leaf_local_displacement_voxels"];
const LEAF_AMPLITUDE_CURVE: &[&str] = &[
    "leaf_flutter_amplitude_low",
    "leaf_flutter_amplitude_high",
    "leaf_flutter_wind_start",
    "leaf_flutter_wind_full",
    "leaf_flutter_wind_knee",
];
const LEAF_CURVES: &[&str] = &[
    "leaf_paddle_amplitude_wind_start_strength",
    "leaf_paddle_amplitude_wind_full_strength",
    "leaf_paddle_amplitude_wind_knee_bias",
    "leaf_paddle_frequency_wind_start_strength",
    "leaf_paddle_frequency_wind_full_strength",
    "leaf_paddle_frequency_wind_knee_bias",
    "leaf_paddle_frequency_min_multiplier",
    "leaf_paddle_frequency_max_multiplier",
];
const LEAF_FREQUENCY: &[&str] = &["leaf_flutter_frequency_ceiling_hz"];
const LEAF_FREQUENCY_CURVE: &[&str] = &[
    "leaf_flutter_frequency_low_hz",
    "leaf_flutter_frequency_high_hz",
    "leaf_flutter_frequency_start",
    "leaf_flutter_frequency_full",
    "leaf_flutter_frequency_knee",
];
const PARAM_GROUPS: &[&[&str]] = &[
    DISTRIBUTION,
    GROUND_MOTION,
    GRASS_COLORS,
    LEAF_MOTION,
    LEAF_CURVES,
];

fn is_grouped(id: &str) -> bool {
    PARAM_GROUPS.iter().any(|group| group.contains(&id))
}

fn category(ui: &mut egui::Ui, title: &str, contents: impl FnOnce(&mut egui::Ui)) {
    egui::CollapsingHeader::new(title)
        .id_salt(("flora_controls", title))
        .default_open(false)
        .show(ui, contents);
}

fn controls(
    ui: &mut egui::Ui,
    section: &GuiSection,
    ids: &[&str],
    adjustables: &mut GuiAdjustables,
) {
    for id in ids {
        if let Some(param) = section.param.iter().find(|param| param.id == *id) {
            render_gui_param_from_config(ui, param, &section.name, adjustables);
        }
    }
}

fn stored_section(
    ui: &mut egui::Ui,
    config: &[GuiSection],
    name: &str,
    title: &str,
    adjustables: &mut GuiAdjustables,
    after_section: &mut impl FnMut(&str, &mut egui::Ui),
) {
    if let Some(section) = config.iter().find(|section| section.name == name) {
        category(ui, title, |ui| {
            for param in &section.param {
                if name == "Leaves"
                    && [
                        LEAF_RESPONSE,
                        LEAF_AMPLITUDE,
                        LEAF_AMPLITUDE_CURVE,
                        LEAF_FREQUENCY,
                        LEAF_FREQUENCY_CURVE,
                    ]
                    .iter()
                    .any(|group| group.contains(&param.id.as_str()))
                {
                    continue;
                }
                render_gui_param_from_config(ui, param, name, adjustables);
            }
            after_section(name, ui);
        });
    }
}

pub(super) fn render(
    ui: &mut egui::Ui,
    config: &[GuiSection],
    flora: &GuiSection,
    adjustables: &mut GuiAdjustables,
    after_section: &mut impl FnMut(&str, &mut egui::Ui),
) {
    if let Some(debug) = config.iter().find(|section| section.name == "Debug") {
        debug_groups::render(ui, debug, adjustables, Some("Flora"));
    }
    category(ui, "Planting", |ui| {
        category(ui, "Distribution", |ui| {
            controls(ui, flora, DISTRIBUTION, adjustables);
        });
        stored_section(
            ui,
            config,
            "Flora Spawn Animation",
            "Spawn Animation",
            adjustables,
            after_section,
        );
    });
    category(ui, "Ground Plants", |ui| {
        category(ui, "Rest Shape", |ui| {
            controls(ui, flora, &GROUND_MOTION[..2], adjustables);
            enforce_flora_natural_bend_order(adjustables);
        });
        category(ui, "Grass Colors", |ui| {
            controls(ui, flora, GRASS_COLORS, adjustables);
        });
        stored_section(
            ui,
            config,
            "FloraVariation",
            "Color Variation",
            adjustables,
            after_section,
        );
        stored_section(
            ui,
            config,
            "Purple Allium",
            "Purple Allium",
            adjustables,
            after_section,
        );
    });
    // The existing Tree editor is supplied by DebugSettings, not duplicated here.
    after_section("Flora", ui);
    category(ui, "Leaves", |ui| {
        stored_section(
            ui,
            config,
            "Leaves",
            "Appearance & Lighting",
            adjustables,
            after_section,
        );
    });
    // Preserve visibility of custom/future controls without spilling them onto a branch level.
    if flora.param.iter().any(|param| !is_grouped(&param.id)) {
        category(ui, "Other Flora Settings", |ui| {
            for param in &flora.param {
                if !is_grouped(&param.id) {
                    render_gui_param_from_config(ui, param, &flora.name, adjustables);
                }
            }
        });
    }
}

// Wind owns response controls; stored sections and persistence remain unchanged.
pub(super) fn render_wind(
    ui: &mut egui::Ui,
    config: &[GuiSection],
    adjustables: &mut GuiAdjustables,
) {
    use crate::app::flutter_response_editor::{draw, Kind};
    if let Some(flora) = config.iter().find(|s| s.name == "Flora") {
        egui::CollapsingHeader::new("Grass")
            .default_open(true)
            .show(ui, |ui| {
                controls(ui, flora, &GROUND_MOTION[2..3], adjustables);
                if let Some(grass) = config.iter().find(|s| s.name == "Grass Wind Response") {
                    for (title, scale, kind) in [
                        (
                            "Grass Amplitude Response",
                            "grass_sway_amplitude_scale",
                            Kind::GrassAmplitude,
                        ),
                        (
                            "Grass Frequency Response",
                            "grass_sway_frequency_scale",
                            Kind::GrassFrequency,
                        ),
                    ] {
                        ui.label(title);
                        controls(ui, grass, &[scale], adjustables);
                        draw(ui, adjustables, kind);
                    }
                }
                if !adjustables.flora_inertial_response.value {
                    ui.label("Direct grass vibration (inertia off)");
                    controls(ui, flora, &GROUND_MOTION[3..], adjustables);
                }
            });
        egui::CollapsingHeader::new("Leaves")
            .id_salt("wind_leaf_response")
            .default_open(true)
            .show(ui, |ui| {
                if let Some(leaves) = config.iter().find(|s| s.name == "Leaves") {
                    controls(ui, leaves, LEAF_RESPONSE, adjustables);
                    ui.label("Leaf Amplitude Response");
                    controls(ui, leaves, LEAF_AMPLITUDE, adjustables);
                    draw(ui, adjustables, Kind::Amplitude);
                    ui.label("Leaf Frequency Response");
                    controls(ui, leaves, LEAF_FREQUENCY, adjustables);
                    draw(ui, adjustables, Kind::Frequency);
                }
                // These controls are still used when the common inertial solver is off.
                // Show their actual purpose, only when applicable, without another menu.
                if !adjustables.flora_inertial_response.value {
                    ui.label("Direct leaf motion (inertia off)");
                    controls(ui, flora, LEAF_MOTION, adjustables);
                    controls(ui, flora, LEAF_CURVES, adjustables);
                    enforce_leaf_curve_order(adjustables);
                    draw_leaf_curve_previews(ui, adjustables);
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::gui_config::GuiConfigLoader;
    use std::collections::BTreeSet;

    #[test]
    fn every_flora_parameter_has_exactly_one_category() {
        let config = GuiConfigLoader::load();
        let flora = config.section.iter().find(|s| s.name == "Flora").unwrap();
        let assignments = PARAM_GROUPS
            .iter()
            .flat_map(|g| g.iter().copied())
            .collect::<Vec<_>>();
        let unique = assignments.iter().copied().collect::<BTreeSet<_>>();
        assert_eq!(unique.len(), assignments.len(), "duplicate Flora control");
        assert_eq!(unique, flora.param.iter().map(|p| p.id.as_str()).collect());
        assert!(!is_grouped("future_flora_control"));
    }

    fn collect_text(shape: &egui::Shape, text: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(shape) => text.push(shape.galley.job.text.clone()),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect_text(shape, text);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn response_curves_are_visible_without_opening_child_menus() {
        let config = GuiConfigLoader::load();
        let mut adjustables = GuiAdjustables::from_config(&config);
        let context = egui::Context::default();
        let output = context.run_ui(egui::RawInput::default(), |ui| {
            render_wind(ui, &config.section, &mut adjustables);
        });
        let mut text = Vec::new();
        for shape in output.shapes {
            collect_text(&shape.shape, &mut text);
        }
        for title in [
            "Grass Amplitude Response",
            "Grass Frequency Response",
            "Leaf Amplitude Response",
            "Leaf Frequency Response",
        ] {
            assert_eq!(text.iter().filter(|s| s.as_str() == title).count(), 1);
        }
        assert!(!text.iter().any(|s| s.contains("Legacy")));
    }

    #[test]
    fn non_inertial_controls_have_functional_names_and_no_child_menus() {
        let config = GuiConfigLoader::load();
        let mut adjustables = GuiAdjustables::from_config(&config);
        adjustables.flora_inertial_response.value = false;
        let context = egui::Context::default();
        let output = context.run_ui(egui::RawInput::default(), |ui| {
            render_wind(ui, &config.section, &mut adjustables);
        });
        let mut text = Vec::new();
        for shape in output.shapes {
            collect_text(&shape.shape, &mut text);
        }
        assert!(text
            .iter()
            .any(|s| s == "Direct grass vibration (inertia off)"));
        assert!(text.iter().any(|s| s == "Direct leaf motion (inertia off)"));
        assert!(!text.iter().any(|s| s.contains("Legacy")));
    }

    #[test]
    fn leaf_response_controls_are_rendered_once_in_the_motion_menu() {
        let config = GuiConfigLoader::load();
        let flora = config.section.iter().find(|s| s.name == "Flora").unwrap();
        let mut adjustables = GuiAdjustables::from_config(&config);
        let context = egui::Context::default();
        context.memory_mut(|memory| memory.set_everything_is_visible(true));
        let output = context.run_ui(egui::RawInput::default(), |ui| {
            render(ui, &config.section, flora, &mut adjustables, &mut |_, _| {});
            render_wind(ui, &config.section, &mut adjustables);
        });
        let mut text = Vec::new();
        for shape in output.shapes {
            collect_text(&shape.shape, &mut text);
        }
        for removed in [
            "Target natural frequency",
            "Scale 1 keeps",
            "frames/s",
            "High frequency may look",
            "Base Flutter Frequency",
            "Strong-Wind Frequency Scale",
            "Frequency Start Wind",
            "Frequency Full Wind",
            "Frequency Curve Bias",
        ] {
            assert!(
                !text.iter().any(|s| s.contains(removed)),
                "unexpected display-only text: {removed}"
            );
        }
        for label in [
            "Amplitude Scaling (voxels)",
            "Overall Wind Offset (0 = off)",
            "Frequency Scaling (Hz)",
        ] {
            assert_eq!(
                text.iter().filter(|s| s.as_str() == label).count(),
                if label == "Overall Wind Offset (0 = off)" {
                    1
                } else {
                    2
                }
            );
        }
    }

    #[test]
    fn flora_root_displays_only_closed_categories() {
        let config = GuiConfigLoader::load();
        let flora = config.section.iter().find(|s| s.name == "Flora").unwrap();
        let mut adjustables = GuiAdjustables::from_config(&config);
        let context = egui::Context::default();
        let output = context.run_ui(egui::RawInput::default(), |ui| {
            render(
                ui,
                &config.section,
                flora,
                &mut adjustables,
                &mut |name, ui| {
                    assert_eq!(name, "Flora", "closed children must not draw controls");
                    ui.collapsing("Tree", |_| {});
                },
            );
        });
        let mut text = Vec::new();
        for shape in output.shapes {
            collect_text(&shape.shape, &mut text);
        }
        assert_eq!(
            text,
            [
                "Growth & Fruiting",
                "Apple Appearance (A/B)",
                "Planting",
                "Ground Plants",
                "Tree",
                "Leaves"
            ]
        );
    }
}
