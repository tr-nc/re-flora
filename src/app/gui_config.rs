/// GUI Adjustables Configuration
///
/// This file loads GUI parameters from config/gui.toml.
/// The config file is the single source of truth.
use crate::app::gui_config_loader::GuiConfigLoader;
use crate::app::gui_config_model::{GuiConfigFile, GuiParamKind, GuiParamValue};
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

pub fn render_gui_from_config(
    ui: &mut egui::Ui,
    config: &GuiConfigFile,
    adjustables: &mut GuiAdjustables,
) {
    use crate::app::gui_config_model::GuiParamKind;

    for section in &config.section {
        ui.collapsing(&section.name, |ui| {
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
