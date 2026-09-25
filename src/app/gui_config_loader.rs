use crate::app::gui_config_model::{
    GuiConfigFile, GuiParamConditionValue, GuiParamKind, GuiParamValue,
};
use std::{collections::HashMap, collections::HashSet, io::Write, path::Path};

const SUPPORTED_SCHEMA_VERSION: u32 = 1;
const CONFIG_FILE_NAME: &str = "gui.toml";
// Keep exact voxel-scale controls such as 1 / 256 while still trimming the
// noisy tail emitted when an f32 is serialized through TOML.
const GUI_FLOAT_DECIMALS: usize = 8;

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
            if param.id == "butterfly_animation_fps" {
                param.label = "Butterfly Update FPS (Position + Heading + Wings)".into();
            }
        }
        Self::add_missing_param(&mut config, "Debug", "tree_stiffness");
        Self::add_missing_param(&mut config, "Debug", "ddgi_continuous_sampling");
        Self::add_missing_param(&mut config, "Debug", "ddgi_aggregate_history");
        Self::add_missing_section_params(&mut config, "Terrain Material");
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
                        | "apple_preview_model"
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
                } else {
                    saved.param.push(param);
                }
            }
        } else {
            config.section.push(section);
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
    fn old_configs_default_ddgi_sampling_to_original_and_save_experiment() {
        use crate::app::gui_config_model::GuiParamValue;
        for id in ["ddgi_continuous_sampling", "ddgi_aggregate_history"] {
            let mut config: GuiConfigFile =
                toml::from_str(include_str!("../../config/gui.toml")).unwrap();
            for section in &mut config.section {
                section.param.retain(|p| p.id != id);
            }
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("gui.toml");
            GuiConfigLoader::save_to_path(&config, &path).unwrap();
            let mut loaded = GuiConfigLoader::load_from_path(&path);
            let control = loaded
                .section
                .iter_mut()
                .flat_map(|s| &mut s.param)
                .find(|p| p.id == id)
                .unwrap();
            assert!(matches!(
                control.value,
                GuiParamValue::Bool { value: false }
            ));
            control.value = GuiParamValue::Bool { value: true };
            GuiConfigLoader::save_to_path(&loaded, &path).unwrap();
            let reloaded = GuiConfigLoader::load_from_path(&path);
            assert_eq!(
                toml::to_string(&loaded).unwrap(),
                toml::to_string(&reloaded).unwrap()
            );
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
                if ["raster_tree_static", "raster_tree_wind"].contains(&param.id.as_str()) {
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
                    .find(|p| p.id == "butterfly_self_shadows")
                    .unwrap()
                    .clone();
                retired.id = "butterfly_mesh_enabled".into();
                retired.value = GuiParamValue::Bool { value: enabled };
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
