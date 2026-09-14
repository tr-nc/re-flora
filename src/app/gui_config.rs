/// GUI Adjustables Configuration
///
/// This file loads GUI parameters from config/gui.toml.
/// The config file is the single source of truth.
use crate::app::curve_preview::{
    draw_curve_preview, smoothstep_variant_response, CurvePreviewMarker,
};
use crate::app::gui_config_loader::GuiConfigLoader;
use crate::app::gui_config_model::{
    GuiConfigFile, GuiParam, GuiParamConditionValue, GuiParamEnabledIf, GuiParamKind, GuiParamValue,
};
use crate::app::tree_gui::edit_tree_desc;
use egui::Color32;
use std::path::Path;
mod audio_mix;
pub(crate) mod butterfly_flight;
mod debug_groups;
mod flora_groups;
pub(crate) mod saved_controls;

mod generated {
    include!("generated/gui_adjustables_gen.rs");
}

pub use generated::GuiAdjustables;

pub struct DebugSettings {
    pub config: GuiConfigFile,
    pub adjustables: GuiAdjustables,
    save_status: Option<String>,
}

// Custom live values ARE the serializable document, never copies requiring save hooks.
impl std::ops::Deref for DebugSettings {
    type Target = GuiConfigFile;
    fn deref(&self) -> &Self::Target {
        &self.config
    }
}
impl std::ops::DerefMut for DebugSettings {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.config
    }
}

impl DebugSettings {
    pub fn load() -> Self {
        let config = GuiConfigLoader::load();
        Self::from_config(config)
    }

    fn from_config(mut config: GuiConfigFile) -> Self {
        let adjustables = GuiAdjustables::from_config(&config);
        config.butterfly_flight.tuning = config.butterfly_flight.tuning.sanitized();
        Self {
            config,
            adjustables,
            save_status: None,
        }
    }

    pub fn save(&mut self) -> std::io::Result<()> {
        self.save_to_path(&GuiConfigLoader::config_path())
    }

    fn save_to_path(&mut self, path: &Path) -> std::io::Result<()> {
        self.sync_config();
        let result = GuiConfigLoader::save_to_path(&self.config, path);
        self.save_status = Some(match &result {
            Ok(()) => "Settings saved".to_owned(),
            Err(error) => format!("Save failed: {error}"),
        });
        result
    }

    pub fn save_status(&self) -> Option<&str> {
        self.save_status.as_deref()
    }

    fn sync_config(&mut self) {
        self.adjustables.write_to_config(&mut self.config);
    }

    pub fn draw(
        &mut self,
        ui: &mut egui::Ui,
        mut temporary_controls: impl FnMut(&str, &mut saved_controls::TemporaryControls<'_>),
    ) -> bool {
        let Self {
            config,
            adjustables,
            ..
        } = self;
        let custom = &mut config.custom;
        let mut tree_desc_changed = false;
        render_gui_from_config(ui, &config.section, adjustables, |section_name, ui| {
            if section_name == "Audio" {
                audio_mix::draw(&mut saved_controls::SavedControls::new(ui, custom));
            }
            if section_name == "Butterflies" {
                self::butterfly_flight::draw_butterfly_flight_ab_controls(
                    &mut saved_controls::SavedControls::new(ui, custom),
                );
            }
            if section_name == "Flora" {
                ui.collapsing("Tree", |ui| {
                    let response = saved_controls::SavedControls::new(ui, custom).toggle(
                        |s| &mut s.tree.desc.cull_thin_branches, true, false,
                        "B: Hide branches thinner than minimum (A/B)",
                    ).on_hover_text("Off: inflate thin wood to the existing minimum radius. On: omit thin wood and trim taper crossings at the threshold. Leaf and fruit anchors stay unchanged. Rebuilds the tuning tree; saved with Save.");
                    tree_desc_changed |= response.changed();
                    tree_desc_changed |= edit_tree_desc(
                        ui,
                        &mut custom.tree.desc,
                        Some(&mut custom.tree.render_leaves),
                    );
                });
            }
            temporary_controls(
                section_name,
                &mut saved_controls::TemporaryControls::new(ui),
            );
        });

        tree_desc_changed
    }
}

fn parse_color(hex: &str) -> Color32 {
    let hex = hex.trim_start_matches('#');
    let (r, g, b, a) = match hex.len() {
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).expect("invalid red");
            let g = u8::from_str_radix(&hex[2..4], 16).expect("invalid green");
            let b = u8::from_str_radix(&hex[4..6], 16).expect("invalid blue");
            (r, g, b, 255)
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).expect("invalid red");
            let g = u8::from_str_radix(&hex[2..4], 16).expect("invalid green");
            let b = u8::from_str_radix(&hex[4..6], 16).expect("invalid blue");
            let a = u8::from_str_radix(&hex[6..8], 16).expect("invalid alpha");
            (r, g, b, a)
        }
        _ => panic!(
            "Invalid color format: #{}. Expected #RRGGBB or #RRGGBBAA",
            hex
        ),
    };
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

fn color_to_hex(color: Color32) -> String {
    format!("#{:02X}{:02X}{:02X}", color.r(), color.g(), color.b())
}

impl GuiAdjustables {
    fn matches_condition(&self, condition: &GuiParamEnabledIf) -> bool {
        match &condition.equals {
            GuiParamConditionValue::Bool(expected) => Self::get_bool_param(self, &condition.param)
                .is_some_and(|field| field.value == *expected),
            GuiParamConditionValue::Integer(expected) => {
                Self::get_int_param(self, &condition.param)
                    .is_some_and(|field| i64::from(field.value) == *expected)
                    || Self::get_uint_param(self, &condition.param)
                        .is_some_and(|field| i64::from(field.value) == *expected)
                    || Self::get_choice_param(self, &condition.param)
                        .is_some_and(|field| i64::from(field.value) == *expected)
            }
            GuiParamConditionValue::String(expected) => {
                Self::get_string_param(self, &condition.param)
                    .is_some_and(|field| field.value == *expected)
                    || Self::get_color_param(self, &condition.param)
                        .is_some_and(|field| color_to_hex(field.value) == *expected)
            }
        }
    }

