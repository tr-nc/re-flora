/// Declarative GUI Adjustables System
///
/// This module provides a macro-based system for defining GUI-adjustable parameters.
/// Adding a new parameter requires only a single line in the declaration.
///
/// Example usage:
/// ```
/// declare_gui_adjustables! {
///     [Debug] {
///         debug_float: f32 = 0.0, range(0.0..=10.0), "Debug Float",
///     }
/// }
/// ```
use egui::Color32;

/// Helper trait for types that can be rendered as GUI controls
#[allow(dead_code)]
pub trait GuiRenderable {
    fn render(&mut self, ui: &mut egui::Ui, label: &str) -> bool;
}

/// Float slider control
#[derive(Debug, Clone)]
pub struct FloatParam {
    pub value: f32,
    #[allow(dead_code)]
    pub range: std::ops::RangeInclusive<f32>,
}

impl FloatParam {
    pub fn new(value: f32, range: std::ops::RangeInclusive<f32>) -> Self {
        Self { value, range }
    }

    #[allow(dead_code)]
    pub fn get(&self) -> f32 {
        self.value
    }

    #[allow(dead_code)]
    pub fn set(&mut self, value: f32) {
        self.value = value;
    }
}

impl GuiRenderable for FloatParam {
    fn render(&mut self, ui: &mut egui::Ui, label: &str) -> bool {
        ui.add(egui::Slider::new(&mut self.value, self.range.clone()).text(label))
            .changed()
    }
}

/// Integer slider control
#[derive(Debug, Clone)]
pub struct IntParam {
    pub value: i32,
    #[allow(dead_code)]
    pub range: std::ops::RangeInclusive<i32>,
}

impl IntParam {
    pub fn new(value: i32, range: std::ops::RangeInclusive<i32>) -> Self {
        Self { value, range }
    }

    #[allow(dead_code)]
    pub fn get(&self) -> i32 {
        self.value
    }
}

impl GuiRenderable for IntParam {
    fn render(&mut self, ui: &mut egui::Ui, label: &str) -> bool {
        ui.add(egui::Slider::new(&mut self.value, self.range.clone()).text(label))
            .changed()
    }
}

/// Unsigned integer slider control
#[derive(Debug, Clone)]
pub struct UintParam {
    pub value: u32,
    #[allow(dead_code)]
    pub range: std::ops::RangeInclusive<u32>,
}

impl UintParam {
    pub fn new(value: u32, range: std::ops::RangeInclusive<u32>) -> Self {
        Self { value, range }
    }

    #[allow(dead_code)]
    pub fn get(&self) -> u32 {
        self.value
    }
}

impl GuiRenderable for UintParam {
    fn render(&mut self, ui: &mut egui::Ui, label: &str) -> bool {
        ui.add(egui::Slider::new(&mut self.value, self.range.clone()).text(label))
            .changed()
    }
}

/// Editable string control.
#[derive(Debug, Clone)]
pub struct StringParam {
    pub value: String,
}

impl StringParam {
    pub fn new(value: String) -> Self {
        Self { value }
    }

    #[allow(dead_code)]
    pub fn get(&self) -> &str {
        &self.value
    }
}

impl GuiRenderable for StringParam {
    fn render(&mut self, ui: &mut egui::Ui, label: &str) -> bool {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.text_edit_singleline(&mut self.value).changed()
        })
        .inner
    }
}

/// Dropdown choice control. The config renderer owns the displayed option labels.
#[derive(Debug, Clone)]
pub struct ChoiceParam {
    pub value: u32,
}

impl ChoiceParam {
    #[allow(dead_code)]
    pub fn new(value: u32) -> Self {
        Self { value }
    }

    #[allow(dead_code)]
    pub fn get(&self) -> u32 {
        self.value
    }
}

impl GuiRenderable for ChoiceParam {
    fn render(&mut self, ui: &mut egui::Ui, label: &str) -> bool {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.add(egui::DragValue::new(&mut self.value).speed(1.0))
        })
        .inner
        .changed()
    }
}

/// Boolean checkbox control
#[derive(Debug, Clone)]
pub struct BoolParam {
    pub value: bool,
}

impl BoolParam {
    pub fn new(value: bool) -> Self {
        Self { value }
    }

    #[allow(dead_code)]
    pub fn get(&self) -> bool {
        self.value
    }
}

impl GuiRenderable for BoolParam {
    fn render(&mut self, ui: &mut egui::Ui, label: &str) -> bool {
        ui.add(egui::Checkbox::new(&mut self.value, label))
            .changed()
    }
}

/// Color picker control
#[derive(Debug, Clone)]
pub struct ColorParam {
    pub value: Color32,
}

impl ColorParam {
    pub fn new(value: Color32) -> Self {
        Self { value }
    }

    #[allow(dead_code)]
    pub fn get(&self) -> Color32 {
        self.value
    }

    #[allow(dead_code)]
    pub fn get_vec3(&self) -> glam::Vec3 {
        glam::Vec3::new(
            self.value.r() as f32 / 255.0,
            self.value.g() as f32 / 255.0,
            self.value.b() as f32 / 255.0,
        )
    }
}

impl GuiRenderable for ColorParam {
    fn render(&mut self, ui: &mut egui::Ui, label: &str) -> bool {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.color_edit_button_srgba(&mut self.value)
        })
        .inner
        .changed()
    }
}

/// Macro to declaratively define all GUI adjustables
#[macro_export]
macro_rules! declare_gui_adjustables {
    (
        $(
            [$section_name:ident] {
                $(
                    $field_name:ident : $field_type:ty = $default:expr, $control_type:ident$( ( $($control_args:expr),* ) )?, $label:expr
                ),* $(,)?
            }
        ),* $(,)?
    ) => {
        pub struct GuiAdjustables {
            $(
                $(
                    pub $field_name: $field_type,
                )*
            )*
        }

        impl GuiAdjustables {
            /// Render all GUI controls organized by sections
            #[allow(dead_code)]
            pub fn render(&mut self, ui: &mut egui::Ui) {
                use $crate::gui_adjustables::GuiRenderable;
                $(
                    ui.collapsing(stringify!($section_name), |ui| {
                        $(
                            self.$field_name.render(ui, $label);
                        )*
                    });
                )*
            }

            /// Render a specific section
            #[allow(dead_code)]
            pub fn render_section(&mut self, ui: &mut egui::Ui, section: &str) -> bool {
                use $crate::gui_adjustables::GuiRenderable;
                let mut changed = false;
                $(
                    if section == stringify!($section_name) {
                        $(
                            changed |= self.$field_name.render(ui, $label);
                        )*
                    }
                )*
                changed
            }
        }
    };

    // Initialization helpers
    (@init float, $default:expr, $range:expr) => {
        $crate::gui_adjustables::FloatParam::new($default, $range)
    };
    (@init int, $default:expr, $range:expr) => {
        $crate::gui_adjustables::IntParam::new($default, $range)
    };
    (@init uint, $default:expr, $range:expr) => {
        $crate::gui_adjustables::UintParam::new($default, $range)
    };
    (@init choice, $default:expr) => {
        $crate::gui_adjustables::ChoiceParam::new($default)
    };
    (@init bool, $default:expr) => {
        $crate::gui_adjustables::BoolParam::new($default)
    };
    (@init color, $default:expr) => {
        $crate::gui_adjustables::ColorParam::new($default)
    };
}
