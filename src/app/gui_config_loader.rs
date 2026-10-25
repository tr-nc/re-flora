use crate::app::gui_config_model::{
    GuiConfigFile, GuiParamConditionValue, GuiParamKind, GuiParamValue,
};
use std::{collections::HashMap, collections::HashSet, io::Write, path::Path};

const SUPPORTED_SCHEMA_VERSION: u32 = 1;
const CONFIG_FILE_NAME: &str = "gui.toml";
const RETIRED_FLORA_COLOR_PARAMS: &[&str] = &[
    "flora_instance_hue_offset",
    "flora_instance_saturation_offset",
    "flora_instance_value_offset",
    "flora_voxel_hue_offset",
    "flora_voxel_saturation_offset",
    "flora_voxel_value_offset",
];
// Keep exact voxel-scale controls such as 1 / 256 while still trimming the
// noisy tail emitted when an f32 is serialized through TOML.
const GUI_FLOAT_DECIMALS: usize = 8;

// Schema-v1 compatibility only. These IDs must never reach generated adjustables.
const RETIRED_CLOUD_PARAMS: &[&str] = &[
    "clouds_enabled",
    "cloud_coverage",
    "cloud_density",
    "cloud_bottom_height",
    "cloud_top_height",
    "cloud_shape_scale",
    "cloud_detail_scale",
    "cloud_detail_strength",
    "cloud_wind_speed",
    "cloud_primary_steps",
    "cloud_light_steps",
    "cloud_temporal_alpha",
    "cloud_absorption",
    "cloud_phase_eccentricity",
    "cloud_silver_intensity",
    "cloud_max_distance",
    "cloud_shadows_enabled",
    "cloud_shadow_strength",
    "cloud_shadow_min_transmittance",
    "cloud_shadow_steps",
];

pub struct GuiConfigLoader;

impl GuiConfigLoader {
    pub fn load() -> GuiConfigFile {
        Self::load_from_path(&Self::config_path())
    }

    pub(crate) fn load_from_path(config_path: &Path) -> GuiConfigFile {
        if !config_path.exists() {
            panic!(
                "GUI config file not found: {}\n\
                 Please ensure {} exists in the config directory.",
                config_path.display(),
                CONFIG_FILE_NAME
            );
        }

        let content = std::fs::read_to_string(config_path).unwrap_or_else(|e| {
            panic!(
                "Failed to read GUI config file at {}: {}",
                config_path.display(),
                e
            );
        });

        let mut config: GuiConfigFile = toml::from_str(&content).unwrap_or_else(|e| {
            panic!(
                "Failed to parse GUI config at {}:\n{}",
                config_path.display(),
                e
            );
        });

        Self::retire_dither_setting(&mut config);
        Self::simplify_ordered_dither_settings(&mut config);
        Self::retire_cloud_settings(&mut config);
        for section in &mut config.section {
            section.param.retain(|param| {
                param.id != "grass_natural_bend_min_voxels"
                    && param.id != "flora_bend_height_power"
                    && param.id != "apple_directional_lighting"
                    && param.id != "apple_light_bands"
                    && param.id != "apple_color_patches"
                    && param.id != "apple_color_stripes"
                    && param.id != "apple_sun_gloss"
                    && param.id != "sprinkler_wetting_patches"
                    && !matches!(
                        param.id.as_str(),
                        "stone_direct_triangles"
                            | "stone_preview_enabled"
                            | "stone_preview_focus"
                            | "stone_preview_lift"
                            | "stone_kind"
                            | "walk_ground_detail"
                            | "low_wall_enabled"
                    )
                    && !RETIRED_FLORA_COLOR_PARAMS.contains(&param.id.as_str())
            });
        }
        config
            .section
            .retain(|section| section.name != "FloraVariation" || !section.param.is_empty());
        for id in [
            "low_wall_kind",
            "low_wall_width",
            "low_wall_height",
            "pavement_kind",
            "pavement_width",
            "pavement_piece_length",
            "pavement_seed",
            "pavement_smooth_curves",
            "pavement_smooth_scale",
            "pavement_retract_stroke",
            "pavement_retract_scale",
            "stone_place_slab",
            "stone_placement_scale",
            "stone_yaw",
            "stone_seed",
            "stone_width",
            "stone_depth",
            "stone_slab_thickness",
            "stone_rock_height",
            "stone_variation",
            "stone_slab_edge_cut",
            "stone_rock_facets",
        ] {
            Self::add_missing_param(&mut config, "Debug", id);
        }
        Self::add_missing_param(&mut config, "Debug", "gui_panel_outlines");
        Self::restore_low_wall_controls(&mut config);
        for param in config
            .section
            .iter_mut()
            .flat_map(|section| &mut section.param)
        {
            // Permanent paths keep saved tuning but no obsolete A/B dependency.
            if matches!(
                param.id.as_str(),
                "walk_ground_detail_height" | "sprinkler_wet_patch_size" | "apple_environment_fill"
            ) {
                param.enabled_if = None;
            }
            if let Some(condition) = &mut param.enabled_if {
                if condition.param == "stone_kind" {
                    condition.param = "stone_place_slab".into();
                    condition.equals = match &condition.equals {
                        GuiParamConditionValue::Integer(0) => GuiParamConditionValue::Bool(true),
                        GuiParamConditionValue::Integer(1) => GuiParamConditionValue::Bool(false),
                        value => value.clone(),
                    };
                }
            }
        }
        for id in ["camera_orbit_max_elevation", "walk_ground_detail_height"] {
            Self::add_missing_param(&mut config, "Debug", id);
        }
        Self::retire_tree_display_experiments(&mut config);
        Self::migrate_flower_stem_selector(&mut config);
        Self::migrate_model_view_quantization(&mut config);
        Self::migrate_scene_pixel_size(&mut config);
        for id in [
            "scene_supersampling_enabled",
            "scene_supersampling_quality",
            "scene_pixel_resolve_mode",
        ] {
            Self::add_missing_param(&mut config, "Debug", id);
        }
        for param in config
            .section
            .iter_mut()
            .flat_map(|s| &mut s.param)
            .filter(|p| {
                matches!(
                    p.id.as_str(),
                    "scene_supersampling_enabled" | "scene_supersampling_quality"
                ) && matches!(
                    p.label.as_str(),
                    "Scene: 2x supersampling (A/B)"
                        | "Scene: supersampling antialiasing (A/B)"
                        | "Scene: antialiasing quality (up to native)"
                )
            })
        {
            // Refresh legacy terminology/options from their schema owner, not values.
            let defaults: GuiConfigFile = toml::from_str(include_str!("../../config/gui.toml"))
                .expect("compiled GUI defaults");
            let schema = defaults
                .section
                .into_iter()
                .flat_map(|s| s.param)
                .find(|p| p.id == param.id)
                .unwrap();
            param.label = schema.label;
            if let (
                GuiParamValue::Choice { options, .. },
                GuiParamValue::Choice {
                    options: new_options,
                    ..
                },
            ) = (&mut param.value, schema.value)
            {
                *options = new_options;
            }
        }
        Self::migrate_model_surface_cache(&mut config);
        Self::add_missing_param(&mut config, "Debug", "sprinkler_mist_density");
        Self::add_missing_param(&mut config, "Debug", "sprinkler_design");
        Self::add_missing_param(&mut config, "Debug", "sprinkler_placement_type");
        Self::add_missing_param(&mut config, "Debug", "sprinkler_size_scale");
        Self::add_missing_param(&mut config, "Debug", "sprinkler_mist_particles");
        // Retire obsolete sprinkler settings at the load boundary; do not rewrite user files.
        for section in &mut config.section {
            section.param.retain(|p| {
                !matches!(
                    p.id.as_str(),
                    "sprinkler_pulse_frequency"
                        | "sprinkler_pulse_duty"
                        | "sprinkler_mist_flow"
                        | "sprinkler_design_variants"
                        | "sprinkler_mist_air"
                        | "vegetation_spray_response"
                )
            });
            for param in &mut section.param {
                if param.enabled_if.as_ref().is_some_and(|condition| {
                    matches!(
                        condition.param.as_str(),
                        "sprinkler_mist_flow"
                            | "sprinkler_design_variants"
                            | "vegetation_spray_response"
                    )
                }) {
                    param.enabled_if = None;
                }
            }
        }
        for p in config
            .section
            .iter_mut()
            .flat_map(|s| &mut s.param)
            .filter(|p| p.id == "sprinkler_mist_density")
        {
            p.label = "Sprinklers: mist intensity".into();
            if let GuiParamValue::Float { min, max, .. } = &mut p.value {
                *min = Some(0.0);
                *max = Some(6.0);
            }
        }
        // Accept both old A/B values, but only the rotating assembly remains.
        // Retire its UI conditions too; never write a file during migration.
        for section in &mut config.section {
            section.param.retain(|p| p.id != "sprinkler_rotary");
            for p in &mut section.param {
                if p.enabled_if
                    .as_ref()
                    .is_some_and(|c| c.param == "sprinkler_rotary")
                {
                    p.enabled_if = None;
                }
            }
        }
        for id in ["sprinkler_spray_angle", "sprinkler_rotation_speed"] {
            Self::add_missing_param(&mut config, "Debug", id);
        }
        Self::add_missing_param(&mut config, "Debug", "sprinkler_wet_patch_size");
        Self::add_missing_param(&mut config, "Debug", "sprinkler_wetting_speed");
        Self::add_missing_param(&mut config, "Debug", "soil_drying_speed");
        Self::add_missing_param(&mut config, "Debug", "grass_moisture_response");
        Self::migrate_stem_rendering_controls(&mut config);
        for id in ["apple_size_scale", "apple_environment_fill"] {
            Self::add_missing_param(&mut config, "Debug", id);
        }
        Self::add_missing_param(&mut config, "Debug", "grass_band_pose_reuse");
        Self::add_missing_param(&mut config, "Debug", "grass_band_pixelization");
        Self::add_missing_param(&mut config, "Debug", "cpu_stem_band_rendering");
        Self::validate(&config, config_path);
        Self::migrate_flutter_frequency(&mut config);
        Self::migrate_frequency_ceiling(&mut config);
        if !config
            .section
            .iter()
            .any(|s| s.name == "Grass Wind Response")
        {
            let defaults: GuiConfigFile =
                toml::from_str(include_str!("../../config/gui.toml")).expect("GUI defaults");
            config.section.push(
                defaults
                    .section
                    .into_iter()
                    .find(|s| s.name == "Grass Wind Response")
                    .unwrap(),
            );
        }
        Self::migrate_flutter_amplitude(&mut config);
        Self::add_missing_param(&mut config, "Sky", "sky_light_strength");
        Self::add_missing_param(&mut config, "Butterflies", "butterfly_wing_transmission");
        for param in config.section.iter_mut().flat_map(|s| &mut s.param) {
            if param.id == "fruit_cycle" {
                param.label = "Fruit Growth (held until kick)".into();
            }
            if param.id == "butterfly_animation_fps" {
                param.label = "Butterfly Update FPS (Position + Heading + Wings)".into();
            }
        }
        Self::add_missing_param(&mut config, "Debug", "tree_stiffness");
        // Rename the old quota control without rewriting saves or losing its value.
        for section in &mut config.section {
            let has_new = section
                .param
                .iter()
                .any(|p| p.id == "tree_kick_fruit_chance_scale");
            if has_new {
                section.param.retain(|p| p.id != "tree_kick_fruit_fraction");
            }
            for param in &mut section.param {
                if param.id == "tree_kick_fruit_fraction" {
                    param.id = "tree_kick_fruit_chance_scale".into();
                }
                if param.id == "tree_kick_fruit_chance_scale" {
                    param.label = "Tree kick fruit release chance scale".into();
                }
            }
        }
        for id in [
            "tree_kick_strength",
            "tree_kick_frequency",
            "tree_kick_damping",
            "tree_kick_reach",
            "tree_kick_cooldown",
            "tree_kick_propagation_speed",
            "tree_kick_fruit_chance_scale",
            "tree_kick_leaf_count",
            "tree_kick_rustle_volume_db",
            "tree_kick_rustle_duration",
            "tree_kick_rustle_body",
            "tree_kick_rustle_brightness",
            "tree_kick_rustle_crackle",
        ] {
            Self::add_missing_param(&mut config, "Debug", id);
        }
        for section in &mut config.section {
            section.param.retain(|p| {
                !matches!(
                    p.id.as_str(),
                    "ddgi_continuous_sampling" | "ddgi_aggregate_history"
                )
            });
        }
        Self::add_missing_section_params(&mut config, "Terrain Material");
        Self::add_missing_section_params(&mut config, "Climbing Plants");
        // Old saves gain new flower controls from the declarations, without
        // reinterpreting or overwriting the existing overall size.
        Self::add_missing_section_params(&mut config, "Flora");
        // The vine no longer has a pause mode. Old zero-speed saves must also
        // become a positive rate, not silently preserve a second way to pause.
        for param in config.section.iter_mut().flat_map(|s| &mut s.param) {
            if param.id == "climbing_speed" {
                if let GuiParamValue::Float { value, min, .. } = &mut param.value {
                    *min = Some(1.0);
                    *value = (*value).max(1.0);
                }
            } else if param.id == "climbing_search_turn" {
                if let GuiParamValue::Float { max, .. } = &mut param.value {
                    *max = Some(6.0);
                }
            }
        }
        // Retired controls must not survive in the live config or on the next save.
        for section in &mut config.section {
            section.param.retain(|param| {
                !matches!(
                    param.id.as_str(),
                    "raster_tree_axis_aligned"
                        | "terrain_missing_lighting_strength"
                        | "terrain_hybrid_lighting"
                        | "terrain_soil_scale_voxels"
                        | "terrain_rock_scale_voxels"
                        | "terrain_rock_layer_tilt"
                        | "terrain_material_enabled"
                        | "terrain_material_color_band"
                        | "butterfly_mesh_enabled"
                        | "butterfly_self_shadows"
                        | "climbing_continuous_stem"
                        | "climbing_paused"
                        | "climbing_enabled"
                        | "climbing_clockwise"
                        | "climbing_seed"
                        | "apple_preview_model"
                        | "model_flower_heads_only"
                        | "model_pixel_snap_views"
                        | "model_pixel_single_light"
                        | "model_pixel_screen_grid"
                        | "raster_tree_hybrid_lighting"
                        | "raster_tree_static"
                )
            });
        }

        log::info!(
            "Loaded GUI config: {} (schema v{}, {} sections, {} params)",
            config_path.display(),
            config.schema_version,
            config.section.len(),
            config.section.iter().map(|s| s.param.len()).sum::<usize>()
        );

        config
    }

    // Keep the saved ID, but distinguish old dropdown indices from new strides
    // by the serialized value type. Legacy 0/1/2/3 become 1/2/4/8, not 1/1/2/3.
    // Loading refreshes the slider schema in memory and never writes the file.
    fn migrate_scene_pixel_size(config: &mut GuiConfigFile) {
        let saved_stride = config
            .section
            .iter()
            .flat_map(|s| &s.param)
            .find(|p| p.id == "scene_pixel_ratio")
            .and_then(|p| match &p.value {
                GuiParamValue::Choice { value, .. } => {
                    Some([1, 2, 4, 8].get(*value as usize).copied().unwrap_or(8))
                }
                GuiParamValue::Uint { value, .. } => Some(
                    crate::tracer::scene_resolution::normalize_pixel_stride(*value),
                ),
                _ => None,
            });
        let mut seen = false;
        for section in &mut config.section {
            let canonical_owner = section.name == "Debug";
            section.param.retain(|p| {
                if p.id != "scene_pixel_ratio" {
                    return true;
                }
                let keep = canonical_owner && !seen;
                seen |= keep;
                keep
            });
        }
        Self::add_missing_param(config, "Debug", "scene_pixel_ratio");
        let defaults: GuiConfigFile =
            toml::from_str(include_str!("../../config/gui.toml")).expect("compiled GUI defaults");
        let schema = defaults
            .section
            .iter()
            .flat_map(|s| &s.param)
            .find(|p| p.id == "scene_pixel_ratio")
            .unwrap();
        let param = config
            .section
            .iter_mut()
            .find(|s| s.name == "Debug")
            .unwrap()
            .param
            .iter_mut()
            .find(|p| p.id == "scene_pixel_ratio")
            .unwrap();
        *param = schema.clone();
        if let (Some(saved), GuiParamValue::Uint { value, .. }) = (saved_stride, &mut param.value) {
            *value = saved;
        }
    }

    // Quantization is always active. Keep one count under the old global ID,
    // inherit flower-only saves, and drop the retired A/B flag regardless of its
    // value. Loading only normalizes memory; the user's file changes on Save.
    fn migrate_model_view_quantization(config: &mut GuiConfigFile) {
        use crate::tracer::model_pixel_views::runtime_count;
        let uint_value = |id: &str| {
            config.section.iter().flat_map(|s| &s.param).find_map(|p| {
                if p.id == id {
                    if let GuiParamValue::Uint { value, .. } = p.value {
                        return Some(value);
                    }
                }
                None
            })
        };
        let requested =
            uint_value("model_pixel_view_count").or_else(|| uint_value("model_flower_view_count"));
        let mut seen_count = false;
        for section in &mut config.section {
            let canonical_owner = section.name == "Debug";
            section.param.retain(|p| match p.id.as_str() {
                "model_flower_view_count" | "model_view_quantization_enabled" => false,
                "model_pixel_view_count" => {
                    let keep = canonical_owner && !seen_count;
                    seen_count |= keep;
                    keep
                }
                _ => true,
            });
        }
        Self::add_missing_param(config, "Debug", "model_pixel_view_count");
        let defaults: GuiConfigFile =
            toml::from_str(include_str!("../../config/gui.toml")).expect("compiled GUI defaults");
        let schema = defaults
            .section
            .iter()
            .flat_map(|s| &s.param)
            .find(|p| p.id == "model_pixel_view_count")
            .unwrap();
        let param = config
            .section
            .iter_mut()
            .find(|s| s.name == "Debug")
            .unwrap()
            .param
            .iter_mut()
            .find(|p| p.id == "model_pixel_view_count")
            .unwrap();
        // Refresh this concern's label/range/conditions without resetting its
        // saved count or changing unrelated settings and ordering.
        *param = schema.clone();
        if let (Some(requested), GuiParamValue::Uint { value, .. }) = (requested, &mut param.value)
        {
            *value = runtime_count(requested);
        }
    }

    fn migrate_stem_rendering_controls(config: &mut GuiConfigFile) {
        for section in &mut config.section {
            section.param.retain(|p| {
                !matches!(
                    p.id.as_str(),
                    "stem_band_mode" | "grass_stem_rendering" | "flower_stem_stepped_width"
                )
            });
        }
        let defaults: GuiConfigFile =
            toml::from_str(include_str!("../../config/gui.toml")).expect("compiled GUI defaults");
        for schema in defaults
            .section
            .iter()
            .flat_map(|s| &s.param)
            .filter(|p| p.id == "grass_band_pixelization")
        {
            for param in config
                .section
                .iter_mut()
                .flat_map(|s| &mut s.param)
                .filter(|p| p.id == schema.id)
            {
                param.label.clone_from(&schema.label);
            }
        }
    }