    fn write_to_config(&self, config: &mut GuiConfigFile) {
        for section in &mut config.section {
            for param in &mut section.param {
                match param.kind {
                    GuiParamKind::Float => {
                        let field = Self::get_float_param(self, &param.id).unwrap_or_else(|| {
                            panic!(
                                "GUI param '{}' (section '{}') missing FloatParam in GuiAdjustables; rebuild required",
                                param.id, section.name
                            )
                        });
                        param.value.set_float(field.value);
                    }
                    GuiParamKind::Int => {
                        let field = Self::get_int_param(self, &param.id).unwrap_or_else(|| {
                            panic!(
                                "GUI param '{}' (section '{}') missing IntParam in GuiAdjustables; rebuild required",
                                param.id, section.name
                            )
                        });
                        param.value.set_int(field.value);
                    }
                    GuiParamKind::Uint => {
                        let field = Self::get_uint_param(self, &param.id).unwrap_or_else(|| {
                            panic!(
                                "GUI param '{}' (section '{}') missing UintParam in GuiAdjustables; rebuild required",
                                param.id, section.name
                            )
                        });
                        param.value.set_uint(field.value);
                    }
                    GuiParamKind::Choice => {
                        let field = Self::get_choice_param(self, &param.id).unwrap_or_else(|| {
                            panic!(
                                "GUI param '{}' (section '{}') missing ChoiceParam in GuiAdjustables; rebuild required",
                                param.id, section.name
                            )
                        });
                        param.value.set_choice(field.value);
                    }
                    GuiParamKind::String => {
                        let field = Self::get_string_param(self, &param.id).unwrap_or_else(|| {
                            panic!(
                                "GUI param '{}' (section '{}') missing StringParam in GuiAdjustables; rebuild required",
                                param.id, section.name
                            )
                        });
                        param.value.set_string(field.value.clone());
                    }
                    GuiParamKind::Bool => {
                        let field = Self::get_bool_param(self, &param.id).unwrap_or_else(|| {
                            panic!(
                                "GUI param '{}' (section '{}') missing BoolParam in GuiAdjustables; rebuild required",
                                param.id, section.name
                            )
                        });
                        param.value.set_bool(field.value);
                    }
                    GuiParamKind::Color => {
                        let field = Self::get_color_param(self, &param.id).unwrap_or_else(|| {
                            panic!(
                                "GUI param '{}' (section '{}') missing ColorParam in GuiAdjustables; rebuild required",
                                param.id, section.name
                            )
                        });
                        param.value.set_color(color_to_hex(field.value));
                    }
                }
            }
        }
    }

    #[allow(dead_code)]
    fn get_float_param<'a>(
        adjustables: &'a GuiAdjustables,
        id: &str,
    ) -> Option<&'a crate::gui_adjustables::FloatParam> {
        generated::get_float_param(adjustables, id)
    }

    #[allow(dead_code)]
    fn get_int_param<'a>(
        adjustables: &'a GuiAdjustables,
        id: &str,
    ) -> Option<&'a crate::gui_adjustables::IntParam> {
        generated::get_int_param(adjustables, id)
    }

    #[allow(dead_code)]
    fn get_uint_param<'a>(
        adjustables: &'a GuiAdjustables,
        id: &str,
    ) -> Option<&'a crate::gui_adjustables::UintParam> {
        generated::get_uint_param(adjustables, id)
    }

    #[allow(dead_code)]
    fn get_choice_param<'a>(
        adjustables: &'a GuiAdjustables,
        id: &str,
    ) -> Option<&'a crate::gui_adjustables::ChoiceParam> {
        generated::get_choice_param(adjustables, id)
    }

    #[allow(dead_code)]
    fn get_string_param<'a>(
        adjustables: &'a GuiAdjustables,
        id: &str,
    ) -> Option<&'a crate::gui_adjustables::StringParam> {
        generated::get_string_param(adjustables, id)
    }

    fn get_bool_param<'a>(
        adjustables: &'a GuiAdjustables,
        id: &str,
    ) -> Option<&'a crate::gui_adjustables::BoolParam> {
        generated::get_bool_param(adjustables, id)
    }

    #[allow(dead_code)]
    fn get_color_param<'a>(
        adjustables: &'a GuiAdjustables,
        id: &str,
    ) -> Option<&'a crate::gui_adjustables::ColorParam> {
        generated::get_color_param(adjustables, id)
    }

    #[allow(dead_code)]
    pub fn get_float_param_mut<'a>(
        adjustables: &'a mut GuiAdjustables,
        id: &str,
    ) -> Option<&'a mut crate::gui_adjustables::FloatParam> {
        generated::get_float_param_mut(adjustables, id)
    }

    #[allow(dead_code)]
    pub fn get_int_param_mut<'a>(
        adjustables: &'a mut GuiAdjustables,
        id: &str,
    ) -> Option<&'a mut crate::gui_adjustables::IntParam> {
        generated::get_int_param_mut(adjustables, id)
    }

    #[allow(dead_code)]
    pub fn get_uint_param_mut<'a>(
        adjustables: &'a mut GuiAdjustables,
        id: &str,
    ) -> Option<&'a mut crate::gui_adjustables::UintParam> {
        generated::get_uint_param_mut(adjustables, id)
    }

    #[allow(dead_code)]
    pub fn get_choice_param_mut<'a>(
        adjustables: &'a mut GuiAdjustables,
        id: &str,
    ) -> Option<&'a mut crate::gui_adjustables::ChoiceParam> {
        generated::get_choice_param_mut(adjustables, id)
    }

    #[allow(dead_code)]
    pub fn get_string_param_mut<'a>(
        adjustables: &'a mut GuiAdjustables,
        id: &str,
    ) -> Option<&'a mut crate::gui_adjustables::StringParam> {
        generated::get_string_param_mut(adjustables, id)
    }

    pub fn get_bool_param_mut<'a>(
        adjustables: &'a mut GuiAdjustables,
        id: &str,
    ) -> Option<&'a mut crate::gui_adjustables::BoolParam> {
        generated::get_bool_param_mut(adjustables, id)
    }

    #[allow(dead_code)]
    pub fn get_color_param_mut<'a>(
        adjustables: &'a mut GuiAdjustables,
        id: &str,
    ) -> Option<&'a mut crate::gui_adjustables::ColorParam> {
        generated::get_color_param_mut(adjustables, id)
    }
}

