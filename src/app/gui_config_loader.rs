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

        let content = std::fs::read_to_string(&config_path).unwrap_or_else(|e| {
            panic!(
                "Failed to read GUI config file at {}: {}",
                config_path.display(),
                e
            );
        });

        let config: GuiConfigFile = toml::from_str(&content).unwrap_or_else(|e| {
            panic!(
                "Failed to parse GUI config at {}:\n{}",
                config_path.display(),
                e
            );
        });

        Self::validate(&config, config_path);

        log::info!(
            "Loaded GUI config: {} (schema v{}, {} sections, {} params)",
            config_path.display(),
            config.schema_version,
            config.section.len(),
            config.section.iter().map(|s| s.param.len()).sum::<usize>()
        );

        config
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
        config.tree.as_mut().unwrap().desc.size = 17.5;

        GuiConfigLoader::save_to_path(&config, &path).unwrap();

        let loaded = GuiConfigLoader::load_from_path(&path);
        assert_eq!(loaded.tree.unwrap().desc.size, 17.5);
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
