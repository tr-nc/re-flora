pub(super) fn scroll_area() -> egui::ScrollArea {
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .scroll_source(
            egui::containers::scroll_area::ScrollSource::MOUSE_WHEEL
                | egui::containers::scroll_area::ScrollSource::SCROLL_BAR,
        )
}

/// Style only this scroll area's rail and handle, not its settings widgets.
/// Keep egui's native input handling instead of maintaining a custom scroller.
pub(super) fn show_scroll_area<R>(
    area: egui::ScrollArea,
    ui: &mut egui::Ui,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::scroll_area::ScrollAreaOutput<R> {
    let content_style = ui.style().clone();
    ui.scope(|ui| {
        let style = ui.style_mut();
        style.spacing.scroll.floating = false;
        style.spacing.scroll.bar_width = 12.0;
        style.spacing.scroll.bar_inner_margin = 4.0;
        style.spacing.scroll.bar_outer_margin = 0.0;
        style.spacing.scroll.handle_min_length = 24.0;
        style.spacing.scroll.foreground_color = true;
        for (visuals, color) in [
            (
                &mut style.visuals.widgets.inactive,
                super::ui_style::SAGE_ACCENT,
            ),
            (
                &mut style.visuals.widgets.hovered,
                super::ui_style::GOLD_ACCENT,
            ),
            (
                &mut style.visuals.widgets.active,
                super::ui_style::FLOWER_ACCENT,
            ),
        ] {
            visuals.corner_radius = egui::CornerRadius::ZERO;
            visuals.fg_stroke.color = color;
        }
        area.show(ui, |ui| {
            ui.set_style(content_style);
            contents(ui)
        })
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, PointerButton, Pos2, Rect, Vec2};

    #[test]
    fn pixel_scrollbar_is_square_and_does_not_restyle_settings() {
        let context = egui::Context::default();
        context.global_style_mut(super::super::ui_style::apply_gui_style);
        let mut bar_rects = Vec::new();
        fn collect(shape: &egui::Shape, rects: &mut Vec<egui::epaint::RectShape>) {
            match shape {
                egui::Shape::Rect(rect)
                    if rect.fill == super::super::ui_style::SAGE_ACCENT
                        || rect.fill == super::super::ui_style::PANEL_DARK =>
                {
                    rects.push(rect.clone())
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, rects);
                    }
                }
                _ => {}
            }
        }
        for frame in 0..4 {
            let output = context.run_ui(
                egui::RawInput {
                    time: Some(frame as f64),
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 600.))),
                    ..Default::default()
                },
                |ui| {
                    let before = ui.style().clone();
                    let output = show_scroll_area(scroll_area().max_height(180.), ui, |ui| {
                        assert_eq!(ui.style(), &before, "settings inherited scrollbar styling");
                        ui.allocate_space(Vec2::new(200., 1800.));
                    });
                    assert_eq!(
                        ui.style(),
                        &before,
                        "scrollbar styling leaked to later widgets"
                    );
                    assert!(output.inner_rect.height() <= 180.);
                },
            );
            bar_rects.clear();
            for shape in output.shapes {
                collect(&shape.shape, &mut bar_rects);
            }
        }
        let bars: Vec<_> = bar_rects
            .iter()
            .filter(|r| (r.rect.width() - 12.).abs() < 0.1)
            .collect();
        assert_eq!(
            bars.len(),
            2,
            "expected a solid rail and handle: {bar_rects:?}"
        );
        assert!(bars
            .iter()
            .all(|r| r.corner_radius == egui::CornerRadius::ZERO));
        assert!(bars
            .iter()
            .any(|r| r.fill == super::super::ui_style::SAGE_ACCENT));
    }

    #[test]
    fn search_toolbar_does_not_grow_the_debug_window_across_frames() {
        for content_width in [260., 345., 520.] {
            for font_scale in [1., 1.5] {
                let context = egui::Context::default();
                context.global_style_mut(|style| {
                    super::super::ui_style::apply_gui_style(style);
                    for font in style.text_styles.values_mut() {
                        font.size *= font_scale;
                    }
                });
                let mut settings = crate::app::gui_config::DebugSettings::load();
                let mut widths = Vec::new();
                for frame in 0..30 {
                    let modifiers = if frame == 3 {
                        egui::Modifiers::COMMAND
                    } else {
                        egui::Modifiers::NONE
                    };
                    let events = match frame {
                        3 => vec![Event::Key {
                            key: egui::Key::F,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers,
                        }],
                        4 => vec![Event::Text("restaurant true voxels ".repeat(20))],
                        _ => vec![],
                    };
                    let _ = context.run_ui(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                Pos2::ZERO,
                                Vec2::new(1440., 900.),
                            )),
                            modifiers,
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            let context = ui.ctx();
                            let window = egui::Window::new("Debug Panel")
                                .id(egui::Id::new("config_panel"))
                                .default_size(Vec2::new(content_width, 540.))
                                .show(context, |ui| {
                                    settings.search_toolbar(ui);
                                })
                                .unwrap();
                            widths.push(window.response.rect.width());
                        },
                    );
                }
                assert!(
                    widths.iter().skip(3).all(|width| *width <= widths[2] + 1.),
                    "search expanded the window over time: {widths:?}"
                );
                assert!(
                    widths[29] < content_width + 35.,
                    "search exceeded the existing panel width: {widths:?}"
                );
            }
        }
    }

    #[test]
    fn scrollbar_drag_scrolls_content_without_moving_window() {
        assert_scrollbar_drag(false, false);
    }

    #[test]
    fn floating_scrollbar_drag_scrolls_without_moving_window() {
        assert_scrollbar_drag(true, false);
    }

    #[test]
    fn pixel_scrollbar_drag_scrolls_without_moving_window() {
        assert_scrollbar_drag(false, true);
    }

    fn assert_scrollbar_drag(floating: bool, pixel: bool) {
        let context = egui::Context::default();
        context.global_style_mut(|style| {
            style.spacing.scroll.floating = floating;
            style.spacing.scroll.bar_width = 8.;
            style.spacing.scroll.floating_width = 4.;
        });
        let mut inner = Rect::NOTHING;
        let mut window = Rect::NOTHING;
        let mut offset = 0.;
        let mut frame = |events: Vec<Event>| {
            let _ = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 600.))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    let shown = egui::Window::new("Debug Panel")
                        .id(egui::Id::new("config_panel"))
                        .movable(true)
                        .default_pos(Pos2::new(40., 40.))
                        .default_size(Vec2::new(300., 240.))
                        .show(ui.ctx(), |ui| {
                            let area = scroll_area().max_height(180.);
                            let contents = |ui: &mut egui::Ui| {
                                ui.allocate_space(Vec2::new(200., 1800.));
                            };
                            let output = if pixel {
                                show_scroll_area(area, ui, contents)
                            } else {
                                area.show(ui, contents)
                            };
                            inner = output.inner_rect;
                            offset = output.state.offset.y;
                        })
                        .unwrap();
                    window = shown.response.rect;
                },
            );
            (inner, window, offset)
        };
        for _ in 0..4 {
            frame(vec![]);
        }
        let (inner, before, _) = frame(vec![]);
        let from = Pos2::new(
            inner.right() + if floating { -4. } else { 6. },
            inner.top() + 8.,
        );
        frame(vec![Event::PointerMoved(from)]);
        frame(vec![Event::PointerButton {
            pos: from,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        }]);
        let to = from + Vec2::new(0., 60.);
        frame(vec![Event::PointerMoved(to)]);
        let (_, after, offset) = frame(vec![Event::PointerButton {
            pos: to,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }]);
        assert!(
            offset > 100.,
            "scrollbar drag did not scroll: offset={offset}, window_delta={:?}",
            after.min - before.min
        );
        assert!(
            (after.min - before.min).length() < 1.,
            "scrollbar dragged the window"
        );
    }
}
