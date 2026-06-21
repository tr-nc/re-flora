use std::collections::{BTreeMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {{
        let message = format!($($arg)*);
        eprintln!("{}", message);
    }};
}

fn dump_env() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set");
    let target_dir = env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| {
        let default = Path::new(&manifest_dir).join("target");
        default.to_str().unwrap().to_owned()
    });
    println!("cargo:rustc-env=PROJECT_ROOT={}/", manifest_dir);
    println!("cargo:rustc-env=TARGET_DIR={}/", target_dir);
}

fn project_root() -> PathBuf {
    PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"))
}

fn kind_to_type(kind: &str, id: &str) -> &'static str {
    match kind {
        "float" => "crate::gui_adjustables::FloatParam",
        "int" => "crate::gui_adjustables::IntParam",
        "uint" => "crate::gui_adjustables::UintParam",
        "choice" => "crate::gui_adjustables::ChoiceParam",
        "string" => "crate::gui_adjustables::StringParam",
        "bool" => "crate::gui_adjustables::BoolParam",
        "color" => "crate::gui_adjustables::ColorParam",
        other => panic!(
            "GUI config generation failed: unsupported kind '{}' for param '{}'",
            other, id
        ),
    }
}

fn generate_gui_adjustables() {
    let root = project_root();
    let config_path = root.join("config").join("gui.toml");

    let content = fs::read_to_string(&config_path).unwrap_or_else(|e| {
        panic!(
            "GUI config generation failed: unable to read {}: {}",
            config_path.display(),
            e
        )
    });

    let parsed: toml::Value = toml::from_str(&content).unwrap_or_else(|e| {
        panic!(
            "GUI config generation failed: unable to parse {}: {}",
            config_path.display(),
            e
        )
    });

    let schema_version = parsed
        .get("schema_version")
        .and_then(|v| v.as_integer())
        .unwrap_or_else(|| {
            panic!(
                "GUI config generation failed: missing or invalid integer schema_version in {}",
                config_path.display()
            )
        });

    let mut descriptors: Vec<(String, String, String, String)> = Vec::new();
    let mut seen_sections = HashSet::new();
    let mut seen_ids = HashSet::new();
    let sections = parsed
        .get("section")
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| {
            panic!(
                "GUI config generation failed: missing or invalid [[section]] array in {}",
                config_path.display()
            )
        });

    for (section_idx, section) in sections.iter().enumerate() {
        let table = section.as_table().unwrap_or_else(|| {
            panic!(
                "GUI config generation failed: section at index {} is not a table",
                section_idx
            )
        });

        let section_name = table
            .get("name")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| {
                panic!(
                    "GUI config generation failed: section at index {} has missing/empty name",
                    section_idx
                )
            })
            .to_owned();

        if !seen_sections.insert(section_name.clone()) {
            panic!(
                "GUI config generation failed: duplicate section name '{}'",
                section_name
            );
        }

        let params = table
            .get("param")
            .and_then(|v| v.as_array())
            .unwrap_or_else(|| {
                panic!(
                    "GUI config generation failed: section '{}' has missing/invalid param array",
                    section_name
                )
            });

        for (param_idx, param) in params.iter().enumerate() {
            let param_tbl = param.as_table().unwrap_or_else(|| {
                panic!(
                    "GUI config generation failed: section '{}' param at index {} is not a table",
                    section_name, param_idx
                )
            });

            let id = param_tbl
                .get("id")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .unwrap_or_else(|| {
                    panic!(
                        "GUI config generation failed: section '{}' param at index {} missing/empty id",
                        section_name, param_idx
                    )
                })
                .to_owned();

            if !seen_ids.insert(id.clone()) {
                panic!("GUI config generation failed: duplicate param id '{}'", id);
            }

            let kind = param_tbl
                .get("kind")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|kind| !kind.is_empty())
                .unwrap_or_else(|| {
                    panic!(
                        "GUI config generation failed: section '{}' param '{}' missing/empty kind",
                        section_name, id
                    )
                })
                .to_owned();
            let _ = kind_to_type(&kind, &id);

            let label = param_tbl
                .get("label")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|label| !label.is_empty())
                .unwrap_or_else(|| {
                    panic!(
                        "GUI config generation failed: section '{}' param '{}' missing/empty label",
                        section_name, id
                    )
                })
                .to_owned();

            descriptors.push((section_name.clone(), id, kind, label));
        }
    }

    let generated_dir = root.join("src").join("app").join("generated");
    fs::create_dir_all(&generated_dir).unwrap_or_else(|e| {
        panic!(
            "GUI config generation failed: unable to create {}: {}",
            generated_dir.display(),
            e
        )
    });

    let out_path = generated_dir.join("gui_adjustables_gen.rs");

    let mut code = String::new();
    code.push_str(
        "// ============================================================================\n",
    );
    code.push_str("// !!! DO NOT EDIT THIS FILE BY HAND !!!\n");
    code.push_str("// This file is generated at build time.\n");
    code.push_str("//\n");
    code.push_str("// generator: build.rs::generate_gui_adjustables\n");
    code.push_str("// source: config/gui.toml\n");
    code.push_str("//\n");
    code.push_str("// To regenerate this file, run a Cargo build command, for example:\n");
    code.push_str("//   cargo check\n");
    code.push_str(
        "// ============================================================================\n",
    );
    code.push_str("// @generated by build.rs - do not edit.\n");
    code.push_str("// This file reflects config/gui.toml at build time.\n\n");

    code.push_str("#[allow(dead_code)]\n");
    code.push_str("pub const GENERATED_SCHEMA_VERSION: u32 = ");
    code.push_str(&schema_version.to_string());
    code.push_str(";\n\n");

    code.push_str("#[allow(dead_code)]\n");
    code.push_str("pub struct GeneratedGuiParamDescriptor {\n");
    code.push_str("    pub section: &'static str,\n");
    code.push_str("    pub id: &'static str,\n");
    code.push_str("    pub kind: &'static str,\n");
    code.push_str("    pub label: &'static str,\n");
    code.push_str("}\n\n");

    code.push_str("#[allow(dead_code)]\n");
    code.push_str("pub static GENERATED_GUI_PARAMS: &[GeneratedGuiParamDescriptor] = &[\n");

    for (section, id, kind, label) in &descriptors {
        code.push_str("    GeneratedGuiParamDescriptor {\n");
        code.push_str(&format!(
            "        section: \"{}\",\n",
            section.replace('"', "\\\"")
        ));
        code.push_str(&format!("        id: \"{}\",\n", id.replace('"', "\\\"")));
        code.push_str(&format!(
            "        kind: \"{}\",\n",
            kind.replace('"', "\\\"")
        ));
        code.push_str(&format!(
            "        label: \"{}\",\n",
            label.replace('"', "\\\"")
        ));
        code.push_str("    },\n");
    }

    code.push_str("];\n\n");

    // generated struct with one field per GUI param
    code.push_str("#[allow(dead_code)]\n");
    code.push_str("pub struct GuiAdjustables {\n");
    for (_section, id, kind, _label) in &descriptors {
        let ty = kind_to_type(kind, id);

        code.push_str(&format!("    pub {}: {},\n", id, ty));
    }
    code.push_str("}\n\n");

    // Default implementation that loads the config file
    code.push_str("impl Default for GuiAdjustables {\n");
    code.push_str("    fn default() -> Self {\n");
    code.push_str("        let config = crate::app::gui_config_loader::GuiConfigLoader::load();\n");
    code.push_str("        Self::from_config(&config)\n");
    code.push_str("    }\n");
    code.push_str("}\n\n");

    // from_config constructor that materializes params from GuiConfigFile
    code.push_str("impl GuiAdjustables {\n");
    code.push_str(
        "    pub fn from_config(config: &crate::app::gui_config_model::GuiConfigFile) -> Self {\n",
    );
    code.push_str("        use crate::app::gui_config_model::{GuiParamKind, GuiParamValue};\n\n");

    // one local Option per field
    for (_section, id, kind, _label) in &descriptors {
        let ty = kind_to_type(kind, id);
        code.push_str(&format!(
            "        let mut {id}_field: Option<{ty}> = None;\n",
            id = id,
            ty = ty
        ));
    }

    code.push_str("\n        for section in &config.section {\n");
    code.push_str("            for param in &section.param {\n");
    code.push_str("                match param.id.as_str() {\n");

    for (_section, id, kind, _label) in &descriptors {
        code.push_str(&format!("                    \"{}\" => {{\n", id));
        match kind.as_str() {
            "float" => {
                code.push_str(
                    "                        if let (GuiParamKind::Float, GuiParamValue::Float { value, min, max }) = (&param.kind, &param.value) {\n",
                );
                code.push_str(
                    "                            let min = min.unwrap_or(0.0);\n                            let max = max.unwrap_or(1.0);\n",
                );
                code.push_str(&format!(
                    "                            {id}_field = Some(crate::gui_adjustables::FloatParam::new(*value, min..=max));\n",
                    id = id
                ));
                code.push_str("                        }\n");
            }
            "int" => {
                code.push_str(
                    "                        if let (GuiParamKind::Int, GuiParamValue::Int { value, min, max }) = (&param.kind, &param.value) {\n",
                );
                code.push_str(
                    "                            let min = min.unwrap_or(0);\n                            let max = max.unwrap_or(100);\n",
                );
                code.push_str(&format!(
                    "                            {id}_field = Some(crate::gui_adjustables::IntParam::new(*value, min..=max));\n",
                    id = id
                ));
                code.push_str("                        }\n");
            }
            "uint" => {
                code.push_str(
                    "                        if let (GuiParamKind::Uint, GuiParamValue::Uint { value, min, max }) = (&param.kind, &param.value) {\n",
                );
                code.push_str(
                    "                            let min = min.unwrap_or(0);\n                            let max = max.unwrap_or(100);\n",
                );
                code.push_str(&format!(
                    "                            {id}_field = Some(crate::gui_adjustables::UintParam::new(*value, min..=max));\n",
                    id = id
                ));
                code.push_str("                        }\n");
            }
            "choice" => {
                code.push_str(
                    "                        if let (GuiParamKind::Choice, GuiParamValue::Choice { value, .. }) = (&param.kind, &param.value) {\n",
                );
                code.push_str(&format!(
                    "                            {id}_field = Some(crate::gui_adjustables::ChoiceParam::new(*value));\n",
                    id = id
                ));
                code.push_str("                        }\n");
            }
            "string" => {
                code.push_str(
                    "                        if let (GuiParamKind::String, GuiParamValue::String { value }) = (&param.kind, &param.value) {\n",
                );
                code.push_str(&format!(
                    "                            {id}_field = Some(crate::gui_adjustables::StringParam::new(value.clone()));\n",
                    id = id
                ));
                code.push_str("                        }\n");
            }
            "bool" => {
                code.push_str(
                    "                        if let (GuiParamKind::Bool, GuiParamValue::Bool { value }) = (&param.kind, &param.value) {\n",
                );
                code.push_str(&format!(
                    "                            {id}_field = Some(crate::gui_adjustables::BoolParam::new(*value));\n",
                    id = id
                ));
                code.push_str("                        }\n");
            }
            "color" => {
                code.push_str(
                    "                        if let (GuiParamKind::Color, GuiParamValue::Color { value }) = (&param.kind, &param.value) {\n",
                );
                code.push_str(&format!(
                    "                            {id}_field = Some(crate::gui_adjustables::ColorParam::new(crate::app::gui_config::parse_color(value)));\n",
                    id = id
                ));
                code.push_str("                        }\n");
            }
            _ => {}
        }
        code.push_str("                    }\n");
    }

    code.push_str("                    _ => {}\n");
    code.push_str("                }\n");
    code.push_str("            }\n");
    code.push_str("        }\n\n");

    code.push_str("        GuiAdjustables {\n");
    for (_section, id, kind, _label) in &descriptors {
        let _ = kind_to_type(kind, id);
        code.push_str(&format!(
            "            {id}: {id}_field.expect(\"Missing parameter: {id}\"),\n",
            id = id
        ));
    }
    code.push_str("        }\n");
    code.push_str("    }\n");
    code.push_str("}\n\n");

    // generated accessors operating on GuiAdjustables by id
    code.push_str("#[allow(dead_code)]\n");
    code.push_str(
        "pub fn get_float_param<'a>(adjustables: &'a crate::app::GuiAdjustables, id: &str) -> Option<&'a crate::gui_adjustables::FloatParam> {\n",
    );
    code.push_str("    match id {\n");
    for (_section, id, kind, _label) in &descriptors {
        if kind == "float" {
            code.push_str(&format!(
                "        \"{}\" => Some(&adjustables.{}),\n",
                id, id
            ));
        }
    }
    code.push_str("        _ => None,\n");
    code.push_str("    }\n");
    code.push_str("}\n\n");

    code.push_str("#[allow(dead_code)]\n");
    code.push_str(
        "pub fn get_int_param<'a>(adjustables: &'a crate::app::GuiAdjustables, id: &str) -> Option<&'a crate::gui_adjustables::IntParam> {\n",
    );
    code.push_str("    match id {\n");
    for (_section, id, kind, _label) in &descriptors {
        if kind == "int" {
            code.push_str(&format!(
                "        \"{}\" => Some(&adjustables.{}),\n",
                id, id
            ));
        }
    }
    code.push_str("        _ => None,\n");
    code.push_str("    }\n");
    code.push_str("}\n\n");

    code.push_str("#[allow(dead_code)]\n");
    code.push_str(
        "pub fn get_uint_param<'a>(adjustables: &'a crate::app::GuiAdjustables, id: &str) -> Option<&'a crate::gui_adjustables::UintParam> {\n",
    );
    code.push_str("    match id {\n");
    for (_section, id, kind, _label) in &descriptors {
        if kind == "uint" {
            code.push_str(&format!(
                "        \"{}\" => Some(&adjustables.{}),\n",
                id, id
            ));
        }
    }
    code.push_str("        _ => None,\n");
    code.push_str("    }\n");
    code.push_str("}\n\n");

    code.push_str("#[allow(dead_code, unused_variables)]\n");
    code.push_str(
        "pub fn get_choice_param<'a>(adjustables: &'a crate::app::GuiAdjustables, id: &str) -> Option<&'a crate::gui_adjustables::ChoiceParam> {\n",
    );
    if descriptors
        .iter()
        .any(|(_section, _id, kind, _label)| kind == "choice")
    {
        code.push_str("    match id {\n");
        for (_section, id, kind, _label) in &descriptors {
            if kind == "choice" {
                code.push_str(&format!(
                    "        \"{}\" => Some(&adjustables.{}),\n",
                    id, id
                ));
            }
        }
        code.push_str("        _ => None,\n");
        code.push_str("    }\n");
    } else {
        code.push_str("    None\n");
    }
    code.push_str("}\n\n");

    code.push_str("#[allow(dead_code, unused_variables)]\n");
    code.push_str(
        "pub fn get_string_param<'a>(adjustables: &'a crate::app::GuiAdjustables, id: &str) -> Option<&'a crate::gui_adjustables::StringParam> {\n",
    );
    if descriptors
        .iter()
        .any(|(_section, _id, kind, _label)| kind == "string")
    {
        code.push_str("    match id {\n");
        for (_section, id, kind, _label) in &descriptors {
            if kind == "string" {
                code.push_str(&format!(
                    "        \"{}\" => Some(&adjustables.{}),\n",
                    id, id
                ));
            }
        }
        code.push_str("        _ => None,\n");
        code.push_str("    }\n");
    } else {
        code.push_str("    None\n");
    }
    code.push_str("}\n\n");

    code.push_str("#[allow(dead_code)]\n");
    code.push_str(
        "pub fn get_bool_param<'a>(adjustables: &'a crate::app::GuiAdjustables, id: &str) -> Option<&'a crate::gui_adjustables::BoolParam> {\n",
    );
    code.push_str("    match id {\n");
    for (_section, id, kind, _label) in &descriptors {
        if kind == "bool" {
            code.push_str(&format!(
                "        \"{}\" => Some(&adjustables.{}),\n",
                id, id
            ));
        }
    }
    code.push_str("        _ => None,\n");
    code.push_str("    }\n");
    code.push_str("}\n\n");

    code.push_str("#[allow(dead_code)]\n");
    code.push_str(
        "pub fn get_color_param<'a>(adjustables: &'a crate::app::GuiAdjustables, id: &str) -> Option<&'a crate::gui_adjustables::ColorParam> {\n",
    );
    code.push_str("    match id {\n");
    for (_section, id, kind, _label) in &descriptors {
        if kind == "color" {
            code.push_str(&format!(
                "        \"{}\" => Some(&adjustables.{}),\n",
                id, id
            ));
        }
    }
    code.push_str("        _ => None,\n");
    code.push_str("    }\n");
    code.push_str("}\n\n");

    code.push_str("#[allow(dead_code)]\n");
    code.push_str(
        "pub fn get_float_param_mut<'a>(adjustables: &'a mut crate::app::GuiAdjustables, id: &str) -> Option<&'a mut crate::gui_adjustables::FloatParam> {\n",
    );
    code.push_str("    match id {\n");
    for (_section, id, kind, _label) in &descriptors {
        if kind == "float" {
            code.push_str(&format!(
                "        \"{}\" => Some(&mut adjustables.{}),\n",
                id, id
            ));
        }
    }
    code.push_str("        _ => None,\n");
    code.push_str("    }\n");
    code.push_str("}\n\n");

    code.push_str("#[allow(dead_code)]\n");
    code.push_str(
        "pub fn get_int_param_mut<'a>(adjustables: &'a mut crate::app::GuiAdjustables, id: &str) -> Option<&'a mut crate::gui_adjustables::IntParam> {\n",
    );
    code.push_str("    match id {\n");
    for (_section, id, kind, _label) in &descriptors {
        if kind == "int" {
            code.push_str(&format!(
                "        \"{}\" => Some(&mut adjustables.{}),\n",
                id, id
            ));
        }
    }
    code.push_str("        _ => None,\n");
    code.push_str("    }\n");
    code.push_str("}\n\n");

    code.push_str("#[allow(dead_code)]\n");
    code.push_str(
        "pub fn get_uint_param_mut<'a>(adjustables: &'a mut crate::app::GuiAdjustables, id: &str) -> Option<&'a mut crate::gui_adjustables::UintParam> {\n",
    );
    code.push_str("    match id {\n");
    for (_section, id, kind, _label) in &descriptors {
        if kind == "uint" {
            code.push_str(&format!(
                "        \"{}\" => Some(&mut adjustables.{}),\n",
                id, id
            ));
        }
    }
    code.push_str("        _ => None,\n");
    code.push_str("    }\n");
    code.push_str("}\n\n");

    code.push_str("#[allow(dead_code, unused_variables)]\n");
    code.push_str(
        "pub fn get_choice_param_mut<'a>(adjustables: &'a mut crate::app::GuiAdjustables, id: &str) -> Option<&'a mut crate::gui_adjustables::ChoiceParam> {\n",
    );
    if descriptors
        .iter()
        .any(|(_section, _id, kind, _label)| kind == "choice")
    {
        code.push_str("    match id {\n");
        for (_section, id, kind, _label) in &descriptors {
            if kind == "choice" {
                code.push_str(&format!(
                    "        \"{}\" => Some(&mut adjustables.{}),\n",
                    id, id
                ));
            }
        }
        code.push_str("        _ => None,\n");
        code.push_str("    }\n");
    } else {
        code.push_str("    None\n");
    }
    code.push_str("}\n\n");

    code.push_str("#[allow(dead_code, unused_variables)]\n");
    code.push_str(
        "pub fn get_string_param_mut<'a>(adjustables: &'a mut crate::app::GuiAdjustables, id: &str) -> Option<&'a mut crate::gui_adjustables::StringParam> {\n",
    );
    if descriptors
        .iter()
        .any(|(_section, _id, kind, _label)| kind == "string")
    {
        code.push_str("    match id {\n");
        for (_section, id, kind, _label) in &descriptors {
            if kind == "string" {
                code.push_str(&format!(
                    "        \"{}\" => Some(&mut adjustables.{}),\n",
                    id, id
                ));
            }
        }
        code.push_str("        _ => None,\n");
        code.push_str("    }\n");
    } else {
        code.push_str("    None\n");
    }
    code.push_str("}\n\n");

    code.push_str("#[allow(dead_code)]\n");
    code.push_str(
        "pub fn get_bool_param_mut<'a>(adjustables: &'a mut crate::app::GuiAdjustables, id: &str) -> Option<&'a mut crate::gui_adjustables::BoolParam> {\n",
    );
    code.push_str("    match id {\n");
    for (_section, id, kind, _label) in &descriptors {
        if kind == "bool" {
            code.push_str(&format!(
                "        \"{}\" => Some(&mut adjustables.{}),\n",
                id, id
            ));
        }
    }
    code.push_str("        _ => None,\n");
    code.push_str("    }\n");
    code.push_str("}\n\n");

    code.push_str("#[allow(dead_code)]\n");
    code.push_str(
        "pub fn get_color_param_mut<'a>(adjustables: &'a mut crate::app::GuiAdjustables, id: &str) -> Option<&'a mut crate::gui_adjustables::ColorParam> {\n",
    );
    code.push_str("    match id {\n");
    for (_section, id, kind, _label) in &descriptors {
        if kind == "color" {
            code.push_str(&format!(
                "        \"{}\" => Some(&mut adjustables.{}),\n",
                id, id
            ));
        }
    }
    code.push_str("        _ => None,\n");
    code.push_str("    }\n");
    code.push_str("}\n");

    fs::write(&out_path, code).unwrap_or_else(|e| {
        panic!(
            "GUI config generation failed: unable to write {}: {}",
            out_path.display(),
            e
        )
    });
    log!("wrote generated GUI descriptors to {}", out_path.display());
}