fn enforce_flora_natural_bend_order(adjustables: &mut GuiAdjustables) {
    if adjustables.grass_natural_bend_max_voxels.value
        < adjustables.grass_natural_bend_min_voxels.value
    {
        adjustables.grass_natural_bend_max_voxels.value =
            adjustables.grass_natural_bend_min_voxels.value;
    }
}

fn enforce_leaf_curve_order(adjustables: &mut GuiAdjustables) {
    if adjustables.leaf_paddle_amplitude_wind_full_strength.value
        < adjustables.leaf_paddle_amplitude_wind_start_strength.value
    {
        adjustables.leaf_paddle_amplitude_wind_full_strength.value =
            adjustables.leaf_paddle_amplitude_wind_start_strength.value;
    }

    if adjustables.leaf_paddle_frequency_wind_full_strength.value
        < adjustables.leaf_paddle_frequency_wind_start_strength.value
    {
        adjustables.leaf_paddle_frequency_wind_full_strength.value =
            adjustables.leaf_paddle_frequency_wind_start_strength.value;
    }

    if adjustables.leaf_paddle_frequency_max_multiplier.value
        < adjustables.leaf_paddle_frequency_min_multiplier.value
    {
        adjustables.leaf_paddle_frequency_max_multiplier.value =
            adjustables.leaf_paddle_frequency_min_multiplier.value;
    }
}

fn draw_flutter_curve_preview(ui: &mut egui::Ui, adjustables: &mut GuiAdjustables) {
    let start = adjustables.leaf_flutter_wind_start.value;
    adjustables.leaf_flutter_wind_full.value = adjustables.leaf_flutter_wind_full.value.max(start);
    let full = adjustables.leaf_flutter_wind_full.value;
    let knee = adjustables.leaf_flutter_wind_knee.value;
    let markers = [
        CurvePreviewMarker {
            x: start,
            label: "start",
            color: Color32::from_rgb(120, 180, 255),
        },
        CurvePreviewMarker {
            x: full,
            label: "full",
            color: Color32::from_rgb(255, 200, 90),
        },
    ];
    draw_curve_preview(
        ui,
        "Flutter wind response",
        0.0..=4.0,
        0.0..=1.0,
        &markers,
        |wind| {
            if wind <= 0.0 {
                0.0
            } else {
                smoothstep_variant_response(wind, start, full, knee)
            }
        },
    );
    ui.small("Start: wind speed where flutter begins. Full: maximum response, not a stop threshold. Bias: negative responds earlier, positive later; 0 is a smooth S curve. Falling wind follows the same curve; inertia lets motion settle. These are wind strengths, not seconds.");
}

fn draw_leaf_curve_previews(ui: &mut egui::Ui, adjustables: &GuiAdjustables) {
    let marker_start_color = Color32::from_rgb(120, 180, 255);
    let marker_full_color = Color32::from_rgb(255, 200, 90);

    let amplitude_start = adjustables.leaf_paddle_amplitude_wind_start_strength.value;
    let amplitude_full = adjustables.leaf_paddle_amplitude_wind_full_strength.value;
    let amplitude_knee = adjustables.leaf_paddle_amplitude_wind_knee_bias.value;
    let amplitude_markers = [
        CurvePreviewMarker {
            x: amplitude_start,
            label: "start",
            color: marker_start_color,
        },
        CurvePreviewMarker {
            x: amplitude_full,
            label: "full",
            color: marker_full_color,
        },
    ];
    draw_curve_preview(
        ui,
        "Amplitude response preview",
        0.0..=4.0,
        0.0..=1.0,
        &amplitude_markers,
        |wind| smoothstep_variant_response(wind, amplitude_start, amplitude_full, amplitude_knee),
    );

    let frequency_start = adjustables.leaf_paddle_frequency_wind_start_strength.value;
    let frequency_full = adjustables.leaf_paddle_frequency_wind_full_strength.value;
    let frequency_knee = adjustables.leaf_paddle_frequency_wind_knee_bias.value;
    let frequency_min = adjustables.leaf_paddle_frequency_min_multiplier.value;
    let frequency_max = adjustables.leaf_paddle_frequency_max_multiplier.value;
    let frequency_markers = [
        CurvePreviewMarker {
            x: frequency_start,
            label: "start",
            color: marker_start_color,
        },
        CurvePreviewMarker {
            x: frequency_full,
            label: "full",
            color: marker_full_color,
        },
    ];
    draw_curve_preview(
        ui,
        "Frequency multiplier preview",
        0.0..=4.0,
        0.0..=3.0,
        &frequency_markers,
        |wind| {
            let response =
                smoothstep_variant_response(wind, frequency_start, frequency_full, frequency_knee);
            frequency_min + (frequency_max - frequency_min) * response
        },
    );
}

