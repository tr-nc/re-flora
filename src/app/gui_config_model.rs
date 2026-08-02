use crate::tree_gen::TreeDesc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GuiConfigFile {
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tree: Option<TreeGuiConfig>,
    pub section: Vec<GuiSection>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct TreeGuiConfig {
    #[serde(default = "default_true")]
    pub render_leaves: bool,
    #[serde(default)]
    pub desc: TreeDesc,
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
        let mut desc = TreeDesc::default();
        desc.size = 17.25;
        desc.branching.initial_length = 63.0;
        desc.leaf_density = 0.123;
        desc.fruit_swing_speed = 4.5;
        desc.subdivision_count_max = 8;
        let config = GuiConfigFile {
            schema_version: 1,
            tree: Some(TreeGuiConfig {
                render_leaves: false,
                desc: desc.clone(),
            }),
            section: Vec::new(),
        };

        let serialized = toml::to_string_pretty(&config).unwrap();
        let parsed: GuiConfigFile = toml::from_str(&serialized).unwrap();
        let parsed_tree = parsed.tree.unwrap();

        assert!(!parsed_tree.render_leaves);
        assert_eq!(parsed_tree.desc, desc);
    }

    #[test]
    fn legacy_gui_config_without_tree_still_loads() {
        let parsed: GuiConfigFile = toml::from_str("schema_version = 1\nsection = []\n").unwrap();

        assert!(parsed.tree.is_none());
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