// ============================================================================
// gpu_structs codegen - phase 1
// ============================================================================

/// All GLSL shader source files to reflect, relative to the project root.
const SHADER_FILES: &[(&str, shaderc::ShaderKind)] = &[
    (
        "shader/builder/chunk_writer/buffer_setup.comp",
        shaderc::ShaderKind::Compute,
    ),
    (
        "shader/builder/chunk_writer/chunk_modify.comp",
        shaderc::ShaderKind::Compute,
    ),
    (
        "shader/builder/chunk_writer/chunk_modify_sample.comp",
        shaderc::ShaderKind::Compute,
    ),
    (
        "shader/builder/chunk_writer/chunk_solid_sample.comp",
        shaderc::ShaderKind::Compute,
    ),
    (
        "shader/builder/chunk_writer/model_voxelize.comp",
        shaderc::ShaderKind::Compute,
    ),
    (
        "shader/builder/contree/buffer_setup.comp",
        shaderc::ShaderKind::Compute,
    ),
    (
        "shader/builder/scene_accel/update_scene_tex.comp",
        shaderc::ShaderKind::Compute,
    ),
    (
        "shader/builder/surface/clear_occupancy.comp",
        shaderc::ShaderKind::Compute,
    ),
    (
        "shader/builder/surface/edit_occupancy_capsule.comp",
        shaderc::ShaderKind::Compute,
    ),
    (
        "shader/builder/surface/instances_to_occupancy.comp",
        shaderc::ShaderKind::Compute,
    ),
    (
        "shader/builder/surface/make_surface.comp",
        shaderc::ShaderKind::Compute,
    ),
    (
        "shader/builder/surface/occupancy_to_flora_instances.comp",
        shaderc::ShaderKind::Compute,
    ),
    ("shader/tracer/tracer.comp", shaderc::ShaderKind::Compute),
    (
        "shader/tracer/tracer_shadow.comp",
        shaderc::ShaderKind::Compute,
    ),
    (
        "shader/tracer/leaf_shadow_temporal.comp",
        shaderc::ShaderKind::Compute,
    ),
    (
        "shader/tracer/leaf_shadow_mask.comp",
        shaderc::ShaderKind::Compute,
    ),
    (
        "shader/tracer/vsm_blur_h.comp",
        shaderc::ShaderKind::Compute,
    ),
    (
        "shader/tracer/vsm_blur_v.comp",
        shaderc::ShaderKind::Compute,
    ),
    (
        "shader/tracer/composition.comp",
        shaderc::ShaderKind::Compute,
    ),
    ("shader/tracer/god_ray.comp", shaderc::ShaderKind::Compute),
    (
        "shader/tracer/post_processing.comp",
        shaderc::ShaderKind::Compute,
    ),
    (
        "shader/tracer/player_collider.comp",
        shaderc::ShaderKind::Compute,
    ),
    (
        "shader/tracer/terrain_query.comp",
        shaderc::ShaderKind::Compute,
    ),
    (
        "shader/tracer/wind_volume.comp",
        shaderc::ShaderKind::Compute,
    ),
    (
        "shader/denoiser/temporal.comp",
        shaderc::ShaderKind::Compute,
    ),
    ("shader/denoiser/spatial.comp", shaderc::ShaderKind::Compute),
    ("shader/foliage/flora.vert", shaderc::ShaderKind::Vertex),
    ("shader/foliage/flora_lod.vert", shaderc::ShaderKind::Vertex),
    (
        "shader/foliage/leaves_shadow.vert",
        shaderc::ShaderKind::Vertex,
    ),
];

