use re_flora_shader_build::{NativeSlangCompiler, OptimizationLevel, NATIVE_SHADERS};
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

/// Logical native Slang entry points reflected for CPU/GPU struct generation.
const SHADER_FILES: &[&str] = &[
    "shader/builder/chunk_writer/buffer_setup.comp",
    "shader/builder/chunk_writer/chunk_modify.comp",
    "shader/builder/chunk_writer/chunk_modify_sample.comp",
    "shader/builder/chunk_writer/chunk_solid_sample.comp",
    "shader/builder/chunk_writer/voxel_property_sample.comp",
    "shader/builder/chunk_writer/model_voxelize.comp",
    "shader/builder/contree/buffer_setup.comp",
    "shader/builder/scene_accel/update_scene_tex.comp",
    "shader/builder/surface/clear_occupancy.comp",
    "shader/builder/surface/edit_occupancy_capsule.comp",
    "shader/builder/surface/instances_to_occupancy.comp",
    "shader/builder/surface/make_surface.comp",
    "shader/builder/surface/occupancy_to_flora_instances.comp",
    "shader/ddgi/global_sky_filter.comp",
    "shader/ddgi/octahedral_gutter.comp",
    "shader/ddgi/probe_relocate.comp",
    "shader/ddgi/probe_trace.comp",
    "shader/lighting/local_light_visibility_diagnostic.comp",
    "shader/ddgi/irradiance_filter.comp",
    "shader/ddgi/visibility_filter.comp",
    "shader/ddgi/irradiance_gutter.comp",
    "shader/ddgi/visibility_gutter.comp",
    "shader/ddgi/atlas_reduce.comp",
    "shader/ddgi/voxel_visibility_pack.comp",
    "shader/ddgi/voxel_visibility_blocks.comp",
    "shader/lighting/local_light_abi.comp",
    "shader/tracer/tracer.comp",
    "shader/tracer/tracer_glass.comp",
    "shader/tracer/tracer_shadow.comp",
    "shader/tracer/leaf_shadow_temporal.comp",
    "shader/tracer/leaf_shadow_mask.comp",
    "shader/tracer/vsm_blur_h.comp",
    "shader/tracer/vsm_blur_v.comp",
    "shader/tracer/composition.comp",
    "shader/tracer/glass_resolve.comp",
    "shader/tracer/god_ray.comp",
    "shader/tracer/post_processing.comp",
    "shader/tracer/player_collider.comp",
    "shader/tracer/terrain_query.comp",
    "shader/tracer/wind_volume.comp",
    "shader/foliage/flora.vert",
    "shader/foliage/flora_lighting_cache.comp",
    "shader/foliage/flora_lod.vert",
    "shader/foliage/leaves.vert",
    "shader/foliage/leaves_shadow.vert",
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
    /// Reflected shader type name (e.g. `U_CameraInfo`)
    type_name: String,
    /// Ordered by offset
    fields: Vec<PlainField>,
    /// Total size in bytes (offset of last field + its padded_size)
    total_size: u32,
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

fn normalize_shader_type_name(type_name: &str) -> String {
    for suffix in ["_std140", "_std430", "_scalar", "_natural"] {
        if let Some(source_name) = type_name.strip_suffix(suffix) {
            return source_name.to_owned();
        }
    }
    type_name.to_owned()
}

fn slang_matrix_wrapper_type(type_name: &str) -> Option<FieldType> {
    let storage_type = type_name.strip_prefix("_MatrixStorage_float")?;
    if storage_type.starts_with("2x2") {
        Some(FieldType::Mat2)
    } else if storage_type.starts_with("3x3") {
        Some(FieldType::Mat3)
    } else if storage_type.starts_with("4x4") {
        Some(FieldType::Mat4)
    } else if storage_type.starts_with("3x4") {
        Some(FieldType::Mat3x4)
    } else {
        None
    }
}

fn flatten_block_members(
    members: &[spirv_reflect::types::ReflectBlockVariable],
    fields: &mut Vec<PlainField>,
) {
    for member in members {
        let Some(type_description) = &member.type_description else {
            continue;
        };
        let wrapper_type = slang_matrix_wrapper_type(&type_description.type_name).or_else(|| {
            type_description
                .type_name
                .starts_with("_Array_")
                .then_some(FieldType::Array)
        });
        if let Some(ty) = wrapper_type {
            fields.push(PlainField {
                name: member.name.clone(),
                ty,
                offset: member.offset,
                size: member.size,
                padded_size: member.padded_size,
            });
        } else if type_description
            .type_flags
            .contains(ReflectTypeFlags::STRUCT)
        {
            // Wrapper and nested-struct member offsets are relative to their parent.
            // The generated CPU structs only consume the top-level shader blocks reflected here.
            flatten_block_members(&member.members, fields);
        } else {
            let Some(ty) = reflect_field_type(
                &type_description.type_flags,
                &type_description.traits,
                member.size,
            ) else {
                continue;
            };
            fields.push(PlainField {
                name: member.name.clone(),
                ty,
                offset: member.offset,
                size: member.size,
                padded_size: member.padded_size,
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

fn reflect_shader(spirv_bytes: &[u8], path: &str) -> Vec<StructLayout> {
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
            Some(type_description) => normalize_shader_type_name(&type_description.type_name),
            None => continue,
        };
        // Native StructuredBuffer<T> resources expose the Slang wrapper name here;
        // only named CPU/GPU ABI blocks participate in Rust struct generation.
        if !type_name.starts_with("U_") && !type_name.starts_with("B_") {
            continue;
        }
        // skip pure GPU-internal read-only storage buffers that the CPU never writes
        // (contree and scene tex – identified by `B_Contree*`, `B_Scene*`)
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
fn struct_name(shader_type_name: &str) -> String {
    let shader_type_name = normalize_shader_type_name(shader_type_name);
    let stripped = shader_type_name
        .strip_prefix("U_")
        .or_else(|| shader_type_name.strip_prefix("B_"))
        .unwrap_or(&shader_type_name);
    stripped.to_owned()
}

// ---- code emitter -----------------------------------------------------------

fn emit_struct(layout: &StructLayout) -> String {
    let name = struct_name(&layout.type_name);
    let mut code = String::new();

    code.push_str(&format!(
        "/// Auto-generated from `{}` (native Slang source of truth).\n",
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

    // Native Slang modules and entry points are the sole shader inputs.
    println!(
        "cargo:rerun-if-changed={}",
        shader_root.join("slang").display()
    );

    // collect all layouts across all shaders, deduplicating by type name
    // BTreeMap for deterministic output order
    let mut all_layouts: BTreeMap<String, StructLayout> = BTreeMap::new();

    let compiler = NativeSlangCompiler::new();
    for logical_path in SHADER_FILES {
        let shader = NATIVE_SHADERS
            .iter()
            .find(|shader| shader.logical_path == *logical_path)
            .unwrap_or_else(|| panic!("missing native shader manifest entry for {logical_path}"));
        let compiled = compiler.compile_shader(shader, &root, OptimizationLevel::Zero);
        let layouts = reflect_shader(&compiled.spirv, logical_path);
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
    code.push_str(
        "// Generated by build.rs::generate_gpu_structs from native Slang shader sources.\n",
    );
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

fn slang_array_initializer<'a>(source: &'a str, name: &str) -> &'a str {
    let declaration = source
        .find(name)
        .unwrap_or_else(|| panic!("missing {name} in sky environment data"));
    let initializer_start = source[declaration..]
        .find('{')
        .map(|offset| declaration + offset + 1)
        .unwrap_or_else(|| panic!("missing initializer for {name}"));
    let initializer_end = source[initializer_start..]
        .find("};")
        .map(|offset| initializer_start + offset)
        .unwrap_or_else(|| panic!("missing initializer terminator for {name}"));
    &source[initializer_start..initializer_end]
}

fn parse_slang_float_array(source: &str, name: &str) -> Vec<f32> {
    slang_array_initializer(source, name)
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .parse::<f32>()
                .unwrap_or_else(|error| panic!("invalid {name} value {value:?}: {error}"))
        })
        .collect()
}

fn parse_slang_float3_array(source: &str, name: &str) -> Vec<[f32; 3]> {
    let initializer = slang_array_initializer(source, name);
    let mut values = Vec::new();
    let mut remaining = initializer;
    while let Some(start) = remaining.find("float3(") {
        remaining = &remaining[start + "float3(".len()..];
        let end = remaining
            .find(')')
            .unwrap_or_else(|| panic!("unterminated float3 in {name}"));
        let channels = remaining[..end]
            .split(',')
            .map(str::trim)
            .map(|value| {
                value
                    .parse::<f32>()
                    .unwrap_or_else(|error| panic!("invalid {name} channel {value:?}: {error}"))
            })
            .collect::<Vec<_>>();
        assert_eq!(
            channels.len(),
            3,
            "{name} entries must contain three channels"
        );
        values.push([channels[0], channels[1], channels[2]]);
        remaining = &remaining[end + 1..];
    }
    values
}

fn parse_slang_u32_constant(source: &str, name: &str) -> u32 {
    let prefix = format!("public static const uint {name} = ");
    let literal = source
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix(&prefix))
        .and_then(|line| line.strip_suffix(';'))
        .and_then(|line| line.strip_suffix('u'))
        .unwrap_or_else(|| panic!("missing Slang uint constant {name}"));
    literal
        .parse::<u32>()
        .unwrap_or_else(|error| panic!("invalid {name} value {literal:?}: {error}"))
}

fn parse_slang_f32_constant(source: &str, name: &str) -> f32 {
    let prefix = format!("public static const float {name} = ");
    let literal = source
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix(&prefix))
        .and_then(|line| line.strip_suffix(';'))
        .unwrap_or_else(|| panic!("missing Slang float constant {name}"));
    literal
        .parse::<f32>()
        .unwrap_or_else(|error| panic!("invalid {name} value {literal:?}: {error}"))
}

fn parse_slang_float3_constant(source: &str, name: &str) -> [f32; 3] {
    let prefix = format!("public static const float3 {name} = float3(");
    let literal = source
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix(&prefix))
        .and_then(|line| line.strip_suffix(");"))
        .unwrap_or_else(|| panic!("missing Slang float3 constant {name}"));
    let values = literal
        .split(',')
        .map(str::trim)
        .map(|value| {
            value
                .parse::<f32>()
                .unwrap_or_else(|error| panic!("invalid {name} channel {value:?}: {error}"))
        })
        .collect::<Vec<_>>();
    assert_eq!(values.len(), 3, "{name} must contain three channels");
    [values[0], values[1], values[2]]
}

fn generate_voxel_material_config() {
    let root = project_root();
    let source_path = root.join("shader/slang/voxel_material.slang");
    let source = fs::read_to_string(&source_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", source_path.display()));
    let uint_names = [
        "VOXEL_SURFACE_CLASS_EMPTY",
        "VOXEL_SURFACE_CLASS_OPAQUE",
        "VOXEL_SURFACE_CLASS_DIELECTRIC",
        "VOXEL_MATERIAL_FLAG_COLLISION_SOLID",
        "VOXEL_MATERIAL_FLAG_WATER_SOLID",
        "VOXEL_MATERIAL_FLAG_TERRAIN_SUPPORT",
        "VOXEL_MATERIAL_FLAG_PROBE_RELOCATION_SOLID",
        "VOXEL_MATERIAL_FLAG_BLOCKS_DDGI_VISIBILITY",
        "VOXEL_MATERIAL_FLAG_SOIL_STATE_ALLOWED",
        "VOXEL_DIRECT_SHADOW_OPAQUE",
        "VOXEL_DIRECT_SHADOW_SKIP",
        "VOXEL_LOCAL_SHADOW_OPAQUE",
        "VOXEL_LOCAL_SHADOW_OPTICAL_TRANSMITTANCE",
        "GLASS_EXPERIMENT_VOXEL_TYPE",
        "STANDARD_SOIL_VOXEL_TYPE_MASK",
        "STANDARD_SOLID_MATERIAL_FLAGS",
        "GLASS_EXPERIMENT_MATERIAL_FLAGS",
        "GLASS_EXPERIMENT_MATERIAL_REVISION",
    ];
    let mut code = String::from(
        "// @generated by build.rs::generate_voxel_material_config.\n\
         // Source: shader/slang/voxel_material.slang\n\n",
    );
    for name in uint_names {
        let value = parse_slang_u32_constant(&source, name);
        code.push_str(&format!("pub const {name}: u32 = {value};\n"));
    }
    let ior = parse_slang_f32_constant(&source, "GLASS_EXPERIMENT_IOR");
    let attenuation_distance =
        parse_slang_f32_constant(&source, "GLASS_EXPERIMENT_ATTENUATION_DISTANCE_WORLD");
    let attenuation_color =
        parse_slang_float3_constant(&source, "GLASS_EXPERIMENT_ATTENUATION_COLOR");
    code.push_str(&format!("pub const GLASS_EXPERIMENT_IOR: f32 = {ior:?};\n"));
    code.push_str(&format!(
        "pub const GLASS_EXPERIMENT_ATTENUATION_DISTANCE_WORLD: f32 = {attenuation_distance:?};\n"
    ));
    code.push_str(&format!(
        "pub const GLASS_EXPERIMENT_ATTENUATION_COLOR: [f32; 3] = {attenuation_color:?};\n"
    ));

    let output_path = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is not set"))
        .join("voxel_material_config.rs");
    fs::write(&output_path, code)
        .unwrap_or_else(|error| panic!("write {}: {error}", output_path.display()));
}

fn generate_ddgi_config() {
    let root = project_root();
    let source_path = root.join("shader/slang/ddgi_config.slang");
    let source = fs::read_to_string(&source_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", source_path.display()));
    let rays_per_probe = parse_slang_u32_constant(&source, "DDGI_RAYS_PER_PROBE");
    assert!(rays_per_probe > 0, "DDGI_RAYS_PER_PROBE must be nonzero");

    let code = format!(
        "// @generated by build.rs::generate_ddgi_config.\n\
         // Source: shader/slang/ddgi_config.slang\n\n\
         pub const DDGI_RAYS_PER_PROBE: u32 = {rays_per_probe};\n"
    );
    let output_path =
        PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is not set")).join("ddgi_config.rs");
    fs::write(&output_path, code)
        .unwrap_or_else(|error| panic!("write {}: {error}", output_path.display()));
}

fn generate_sky_environment_data() {
    let root = project_root();
    let source_path = root.join("shader/slang/sky_environment_data.slang");
    let source = fs::read_to_string(&source_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", source_path.display()));
    let altitudes = parse_slang_float_array(&source, "SKY_COLOR_ALTITUDES");
    let top_colors = parse_slang_float3_array(&source, "SKY_COLOR_TOP");
    let bottom_colors = parse_slang_float3_array(&source, "SKY_COLOR_BOTTOM");
    assert_eq!(
        altitudes.len(),
        top_colors.len(),
        "sky altitude and top-color counts must match"
    );
    assert_eq!(
        altitudes.len(),
        bottom_colors.len(),
        "sky altitude and bottom-color counts must match"
    );
    assert!(
        altitudes.windows(2).all(|pair| pair[0] < pair[1]),
        "sky altitudes must be strictly increasing"
    );

    let float_literal = |value: f32| format!("{value:.6}");
    let emit_vec3_array = |name: &str, values: &[[f32; 3]], code: &mut String| {
        code.push_str(&format!(
            "pub const {name}: [glam::Vec3; {}] = [\n",
            values.len()
        ));
        for value in values {
            code.push_str(&format!(
                "    glam::Vec3::new({}, {}, {}),\n",
                float_literal(value[0]),
                float_literal(value[1]),
                float_literal(value[2])
            ));
        }
        code.push_str("];\n");
    };

    let mut code = String::from(
        "// @generated by build.rs::generate_sky_environment_data.\n\
         // Source: shader/slang/sky_environment_data.slang\n\n",
    );
    code.push_str(&format!(
        "pub const SKY_COLOR_ALTITUDES: [f32; {}] = [\n",
        altitudes.len()
    ));
    for altitude in altitudes {
        code.push_str(&format!("    {},\n", float_literal(altitude)));
    }
    code.push_str("];\n");
    emit_vec3_array("SKY_COLOR_TOP", &top_colors, &mut code);
    emit_vec3_array("SKY_COLOR_BOTTOM", &bottom_colors, &mut code);

    let output_path = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is not set"))
        .join("sky_environment_data.rs");
    fs::write(&output_path, code)
        .unwrap_or_else(|error| panic!("write {}: {error}", output_path.display()));
}

fn main() {
    // Tell Cargo to rerun this script if these files/directories change.
    // config/gui.toml drives GuiAdjustables codegen.
    println!("cargo:rerun-if-changed=config/gui.toml");
    println!("cargo:rerun-if-changed=shader/slang/ddgi_config.slang");
    println!("cargo:rerun-if-changed=shader/slang/sky_environment_data.slang");
    println!("cargo:rerun-if-changed=shader/slang/voxel_material.slang");

    dump_env();

    generate_gui_adjustables();
    generate_ddgi_config();
    generate_sky_environment_data();
    generate_voxel_material_config();
    generate_gpu_structs();
}
