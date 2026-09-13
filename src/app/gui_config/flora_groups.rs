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
const LEAF_RESPONSE: &[&str] = &[
    "leaf_flutter_strength",
    "leaf_local_displacement_voxels",
    "leaf_global_offset_scale",
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
                if name == "Leaves" && LEAF_RESPONSE.contains(&param.id.as_str()) {
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
        category(ui, "Shape & Motion", |ui| {
            controls(ui, flora, GROUND_MOTION, adjustables);
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
        category(ui, "Wind Motion", |ui| {
            if let Some(leaves) = config.iter().find(|s| s.name == "Leaves") {
                controls(ui, leaves, LEAF_RESPONSE, adjustables);
            }
            ui.small("Flutter Strength: wind-driven hinge torque and optical turning (requires inertia). Displacement: local position radius, up to 5 voxels; actual motion follows wind. Overall Offset: independent whole-leaf translation. These do not change sound or grass.");
            category(ui, "Legacy Motion (inertia off)", |ui| {
                controls(ui, flora, LEAF_MOTION, adjustables);
            });
        });
        category(ui, "Legacy Wind Curves (inertia off)", |ui| {
            ui.weak("Knee Bias: negative responds earlier; positive delays response until stronger wind.");
            controls(ui, flora, LEAF_CURVES, adjustables);
            enforce_leaf_curve_order(adjustables);
            draw_leaf_curve_previews(ui, adjustables);
        });
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
    fn leaf_response_controls_are_rendered_once_in_the_motion_menu() {
        let config = GuiConfigLoader::load();
        let flora = config.section.iter().find(|s| s.name == "Flora").unwrap();
        let mut adjustables = GuiAdjustables::from_config(&config);
        let context = egui::Context::default();
        context.memory_mut(|memory| memory.set_everything_is_visible(true));
        let output = context.run_ui(egui::RawInput::default(), |ui| {
            render(ui, &config.section, flora, &mut adjustables, &mut |_, _| {});
        });
        let mut text = Vec::new();
        for shape in output.shapes {
            collect_text(&shape.shape, &mut text);
        }
        for label in [
            "Local Flutter Strength (0 = off)",
            "Local Flutter Displacement (voxels)",
            "Overall Wind Offset (0 = off)",
        ] {
            assert_eq!(text.iter().filter(|s| s.as_str() == label).count(), 1);
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
                "Planting",
                "Ground Plants",
                "Tree",
                "Leaves"
            ]
        );
    }
}