// ---- type model (mirrors the runtime PlainMemberType) ----------------------

#[derive(Debug, Clone, PartialEq)]
enum FieldType {
    Int,
    UInt,
    Int64,
    UInt64,
    Float,
    Vec2,
    Vec3,
    Vec4,
    IVec2,
    IVec3,
    IVec4,
    UVec2,
    UVec3,
    UVec4,
    Mat2,
    Mat3,
    Mat4,
    Mat3x4,
    Array,
}

#[derive(Debug, Clone)]
struct PlainField {
    name: String,
    ty: FieldType,
    offset: u32,
    #[allow(dead_code)]
    size: u32,
    padded_size: u32,
}

#[derive(Debug, Clone)]
struct StructLayout {
    /// Type name as in GLSL (e.g. `U_CameraInfo`)
    type_name: String,
    /// Ordered by offset
    fields: Vec<PlainField>,
    /// Total size in bytes (offset of last field + its padded_size)
    total_size: u32,
}

// ---- shaderc include callback (mirrors compiler.rs) -------------------------

fn build_include_callback(
    requested_source: &str,
    include_type: shaderc::IncludeType,
    requesting_source: &str,
    _depth: usize,
) -> Result<shaderc::ResolvedInclude, String> {
    let base = match include_type {
        shaderc::IncludeType::Relative => Path::new(requesting_source)
            .parent()
            .ok_or_else(|| format!("{requesting_source} has no parent"))?
            .to_owned(),
        shaderc::IncludeType::Standard => {
            return Err("standard includes not supported".into());
        }
    };
    let full_path = base
        .join(requested_source)
        .canonicalize()
        .map_err(|e| format!("{requested_source}: {e}"))?;
    let content =
        std::fs::read_to_string(&full_path).map_err(|e| format!("{}: {e}", full_path.display()))?;
    Ok(shaderc::ResolvedInclude {
        resolved_name: full_path.to_string_lossy().into_owned(),
        content,
    })
}

