/// GUI Adjustables Configuration
///
/// This file loads GUI parameters from config/gui.toml.
/// The config file is the single source of truth.
use crate::app::curve_preview::{
    draw_curve_preview, smoothstep_variant_response, CurvePreviewMarker,
};
use crate::app::gui_config_loader::GuiConfigLoader;
use crate::app::gui_config_model::{
    GuiConfigFile, GuiParam, GuiParamConditionValue, GuiParamEnabledIf, GuiParamKind,
    GuiParamValue, TreeGuiConfig,
};
use crate::app::tree_gui::edit_tree_desc;
use crate::tree_gen::TreeDesc;
use egui::Color32;
use std::path::Path;
mod debug_groups;

mod generated {
    include!("generated/gui_adjustables_gen.rs");
}

pub use generated::GuiAdjustables;

pub struct DebugSettings {
    pub config: GuiConfigFile,
    pub adjustables: GuiAdjustables,
    pub tree: TreeGuiConfig,
    save_status: Option<String>,
}

impl DebugSettings {
    pub fn load() -> Self {
        let config = GuiConfigLoader::load();
        Self::from_config(config)
    }

    fn from_config(config: GuiConfigFile) -> Self {
        let adjustables = GuiAdjustables::from_config(&config);
        let tree = config.tree.clone().unwrap_or_else(|| TreeGuiConfig {
            render_leaves: true,
            desc: TreeDesc::default(),
        });
        Self {
            config,
            adjustables,
            tree,
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
        self.adjustables.write_to_config(
            &mut self.config,
            &self.tree.desc,
            self.tree.render_leaves,
        );
    }

    pub fn draw(
        &mut self,
        ui: &mut egui::Ui,
        mut extra_controls: impl FnMut(&str, &mut egui::Ui),
    ) -> bool {
        let Self {
            config,
            adjustables,
            tree,
            ..
        } = self;
        let mut tree_desc_changed = false;
        render_gui_from_config(ui, config, adjustables, |section_name, ui| {
            if section_name == "Flora" {
                ui.collapsing("Tree", |ui| {
                    tree_desc_changed |=
                        edit_tree_desc(ui, &mut tree.desc, Some(&mut tree.render_leaves));
                });
            }
            extra_controls(section_name, ui);
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

    fn write_to_config(
        &self,
        config: &mut GuiConfigFile,
        tree_desc: &TreeDesc,
        render_leaves: bool,
    ) {
        config.tree = Some(TreeGuiConfig {
            render_leaves,
            desc: tree_desc.clone(),
        });

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

fn is_custom_flora_param(id: &str) -> bool {
    matches!(
        id,
        "grass_natural_bend_min_voxels"
            | "grass_natural_bend_max_voxels"
            | "flora_bend_height_power"
            | "grass_vibration_amplitude_voxels"
            | "grass_vibration_primary_speed"
            | "grass_vibration_secondary_speed"
            | "leaf_paddle_amplitude_voxels"
            | "leaf_paddle_primary_speed"
            | "leaf_paddle_secondary_speed"
            | "leaf_paddle_amplitude_wind_start_strength"
            | "leaf_paddle_amplitude_wind_full_strength"
            | "leaf_paddle_amplitude_wind_knee_bias"
            | "leaf_paddle_frequency_wind_start_strength"
            | "leaf_paddle_frequency_wind_full_strength"
            | "leaf_paddle_frequency_wind_knee_bias"
            | "leaf_paddle_frequency_min_multiplier"
            | "leaf_paddle_frequency_max_multiplier"
            | "grass_bottom_dark_color"
            | "grass_bottom_light_color"
            | "grass_tip_dark_color"
            | "grass_tip_light_color"
    )
}

fn render_flora_gui(ui: &mut egui::Ui, adjustables: &mut GuiAdjustables) {
    ui.label("Natural Bend");
    ui.add(
        egui::Slider::new(
            &mut adjustables.grass_natural_bend_min_voxels.value,
            adjustables.grass_natural_bend_min_voxels.range.clone(),
        )
        .text("Flora Natural Bend Min (voxels)"),
    );
    ui.add(
        egui::Slider::new(
            &mut adjustables.grass_natural_bend_max_voxels.value,
            adjustables.grass_natural_bend_max_voxels.range.clone(),
        )
        .text("Flora Natural Bend Max (voxels)"),
    );
    enforce_flora_natural_bend_order(adjustables);
    ui.add(
        egui::Slider::new(
            &mut adjustables.flora_bend_height_power.value,
            adjustables.flora_bend_height_power.range.clone(),
        )
        .text("Flora Bend Height Power"),
    );

    ui.add_space(4.0);
    ui.label("Flora Vibration");
    ui.add(
        egui::Slider::new(
            &mut adjustables.grass_vibration_amplitude_voxels.value,
            adjustables.grass_vibration_amplitude_voxels.range.clone(),
        )
        .text("Flora Vibration Amplitude (voxels)"),
    );
    ui.add(
        egui::Slider::new(
            &mut adjustables.grass_vibration_primary_speed.value,
            adjustables.grass_vibration_primary_speed.range.clone(),
        )
        .text("Flora Vibration Primary Speed"),
    );
    ui.add(
        egui::Slider::new(
            &mut adjustables.grass_vibration_secondary_speed.value,
            adjustables.grass_vibration_secondary_speed.range.clone(),
        )
        .text("Flora Vibration Secondary Speed"),
    );
    ui.add(
        egui::Slider::new(
            &mut adjustables.leaf_paddle_amplitude_voxels.value,
            adjustables.leaf_paddle_amplitude_voxels.range.clone(),
        )
        .text("Leaf Paddle Amplitude (voxels)"),
    );
    ui.add(
        egui::Slider::new(
            &mut adjustables.leaf_paddle_primary_speed.value,
            adjustables.leaf_paddle_primary_speed.range.clone(),
        )
        .text("Leaf Paddle Primary Speed"),
    );
    ui.add(
        egui::Slider::new(
            &mut adjustables.leaf_paddle_secondary_speed.value,
            adjustables.leaf_paddle_secondary_speed.range.clone(),
        )
        .text("Leaf Paddle Secondary Speed"),
    );

    ui.add_space(4.0);
    ui.label("Leaf Wind Response Curves");
    ui.label("Knee Bias: negative responds earlier; positive delays response until stronger wind.");
    ui.add(
        egui::Slider::new(
            &mut adjustables.leaf_paddle_amplitude_wind_start_strength.value,
            adjustables
                .leaf_paddle_amplitude_wind_start_strength
                .range
                .clone(),
        )
        .text("Amplitude Start Wind"),
    );
    ui.add(
        egui::Slider::new(
            &mut adjustables.leaf_paddle_amplitude_wind_full_strength.value,
            adjustables
                .leaf_paddle_amplitude_wind_full_strength
                .range
                .clone(),
        )
        .text("Amplitude Full Wind"),
    );
    ui.add(
        egui::Slider::new(
            &mut adjustables.leaf_paddle_amplitude_wind_knee_bias.value,
            adjustables
                .leaf_paddle_amplitude_wind_knee_bias
                .range
                .clone(),
        )
        .text("Amplitude Knee Bias"),
    );
    ui.add(
        egui::Slider::new(
            &mut adjustables.leaf_paddle_frequency_wind_start_strength.value,
            adjustables
                .leaf_paddle_frequency_wind_start_strength
                .range
                .clone(),
        )
        .text("Frequency Start Wind"),
    );
    ui.add(
        egui::Slider::new(
            &mut adjustables.leaf_paddle_frequency_wind_full_strength.value,
            adjustables
                .leaf_paddle_frequency_wind_full_strength
                .range
                .clone(),
        )
        .text("Frequency Full Wind"),
    );
    ui.add(
        egui::Slider::new(
            &mut adjustables.leaf_paddle_frequency_wind_knee_bias.value,
            adjustables
                .leaf_paddle_frequency_wind_knee_bias
                .range
                .clone(),
        )
        .text("Frequency Knee Bias"),
    );
    ui.add(
        egui::Slider::new(
            &mut adjustables.leaf_paddle_frequency_min_multiplier.value,
            adjustables
                .leaf_paddle_frequency_min_multiplier
                .range
                .clone(),
        )
        .text("Frequency Min Multiplier"),
    );
    ui.add(
        egui::Slider::new(
            &mut adjustables.leaf_paddle_frequency_max_multiplier.value,
            adjustables
                .leaf_paddle_frequency_max_multiplier
                .range
                .clone(),
        )
        .text("Frequency Max Multiplier"),
    );
    enforce_leaf_curve_order(adjustables);
    ui.add_space(4.0);
    draw_leaf_curve_previews(ui, adjustables);

    ui.add_space(4.0);
    ui.label("Grass Colors");
    ui.horizontal(|ui| {
        ui.label("Bottom Dark");
        ui.color_edit_button_srgba(&mut adjustables.grass_bottom_dark_color.value);
    });
    ui.horizontal(|ui| {
        ui.label("Bottom Light");
        ui.color_edit_button_srgba(&mut adjustables.grass_bottom_light_color.value);
    });
    ui.horizontal(|ui| {
        ui.label("Tip Dark");
        ui.color_edit_button_srgba(&mut adjustables.grass_tip_dark_color.value);
    });
    ui.horizontal(|ui| {
        ui.label("Tip Light");
        ui.color_edit_button_srgba(&mut adjustables.grass_tip_light_color.value);
    });
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
    ("Kochia", "Flora"),
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

pub fn render_gui_from_config(
    ui: &mut egui::Ui,
    config: &GuiConfigFile,
    adjustables: &mut GuiAdjustables,
    mut after_section: impl FnMut(&str, &mut egui::Ui),
) {
    for section in &config.section {
        if section.name == "Debug" {
            debug_groups::render(ui, section, adjustables, None);
            continue;
        }
        // If a custom config lacks a parent, keep its children visible at the top level.
        if section_parent(&section.name)
            .is_some_and(|parent| config.section.iter().any(|s| s.name == parent))
        {
            continue;
        }
        ui.collapsing(section_title(&section.name), |ui| {
            render_section_controls(ui, section, adjustables);
            if let Some(debug) = config.section.iter().find(|s| s.name == "Debug") {
                debug_groups::render(ui, debug, adjustables, Some(&section.name));
            }
            after_section(&section.name, ui);
            for child in &config.section {
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
    if section.name == "Sky" {
        ui.label("Scene lighting");
        for id in ["sun_luminance", "sky_light_strength"] {
            if let Some(param) = section.param.iter().find(|param| param.id == id) {
                render_gui_param_from_config(ui, param, &section.name, adjustables);
            }
        }
        ui.small("Sun lights exposed surfaces; sky fills shadows. Changes apply live; indirect light settles over several frames.");
        ui.separator();
        ui.label("Sky appearance & time");
        ui.small("The sky gradient and its mirror image keep their appearance. Clouds use scene lighting. Sun disk brightness does not set surface lighting.");
        for param in &section.param {
            if !matches!(param.id.as_str(), "sun_luminance" | "sky_light_strength") {
                render_gui_param_from_config(ui, param, &section.name, adjustables);
            }
        }
        return;
    }
    if section.name == "Flora" {
        for param in &section.param {
            if !is_custom_flora_param(&param.id) {
                render_gui_param_from_config(ui, param, &section.name, adjustables);
            }
        }
        render_flora_gui(ui, adjustables);
        return;
    }

    for param in &section.param {
        render_gui_param_from_config(ui, param, &section.name, adjustables);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn custom_sections_fall_back_to_generic_rendering_for_unhandled_params() {
        assert!(!is_custom_flora_param("special_flora_plants_per_release"));
        assert!(is_custom_flora_param("grass_natural_bend_min_voxels"));
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
        assert_eq!(settings.config.tree, Some(settings.tree.clone()));

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
