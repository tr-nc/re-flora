use crate::tree_gen::TreeDesc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GuiConfigFile {
    pub schema_version: u32,
    #[serde(flatten)]
    pub custom: SavedCustomSettings,
    pub section: Vec<GuiSection>,
}

/// Add typed custom settings here. Serde saves the live object directly; no save hook.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SavedCustomSettings {
    #[serde(default)]
    pub audio_mix: crate::audio::mixer::AudioMixSettings,
    // Simulates a newly added custom setting in every test build. There is no
    // matching entry in DebugSettings::save, load, or synchronization code.
    #[cfg(test)]
    #[serde(default)]
    pub future_control_fixture: f32,
    #[serde(default)]
    pub butterfly_flight: crate::particles::ButterflyFlightSettings,
    #[serde(default)]
    pub tree: TreeGuiConfig,
}

impl std::ops::Deref for GuiConfigFile {
    type Target = SavedCustomSettings;
    fn deref(&self) -> &Self::Target {
        &self.custom
    }
}
impl std::ops::DerefMut for GuiConfigFile {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.custom
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct TreeGuiConfig {
    #[serde(default = "default_true")]
    pub render_leaves: bool,
    #[serde(default = "default_true")]
    pub conifer_axis_model: bool,
    #[serde(default, deserialize_with = "deserialize_current_tree")]
    pub desc: TreeDesc,
}

impl Default for TreeGuiConfig {
    fn default() -> Self {
        Self {
            render_leaves: true,
            conifer_axis_model: true,
            desc: TreeDesc::default(),
        }
    }
}

fn deserialize_current_tree<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<TreeDesc, D::Error> {
    TreeDesc::deserialize(deserializer).map(TreeDesc::into_current)
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GuiSection {
    pub name: String,
    pub param: Vec<GuiParam>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GuiParam {
    pub id: String,
    pub kind: GuiParamKind,
    #[allow(dead_code)]
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled_if: Option<GuiParamEnabledIf>,
    #[serde(flatten)]
    pub value: GuiParamValue,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct GuiParamEnabledIf {
    pub param: String,
    pub equals: GuiParamConditionValue,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum GuiParamConditionValue {
    Bool(bool),
    Integer(i64),
    String(String),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GuiParamKind {
    Float,
    Int,
    Uint,
    Choice,
    String,
    Bool,
    Color,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum GuiParamValue {
    Float {
        value: f32,
        #[serde(default)]
        min: Option<f32>,
        #[serde(default)]
        max: Option<f32>,
    },
    Int {
        value: i32,
        #[serde(default)]
        min: Option<i32>,
        #[serde(default)]
        max: Option<i32>,
    },
    Uint {
        value: u32,
        #[serde(default)]
        min: Option<u32>,
        #[serde(default)]
        max: Option<u32>,
    },
    Choice {
        value: u32,
        options: Vec<String>,
    },
    String {
        value: String,
    },
    Bool {
        value: bool,
    },
    Color {
        value: String,
    },
}

impl GuiParamValue {
    #[allow(dead_code)]
    pub fn get_float(&self) -> Option<(f32, Option<f32>, Option<f32>)> {
        match self {
            GuiParamValue::Float { value, min, max } => Some((*value, *min, *max)),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn get_int(&self) -> Option<(i32, Option<i32>, Option<i32>)> {
        match self {
            GuiParamValue::Int { value, min, max } => Some((*value, *min, *max)),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn get_uint(&self) -> Option<(u32, Option<u32>, Option<u32>)> {
        match self {
            GuiParamValue::Uint { value, min, max } => Some((*value, *min, *max)),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn get_choice(&self) -> Option<(u32, &[String])> {
        match self {
            GuiParamValue::Choice { value, options } => Some((*value, options.as_slice())),
            _ => None,
        }
    }

    #[allow(dead_code)]
    #[allow(dead_code)]
    pub fn get_string(&self) -> Option<String> {
        match self {
            GuiParamValue::String { value } => Some(value.clone()),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn get_bool(&self) -> Option<bool> {
        match self {
            GuiParamValue::Bool { value } => Some(*value),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn get_color(&self) -> Option<String> {
        match self {
            GuiParamValue::Color { value } => Some(value.clone()),
            _ => None,
        }
    }

    pub fn set_float(&mut self, value: f32) {
        if let GuiParamValue::Float { value: v, .. } = self {
            *v = value;
        }
    }

    pub fn set_int(&mut self, value: i32) {
        if let GuiParamValue::Int { value: v, .. } = self {
            *v = value;
        }
    }

    pub fn set_uint(&mut self, value: u32) {
        if let GuiParamValue::Uint { value: v, .. } = self {
            *v = value;
        }
    }

    pub fn set_choice(&mut self, value: u32) {
        if let GuiParamValue::Choice { value: v, .. } = self {
            *v = value;
        }
    }

    pub fn set_string(&mut self, value: String) {
        if let GuiParamValue::String { value: v } = self {
            *v = value;
        }
    }

    pub fn set_bool(&mut self, value: bool) {
        if let GuiParamValue::Bool { value: v } = self {
            *v = value;
        }
    }

    pub fn set_color(&mut self, value: String) {
        if let GuiParamValue::Color { value: v } = self {
            *v = value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_gui_config_round_trip_preserves_all_tree_fields() {
        let mut desc = TreeDesc {
            size: 17.25,
            ..Default::default()
        };
        desc.branching.initial_length = 63.0;
        desc.leaf_density = 0.123;
        desc.fruit_swing_speed = 4.5;
        desc.subdivision_count_max = 8;
        let config = GuiConfigFile {
            schema_version: 1,
            custom: SavedCustomSettings {
                butterfly_flight: Default::default(),
                tree: TreeGuiConfig {
                    render_leaves: false,
                    conifer_axis_model: false,
                    desc: desc.clone(),
                },
                ..Default::default()
            },
            section: Vec::new(),
        };

        let serialized = toml::to_string_pretty(&config).unwrap();
        let parsed: GuiConfigFile = toml::from_str(&serialized).unwrap();
        let parsed_tree = &parsed.tree;

        assert!(!parsed_tree.render_leaves);
        assert!(!parsed_tree.conifer_axis_model);
        assert_eq!(parsed_tree.desc, desc);
    }

    #[test]
    fn fresh_apple_tuning_uses_current_defaults_without_overriding_saved_authoring() {
        // The user's mutable config file is not a test fixture. Missing tuning uses the
        // current defaults, while an explicitly authored older Apple must remain unchanged.
        let fresh: TreeGuiConfig = toml::from_str("").unwrap();
        assert_eq!(fresh.desc, TreeDesc::default());
        let saved: TreeGuiConfig = toml::from_str(
            "[desc]\nspecies = \"apple_tree\"\nsize = 20.0\nleaves_size_level = 5\nfruit_down_offset_voxels = 14.0\n",
        ).unwrap();
        assert_eq!(saved.desc.size, 20.);
        assert_eq!(saved.desc.leaves_size_level, 5);
        assert_eq!(saved.desc.fruit_down_offset_voxels, 14.);
        let restored: TreeGuiConfig = toml::from_str(&toml::to_string(&saved).unwrap()).unwrap();
        assert_eq!(restored, saved);
    }

    #[test]
    fn legacy_tuning_tree_loads_as_current_apple_and_saves_without_retired_identity() {
        for species in ["", "species = \"garden_tree\"\n"] {
            let source = format!("[desc]\n{species}size = 17.25\n");
            let parsed: TreeGuiConfig = toml::from_str(&source).unwrap();
            assert_eq!(parsed.desc.species, crate::tree_gen::TreeSpecies::AppleTree);
            assert_eq!(parsed.desc.size, 17.25);
            assert_eq!(parsed.desc.branching.seed, 122);
            assert_eq!(
                parsed.desc.trunk_thickness,
                TreeDesc::default().trunk_thickness
            );
            assert!(!toml::to_string(&parsed).unwrap().contains("garden_tree"));
        }
    }

    #[test]
    fn legacy_gui_config_without_tree_still_loads() {
        let parsed: GuiConfigFile = toml::from_str("schema_version = 1\nsection = []\n").unwrap();

        assert_eq!(parsed.tree, TreeGuiConfig::default());
    }

    #[test]
    fn gui_param_enabled_if_round_trip_preserves_condition() {
        let source = r#"
schema_version = 1

[[section]]
name = "Debug"

[[section.param]]
id = "dependent"
kind = "bool"
label = "Dependent"
enabled_if = { param = "controller", equals = true }
type = "Bool"

[section.param.data]
value = false
"#;

        let config: GuiConfigFile = toml::from_str(source).unwrap();
        let serialized = toml::to_string_pretty(&config).unwrap();
        let parsed: GuiConfigFile = toml::from_str(&serialized).unwrap();

        assert_eq!(
            parsed.section[0].param[0].enabled_if,
            Some(GuiParamEnabledIf {
                param: "controller".to_owned(),
                equals: GuiParamConditionValue::Bool(true),
            })
        );
    }
}