// ---- spirv-reflect helpers --------------------------------------------------

use spirv_reflect::types::{ReflectDescriptorType, ReflectTypeFlags};

fn reflect_field_type(
    type_flags: &ReflectTypeFlags,
    traits: &spirv_reflect::types::ReflectTypeDescriptionTraits,
    size: u32,
) -> Option<FieldType> {
    if type_flags.contains(ReflectTypeFlags::ARRAY) {
        return Some(FieldType::Array);
    }
    if type_flags.contains(ReflectTypeFlags::MATRIX) {
        let cols = traits.numeric.matrix.column_count;
        let rows = traits.numeric.matrix.row_count;
        return match (rows, cols) {
            (4, 4) => Some(FieldType::Mat4),
            (3, 3) => Some(FieldType::Mat3),
            (2, 2) => Some(FieldType::Mat2),
            (4, 3) => Some(FieldType::Mat3x4),
            _ => None,
        };
    }
    if type_flags.contains(ReflectTypeFlags::VECTOR) {
        let n = traits.numeric.vector.component_count;
        let is_float = type_flags.contains(ReflectTypeFlags::FLOAT);
        let is_int = type_flags.contains(ReflectTypeFlags::INT);
        let signed = traits.numeric.scalar.signedness == 1;
        if is_float {
            return match n {
                2 => Some(FieldType::Vec2),
                3 => Some(FieldType::Vec3),
                4 => Some(FieldType::Vec4),
                _ => None,
            };
        }
        if is_int {
            if signed {
                return match n {
                    2 => Some(FieldType::IVec2),
                    3 => Some(FieldType::IVec3),
                    4 => Some(FieldType::IVec4),
                    _ => None,
                };
            } else {
                return match n {
                    2 => Some(FieldType::UVec2),
                    3 => Some(FieldType::UVec3),
                    4 => Some(FieldType::UVec4),
                    _ => None,
                };
            }
        }
    }
    if type_flags.contains(ReflectTypeFlags::FLOAT) {
        return Some(FieldType::Float);
    }
    if type_flags.contains(ReflectTypeFlags::INT) {
        let signed = traits.numeric.scalar.signedness == 1;
        return match size {
            4 => Some(if signed {
                FieldType::Int
            } else {
                FieldType::UInt
            }),
            8 => Some(if signed {
                FieldType::Int64
            } else {
                FieldType::UInt64
            }),
            _ => None,
        };
    }
    None
}

