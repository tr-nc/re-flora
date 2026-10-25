//! Presentation only: parameter values, ranges, conditions and saving stay config-owned.
use super::{render_gui_param_from_config, GuiAdjustables};
use crate::app::gui_config_model::GuiSection;
use crate::app::ui_text;

pub(super) struct ControlGroup {
    parent: Option<&'static str>,
    pub(super) title: &'static str,
    description: &'static str,
    initially_open: bool,
    params: &'static [&'static str],
}

const GROUPS: &[ControlGroup] = &[
    ControlGroup {
        parent: None,
        title: "GUI Appearance",
        description: "Show panel outlines. Backgrounds, shadows, input controls and selection indicators are unchanged. The text toolbar is always borderless. Save remembers this preference.",
        initially_open: false,
        params: &["gui_panel_outlines"],
    },
    ControlGroup {
        parent: None,
        title: "Moisture Appearance",
        description: "Grass darkens by up to 25% in linear material color as supporting soil becomes moist. This is root/soil moisture, not water measured on each blade. Manual watering, transfer, drying and restored soil all use the same atlas. Unchecked preserves original grass color; growth and other plants are unchanged. Saved with Save.",
        initially_open: false,
        params: &["grass_moisture_response"],
    },
    ControlGroup {
        parent: None,
        title: "Walking Traversal",
        description: "Cross low, narrow ground detail above a nearby supported patch. Height is measured from real surrounding terrain, not accumulated from previous steps. Default 3 voxels; capped at the player's 4-voxel radius. Normal 12-voxel steps, body clearance, jumping and mower movement are unchanged. Changes apply immediately; saved with Save.",
        initially_open: false,
        params: &["walk_ground_detail_height"],
    },
    ControlGroup {
        parent: None,
        title: "Camera View",
        description: "Limit the orbit/edit camera's downward viewing angle: 0 is horizontal, 90 is directly overhead. Live changes retain the focus and distance. Walking and free-flight look angles are unchanged. Saved with the unified Save button.",
        initially_open: false,
        params: &["camera_orbit_max_elevation"],
    },
    ControlGroup {
        parent: None,
        title: "Pavement & Walls",
        description: "Pave (V) builds paving; Wall (J) builds low walls. Left-drag to draw. Tab changes the selected surface/material; Shift + wheel adjusts next-stroke width/thickness. Ctrl + wheel adjusts wall height independently. Earth walls are continuous tapered soil; stone and brick walls have real staggered joints. Path spacing is a paving course or wall half-brick, clamped to width. Each gesture freezes its recipe and ground reference. Backdrag and optional inward smoothing apply to slabs/brick paving only, not walls or dirt. Backdrag restores this held paving stroke's voxels and flora; release/cancel retains published terrain. Red targets remain replaceable previews. Existing masonry, unsupported ground and world boundaries stay protected. Terrain Save/Load saves voxels and flora; GUI Save saves preferences. Dig edits everything. No editable path objects, automatic closed joins, inside fill or whole-wall Undo.",
        initially_open: false,
        params: &["pavement_kind", "low_wall_kind", "pavement_width", "low_wall_width", "low_wall_height", "pavement_piece_length", "pavement_seed", "pavement_retract_stroke", "pavement_retract_scale", "pavement_smooth_curves", "pavement_smooth_scale"],
    },
    ControlGroup {
        parent: None,
        title: "Stone",
        description: "",
        initially_open: false,
        params: &[],
    },
    ControlGroup {
        parent: Some("Stone"),
        title: "Geometry",
        description: "Deterministic closed triangle models, not voxel source data. Slabs have broad flat tops and bottoms with corner cuts and narrow chamfers; rocks use bounded asymmetric cutting planes. Slab thickness is limited to 30% of the short edge. All dimensions use garden world units (256 terrain voxels per unit). These are visual candidates, not approved art.",
        initially_open: false,
        params: &["stone_place_slab", "stone_placement_scale", "stone_seed", "stone_width", "stone_depth", "stone_slab_thickness", "stone_rock_height", "stone_variation", "stone_slab_edge_cut", "stone_rock_facets"],
    },
    ControlGroup {
        parent: Some("Stone"),
        title: "Rendering",
        description: "Yaw for the next placed rock or slab, around the contacted terrain normal. Placed stones use the garden material palette and scene pixel processing; they are saved and edited as terrain voxels.",
        initially_open: false,
        params: &["stone_yaw"],
    },
    ControlGroup {
        parent: None,
        title: "Scene Pixel Sampling",
        description: "Unchecked: original direct low-resolution rendering. Checked: render a denser source and select a pixel color resolve. HDR average preserves the previous reference; Contrast-aware uses perceptual luminance statistics to select one complete source color after tone mapping; it never splices color channels. No palette reduction or outline expansion. Compare at the same source density and final pixel ratio. Contrast may change colors or flicker; not visually approved yet. Pixel size is an integer 1-8: N groups NxN physical screen pixels, giving N-squared:1 (1=1:1, 2=4:1, 3=9:1, 5=25:1). Default size is 5. Edge blocks are cropped, UI stays native. Source density is capped at native-equivalent resolution: 1:1 bypasses the candidate, 4:1 caps at four source pixels. Preferences are retained and saved.",
        initially_open: false,
        params: &["scene_pixel_ratio", "scene_supersampling_enabled", "scene_supersampling_quality", "scene_pixel_resolve_mode"],
    },
    ControlGroup {
        parent: None,
        title: "Stem Model Pixelization",
        description: "Grass square color bands use hardware rasterization and prepared poses on the scene-wide pixel grid. Flower sampling and CPU climbing stems are unchanged.",
        initially_open: false,
        params: &["grass_band_pixelization", "flower_stem_model_resolution"],
    },
    ControlGroup {
        parent: None,
        title: "Stem Geometry & Color Bands",
        description: "Grass unchecked: original voxel geometry. Checked: square color-band stems with flat color and lighting, hardware depth and no spherical caps. All objects use the scene-wide pixel grid. Growth and wind stay live.",
        initially_open: false,
        params: &["cpu_stem_band_rendering", "grass_band_pose_reuse"],
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
            "apple_size_scale",
        ],
    },
    ControlGroup {
        parent: None,
        title: "Apple Appearance & Lighting",
        description: "Apples use smooth directional sunlight with live surface normals and shadows. Environment fill adjusts indirect light, not sun visibility; 1 is unchanged. Plain red peel has no color patches or stripes. Sun highlights follow the light and camera. Works on attached/fallen apples with native triangles and pre-cache; size and physics are unchanged.",
        initially_open: true,
        params: &[
            "apple_environment_fill",
        ],
    },
    ControlGroup {
        parent: None,
        title: "Hardware Appearance",
        description: "Sprinklers: placement snaps the anchor to a voxel center on X/Y/Z. Overall size scales the complete hardware around that anchor, including the spray outlet. It does not scale launch speed, spray angle, RPM, sound or watering strength; changing nozzle height can naturally shift ground coverage. Choose a brass impact, garden pulse or enclosed rotor head. Size applies live to native and pre-cached models without rebaking. Saved with Save.",
        initially_open: false,
        params: &["sprinkler_size_scale", "sprinkler_design"],
    },
    ControlGroup {
        parent: None,
        title: "Sprinkler Spray & Motion",
        description: "Type applies to NEW placements: compact and tall impact sprinklers can coexist. Tall has a raised head, stronger upward launch and a wider falling arc. Plants never block water; terrain does. One rotating nozzle. The whole assembly and spray launches use the same 32 internal poses (11.25 degrees apart); existing droplets keep their birth direction. Camera directions follow the global Model direction count. Spray angle changes the shared air/soil distribution; RPM changes rotation, not flight speed. Six fixed spray bursts follow the recording.",
        initially_open: false,
        params: &["sprinkler_placement_type", "sprinkler_spray_angle", "sprinkler_rotation_speed"],
    },
    ControlGroup {
        parent: None,
        title: "Soil Wetting",
        description: "Drying speed affects all soil: 1 is the original evaporation rate; zero pauses evaporation, not watering or vertical moisture transfer. Sprinklers use coherent wet patches: lower scale is finer-grained, higher scale is broader. Scale changes coherence, not spray reach or average water input. Wetting speed scales sprinkler irrigation; zero stops watering, not mist. Four soil moisture levels and manual Water brush are unchanged.",
        initially_open: false,
        params: &["soil_drying_speed", "sprinkler_wet_patch_size", "sprinkler_wetting_speed"],
    },
    ControlGroup {
        parent: None,
        title: "Mist & Scattering",
        description: "Mist supplies the spray body; optional small droplets follow the same nozzle, launch law, wind, valve and terrain. Intensity changes mist only (2 is twice the former default); zero leaves enabled droplets visible. Droplets are optical only and never add irrigation. Mist uses the original analytic rendering. Small droplets are screen-facing quads.",
        initially_open: false,
        params: &["sprinkler_mist_density", "sprinkler_mist_particles"],
    },
    ControlGroup {
        parent: None,
        title: "Model View Quantization",
        description: "Native models always use the nearest model-local direction from one finite sphere bank. The direction count applies to flower heads, attached and fallen apples, mesh butterflies, sprinklers. Positions, growth, wind, articulated poses and roll stay live; player camera, simulation, collisions, world voxels and the scene pixel grid are unchanged. Compare 128 and 256, or 8-512 custom directions.",
        initially_open: false,
        params: &["model_pixel_view_count"],
    },
    ControlGroup {
        parent: None,
        title: "Model Pixel Pre-cache",
        description: "Pre-bake finite-view N x N surfaces for flower heads, butterflies, apples and sprinklers. Each object has an independent saved switch and resolution (8-32). Unchecked uses native triangles; both modes share Model direction count. Poses, roll, lighting, growth and flight remain live. Resolution changes rebuild immutable banks asynchronously; native models remain visible until ready. Leaves stay ordinary voxel particles. UI is not pixelized.",
        initially_open: false,
        params: &["model_flower_cache_enabled", "model_flower_pixel_resolution", "butterfly_cache_enabled", "butterfly_pixel_resolution", "apple_cache_enabled", "apple_pixel_resolution", "sprinkler_cache_enabled", "sprinkler_pixel_resolution"],
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
        title: "Tree Kick",
        description: "Walk near tree wood, aim and press K. Bending travels along the actual trunk/branch paths; canopy arrival triggers decorative leaves, apple roulette and finite leaf rustle. No wood damage or collider motion. Lower wave speed slows both motion and effects; lower sound body reduces muffled low/mid texture. Sound works without wind. These controls are saved with Debug Panel Save.",
        initially_open: false,
        params: &["tree_kick_strength", "tree_kick_frequency", "tree_kick_damping", "tree_kick_reach", "tree_kick_cooldown", "tree_kick_propagation_speed", "tree_kick_fruit_chance_scale", "tree_kick_leaf_count", "tree_kick_rustle_volume_db", "tree_kick_rustle_duration", "tree_kick_rustle_body", "tree_kick_rustle_brightness", "tree_kick_rustle_crackle"],
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
    ("Debug", "apple_cache_enabled"),
    ("Debug", "butterfly_cache_enabled"),
    ("Debug", "model_flower_cache_enabled"),
    ("Debug", "sprinkler_cache_enabled"),
    ("Debug", "sprinkler_pixel_resolution"),
    ("Debug", "apple_pixel_resolution"),
    ("Butterflies", "butterfly_pixel_resolution"),
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
            | "model_flower_view_count"
            | "falling_leaf_pixel_resolution"
    )
}

