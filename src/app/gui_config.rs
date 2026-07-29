/// GUI Adjustables Configuration
///
/// This file loads GUI parameters from config/gui.toml.
/// The config file is the single source of truth.
use crate::app::curve_preview::{
    draw_curve_preview, smoothstep_variant_response, CurvePreviewMarker,
};
use crate::app::gui_config_loader::GuiConfigLoader;
use crate::app::gui_config_model::{
    GuiConfigFile, GuiParam, GuiParamKind, GuiParamValue, TreeGuiConfig,
};
use crate::tree_gen::TreeDesc;
use crate::wind::WindSource;
use egui::Color32;
use std::path::Path;

mod generated {
    include!("generated/gui_adjustables_gen.rs");
}

pub use generated::GuiAdjustables;

pub struct DebugSettings {
    pub config: GuiConfigFile,
    pub adjustables: GuiAdjustables,
    pub wind_sources: Vec<WindSourceGuiValues>,
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
        let wind_sources = wind_sources_from_config(&config);
        let tree = config.tree.clone().unwrap_or_else(|| TreeGuiConfig {
            render_leaves: true,
            desc: TreeDesc::default(),
        });
        Self {
            config,
            adjustables,
            wind_sources,
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
            &self.wind_sources,
            &self.tree.desc,
            self.tree.render_leaves,
        );
    }

    pub fn draw(&mut self, ui: &mut egui::Ui) -> bool {
        let Self {
            config,
            adjustables,
            wind_sources,
            tree,
            ..
        } = self;
        let mut tree_desc_changed = false;
        render_gui_from_config(ui, config, adjustables, wind_sources, |section_name, ui| {
            if section_name == "Debug" {
                ui.collapsing("Tree", |ui| {
                    tree_desc_changed |= tree
                        .desc
                        .edit_by_gui_with_leaves_toggle(ui, Some(&mut tree.render_leaves));
                });
            }
        });

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);
        ui.heading("Audio Ray Tracing");
        ui.checkbox(
            &mut adjustables.audio_ray_tracing_enabled.value,
            "Enable Audio Ray Tracing",
        );

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

    fn write_to_config(
        &self,
        config: &mut GuiConfigFile,
        wind_sources: &[WindSourceGuiValues],
        tree_desc: &TreeDesc,
        render_leaves: bool,
    ) {
        config.tree = Some(TreeGuiConfig {
            render_leaves,
            desc: tree_desc.clone(),
        });

        for section in &mut config.section {
            for param in &mut section.param {
                if is_wind_source_param_id(&param.id) {
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

#[derive(Clone, Debug, PartialEq)]
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

fn is_custom_wind_param(id: &str) -> bool {
    id == "wind_source_count"
        || is_wind_source_param_id(id)
        || matches!(
            id,
            "wind_audio_attack_decay"
                | "wind_audio_release_decay"
                | "wind_directional_bias_fraction"
                | "wind_turbulence_fraction"
        )
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
            wind_sources.push(WindSourceGuiValues {
                name: format!("Wind Source {}", wind_sources.len() + 1),
                ..WindSourceGuiValues::default()
            });
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

    ui.add_space(4.0);
    ui.label("Wind Shape");
    ui.add(
        egui::Slider::new(
            &mut adjustables.wind_directional_bias_fraction.value,
            adjustables.wind_directional_bias_fraction.range.clone(),
        )
        .text("Directional Bias Fraction"),
    );
    ui.add(
        egui::Slider::new(
            &mut adjustables.wind_turbulence_fraction.value,
            adjustables.wind_turbulence_fraction.range.clone(),
        )
        .text("Turbulence Fraction"),
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

fn render_gui_param_from_config(
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

pub fn render_gui_from_config(
    ui: &mut egui::Ui,
    config: &GuiConfigFile,
    adjustables: &mut GuiAdjustables,
    wind_sources: &mut Vec<WindSourceGuiValues>,
    mut after_section: impl FnMut(&str, &mut egui::Ui),
) {
    for section in &config.section {
        ui.collapsing(&section.name, |ui| {
            if section.name == "Wind" {
                for param in &section.param {
                    if !is_custom_wind_param(&param.id) {
                        render_gui_param_from_config(ui, param, &section.name, adjustables);
                    }
                }
                render_wind_sources_gui(ui, adjustables, wind_sources);
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
        });
        after_section(&section.name, ui);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_generic_values_match(config: &GuiConfigFile, adjustables: &GuiAdjustables) {
        for section in &config.section {
            for param in &section.param {
                if param.id == "wind_source_count" || is_wind_source_param_id(&param.id) {
                    continue;
                }

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
        assert!(!is_custom_wind_param("future_wind_setting"));
        assert!(is_custom_wind_param("wind_audio_attack_decay"));
        assert!(is_custom_wind_param("wind_source_0_gain"));
    }

    #[test]
    fn current_debug_settings_write_complete_generic_tree_and_wind_state() {
        let mut settings = DebugSettings::from_config(GuiConfigLoader::load());
        settings.adjustables.time_of_day.value = 0.987;
        settings.adjustables.voxel_dirt_color.value = Color32::from_rgb(12, 34, 56);
        settings.tree.render_leaves = false;
        settings.tree.desc.size = 19.5;
        settings.tree.desc.branching.seed = 9876;
        settings.tree.desc.fruit_swing_speed = 3.25;
        settings.wind_sources = vec![
            WindSourceGuiValues {
                name: "Primary".to_owned(),
                muted: false,
                source: WindSource::new(45.0, 1.5, 2.0, 4, 2.25, 0.6, 1.2),
            },
            WindSourceGuiValues {
                name: "Muted".to_owned(),
                muted: true,
                source: WindSource::new(270.0, 0.75, 0.8, 2, 1.75, 0.4, 0.5),
            },
        ];

        settings.sync_config();

        assert_generic_values_match(&settings.config, &settings.adjustables);
        assert_eq!(
            wind_sources_from_config(&settings.config),
            settings.wind_sources
        );
        assert_eq!(settings.config.tree, Some(settings.tree.clone()));

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("gui.toml");
        settings.save_to_path(&path).unwrap();
        let reloaded = DebugSettings::from_config(GuiConfigLoader::load_from_path(&path));

        assert_generic_values_match(&reloaded.config, &reloaded.adjustables);
        assert_eq!(reloaded.wind_sources, settings.wind_sources);
        assert_eq!(reloaded.tree, settings.tree);
        assert_eq!(
            reloaded.adjustables.voxel_dirt_color.value,
            Color32::from_rgb(12, 34, 56)
        );
        assert_eq!(reloaded.adjustables.time_of_day.value, 0.987);
    }
}