fn flatten_block_members(
    members: &[spirv_reflect::types::ReflectBlockVariable],
    fields: &mut Vec<PlainField>,
) {
    for m in members {
        let td = match &m.type_description {
            Some(td) => td,
            None => continue,
        };
        if td.type_flags.contains(ReflectTypeFlags::STRUCT) {
            // recurse into nested struct, members share the parent offset space
            flatten_block_members(&m.members, fields);
        } else {
            let ty = match reflect_field_type(&td.type_flags, &td.traits, m.size) {
                Some(t) => t,
                None => continue,
            };
            fields.push(PlainField {
                name: m.name.clone(),
                ty,
                offset: m.offset,
                size: m.size,
                padded_size: m.padded_size,
            });
        }
    }
}

fn to_pascal_case(value: &str) -> String {
    let mut out = String::new();
    for part in value.split(|c: char| !c.is_ascii_alphanumeric()) {
        if part.is_empty() {
            continue;
        }
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            out.push(first.to_ascii_uppercase());
            out.push_str(chars.as_str());
        }
    }
    if out.is_empty() {
        "Generated".to_owned()
    } else {
        out
    }
}

fn push_constant_type_name(path: &str) -> String {
    let stem = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("generated");
    format!("PushConstant{}", to_pascal_case(stem))
}

