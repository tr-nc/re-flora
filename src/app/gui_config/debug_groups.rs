//! Presentation only: parameter values, ranges, conditions and saving stay config-owned.
use super::{render_gui_param_from_config, GuiAdjustables};
use crate::app::gui_config_model::GuiSection;
use crate::app::ui_text;

struct ControlGroup {
    parent: Option<&'static str>,
    title: &'static str,
    description: &'static str,
    initially_open: bool,
    params: &'static [&'static str],
}

const GROUPS: &[ControlGroup] = &[
    ControlGroup {
        parent: None,
        title: "Scene Depth Outlines",
        description: "One internal scene pixel, before nearest upscaling; UI stays untouched. Relative threshold and minimum gap reject small depth changes; opposing gradients reject planar slopes. Sky outlines stay inside objects. Thin detail strength protects grass/leaves. Lower strength for subtle separation. Depth alone does not outline same-depth colors or every crease; camera movement can still reveal the native pixel grid.",
        initially_open: false,
        params: &["depth_outline_enabled", "depth_outline_strength", "depth_outline_color", "depth_outline_relative_threshold", "depth_outline_minimum_gap", "depth_outline_softness", "depth_outline_sky_strength", "depth_outline_thin_strength"],
    },
    ControlGroup {
        parent: None,
        title: "Stem Model Pixelization",
        description: "Grass in both Analytic Reference and Square Color Bands: unchecked = continuous silhouette sampling; checked = model-anchored pixelized silhouette and depth. Analytic keeps ray intersections; square uses hardware-raster tiles and prepared poses. Flower sampling and CPU climbing stems are unchanged.",
        initially_open: false,
        params: &["grass_band_pixelization", "flower_stem_model_resolution"],
    },
    ControlGroup {
        parent: None,
        title: "Stem Geometry & Color Bands",
        description: "Grass unchecked: original voxel geometry. Checked: selected stem geometry. Square bands have flat color and lighting, hardware depth and no spherical caps. All objects use the scene-wide pixel grid. Growth and wind stay live.",
        initially_open: false,
        params: &["grass_stem_rendering", "cpu_stem_band_rendering", "stem_band_mode", "grass_band_pose_reuse"],
    },
    ControlGroup {
        parent: None,
        title: "Flower Stem Geometry & Color",
        description: "",
        initially_open: false,
        params: &[
            "flower_stem_cell_height_voxels",
            "flower_stem_radius_scale",
            "flower_stem_tip_radius_ratio",
            "flower_stem_test_branches",
        ],
    },
    ControlGroup {
        parent: None,
        title: "Growth & Fruiting",
        description: "Plant growth, tree age and the independent fruiting cycle.",
        initially_open: false,
        params: &[
            "flora_growth_override_enabled",
            "flora_growth_override",
            "tree_age",
            "fruit_cycle",
        ],
    },
    ControlGroup {
        parent: None,
        title: "Pixel Models — Global",
        description: "Pixel-model cache settings for all affected objects. Dynamic models share one view count; flower heads have a separate static count. More views or pixels use more GPU cache memory.",
        initially_open: false,
        params: &["model_pixel_view_count", "apple_pixel_resolution"],
    },
    ControlGroup {
        parent: Some("Wind"),
        title: "Response & Motion",
        description: "Wind animation and response for trees and vegetation. Pose rate is separate from the world tick.",
        initially_open: true,
        params: &[
            "raster_tree_wind",
            "tree_stiffness",
            "flora_inertial_response",
            "vegetation_response_speed",
            "vegetation_response_damping",
            "vegetation_response_gain",
            "vegetation_response_pose_hz",
        ],
    },
    ControlGroup {
        parent: None,
        title: "Visibility & Detail",
        description: "Draw distance, level of detail and which grass species are visible.",
        initially_open: false,
        params: &["lod_distance", "flora_draw_distance", "grass_render_mode"],
    },
    ControlGroup {
        parent: None,
        title: "Lighting Diagnostics",
        description: "Flora lighting and terrain path-tracing reference. Terrain automatically uses geometry-aware hybrid lighting in the normal consumer path; reference/debug transport is independent.",
        initially_open: false,
        params: &[
            "path_tracing_reference",
            "path_tracing_max_bounces",
        ],
    },
    ControlGroup {
        parent: None,
        title: "World Timing",
        description: "The shared world update interval, not the vegetation pose rate.",
        initially_open: false,
        params: &["world_tick_seconds"],
    },
];

