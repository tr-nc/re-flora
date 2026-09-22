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
        parent: None, title: "Whole Tree Rasterization",
        description: "A: voxel trees. B: rasterized voxel surfaces. Static lighting comparison; branch wind comes next. Static terrain remains the exact secondary-ray and collision representation.",
        initially_open: true, params: &["raster_tree_static", "raster_tree_wind", "raster_tree_hybrid_lighting", "tree_stiffness"],
    },
    ControlGroup {
        parent: Some("Flora"),
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
        parent: Some("Wind"),
        title: "Vegetation Wind Response",
        description: "How plants react to wind. Pose rate is separate from the world tick.",
        initially_open: true,
        params: &[
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
        description: "Thin-voxel terrain A/B, flora lighting and terrain path-tracing reference. Hybrid terrain lighting affects only the normal consumer path, not reference/debug transport.",
        initially_open: false,
        params: &[
            "raster_flora_ddgi_lighting",
            "terrain_hybrid_lighting",
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

fn is_grouped(id: &str) -> bool {
    GROUPS.iter().any(|group| group.params.contains(&id))
}

pub(super) fn render(
    ui: &mut egui::Ui,
    section: &GuiSection,
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
                ui.weak(group.description);
                ui.add_space(4.0);
                for id in group.params {
                    if let Some(param) = section.param.iter().find(|param| param.id == *id) {
                        render_gui_param_from_config(ui, param, &section.name, adjustables);
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
        let expected_sections = settings
            .config
            .section
            .iter()
            .filter(|s| s.name != "Debug")
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
                .find(|g| g.title == "Vegetation Wind Response")
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
            Some("Flora")
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
            "Clouds",
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
            "Purple Allium",
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