fn reflect_shader(source: &str, kind: shaderc::ShaderKind, path: &str) -> Vec<StructLayout> {
    let compiler = shaderc::Compiler::new().expect("shaderc compiler");
    let mut opts = shaderc::CompileOptions::new().expect("shaderc options");
    opts.set_target_env(
        shaderc::TargetEnv::Vulkan,
        shaderc::EnvVersion::Vulkan1_3 as u32,
    );
    opts.set_target_spirv(shaderc::SpirvVersion::V1_6);
    opts.set_source_language(shaderc::SourceLanguage::GLSL);
    opts.set_optimization_level(shaderc::OptimizationLevel::Zero);
    opts.set_include_callback(build_include_callback);

    let artifact = match compiler.compile_into_spirv(source, kind, path, "main", Some(&opts)) {
        Ok(a) => a,
        Err(e) => {
            // emit a warning but don't abort; partial failures shouldn't break the build
            println!("cargo:warning=gpu_structs codegen: failed to compile {path}: {e}");
            return Vec::new();
        }
    };

    let spirv_bytes = artifact.as_binary_u8();
    let module = match spirv_reflect::ShaderModule::load_u8_data(spirv_bytes) {
        Ok(m) => m,
        Err(e) => {
            println!("cargo:warning=gpu_structs codegen: failed to reflect {path}: {e}");
            return Vec::new();
        }
    };

    let bindings = match module.enumerate_descriptor_bindings(None) {
        Ok(b) => b,
        Err(_) => return Vec::new(),
    };

    let mut layouts = Vec::new();
    for binding in bindings {
        let is_buf = binding.descriptor_type == ReflectDescriptorType::UniformBuffer
            || binding.descriptor_type == ReflectDescriptorType::StorageBuffer;
        if !is_buf {
            continue;
        }
        let type_name = match &binding.type_description {
            Some(td) => td.type_name.clone(),
            None => continue,
        };
        // skip pure GPU-internal read-only storage buffers that the CPU never writes
        // (contree, terrain query info, scene tex – identified by `B_Contree*`, `B_Scene*`)
        // We still include B_PlayerCollisionResult (CPU reads it back).
        if type_name.starts_with("B_Contree") || type_name == "B_SceneTex" {
            continue;
        }
        // skip image/sampler bindings that sneak through
        if type_name.is_empty() {
            continue;
        }

        let mut fields: Vec<PlainField> = Vec::new();
        flatten_block_members(&binding.block.members, &mut fields);
        // sort by offset so the struct fields are in layout order
        fields.sort_by_key(|f| f.offset);

        if fields.is_empty() {
            continue;
        }

        let total_size = fields
            .iter()
            .map(|f| f.offset + f.padded_size)
            .max()
            .unwrap_or(0);

        layouts.push(StructLayout {
            type_name,
            fields,
            total_size,
        });
    }

    if let Ok(push_blocks) = module.enumerate_push_constant_blocks(None) {
        for block in push_blocks {
            let mut fields: Vec<PlainField> = Vec::new();
            flatten_block_members(&block.members, &mut fields);
            fields.sort_by_key(|f| f.offset);
            if fields.is_empty() {
                continue;
            }
            let total_size = fields
                .iter()
                .map(|f| f.offset + f.padded_size)
                .max()
                .unwrap_or(0)
                .max(block.size);
            layouts.push(StructLayout {
                type_name: push_constant_type_name(path),
                fields,
                total_size,
            });
        }
    }
    layouts
}