fn render_gui_param_from_config(
    ui: &mut egui::Ui,
    param: &GuiParam,
    section_name: &str,
    adjustables: &mut GuiAdjustables,
) {
    let enabled = param
        .enabled_if
        .as_ref()
        .is_none_or(|condition| adjustables.matches_condition(condition));
    ui.add_enabled_ui(enabled, |ui| {
        render_gui_param_control(ui, param, section_name, adjustables);
    });
}

fn render_gui_param_control(
    ui: &mut egui::Ui,
    param: &GuiParam,
    section_name: &str,
    adjustables: &mut GuiAdjustables,
) {
    match (&param.kind, &param.value) {
        (GuiParamKind::Float, GuiParamValue::Float { min, max, .. }) => {
            let field = GuiAdjustables::get_float_param_mut(adjustables, &param.id).unwrap_or_else(|| {
                panic!(
                    "GUI param '{}' (section '{}') missing FloatParam in GuiAdjustables; rebuild required",
                    param.id, section_name
                )
            });
            let range = min.unwrap_or(0.0)..=max.unwrap_or(1.0);
            ui.add(egui::Slider::new(&mut field.value, range).text(&param.label));
        }
        (GuiParamKind::Int, GuiParamValue::Int { min, max, .. }) => {
            let field = GuiAdjustables::get_int_param_mut(adjustables, &param.id).unwrap_or_else(|| {
                panic!(
                    "GUI param '{}' (section '{}') missing IntParam in GuiAdjustables; rebuild required",
                    param.id, section_name
                )
            });
            let range = min.unwrap_or(0)..=max.unwrap_or(100);
            ui.add(egui::Slider::new(&mut field.value, range).text(&param.label));
        }
        (GuiParamKind::Uint, GuiParamValue::Uint { min, max, .. }) => {
            let field = GuiAdjustables::get_uint_param_mut(adjustables, &param.id).unwrap_or_else(|| {
                panic!(
                    "GUI param '{}' (section '{}') missing UintParam in GuiAdjustables; rebuild required",
                    param.id, section_name
                )
            });
            let range = min.unwrap_or(0)..=max.unwrap_or(100);
            ui.add(egui::Slider::new(&mut field.value, range).text(&param.label));
        }
        (GuiParamKind::Choice, GuiParamValue::Choice { options, .. }) => {
            let field = GuiAdjustables::get_choice_param_mut(adjustables, &param.id).unwrap_or_else(|| {
                panic!(
                    "GUI param '{}' (section '{}') missing ChoiceParam in GuiAdjustables; rebuild required",
                    param.id, section_name
                )
            });
            let selected_text = options
                .get(field.value as usize)
                .map(String::as_str)
                .unwrap_or("Invalid choice");
            egui::ComboBox::from_label(&param.label)
                .selected_text(selected_text)
                .show_ui(ui, |ui| {
                    for (index, option) in options.iter().enumerate() {
                        ui.selectable_value(&mut field.value, index as u32, option);
                    }
                });
        }
        (GuiParamKind::String, GuiParamValue::String { .. }) => {
            let field = GuiAdjustables::get_string_param_mut(adjustables, &param.id).unwrap_or_else(|| {
                panic!(
                    "GUI param '{}' (section '{}') missing StringParam in GuiAdjustables; rebuild required",
                    param.id, section_name
                )
            });
            ui.horizontal(|ui| {
                ui.label(&param.label);
                ui.text_edit_singleline(&mut field.value);
            });
        }
        (GuiParamKind::Bool, GuiParamValue::Bool { .. }) => {
            let field = GuiAdjustables::get_bool_param_mut(adjustables, &param.id).unwrap_or_else(|| {
                panic!(
                    "GUI param '{}' (section '{}') missing BoolParam in GuiAdjustables; rebuild required",
                    param.id, section_name
                )
            });
            ui.checkbox(&mut field.value, &param.label);
        }
        (GuiParamKind::Color, GuiParamValue::Color { .. }) => {
            let field = GuiAdjustables::get_color_param_mut(adjustables, &param.id).unwrap_or_else(|| {
                panic!(
                    "GUI param '{}' (section '{}') missing ColorParam in GuiAdjustables; rebuild required",
                    param.id, section_name
                )
            });
            ui.horizontal(|ui| {
                ui.label(&param.label);
                ui.color_edit_button_srgba(&mut field.value);
            });
        }
        _ => unreachable!(
            "GUI param '{}' (section '{}') has kind that is not supported by the GUI renderer",
            param.id, section_name
        ),
    }
}

// UI ownership only. Stored section names and generated parameter bindings stay unchanged.
const SECTION_PARENTS: &[(&str, &str)] = &[
    ("GodRay", "Sky"),
    ("Starlight", "Sky"),
    ("Clouds", "Sky"),
    ("Purple Allium", "Flora"),
    ("Flora Spawn Animation", "Flora"),
    ("FloraVariation", "Flora"),
    ("Leaves", "Flora"),
    ("Terrain Harvest Particles", "Voxel"),
];

fn section_parent(name: &str) -> Option<&'static str> {
    SECTION_PARENTS
        .iter()
        .find_map(|(child, parent)| (*child == name).then_some(*parent))
}

fn section_title(name: &str) -> &str {
    match name {
        "Sky" => "Atmos",
        "Voxel" => "Terrain",
        "HeadBob" => "Camera",
        "FloraVariation" => "Flora Variation",
        _ => name,
    }
}

