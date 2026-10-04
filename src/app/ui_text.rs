//! Semantic GUI text: callers supply plain text and a responsibility, never styling.
//! Typography and colors have one owner; settings keep their existing storage/search owners.
use egui::{Response, RichText, TextStyle, Ui};

#[derive(Clone, Copy)]
enum Role {
    Label,
    Hint,
    Section,
    Status,
    Warning,
    Title,
}

fn show(ui: &mut Ui, role: Role, text: impl Into<String>) -> Response {
    let font = if matches!(role, Role::Title) {
        TextStyle::Heading
    } else {
        TextStyle::Body
    }
    .resolve(ui.style());
    let color = match role {
        Role::Hint => ui.visuals().weak_text_color(),
        Role::Warning => ui.visuals().warn_fg_color,
        Role::Title => ui.visuals().hyperlink_color,
        _ => ui.visuals().text_color(),
    };
    let mut text = RichText::new(text.into()).font(font).color(color);
    if matches!(role, Role::Section) {
        text = text.strong();
    }
    ui.label(text)
}

macro_rules! text_roles {
    ($($name:ident => $role:ident),+ $(,)?) => {
        $(pub(crate) fn $name(ui: &mut Ui, text: impl Into<String>) -> Response {
            show(ui, Role::$role, text)
        })+
    };
}
text_roles! {
    label => Label,
    hint => Hint,
    section => Section,
    status => Status,
    warning => Warning,
    title => Title,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hints_share_body_font_and_muted_color_without_changing_the_theme() {
        let context = egui::Context::default();
        for size in [14.0, 21.0] {
            context.global_style_mut(|style| {
                style
                    .text_styles
                    .insert(TextStyle::Body, egui::FontId::proportional(size));
                style.visuals.override_text_color = Some(egui::Color32::WHITE);
            });
            let mut expected_color = egui::Color32::TRANSPARENT;
            let output = context.run_ui(egui::RawInput::default(), |ui| {
                let before = ui.style().clone();
                expected_color = ui.visuals().weak_text_color();
                hint(ui, "One explanation");
                hint(ui, String::from("Another explanation"));
                assert_eq!(ui.style(), &before);
            });
            let texts: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| {
                    if let egui::Shape::Text(text) = &shape.shape {
                        Some(text)
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(texts.len(), 2);
            for text in texts {
                assert!(text
                    .galley
                    .job
                    .sections
                    .iter()
                    .all(|s| s.format.font_id.size == size && s.format.color == expected_color));
            }
        }
    }

    #[test]
    fn text_responsibilities_have_consistent_but_distinct_styles() {
        let context = egui::Context::default();
        let mut colors = Vec::new();
        let output = context.run_ui(egui::RawInput::default(), |ui| {
            colors = vec![
                ui.visuals().text_color(),
                ui.visuals().text_color(),
                ui.visuals().warn_fg_color,
                ui.visuals().text_color(),
                ui.visuals().hyperlink_color,
            ];
            label(ui, "A control label");
            status(ui, "A live status");
            warning(ui, "A warning");
            section(ui, "A section");
            title(ui, "A panel title");
        });
        let texts: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Text(t) => Some(t),
                _ => None,
            })
            .collect();
        assert_eq!(texts.len(), 5);
        for (text, color) in texts.iter().zip(colors) {
            assert!(text
                .galley
                .job
                .sections
                .iter()
                .all(|s| s.format.color == color));
        }
        let font = |index: usize| &texts[index].galley.job.sections[0].format.font_id;
        assert_eq!(font(0), font(1));
        assert_eq!(font(0), font(2));
        assert_eq!(font(0), font(3));
        assert!(font(4).size > font(0).size);
    }

    #[test]
    fn gui_explanations_cannot_bypass_semantic_text() {
        fn check(directory: &std::path::Path) {
            for entry in std::fs::read_dir(directory).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    if path.file_name().unwrap() != "generated" {
                        check(&path);
                    }
                } else if path.extension().is_some_and(|e| e == "rs")
                    && path.file_name().unwrap() != "ui_text.rs"
                {
                    let source = std::fs::read_to_string(&path).unwrap();
                    let production = source.split("#[cfg(test)]").next().unwrap();
                    let settings_module = path.components().any(|c| c.as_os_str() == "gui_config")
                        || matches!(
                            path.file_name().unwrap().to_str().unwrap(),
                            "gui_config.rs"
                                | "wind_prototype.rs"
                                | "climbing_plants.rs"
                                | "camera_snapshot_ui.rs"
                                | "snapshot_controls.rs"
                                | "curve_preview.rs"
                        )
                        || path.ends_with("terrain_persistence/selector.rs");
                    let debug_panel = if path.ends_with("core/mod.rs") {
                        production
                            .split("let mut config_panel_open")
                            .nth(1)
                            .and_then(|s| {
                                s.split("self.config_panel_visible = config_panel_open")
                                    .next()
                            })
                    } else {
                        None
                    };
                    for forbidden in [
                        "ui.small(",
                        "ui.weak(",
                        "ui.label(",
                        "ui.heading(",
                        "ui.colored_label(",
                        "RichText::",
                    ] {
                        let scope = if settings_module {
                            production
                        } else if let Some(panel) = debug_panel {
                            panel
                        } else if matches!(forbidden, "ui.small(" | "ui.weak(") {
                            production
                        } else {
                            continue;
                        };
                        assert!(
                            !scope.contains(forbidden),
                            "{} bypasses semantic GUI text with {forbidden}",
                            path.display()
                        );
                    }
                }
            }
        }
        check(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/app"));
    }
}