// ---- Rust type for each FieldType ------------------------------------------

fn rust_field_type(ty: &FieldType) -> &'static str {
    match ty {
        FieldType::Int => "i32",
        FieldType::UInt => "u32",
        FieldType::Int64 => "i64",
        FieldType::UInt64 => "u64",
        FieldType::Float => "f32",
        FieldType::Vec2 => "[f32; 2]",
        FieldType::Vec3 => "[f32; 3]",
        FieldType::Vec4 => "[f32; 4]",
        FieldType::IVec2 => "[i32; 2]",
        FieldType::IVec3 => "[i32; 3]",
        FieldType::IVec4 => "[i32; 4]",
        FieldType::UVec2 => "[u32; 2]",
        FieldType::UVec3 => "[u32; 3]",
        FieldType::UVec4 => "[u32; 4]",
        FieldType::Mat2 => "[[f32; 2]; 2]",
        FieldType::Mat3 => "[[f32; 3]; 3]",
        FieldType::Mat4 => "[[f32; 4]; 4]",
        FieldType::Mat3x4 => "[[f32; 4]; 3]",
        FieldType::Array => "[u32; 1]", // placeholder; caller handles real arrays separately
    }
}

fn field_size(ty: &FieldType) -> u32 {
    match ty {
        FieldType::Int | FieldType::UInt | FieldType::Float => 4,
        FieldType::Int64 | FieldType::UInt64 => 8,
        FieldType::Vec2 | FieldType::IVec2 | FieldType::UVec2 => 8,
        FieldType::Vec3 | FieldType::IVec3 | FieldType::UVec3 => 12,
        FieldType::Vec4 | FieldType::IVec4 | FieldType::UVec4 => 16,
        FieldType::Mat2 => 16,
        FieldType::Mat3 => 36,
        FieldType::Mat4 => 64,
        FieldType::Mat3x4 => 48,
        FieldType::Array => 4,
    }
}

/// Strip the `U_` / `B_` prefix and convert `PascalCase` from `CamelCase`.
/// e.g. `U_CameraInfo` -> `CameraInfo`, `B_PlayerCollisionResult` -> `PlayerCollisionResult`
fn struct_name(glsl_type_name: &str) -> String {
    let stripped = glsl_type_name
        .strip_prefix("U_")
        .or_else(|| glsl_type_name.strip_prefix("B_"))
        .unwrap_or(glsl_type_name);
    stripped.to_owned()
}

// ---- code emitter -----------------------------------------------------------