pub(super) fn is_pixel_model_control(section: &str, id: &str) -> bool {
    PIXEL_MODEL_CONTROLS.contains(&(section, id))
}

pub(super) fn search_path(section: &str, id: &str) -> Option<String> {
    if is_pixel_model_control(section, id) {
        return Some("Model Pixel Pre-cache".to_owned());
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

pub(super) fn groups(parent: Option<&str>) -> Vec<&'static ControlGroup> {
    let mut groups: Vec<_> = GROUPS
        .iter()
        .filter(|group| {
            group.parent == parent
                && (group.params.is_empty()
                    || !group.params.iter().all(|id| retired_pixel_control(id)))
        })
        .collect();
    groups.sort_by_key(|group| group.title.to_ascii_lowercase());
    groups
}

pub(super) fn render(
    ui: &mut egui::Ui,
    section: &GuiSection,
    config: &[GuiSection],
    adjustables: &mut GuiAdjustables,
    parent: Option<&str>,
) {
    for group in groups(parent) {
        render_group(ui, section, config, adjustables, group);
    }
    if parent.is_none() && has_ungrouped(section) {
        render_ungrouped(ui, section, adjustables);
    }
}

pub(super) fn render_group(
    ui: &mut egui::Ui,
    section: &GuiSection,
    config: &[GuiSection],
    adjustables: &mut GuiAdjustables,
    group: &ControlGroup,
) {
    if matches!(group.parent, Some("Wind" | "Stone")) {
        ui_text::section(ui, group.title);
        if group.parent == Some("Stone") {
            ui_text::hint(ui, group.description);
        }
        for id in group.params {
            if let Some(param) = section.param.iter().find(|p| p.id == *id) {
                render_gui_param_from_config(ui, param, &section.name, adjustables);
            }
        }
        return;
    }
    egui::CollapsingHeader::new(group.title)
        .id_salt(("debug_controls", group.title))
        .default_open(group.initially_open)
        .show(ui, |ui| {
            if group.params.is_empty() {
                render(ui, section, config, adjustables, Some(group.title));
                return;
            }
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
                                render_gui_param_from_config(ui, param, &owner.name, adjustables);
                            }
                        }
                    }
                }
                return;
            }
            ui_text::hint(ui, group.description);
            ui.add_space(4.0);
            for id in group.params {
                for owner in config {
                    if let Some(param) = owner.param.iter().find(|param| param.id == *id) {
                        render_gui_param_from_config(ui, param, &owner.name, adjustables);
                    }
                }
            }
        });
}