// Presentation-only ownership. Keep IDs and stored sections unchanged so old
// saves and generated settings continue to work. Render each control once.
const PIXEL_MODEL_CONTROLS: &[(&str, &str)] = &[
    ("Debug", "model_pixel_view_count"),
    ("Flora", "model_flower_view_count"),
    ("Debug", "apple_pixel_resolution"),
    ("Butterflies", "butterfly_pixel_resolution"),
    ("Falling Leaves", "falling_leaf_pixel_resolution"),
    ("Flora", "model_flower_pixel_resolution"),
];

/// Accept legacy saves without exposing controls for deleted rendering paths.
/// The experiment has one scene-wide pixel grid, not per-model settings.
pub(super) fn retired_pixel_control(id: &str) -> bool {
    matches!(
        id,
        "grass_band_pixelization"
            | "flower_stem_model_resolution"
            | "flower_stem_cell_height_voxels"
            | "model_pixel_view_count"
            | "model_flower_view_count"
            | "apple_pixel_resolution"
            | "butterfly_pixel_resolution"
            | "falling_leaf_pixel_resolution"
            | "model_flower_pixel_resolution"
    )
}

pub(super) fn is_pixel_model_control(section: &str, id: &str) -> bool {
    PIXEL_MODEL_CONTROLS.contains(&(section, id))
}

pub(super) fn search_path(section: &str, id: &str) -> Option<String> {
    if is_pixel_model_control(section, id) {
        let category = if PIXEL_MODEL_CONTROLS[..2].contains(&(section, id)) {
            "Direction Views"
        } else {
            "Pixels per Model"
        };
        return Some(format!("Pixel Models — Global / {category}"));
    }
    if section != "Debug" {
        return None;
    }
    GROUPS
        .iter()
        .find(|group| group.params.contains(&id))
        .map(|group| match group.parent {
            Some("Wind") => format!("Wind / Response / Shared Mechanics / {}", group.title),
            Some(parent) => format!("{parent} / {}", group.title),
            None => group.title.to_owned(),
        })
}

fn is_grouped(id: &str) -> bool {
    GROUPS.iter().any(|group| group.params.contains(&id))
}