fn emit_struct(layout: &StructLayout) -> String {
    let name = struct_name(&layout.type_name);
    let mut code = String::new();

    code.push_str(&format!(
        "/// Auto-generated from `{}` (GLSL source of truth).\n",
        layout.type_name
    ));
    code.push_str("#[repr(C)]\n");
    code.push_str("#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]\n");
    code.push_str(&format!("pub struct {} {{\n", name));

    let mut cursor: u32 = 0;
    let mut pad_idx: u32 = 0;

    for field in &layout.fields {
        // insert padding gap if needed
        if field.offset > cursor {
            let gap = field.offset - cursor;
            code.push_str(&format!("    pub _pad{}: [u8; {}],\n", pad_idx, gap));
            pad_idx += 1;
            cursor += gap;
        }

        // for Array fields we use the actual padded_size to know how many u32s
        if field.ty == FieldType::Array {
            let count = field.padded_size / 4;
            code.push_str(&format!("    pub {}: [u32; {}],\n", field.name, count));
            cursor += field.padded_size;
        } else {
            code.push_str(&format!(
                "    pub {}: {},\n",
                field.name,
                rust_field_type(&field.ty)
            ));
            let actual = field_size(&field.ty);
            cursor += actual;
            // If the GPU layout pads this field further, emit explicit trailing padding bytes.
            // This is required because #[repr(C)] does not insert implicit holes between fields.
            if field.padded_size > actual {
                let trail = field.padded_size - actual;
                code.push_str(&format!("    pub _pad{}: [u8; {}],\n", pad_idx, trail));
                pad_idx += 1;
                cursor += trail;
            }
        }
    }

    // trailing padding to reach total_size
    if layout.total_size > cursor {
        let gap = layout.total_size - cursor;
        code.push_str(&format!("    pub _pad{}: [u8; {}],\n", pad_idx, gap));
    }

    code.push_str("}\n\n");
    code
}

fn generate_gpu_structs() {
    let root = project_root();
    let shader_root = root.join("shader");
    let out_dir = root.join("src").join("auto-generated");
    fs::create_dir_all(&out_dir).expect("create src/auto-generated");

    // mark all shader source files as inputs so cargo reruns when they change
    for (rel, _) in SHADER_FILES {
        println!("cargo:rerun-if-changed={}", root.join(rel).display());
    }
    // also watch the shader include directory
    println!(
        "cargo:rerun-if-changed={}",
        shader_root.join("include").display()
    );

    // collect all layouts across all shaders, deduplicating by type name
    // BTreeMap for deterministic output order
    let mut all_layouts: BTreeMap<String, StructLayout> = BTreeMap::new();

    for (rel, kind) in SHADER_FILES {
        let path = root.join(rel);
        let source = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                println!(
                    "cargo:warning=gpu_structs codegen: cannot read {}: {e}",
                    path.display()
                );
                continue;
            }
        };
        let layouts = reflect_shader(&source, *kind, &path.to_string_lossy());
        for layout in layouts {
            let existing = all_layouts
                .entry(layout.type_name.clone())
                .or_insert(layout.clone());
            // verify identical layout if the same name appears in multiple shaders
            if existing.total_size != layout.total_size {
                println!(
                    "cargo:warning=gpu_structs codegen: layout mismatch for `{}`: \
                     {} bytes vs {} bytes",
                    layout.type_name, existing.total_size, layout.total_size
                );
            }
        }
    }

    let out_path = out_dir.join("gpu_structs.rs");
    let mut code = String::new();
    code.push_str(
        "// ============================================================================\n",
    );
    code.push_str("// !!! DO NOT EDIT THIS FILE BY HAND !!!\n");
    code.push_str("// Generated by build.rs::generate_gpu_structs from GLSL shader sources.\n");
    code.push_str(
        "// ============================================================================\n\n",
    );
    code.push_str("#![allow(dead_code, non_snake_case)]\n\n");

    for layout in all_layouts.values() {
        code.push_str(&emit_struct(layout));
    }

    // Strip trailing blank line so rustfmt --check passes on the generated file.
    while code.ends_with("\n\n") {
        code.pop();
    }

    fs::write(&out_path, &code).expect("write gpu_structs.rs");
    log!(
        "gpu_structs codegen: wrote {} structs to {}",
        all_layouts.len(),
        out_path.display()
    );
}

#[cfg(target_os = "macos")]
// adds runtime rpath for bundled native libraries when running outside cargo.
fn add_platform_runtime_rpath() {
    // Add rpath for libphonon.dylib so the binary works when run directly
    // (not just via `cargo run` which sets DYLD_LIBRARY_PATH).
    // audionimbus-sys downloads libphonon into its OUT_DIR/lib during build.
    let out_dir = env::var("OUT_DIR").unwrap_or_default();
    if !out_dir.is_empty() {
        // Walk up from our OUT_DIR to find the audionimbus-sys build output
        let build_dir = Path::new(&out_dir)
            .ancestors()
            .nth(2) // OUT_DIR is target/<profile>/build/<pkg>/out → go up to build/
            .unwrap_or(Path::new("."));
        for entry in fs::read_dir(build_dir).into_iter().flatten().flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with("audionimbus-sys-") {
                let lib_dir = entry.path().join("out").join("lib");
                if lib_dir.join("libphonon.dylib").exists() {
                    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
                    break;
                }
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
// adds runtime rpath for bundled native libraries when running outside cargo.
fn add_platform_runtime_rpath() {
    // TODO: add platform-specific runtime library search path handling if needed.
}

fn main() {
    // Tell Cargo to rerun this script if these files/directories change.
    // config/gui.toml drives GuiAdjustables codegen.
    println!("cargo:rerun-if-changed=config/gui.toml");

    // All shader files drive gpu_structs codegen.
    for (path, _) in SHADER_FILES {
        println!("cargo:rerun-if-changed={}", path);
    }

    dump_env();

    add_platform_runtime_rpath();

    generate_gui_adjustables();
    generate_gpu_structs();
}