fn render_gui_from_config(
    ui: &mut egui::Ui,
    config: &[crate::app::gui_config_model::GuiSection],
    adjustables: &mut GuiAdjustables,
    mut after_section: impl FnMut(&str, &mut egui::Ui),
) {
    for section in config {
        if section.name == "Debug" {
            debug_groups::render(ui, section, adjustables, None);
            continue;
        }
        // If a custom config lacks a parent, keep its children visible at the top level.
        if section_parent(&section.name)
            .is_some_and(|parent| config.iter().any(|s| s.name == parent))
        {
            continue;
        }
        ui.collapsing(section_title(&section.name), |ui| {
            if section.name == "Audio" {
                after_section(&section.name, ui);
                ui.collapsing("Advanced audio / source trims", |ui| {
                    render_section_controls(ui, section, adjustables);
                });
                return;
            }
            if section.name == "Flora" {
                flora_groups::render(ui, config, section, adjustables, &mut after_section);
                return;
            }
            render_section_controls(ui, section, adjustables);
            if let Some(debug) = config.iter().find(|s| s.name == "Debug") {
                debug_groups::render(ui, debug, adjustables, Some(&section.name));
            }
            after_section(&section.name, ui);
            for child in config {
                if section_parent(&child.name) == Some(section.name.as_str()) {
                    ui.collapsing(section_title(&child.name), |ui| {
                        render_section_controls(ui, child, adjustables);
                        after_section(&child.name, ui);
                    });
                }
            }
        });
    }
}

fn render_section_controls(
    ui: &mut egui::Ui,
    section: &crate::app::gui_config_model::GuiSection,
    adjustables: &mut GuiAdjustables,
) {
    if section.name == "Wind" {
        for param in &section.param {
            if !is_tree_sound_synthesis_param(&param.id) {
                render_gui_param_from_config(ui, param, &section.name, adjustables);
            }
        }
        ui.collapsing("Procedural Tree Sound", |ui| {
            ui.small(
                "Synthesized rustle texture only; does not change the world's wind or leaf motion.",
            );
            for param in &section.param {
                if is_tree_sound_synthesis_param(&param.id) {
                    render_gui_param_from_config(ui, param, &section.name, adjustables);
                }
            }
        });
        return;
    }
    if section.name == "Sky" {
        ui.label("Scene lighting");
        for id in ["sun_luminance", "sky_light_strength"] {
            if let Some(param) = section.param.iter().find(|param| param.id == id) {
                render_gui_param_from_config(ui, param, &section.name, adjustables);
            }
        }
        ui.small("Sun lights exposed surfaces; sky fills shadows. Changes apply live; indirect light settles over several frames.");
        ui.label("Sky appearance & time");
        ui.small("The sky gradient and its mirror image keep their appearance. Clouds use scene lighting. Sun disk brightness does not set surface lighting.");
        for param in &section.param {
            if !matches!(param.id.as_str(), "sun_luminance" | "sky_light_strength") {
                render_gui_param_from_config(ui, param, &section.name, adjustables);
            }
        }
        return;
    }
    for param in &section.param {
        render_gui_param_from_config(ui, param, &section.name, adjustables);
    }
}

