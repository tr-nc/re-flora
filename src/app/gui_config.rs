/// GUI Adjustables Configuration
///
/// This file loads GUI parameters from config/gui.toml.
/// The config file is the single source of truth.
use crate::app::gui_config_loader::GuiConfigLoader;
use crate::app::gui_config_model::{GuiConfigFile, GuiParamKind, GuiParamValue};
use crate::gui_adjustables::{BoolParam, FloatParam, StringParam, UintParam};
use crate::wind::{WindSource, MAX_WIND_SOURCES};
use egui::Color32;

mod generated {
    include!("generated/gui_adjustables_gen.rs");
}

pub use generated::GuiAdjustables;

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
    const SAVE_DENYLIST: &'static [&'static str] = &["time_of_day"];

    pub fn active_wind_sources(&self) -> Vec<WindSource> {
        let count = self.wind_source_count.value.min(MAX_WIND_SOURCES as u32) as usize;
        (0..count)
            .filter_map(|index| {
                wind_source_gui_values(self, index)
                    .filter(|values| !values.muted)
                    .map(|values| values.source)
            })
            .collect()
    }

    pub fn save_to_config(&self) -> std::io::Result<()> {
        let mut config = GuiConfigLoader::load();

        for section in &mut config.section {
            for param in &mut section.param {
                if Self::SAVE_DENYLIST.contains(&param.id.as_str()) {
                    continue;
                }

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

        GuiConfigLoader::save(&config)
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

#[derive(Clone)]
struct WindSourceGuiValues {
    name: String,
    muted: bool,
    source: WindSource,
}

fn wind_source_gui_values(
    adjustables: &GuiAdjustables,
    index: usize,
) -> Option<WindSourceGuiValues> {
    let sharpness = adjustables.wind_sharpness.value;
    let values = match index {
        0 => WindSourceGuiValues {
            name: adjustables.wind_source_0_name.value.clone(),
            muted: adjustables.wind_source_0_muted.value,
            source: WindSource::new(
                adjustables.wind_source_0_direction_deg.value,
                adjustables.wind_source_0_speed.value,
                sharpness,
                adjustables.wind_source_0_strength.value,
                adjustables.wind_source_0_coverage.value,
                adjustables.wind_source_0_pattern_scale.value,
                adjustables.wind_source_0_pattern_frequency.value,
                adjustables.wind_source_0_octaves.value,
                adjustables.wind_source_0_lacunarity.value,
                adjustables.wind_source_0_gain.value,
            ),
        },
        1 => WindSourceGuiValues {
            name: adjustables.wind_source_1_name.value.clone(),
            muted: adjustables.wind_source_1_muted.value,
            source: WindSource::new(
                adjustables.wind_source_1_direction_deg.value,
                adjustables.wind_source_1_speed.value,
                sharpness,
                adjustables.wind_source_1_strength.value,
                adjustables.wind_source_1_coverage.value,
                adjustables.wind_source_1_pattern_scale.value,
                adjustables.wind_source_1_pattern_frequency.value,
                adjustables.wind_source_1_octaves.value,
                adjustables.wind_source_1_lacunarity.value,
                adjustables.wind_source_1_gain.value,
            ),
        },
        2 => WindSourceGuiValues {
            name: adjustables.wind_source_2_name.value.clone(),
            muted: adjustables.wind_source_2_muted.value,
            source: WindSource::new(
                adjustables.wind_source_2_direction_deg.value,
                adjustables.wind_source_2_speed.value,
                sharpness,
                adjustables.wind_source_2_strength.value,
                adjustables.wind_source_2_coverage.value,
                adjustables.wind_source_2_pattern_scale.value,
                adjustables.wind_source_2_pattern_frequency.value,
                adjustables.wind_source_2_octaves.value,
                adjustables.wind_source_2_lacunarity.value,
                adjustables.wind_source_2_gain.value,
            ),
        },
        3 => WindSourceGuiValues {
            name: adjustables.wind_source_3_name.value.clone(),
            muted: adjustables.wind_source_3_muted.value,
            source: WindSource::new(
                adjustables.wind_source_3_direction_deg.value,
                adjustables.wind_source_3_speed.value,
                sharpness,
                adjustables.wind_source_3_strength.value,
                adjustables.wind_source_3_coverage.value,
                adjustables.wind_source_3_pattern_scale.value,
                adjustables.wind_source_3_pattern_frequency.value,
                adjustables.wind_source_3_octaves.value,
                adjustables.wind_source_3_lacunarity.value,
                adjustables.wind_source_3_gain.value,
            ),
        },
        _ => return None,
    };
    Some(values)
}

fn set_wind_source_gui_values(
    adjustables: &mut GuiAdjustables,
    index: usize,
    values: WindSourceGuiValues,
) {
    match index {
        0 => {
            adjustables.wind_source_0_name.value = values.name;
            adjustables.wind_source_0_muted.value = values.muted;
            adjustables.wind_source_0_direction_deg.value = values.source.direction_degrees;
            adjustables.wind_source_0_speed.value = values.source.speed;
            adjustables.wind_source_0_strength.value = values.source.strength;
            adjustables.wind_source_0_coverage.value = values.source.coverage;
            adjustables.wind_source_0_pattern_scale.value = values.source.pattern_scale;
            adjustables.wind_source_0_pattern_frequency.value = values.source.pattern_frequency;
            adjustables.wind_source_0_octaves.value = values.source.octaves;
            adjustables.wind_source_0_lacunarity.value = values.source.lacunarity;
            adjustables.wind_source_0_gain.value = values.source.gain;
        }
        1 => {
            adjustables.wind_source_1_name.value = values.name;
            adjustables.wind_source_1_muted.value = values.muted;
            adjustables.wind_source_1_direction_deg.value = values.source.direction_degrees;
            adjustables.wind_source_1_speed.value = values.source.speed;
            adjustables.wind_source_1_strength.value = values.source.strength;
            adjustables.wind_source_1_coverage.value = values.source.coverage;
            adjustables.wind_source_1_pattern_scale.value = values.source.pattern_scale;
            adjustables.wind_source_1_pattern_frequency.value = values.source.pattern_frequency;
            adjustables.wind_source_1_octaves.value = values.source.octaves;
            adjustables.wind_source_1_lacunarity.value = values.source.lacunarity;
            adjustables.wind_source_1_gain.value = values.source.gain;
        }
        2 => {
            adjustables.wind_source_2_name.value = values.name;
            adjustables.wind_source_2_muted.value = values.muted;
            adjustables.wind_source_2_direction_deg.value = values.source.direction_degrees;
            adjustables.wind_source_2_speed.value = values.source.speed;
            adjustables.wind_source_2_strength.value = values.source.strength;
            adjustables.wind_source_2_coverage.value = values.source.coverage;
            adjustables.wind_source_2_pattern_scale.value = values.source.pattern_scale;
            adjustables.wind_source_2_pattern_frequency.value = values.source.pattern_frequency;
            adjustables.wind_source_2_octaves.value = values.source.octaves;
            adjustables.wind_source_2_lacunarity.value = values.source.lacunarity;
            adjustables.wind_source_2_gain.value = values.source.gain;
        }
        3 => {
            adjustables.wind_source_3_name.value = values.name;
            adjustables.wind_source_3_muted.value = values.muted;
            adjustables.wind_source_3_direction_deg.value = values.source.direction_degrees;
            adjustables.wind_source_3_speed.value = values.source.speed;
            adjustables.wind_source_3_strength.value = values.source.strength;
            adjustables.wind_source_3_coverage.value = values.source.coverage;
            adjustables.wind_source_3_pattern_scale.value = values.source.pattern_scale;
            adjustables.wind_source_3_pattern_frequency.value = values.source.pattern_frequency;
            adjustables.wind_source_3_octaves.value = values.source.octaves;
            adjustables.wind_source_3_lacunarity.value = values.source.lacunarity;
            adjustables.wind_source_3_gain.value = values.source.gain;
        }
        _ => {}
    }
}

fn toggle_wind_source_muted(adjustables: &mut GuiAdjustables, index: usize) {
    match index {
        0 => adjustables.wind_source_0_muted.value = !adjustables.wind_source_0_muted.value,
        1 => adjustables.wind_source_1_muted.value = !adjustables.wind_source_1_muted.value,
        2 => adjustables.wind_source_2_muted.value = !adjustables.wind_source_2_muted.value,
        3 => adjustables.wind_source_3_muted.value = !adjustables.wind_source_3_muted.value,
        _ => {}
    }
}

fn delete_wind_source(adjustables: &mut GuiAdjustables, index: usize) {
    let count = adjustables
        .wind_source_count
        .value
        .min(MAX_WIND_SOURCES as u32) as usize;
    if index >= count {
        return;
    }

    for source_index in index..count.saturating_sub(1) {
        if let Some(next_values) = wind_source_gui_values(adjustables, source_index + 1) {
            set_wind_source_gui_values(adjustables, source_index, next_values);
        }
    }
    adjustables.wind_source_count.value = adjustables.wind_source_count.value.saturating_sub(1);
}

fn wind_source_params_mut(
    adjustables: &mut GuiAdjustables,
    index: usize,
) -> Option<(
    &mut StringParam,
    &mut BoolParam,
    &mut FloatParam,
    &mut FloatParam,
    &mut FloatParam,
    &mut FloatParam,
    &mut FloatParam,
    &mut FloatParam,
    &mut UintParam,
    &mut FloatParam,
    &mut FloatParam,
)> {
    match index {
        0 => Some((
            &mut adjustables.wind_source_0_name,
            &mut adjustables.wind_source_0_muted,
            &mut adjustables.wind_source_0_direction_deg,
            &mut adjustables.wind_source_0_speed,
            &mut adjustables.wind_source_0_strength,
            &mut adjustables.wind_source_0_coverage,
            &mut adjustables.wind_source_0_pattern_scale,
            &mut adjustables.wind_source_0_pattern_frequency,
            &mut adjustables.wind_source_0_octaves,
            &mut adjustables.wind_source_0_lacunarity,
            &mut adjustables.wind_source_0_gain,
        )),
        1 => Some((
            &mut adjustables.wind_source_1_name,
            &mut adjustables.wind_source_1_muted,
            &mut adjustables.wind_source_1_direction_deg,
            &mut adjustables.wind_source_1_speed,
            &mut adjustables.wind_source_1_strength,
            &mut adjustables.wind_source_1_coverage,
            &mut adjustables.wind_source_1_pattern_scale,
            &mut adjustables.wind_source_1_pattern_frequency,
            &mut adjustables.wind_source_1_octaves,
            &mut adjustables.wind_source_1_lacunarity,
            &mut adjustables.wind_source_1_gain,
        )),
        2 => Some((
            &mut adjustables.wind_source_2_name,
            &mut adjustables.wind_source_2_muted,
            &mut adjustables.wind_source_2_direction_deg,
            &mut adjustables.wind_source_2_speed,
            &mut adjustables.wind_source_2_strength,
            &mut adjustables.wind_source_2_coverage,
            &mut adjustables.wind_source_2_pattern_scale,
            &mut adjustables.wind_source_2_pattern_frequency,
            &mut adjustables.wind_source_2_octaves,
            &mut adjustables.wind_source_2_lacunarity,
            &mut adjustables.wind_source_2_gain,
        )),
        3 => Some((
            &mut adjustables.wind_source_3_name,
            &mut adjustables.wind_source_3_muted,
            &mut adjustables.wind_source_3_direction_deg,
            &mut adjustables.wind_source_3_speed,
            &mut adjustables.wind_source_3_strength,
            &mut adjustables.wind_source_3_coverage,
            &mut adjustables.wind_source_3_pattern_scale,
            &mut adjustables.wind_source_3_pattern_frequency,
            &mut adjustables.wind_source_3_octaves,
            &mut adjustables.wind_source_3_lacunarity,
            &mut adjustables.wind_source_3_gain,
        )),
        _ => None,
    }
}

fn render_wind_sources_gui(ui: &mut egui::Ui, adjustables: &mut GuiAdjustables) {
    let max_sources = MAX_WIND_SOURCES as u32;
    adjustables.wind_source_count.value = adjustables.wind_source_count.value.min(max_sources);

    ui.horizontal(|ui| {
        ui.label(format!(
            "Wind Sources: {}",
            adjustables.wind_source_count.value
        ));
        if ui
            .add_enabled(
                adjustables.wind_source_count.value < max_sources,
                egui::Button::new("+ Wind Source"),
            )
            .clicked()
        {
            adjustables.wind_source_count.value += 1;
        }
    });

    ui.add(
        egui::Slider::new(
            &mut adjustables.wind_sharpness.value,
            adjustables.wind_sharpness.range.clone(),
        )
        .text("Sharpness"),
    );

    if adjustables.wind_source_count.value == 0 {
        ui.label("No active wind sources.");
        return;
    }

    let mut delete_index = None;
    let mut toggle_mute_index = None;
    for index in 0..adjustables.wind_source_count.value as usize {
        ui.add_space(4.0);
        let source_values = wind_source_gui_values(adjustables, index);
        let title = source_values
            .as_ref()
            .map(|values| {
                if values.muted {
                    format!("{}: {} (muted)", index + 1, values.name)
                } else {
                    format!("{}: {}", index + 1, values.name)
                }
            })
            .unwrap_or_else(|| format!("Wind Source {}", index + 1));
        ui.collapsing(title, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Delete").clicked() {
                    delete_index = Some(index);
                }
                let mute_label = if source_values.as_ref().is_some_and(|values| values.muted) {
                    "Unmute"
                } else {
                    "Mute"
                };
                if ui.button(mute_label).clicked() {
                    toggle_mute_index = Some(index);
                }
            });

            if let Some((
                name,
                muted,
                direction,
                speed,
                strength,
                coverage,
                pattern_scale,
                pattern_frequency,
                octaves,
                lacunarity,
                gain,
            )) = wind_source_params_mut(adjustables, index)
            {
                ui.horizontal(|ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(&mut name.value);
                });
                ui.checkbox(&mut muted.value, "Muted");
                ui.add(
                    egui::Slider::new(&mut direction.value, direction.range.clone())
                        .text("Direction (deg)"),
                );
                ui.add(egui::Slider::new(&mut speed.value, speed.range.clone()).text("Speed"));
                ui.add(
                    egui::Slider::new(&mut strength.value, strength.range.clone()).text("Strength"),
                );
                ui.add(
                    egui::Slider::new(&mut coverage.value, coverage.range.clone()).text("Coverage"),
                );
                ui.add(
                    egui::Slider::new(&mut pattern_scale.value, pattern_scale.range.clone())
                        .text("Pattern Scale"),
                );
                ui.add(
                    egui::Slider::new(
                        &mut pattern_frequency.value,
                        pattern_frequency.range.clone(),
                    )
                    .text("Pattern Frequency"),
                );
                ui.add(
                    egui::Slider::new(&mut octaves.value, octaves.range.clone()).text("Octaves"),
                );
                ui.add(
                    egui::Slider::new(&mut lacunarity.value, lacunarity.range.clone())
                        .text("Lacunarity"),
                );
                ui.add(egui::Slider::new(&mut gain.value, gain.range.clone()).text("Gain"));
            }
        });
    }

    if let Some(index) = toggle_mute_index {
        toggle_wind_source_muted(adjustables, index);
    }
    if let Some(index) = delete_index {
        delete_wind_source(adjustables, index);
    }
}

