/// GUI Adjustables Configuration
///
/// This file loads GUI parameters from config/gui.toml.
/// The config file is the single source of truth.
use crate::app::gui_config_loader::GuiConfigLoader;
use crate::app::gui_config_model::{GuiConfigFile, GuiParamKind, GuiParamValue};
use crate::wind::WindSource;
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

    pub fn active_wind_sources(wind_sources: &[WindSourceGuiValues]) -> Vec<WindSource> {
        wind_sources
            .iter()
            .map(|values| {
                let mut source = values.source;
                if values.muted {
                    source.gain = 0.0;
                }
                source
            })
            .collect()
    }

    pub fn save_to_config_with_wind_sources(
        &self,
        wind_sources: &[WindSourceGuiValues],
    ) -> std::io::Result<()> {
        let mut config = GuiConfigLoader::load();

        for section in &mut config.section {
            for param in &mut section.param {
                if Self::SAVE_DENYLIST.contains(&param.id.as_str())
                    || is_wind_source_param_id(&param.id)
                {
                    continue;
                }

                if param.id == "wind_source_count" {
                    param.value.set_uint(wind_sources.len() as u32);
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

            if section.name == "Wind" {
                section
                    .param
                    .retain(|param| !is_wind_source_param_id(&param.id));
                section.param.extend(wind_source_params(wind_sources));
            }
        }

        GuiConfigLoader::save(&config)
    }

    #[allow(dead_code)]
    pub fn save_to_config(&self) -> std::io::Result<()> {
        let config = GuiConfigLoader::load();
        let wind_sources = wind_sources_from_config(&config);
        self.save_to_config_with_wind_sources(&wind_sources)
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
pub struct WindSourceGuiValues {
    pub name: String,
    pub muted: bool,
    pub source: WindSource,
}

impl Default for WindSourceGuiValues {
    fn default() -> Self {
        Self {
            name: "Wind Source".to_owned(),
            muted: false,
            source: WindSource::default(),
        }
    }
}

fn is_wind_source_param_id(id: &str) -> bool {
    parse_wind_source_param_id(id).is_some()
}

fn parse_wind_source_param_id(id: &str) -> Option<(usize, &str)> {
    let rest = id.strip_prefix("wind_source_")?;
    let (index, field) = rest.split_once('_')?;
    Some((index.parse().ok()?, field))
}

fn wind_source_params(
    wind_sources: &[WindSourceGuiValues],
) -> Vec<crate::app::gui_config_model::GuiParam> {
    use crate::app::gui_config_model::GuiParam;

    let persisted_count = wind_sources.len().max(4);
    let mut persisted_sources = wind_sources.to_vec();
    persisted_sources.resize_with(persisted_count, WindSourceGuiValues::default);

    let mut params = Vec::new();
    for (index, values) in persisted_sources.iter().enumerate() {
        let prefix = format!("wind_source_{index}");
        params.push(GuiParam {
            id: format!("{prefix}_name"),
            kind: GuiParamKind::String,
            label: format!("Wind Source {} Name", index + 1),
            value: GuiParamValue::String {
                value: values.name.clone(),
            },
        });
        params.push(GuiParam {
            id: format!("{prefix}_muted"),
            kind: GuiParamKind::Bool,
            label: format!("Wind Source {} Active", index + 1),
            value: GuiParamValue::Bool {
                value: values.muted,
            },
        });
        params.push(GuiParam {
            id: format!("{prefix}_direction_deg"),
            kind: GuiParamKind::Float,
            label: format!("Wind Source {} Direction", index + 1),
            value: GuiParamValue::Float {
                value: values.source.direction_degrees,
                min: Some(0.0),
                max: Some(360.0),
            },
        });
        params.push(GuiParam {
            id: format!("{prefix}_speed"),
            kind: GuiParamKind::Float,
            label: format!("Wind Source {} Speed", index + 1),
            value: GuiParamValue::Float {
                value: values.source.speed,
                min: Some(0.0),
                max: Some(4.0),
            },
        });
        params.push(GuiParam {
            id: format!("{prefix}_pattern_scale"),
            kind: GuiParamKind::Float,
            label: format!("Wind Source {} Pattern Scale", index + 1),
            value: GuiParamValue::Float {
                value: values.source.pattern_scale,
                min: Some(0.05),
                max: Some(8.0),
            },
        });
        params.push(GuiParam {
            id: format!("{prefix}_octaves"),
            kind: GuiParamKind::Uint,
            label: format!("Wind Source {} Octaves", index + 1),
            value: GuiParamValue::Uint {
                value: values.source.octaves,
                min: Some(1),
                max: Some(8),
            },
        });
        params.push(GuiParam {
            id: format!("{prefix}_lacunarity"),
            kind: GuiParamKind::Float,
            label: format!("Wind Source {} Lacunarity", index + 1),
            value: GuiParamValue::Float {
                value: values.source.lacunarity,
                min: Some(1.0),
                max: Some(4.0),
            },
        });
        params.push(GuiParam {
            id: format!("{prefix}_persistence"),
            kind: GuiParamKind::Float,
            label: format!("Wind Source {} Persistence", index + 1),
            value: GuiParamValue::Float {
                value: values.source.persistence,
                min: Some(0.0),
                max: Some(1.0),
            },
        });
        params.push(GuiParam {
            id: format!("{prefix}_gain"),
            kind: GuiParamKind::Float,
            label: format!("Wind Source {} Gain", index + 1),
            value: GuiParamValue::Float {
                value: values.source.gain,
                min: Some(0.0),
                max: Some(8.0),
            },
        });
    }
    params
}

pub fn wind_sources_from_config(config: &GuiConfigFile) -> Vec<WindSourceGuiValues> {
    let Some(section) = config.section.iter().find(|section| section.name == "Wind") else {
        return Vec::new();
    };

    let mut count = None;
    let mut max_index = None;
    let mut sources = Vec::<WindSourceGuiValues>::new();

    for param in &section.param {
        if param.id == "wind_source_count" {
            if let GuiParamValue::Uint { value, .. } = param.value {
                count = Some(value as usize);
            }
            continue;
        }

        let Some((index, field)) = parse_wind_source_param_id(&param.id) else {
            continue;
        };
        max_index = Some(max_index.map_or(index, |max: usize| max.max(index)));
        if sources.len() <= index {
            sources.resize_with(index + 1, WindSourceGuiValues::default);
        }
        let values = &mut sources[index];
        match (field, &param.value) {
            ("name", GuiParamValue::String { value }) => values.name = value.clone(),
            ("muted", GuiParamValue::Bool { value }) => values.muted = *value,
            ("direction_deg", GuiParamValue::Float { value, .. }) => {
                values.source.direction_degrees = *value
            }
            ("speed", GuiParamValue::Float { value, .. }) => values.source.speed = *value,
            ("pattern_scale", GuiParamValue::Float { value, .. }) => {
                values.source.pattern_scale = *value
            }
            ("octaves", GuiParamValue::Uint { value, .. }) => values.source.octaves = *value,
            ("lacunarity", GuiParamValue::Float { value, .. }) => values.source.lacunarity = *value,
            ("persistence", GuiParamValue::Float { value, .. }) => {
                values.source.persistence = *value
            }
            ("gain", GuiParamValue::Float { value, .. }) => values.source.gain = *value,
            _ => {}
        }
    }

    let desired_count = count
        .or_else(|| max_index.map(|index| index + 1))
        .unwrap_or(0);
    sources.resize_with(desired_count, WindSourceGuiValues::default);
    sources.truncate(desired_count);
    for (index, source) in sources.iter_mut().enumerate() {
        if source.name == "Wind Source" {
            source.name = format!("Wind Source {}", index + 1);
        }
    }
    sources
}

fn delete_wind_source(wind_sources: &mut Vec<WindSourceGuiValues>, index: usize) {
    if index < wind_sources.len() {
        wind_sources.remove(index);
    }
}

fn render_wind_sources_gui(
    ui: &mut egui::Ui,
    adjustables: &mut GuiAdjustables,
    wind_sources: &mut Vec<WindSourceGuiValues>,
) {
    adjustables.wind_source_count.value = wind_sources.len() as u32;

    ui.horizontal(|ui| {
        ui.label(format!("Wind Sources: {}", wind_sources.len()));
        if ui.button("+ Wind Source").clicked() {
            let mut source = WindSourceGuiValues::default();
            source.name = format!("Wind Source {}", wind_sources.len() + 1);
            wind_sources.push(source);
        }
    });

    ui.add(
        egui::Slider::new(
            &mut adjustables.wind_audio_attack_decay.value,
            adjustables.wind_audio_attack_decay.range.clone(),
        )
        .text("Audio Attack Decay (0 slow, 1 fast)"),
    );
    ui.add(
        egui::Slider::new(
            &mut adjustables.wind_audio_release_decay.value,
            adjustables.wind_audio_release_decay.range.clone(),
        )
        .text("Audio Release Decay (0 slow, 1 fast)"),
    );

    if wind_sources.is_empty() {
        ui.label("No wind sources.");
        return;
    }

    let mut delete_index = None;
    for (index, values) in wind_sources.iter_mut().enumerate() {
        ui.add_space(4.0);
        let title = if values.muted {
            format!("{}: {} (inactive)", index + 1, values.name)
        } else {
            format!("{}: {}", index + 1, values.name)
        };
        ui.collapsing(title, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Delete").clicked() {
                    delete_index = Some(index);
                }
            });

            ui.horizontal(|ui| {
                ui.label("Name");
                ui.text_edit_singleline(&mut values.name);
            });
            let mut active = !values.muted;
            if ui.checkbox(&mut active, "Active").changed() {
                values.muted = !active;
            }
            ui.add(
                egui::Slider::new(&mut values.source.direction_degrees, 0.0..=360.0)
                    .text("Direction (deg)"),
            );
            ui.add(egui::Slider::new(&mut values.source.speed, 0.0..=4.0).text("Speed"));
            ui.add(
                egui::Slider::new(&mut values.source.pattern_scale, 0.05..=8.0)
                    .text("Pattern Scale"),
            );
            ui.add(egui::Slider::new(&mut values.source.octaves, 1..=8).text("Octaves"));
            ui.add(egui::Slider::new(&mut values.source.lacunarity, 1.0..=4.0).text("Lacunarity"));
            ui.add(
                egui::Slider::new(&mut values.source.persistence, 0.0..=1.0).text("Persistence"),
            );
            ui.add(egui::Slider::new(&mut values.source.gain, 0.0..=8.0).text("Gain"));
        });
    }

    if let Some(index) = delete_index {
        delete_wind_source(wind_sources, index);
    }
    adjustables.wind_source_count.value = wind_sources.len() as u32;
}

pub fn render_gui_from_config(
    ui: &mut egui::Ui,
    config: &GuiConfigFile,
    adjustables: &mut GuiAdjustables,
    wind_sources: &mut Vec<WindSourceGuiValues>,
) {
    use crate::app::gui_config_model::GuiParamKind;

    for section in &config.section {
        ui.collapsing(&section.name, |ui| {
            if section.name == "Wind" {
                render_wind_sources_gui(ui, adjustables, wind_sources);
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