fn is_tree_sound_synthesis_param(id: &str) -> bool {
    id.starts_with("tree_rustle_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_sound_synthesis_is_separate_from_spatial_wind_controls() {
        let settings = DebugSettings::load();
        let wind = settings
            .config
            .section
            .iter()
            .find(|s| s.name == "Wind")
            .unwrap();
        let synthesis: Vec<_> = wind
            .param
            .iter()
            .filter(|p| is_tree_sound_synthesis_param(&p.id))
            .map(|p| p.id.as_str())
            .collect();
        assert_eq!(synthesis.len(), 8);
        assert!(!wind.param.iter().any(|p| p.id == "tree_rustle_base_wind"));
        for id in [
            "canopy_audio_sample_budget",
            "wind_audio_attack_decay",
            "wind_audio_release_decay",
            "tree_wind_response_min_strength",
            "tree_wind_response_max_strength",
        ] {
            assert!(wind.param.iter().any(|p| p.id == id));
            assert!(!is_tree_sound_synthesis_param(id));
        }
    }

    #[test]
    fn every_declared_generic_setting_saves_its_live_value() {
        let mut settings = DebugSettings::load();
        // Iterate the declaration, not a manually maintained list of controls.
        for param in settings.config.section.iter().flat_map(|s| &s.param) {
            let id = &param.id;
            let a = &mut settings.adjustables;
            match param.kind {
                GuiParamKind::Float => {
                    let f = GuiAdjustables::get_float_param_mut(a, id).unwrap();
                    f.value = if f.value == *f.range.start() {
                        *f.range.end()
                    } else {
                        *f.range.start()
                    };
                }
                GuiParamKind::Int => {
                    let f = GuiAdjustables::get_int_param_mut(a, id).unwrap();
                    f.value = if f.value == *f.range.start() {
                        *f.range.end()
                    } else {
                        *f.range.start()
                    };
                }
                GuiParamKind::Uint => {
                    let f = GuiAdjustables::get_uint_param_mut(a, id).unwrap();
                    f.value = if f.value == *f.range.start() {
                        *f.range.end()
                    } else {
                        *f.range.start()
                    };
                }
                GuiParamKind::Choice => {
                    let f = GuiAdjustables::get_choice_param_mut(a, id).unwrap();
                    f.value ^= 1;
                }
                GuiParamKind::String => {
                    let f = GuiAdjustables::get_string_param_mut(a, id).unwrap();
                    f.value.push_str("_roundtrip");
                }
                GuiParamKind::Bool => {
                    let f = GuiAdjustables::get_bool_param_mut(a, id).unwrap();
                    f.value = !f.value;
                }
                GuiParamKind::Color => {
                    let f = GuiAdjustables::get_color_param_mut(a, id).unwrap();
                    f.value = if f.value == Color32::RED {
                        Color32::BLUE
                    } else {
                        Color32::RED
                    };
                }
            }
        }
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("gui.toml");
        settings.save_to_path(&path).unwrap();
        let loaded = DebugSettings::from_config(GuiConfigLoader::load_from_path(&path));
        assert_generic_values_match(&loaded.config, &settings.adjustables);
        assert_generic_values_match(&loaded.config, &loaded.adjustables);
    }

    #[test]
    fn branch_checkbox_requests_rebuild_and_saves_through_the_common_path() {
        fn find(shape: &egui::Shape) -> Option<egui::Pos2> {
            match shape {
                egui::Shape::Text(t)
                    if t.galley.job.text == "B: Hide branches thinner than minimum (A/B)" =>
                {
                    Some(t.pos + egui::vec2(5.0, 6.0))
                }
                egui::Shape::Vec(shapes) => shapes.iter().find_map(find),
                _ => None,
            }
        }
        let mut settings = DebugSettings::load();
        settings.tree.desc.cull_thin_branches = false;
        let context = egui::Context::default();
        context.memory_mut(|m| m.set_everything_is_visible(true));
        let mut draw = |events| {
            let mut changed = false;
            let output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 24000.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    changed = settings.draw(ui, |_, _| {});
                },
            );
            (output.shapes.iter().find_map(|s| find(&s.shape)), changed)
        };
        draw(Vec::new());
        let pos = draw(Vec::new())
            .0
            .expect("branch checkbox must be in Debug Panel");
        draw(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
        ]);
        assert!(
            draw(vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default()
            }])
            .1,
            "checkbox must request the existing tree rebuild"
        );
        assert!(settings.tree.desc.cull_thin_branches);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("gui.toml");
        settings.save_to_path(&path).unwrap();
        assert!(
            DebugSettings::from_config(GuiConfigLoader::load_from_path(&path))
                .tree
                .desc
                .cull_thin_branches
        );
    }

    #[test]
    fn every_serialized_custom_leaf_survives_live_edit_save_reload() {
        use serde_json::Value;
        fn leaves(value: &Value, path: String, output: &mut Vec<String>) {
            match value {
                Value::Object(fields) => {
                    for (key, v) in fields {
                        leaves(v, format!("{path}/{key}"), output);
                    }
                }
                Value::Array(values) => {
                    for (i, v) in values.iter().enumerate() {
                        leaves(v, format!("{path}/{i}"), output);
                    }
                }
                _ => output.push(path),
            }
        }
        let original = serde_json::to_value(&DebugSettings::load().config.custom).unwrap();
        let mut paths = Vec::new();
        leaves(&original, String::new(), &mut paths);
        assert!(paths.len() > 20);
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("gui.toml");
        for path in paths {
            let mut edited = original.clone();
            let value = edited.pointer_mut(&path).unwrap();
            *value = match value {
                Value::Bool(v) => Value::Bool(!*v),
                Value::Number(_) if path.ends_with("/height_above_ground") => Value::from(0.125),
                Value::Number(v) if v.is_f64() => {
                    Value::from(if v.as_f64() == Some(0.5) { 0.75 } else { 0.5 })
                }
                Value::Number(v) => {
                    Value::from(v.as_u64().expect("add a signed-value test policy") ^ 1)
                }
                Value::String(v) if v == "DartingBlock" => Value::from("OriginalSprite"),
                Value::String(v) if v == "DartingSprite" => Value::from("DartingBlock"),
                Value::String(v) if v == "OriginalSprite" => Value::from("DartingBlock"),
                _ => panic!("New custom leaf {path} needs a valid alternate-value policy"),
            };
            let mut settings = DebugSettings::load();
            // Modify the live saved object AFTER construction: a stale load/save
            // roundtrip would not detect a missing synchronization hook.
            settings.config.custom = serde_json::from_value(edited).unwrap();
            let expected = serde_json::to_value(&settings.config.custom).unwrap();
            settings.save_to_path(&file).unwrap();
            let loaded = DebugSettings::from_config(GuiConfigLoader::load_from_path(&file));
            assert_eq!(
                serde_json::to_value(&loaded.config.custom).unwrap(),
                expected,
                "{path}"
            );
        }
    }

    #[test]
    fn fallen_leaf_experiment_controls_are_removed() {
        let config = GuiConfigLoader::load();
        for param in config.section.iter().flat_map(|s| &s.param) {
            assert!(!matches!(
                param.id.as_str(),
                "fallen_leaf_flight" | "fallen_leaf_rotating_plate"
            ));
        }
        assert!(config
            .section
            .iter()
            .flat_map(|s| &s.param)
            .any(|p| p.id == "leaf_flutter_strength"));
    }

    #[test]
    fn section_hierarchy_has_unique_children_and_existing_top_level_parents() {
        let config = GuiConfigLoader::load();
        let mut children = std::collections::BTreeSet::new();
        for (child, parent) in SECTION_PARENTS {
            assert!(children.insert(child), "duplicate child {child}");
            assert!(config.section.iter().any(|s| s.name == *child));
            assert!(config.section.iter().any(|s| s.name == *parent));
            assert_eq!(
                section_parent(parent),
                None,
                "unexpected third-level section"
            );
        }
        assert_eq!(section_parent("GodRay"), Some("Sky"));
        assert_eq!(section_parent("Leaves"), Some("Flora"));
        assert_eq!(section_title("Sky"), "Atmos");
        assert_eq!(section_title("Voxel"), "Terrain");
        assert_eq!(section_title("HeadBob"), "Camera");
    }

    fn assert_generic_values_match(config: &GuiConfigFile, adjustables: &GuiAdjustables) {
        for section in &config.section {
            for param in &section.param {
                match (&param.kind, &param.value) {
                    (GuiParamKind::Float, GuiParamValue::Float { value, .. }) => assert_eq!(
                        GuiAdjustables::get_float_param(adjustables, &param.id)
                            .map(|field| field.value),
                        Some(*value),
                        "float param {}",
                        param.id
                    ),
                    (GuiParamKind::Int, GuiParamValue::Int { value, .. }) => assert_eq!(
                        GuiAdjustables::get_int_param(adjustables, &param.id)
                            .map(|field| field.value),
                        Some(*value),
                        "int param {}",
                        param.id
                    ),
                    (GuiParamKind::Uint, GuiParamValue::Uint { value, .. }) => assert_eq!(
                        GuiAdjustables::get_uint_param(adjustables, &param.id)
                            .map(|field| field.value),
                        Some(*value),
                        "uint param {}",
                        param.id
                    ),
                    (GuiParamKind::Choice, GuiParamValue::Choice { value, .. }) => assert_eq!(
                        GuiAdjustables::get_choice_param(adjustables, &param.id)
                            .map(|field| field.value),
                        Some(*value),
                        "choice param {}",
                        param.id
                    ),
                    (GuiParamKind::String, GuiParamValue::String { value }) => assert_eq!(
                        GuiAdjustables::get_string_param(adjustables, &param.id)
                            .map(|field| field.value.as_str()),
                        Some(value.as_str()),
                        "string param {}",
                        param.id
                    ),
                    (GuiParamKind::Bool, GuiParamValue::Bool { value }) => assert_eq!(
                        GuiAdjustables::get_bool_param(adjustables, &param.id)
                            .map(|field| field.value),
                        Some(*value),
                        "bool param {}",
                        param.id
                    ),
                    (GuiParamKind::Color, GuiParamValue::Color { value }) => assert_eq!(
                        GuiAdjustables::get_color_param(adjustables, &param.id)
                            .map(|field| color_to_hex(field.value)),
                        Some(value.clone()),
                        "color param {}",
                        param.id
                    ),
                    _ => panic!("kind/value mismatch for {}", param.id),
                }
            }
        }
    }

    #[test]
    fn environmental_acoustics_is_always_on_with_an_unconditional_quality_control() {
        let config = GuiConfigLoader::load();
        let params = config
            .section
            .iter()
            .flat_map(|section| section.param.iter())
            .collect::<Vec<_>>();

        assert!(params
            .iter()
            .all(|param| param.id != "audio_ray_tracing_enabled"));
        let quality = params
            .iter()
            .find(|param| param.id == "audio_ray_tracing_quality_percent")
            .expect("environmental acoustics quality control");
        assert!(quality.enabled_if.is_none());
    }

    #[test]
    fn glass_raster_reflections_are_mandatory_not_gui_adjustable() {
        let config = include_str!("../../config/gui.toml");

        assert!(
            !config.contains("id = \"glass_raster_reflections\""),
            "mandatory Glass raster reflections must not be exposed as a GUI checkbox",
        );
    }

    #[test]
    fn legacy_per_voxel_glass_is_fixed_not_gui_adjustable() {
        let config = include_str!("../../config/gui.toml");

        assert!(
            !config.contains("id = \"glass_per_voxel_reflection\""),
            "fixed legacy per-voxel Glass must not be exposed as a GUI checkbox",
        );
    }

    #[test]
    fn glass_unrefracted_raster_fallback_defaults_off() {
        let settings = DebugSettings::from_config(GuiConfigLoader::load());

        assert!(!settings.adjustables.glass_unrefracted_raster_fallback.value);
    }

    #[test]
    fn glass_refraction_defaults_on_and_owns_the_fallback_control() {
        let settings = DebugSettings::from_config(GuiConfigLoader::load());

        assert!(settings.adjustables.glass_refraction_enabled.value);
        let fallback = settings
            .config
            .section
            .iter()
            .flat_map(|section| section.param.iter())
            .find(|param| param.id == "glass_unrefracted_raster_fallback")
            .expect("Glass unrefracted fallback GUI parameter");
        assert_eq!(
            fallback.enabled_if,
            Some(GuiParamEnabledIf {
                param: "glass_refraction_enabled".to_owned(),
                equals: GuiParamConditionValue::Bool(true),
            })
        );
    }

    #[test]
    fn glass_stored_voxel_normal_defaults_on() {
        let settings = DebugSettings::from_config(GuiConfigLoader::load());

        assert!(settings.adjustables.glass_stored_voxel_normal.value);
    }

    #[test]
    fn enabled_if_condition_follows_controller_without_mutating_dependent_value() {
        let config = GuiConfigLoader::load();
        let mut adjustables = GuiAdjustables::from_config(&config);
        let condition = GuiParamEnabledIf {
            param: "path_tracing_reference".to_owned(),
            equals: GuiParamConditionValue::Bool(true),
        };

        adjustables.path_tracing_reference.value = false;
        assert!(!adjustables.matches_condition(&condition));
        adjustables.path_tracing_reference.value = true;
        assert!(adjustables.matches_condition(&condition));
    }

    #[test]
    fn older_sky_settings_load_the_new_control_and_preserve_saved_sun_values() {
        let mut settings = DebugSettings::from_config(GuiConfigLoader::load());
        settings.adjustables.sun_luminance.value = 3.25;
        settings.adjustables.sun_display_luminance.value = 0.75;
        settings.sync_config();
        for section in &mut settings.config.section {
            section
                .param
                .retain(|param| param.id != "sky_light_strength");
        }
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("gui.toml");
        GuiConfigLoader::save_to_path(&settings.config, &path).unwrap();
        let mut reloaded = DebugSettings::from_config(GuiConfigLoader::load_from_path(&path));
        assert_eq!(reloaded.adjustables.sun_luminance.value, 3.25);
        assert_eq!(reloaded.adjustables.sun_display_luminance.value, 0.75);
        assert_eq!(
            reloaded.adjustables.sky_light_strength.value,
            settings.adjustables.sky_light_strength.value
        );
        reloaded.adjustables.sky_light_strength.value = 0.0;
        reloaded.save_to_path(&path).unwrap();
        let reloaded = DebugSettings::from_config(GuiConfigLoader::load_from_path(&path));
        assert_eq!(reloaded.adjustables.sky_light_strength.value, 0.0);
    }

    #[test]
    fn butterfly_flight_controls_survive_debug_settings_save_and_reload() {
        let mut document: toml::Value =
            toml::from_str(include_str!("../../config/gui.toml")).unwrap();
        let flight: toml::Value = toml::from_str(
            r#"
variant = "DartingBlock"
[tuning]
flight_frequency_hz = 5.5
maneuver_tempo = 2.75
vertical_strength = 2.0
turn_sharpness = 1.0
speed = 0.35
wind_drift = 1.0
"#,
        )
        .unwrap();
        document
            .as_table_mut()
            .unwrap()
            .insert("butterfly_flight".to_owned(), flight);
        let config = document.try_into().unwrap();
        let mut settings = DebugSettings::from_config(config);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("gui.toml");
        settings.save_to_path(&path).unwrap();
        let reloaded = GuiConfigLoader::load_from_path(&path);
        let saved = toml::Value::try_from(reloaded).unwrap();
        assert_eq!(
            saved
                .get("butterfly_flight")
                .and_then(|f| f.get("tuning"))
                .and_then(|t| t.get("flight_frequency_hz"))
                .and_then(toml::Value::as_float),
            Some(5.5),
            "Debug Settings Save must preserve butterfly flight controls",
        );
    }

    #[test]
    fn live_butterfly_settings_and_disabled_b_controls_persist_without_reset_button() {
        let mut settings = DebugSettings::load();
        let context = egui::Context::default();
        context.memory_mut(|m| m.set_everything_is_visible(true));
        let output = context.run_ui(egui::RawInput::default(), |ui| {
            settings.draw(ui, |_, _| {});
        });
        let text = format!("{:?}", output.shapes);
        assert!(text.contains("Shared flight frequency"));
        assert!(text.contains("Flight height above ground"));
        assert!(!text.contains("Horizontal maneuver tempo"));
        assert!(!text.contains("Reset B flight controls"));
        settings.butterfly_flight.variant =
            crate::particles::ButterflyFlightVariant::OriginalSprite;
        settings.butterfly_flight.tuning = crate::particles::ButterflyFlightTuning {
            flight_frequency_hz: 0.0,
            height_above_ground: 0.12,
            maneuver_tempo: 3.25,
            vertical_strength: 1.25,
            turn_sharpness: 2.5,
            speed: 0.75,
            wind_drift: 0.5,
        };
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("gui.toml");
        settings.save_to_path(&path).unwrap();
        assert_eq!(settings.save_status(), Some("Settings saved"));
        let reloaded = DebugSettings::from_config(GuiConfigLoader::load_from_path(&path));
        assert_eq!(settings.butterfly_flight, reloaded.butterfly_flight);
        settings.butterfly_flight.variant = crate::particles::ButterflyFlightVariant::DartingBlock;
        settings.save_to_path(&path).unwrap();
        let reloaded = DebugSettings::from_config(GuiConfigLoader::load_from_path(&path));
        assert_eq!(settings.butterfly_flight, reloaded.butterfly_flight);
        assert!(settings.save_to_path(directory.path()).is_err());
        assert!(settings.save_status().unwrap().starts_with("Save failed:"));
    }

    #[test]
    fn older_gui_files_without_butterfly_controls_load_legacy_flight_defaults() {
        let mut config = toml::Value::try_from(GuiConfigLoader::load()).unwrap();
        config.as_table_mut().unwrap().remove("butterfly_flight");
        let settings = DebugSettings::from_config(config.try_into().unwrap());
        assert_eq!(
            settings.butterfly_flight,
            crate::particles::ButterflyFlightSettings::default()
        );
    }

    #[test]
    fn current_debug_settings_write_complete_generic_and_tree_state() {
        let mut settings = DebugSettings::from_config(GuiConfigLoader::load());
        settings.adjustables.time_of_day.value = 0.987;
        settings.adjustables.sun_luminance.value = 3.25;
        settings.adjustables.sky_light_strength.value = 0.125;
        settings.adjustables.sun_display_luminance.value = 0.75;
        settings.adjustables.voxel_dirt_color.value = Color32::from_rgb(12, 34, 56);
        settings.tree.render_leaves = false;
        settings.tree.desc.size = 19.5;
        settings.tree.desc.branching.seed = 9876;
        settings.tree.desc.fruit_swing_speed = 3.25;

        settings.sync_config();

        assert_generic_values_match(&settings.config, &settings.adjustables);
        assert_eq!(settings.config.tree, settings.tree.clone());

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("gui.toml");
        settings.save_to_path(&path).unwrap();
        let reloaded = DebugSettings::from_config(GuiConfigLoader::load_from_path(&path));

        assert_generic_values_match(&reloaded.config, &reloaded.adjustables);
        assert_eq!(reloaded.tree, settings.tree);
        assert_eq!(
            reloaded.adjustables.voxel_dirt_color.value,
            Color32::from_rgb(12, 34, 56)
        );
        assert_eq!(reloaded.adjustables.time_of_day.value, 0.987);
        assert_eq!(reloaded.adjustables.sun_luminance.value, 3.25);
        assert_eq!(reloaded.adjustables.sky_light_strength.value, 0.125);
        assert_eq!(reloaded.adjustables.sun_display_luminance.value, 0.75);
    }
}