pub(super) fn render(
    ui: &mut egui::Ui,
    section: &GuiSection,
    config: &[GuiSection],
    adjustables: &mut GuiAdjustables,
    parent: Option<&str>,
) {
    for group in GROUPS.iter().filter(|group| {
        group.parent == parent && !group.params.iter().all(|id| retired_pixel_control(id))
    }) {
        if parent == Some("Wind") {
            ui_text::section(ui, group.title);
            for id in group.params {
                if let Some(param) = section.param.iter().find(|p| p.id == *id) {
                    render_gui_param_from_config(ui, param, &section.name, adjustables);
                }
            }
            continue;
        }
        egui::CollapsingHeader::new(group.title)
            .id_salt(("debug_controls", group.title))
            .default_open(group.initially_open)
            .show(ui, |ui| {
                if group.title == "Flower Stem Geometry & Color" {
                    for (title, ids) in [
                        (
                            "Geometry",
                            &[
                                "model_flower_voxel_scale",
                                "flower_stem_radius_scale",
                                "flower_stem_tip_radius_ratio",
                                "flower_stem_test_branches",
                            ][..],
                        ),
                        (
                            "Shading",
                            &[
                                "flower_stem_cell_height_voxels",
                                "model_flower_stem_bottom_color",
                                "model_flower_stem_tip_color",
                            ][..],
                        ),
                    ] {
                        ui_text::section(ui, title);
                        for id in ids {
                            for owner in config {
                                if let Some(param) = owner.param.iter().find(|p| p.id == *id) {
                                    render_gui_param_from_config(
                                        ui,
                                        param,
                                        &owner.name,
                                        adjustables,
                                    );
                                }
                            }
                        }
                    }
                    return;
                }
                ui_text::hint(ui, group.description);
                ui.add_space(4.0);
                if group.title == "Pixel Models — Global" {
                    for (title, controls) in [
                        ("Direction Views", &PIXEL_MODEL_CONTROLS[..2]),
                        ("Pixels per Model", &PIXEL_MODEL_CONTROLS[2..6]),
                    ] {
                        ui_text::section(ui, title);
                        for &(section_name, id) in controls {
                            if let Some(owner) = config.iter().find(|s| s.name == section_name) {
                                if let Some(param) = owner.param.iter().find(|p| p.id == id) {
                                    render_gui_param_from_config(
                                        ui,
                                        param,
                                        &owner.name,
                                        adjustables,
                                    );
                                }
                            }
                        }
                    }
                } else {
                    for id in group.params {
                        if let Some(param) = section.param.iter().find(|param| param.id == *id) {
                            render_gui_param_from_config(ui, param, &section.name, adjustables);
                        }
                    }
                }
            });
    }
    // New/unrecognized settings must never silently disappear. The coverage test below requires
    // intentional classification of all settings shipped in our config.
    if parent.is_none() && section.param.iter().any(|param| !is_grouped(&param.id)) {
        ui.collapsing("Other Diagnostics", |ui| {
            for param in &section.param {
                if !is_grouped(&param.id) {
                    render_gui_param_from_config(ui, param, &section.name, adjustables);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::gui_config::{DebugSettings, GuiConfigLoader};
    use std::collections::BTreeSet;

    #[test]
    fn stem_controls_keep_tuning_without_retired_switches_or_hints() {
        let group = GROUPS
            .iter()
            .find(|g| g.title == "Flower Stem Geometry & Color")
            .unwrap();
        for resolution in [32, 192, 512] {
            let mut settings = DebugSettings::load();
            settings.adjustables.flower_stem_model_resolution.value = resolution;
            settings.adjustables.model_flower_voxel_scale.value = 1.8;
            settings.sync_config();
            let before = serde_json::to_value(&settings.config).unwrap();
            let context = egui::Context::default();
            context.memory_mut(|memory| memory.set_everything_is_visible(true));
            let output = context.run_ui(egui::RawInput::default(), |ui| {
                settings.draw(ui, |_, _| {});
            });
            let text = format!("{:?}", output.shapes);
            assert!(!text.contains("Continuous stems"));
            assert!(!text.contains("model-space pixelization"));
            assert!(!text.contains("Flower stems: surface-attached cell shading"));
            assert!(!text.contains("Off: continuous shading"));
            assert!(!text.contains("Model-sized cells with continuous perspective views"));
            assert!(text.contains("Geometry"));
            assert!(text.contains("Shading"));
            assert!(!text.contains("Stem Model Pixelization"));
            for id in group.params {
                let label = &settings
                    .config
                    .section
                    .iter()
                    .flat_map(|s| &s.param)
                    .find(|p| p.id == *id)
                    .unwrap()
                    .label;
                assert_eq!(
                    text.contains(label),
                    !retired_pixel_control(id),
                    "resolution={resolution} id={id}"
                );
            }
            let voxel_label = &settings
                .config
                .section
                .iter()
                .flat_map(|s| &s.param)
                .find(|p| p.id == "model_flower_voxel_scale")
                .unwrap()
                .label;
            assert_eq!(
                text.matches(voxel_label).count(),
                1,
                "Stem dimensions must have one owner"
            );
            assert!(!text.contains("world-direction"));
            assert!(!text.contains("direction cells per cube face"));
            assert!(!text.contains("block geometry B"));
            assert!(!text.contains("geometry cell size"));
            settings.sync_config();
            assert_eq!(serde_json::to_value(&settings.config).unwrap(), before);
        }
    }

    #[test]
    fn every_debug_parameter_has_exactly_one_group() {
        // Review the full Debug section by adjustment concern, not by object.
        assert!(GROUPS
            .iter()
            .all(|group| !matches!(group.title, "Apple Appearance" | "Tree Rendering")));
        let wind = GROUPS
            .iter()
            .find(|g| g.title == "Response & Motion")
            .unwrap();
        assert!(wind.params.contains(&"raster_tree_wind"));
        assert!(wind.params.contains(&"flora_inertial_response"));
        let config = GuiConfigLoader::load();
        let debug = config
            .section
            .iter()
            .find(|section| section.name == "Debug")
            .unwrap();
        let assignments = GROUPS
            .iter()
            .flat_map(|group| group.params.iter().copied())
            .collect::<Vec<_>>();
        let unique = assignments.iter().copied().collect::<BTreeSet<_>>();
        assert_eq!(
            unique.len(),
            assignments.len(),
            "duplicate control group membership"
        );
        assert_eq!(
            unique,
            debug
                .param
                .iter()
                .map(|param| param.id.as_str())
                .collect::<BTreeSet<_>>()
        );
        assert!(!is_grouped("future_debug_control"));
    }

    #[test]
    fn pixel_model_global_collects_sampling_for_every_object() {
        let global = GROUPS
            .iter()
            .find(|g| g.title == "Pixel Models — Global")
            .unwrap();
        assert_eq!(global.parent, None);
        assert_eq!(
            global.params,
            &["model_pixel_view_count", "apple_pixel_resolution"]
        );
        assert_eq!(PIXEL_MODEL_CONTROLS.len(), 6);
        let config: crate::app::gui_config_model::GuiConfigFile =
            toml::from_str(include_str!("../../../config/gui.toml")).unwrap();
        for &(section, id) in PIXEL_MODEL_CONTROLS {
            let owner = config.section.iter().find(|s| s.name == section).unwrap();
            assert_eq!(
                owner.param.iter().filter(|p| p.id == id).count(),
                1,
                "missing or duplicated pixel control: {section}/{id}"
            );
            assert!(is_pixel_model_control(section, id));
        }
        assert!(!is_pixel_model_control("Wind", "tree_stiffness"));
        let model_controls = config
            .section
            .iter()
            .flat_map(|s| {
                s.param
                    .iter()
                    .map(move |p| (s.name.as_str(), p.id.as_str()))
            })
            .filter(|(_, id)| {
                id.ends_with("pixel_resolution")
                    || matches!(*id, "model_pixel_view_count" | "model_flower_view_count")
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(
            model_controls,
            PIXEL_MODEL_CONTROLS.iter().copied().collect(),
            "new pixel model sampling controls need a Pixel Models owner"
        );
    }

    #[test]
    fn drawing_expanded_groups_preserves_all_saved_values() {
        let mut settings = DebugSettings::from_config(GuiConfigLoader::load());
        settings.adjustables.vegetation_response_speed.value = 2.7;
        settings.adjustables.tree_age.value = 0.37;
        settings.sync_config();
        let before = serde_json::to_value(&settings.config).unwrap();
        let context = egui::Context::default();
        context.memory_mut(|memory| memory.set_everything_is_visible(true));
        let mut sections = Vec::new();
        let output = context.run_ui(egui::RawInput::default(), |ui| {
            settings.draw(ui, |section, _| sections.push(section.to_owned()));
        });
        assert!(!output.shapes.is_empty());
        fn collect_text(shape: &egui::Shape, text: &mut String) {
            match shape {
                egui::Shape::Text(shape) => {
                    text.push_str(&shape.galley.job.text);
                    text.push('\n');
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect_text(shape, text);
                    }
                }
                _ => {}
            }
        }
        let mut text = String::new();
        for shape in &output.shapes {
            collect_text(&shape.shape, &mut text);
        }
        for group in GROUPS {
            assert_eq!(
                text.contains(group.title),
                !group.params.iter().all(|id| retired_pixel_control(id)),
                "group {}",
                group.title
            );
        }
        for category in [
            "Rendering & Lighting",
            "World & Simulation",
            "Plants & Wildlife",
            "Camera & Audio",
            "Other Settings",
        ] {
            assert!(!text.contains(category), "unnecessary category: {category}");
        }
        for &(section, id) in PIXEL_MODEL_CONTROLS {
            let label = &settings
                .config
                .section
                .iter()
                .find(|s| s.name == section)
                .unwrap()
                .param
                .iter()
                .find(|p| p.id == id)
                .unwrap()
                .label;
            assert_eq!(
                text.lines().filter(|line| *line == label).count(),
                0,
                "retired {section}/{id} must not appear in the global scene experiment"
            );
        }
        assert!(!text
            .lines()
            .any(|line| matches!(line, "Post Processing" | "Post-processing")));
        assert!(!text.contains("Dither Strength"));
        assert!(!text.contains("DDGI Experiments"));
        assert!(!text.contains("DDGI continuous accepted-batch sampling"));
        assert!(!text.contains("DDGI geometry-qualified aggregate history"));
        let expected_sections = settings
            .config
            .section
            .iter()
            .filter(|s| s.name != "Debug" && s.name != "Post Processing")
            .map(|s| s.name.clone())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            sections.iter().cloned().collect::<BTreeSet<_>>(),
            expected_sections
        );
        assert_eq!(
            sections.len(),
            expected_sections.len(),
            "a section was drawn twice"
        );
        assert_eq!(
            GROUPS
                .iter()
                .find(|g| g.title == "Response & Motion")
                .unwrap()
                .parent,
            Some("Wind")
        );
        assert_eq!(
            GROUPS
                .iter()
                .find(|g| g.title == "Growth & Fruiting")
                .unwrap()
                .parent,
            None
        );
        assert!(!text.contains("Reset Inertia"));
        assert!(!text.contains("Original C Rhythm"));
        for title in [
            "Atmos",
            "Terrain",
            "Camera",
            "Tree",
            "GodRay",
            "Starlight",
            "Planting",
            "Distribution",
            "Spawn Animation",
            "Ground Plants",
            "Generation",
            "Response",
            "Grass Amplitude Response",
            "Grass Frequency Response",
            "Grass Colors",
            "Leaves",
            "Appearance & Lighting",
            "Leaf Amplitude Response",
            "Leaf Frequency Response",
        ] {
            assert!(
                text.lines().any(|line| line == title),
                "missing section {title}"
            );
        }
        for title in ["Debug", "Sky", "Voxel", "HeadBob", "Color Variation"] {
            assert!(
                !text.lines().any(|line| line == title),
                "obsolete section {title}"
            );
        }
        settings.sync_config();
        assert_eq!(before, serde_json::to_value(&settings.config).unwrap());
    }
}
