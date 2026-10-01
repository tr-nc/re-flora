//! Presentation only: parameter values, ranges, conditions and saving stay config-owned.
use super::{render_gui_param_from_config, GuiAdjustables};
use crate::app::gui_config_model::GuiSection;

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
        title: "Pixel Sampling — Flower Stems",
        description: "Select a stem renderer; only its active controls are shown.",
        initially_open: true,
        params: &[
            "flower_stem_sampling",
            "flower_stem_object_sampling",
            "flower_stem_direction_resolution",
            "flower_stem_object_resolution",
            "flower_stem_surface_geometry",
            "flower_stem_geometry_cell_scale",
            "flower_stem_radius_scale",
            "flower_stem_test_branches",
            "flower_stem_freeze_motion",
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
        description: "Pixel-model cache settings for all affected objects, plus the scene-wide post-processing dither. Dynamic models share one view count; flower heads have a separate static count. More views or pixels use more GPU cache memory.",
        initially_open: true,
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
            "raster_flora_ddgi_lighting",
            "path_tracing_reference",
            "path_tracing_ambient_light",
            "path_tracing_max_bounces",
        ],
    },
    ControlGroup {
        parent: None,
        title: "DDGI Experiments",
        description: "Optional A/B candidates, off by default. Off = original policy; changes apply next field. Tested at 32-voxel spacing; 64-voxel wall spikes regressed. Not required for the digging performance fix.",
        initially_open: false,
        params: &["ddgi_continuous_sampling", "ddgi_aggregate_history"],
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
    ("Post Processing", "dither_strength_lsb"),
];

pub(super) fn is_pixel_model_control(section: &str, id: &str) -> bool {
    PIXEL_MODEL_CONTROLS.contains(&(section, id))
}

fn is_grouped(id: &str) -> bool {
    GROUPS.iter().any(|group| group.params.contains(&id))
}

fn stem_control_visible(id: &str, mode: u32, object_b: bool, geometry_b: bool) -> bool {
    match id {
        "flower_stem_sampling" => true,
        "flower_stem_object_sampling" => mode == 1,
        "flower_stem_direction_resolution" => mode == 1 && !object_b,
        "flower_stem_object_resolution" => mode == 1 && object_b,
        "flower_stem_surface_geometry" => mode == 2,
        "flower_stem_geometry_cell_scale" => mode == 2 && geometry_b,
        _ => mode != 0,
    }
}

fn stem_description(mode: u32, object_b: bool, geometry_b: bool) -> &'static str {
    match mode {
        1 if object_b => "World-direction B: fixed object pixels; distance changes display size, not source resolution.",
        1 => "World-direction A: angular source cells. Thin stems can lose samples with distance.",
        2 if geometry_b => "Surface-attached B: real block geometry follows the plant. Larger cells are blockier and can change thickness.",
        2 => "Surface-attached A: continuous geometry with fixed material cells. Enable B to adjust geometric pixelization.",
        _ => "Original cube stems; no experimental parameters apply.",
    }
}