    // Retire appearance selectors; the combined appearance is now permanent.
    // Preserve tuning values and never write the file on load.
    fn migrate_flower_stem_selector(config: &mut GuiConfigFile) {
        for section in &mut config.section {
            section.param.retain(|p| {
                !matches!(
                    p.id.as_str(),
                    "flower_stem_pixelized"
                        | "flower_stem_fixed_cell_height"
                        | "flower_stem_surface_cells"
                        | "flower_stem_experiment"
                        | "flower_stem_sampling"
                        | "flower_stem_object_sampling"
                        | "flower_stem_object_resolution"
                        | "flower_stem_model_sampling"
                        | "flower_stem_direction_resolution"
                        | "flower_stem_freeze_motion"
                        | "flower_stem_surface_cell_scale"
                        | "flower_stem_surface_geometry"
                        | "flower_stem_geometry_cell_scale"
                )
            });
        }
        let defaults: GuiConfigFile =
            toml::from_str(include_str!("../../config/gui.toml")).expect("compiled GUI defaults");
        for schema in defaults
            .section
            .iter()
            .flat_map(|s| &s.param)
            .filter(|p| p.id.starts_with("flower_stem_") || p.id == "model_flower_voxel_scale")
        {
            if !config
                .section
                .iter()
                .flat_map(|s| &s.param)
                .any(|p| p.id == schema.id)
            {
                Self::add_missing_param(
                    config,
                    if schema.id == "model_flower_voxel_scale" {
                        "Flora"
                    } else {
                        "Debug"
                    },
                    &schema.id,
                );
            }
            for param in config
                .section
                .iter_mut()
                .flat_map(|s| &mut s.param)
                .filter(|p| p.id == schema.id)
            {
                param.label.clone_from(&schema.label);
                param.enabled_if.clone_from(&schema.enabled_if);
                if param.id == "flower_stem_radius_scale" {
                    if let (
                        GuiParamValue::Float { min, max, .. },
                        GuiParamValue::Float {
                            min: schema_min,
                            max: schema_max,
                            ..
                        },
                    ) = (&mut param.value, &schema.value)
                    {
                        *min = *schema_min;
                        *max = *schema_max;
                    }
                }
            }
        }
    }

    // Return saved experimental files to main's voxel-derived tree/leaf display.
    // Keep authored wind; retire real-leaf lifecycle settings without saving on load.
    fn retire_tree_display_experiments(config: &mut GuiConfigFile) {
        let old_wind = config
            .section
            .iter()
            .flat_map(|s| &s.param)
            .find(|p| p.id == "tree_wind")
            .map(|p| p.value.clone());
        let has_wind = config
            .section
            .iter()
            .flat_map(|s| &s.param)
            .any(|p| p.id == "raster_tree_wind");
        for section in &mut config.section {
            section.param.retain(|p| {
                !matches!(
                    p.id.as_str(),
                    "tree_wind"
                        | "tree_pixelized"
                        | "tree_pixel_size"
                        | "attached_leaf_rotation"
                        | "real_leaf_lifecycle"
                        | "leaf_connection_strength"
                        | "leaf_connection_half_life"
                        | "leaf_regrowth_delay"
                        | "leaf_regrowth_duration"
                        | "falling_leaf_mesh"
                        | "falling_leaf_size_scale"
                        | "falling_leaf_pixel_resolution"
                )
            });
        }
        Self::add_missing_param(config, "Debug", "raster_tree_wind");
        if !has_wind {
            if let Some(value) = old_wind {
                if let Some(param) = config
                    .section
                    .iter_mut()
                    .flat_map(|s| &mut s.param)
                    .find(|p| p.id == "raster_tree_wind")
                {
                    param.value = value;
                }
            }
        }
    }

    fn migrate_model_surface_cache(config: &mut GuiConfigFile) {
        use super::gui_config_model::GuiParamValue;
        let defaults: GuiConfigFile =
            toml::from_str(include_str!("../../config/gui.toml")).expect("GUI schema");
        for (owner, id) in [
            ("Debug", "apple_cache_enabled"),
            ("Debug", "butterfly_cache_enabled"),
            ("Debug", "model_flower_cache_enabled"),
            ("Debug", "sprinkler_cache_enabled"),
            ("Debug", "sprinkler_pixel_resolution"),
            ("Debug", "apple_pixel_resolution"),
            ("Butterflies", "butterfly_pixel_resolution"),
            ("Flora", "model_flower_pixel_resolution"),
        ] {
            let retained = config
                .section
                .iter()
                .flat_map(|s| &s.param)
                .find(|p| p.id == id)
                .map(|p| p.value.clone());
            let mut kept = false;
            for section in &mut config.section {
                let canonical = section.name == owner;
                section.param.retain(|p| {
                    if p.id != id {
                        return true;
                    }
                    if canonical && !kept {
                        kept = true;
                        true
                    } else {
                        false
                    }
                });
            }
            Self::add_missing_param(config, owner, id);
            let schema = defaults
                .section
                .iter()
                .flat_map(|s| &s.param)
                .find(|p| p.id == id)
                .unwrap();
            let param = config
                .section
                .iter_mut()
                .flat_map(|s| &mut s.param)
                .find(|p| p.id == id)
                .unwrap();
            *param = schema.clone();
            match (retained, &mut param.value) {
                (Some(GuiParamValue::Bool { value }), GuiParamValue::Bool { value: new }) => {
                    *new = value
                }
                (
                    Some(GuiParamValue::Uint { value, .. }),
                    GuiParamValue::Uint {
                        value: new,
                        min,
                        max,
                    },
                ) => *new = value.clamp(min.unwrap_or(8), max.unwrap_or(32)),
                _ => {}
            }
        }
    }

    fn retire_dither_setting(config: &mut GuiConfigFile) {
        for section in &mut config.section {
            section.param.retain(|p| p.id != "dither_strength_lsb");
        }
        config
            .section
            .retain(|s| s.name != "Post Processing" || !s.param.is_empty());
    }

    fn simplify_ordered_dither_settings(config: &mut GuiConfigFile) {
        // Accept retired preferences on input, without displaying, uploading
        // or writing them back. Never promote local flags to the global flag.
        for section in &mut config.section {
            section.param.retain(|p| {
                !matches!(
                    p.id.as_str(),
                    "ordered_dither_pattern"
                        | "ordered_dither_god_rays"
                        | "ordered_dither_lens_flare"
                        | "ordered_dither_sky_background"
                        | "ordered_dither_terrain_ambient"
                )
            });
        }
        Self::add_missing_section_params(config, "Ordered Dithering");
        let defaults: GuiConfigFile =
            toml::from_str(include_str!("../../config/gui.toml")).expect("compiled GUI defaults");
        for schema in &defaults
            .section
            .iter()
            .find(|s| s.name == "Ordered Dithering")
            .unwrap()
            .param
        {
            for param in config
                .section
                .iter_mut()
                .flat_map(|s| &mut s.param)
                .filter(|p| p.id == schema.id)
            {
                param.label.clone_from(&schema.label);
                param.enabled_if.clone_from(&schema.enabled_if);
            }
        }
    }

    fn retire_cloud_settings(config: &mut GuiConfigFile) {
        for section in &mut config.section {
            section
                .param
                .retain(|param| !RETIRED_CLOUD_PARAMS.contains(&param.id.as_str()));
        }
        // Preserve unrelated authored settings even if a user moved them into
        // the old group, while removing that group's obsolete presentation.
        let mut retained = Vec::new();
        config.section.retain_mut(|section| {
            if section.name == "Clouds" {
                retained.append(&mut section.param);
                false
            } else {
                true
            }
        });
        if !retained.is_empty() {
            if let Some(sky) = config.section.iter_mut().find(|s| s.name == "Sky") {
                sky.param.extend(retained);
            } else {
                config
                    .section
                    .push(crate::app::gui_config_model::GuiSection {
                        name: "Sky".into(),
                        param: retained,
                    });
            }
        }
    }

    fn migrate_flutter_amplitude(config: &mut GuiConfigFile) {
        let Some(leaves) = config.section.iter_mut().find(|s| s.name == "Leaves") else {
            return;
        };
        let Some(strength) = leaves
            .param
            .iter()
            .find(|p| p.id == "leaf_flutter_strength")
            .and_then(|p| p.value.get_float())
            .map(|v| v.0)
        else {
            return;
        };
        let defaults: GuiConfigFile =
            toml::from_str(include_str!("../../config/gui.toml")).expect("compiled GUI defaults");
        for (id, value) in [
            ("leaf_flutter_amplitude_low", 0.),
            ("leaf_flutter_amplitude_high", strength.clamp(0., 2.) * 0.5),
        ] {
            if leaves.param.iter().any(|p| p.id == id) {
                continue;
            }
            let mut param = defaults
                .section
                .iter()
                .flat_map(|s| &s.param)
                .find(|p| p.id == id)
                .expect("amplitude schema")
                .clone();
            if let GuiParamValue::Float { value: stored, .. } = &mut param.value {
                *stored = value;
            }
            leaves.param.push(param);
        }
        leaves.param.retain(|p| p.id != "leaf_flutter_strength");
        if let Some(scale) = leaves
            .param
            .iter_mut()
            .find(|p| p.id == "leaf_local_displacement_voxels")
        {
            scale.label = "Amplitude Scaling (voxels)".into();
        }
    }

    fn migrate_flutter_frequency(config: &mut GuiConfigFile) {
        let Some(leaves) = config.section.iter_mut().find(|s| s.name == "Leaves") else {
            return;
        };
        let old = |id: &str| {
            leaves
                .param
                .iter()
                .find(|p| p.id == id)
                .and_then(|p| p.value.get_float())
                .map(|v| v.0)
        };
        let base = old("leaf_flutter_frequency_hz");
        let scale = old("leaf_flutter_frequency_scale");
        if base.is_none() && scale.is_none() {
            return;
        }
        let base = base.unwrap_or(1.8).clamp(0.5, 12.);
        let high = base * scale.unwrap_or(1.).clamp(0.5, 2.);
        let defaults: GuiConfigFile =
            toml::from_str(include_str!("../../config/gui.toml")).expect("compiled GUI defaults");
        for (id, value) in [
            ("leaf_flutter_frequency_low_hz", base),
            ("leaf_flutter_frequency_high_hz", high),
            ("leaf_flutter_frequency_ceiling_hz", 24.),
        ] {
            if leaves.param.iter().any(|p| p.id == id) {
                continue;
            }
            let mut param = defaults
                .section
                .iter()
                .flat_map(|s| &s.param)
                .find(|p| p.id == id)
                .expect("frequency schema")
                .clone();
            if let GuiParamValue::Float { value: stored, .. } = &mut param.value {
                *stored = value;
            }
            leaves.param.push(param);
        }
        leaves.param.retain(|p| {
            p.id != "leaf_flutter_frequency_hz" && p.id != "leaf_flutter_frequency_scale"
        });
    }

    fn migrate_frequency_ceiling(config: &mut GuiConfigFile) {
        let Some(leaves) = config.section.iter_mut().find(|s| s.name == "Leaves") else {
            return;
        };
        let old = leaves
            .param
            .iter()
            .find(|p| p.id == "leaf_flutter_frequency_multiplier")
            .and_then(|p| p.value.get_float())
            .map(|v| v.0);
        if let Some(multiplier) = old {
            if !leaves
                .param
                .iter()
                .any(|p| p.id == "leaf_flutter_frequency_ceiling_hz")
            {
                let defaults: GuiConfigFile =
                    toml::from_str(include_str!("../../config/gui.toml")).expect("GUI defaults");
                let mut ceiling = defaults
                    .section
                    .iter()
                    .flat_map(|s| &s.param)
                    .find(|p| p.id == "leaf_flutter_frequency_ceiling_hz")
                    .unwrap()
                    .clone();
                if let GuiParamValue::Float { value, .. } = &mut ceiling.value {
                    *value = multiplier.clamp(0.25, 2.) * 24.;
                }
                leaves.param.push(ceiling);
            }
            leaves
                .param
                .retain(|p| p.id != "leaf_flutter_frequency_multiplier");
        }
    }

    fn add_missing_section_params(config: &mut GuiConfigFile, section_name: &str) {
        let defaults: GuiConfigFile = toml::from_str(include_str!("../../config/gui.toml"))
            .expect("compiled GUI defaults must be valid");
        let section = defaults
            .section
            .into_iter()
            .find(|section| section.name == section_name)
            .expect("compiled GUI defaults must define requested section");
        if let Some(saved) = config.section.iter_mut().find(|s| s.name == section_name) {
            for param in section.param {
                if let Some(existing) = saved.param.iter_mut().find(|p| p.id == param.id) {
                    // Presentation follows the current schema; retain the user's authored value.
                    existing.label = param.label;
                    if existing.id == "climbing_fixture" {
                        if let (
                            GuiParamValue::Choice { options, .. },
                            GuiParamValue::Choice {
                                options: defaults, ..
                            },
                        ) = (&mut existing.value, &param.value)
                        {
                            options.clone_from(defaults);
                        }
                    }
                } else {
                    saved.param.push(param);
                }
            }
        } else {
            config.section.push(section);
        }
    }

    // Old prototype saves use these same IDs. Refresh presentation/conditions,
    // never their saved material, dimensions, mode or other tool preferences.
    fn restore_low_wall_controls(config: &mut GuiConfigFile) {
        let defaults: GuiConfigFile =
            toml::from_str(include_str!("../../config/gui.toml")).expect("compiled GUI defaults");
        for schema in defaults.section.iter().flat_map(|s| &s.param).filter(|p| {
            p.id.starts_with("low_wall_")
                || matches!(
                    p.id.as_str(),
                    "pavement_kind" | "pavement_width" | "pavement_piece_length"
                )
        }) {
            for param in config
                .section
                .iter_mut()
                .flat_map(|s| &mut s.param)
                .filter(|p| p.id == schema.id)
            {
                param.label.clone_from(&schema.label);
                param.enabled_if.clone_from(&schema.enabled_if);
            }
        }
    }

    fn add_missing_param(config: &mut GuiConfigFile, section_name: &str, param_id: &str) {
        if config
            .section
            .iter()
            .flat_map(|section| &section.param)
            .any(|param| param.id == param_id)
        {
            return;
        }
        let Some(section) = config
            .section
            .iter_mut()
            .find(|section| section.name == section_name)
        else {
            return;
        };
        // Older saved settings predate this control. Take its schema and default from the same
        // build-time source as the generated GUI; never overwrite an existing authored value.
        let defaults: GuiConfigFile = toml::from_str(include_str!("../../config/gui.toml"))
            .expect("compiled GUI defaults must be valid");
        let param = defaults
            .section
            .into_iter()
            .flat_map(|section| section.param)
            .find(|param| param.id == param_id)
            .expect("compiled GUI defaults must define requested parameter");
        section.param.push(param);
    }

    pub fn config_path() -> std::path::PathBuf {
        re_flora_vkn::project_root()
            .join("config")
            .join(CONFIG_FILE_NAME)
    }