// Future controls must stay visible, with the fallback participating in root ordering too.
pub(super) fn has_ungrouped(section: &GuiSection) -> bool {
    section.param.iter().any(|param| !is_grouped(&param.id))
}

pub(super) fn render_ungrouped(
    ui: &mut egui::Ui,
    section: &GuiSection,
    adjustables: &mut GuiAdjustables,
) {
    ui.collapsing("Other Diagnostics", |ui| {
        for param in &section.param {
            if !is_grouped(&param.id) {
                render_gui_param_from_config(ui, param, &section.name, adjustables);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::gui_config::{DebugSettings, GuiConfigLoader};
    use std::collections::BTreeSet;

    #[test]
    fn stone_controls_share_one_parent_with_two_regions() {
        let stone = GROUPS.iter().find(|group| group.title == "Stone").unwrap();
        assert_eq!(stone.parent, None);
        assert!(stone.params.is_empty());
        let regions = GROUPS
            .iter()
            .filter(|group| group.parent == Some("Stone"))
            .collect::<Vec<_>>();
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].title, "Geometry");
        assert_eq!(regions[1].title, "Rendering");
        for region in regions {
            for id in region.params {
                assert_eq!(
                    search_path("Debug", id),
                    Some(format!("Stone / {}", region.title))
                );
            }
        }
    }

    #[test]
    fn supersampling_has_one_searchable_adjustment_first_owner() {
        for id in [
            "scene_pixel_ratio",
            "scene_supersampling_enabled",
            "scene_supersampling_quality",
            "scene_pixel_resolve_mode",
        ] {
            assert_eq!(
                search_path("Debug", id).as_deref(),
                Some("Scene Pixel Sampling")
            );
            assert_eq!(
                GROUPS
                    .iter()
                    .filter(|group| group.params.contains(&id))
                    .count(),
                1
            );
            assert!(!retired_pixel_control(id));
        }
    }

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
                .chain(PIXEL_MODEL_CONTROLS.iter().map(|(_, id)| *id))
                .collect::<BTreeSet<_>>()
        );
        assert!(!is_grouped("future_debug_control"));
    }

    #[test]
    fn model_view_controls_have_one_live_adjustment_first_owner() {
        let group = GROUPS
            .iter()
            .find(|g| g.title == "Model View Quantization")
            .unwrap();
        assert_eq!(group.params, &["model_pixel_view_count"]);
        for id in group.params {
            assert_eq!(
                search_path("Debug", id).as_deref(),
                Some("Model View Quantization")
            );
            assert_eq!(GROUPS.iter().filter(|g| g.params.contains(id)).count(), 1);
            assert!(!retired_pixel_control(id));
        }
        assert_eq!(group.parent, None);
    }

    #[test]
    fn cached_model_controls_have_one_live_owner() {
        let global = GROUPS
            .iter()
            .find(|g| g.title == "Model Pixel Pre-cache")
            .unwrap();
        assert_eq!(global.parent, None);
        assert_eq!(global.params.len(), 8);
        assert_eq!(PIXEL_MODEL_CONTROLS.len(), 8);
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
            .filter(|(_, id)| id.ends_with("pixel_resolution"))
            .collect::<BTreeSet<_>>();
        assert_eq!(
            model_controls,
            PIXEL_MODEL_CONTROLS
                .iter()
                .copied()
                .filter(|(_, id)| id.ends_with("pixel_resolution"))
                .collect(),
            "retired model pixel resolutions must not gain new live owners"
        );
    }

    #[test]
    fn root_sections_follow_display_name_alphabetical_order() {
        let mut settings = DebugSettings::load();
        // Presentation must not depend on the saved declaration order.
        settings.config.section.reverse();
        let context = egui::Context::default();
        let output = context.run_ui(egui::RawInput::default(), |ui| {
            settings.draw(ui, |_, _| {});
        });
        fn collect(shape: &egui::Shape, titles: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(shape) => {
                    let text = &shape.galley.job.text;
                    if [
                        "Apple Appearance & Lighting",
                        "Atmos",
                        "Audio",
                        "Camera",
                        "Camera View",
                        "Material Supply",
                        "Soil Deposition",
                        "Stone",
                        "Terrain",
                        "World Timing",
                    ]
                    .contains(&text.as_str())
                    {
                        titles.push(text.clone());
                    }
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, titles);
                    }
                }
                _ => {}
            }
        }
        let mut titles = Vec::new();
        for shape in &output.shapes {
            collect(&shape.shape, &mut titles);
        }
        assert_eq!(titles.len(), 10);
        let mut sorted = titles.clone();
        sorted.sort_by_key(|title| title.to_ascii_lowercase());
        assert_eq!(
            titles, sorted,
            "all root group kinds must share one display-name order"
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
        assert_eq!(text.lines().filter(|line| *line == "Stone").count(), 1);
        assert!(!text.contains("Stone Geometry"));
        assert!(!text.contains("Stone Rendering"));
        for region in GROUPS.iter().filter(|group| group.parent == Some("Stone")) {
            for id in region.params {
                let label = &settings
                    .config
                    .section
                    .iter()
                    .flat_map(|section| &section.param)
                    .find(|param| param.id == *id)
                    .unwrap()
                    .label;
                assert_eq!(text.lines().filter(|line| *line == label).count(), 1);
            }
        }
        for group in GROUPS {
            assert_eq!(
                text.contains(group.title),
                group.params.is_empty() || !group.params.iter().all(|id| retired_pixel_control(id)),
                "group {}",
                group.title
            );
        }
        assert_eq!(
            text.lines()
                .filter(|line| *line == "Scene: higher-resolution pixelization (A/B)")
                .count(),
            1,
            "the saved sampling checkbox must be drawn exactly once",
        );
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
                1,
                "{section}/{id} must appear exactly once in the pre-cache group"
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
