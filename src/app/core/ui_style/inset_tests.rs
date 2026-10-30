use super::*;

fn style() -> egui::Style {
    let mut style = egui::Style::default();
    apply_gui_style(&mut style);
    style
}

fn assert_translucent_tint(fill: Color32) {
    assert!(
        fill.a() > 0 && fill.a() < 255,
        "opaque/missing inset: {fill:?}"
    );
    assert_eq!(fill, panel_tint(fill.a()), "insets share the glass tint");
}

#[test]
fn all_control_states_use_translucent_glass_tint() {
    let style = style();
    for visuals in [
        style.visuals.widgets.inactive,
        style.visuals.widgets.hovered,
        style.visuals.widgets.active,
        style.visuals.widgets.open,
    ] {
        assert_translucent_tint(visuals.bg_fill);
        assert_translucent_tint(visuals.weak_bg_fill);
        assert_eq!(visuals.fg_stroke.color, TEXT_COLOR);
    }
    for fill in [
        style.visuals.selection.bg_fill,
        style.visuals.extreme_bg_color,
        style.visuals.code_bg_color,
        style.visuals.text_edit_bg_color.unwrap(),
        style.visuals.faint_bg_color,
        panel_entry_fill(false, false),
        panel_entry_fill(false, true),
        panel_entry_fill(true, false),
        panel_entry_fill(true, true),
    ] {
        assert_translucent_tint(fill);
    }
    assert!(PANEL_LIGHT.a() < PANEL_HOVERED.a());
    assert!(PANEL_HOVERED.a() < PANEL_SELECTED.a());
    assert_eq!(style.visuals.selection.stroke.color, TEXT_COLOR);
}

#[test]
fn translucent_controls_keep_text_readable_and_backdrop_visible() {
    let linear = |x: f32| {
        if x <= 0.04045 {
            x / 12.92
        } else {
            ((x + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance = |rgb: [f32; 3]| rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
    let text_luminance = luminance(
        [TEXT_COLOR.r(), TEXT_COLOR.g(), TEXT_COLOR.b()].map(|c| linear(c as f32 / 255.0)),
    );
    for fill in [PANEL_LIGHT, PANEL_DARK, PANEL_HOVERED, PANEL_SELECTED] {
        let alpha = fill.a() as f32 / 255.0;
        // The regular egui path unassociates gamma-premultiplied colors before
        // linear-space source-over blending (egui_color.slang). Test the actual
        // composite, not the RGB bytes of a translucent color in isolation.
        let tint = [fill.r(), fill.g(), fill.b()].map(|c| linear(c as f32 / fill.a() as f32));
        for backdrop in [[0.0; 3], [1.0; 3], [0.1, 0.35, 0.6]] {
            let composite = std::array::from_fn(|i| tint[i] * alpha + backdrop[i] * (1.0 - alpha));
            let contrast = (luminance(composite) + 0.05) / (text_luminance + 0.05);
            assert!(
                contrast >= 4.5,
                "control contrast: {contrast}, fill {fill:?}"
            );
        }
        // Even the strongest state transmits at least a third of the parent's
        // cell-to-cell contrast; it must never become an opaque colored card.
        assert!(1.0 - alpha >= 1.0 / 3.0);
    }
}

#[test]
fn real_widgets_paint_translucent_insets_over_one_parent_surface() {
    let ctx = egui::Context::default();
    ctx.set_global_style(style());
    let mut text = "Editable".to_owned();
    let mut checked = true;
    let mut value = 0.5;
    let mut output = None;
    for _ in 0..3 {
        output = Some(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                ..Default::default()
            },
            |root_ui| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show_inside(root_ui, |ui| {
                        egui::Frame::new().inner_margin(10).show_matte(ui, |ui| {
                            let _ = ui.button("Button");
                            let _ = ui.selectable_label(true, "Selected");
                            ui.text_edit_singleline(&mut text);
                            ui.checkbox(&mut checked, "Checkbox");
                            ui.add(egui::Slider::new(&mut value, 0.0..=1.0));
                            ui.collapsing("Closed section", |ui| {
                                ui.label("Hidden");
                            });
                            egui::CollapsingHeader::new("Open section")
                                .default_open(true)
                                .show(ui, |ui| {
                                    ui.label("Visible");
                                });
                        });
                    });
            },
        ));
    }
    fn inspect(shape: &egui::Shape, fills: &mut Vec<Color32>, surfaces: &mut usize) {
        match shape {
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    inspect(shape, fills, surfaces);
                }
            }
            egui::Shape::Rect(rect) if rect.fill.a() != 0 => {
                assert_translucent_tint(rect.fill);
                fills.push(rect.fill);
            }
            egui::Shape::Mesh(mesh)
                if mesh.texture_id == crate::egui_renderer::matte_glass::TEXTURE_ID =>
            {
                *surfaces += 1
            }
            _ => {}
        }
    }
    let mut fills = vec![];
    let mut surfaces = 0;
    for shape in output.unwrap().shapes {
        inspect(&shape.shape, &mut fills, &mut surfaces);
    }
    assert_eq!(
        surfaces, 1,
        "controls must reuse their parent's filtered grid"
    );
    assert!(
        fills.len() >= 7,
        "expected real control backgrounds: {fills:?}"
    );
    assert!(fills.contains(&PANEL_LIGHT));
    assert!(fills.contains(&PANEL_DARK));
    assert!(fills.contains(&PANEL_SELECTED));
}