pub(super) fn render(
    ui: &mut egui::Ui,
    section: &GuiSection,
    config: &[GuiSection],
    adjustables: &mut GuiAdjustables,
    parent: Option<&str>,
) {
    for group in GROUPS.iter().filter(|group| group.parent == parent) {
        if parent == Some("Wind") {
            ui.label(group.title);
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
                if group.title == "Pixel Sampling — Flower Stems" {
                    if let Some(param) = section
                        .param
                        .iter()
                        .find(|p| p.id == "flower_stem_sampling")
                    {
                        render_gui_param_from_config(ui, param, &section.name, adjustables);
                    }
                    ui.weak(stem_description(
                        adjustables.flower_stem_sampling.value,
                        adjustables.flower_stem_object_sampling.value,
                        adjustables.flower_stem_surface_geometry.value,
                    ));
                    for id in group
                        .params
                        .iter()
                        .filter(|id| **id != "flower_stem_sampling")
                    {
                        if stem_control_visible(
                            id,
                            adjustables.flower_stem_sampling.value,
                            adjustables.flower_stem_object_sampling.value,
                            adjustables.flower_stem_surface_geometry.value,
                        ) {
                            if let Some(param) = section.param.iter().find(|p| p.id == *id) {
                                render_gui_param_from_config(ui, param, &section.name, adjustables);
                            }
                        }
                    }
                    return;
                }
                ui.weak(group.description);
                ui.add_space(4.0);
                if group.title == "Pixel Models — Global" {
                    for (title, controls) in [
                        ("Direction Views", &PIXEL_MODEL_CONTROLS[..2]),
                        ("Pixels per Model", &PIXEL_MODEL_CONTROLS[2..6]),
                        ("Post-processing", &PIXEL_MODEL_CONTROLS[6..]),
                    ] {
                        ui.label(title);
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
    fn stem_dropdown_renders_only_relevant_controls_without_resetting_hidden_values() {
        let group = GROUPS
            .iter()
            .find(|g| g.title == "Pixel Sampling — Flower Stems")
            .unwrap();
        for (mode, object_b, geometry_b) in [
            (0, true, true),
            (1, false, true),
            (1, true, true),
            (2, true, false),
            (2, true, true),
        ] {
            let mut settings = DebugSettings::load();
            settings.adjustables.flower_stem_sampling.value = mode;
            settings.adjustables.flower_stem_object_sampling.value = object_b;
            settings.adjustables.flower_stem_surface_geometry.value = geometry_b;
            settings.adjustables.flower_stem_object_resolution.value = 192;
            settings.adjustables.flower_stem_geometry_cell_scale.value = 2.5;
            settings.sync_config();
            let before = serde_json::to_value(&settings.config).unwrap();
            let context = egui::Context::default();
            context.memory_mut(|memory| memory.set_everything_is_visible(true));
            let output = context.run_ui(egui::RawInput::default(), |ui| {
                settings.draw(ui, |_, _| {});
            });
            let text = format!("{:?}", output.shapes);
            let mut expected = vec!["flower_stem_sampling"];
            if mode != 0 {
                expected.extend([
                    "flower_stem_radius_scale",
                    "flower_stem_test_branches",
                    "flower_stem_freeze_motion",
                ]);
            }
            match mode {
                1 => {
                    expected.push("flower_stem_object_sampling");
                    expected.push(if object_b {
                        "flower_stem_object_resolution"
                    } else {
                        "flower_stem_direction_resolution"
                    });
                }
                2 => {
                    expected.push("flower_stem_surface_geometry");
                    if geometry_b {
                        expected.push("flower_stem_geometry_cell_scale");
                    }
                }
                _ => {}
            }
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
                    expected.contains(id),
                    "mode={mode} object={object_b} geometry={geometry_b} id={id}"
                );
            }
            settings.sync_config();
            assert_eq!(serde_json::to_value(&settings.config).unwrap(), before);
        }
    }

    #[test]
    fn optional_ddgi_candidates_are_isolated_in_a_collapsed_experiment_group() {
        let group = GROUPS
            .iter()
            .find(|group| group.title == "DDGI Experiments")
            .unwrap();
        assert!(!group.initially_open);
        assert_eq!(
            group.params,
            &["ddgi_continuous_sampling", "ddgi_aggregate_history"]
        );
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
    fn pixel_model_global_collects_post_processing_for_every_object() {
        let global = GROUPS
            .iter()
            .find(|g| g.title == "Pixel Models — Global")
            .unwrap();
        assert_eq!(global.parent, None);
        assert_eq!(
            global.params,
            &["model_pixel_view_count", "apple_pixel_resolution"]
        );
        assert_eq!(PIXEL_MODEL_CONTROLS.len(), 7);
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
                    || matches!(
                        *id,
                        "model_pixel_view_count"
                            | "model_flower_view_count"
                            | "dither_strength_lsb"
                    )
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(
            model_controls,
            PIXEL_MODEL_CONTROLS.iter().copied().collect(),
            "new pixel model post-processing controls need a Pixel Models owner"
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
            assert!(text.contains(group.title), "missing group {}", group.title);
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
                1,
                "{section}/{id} should appear exactly once in Pixel Models — Global"
            );
        }
        assert!(!text.lines().any(|line| line == "Post Processing"));
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
            "Color Variation",
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
        for title in ["Debug", "Sky", "Voxel", "HeadBob"] {
            assert!(
                !text.lines().any(|line| line == title),
                "obsolete section {title}"
            );
        }
        settings.sync_config();
        assert_eq!(before, serde_json::to_value(&settings.config).unwrap());
    }
}