pub fn render_gui_from_config(
    ui: &mut egui::Ui,
    config: &GuiConfigFile,
    adjustables: &mut GuiAdjustables,
) {
    use crate::app::gui_config_model::GuiParamKind;

    for section in &config.section {
        ui.collapsing(&section.name, |ui| {
            if section.name == "Wind" {
                render_wind_sources_gui(ui, adjustables);
                return;
            }

            for param in &section.param {
                match (&param.kind, &param.value) {
                    (GuiParamKind::Float, GuiParamValue::Float { min, max, .. }) => {
                        let field = GuiAdjustables::get_float_param_mut(adjustables, &param.id)
                            .unwrap_or_else(|| {
                                panic!(
                                    "GUI param '{}' (section '{}') missing FloatParam in GuiAdjustables; rebuild required",
                                    param.id, section.name
                                )
                            });
                        let range = min.unwrap_or(0.0)..=max.unwrap_or(1.0);
                        ui.add(egui::Slider::new(&mut field.value, range).text(&param.label));
                    }
                    (GuiParamKind::Int, GuiParamValue::Int { min, max, .. }) => {
                        let field = GuiAdjustables::get_int_param_mut(adjustables, &param.id)
                            .unwrap_or_else(|| {
                                panic!(
                                    "GUI param '{}' (section '{}') missing IntParam in GuiAdjustables; rebuild required",
                                    param.id, section.name
                                )
                            });
                        let range = min.unwrap_or(0)..=max.unwrap_or(100);
                        ui.add(egui::Slider::new(&mut field.value, range).text(&param.label));
                    }
                    (GuiParamKind::Uint, GuiParamValue::Uint { min, max, .. }) => {
                        let field = GuiAdjustables::get_uint_param_mut(adjustables, &param.id)
                            .unwrap_or_else(|| {
                                panic!(
                                    "GUI param '{}' (section '{}') missing UintParam in GuiAdjustables; rebuild required",
                                    param.id, section.name
                                )
                            });
                        let range = min.unwrap_or(0)..=max.unwrap_or(100);
                        ui.add(egui::Slider::new(&mut field.value, range).text(&param.label));
                    }
                    (GuiParamKind::Choice, GuiParamValue::Choice { options, .. }) => {
                        let field = GuiAdjustables::get_choice_param_mut(adjustables, &param.id)
                            .unwrap_or_else(|| {
                                panic!(
                                    "GUI param '{}' (section '{}') missing ChoiceParam in GuiAdjustables; rebuild required",
                                    param.id, section.name
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
                        let field = GuiAdjustables::get_string_param_mut(adjustables, &param.id)
                            .unwrap_or_else(|| {
                                panic!(
                                    "GUI param '{}' (section '{}') missing StringParam in GuiAdjustables; rebuild required",
                                    param.id, section.name
                                )
                            });
                        ui.horizontal(|ui| {
                            ui.label(&param.label);
                            ui.text_edit_singleline(&mut field.value);
                        });
                    }
                    (GuiParamKind::Bool, GuiParamValue::Bool { .. }) => {
                        let field = GuiAdjustables::get_bool_param_mut(adjustables, &param.id)
                            .unwrap_or_else(|| {
                                panic!(
                                    "GUI param '{}' (section '{}') missing BoolParam in GuiAdjustables; rebuild required",
                                    param.id, section.name
                                )
                            });
                        ui.checkbox(&mut field.value, &param.label);
                    }
                    (GuiParamKind::Color, GuiParamValue::Color { .. }) => {
                        let field = GuiAdjustables::get_color_param_mut(adjustables, &param.id)
                            .unwrap_or_else(|| {
                                panic!(
                                    "GUI param '{}' (section '{}') missing ColorParam in GuiAdjustables; rebuild required",
                                    param.id, section.name
                                )
                            });
                        ui.horizontal(|ui| {
                            ui.label(&param.label);
                            ui.color_edit_button_srgba(&mut field.value);
                        });
                    }
                    _ => unreachable!(
                        "GUI param '{}' (section '{}') has kind that is not supported by the GUI renderer",
                        param.id,
                        section.name
                    ),
                }
            }
        });
    }
}