    pub(crate) fn save_to_path(config: &GuiConfigFile, config_path: &Path) -> std::io::Result<()> {
        let content = toml::to_string_pretty(config)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let content = Self::normalize_float_assignments(&content, GUI_FLOAT_DECIMALS);
        let parent = config_path.parent().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("GUI config path has no parent: {}", config_path.display()),
            )
        })?;
        let mut temporary = tempfile::Builder::new()
            .prefix(".gui-config-")
            .suffix(".tmp")
            .tempfile_in(parent)?;
        temporary.write_all(content.as_bytes())?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(config_path)
            .map_err(|error| error.error)?;
        log::info!("Saved GUI config to {}", config_path.display());
        Ok(())
    }

    fn normalize_float_assignments(content: &str, decimals: usize) -> String {
        let mut normalized = String::with_capacity(content.len());

        for (idx, line) in content.lines().enumerate() {
            if idx > 0 {
                normalized.push('\n');
            }
            normalized.push_str(&Self::normalize_float_line(line, decimals));
        }

        if content.ends_with('\n') {
            normalized.push('\n');
        }

        normalized
    }

    fn normalize_float_line(line: &str, decimals: usize) -> String {
        let Some((lhs, rhs)) = line.split_once('=') else {
            return line.to_string();
        };

        let key = lhs.trim();
        if !matches!(key, "value" | "min" | "max") {
            return line.to_string();
        }

        let raw_value = rhs.trim();
        if raw_value.starts_with('"') || raw_value.starts_with('\'') {
            return line.to_string();
        }

        let Some(value) = Self::format_decimal(raw_value, decimals) else {
            return line.to_string();
        };

        format!("{lhs}= {value}")
    }

    fn format_decimal(raw_value: &str, decimals: usize) -> Option<String> {
        let parsed = raw_value.parse::<f64>().ok()?;
        let mut value = format!("{parsed:.decimals$}");

        if value.contains('.') {
            value = value
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_string();
        }

        if value == "-0" {
            value = "0".to_string();
        }

        Some(value)
    }

    fn validate(config: &GuiConfigFile, path: &Path) {
        let mut errors = Vec::new();

        if config.schema_version != SUPPORTED_SCHEMA_VERSION {
            errors.push(format!(
                "Unsupported schema version: {} (supported: {})",
                config.schema_version, SUPPORTED_SCHEMA_VERSION
            ));
        }

        let mut section_names = HashSet::new();
        let mut param_ids = HashSet::new();

        for (section_idx, section) in config.section.iter().enumerate() {
            if section.name.is_empty() {
                errors.push(format!("Section at index {} has empty name", section_idx));
            }

            if !section_names.insert(section.name.clone()) {
                errors.push(format!("Duplicate section name: '{}'", section.name));
            }

            for (param_idx, param) in section.param.iter().enumerate() {
                if param.id.is_empty() {
                    errors.push(format!(
                        "Section '{}' param at index {} has empty id",
                        section.name, param_idx
                    ));
                }

                if !param_ids.insert(param.id.clone()) {
                    errors.push(format!(
                        "Duplicate param id: '{}' in section '{}'",
                        param.id, section.name
                    ));
                }

                Self::validate_param(&mut errors, &section.name, param, param_idx);
            }
        }

        let params_by_id = config
            .section
            .iter()
            .flat_map(|section| section.param.iter())
            .map(|param| (param.id.as_str(), param))
            .collect::<HashMap<_, _>>();
        for section in &config.section {
            for param in &section.param {
                Self::validate_enabled_if(&mut errors, &section.name, param, &params_by_id);
            }
        }

        if !errors.is_empty() {
            let mut msg = format!("GUI config validation failed for {}:\n", path.display());
            for error in errors {
                msg.push_str(&format!("  - {}\n", error));
            }
            panic!("{}", msg);
        }
    }

    fn validate_param(
        errors: &mut Vec<String>,
        section_name: &str,
        param: &crate::app::gui_config_model::GuiParam,
        _param_idx: usize,
    ) {
        use crate::app::gui_config_model::GuiParamValue;

        match (&param.kind, &param.value) {
            (GuiParamKind::Float, GuiParamValue::Float { value, min, max }) => {
                if let (Some(min), Some(max)) = (min, max) {
                    if min > max {
                        errors.push(format!(
                            "Section '{}' param '{}': min ({}) > max ({})",
                            section_name, param.id, min, max
                        ));
                    }
                }
                if let Some(min) = min {
                    if value < min {
                        errors.push(format!(
                            "Section '{}' param '{}': value ({}) < min ({})",
                            section_name, param.id, value, min
                        ));
                    }
                }
                if let Some(max) = max {
                    if value > max {
                        errors.push(format!(
                            "Section '{}' param '{}': value ({}) > max ({})",
                            section_name, param.id, value, max
                        ));
                    }
                }
            }
            (GuiParamKind::Int, GuiParamValue::Int { value, min, max }) => {
                if let (Some(min), Some(max)) = (min, max) {
                    if min > max {
                        errors.push(format!(
                            "Section '{}' param '{}': min ({}) > max ({})",
                            section_name, param.id, min, max
                        ));
                    }
                }
                if let Some(min) = min {
                    if value < min {
                        errors.push(format!(
                            "Section '{}' param '{}': value ({}) < min ({})",
                            section_name, param.id, value, min
                        ));
                    }
                }
                if let Some(max) = max {
                    if value > max {
                        errors.push(format!(
                            "Section '{}' param '{}': value ({}) > max ({})",
                            section_name, param.id, value, max
                        ));
                    }
                }
            }
            (GuiParamKind::Uint, GuiParamValue::Uint { value, min, max }) => {
                if let (Some(min), Some(max)) = (min, max) {
                    if min > max {
                        errors.push(format!(
                            "Section '{}' param '{}': min ({}) > max ({})",
                            section_name, param.id, min, max
                        ));
                    }
                }
                if let Some(min) = min {
                    if value < min {
                        errors.push(format!(
                            "Section '{}' param '{}': value ({}) < min ({})",
                            section_name, param.id, value, min
                        ));
                    }
                }
                if let Some(max) = max {
                    if value > max {
                        errors.push(format!(
                            "Section '{}' param '{}': value ({}) > max ({})",
                            section_name, param.id, value, max
                        ));
                    }
                }
            }
            (GuiParamKind::Choice, GuiParamValue::Choice { value, options }) => {
                if options.is_empty() {
                    errors.push(format!(
                        "Section '{}' param '{}': choice options must not be empty",
                        section_name, param.id
                    ));
                }
                if *value as usize >= options.len() {
                    errors.push(format!(
                        "Section '{}' param '{}': value ({}) is outside choice options 0..{}",
                        section_name,
                        param.id,
                        value,
                        options.len().saturating_sub(1)
                    ));
                }
                if options.iter().any(|option| option.trim().is_empty()) {
                    errors.push(format!(
                        "Section '{}' param '{}': choice options must not contain empty labels",
                        section_name, param.id
                    ));
                }
            }
            (GuiParamKind::String, GuiParamValue::String { value }) => {
                if value.trim().is_empty() {
                    errors.push(format!(
                        "Section '{}' param '{}': string value must not be empty",
                        section_name, param.id
                    ));
                }
            }
            (GuiParamKind::Bool, GuiParamValue::Bool { .. }) => {}
            (GuiParamKind::Color, GuiParamValue::Color { value }) => {
                if !Self::is_valid_color(value) {
                    errors.push(format!(
                        "Section '{}' param '{}': invalid color format '{}' (expected #RRGGBB or #RRGGBBAA)",
                        section_name, param.id, value
                    ));
                }
            }
            (kind, _value) => {
                let expected = match kind {
                    GuiParamKind::Float => "float { value, min, max }",
                    GuiParamKind::Int => "int { value, min, max }",
                    GuiParamKind::Uint => "uint { value, min, max }",
                    GuiParamKind::Choice => "choice { value, options }",
                    GuiParamKind::String => "string { value }",
                    GuiParamKind::Bool => "bool { value }",
                    GuiParamKind::Color => "color { value }",
                };
                errors.push(format!(
                    "Section '{}' param '{}': wrong value type for kind '{}', expected {}",
                    section_name,
                    param.id,
                    match kind {
                        GuiParamKind::Float => "float",
                        GuiParamKind::Int => "int",
                        GuiParamKind::Uint => "uint",
                        GuiParamKind::Choice => "choice",
                        GuiParamKind::String => "string",
                        GuiParamKind::Bool => "bool",
                        GuiParamKind::Color => "color",
                    },
                    expected
                ));
            }
        }
    }

    fn validate_enabled_if(
        errors: &mut Vec<String>,
        section_name: &str,
        param: &crate::app::gui_config_model::GuiParam,
        params_by_id: &HashMap<&str, &crate::app::gui_config_model::GuiParam>,
    ) {
        let Some(condition) = &param.enabled_if else {
            return;
        };

        if condition.param == param.id {
            errors.push(format!(
                "Section '{}' param '{}': enabled_if cannot reference itself",
                section_name, param.id
            ));
            return;
        }

        let Some(controller) = params_by_id.get(condition.param.as_str()) else {
            errors.push(format!(
                "Section '{}' param '{}': enabled_if references unknown param '{}'",
                section_name, param.id, condition.param
            ));
            return;
        };

        let compatible = matches!(
            (&condition.equals, &controller.value),
            (GuiParamConditionValue::Bool(_), GuiParamValue::Bool { .. })
                | (
                    GuiParamConditionValue::Integer(_),
                    GuiParamValue::Int { .. }
                )
                | (
                    GuiParamConditionValue::Integer(_),
                    GuiParamValue::Uint { .. }
                )
                | (
                    GuiParamConditionValue::Integer(_),
                    GuiParamValue::Choice { .. }
                )
                | (
                    GuiParamConditionValue::String(_),
                    GuiParamValue::String { .. }
                )
                | (
                    GuiParamConditionValue::String(_),
                    GuiParamValue::Color { .. }
                )
        );
        if !compatible {
            errors.push(format!(
                "Section '{}' param '{}': enabled_if value type does not match controller '{}'",
                section_name, param.id, condition.param
            ));
        }
    }

    fn is_valid_color(s: &str) -> bool {
        if s.len() != 7 && s.len() != 9 {
            return false;
        }
        if !s.starts_with('#') {
            return false;
        }
        s[1..].chars().all(|c| c.is_ascii_hexdigit())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn panel_outline_setting_upgrades_old_saves_and_preserves_both_choices() {
        for saved in [None, Some(false), Some(true)] {
            let mut config: GuiConfigFile =
                toml::from_str(include_str!("../../config/gui.toml")).unwrap();
            for section in &mut config.section {
                if let Some(value) = saved {
                    for param in &mut section.param {
                        if param.id == "gui_panel_outlines" {
                            param.value = super::GuiParamValue::Bool { value };
                        }
                    }
                } else {
                    section.param.retain(|p| p.id != "gui_panel_outlines");
                }
            }
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("gui.toml");
            let original = toml::to_string_pretty(&config).unwrap();
            std::fs::write(&path, &original).unwrap();
            let loaded = GuiConfigLoader::load_from_path(&path);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
            let fields: Vec<_> = loaded
                .section
                .iter()
                .flat_map(|s| &s.param)
                .filter(|p| p.id == "gui_panel_outlines")
                .collect();
            assert_eq!(fields.len(), 1);
            assert_eq!(fields[0].value.get_bool(), Some(saved.unwrap_or(true)));
            GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
            assert_eq!(
                serde_json::to_value(&loaded).unwrap(),
                serde_json::to_value(GuiConfigLoader::load_from_path(&path)).unwrap()
            );
        }
    }

    #[test]
    fn hardware_candidates_load_old_files_and_preserve_saved_choices_without_writing() {
        use crate::app::gui_config_model::{GuiParamConditionValue, GuiParamEnabledIf};
        for existing in [false, true] {
            for old_enabled in [false, true] {
                let mut config: GuiConfigFile =
                    toml::from_str(include_str!("../../config/gui.toml")).unwrap();
                let section = config
                    .section
                    .iter_mut()
                    .find(|s| s.name == "Debug")
                    .unwrap();
                let template = section
                    .param
                    .iter()
                    .find(|p| p.id == "sprinkler_cache_enabled")
                    .unwrap()
                    .clone();
                for id in [
                    "sprinkler_design_variants",
                    "sprinkler_mist_flow",
                    "sprinkler_mist_air",
                ] {
                    let mut obsolete = template.clone();
                    obsolete.id = id.into();
                    obsolete.value = super::GuiParamValue::Bool { value: old_enabled };
                    section.param.push(obsolete);
                }
                for (id, dependency) in [
                    ("sprinkler_design", "sprinkler_design_variants"),
                    ("sprinkler_mist_air", "sprinkler_mist_flow"),
                ] {
                    let param = section.param.iter_mut().find(|p| p.id == id).unwrap();
                    param.enabled_if = Some(GuiParamEnabledIf {
                        param: dependency.into(),
                        equals: GuiParamConditionValue::Bool(true),
                    });
                    if id == "sprinkler_design" {
                        if let super::GuiParamValue::Choice { value, .. } = &mut param.value {
                            *value = 2;
                        }
                    }
                }
                if !existing {
                    section.param.retain(|p| p.id != "sprinkler_design");
                }
                let dir = tempfile::tempdir().unwrap();
                let path = dir.path().join("gui.toml");
                let original = toml::to_string_pretty(&config).unwrap();
                std::fs::write(&path, &original).unwrap();
                let loaded = GuiConfigLoader::load_from_path(&path);
                assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
                let params: Vec<_> = loaded.section.iter().flat_map(|s| &s.param).collect();
                assert!(!params.iter().any(|p| matches!(
                    p.id.as_str(),
                    "sprinkler_design_variants" | "sprinkler_mist_flow"
                )));
                let design = params.iter().find(|p| p.id == "sprinkler_design").unwrap();
                assert_eq!(
                    design.value.get_choice().unwrap().0,
                    if existing { 2 } else { 0 }
                );
                assert!(design.enabled_if.is_none());
                assert!(!params.iter().any(|p| p.id == "sprinkler_mist_air"));
                GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
                assert_eq!(
                    serde_json::to_value(&loaded).unwrap(),
                    serde_json::to_value(GuiConfigLoader::load_from_path(&path)).unwrap()
                );
            }
        }
    }

    #[test]
    fn grass_moisture_migrates_additively_without_writes_and_saves_disabled() {
        for saved in [None, Some(false), Some(true)] {
            let mut config: GuiConfigFile =
                toml::from_str(include_str!("../../config/gui.toml")).unwrap();
            for section in &mut config.section {
                if let Some(value) = saved {
                    for param in &mut section.param {
                        if param.id == "grass_moisture_response" {
                            param.value = super::GuiParamValue::Bool { value };
                        }
                    }
                } else {
                    section.param.retain(|p| p.id != "grass_moisture_response");
                }
            }
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("gui.toml");
            let original = toml::to_string_pretty(&config).unwrap();
            std::fs::write(&path, &original).unwrap();
            let loaded = GuiConfigLoader::load_from_path(&path);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
            let values: Vec<_> = loaded
                .section
                .iter()
                .flat_map(|s| &s.param)
                .filter(|p| p.id == "grass_moisture_response")
                .collect();
            assert_eq!(values.len(), 1);
            assert!(
                matches!(values[0].value, super::GuiParamValue::Bool { value } if value == saved.unwrap_or(true))
            );
            GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
            let reloaded = GuiConfigLoader::load_from_path(&path);
            assert_eq!(
                serde_json::to_value(&loaded).unwrap(),
                serde_json::to_value(&reloaded).unwrap()
            );
        }
    }

    #[test]
    fn retired_sprinkler_air_is_removed_for_both_saved_modes_without_writes() {
        for saved in [None, Some(false), Some(true)] {
            let mut config: GuiConfigFile =
                toml::from_str(include_str!("../../config/gui.toml")).unwrap();
            for section in &mut config.section {
                if let Some(value) = saved {
                    if section.name == "Debug" {
                        let mut param = section
                            .param
                            .iter()
                            .find(|p| p.id == "sprinkler_mist_particles")
                            .unwrap()
                            .clone();
                        param.id = "sprinkler_mist_air".into();
                        param.value = super::GuiParamValue::Bool { value };
                        section.param.push(param);
                    }
                } else {
                    section.param.retain(|p| p.id != "sprinkler_mist_air");
                }
            }
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("gui.toml");
            let original = toml::to_string_pretty(&config).unwrap();
            std::fs::write(&path, &original).unwrap();
            let loaded = GuiConfigLoader::load_from_path(&path);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
            let controls: Vec<_> = loaded
                .section
                .iter()
                .flat_map(|s| &s.param)
                .filter(|p| p.id == "sprinkler_mist_air")
                .collect();
            assert!(controls.is_empty());
            // Only retire the old control; all unrelated preference data is preserved.
            let mut normalized = loaded.clone();
            for section in &mut normalized.section {
                section.param.retain(|p| p.id != "sprinkler_mist_air");
            }
            for section in &mut config.section {
                section.param.retain(|p| p.id != "sprinkler_mist_air");
            }
            assert_eq!(
                serde_json::to_value(normalized).unwrap(),
                serde_json::to_value(config).unwrap()
            );
            GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
            assert_eq!(
                serde_json::to_value(&loaded).unwrap(),
                serde_json::to_value(GuiConfigLoader::load_from_path(&path)).unwrap()
            );
        }
    }

    #[test]
    fn spray_response_retires_without_changing_tall_sprinklers_or_wind() {
        use crate::app::gui_config_model::{GuiParamConditionValue, GuiParamEnabledIf};
        for enabled in [false, true] {
            for section_name in ["Debug", "Grass Wind Response"] {
                let mut config: GuiConfigFile =
                    toml::from_str(include_str!("../../config/gui.toml")).unwrap();
                let params = &mut config
                    .section
                    .iter_mut()
                    .find(|s| s.name == "Debug")
                    .unwrap()
                    .param;
                let placement = params
                    .iter_mut()
                    .find(|p| p.id == "sprinkler_placement_type")
                    .unwrap();
                if let super::GuiParamValue::Choice { value, .. } = &mut placement.value {
                    *value = 1;
                } else {
                    panic!("placement type must remain a choice");
                }
                let mut obsolete = params
                    .iter()
                    .find(|p| p.id == "sprinkler_mist_particles")
                    .unwrap()
                    .clone();
                obsolete.id = "vegetation_spray_response".into();
                obsolete.value = super::GuiParamValue::Bool { value: enabled };
                let expected = config.clone();
                config
                    .section
                    .iter_mut()
                    .find(|s| s.name == section_name)
                    .unwrap()
                    .param
                    .push(obsolete);
                config
                    .section
                    .iter_mut()
                    .flat_map(|s| &mut s.param)
                    .find(|p| p.id == "sprinkler_rotation_speed")
                    .unwrap()
                    .enabled_if = Some(GuiParamEnabledIf {
                    param: "vegetation_spray_response".into(),
                    equals: GuiParamConditionValue::Bool(true),
                });
                let dir = tempfile::tempdir().unwrap();
                let path = dir.path().join("gui.toml");
                let original = toml::to_string_pretty(&config).unwrap();
                std::fs::write(&path, &original).unwrap();
                let loaded = GuiConfigLoader::load_from_path(&path);
                assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
                assert_eq!(
                    serde_json::to_value(&loaded).unwrap(),
                    serde_json::to_value(&expected).unwrap()
                );
                GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
                assert!(!std::fs::read_to_string(&path)
                    .unwrap()
                    .contains("vegetation_spray_response"));
                assert_eq!(
                    serde_json::to_value(GuiConfigLoader::load_from_path(&path)).unwrap(),
                    serde_json::to_value(&expected).unwrap()
                );
            }
        }
    }

    #[test]
    fn obsolete_sprinkler_oscillators_are_discarded_without_rewriting_old_saves() {
        let mut config: GuiConfigFile =
            toml::from_str(include_str!("../../config/gui.toml")).unwrap();
        let section = config
            .section
            .iter_mut()
            .find(|s| s.name == "Debug")
            .unwrap();
        let template = section
            .param
            .iter()
            .find(|p| p.id == "sprinkler_rotation_speed")
            .unwrap()
            .clone();
        for id in ["sprinkler_pulse_frequency", "sprinkler_pulse_duty"] {
            let mut old = template.clone();
            old.id = id.into();
            section.param.push(old);
        }
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("gui.toml");
        GuiConfigLoader::save_to_path(&config, &path).unwrap();
        let original = std::fs::read(&path).unwrap();
        let loaded = GuiConfigLoader::load_from_path(&path);
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert!(!loaded
            .section
            .iter()
            .flat_map(|s| &s.param)
            .any(|p| matches!(
                p.id.as_str(),
                "sprinkler_pulse_frequency" | "sprinkler_pulse_duty"
            )));
    }

    #[test]
    fn spray_controls_migrate_without_overwriting_saved_values() {
        let ids = ["sprinkler_spray_angle", "sprinkler_rotation_speed"];
        for existing in [false, true] {
            let mut config: GuiConfigFile =
                toml::from_str(include_str!("../../config/gui.toml")).unwrap();
            let defaults: Vec<_> = config
                .section
                .iter()
                .flat_map(|s| &s.param)
                .filter(|p| ids.contains(&p.id.as_str()))
                .cloned()
                .collect();
            assert_eq!(defaults.len(), 2);
            for section in &mut config.section {
                if existing {
                    for p in &mut section.param {
                        if ids.contains(&p.id.as_str()) {
                            if let super::GuiParamValue::Float { value, .. } = &mut p.value {
                                *value = 27.0;
                            }
                        }
                    }
                } else {
                    section.param.retain(|p| !ids.contains(&p.id.as_str()));
                }
            }
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("gui.toml");
            let original = toml::to_string_pretty(&config).unwrap();
            std::fs::write(&path, &original).unwrap();
            let loaded = GuiConfigLoader::load_from_path(&path);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
            for p in loaded
                .section
                .iter()
                .flat_map(|s| &s.param)
                .filter(|p| ids.contains(&p.id.as_str()))
            {
                let expected = defaults.iter().find(|d| d.id == p.id).unwrap();
                if existing {
                    assert!(
                        matches!(p.value, super::GuiParamValue::Float { value, .. } if value == 27.0)
                    );
                } else {
                    assert_eq!(
                        serde_json::to_value(p).unwrap(),
                        serde_json::to_value(expected).unwrap()
                    );
                }
            }
            GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
            assert_eq!(
                serde_json::to_value(&loaded).unwrap(),
                serde_json::to_value(GuiConfigLoader::load_from_path(&path)).unwrap()
            );
        }
    }

    #[test]
    fn rotary_switch_retires_without_writing_or_disabling_saved_controls() {
        use crate::app::gui_config_model::{GuiParamConditionValue, GuiParamEnabledIf};
        for enabled in [false, true] {
            let mut config: GuiConfigFile =
                toml::from_str(include_str!("../../config/gui.toml")).unwrap();
            let params = &mut config
                .section
                .iter_mut()
                .find(|s| s.name == "Debug")
                .unwrap()
                .param;
            let mut obsolete = params
                .iter()
                .find(|p| p.id == "sprinkler_cache_enabled")
                .unwrap()
                .clone();
            obsolete.id = "sprinkler_rotary".into();
            obsolete.value = super::GuiParamValue::Bool { value: enabled };
            params.push(obsolete);
            for p in params.iter_mut().filter(|p| {
                matches!(
                    p.id.as_str(),
                    "sprinkler_spray_angle" | "sprinkler_rotation_speed"
                )
            }) {
                p.enabled_if = Some(GuiParamEnabledIf {
                    param: "sprinkler_rotary".into(),
                    equals: GuiParamConditionValue::Bool(true),
                });
            }
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("gui.toml");
            let original = toml::to_string_pretty(&config).unwrap();
            std::fs::write(&path, &original).unwrap();
            let loaded = GuiConfigLoader::load_from_path(&path);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
            let params: Vec<_> = loaded.section.iter().flat_map(|s| &s.param).collect();
            assert!(!params.iter().any(|p| p.id == "sprinkler_rotary"));
            for p in config
                .section
                .iter()
                .flat_map(|s| &s.param)
                .filter(|p| p.id != "sprinkler_rotary")
            {
                let after = params.iter().find(|q| q.id == p.id).unwrap();
                assert_eq!(
                    serde_json::to_value(&p.value).unwrap(),
                    serde_json::to_value(&after.value).unwrap()
                );
                assert!(!after
                    .enabled_if
                    .as_ref()
                    .is_some_and(|c| c.param == "sprinkler_rotary"));
            }
            GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
            assert_eq!(
                serde_json::to_value(GuiConfigLoader::load_from_path(&path)).unwrap(),
                serde_json::to_value(loaded).unwrap()
            );
        }
    }

    #[test]
    fn sprinkler_wetting_switch_retires_without_losing_scale_or_disabling_slider() {
        use crate::app::gui_config_model::{GuiParamConditionValue, GuiParamEnabledIf};
        for enabled in [false, true] {
            for scale in [2.0, 4.5, 17.0, 32.0] {
                let mut config: GuiConfigFile =
                    toml::from_str(include_str!("../../config/gui.toml")).unwrap();
                let params = &mut config
                    .section
                    .iter_mut()
                    .find(|s| s.name == "Debug")
                    .unwrap()
                    .param;
                let mut toggle = params
                    .iter()
                    .find(|p| p.id == "sprinkler_cache_enabled")
                    .unwrap()
                    .clone();
                toggle.id = "sprinkler_wetting_patches".into();
                toggle.value = super::GuiParamValue::Bool { value: enabled };
                params.push(toggle);
                let slider = params
                    .iter_mut()
                    .find(|p| p.id == "sprinkler_wet_patch_size")
                    .unwrap();
                slider.value = super::GuiParamValue::Float {
                    value: scale,
                    min: Some(2.0),
                    max: Some(32.0),
                };
                slider.enabled_if = Some(GuiParamEnabledIf {
                    param: "sprinkler_wetting_patches".into(),
                    equals: GuiParamConditionValue::Bool(true),
                });
                let dir = tempfile::tempdir().unwrap();
                let path = dir.path().join("gui.toml");
                let original = toml::to_string_pretty(&config).unwrap();
                std::fs::write(&path, &original).unwrap();
                let loaded = GuiConfigLoader::load_from_path(&path);
                assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
                assert!(!loaded
                    .section
                    .iter()
                    .flat_map(|s| &s.param)
                    .any(|p| p.id == "sprinkler_wetting_patches"));
                let slider = loaded
                    .section
                    .iter()
                    .flat_map(|s| &s.param)
                    .find(|p| p.id == "sprinkler_wet_patch_size")
                    .unwrap();
                assert!(slider.enabled_if.is_none());
                assert!(
                    matches!(slider.value,super::GuiParamValue::Float{value,..} if value==scale)
                );
                GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
                assert!(!std::fs::read_to_string(&path)
                    .unwrap()
                    .contains("sprinkler_wetting_patches"));
                let reloaded = GuiConfigLoader::load_from_path(&path);
                assert_eq!(
                    serde_json::to_value(reloaded).unwrap(),
                    serde_json::to_value(loaded).unwrap()
                );
            }
        }
    }

    #[test]
    fn walking_detail_height_upgrades_old_saves_without_rewriting_and_round_trips() {
        use super::GuiParamValue;
        let mut config: GuiConfigFile =
            toml::from_str(include_str!("../../config/gui.toml")).unwrap();
        for section in &mut config.section {
            section
                .param
                .retain(|p| p.id != "walk_ground_detail_height");
        }
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("gui.toml");
        let old = toml::to_string_pretty(&config).unwrap();
        std::fs::write(&path, &old).unwrap();
        let mut loaded = GuiConfigLoader::load_from_path(&path);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), old);
        let slider = loaded
            .section
            .iter_mut()
            .flat_map(|s| &mut s.param)
            .find(|p| p.id == "walk_ground_detail_height")
            .unwrap();
        assert!(slider.enabled_if.is_none());
        if let GuiParamValue::Float { value, min, max } = &mut slider.value {
            assert_eq!((*value, *min, *max), (3.0, Some(0.0), Some(4.0)));
            *value = 1.5;
        } else {
            panic!("ground detail must be a saved float");
        }
        GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
        let reloaded = GuiConfigLoader::load_from_path(&path);
        let slider = reloaded
            .section
            .iter()
            .flat_map(|s| &s.param)
            .find(|p| p.id == "walk_ground_detail_height")
            .unwrap();
        assert!(slider.enabled_if.is_none());
        assert!(matches!(
            slider.value,
            GuiParamValue::Float { value: 1.5, .. }
        ));
    }

    #[test]
    fn walking_detail_switch_is_retired_without_losing_saved_tuning() {
        use crate::app::gui_config_model::{
            GuiParamConditionValue, GuiParamEnabledIf, GuiParamKind, GuiParamValue,
        };
        for enabled in [false, true] {
            let mut config: GuiConfigFile =
                toml::from_str(include_str!("../../config/gui.toml")).unwrap();
            let params = &mut config
                .section
                .iter_mut()
                .find(|s| s.name == "Debug")
                .unwrap()
                .param;
            let slider = params
                .iter_mut()
                .find(|p| p.id == "walk_ground_detail_height")
                .unwrap();
            slider.value = GuiParamValue::Float {
                value: 1.5,
                min: Some(0.0),
                max: Some(4.0),
            };
            slider.enabled_if = Some(GuiParamEnabledIf {
                param: "walk_ground_detail".into(),
                equals: GuiParamConditionValue::Bool(true),
            });
            let mut toggle = slider.clone();
            toggle.id = "walk_ground_detail".into();
            toggle.label = "Player: traverse small ground obstacles (A/B)".into();
            toggle.kind = GuiParamKind::Bool;
            toggle.value = GuiParamValue::Bool { value: enabled };
            toggle.enabled_if = None;
            params.push(toggle);
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("gui.toml");
            let old = toml::to_string_pretty(&config).unwrap();
            std::fs::write(&path, &old).unwrap();
            let loaded = GuiConfigLoader::load_from_path(&path);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), old);
            assert!(!loaded
                .section
                .iter()
                .flat_map(|s| &s.param)
                .any(|p| p.id == "walk_ground_detail"));
            let slider = loaded
                .section
                .iter()
                .flat_map(|s| &s.param)
                .find(|p| p.id == "walk_ground_detail_height")
                .unwrap();
            assert!(slider.enabled_if.is_none());
            assert!(matches!(
                slider.value,
                GuiParamValue::Float { value: 1.5, .. }
            ));
            GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
            let saved = std::fs::read_to_string(&path).unwrap();
            assert!(!saved.contains("\"walk_ground_detail\""));
            let reloaded = GuiConfigLoader::load_from_path(&path);
            let slider = reloaded
                .section
                .iter()
                .flat_map(|s| &s.param)
                .find(|p| p.id == "walk_ground_detail_height")
                .unwrap();
            assert!(slider.enabled_if.is_none());
            assert!(matches!(
                slider.value,
                GuiParamValue::Float { value: 1.5, .. }
            ));
        }
    }

    #[test]
    fn orbit_limit_is_added_to_old_saves_without_writing_and_round_trips() {
        use super::GuiParamValue;
        let mut config: GuiConfigFile =
            toml::from_str(include_str!("../../config/gui.toml")).unwrap();
        for section in &mut config.section {
            section
                .param
                .retain(|p| p.id != "camera_orbit_max_elevation");
        }
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("gui.toml");
        let old = toml::to_string_pretty(&config).unwrap();
        std::fs::write(&path, &old).unwrap();
        let mut loaded = GuiConfigLoader::load_from_path(&path);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), old);
        let param = loaded
            .section
            .iter_mut()
            .flat_map(|s| &mut s.param)
            .find(|p| p.id == "camera_orbit_max_elevation")
            .unwrap();
        if let GuiParamValue::Float { value, .. } = &mut param.value {
            assert_eq!(*value, 45.);
            *value = 60.;
        } else {
            panic!("orbit limit must be a saved float");
        }
        GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
        let reloaded = GuiConfigLoader::load_from_path(&path);
        let param = reloaded
            .section
            .iter()
            .flat_map(|s| &s.param)
            .find(|p| p.id == "camera_orbit_max_elevation")
            .unwrap();
        assert!(matches!(param.value, GuiParamValue::Float { value, .. } if value == 60.));
    }

    #[test]
    fn older_saves_gain_default_apple_size_without_rewriting_the_file() {
        let mut config: GuiConfigFile =
            toml::from_str(include_str!("../../config/gui.toml")).unwrap();
        for section in &mut config.section {
            section.param.retain(|p| p.id != "apple_size_scale");
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gui.toml");
        GuiConfigLoader::save_to_path(&config, &path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let loaded = GuiConfigLoader::load_from_path(&path);
        let size = loaded
            .section
            .iter()
            .flat_map(|s| &s.param)
            .find(|p| p.id == "apple_size_scale")
            .unwrap();
        assert!(matches!(
            size.value,
            super::GuiParamValue::Float { value: 1.0, .. }
        ));
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }

    #[test]
    fn retired_stem_modes_are_removed_without_writing_saves() {
        for old_mode in 0..=3 {
            let mut config: GuiConfigFile =
                toml::from_str(include_str!("../../config/gui.toml")).unwrap();
            let section = config
                .section
                .iter_mut()
                .find(|s| s.name == "Debug")
                .unwrap();
            let mut param = section.param[0].clone();
            param.id = "stem_band_mode".into();
            param.kind = super::GuiParamKind::Choice;
            param.value = super::GuiParamValue::Choice {
                value: old_mode,
                options: vec![
                    "analytic".into(),
                    "square".into(),
                    "taper".into(),
                    "ribbon".into(),
                ],
            };
            section.param.push(param);
            let mut retired_checkbox = section.param[0].clone();
            retired_checkbox.id = "grass_stem_rendering".into();
            retired_checkbox.kind = super::GuiParamKind::Bool;
            retired_checkbox.value = super::GuiParamValue::Bool {
                value: old_mode % 2 == 0,
            };
            section.param.push(retired_checkbox.clone());
            retired_checkbox.id = "flower_stem_stepped_width".into();
            section.param.push(retired_checkbox);
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("gui.toml");
            GuiConfigLoader::save_to_path(&config, &path).unwrap();
            let bytes = std::fs::read(&path).unwrap();
            let loaded = GuiConfigLoader::load_from_path(&path);
            assert!(!loaded
                .section
                .iter()
                .flat_map(|s| &s.param)
                .any(|p| matches!(
                    p.id.as_str(),
                    "stem_band_mode" | "grass_stem_rendering" | "flower_stem_stepped_width"
                )));
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
            GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
            let saved: GuiConfigFile =
                toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            assert!(!saved
                .section
                .iter()
                .flat_map(|s| &s.param)
                .any(|p| matches!(
                    p.id.as_str(),
                    "stem_band_mode" | "grass_stem_rendering" | "flower_stem_stepped_width"
                )));
        }
    }

    #[test]
    fn pavement_defaults_upgrade_old_configs_without_writing_or_losing_stone_preferences() {
        let mut config: GuiConfigFile =
            toml::from_str(include_str!("../../config/gui.toml")).unwrap();
        for section in &mut config.section {
            section.param.retain(|p| !p.id.starts_with("pavement_"));
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gui.toml");
        GuiConfigLoader::save_to_path(&config, &path).unwrap();
        let original = std::fs::read(&path).unwrap();
        let loaded = GuiConfigLoader::load_from_path(&path);
        let fields = crate::app::gui_config::GuiAdjustables::from_config(&loaded);
        assert_eq!(fields.pavement_kind.value, 0);
        assert_eq!(fields.pavement_width.value, 0.12);
        assert_eq!(fields.pavement_piece_length.value, 0.05);
        assert_eq!(fields.pavement_seed.value, 42);
        assert!(!fields.pavement_smooth_curves.value);
        assert_eq!(fields.pavement_smooth_scale.value, 2.);
        assert!(fields.pavement_retract_stroke.value);
        assert_eq!(fields.pavement_retract_scale.value, 2.);
        assert_eq!(
            fields.stone_seed.value,
            config
                .section
                .iter()
                .flat_map(|s| &s.param)
                .find(|p| p.id == "stone_seed")
                .unwrap()
                .value
                .get_uint()
                .unwrap()
                .0
        );
        assert_eq!(original, std::fs::read(&path).unwrap());
    }

    #[test]
    fn low_wall_controls_upgrade_absent_and_prototype_saves_without_changing_values_or_disk() {
        use super::GuiParamValue;
        for legacy_mode in [None, Some(false), Some(true)] {
            let mut config: GuiConfigFile =
                toml::from_str(include_str!("../../config/gui.toml")).unwrap();
            if let Some(value) = legacy_mode {
                let mut legacy = config
                    .section
                    .iter()
                    .flat_map(|s| &s.param)
                    .find(|p| p.id == "pavement_retract_stroke")
                    .unwrap()
                    .clone();
                legacy.id = "low_wall_enabled".into();
                legacy.value = GuiParamValue::Bool { value };
                config.section[0].param.push(legacy);
            }
            for section in &mut config.section {
                if legacy_mode.is_none() {
                    section.param.retain(|p| !p.id.starts_with("low_wall_"));
                }
                for param in &mut section.param {
                    match param.id.as_str() {
                        "low_wall_enabled" => {
                            param.label = "Low wall A/B prototype (off = original pavement)".into();
                            param.value = GuiParamValue::Bool {
                                value: legacy_mode.unwrap(),
                            };
                        }
                        "low_wall_kind" => {
                            if let GuiParamValue::Choice { value, .. } = &mut param.value {
                                *value = 2;
                            }
                        }
                        "low_wall_width" | "low_wall_height" => {
                            if let GuiParamValue::Float { value, .. } = &mut param.value {
                                *value = 0.1;
                            }
                        }
                        "pavement_width" => {
                            param.enabled_if = None; // pre-wall main saved document
                            if let GuiParamValue::Float { value, .. } = &mut param.value {
                                *value = 0.24;
                            }
                        }
                        _ => {}
                    }
                }
            }
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("gui.toml");
            GuiConfigLoader::save_to_path(&config, &path).unwrap();
            let before = std::fs::read(&path).unwrap();
            let loaded = GuiConfigLoader::load_from_path(&path);
            assert_eq!(
                std::fs::read(&path).unwrap(),
                before,
                "load must not rewrite saved settings"
            );
            let fields = crate::app::gui_config::GuiAdjustables::from_config(&loaded);
            assert_eq!(
                fields.low_wall_kind.value,
                if legacy_mode.is_some() { 2 } else { 0 }
            );
            assert_eq!(
                fields.low_wall_width.value,
                if legacy_mode.is_some() { 0.1 } else { 0.06 }
            );
            assert_eq!(
                fields.low_wall_height.value,
                if legacy_mode.is_some() { 0.1 } else { 0.12 }
            );
            assert_eq!(fields.pavement_width.value, 0.24);
            let params: Vec<_> = loaded.section.iter().flat_map(|s| &s.param).collect();
            assert!(params.iter().all(|p| p.id != "low_wall_enabled"));
            for id in [
                "pavement_kind",
                "pavement_width",
                "low_wall_kind",
                "low_wall_width",
                "low_wall_height",
            ] {
                assert!(params
                    .iter()
                    .find(|p| p.id == id)
                    .unwrap()
                    .enabled_if
                    .is_none());
            }
            GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
            let reloaded = GuiConfigLoader::load_from_path(&path);
            assert_eq!(
                toml::to_string(&loaded).unwrap(),
                toml::to_string(&reloaded).unwrap()
            );
        }
    }

    #[test]
    fn retired_stone_preview_controls_are_ignored_without_changing_placement() {
        for enabled in [false, true] {
            let mut config: GuiConfigFile =
                toml::from_str(include_str!("../../config/gui.toml")).unwrap();
            let debug = config
                .section
                .iter_mut()
                .find(|s| s.name == "Debug")
                .unwrap();
            let preview = debug
                .param
                .iter()
                .find(|p| p.id == "stone_place_slab")
                .unwrap()
                .clone();
            for id in [
                "stone_direct_triangles",
                "stone_preview_enabled",
                "stone_preview_focus",
                "stone_preview_lift",
                "stone_kind",
            ] {
                let mut legacy = preview.clone();
                legacy.id = id.into();
                legacy.value = super::GuiParamValue::Bool { value: enabled };
                debug.param.push(legacy);
            }
            for param in &mut debug.param {
                if matches!(
                    param.id.as_str(),
                    "stone_slab_thickness"
                        | "stone_slab_edge_cut"
                        | "stone_rock_height"
                        | "stone_rock_facets"
                ) {
                    let condition = param.enabled_if.as_mut().unwrap();
                    condition.param = "stone_kind".into();
                    condition.equals = super::GuiParamConditionValue::Integer(i64::from(
                        !param.id.starts_with("stone_slab"),
                    ));
                }
            }
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("gui.toml");
            GuiConfigLoader::save_to_path(&config, &path).unwrap();
            let bytes = std::fs::read(&path).unwrap();
            let loaded = GuiConfigLoader::load_from_path(&path);
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
            let params = loaded
                .section
                .iter()
                .flat_map(|s| &s.param)
                .collect::<Vec<_>>();
            assert!(!params.iter().any(|p| matches!(
                p.id.as_str(),
                "stone_direct_triangles"
                    | "stone_preview_enabled"
                    | "stone_preview_focus"
                    | "stone_preview_lift"
                    | "stone_kind"
            )));
            assert_eq!(
                params
                    .iter()
                    .find(|p| p.id == preview.id)
                    .unwrap()
                    .value
                    .get_bool(),
                preview.value.get_bool()
            );
            GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
            assert!(!std::fs::read_to_string(&path)
                .unwrap()
                .contains("stone_direct_triangles"));
        }
    }

    #[test]
    fn retired_apple_lighting_switches_preserve_fill_without_gating_or_rewriting() {
        use crate::app::gui_config_model::{GuiParamEnabledIf, GuiParamValue};
        for peel_flags in 0..8 {
            for directional in [false, true] {
                for bands in [false, true] {
                    let mut config: GuiConfigFile =
                        toml::from_str(include_str!("../../config/gui.toml")).unwrap();
                    let template = config
                        .section
                        .iter()
                        .flat_map(|s| &s.param)
                        .find(|p| p.id == "apple_cache_enabled")
                        .unwrap()
                        .clone();
                    for (id, value) in [
                        ("apple_directional_lighting", directional),
                        ("apple_light_bands", bands),
                        ("apple_color_patches", peel_flags & 1 != 0),
                        ("apple_color_stripes", peel_flags & 2 != 0),
                        ("apple_sun_gloss", peel_flags & 4 != 0),
                    ] {
                        let mut param = template.clone();
                        param.id = id.into();
                        param.value = GuiParamValue::Bool { value };
                        config.section[0].param.push(param);
                    }
                    let fill = config
                        .section
                        .iter_mut()
                        .flat_map(|s| &mut s.param)
                        .find(|p| p.id == "apple_environment_fill")
                        .unwrap();
                    fill.value = GuiParamValue::Float {
                        value: 0.73,
                        min: Some(0.0),
                        max: Some(1.5),
                    };
                    fill.enabled_if = Some(GuiParamEnabledIf {
                        param: "apple_directional_lighting".into(),
                        equals: super::GuiParamConditionValue::Bool(true),
                    });
                    let dir = tempfile::tempdir().unwrap();
                    let path = dir.path().join("gui.toml");
                    GuiConfigLoader::save_to_path(&config, &path).unwrap();
                    let bytes = std::fs::read(&path).unwrap();
                    let loaded = GuiConfigLoader::load_from_path(&path);
                    assert_eq!(std::fs::read(&path).unwrap(), bytes);
                    let fill = loaded
                        .section
                        .iter()
                        .flat_map(|s| &s.param)
                        .find(|p| p.id == "apple_environment_fill")
                        .unwrap();
                    assert_eq!(fill.value.get_float(), Some((0.73, Some(0.0), Some(1.5))));
                    assert!(fill.enabled_if.is_none());
                    GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
                    let saved = std::fs::read_to_string(&path).unwrap();
                    assert!(!saved.contains("apple_directional_lighting"));
                    assert!(!saved.contains("apple_light_bands"));
                    for id in [
                        "apple_color_patches",
                        "apple_color_stripes",
                        "apple_sun_gloss",
                    ] {
                        assert!(!saved.contains(id));
                    }
                }
            }
        }
    }

    #[test]
    fn old_saves_gain_grass_color_distribution_defaults_without_rewriting() {
        let ids = [
            "grass_color_patch_scale",
            "grass_color_light_bias",
            "grass_color_contrast",
            "grass_color_seed",
            "plant_wilt_strength",
            "plant_root_uptake_seconds",
            "plant_transpiration_seconds",
            "plant_wilt_start_hydration",
        ];
        let defaults: GuiConfigFile =
            toml::from_str(include_str!("../../config/gui.toml")).unwrap();
        let mut old = defaults.clone();
        for section in &mut old.section {
            section.param.retain(|p| !ids.contains(&p.id.as_str()));
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gui.toml");
        GuiConfigLoader::save_to_path(&old, &path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let loaded = GuiConfigLoader::load_from_path(&path);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        for id in ids {
            let expected = defaults
                .section
                .iter()
                .flat_map(|s| &s.param)
                .find(|p| p.id == id)
                .unwrap();
            let actual: Vec<_> = loaded
                .section
                .iter()
                .flat_map(|s| &s.param)
                .filter(|p| p.id == id)
                .collect();
            assert_eq!(actual.len(), 1);
            assert_eq!(
                toml::to_string(&actual[0].value).unwrap(),
                toml::to_string(&expected.value).unwrap()
            );
        }
    }

    #[test]
    fn retired_bend_height_power_is_ignored_and_not_saved() {
        for value in [0.25, 1.0, 1.55, 5.0] {
            let mut config: GuiConfigFile =
                toml::from_str(include_str!("../../config/gui.toml")).unwrap();
            let mut param = config
                .section
                .iter()
                .flat_map(|s| &s.param)
                .find(|p| p.id == "grass_natural_bend_max_voxels")
                .unwrap()
                .clone();
            param.id = "flora_bend_height_power".into();
            param.value = super::GuiParamValue::Float {
                value,
                min: Some(0.25),
                max: Some(5.0),
            };
            config.section[0].param.push(param);
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("gui.toml");
            GuiConfigLoader::save_to_path(&config, &path).unwrap();
            let bytes = std::fs::read(&path).unwrap();
            let loaded = GuiConfigLoader::load_from_path(&path);
            assert!(!loaded
                .section
                .iter()
                .flat_map(|s| &s.param)
                .any(|p| p.id == "flora_bend_height_power"));
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
            GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
            assert!(!std::fs::read_to_string(&path)
                .unwrap()
                .contains("flora_bend_height_power"));
        }
    }

    #[test]
    fn retired_natural_bend_minimum_is_ignored_and_not_saved() {
        let mut config: GuiConfigFile =
            toml::from_str(include_str!("../../config/gui.toml")).unwrap();
        let flora = config
            .section
            .iter_mut()
            .find(|s| s.name == "Flora")
            .unwrap();
        let maximum = flora
            .param
            .iter()
            .find(|p| p.id == "grass_natural_bend_max_voxels")
            .unwrap()
            .clone();
        let mut minimum = maximum.clone();
        minimum.id = "grass_natural_bend_min_voxels".into();
        minimum.value = super::GuiParamValue::Float {
            value: 4.0,
            min: Some(0.0),
            max: Some(4.0),
        };
        flora.param.push(minimum);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gui.toml");
        GuiConfigLoader::save_to_path(&config, &path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let loaded = GuiConfigLoader::load_from_path(&path);
        let params: Vec<_> = loaded.section.iter().flat_map(|s| &s.param).collect();
        assert!(!params
            .iter()
            .any(|p| p.id == "grass_natural_bend_min_voxels"));
        assert_eq!(
            params
                .iter()
                .find(|p| p.id == maximum.id)
                .unwrap()
                .value
                .get_float(),
            maximum.value.get_float()
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
        assert!(!std::fs::read_to_string(&path)
            .unwrap()
            .contains("grass_natural_bend_min_voxels"));
    }

    #[test]
    fn retired_flora_color_variation_is_removed_from_legacy_saves() {
        use super::RETIRED_FLORA_COLOR_PARAMS;
        use crate::app::gui_config_model::GuiParamValue;
        let mut config: GuiConfigFile =
            toml::from_str(include_str!("../../config/gui.toml")).unwrap();
        let original = config.clone();
        let template = config
            .section
            .iter()
            .flat_map(|s| &s.param)
            .find(|p| p.id == "flower_stem_tip_radius_ratio")
            .unwrap()
            .clone();
        let retired: Vec<_> = RETIRED_FLORA_COLOR_PARAMS
            .iter()
            .map(|id| {
                let mut param = template.clone();
                param.id = (*id).into();
                param.value = GuiParamValue::Float {
                    value: 0.8,
                    min: Some(0.),
                    max: Some(1.),
                };
                param
            })
            .collect();
        // Remove retired IDs even if an old save moved them to another section.
        config.section[0].param.extend(retired.clone());
        config
            .section
            .push(crate::app::gui_config_model::GuiSection {
                name: "FloraVariation".into(),
                param: retired,
            });
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("gui.toml");
        std::fs::write(&path, toml::to_string(&config).unwrap()).unwrap();
        let loaded = GuiConfigLoader::load_from_path(&path);
        assert_eq!(
            toml::to_string(&loaded).unwrap(),
            toml::to_string(&original).unwrap()
        );
        GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
        let saved = std::fs::read_to_string(&path).unwrap();
        assert!(!saved.contains("FloraVariation"));
        for id in RETIRED_FLORA_COLOR_PARAMS {
            assert!(!saved.contains(id));
        }
    }

    #[test]
    fn scene_pixel_dropdown_migrates_to_saved_integer_stride_without_writing() {
        use crate::app::gui_config_model::GuiParamValue;
        for (old, expected) in [(0, 1), (1, 2), (2, 4), (3, 8), (u32::MAX, 8)] {
            let mut config: GuiConfigFile =
                toml::from_str(include_str!("../../config/gui.toml")).unwrap();
            let param = config
                .section
                .iter_mut()
                .flat_map(|s| &mut s.param)
                .find(|p| p.id == "scene_pixel_ratio")
                .unwrap();
            param.kind = crate::app::gui_config_model::GuiParamKind::Choice;
            param.label = "Scene: final pixel resolution".into();
            param.value = GuiParamValue::Choice {
                value: old,
                options: ["1:1", "4:1", "16:1", "64:1"].map(String::from).to_vec(),
            };
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("gui.toml");
            GuiConfigLoader::save_to_path(&config, &path).unwrap();
            let before = std::fs::read(&path).unwrap();
            let loaded = GuiConfigLoader::load_from_path(&path);
            assert_eq!(std::fs::read(&path).unwrap(), before);
            let migrated = loaded
                .section
                .iter()
                .flat_map(|s| &s.param)
                .find(|p| p.id == "scene_pixel_ratio")
                .unwrap();
            assert!(matches!(
                migrated.kind,
                crate::app::gui_config_model::GuiParamKind::Uint
            ));
            assert!(
                matches!(migrated.value, GuiParamValue::Uint { value, min: Some(1), max: Some(8) } if value == expected)
            );
            assert!(migrated.enabled_if.is_none());
            GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
            let reloaded = GuiConfigLoader::load_from_path(&path);
            assert_eq!(
                crate::app::GuiAdjustables::from_config(&reloaded)
                    .scene_pixel_ratio
                    .value,
                expected
            );
        }
    }

    #[test]
    fn scene_pixel_slider_clamps_invalid_values_and_preserves_valid_sizes() {
        use crate::app::gui_config_model::GuiParamValue;
        for saved in [0, 1, 2, 3, 4, 5, 6, 7, 8, u32::MAX] {
            let mut config: GuiConfigFile =
                toml::from_str(include_str!("../../config/gui.toml")).unwrap();
            let param = config
                .section
                .iter_mut()
                .flat_map(|s| &mut s.param)
                .find(|p| p.id == "scene_pixel_ratio")
                .unwrap();
            param.value = GuiParamValue::Uint {
                value: saved,
                min: None,
                max: None,
            };
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("gui.toml");
            GuiConfigLoader::save_to_path(&config, &path).unwrap();
            let before = std::fs::read(&path).unwrap();
            let loaded = GuiConfigLoader::load_from_path(&path);
            assert_eq!(std::fs::read(&path).unwrap(), before);
            assert_eq!(
                crate::app::GuiAdjustables::from_config(&loaded)
                    .scene_pixel_ratio
                    .value,
                saved.clamp(1, 8)
            );
        }
    }

    #[test]
    fn older_saves_get_declared_supersampling_defaults_and_saved_values_survive_reload() {
        use crate::app::gui_config_model::GuiParamValue;
        let defaults: GuiConfigFile =
            toml::from_str(include_str!("../../config/gui.toml")).unwrap();
        let defaults = crate::app::GuiAdjustables::from_config(&defaults);
        for saved in [None, Some(false), Some(true)] {
            let mut config: GuiConfigFile =
                toml::from_str(include_str!("../../config/gui.toml")).unwrap();
            let debug = config
                .section
                .iter_mut()
                .find(|s| s.name == "Debug")
                .unwrap();
            debug.param.retain(|p| {
                p.id != "scene_pixel_ratio"
                    && p.id != "scene_supersampling_quality"
                    && p.id != "scene_pixel_resolve_mode"
            });
            if let Some(value) = saved {
                let param = debug
                    .param
                    .iter_mut()
                    .find(|p| p.id == "scene_supersampling_enabled")
                    .unwrap();
                param.label = "Scene: 2x supersampling (A/B)".into();
                param.value = GuiParamValue::Bool { value };
            } else {
                debug
                    .param
                    .retain(|p| p.id != "scene_supersampling_enabled");
            }
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("gui.toml");
            GuiConfigLoader::save_to_path(&config, &path).unwrap();
            let before = std::fs::read(&path).unwrap();
            let loaded = GuiConfigLoader::load_from_path(&path);
            assert_eq!(
                std::fs::read(&path).unwrap(),
                before,
                "loading must not save"
            );
            let gui = crate::app::GuiAdjustables::from_config(&loaded);
            assert_eq!(gui.scene_pixel_ratio.value, 5);
            assert_eq!(
                gui.scene_supersampling_quality.value,
                defaults.scene_supersampling_quality.value
            );
            assert_eq!(
                gui.scene_pixel_resolve_mode.value,
                defaults.scene_pixel_resolve_mode.value
            );
            assert!(loaded
                .section
                .iter()
                .flat_map(|s| &s.param)
                .find(|p| p.id == "scene_supersampling_enabled")
                .unwrap()
                .label
                .contains("pixelization"));
            assert_eq!(
                gui.scene_supersampling_enabled.value,
                saved.unwrap_or(defaults.scene_supersampling_enabled.value)
            );
            GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
            let reloaded = GuiConfigLoader::load_from_path(&path);
            assert_eq!(
                crate::app::GuiAdjustables::from_config(&reloaded)
                    .scene_supersampling_enabled
                    .value,
                saved.unwrap_or(defaults.scene_supersampling_enabled.value)
            );
            // Existing selections must round-trip, not be replaced by today's defaults.
            for value in [0, 1] {
                let mut edited = reloaded.clone();
                for param in edited.section.iter_mut().flat_map(|s| &mut s.param) {
                    if matches!(
                        param.id.as_str(),
                        "scene_supersampling_quality" | "scene_pixel_resolve_mode"
                    ) {
                        if let GuiParamValue::Choice {
                            value: selected, ..
                        } = &mut param.value
                        {
                            *selected = value;
                        }
                    }
                }
                GuiConfigLoader::save_to_path(&edited, &path).unwrap();
                let loaded = GuiConfigLoader::load_from_path(&path);
                let gui = crate::app::GuiAdjustables::from_config(&loaded);
                assert_eq!(gui.scene_supersampling_quality.value, value);
                assert_eq!(gui.scene_pixel_resolve_mode.value, value);
            }
            assert_eq!(
                reloaded
                    .section
                    .iter()
                    .flat_map(|s| &s.param)
                    .filter(|p| p.id == "scene_supersampling_enabled")
                    .count(),
                1
            );
        }
    }

    #[test]
    fn retired_dither_values_are_removed_without_saving_or_losing_other_controls() {
        use crate::app::gui_config_model::{GuiParamValue, GuiSection};
        for value in [0.0, 1.0, 4.0] {
            for keep_other in [false, true] {
                let mut config: GuiConfigFile =
                    toml::from_str(include_str!("../../config/gui.toml")).unwrap();
                let other = config
                    .section
                    .iter()
                    .flat_map(|s| &s.param)
                    .find(|p| p.id == "leaf_global_offset_scale")
                    .unwrap()
                    .clone();
                let mut retired = other.clone();
                retired.id = "dither_strength_lsb".into();
                retired.value = GuiParamValue::Float {
                    value,
                    min: Some(0.0),
                    max: Some(4.0),
                };
                let mut section = GuiSection {
                    name: "Post Processing".into(),
                    param: vec![retired],
                };
                if keep_other {
                    for s in &mut config.section {
                        s.param.retain(|p| p.id != other.id);
                    }
                    section.param.push(other.clone());
                }
                config.section.push(section);
                let dir = tempfile::tempdir().unwrap();
                let path = dir.path().join("gui.toml");
                GuiConfigLoader::save_to_path(&config, &path).unwrap();
                let bytes = std::fs::read(&path).unwrap();
                let loaded = GuiConfigLoader::load_from_path(&path);
                assert_eq!(std::fs::read(&path).unwrap(), bytes);
                assert!(!loaded
                    .section
                    .iter()
                    .flat_map(|s| &s.param)
                    .any(|p| p.id == "dither_strength_lsb"));
                assert_eq!(
                    loaded.section.iter().any(|s| s.name == "Post Processing"),
                    keep_other
                );
                assert_eq!(
                    toml::to_string(
                        loaded
                            .section
                            .iter()
                            .flat_map(|s| &s.param)
                            .find(|p| p.id == other.id)
                            .unwrap()
                    )
                    .unwrap(),
                    toml::to_string(&other).unwrap()
                );
                GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
                assert_eq!(
                    toml::to_string(&loaded).unwrap(),
                    toml::to_string(&GuiConfigLoader::load_from_path(&path)).unwrap()
                );
            }
        }
    }

    #[test]
    fn retired_stem_block_switch_preserves_height_and_enables_slider() {
        use crate::app::gui_config_model::{GuiParamKind, GuiParamValue};
        let mut config: GuiConfigFile =
            toml::from_str(include_str!("../../config/gui.toml")).unwrap();
        let mut retired = config
            .section
            .iter()
            .flat_map(|s| &s.param)
            .find(|p| matches!(p.kind, GuiParamKind::Bool))
            .unwrap()
            .clone();
        retired.id = "flower_stem_fixed_cell_height".into();
        retired.value = GuiParamValue::Bool { value: false };
        config.section[0].param.push(retired);
        let height = config
            .section
            .iter_mut()
            .flat_map(|s| &mut s.param)
            .find(|p| p.id == "flower_stem_cell_height_voxels")
            .unwrap();
        height.value = GuiParamValue::Float {
            value: 1.25,
            min: Some(0.1),
            max: Some(4.),
        };
        height.enabled_if =
            Some(toml::from_str("param = 'flower_stem_fixed_cell_height'\nequals = true").unwrap());
        GuiConfigLoader::migrate_flower_stem_selector(&mut config);
        assert!(!config
            .section
            .iter()
            .flat_map(|s| &s.param)
            .any(|p| p.id == "flower_stem_fixed_cell_height"));
        let height = config
            .section
            .iter()
            .flat_map(|s| &s.param)
            .find(|p| p.id == "flower_stem_cell_height_voxels")
            .unwrap();
        assert!(height.enabled_if.is_none());
        assert!(matches!(height.value, GuiParamValue::Float { value, .. } if value == 1.25));
    }

    #[test]
    fn old_stem_radius_range_expands_without_changing_saved_values() {
        use crate::app::gui_config_model::GuiParamValue;
        for value in [0.7, 2.0, 4.0, 10.0] {
            let mut config: GuiConfigFile =
                toml::from_str(include_str!("../../config/gui.toml")).unwrap();
            let param = config
                .section
                .iter_mut()
                .flat_map(|s| &mut s.param)
                .find(|p| p.id == "flower_stem_radius_scale")
                .unwrap();
            param.value = GuiParamValue::Float {
                value,
                min: Some(0.25),
                max: Some(2.0),
            };
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("gui.toml");
            GuiConfigLoader::save_to_path(&config, &path).unwrap();
            let bytes = std::fs::read(&path).unwrap();
            let loaded = GuiConfigLoader::load_from_path(&path);
            assert_eq!(bytes, std::fs::read(&path).unwrap());
            let gui = crate::app::GuiAdjustables::from_config(&loaded);
            assert_eq!(gui.flower_stem_radius_scale.value, value);
            assert!(matches!(
                loaded
                    .section
                    .iter()
                    .flat_map(|s| &s.param)
                    .find(|p| p.id == "flower_stem_radius_scale")
                    .unwrap()
                    .value,
                GuiParamValue::Float {
                    min: Some(0.25),
                    max: Some(10.0),
                    ..
                }
            ));
            GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
            assert_eq!(
                toml::to_string(&loaded).unwrap(),
                toml::to_string(&GuiConfigLoader::load_from_path(&path)).unwrap()
            );
        }
    }

    #[test]
    fn stem_saves_retire_angular_controls_and_preserve_model_settings() {
        use crate::app::gui_config_model::{
            GuiParamConditionValue, GuiParamEnabledIf, GuiParamValue as Value,
        };
        for model in [false, true] {
            for has_resolution in [false, true] {
                for pixelized in [false, true] {
                    let mut config: GuiConfigFile =
                        toml::from_str(include_str!("../../config/gui.toml")).unwrap();
                    let debug = config
                        .section
                        .iter_mut()
                        .find(|s| s.name == "Debug")
                        .unwrap();
                    let mut retired = debug
                        .param
                        .iter()
                        .find(|p| p.id == "flower_stem_test_branches")
                        .unwrap()
                        .clone();
                    retired.id = "flower_stem_pixelized".into();
                    retired.value = Value::Bool { value: pixelized };
                    debug.param.push(retired);
                    let mut flag = debug.param[0].clone();
                    flag.id = "flower_stem_model_sampling".into();
                    flag.value = Value::Bool { value: model };
                    debug.param.push(flag);
                    let resolution = debug
                        .param
                        .iter_mut()
                        .find(|p| p.id == "flower_stem_model_resolution")
                        .unwrap();
                    let default_resolution = match resolution.value {
                        Value::Uint { value, .. } => value,
                        _ => unreachable!("model resolution is an integer"),
                    };
                    resolution.value = Value::Uint {
                        value: 192,
                        min: Some(32),
                        max: Some(512),
                    };
                    resolution.enabled_if = Some(GuiParamEnabledIf {
                        param: "flower_stem_model_sampling".into(),
                        equals: GuiParamConditionValue::Bool(true),
                    });
                    let mut direction = resolution.clone();
                    direction.id = "flower_stem_direction_resolution".into();
                    debug.param.push(direction);
                    if !has_resolution {
                        debug
                            .param
                            .retain(|p| p.id != "flower_stem_model_resolution");
                    }
                    let file = std::env::temp_dir().join(format!(
                        "re-flora-model-only-{}-{model}-{has_resolution}-{pixelized}.toml",
                        std::process::id()
                    ));
                    GuiConfigLoader::save_to_path(&config, &file).unwrap();
                    let before = std::fs::read(&file).unwrap();
                    let loaded = GuiConfigLoader::load_from_path(&file);
                    assert_eq!(before, std::fs::read(&file).unwrap());
                    let params = loaded
                        .section
                        .iter()
                        .flat_map(|s| &s.param)
                        .collect::<Vec<_>>();
                    assert!(!params.iter().any(|p| matches!(
                        p.id.as_str(),
                        "flower_stem_model_sampling" | "flower_stem_direction_resolution"
                    )));
                    assert!(!params.iter().any(|p| p.id == "flower_stem_pixelized"));
                    let resolution = params
                        .iter()
                        .find(|p| p.id == "flower_stem_model_resolution")
                        .unwrap();
                    assert!(
                        matches!(resolution.value, Value::Uint { value, .. } if value == if has_resolution { 192 } else { default_resolution })
                    );
                    assert!(resolution.enabled_if.is_none());
                    GuiConfigLoader::save_to_path(&loaded, &file).unwrap();
                    assert_eq!(
                        toml::to_string(&loaded).unwrap(),
                        toml::to_string(&GuiConfigLoader::load_from_path(&file)).unwrap()
                    );
                    std::fs::remove_file(file).unwrap();
                }
            }
        }
    }

    #[test]
    fn tree_kick_controls_migrate_without_rewriting_old_saves_and_keep_tuning() {
        use crate::app::gui_config_model::GuiParamValue;
        let ids = [
            "tree_kick_strength",
            "tree_kick_frequency",
            "tree_kick_damping",
            "tree_kick_reach",
            "tree_kick_cooldown",
            "tree_kick_propagation_speed",
            "tree_kick_fruit_chance_scale",
            "tree_kick_leaf_count",
            "tree_kick_rustle_volume_db",
            "tree_kick_rustle_duration",
            "tree_kick_rustle_body",
            "tree_kick_rustle_brightness",
            "tree_kick_rustle_crackle",
        ];
        let mut config: GuiConfigFile =
            toml::from_str(include_str!("../../config/gui.toml")).unwrap();
        for s in &mut config.section {
            s.param.retain(|p| !ids.contains(&p.id.as_str()));
        }
        let file =
            std::env::temp_dir().join(format!("re-flora-tree-kick-{}.toml", std::process::id()));
        GuiConfigLoader::save_to_path(&config, &file).unwrap();
        let before = std::fs::read(&file).unwrap();
        let mut loaded = GuiConfigLoader::load_from_path(&file);
        assert_eq!(before, std::fs::read(&file).unwrap());
        for id in ids {
            let p = loaded
                .section
                .iter_mut()
                .flat_map(|s| &mut s.param)
                .find(|p| p.id == id)
                .unwrap();
            match &mut p.value {
                GuiParamValue::Float { value, min, max } => {
                    *value = (min.unwrap() + max.unwrap()) * 0.5
                }
                GuiParamValue::Int { value, min, max } => {
                    *value = (min.unwrap() + max.unwrap()) / 2
                }
                _ => panic!("kick setting is not numeric"),
            }
        }
        GuiConfigLoader::save_to_path(&loaded, &file).unwrap();
        assert_eq!(
            toml::to_string(&loaded).unwrap(),
            toml::to_string(&GuiConfigLoader::load_from_path(&file)).unwrap()
        );
        std::fs::remove_file(file).unwrap();
    }

    #[test]
    fn old_kick_fruit_quota_migrates_to_chance_scale_without_rewriting_saved_tuning() {
        use crate::app::gui_config_model::GuiParamValue;
        let mut config: GuiConfigFile =
            toml::from_str(include_str!("../../config/gui.toml")).unwrap();
        let param = config
            .section
            .iter_mut()
            .flat_map(|s| &mut s.param)
            .find(|p| p.id == "tree_kick_fruit_chance_scale")
            .unwrap();
        param.id = "tree_kick_fruit_fraction".into();
        param.label = "Tree kick fruit release fraction".into();
        if let GuiParamValue::Float { value, .. } = &mut param.value {
            *value = 0.63;
        }
        let file =
            std::env::temp_dir().join(format!("re-flora-fruit-chance-{}.toml", std::process::id()));
        GuiConfigLoader::save_to_path(&config, &file).unwrap();
        let before = std::fs::read(&file).unwrap();
        let loaded = GuiConfigLoader::load_from_path(&file);
        assert_eq!(before, std::fs::read(&file).unwrap());
        let params: Vec<_> = loaded.section.iter().flat_map(|s| &s.param).collect();
        assert!(!params.iter().any(|p| p.id == "tree_kick_fruit_fraction"));
        let new = params
            .iter()
            .find(|p| p.id == "tree_kick_fruit_chance_scale")
            .unwrap();
        assert!(matches!(new.value, GuiParamValue::Float { value, .. } if value == 0.63));
        GuiConfigLoader::save_to_path(&loaded, &file).unwrap();
        assert_eq!(
            toml::to_string(&loaded).unwrap(),
            toml::to_string(&GuiConfigLoader::load_from_path(&file)).unwrap()
        );
        std::fs::remove_file(file).unwrap();
    }

    #[test]
    fn stem_effect_switches_retire_with_legacy_metadata_and_round_trip() {
        use crate::app::gui_config_model::GuiParamValue;
        for pixelized in [false, true] {
            for surface_cells in [false, true] {
                let mut config: GuiConfigFile =
                    toml::from_str(include_str!("../../config/gui.toml")).unwrap();
                let debug = config
                    .section
                    .iter_mut()
                    .find(|s| s.name == "Debug")
                    .unwrap();
                for (id, value) in [
                    ("flower_stem_pixelized", pixelized),
                    ("flower_stem_surface_cells", surface_cells),
                ] {
                    let mut retired = debug
                        .param
                        .iter()
                        .find(|p| p.id == "flower_stem_test_branches")
                        .unwrap()
                        .clone();
                    retired.id = id.into();
                    retired.value = GuiParamValue::Bool { value };
                    debug.param.push(retired);
                }
                let mut retired = debug.param[0].clone();
                retired.id = "flower_stem_sampling".into();
                retired.value = GuiParamValue::Choice {
                    value: 2,
                    options: vec!["original".into(), "direction".into(), "surface".into()],
                };
                debug.param.push(retired);
                let file = std::env::temp_dir().join(format!(
                    "re-flora-stem-effects-{}-{pixelized}-{surface_cells}.toml",
                    std::process::id()
                ));
                GuiConfigLoader::save_to_path(&config, &file).unwrap();
                let loaded = GuiConfigLoader::load_from_path(&file);
                for id in ["flower_stem_pixelized", "flower_stem_surface_cells"] {
                    assert!(!loaded
                        .section
                        .iter()
                        .flat_map(|s| &s.param)
                        .any(|p| p.id == id));
                }
                GuiConfigLoader::save_to_path(&loaded, &file).unwrap();
                assert_eq!(
                    toml::to_string(&loaded).unwrap(),
                    toml::to_string(&GuiConfigLoader::load_from_path(&file)).unwrap()
                );
                std::fs::remove_file(file).unwrap();
            }
        }
    }

    #[test]
    fn flower_stem_selector_migrates_old_enablement_and_retires_controls_without_saving() {
        use crate::app::gui_config_model::{
            GuiParamConditionValue, GuiParamEnabledIf, GuiParamValue as Value,
        };
        for enabled in [false, true] {
            for mode in 0..=2 {
                let mut config: GuiConfigFile =
                    toml::from_str(include_str!("../../config/gui.toml")).unwrap();
                let debug = config
                    .section
                    .iter_mut()
                    .find(|s| s.name == "Debug")
                    .unwrap();
                let mut flag = debug
                    .param
                    .iter()
                    .find(|p| p.id == "flower_stem_test_branches")
                    .unwrap()
                    .clone();
                flag.id = "flower_stem_experiment".into();
                flag.value = Value::Bool { value: enabled };
                debug.param.push(flag);
                let mut material = debug
                    .param
                    .iter()
                    .find(|p| p.id == "flower_stem_radius_scale")
                    .unwrap()
                    .clone();
                material.id = "flower_stem_surface_cell_scale".into();
                let mut geometry = material.clone();
                geometry.id = "flower_stem_geometry_cell_scale".into();
                geometry.value = Value::Float {
                    value: 4.,
                    min: Some(0.25),
                    max: Some(4.),
                };
                debug.param.push(geometry);
                let mut blocks = debug
                    .param
                    .iter()
                    .find(|p| p.id == "flower_stem_test_branches")
                    .unwrap()
                    .clone();
                blocks.id = "flower_stem_surface_geometry".into();
                blocks.value = Value::Bool { value: true };
                debug.param.push(blocks);
                debug.param.push(material);
                debug.param.retain(|p| {
                    !matches!(
                        p.id.as_str(),
                        "flower_stem_pixelized" | "flower_stem_surface_cells"
                    )
                });
                let mut selector = debug.param[0].clone();
                selector.id = "flower_stem_sampling".into();
                selector.enabled_if = Some(GuiParamEnabledIf {
                    param: "flower_stem_experiment".into(),
                    equals: GuiParamConditionValue::Bool(true),
                });
                selector.value = Value::Choice {
                    value: mode,
                    options: vec![
                        "Continuous silhouette (reference)".into(),
                        "World-direction".into(),
                        "Surface-attached".into(),
                    ],
                };
                debug.param.push(selector);
                for id in [
                    "flower_stem_object_sampling",
                    "flower_stem_object_resolution",
                    "flower_stem_freeze_motion",
                ] {
                    let mut retired = debug.param[0].clone();
                    retired.id = id.into();
                    debug.param.push(retired);
                }
                let radius = debug
                    .param
                    .iter_mut()
                    .find(|p| p.id == "flower_stem_radius_scale")
                    .unwrap();
                radius.enabled_if = selector_condition();
                if let Value::Float { value, .. } = &mut radius.value {
                    *value = 1.27;
                }
                if let Value::Uint { value, .. } = &mut debug
                    .param
                    .iter_mut()
                    .find(|p| p.id == "flower_stem_model_resolution")
                    .unwrap()
                    .value
                {
                    *value = 191;
                }
                let file = std::env::temp_dir().join(format!(
                    "re-flora-stem-selector-{}-{enabled}-{mode}.toml",
                    std::process::id()
                ));
                GuiConfigLoader::save_to_path(&config, &file).unwrap();
                let before = std::fs::read(&file).unwrap();
                let loaded = GuiConfigLoader::load_from_path(&file);
                assert_eq!(std::fs::read(&file).unwrap(), before);
                let params = loaded
                    .section
                    .iter()
                    .flat_map(|s| &s.param)
                    .collect::<Vec<_>>();
                assert!(!params.iter().any(|p| matches!(
                    p.id.as_str(),
                    "flower_stem_experiment"
                        | "flower_stem_surface_cell_scale"
                        | "flower_stem_surface_geometry"
                        | "flower_stem_geometry_cell_scale"
                )));
                for id in [
                    "flower_stem_sampling",
                    "flower_stem_object_sampling",
                    "flower_stem_object_resolution",
                    "flower_stem_freeze_motion",
                ] {
                    assert!(!params.iter().any(|p| p.id == id));
                }
                for id in ["flower_stem_pixelized", "flower_stem_surface_cells"] {
                    assert!(!params.iter().any(|p| p.id == id));
                }
                assert!(
                    matches!(params.iter().find(|p| p.id == "flower_stem_radius_scale").unwrap().value, Value::Float { value, .. } if value == 1.27)
                );
                assert!(matches!(
                    params
                        .iter()
                        .find(|p| p.id == "flower_stem_model_resolution")
                        .unwrap()
                        .value,
                    Value::Uint { value: 191, .. }
                ));
                GuiConfigLoader::save_to_path(&loaded, &file).unwrap();
                let once = toml::to_string(&loaded).unwrap();
                assert_eq!(
                    once,
                    toml::to_string(&GuiConfigLoader::load_from_path(&file)).unwrap()
                );
                std::fs::remove_file(file).unwrap();
            }
        }
        fn selector_condition() -> Option<GuiParamEnabledIf> {
            Some(GuiParamEnabledIf {
                param: "flower_stem_experiment".into(),
                equals: GuiParamConditionValue::Bool(true),
            })
        }
    }

    #[test]
    fn retired_leaf_lifecycle_controls_are_removed_without_implicit_save() {
        use crate::app::gui_config_model::GuiParamValue;
        let mut config: GuiConfigFile =
            toml::from_str(include_str!("../../config/gui.toml")).unwrap();
        let mut old = config
            .section
            .iter()
            .flat_map(|s| &s.param)
            .find(|p| p.id == "leaf_global_offset_scale")
            .unwrap()
            .clone();
        let retired = [
            "real_leaf_lifecycle",
            "leaf_connection_strength",
            "leaf_connection_half_life",
            "leaf_regrowth_delay",
            "leaf_regrowth_duration",
        ];
        for id in retired {
            old.id = id.into();
            old.value = GuiParamValue::Float {
                value: 0.7,
                min: None,
                max: None,
            };
            config.section[0].param.push(old.clone());
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gui.toml");
        GuiConfigLoader::save_to_path(&config, &path).unwrap();
        let original = std::fs::read(&path).unwrap();
        let loaded = GuiConfigLoader::load_from_path(&path);
        assert_eq!(
            original,
            std::fs::read(&path).unwrap(),
            "loading must not rewrite authored settings"
        );
        assert!(loaded
            .section
            .iter()
            .flat_map(|s| &s.param)
            .all(|p| !retired.contains(&p.id.as_str())));
        let gui = crate::app::GuiAdjustables::from_config(&loaded);
        let defaults: GuiConfigFile =
            toml::from_str(include_str!("../../config/gui.toml")).unwrap();
        assert_eq!(
            gui.leaf_global_offset_scale.value,
            crate::app::GuiAdjustables::from_config(&defaults)
                .leaf_global_offset_scale
                .value
        );
        GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
        assert_eq!(
            toml::to_string(&loaded).unwrap(),
            toml::to_string(&GuiConfigLoader::load_from_path(&path)).unwrap()
        );
    }

    #[test]
    fn retired_tree_displays_preserve_wind_without_reviving_lifecycle() {
        use crate::app::gui_config_model::{
            GuiParamConditionValue, GuiParamEnabledIf, GuiParamValue,
        };
        for (wind, canonical_wind) in [(false, None), (true, None), (true, Some(false))] {
            let mut config: GuiConfigFile =
                toml::from_str(include_str!("../../config/gui.toml")).unwrap();
            let mut old_wind = config
                .section
                .iter()
                .flat_map(|s| &s.param)
                .find(|p| p.id == "raster_tree_wind")
                .unwrap()
                .clone();
            old_wind.id = "tree_wind".into();
            old_wind.value = GuiParamValue::Bool { value: wind };
            for section in &mut config.section {
                section.param.retain(|p| {
                    !matches!(p.id.as_str(), "falling_leaf_mesh")
                        && (p.id != "raster_tree_wind" || canonical_wind.is_some())
                });
                for p in &mut section.param {
                    match p.id.as_str() {
                        "raster_tree_wind" => {
                            p.value = GuiParamValue::Bool {
                                value: canonical_wind.unwrap(),
                            }
                        }
                        "falling_leaf_size_scale" => {
                            if let GuiParamValue::Float { value, .. } = &mut p.value {
                                *value = 2.0;
                            }
                            p.label = "Tree + Fallen experimental scale".into();
                            p.enabled_if = None;
                        }
                        "falling_leaf_pixel_resolution" => {
                            if let GuiParamValue::Uint { value, .. } = &mut p.value {
                                *value = 32;
                            }
                            p.label = "Tree + Fallen experimental pixels".into();
                            p.enabled_if = None;
                        }
                        _ => {}
                    }
                }
            }
            let debug = config
                .section
                .iter_mut()
                .find(|s| s.name == "Debug")
                .unwrap();
            debug.param.push(old_wind.clone());
            let mut retired_lifecycle = old_wind.clone();
            retired_lifecycle.id = "real_leaf_lifecycle".into();
            retired_lifecycle.value = GuiParamValue::Bool { value: wind };
            debug.param.push(retired_lifecycle);
            for id in ["tree_pixelized", "attached_leaf_rotation"] {
                let mut retired = old_wind.clone();
                retired.id = id.into();
                debug.param.push(retired);
            }
            let mut size = debug
                .param
                .iter()
                .find(|p| p.id == "apple_pixel_resolution")
                .unwrap()
                .clone();
            size.id = "tree_pixel_size".into();
            size.value = GuiParamValue::Uint {
                value: 7,
                min: Some(1),
                max: Some(16),
            };
            size.enabled_if = Some(GuiParamEnabledIf {
                param: "tree_pixelized".into(),
                equals: GuiParamConditionValue::Bool(true),
            });
            debug.param.push(size);
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("gui.toml");
            GuiConfigLoader::save_to_path(&config, &path).unwrap();
            let bytes = std::fs::read(&path).unwrap();
            let loaded = GuiConfigLoader::load_from_path(&path);
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
            assert!(loaded
                .section
                .iter()
                .flat_map(|s| &s.param)
                .all(|p| !matches!(
                    p.id.as_str(),
                    "tree_wind" | "tree_pixelized" | "tree_pixel_size" | "attached_leaf_rotation"
                )));
            let gui = crate::app::GuiAdjustables::from_config(&loaded);
            assert_eq!(gui.raster_tree_wind.value, canonical_wind.unwrap_or(wind));
            assert!(loaded
                .section
                .iter()
                .flat_map(|s| &s.param)
                .all(|p| p.id != "real_leaf_lifecycle"));
            assert!(loaded
                .section
                .iter()
                .flat_map(|s| &s.param)
                .all(|p| !matches!(
                    p.id.as_str(),
                    "falling_leaf_mesh"
                        | "falling_leaf_size_scale"
                        | "falling_leaf_pixel_resolution"
                )));
            GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
            assert_eq!(
                toml::to_string(&loaded).unwrap(),
                toml::to_string(&GuiConfigLoader::load_from_path(&path)).unwrap()
            );
        }
    }

    #[test]
    fn model_cache_controls_migrate_and_save_independently_without_reviving_leaf_models() {
        use crate::app::gui_config_model::GuiParamValue;
        let mut config: GuiConfigFile =
            toml::from_str(include_str!("../../config/gui.toml")).unwrap();
        for p in config.section.iter_mut().flat_map(|s| &mut s.param) {
            match p.id.as_str() {
                "apple_cache_enabled" | "model_flower_cache_enabled" => {
                    p.value = GuiParamValue::Bool { value: false }
                }
                "butterfly_cache_enabled" => p.value = GuiParamValue::Bool { value: true },
                "apple_pixel_resolution" => {
                    if let GuiParamValue::Uint { value, .. } = &mut p.value {
                        *value = 8;
                    }
                }
                "butterfly_pixel_resolution" => {
                    if let GuiParamValue::Uint { value, .. } = &mut p.value {
                        *value = 24;
                    }
                }
                "model_flower_pixel_resolution" => {
                    if let GuiParamValue::Uint { value, .. } = &mut p.value {
                        *value = 64;
                    }
                }
                _ => {}
            }
        }
        let mut legacy = config
            .section
            .iter()
            .flat_map(|s| &s.param)
            .find(|p| p.id == "apple_cache_enabled")
            .unwrap()
            .clone();
        for id in [
            "falling_leaf_mesh",
            "falling_leaf_size_scale",
            "falling_leaf_pixel_resolution",
        ] {
            legacy.id = id.into();
            config.section[0].param.push(legacy.clone());
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gui.toml");
        GuiConfigLoader::save_to_path(&config, &path).unwrap();
        let before = std::fs::read(&path).unwrap();
        let loaded = GuiConfigLoader::load_from_path(&path);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let gui = crate::app::GuiAdjustables::from_config(&loaded);
        assert!(!gui.apple_cache_enabled.value);
        assert!(gui.butterfly_cache_enabled.value);
        assert!(!gui.model_flower_cache_enabled.value);
        assert_eq!(gui.apple_pixel_resolution.value, 8);
        assert_eq!(gui.butterfly_pixel_resolution.value, 24);
        assert_eq!(gui.model_flower_pixel_resolution.value, 32);
        assert!(loaded
            .section
            .iter()
            .flat_map(|s| &s.param)
            .all(|p| !p.id.starts_with("falling_leaf_")));
        GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
        let reloaded = GuiConfigLoader::load_from_path(&path);
        assert_eq!(
            toml::to_string(&loaded).unwrap(),
            toml::to_string(&reloaded).unwrap()
        );
    }

    #[test]
    fn terrain_material_migration_preserves_authored_settings_and_adds_missing_controls() {
        use crate::app::gui_config_model::GuiParamValue;
        for partial in [false, true] {
            let mut config: GuiConfigFile =
                toml::from_str(include_str!("../../config/gui.toml")).unwrap();
            let section = config
                .section
                .iter_mut()
                .find(|s| s.name == "Terrain Material")
                .unwrap();
            if partial {
                section.param.retain(|p| p.id == "terrain_soil_strength");
                section.param[0].value = GuiParamValue::Float {
                    value: 0.6,
                    min: Some(0.0),
                    max: Some(0.75),
                };
            } else {
                config.section.retain(|s| s.name != "Terrain Material");
            }
            let before = config.clone();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("gui.toml");
            GuiConfigLoader::save_to_path(&config, &path).unwrap();
            let bytes = std::fs::read(&path).unwrap();
            let loaded = GuiConfigLoader::load_from_path(&path);
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
            for param in before.section.iter().flat_map(|s| &s.param) {
                let actual = loaded
                    .section
                    .iter()
                    .flat_map(|s| &s.param)
                    .find(|p| p.id == param.id)
                    .unwrap();
                assert_eq!(
                    toml::to_string(actual).unwrap(),
                    toml::to_string(param).unwrap()
                );
            }
            let gui = crate::app::GuiAdjustables::from_config(&loaded);
            assert_eq!(
                gui.terrain_soil_strength.value,
                if partial { 0.6 } else { 0.135 }
            );
            GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
            assert_eq!(
                toml::to_string(&GuiConfigLoader::load_from_path(&path)).unwrap(),
                toml::to_string(&loaded).unwrap()
            );
        }
    }

    #[test]
    fn retired_material_controls_do_not_reset_saved_variation() {
        use crate::app::gui_config_model::GuiParamValue;
        let mut config: GuiConfigFile =
            toml::from_str(include_str!("../../config/gui.toml")).unwrap();
        let section = config
            .section
            .iter_mut()
            .find(|s| s.name == "Terrain Material")
            .unwrap();
        let strength = section
            .param
            .iter_mut()
            .find(|p| p.id == "terrain_soil_strength")
            .unwrap();
        strength.value = GuiParamValue::Float {
            value: 0.6,
            min: Some(0.0),
            max: Some(0.75),
        };
        let expected = section.clone();
        for param in &mut section.param {
            param.label = "Old macro material label".into();
        }
        for id in [
            "terrain_soil_scale_voxels",
            "terrain_rock_scale_voxels",
            "terrain_rock_layer_tilt",
            "terrain_material_color_band",
        ] {
            let mut retired = section
                .param
                .iter()
                .find(|p| p.id == "terrain_soil_strength")
                .unwrap()
                .clone();
            retired.id = id.into();
            section.param.push(retired);
        }
        let mut retired_toggle = section.param[0].clone();
        retired_toggle.id = "terrain_material_enabled".into();
        retired_toggle.kind = crate::app::gui_config_model::GuiParamKind::Bool;
        retired_toggle.value = GuiParamValue::Bool { value: false };
        section.param.push(retired_toggle);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gui.toml");
        GuiConfigLoader::save_to_path(&config, &path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let loaded = GuiConfigLoader::load_from_path(&path);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        let actual = loaded
            .section
            .iter()
            .find(|s| s.name == "Terrain Material")
            .unwrap();
        assert_eq!(
            toml::to_string(actual).unwrap(),
            toml::to_string(&expected).unwrap()
        );
        GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
        assert_eq!(
            toml::to_string(&GuiConfigLoader::load_from_path(&path)).unwrap(),
            toml::to_string(&loaded).unwrap()
        );
    }

    #[test]
    fn retired_terrain_fallback_is_removed_without_changing_other_settings() {
        for strength in [0., 0.035, 0.2] {
            let mut config: GuiConfigFile =
                toml::from_str(include_str!("../../config/gui.toml")).unwrap();
            let expected = toml::to_string(&config).unwrap();
            let shadow = config
                .section
                .iter_mut()
                .find(|s| s.name == "Shadow")
                .unwrap();
            let mut retired = shadow
                .param
                .iter()
                .find(|p| p.id == "terrain_ray_origin_offset_world")
                .unwrap()
                .clone();
            retired.id = "terrain_missing_lighting_strength".into();
            retired.value = crate::app::gui_config_model::GuiParamValue::Float {
                value: strength,
                min: Some(0.),
                max: Some(0.2),
            };
            shadow.param.push(retired);
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("gui.toml");
            GuiConfigLoader::save_to_path(&config, &path).unwrap();
            let loaded = GuiConfigLoader::load_from_path(&path);
            assert_eq!(toml::to_string(&loaded).unwrap(), expected);
            GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
            assert_eq!(
                toml::to_string(&GuiConfigLoader::load_from_path(&path)).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn old_ddgi_experiment_values_are_retired_without_saving() {
        use crate::app::gui_config_model::GuiParamValue;
        for continuous in [false, true] {
            for aggregate in [false, true] {
                let mut config: GuiConfigFile =
                    toml::from_str(include_str!("../../config/gui.toml")).unwrap();
                let debug = config
                    .section
                    .iter_mut()
                    .find(|s| s.name == "Debug")
                    .unwrap();
                for (id, value) in [
                    ("ddgi_continuous_sampling", continuous),
                    ("ddgi_aggregate_history", aggregate),
                ] {
                    let mut control = debug
                        .param
                        .iter()
                        .find(|p| p.id == "flower_stem_test_branches")
                        .unwrap()
                        .clone();
                    control.id = id.into();
                    control.value = GuiParamValue::Bool { value };
                    debug.param.push(control);
                }
                let dir = tempfile::tempdir().unwrap();
                let path = dir.path().join("gui.toml");
                GuiConfigLoader::save_to_path(&config, &path).unwrap();
                let bytes = std::fs::read(&path).unwrap();
                let loaded = GuiConfigLoader::load_from_path(&path);
                assert_eq!(bytes, std::fs::read(&path).unwrap());
                assert!(!loaded
                    .section
                    .iter()
                    .flat_map(|s| &s.param)
                    .any(|p| matches!(
                        p.id.as_str(),
                        "ddgi_continuous_sampling" | "ddgi_aggregate_history"
                    )));
                GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
                assert_eq!(
                    toml::to_string(&loaded).unwrap(),
                    toml::to_string(&GuiConfigLoader::load_from_path(&path)).unwrap()
                );
            }
        }
    }

    #[test]
    fn old_configs_receive_neutral_stiffness_and_authored_values_survive_saving() {
        use crate::app::gui_config_model::GuiParamValue;
        let mut config: GuiConfigFile =
            toml::from_str(include_str!("../../config/gui.toml")).unwrap();
        let expected = toml::to_string(&config).unwrap();
        for section in &mut config.section {
            section.param.retain(|p| p.id != "tree_stiffness");
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gui.toml");
        GuiConfigLoader::save_to_path(&config, &path).unwrap();
        let mut loaded = GuiConfigLoader::load_from_path(&path);
        // The migration may append the control, so compare parameters by id.
        let defaults: GuiConfigFile = toml::from_str(&expected).unwrap();
        for param in defaults.section.iter().flat_map(|s| &s.param) {
            let actual = loaded
                .section
                .iter()
                .flat_map(|s| &s.param)
                .find(|p| p.id == param.id)
                .unwrap();
            assert_eq!(
                toml::to_string(actual).unwrap(),
                toml::to_string(param).unwrap()
            );
        }
        let control = loaded
            .section
            .iter_mut()
            .flat_map(|s| &mut s.param)
            .find(|p| p.id == "tree_stiffness")
            .unwrap();
        assert_eq!(
            control.value.get_float().unwrap(),
            (0.5, Some(0.), Some(1.))
        );
        control.value = GuiParamValue::Float {
            value: 0.8,
            min: Some(0.),
            max: Some(1.),
        };
        GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
        let reloaded = GuiConfigLoader::load_from_path(&path);
        assert_eq!(
            toml::to_string(&reloaded).unwrap(),
            toml::to_string(&loaded).unwrap()
        );
    }

    #[test]
    fn retired_render_switches_are_removed_without_changing_other_settings() {
        use crate::app::gui_config_model::GuiParamValue;
        for (enabled, retired_id) in [false, true].into_iter().flat_map(|enabled| {
            [
                "raster_tree_axis_aligned",
                "terrain_hybrid_lighting",
                "apple_preview_model",
                "model_pixel_snap_views",
                "model_pixel_single_light",
            ]
            .map(|id| (enabled, id))
        }) {
            let mut config: GuiConfigFile =
                toml::from_str(include_str!("../../config/gui.toml")).unwrap();
            let debug = config
                .section
                .iter_mut()
                .find(|s| s.name == "Debug")
                .unwrap();
            for param in &mut debug.param {
                if param.id == "raster_tree_wind" {
                    param.value = GuiParamValue::Bool { value: enabled };
                }
            }
            let mut retired = debug
                .param
                .iter()
                .find(|p| p.id == "raster_tree_wind")
                .unwrap()
                .clone();
            retired.id = retired_id.into();
            retired.value = GuiParamValue::Bool { value: enabled };
            let expected = toml::to_string(&config).unwrap();
            config
                .section
                .iter_mut()
                .find(|s| s.name == "Debug")
                .unwrap()
                .param
                .push(retired);
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("gui.toml");
            GuiConfigLoader::save_to_path(&config, &path).unwrap();
            let migrated = GuiConfigLoader::load_from_path(&path);
            assert_eq!(toml::to_string(&migrated).unwrap(), expected);
            GuiConfigLoader::save_to_path(&migrated, &path).unwrap();
            assert!(!std::fs::read_to_string(&path).unwrap().contains(retired_id));
            assert_eq!(
                toml::to_string(&GuiConfigLoader::load_from_path(&path)).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn old_tree_raster_and_hybrid_switches_do_not_survive_loading() {
        use crate::app::gui_config_model::GuiParamValue;
        let mut config: GuiConfigFile =
            toml::from_str(include_str!("../../config/gui.toml")).unwrap();
        let debug = config
            .section
            .iter_mut()
            .find(|s| s.name == "Debug")
            .unwrap();
        let mut old = debug
            .param
            .iter()
            .find(|p| p.id == "raster_tree_wind")
            .unwrap()
            .clone();
        old.value = GuiParamValue::Bool { value: false };
        for id in ["raster_tree_static", "raster_tree_hybrid_lighting"] {
            old.id = id.into();
            debug.param.push(old.clone());
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gui.toml");
        GuiConfigLoader::save_to_path(&config, &path).unwrap();
        let loaded = GuiConfigLoader::load_from_path(&path);
        assert!(!loaded.section.iter().flat_map(|s| &s.param).any(|p| [
            "raster_tree_static",
            "raster_tree_hybrid_lighting"
        ]
        .contains(&p.id.as_str())));
        GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
        let saved = std::fs::read_to_string(path).unwrap();
        assert!(!saved.contains("raster_tree_static"));
        assert!(!saved.contains("raster_tree_hybrid_lighting"));
    }

    #[test]
    fn global_view_count_wins_and_old_flower_owner_is_retired() {
        use crate::app::gui_config_model::GuiParamValue;
        let mut config: GuiConfigFile =
            toml::from_str(include_str!("../../config/gui.toml")).unwrap();
        let debug = config
            .section
            .iter_mut()
            .find(|s| s.name == "Debug")
            .unwrap();
        let count = debug
            .param
            .iter_mut()
            .find(|p| p.id == "model_pixel_view_count")
            .unwrap();
        count.value = GuiParamValue::Uint {
            value: 8,
            min: Some(8),
            max: Some(512),
        };
        let mut old = debug
            .param
            .iter()
            .find(|p| p.id == "raster_tree_wind")
            .unwrap()
            .clone();
        old.id = "model_pixel_snap_views".into();
        old.value = GuiParamValue::Bool { value: true };
        debug.param.push(old);
        let flora = config
            .section
            .iter_mut()
            .find(|s| s.name == "Flora")
            .unwrap();
        let mut legacy = flora
            .param
            .iter()
            .find(|p| p.id == "model_flower_pixel_resolution")
            .unwrap()
            .clone();
        legacy.id = "model_flower_view_count".into();
        legacy.value = GuiParamValue::Uint {
            value: 37,
            min: Some(8),
            max: Some(512),
        };
        flora.param.push(legacy);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gui.toml");
        GuiConfigLoader::save_to_path(&config, &path).unwrap();
        let loaded = GuiConfigLoader::load_from_path(&path);
        assert!(!loaded
            .section
            .iter()
            .flat_map(|s| &s.param)
            .any(|p| p.id == "model_pixel_snap_views"));
        assert!(loaded
            .section
            .iter()
            .flat_map(|s| &s.param)
            .any(|p| p.id == "model_pixel_view_count"
                && matches!(p.value, GuiParamValue::Uint { value: 8, .. })));
        assert!(!loaded
            .section
            .iter()
            .flat_map(|s| &s.param)
            .any(|p| p.id == "model_flower_view_count"));
        assert!(!loaded
            .section
            .iter()
            .flat_map(|s| &s.param)
            .any(|p| p.id == "model_view_quantization_enabled"));
        GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
        let saved = std::fs::read_to_string(&path).unwrap();
        assert!(!saved.contains("model_flower_view_count"));
        assert!(!saved.contains("model_pixel_snap_views"));
    }

    #[test]
    fn flower_only_view_saves_migrate_clamp_and_do_not_write_on_load() {
        use crate::app::gui_config_model::GuiParamValue;
        for (old, expected) in [(0, 8), (37, 37), (128, 128), (256, 256), (u32::MAX, 512)] {
            let mut config: GuiConfigFile =
                toml::from_str(include_str!("../../config/gui.toml")).unwrap();
            let mut legacy = config
                .section
                .iter()
                .flat_map(|s| &s.param)
                .find(|p| p.id == "model_pixel_view_count")
                .unwrap()
                .clone();
            legacy.id = "model_flower_view_count".into();
            legacy.label = "Flower View Count (Static)".into();
            legacy.value = GuiParamValue::Uint {
                value: old,
                min: None,
                max: None,
            };
            for section in &mut config.section {
                section.param.retain(|p| p.id != "model_pixel_view_count");
            }
            config
                .section
                .iter_mut()
                .find(|s| s.name == "Flora")
                .unwrap()
                .param
                .push(legacy);
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("gui.toml");
            GuiConfigLoader::save_to_path(&config, &path).unwrap();
            let original = std::fs::read(&path).unwrap();
            let loaded = GuiConfigLoader::load_from_path(&path);
            assert_eq!(std::fs::read(&path).unwrap(), original);
            let debug = loaded.section.iter().find(|s| s.name == "Debug").unwrap();
            assert!(debug.param.iter().any(|p| p.id == "model_pixel_view_count" && matches!(p.value, GuiParamValue::Uint { value, min: Some(8), max: Some(512) } if value == expected)));
            assert!(!debug
                .param
                .iter()
                .any(|p| p.id == "model_view_quantization_enabled"));
            assert!(!loaded
                .section
                .iter()
                .flat_map(|s| &s.param)
                .any(|p| p.id == "model_flower_view_count"));
            GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
            let reloaded = GuiConfigLoader::load_from_path(&path);
            assert_eq!(
                serde_json::to_value(&loaded).unwrap(),
                serde_json::to_value(&reloaded).unwrap()
            );
        }
    }

    #[test]
    fn model_view_ab_is_retired_preserving_count_and_unrelated_settings_without_writing() {
        use crate::app::gui_config_model::GuiParamValue;
        for enabled in [false, true] {
            for count in [8, 37, 128, 256, 512] {
                let mut config: GuiConfigFile =
                    toml::from_str(include_str!("../../config/gui.toml")).unwrap();
                let debug = config
                    .section
                    .iter_mut()
                    .find(|s| s.name == "Debug")
                    .unwrap();
                debug
                    .param
                    .iter_mut()
                    .find(|p| p.id == "model_pixel_view_count")
                    .unwrap()
                    .value = GuiParamValue::Uint {
                    value: count,
                    min: Some(8),
                    max: Some(512),
                };
                let wind = debug
                    .param
                    .iter_mut()
                    .find(|p| p.id == "raster_tree_wind")
                    .unwrap();
                wind.value = GuiParamValue::Bool { value: false };
                let wind_before = wind.clone();
                let mut retired = wind.clone();
                retired.id = "model_view_quantization_enabled".into();
                retired.label = "Models: quantized views (A/B)".into();
                retired.value = GuiParamValue::Bool { value: enabled };
                debug.param.push(retired);
                let directory = tempfile::tempdir().unwrap();
                let path = directory.path().join("gui.toml");
                GuiConfigLoader::save_to_path(&config, &path).unwrap();
                let original = std::fs::read(&path).unwrap();
                let loaded = GuiConfigLoader::load_from_path(&path);
                assert_eq!(std::fs::read(&path).unwrap(), original);
                let params: Vec<_> = loaded.section.iter().flat_map(|s| &s.param).collect();
                assert!(!params
                    .iter()
                    .any(|p| p.id == "model_view_quantization_enabled"));
                let counts: Vec<_> = params
                    .iter()
                    .filter(|p| p.id == "model_pixel_view_count")
                    .collect();
                assert_eq!(counts.len(), 1);
                assert!(
                    matches!(counts[0].value, GuiParamValue::Uint { value, .. } if value == count)
                );
                assert!(counts[0].enabled_if.is_none());
                assert_eq!(
                    serde_json::to_value(
                        params.iter().find(|p| p.id == "raster_tree_wind").unwrap()
                    )
                    .unwrap(),
                    serde_json::to_value(&wind_before).unwrap()
                );
                GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
                assert!(!std::fs::read_to_string(&path)
                    .unwrap()
                    .contains("model_view_quantization_enabled"));
                let reloaded = GuiConfigLoader::load_from_path(&path);
                assert_eq!(
                    serde_json::to_value(&loaded).unwrap(),
                    serde_json::to_value(&reloaded).unwrap()
                );
            }
        }
    }

    #[test]
    fn saved_screen_grid_switch_is_retired() {
        use crate::app::gui_config_model::GuiParamValue;
        for enabled in [false, true] {
            let mut config: GuiConfigFile =
                toml::from_str(include_str!("../../config/gui.toml")).unwrap();
            let debug = config
                .section
                .iter_mut()
                .find(|s| s.name == "Debug")
                .unwrap();
            let mut retired = debug
                .param
                .iter()
                .find(|p| p.id == "raster_tree_wind")
                .unwrap()
                .clone();
            retired.id = "model_pixel_screen_grid".into();
            retired.value = GuiParamValue::Bool { value: enabled };
            debug.param.push(retired);
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("gui.toml");
            GuiConfigLoader::save_to_path(&config, &path).unwrap();
            let loaded = GuiConfigLoader::load_from_path(&path);
            assert!(!loaded
                .section
                .iter()
                .flat_map(|s| &s.param)
                .any(|p| p.id == "model_pixel_screen_grid"));
            GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
            assert!(!std::fs::read_to_string(&path)
                .unwrap()
                .contains("model_pixel_screen_grid"));
        }
    }

    #[test]
    fn old_butterfly_settings_gain_authored_transmission_without_other_changes() {
        let mut config: GuiConfigFile =
            toml::from_str(include_str!("../../config/gui.toml")).unwrap();
        let authored_transmission = config
            .section
            .iter()
            .flat_map(|section| &section.param)
            .find(|param| param.id == "butterfly_wing_transmission")
            .unwrap()
            .value
            .get_float();
        for section in &mut config.section {
            section
                .param
                .retain(|p| p.id != "butterfly_wing_transmission");
        }
        let expected = toml::to_string(&config).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gui.toml");
        GuiConfigLoader::save_to_path(&config, &path).unwrap();
        let mut loaded = GuiConfigLoader::load_from_path(&path);
        let transmission = loaded
            .section
            .iter()
            .flat_map(|s| &s.param)
            .find(|p| p.id == "butterfly_wing_transmission")
            .unwrap();
        assert_eq!(transmission.value.get_float(), authored_transmission);
        for section in &mut loaded.section {
            section
                .param
                .retain(|p| p.id != "butterfly_wing_transmission");
        }
        assert_eq!(toml::to_string(&loaded).unwrap(), expected);
    }

    #[test]
    fn retired_butterfly_appearance_settings_preserve_motion_and_saved_values() {
        use crate::app::gui_config_model::GuiParamValue;
        use crate::particles::ButterflyFlightVariant;
        for (legacy, variant) in [
            ("OriginalSprite", ButterflyFlightVariant::Original),
            ("DartingSprite", ButterflyFlightVariant::Darting),
            ("DartingBlock", ButterflyFlightVariant::Darting),
        ] {
            for enabled in [false, true] {
                let mut config: GuiConfigFile =
                    toml::from_str(include_str!("../../config/gui.toml")).unwrap();
                config.custom.butterfly_flight.variant = variant;
                config.custom.butterfly_flight.tuning.speed = 0.73;
                for param in config.section.iter_mut().flat_map(|s| &mut s.param) {
                    match (param.id.as_str(), &mut param.value) {
                        ("butterfly_pixel_resolution", GuiParamValue::Uint { value, .. }) => {
                            *value = 16
                        }
                        ("butterfly_animation_fps", GuiParamValue::Uint { value, .. }) => {
                            *value = 8
                        }
                        ("butterfly_mesh_preview", GuiParamValue::Bool { value }) => *value = true,
                        _ => {}
                    }
                }
                let expected = toml::to_string(&config).unwrap();
                let section = config
                    .section
                    .iter_mut()
                    .find(|s| s.name == "Butterflies")
                    .unwrap();
                let mut retired = section
                    .param
                    .iter()
                    .find(|p| p.id == "butterfly_mesh_preview")
                    .unwrap()
                    .clone();
                retired.id = "butterfly_mesh_enabled".into();
                retired.value = GuiParamValue::Bool { value: enabled };
                let mut self_shadows = retired.clone();
                self_shadows.id = "butterfly_self_shadows".into();
                section.param.push(self_shadows);
                section.param.push(retired);
                let legacy_text = toml::to_string(&config).unwrap().replace(
                    &format!("variant = \"{variant:?}\""),
                    &format!("variant = \"{legacy}\""),
                );
                let dir = tempfile::tempdir().unwrap();
                let path = dir.path().join("gui.toml");
                std::fs::write(&path, legacy_text).unwrap();
                let migrated = GuiConfigLoader::load_from_path(&path);
                assert_eq!(toml::to_string(&migrated).unwrap(), expected);
                GuiConfigLoader::save_to_path(&migrated, &path).unwrap();
                let saved = std::fs::read_to_string(&path).unwrap();
                assert!(!saved.contains("butterfly_mesh_enabled"));
                assert!(!saved.contains("butterfly_self_shadows"));
                assert!(!saved.contains(legacy));
                assert_eq!(
                    toml::to_string(&GuiConfigLoader::load_from_path(&path)).unwrap(),
                    expected
                );
            }
        }
    }

    #[test]
    fn amplitude_migration_preserves_strength_radius_and_other_settings() {
        use crate::app::gui_config_model::GuiParamValue;
        let mut config: GuiConfigFile =
            toml::from_str(include_str!("../../config/gui.toml")).unwrap();
        let leaves = config
            .section
            .iter_mut()
            .find(|s| s.name == "Leaves")
            .unwrap();
        let old = leaves
            .param
            .iter_mut()
            .find(|p| p.id == "leaf_flutter_amplitude_high")
            .unwrap();
        old.id = "leaf_flutter_strength".into();
        old.value = GuiParamValue::Float {
            value: 1.4,
            min: Some(0.),
            max: Some(2.),
        };
        leaves
            .param
            .retain(|p| p.id != "leaf_flutter_amplitude_low");
        let before = config.clone();
        GuiConfigLoader::migrate_flutter_amplitude(&mut config);
        let params: Vec<_> = config.section.iter().flat_map(|s| &s.param).collect();
        assert_eq!(
            params
                .iter()
                .find(|p| p.id == "leaf_flutter_amplitude_high")
                .unwrap()
                .value
                .get_float()
                .unwrap()
                .0,
            0.7
        );
        assert_eq!(
            params
                .iter()
                .find(|p| p.id == "leaf_flutter_amplitude_low")
                .unwrap()
                .value
                .get_float()
                .unwrap()
                .0,
            0.
        );
        for old in before.section.iter().flat_map(|s| &s.param) {
            if old.id == "leaf_flutter_strength" {
                continue;
            }
            let after = params.iter().find(|p| p.id == old.id).unwrap();
            assert_eq!(
                toml::to_string(&old.value).unwrap(),
                toml::to_string(&after.value).unwrap(),
                "{}",
                old.id
            );
        }
        let once = toml::to_string(&config).unwrap();
        GuiConfigLoader::migrate_flutter_amplitude(&mut config);
        assert_eq!(once, toml::to_string(&config).unwrap());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gui.toml");
        GuiConfigLoader::save_to_path(&before, &path).unwrap();
        assert_eq!(
            once,
            toml::to_string(&GuiConfigLoader::load_from_path(&path)).unwrap()
        );
    }

    #[test]
    fn old_flutter_settings_migrate_without_changing_the_saved_curve() {
        use crate::app::gui_config_model::{GuiConfigFile, GuiParamValue};
        let mut config: GuiConfigFile =
            toml::from_str(include_str!("../../config/gui.toml")).unwrap();
        let leaves = config
            .section
            .iter_mut()
            .find(|s| s.name == "Leaves")
            .unwrap();
        for (new, old, value) in [
            (
                "leaf_flutter_frequency_low_hz",
                "leaf_flutter_frequency_hz",
                1.5,
            ),
            (
                "leaf_flutter_frequency_high_hz",
                "leaf_flutter_frequency_scale",
                2.,
            ),
        ] {
            let p = leaves.param.iter_mut().find(|p| p.id == new).unwrap();
            p.id = old.into();
            if let GuiParamValue::Float { value: stored, .. } = &mut p.value {
                *stored = value;
            }
        }
        leaves
            .param
            .retain(|p| p.id != "leaf_flutter_frequency_ceiling_hz");
        let before = config.clone();
        GuiConfigLoader::migrate_flutter_frequency(&mut config);
        let get = |id: &str| {
            config
                .section
                .iter()
                .flat_map(|s| &s.param)
                .find(|p| p.id == id)
                .unwrap()
                .value
                .get_float()
                .unwrap()
                .0
        };
        assert_eq!(get("leaf_flutter_frequency_low_hz"), 1.5);
        assert_eq!(get("leaf_flutter_frequency_high_hz"), 3.);
        assert_eq!(get("leaf_flutter_frequency_ceiling_hz"), 24.);
        for s in &before.section {
            for p in &s.param {
                if p.id == "leaf_flutter_frequency_hz" || p.id == "leaf_flutter_frequency_scale" {
                    continue;
                }
                let after = config
                    .section
                    .iter()
                    .flat_map(|s| &s.param)
                    .find(|q| q.id == p.id)
                    .unwrap();
                assert_eq!(toml::to_string(p).unwrap(), toml::to_string(after).unwrap());
            }
        }
        let once = toml::to_string(&config).unwrap();
        GuiConfigLoader::migrate_flutter_frequency(&mut config);
        assert_eq!(once, toml::to_string(&config).unwrap());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gui.toml");
        GuiConfigLoader::save_to_path(&before, &path).unwrap();
        let loaded = GuiConfigLoader::load_from_path(&path);
        assert_eq!(toml::to_string(&loaded).unwrap(), once);
    }

    #[test]
    fn frequency_ceiling_migration_preserves_saved_curve_and_other_fields() {
        use crate::app::gui_config_model::GuiParamValue;
        let mut config: GuiConfigFile =
            toml::from_str(include_str!("../../config/gui.toml")).unwrap();
        let before = config.clone();
        let p = config
            .section
            .iter_mut()
            .flat_map(|s| &mut s.param)
            .find(|p| p.id == "leaf_flutter_frequency_ceiling_hz")
            .unwrap();
        p.id = "leaf_flutter_frequency_multiplier".into();
        if let GuiParamValue::Float { value, .. } = &mut p.value {
            *value = 0.75;
        }
        GuiConfigLoader::migrate_frequency_ceiling(&mut config);
        for old in before.section.iter().flat_map(|s| &s.param) {
            let new = config
                .section
                .iter()
                .flat_map(|s| &s.param)
                .find(|p| p.id == old.id)
                .unwrap();
            if old.id == "leaf_flutter_frequency_ceiling_hz" {
                assert_eq!(new.value.get_float().unwrap().0, 18.);
            } else {
                assert_eq!(toml::to_string(old).unwrap(), toml::to_string(new).unwrap());
            }
        }
        let once = toml::to_string(&config).unwrap();
        GuiConfigLoader::migrate_frequency_ceiling(&mut config);
        assert_eq!(once, toml::to_string(&config).unwrap());
    }

    use super::{GuiConfigLoader, GUI_FLOAT_DECIMALS};
    use crate::app::gui_config_model::GuiConfigFile;
    use std::path::Path;

    fn validation_error(config: &str) -> String {
        let config: GuiConfigFile = toml::from_str(config).unwrap();
        let panic = std::panic::catch_unwind(|| {
            GuiConfigLoader::validate(&config, Path::new("test-gui.toml"));
        })
        .expect_err("config should fail validation");
        panic
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| {
                panic
                    .downcast_ref::<&str>()
                    .map(|message| (*message).to_owned())
            })
            .unwrap()
    }

    #[test]
    fn ordered_dither_migration_preserves_old_settings_and_partial_preferences() {
        use crate::app::gui_config_model::{GuiParamKind, GuiParamValue};
        let mut original: GuiConfigFile =
            toml::from_str(include_str!("../../config/gui.toml")).unwrap();
        original.section.retain(|s| s.name != "Ordered Dithering");
        let before = toml::to_string(&original).unwrap();
        GuiConfigLoader::simplify_ordered_dither_settings(&mut original);
        let ordered = original
            .section
            .iter_mut()
            .find(|s| s.name == "Ordered Dithering")
            .unwrap();
        assert_eq!(ordered.param.len(), 3);
        assert_eq!(
            ordered
                .param
                .iter()
                .filter(|p| matches!(p.kind, GuiParamKind::Bool))
                .count(),
            1
        );
        ordered.param.retain(|p| p.id == "ordered_dither_levels");
        if let GuiParamValue::Uint { value, .. } = &mut ordered.param[0].value {
            *value = 5;
        }
        GuiConfigLoader::simplify_ordered_dither_settings(&mut original);
        let ordered = original
            .section
            .iter()
            .find(|s| s.name == "Ordered Dithering")
            .unwrap();
        assert_eq!(ordered.param[0].value.get_uint().unwrap().0, 5);
        let once = toml::to_string(&original).unwrap();
        GuiConfigLoader::simplify_ordered_dither_settings(&mut original);
        assert_eq!(once, toml::to_string(&original).unwrap());
        original.section.retain(|s| s.name != "Ordered Dithering");
        assert_eq!(before, toml::to_string(&original).unwrap());
    }

    #[test]
    fn retired_dither_flags_and_pattern_are_dropped_without_changing_global_or_saved_files() {
        use crate::app::gui_config_model::{GuiParamKind, GuiParamValue};
        for enabled in [false, true] {
            let mut legacy: GuiConfigFile =
                toml::from_str(include_str!("../../config/gui.toml")).unwrap();
            let section = legacy
                .section
                .iter_mut()
                .find(|s| s.name == "Ordered Dithering")
                .unwrap();
            let global = section
                .param
                .iter_mut()
                .find(|p| p.id == "ordered_dither_global")
                .unwrap();
            global.value = GuiParamValue::Bool { value: enabled };
            global.label = "Global scene dither (A/B)".into();
            let mut retired = global.clone();
            retired.value = GuiParamValue::Bool { value: true };
            for id in [
                "ordered_dither_god_rays",
                "ordered_dither_lens_flare",
                "ordered_dither_sky_background",
                "ordered_dither_terrain_ambient",
            ] {
                retired.id = id.into();
                section.param.push(retired.clone());
            }
            retired.id = "ordered_dither_pattern".into();
            retired.kind = GuiParamKind::Choice;
            retired.value = GuiParamValue::Choice {
                value: 1,
                options: vec!["Bayer".into(), "Halftone".into()],
            };
            section.param.push(retired);
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("gui.toml");
            let original = toml::to_string(&legacy).unwrap();
            std::fs::write(&path, &original).unwrap();
            let migrated = GuiConfigLoader::load_from_path(&path);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
            let section = migrated
                .section
                .iter()
                .find(|s| s.name == "Ordered Dithering")
                .unwrap();
            assert_eq!(section.param.len(), 3);
            let global = section
                .param
                .iter()
                .find(|p| p.id == "ordered_dither_global")
                .unwrap();
            assert_eq!(global.value.get_bool(), Some(enabled));
            assert!(global.label.contains("Bayer 4x4"));
            for param in &section.param {
                if param.id != "ordered_dither_global" {
                    assert_eq!(
                        param.enabled_if.as_ref().unwrap().param,
                        "ordered_dither_global"
                    );
                    let old = legacy
                        .section
                        .iter()
                        .flat_map(|s| &s.param)
                        .find(|p| p.id == param.id)
                        .unwrap();
                    assert_eq!(
                        toml::Value::try_from(&param.value).unwrap(),
                        toml::Value::try_from(&old.value).unwrap()
                    );
                }
            }
            GuiConfigLoader::save_to_path(&migrated, &path).unwrap();
            let saved = std::fs::read_to_string(&path).unwrap();
            for id in [
                "ordered_dither_pattern",
                "ordered_dither_god_rays",
                "ordered_dither_lens_flare",
                "ordered_dither_sky_background",
                "ordered_dither_terrain_ambient",
            ] {
                assert!(!saved.contains(id));
            }
        }
    }

    #[test]
    fn normalize_float_assignments_rounds_and_trims() {
        let src = "value = 0.05000000074505806\nmin = 0.10000000149011612\nmax = 2.0\n";
        let normalized = GuiConfigLoader::normalize_float_assignments(src, 6);
        assert_eq!(normalized, "value = 0.05\nmin = 0.1\nmax = 2\n");
    }

    #[test]
    fn configured_precision_preserves_one_voxel_world_scale() {
        let src = "value = 0.00390625\n";
        let normalized = GuiConfigLoader::normalize_float_assignments(src, GUI_FLOAT_DECIMALS);
        assert_eq!(normalized, src);
    }

    #[test]
    fn normalize_float_assignments_leaves_non_numeric_values() {
        let src = "value = true\nmin = 1\nmax = \"#FF00FF\"\n";
        let normalized = GuiConfigLoader::normalize_float_assignments(src, 6);
        assert_eq!(normalized, "value = true\nmin = 1\nmax = \"#FF00FF\"\n");
    }

    #[test]
    fn atomic_save_replaces_existing_config_without_leaving_temporary_files() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("gui.toml");
        std::fs::write(&path, "incomplete previous contents").unwrap();
        let mut config = GuiConfigLoader::load();
        config.tree.desc.size = 17.5;

        GuiConfigLoader::save_to_path(&config, &path).unwrap();

        let loaded = GuiConfigLoader::load_from_path(&path);
        assert_eq!(loaded.tree.desc.size, 17.5);
        let files = std::fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(files, vec![std::ffi::OsString::from("gui.toml")]);
    }

    #[test]
    fn enabled_if_rejects_unknown_controller() {
        let error = validation_error(
            r#"
schema_version = 1
[[section]]
name = "Debug"
[[section.param]]
id = "dependent"
kind = "bool"
label = "Dependent"
enabled_if = { param = "missing", equals = true }
type = "Bool"
[section.param.data]
value = false
"#,
        );

        assert!(error.contains("enabled_if references unknown param 'missing'"));
    }

    #[test]
    fn enabled_if_rejects_condition_type_that_does_not_match_controller() {
        let error = validation_error(
            r#"
schema_version = 1
[[section]]
name = "Debug"
[[section.param]]
id = "controller"
kind = "bool"
label = "Controller"
type = "Bool"
[section.param.data]
value = false
[[section.param]]
id = "dependent"
kind = "bool"
label = "Dependent"
enabled_if = { param = "controller", equals = 1 }
type = "Bool"
[section.param.data]
value = false
"#,
        );

        assert!(error.contains("enabled_if value type does not match controller 'controller'"));
    }
}
