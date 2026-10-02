pub(super) fn scroll_area() -> egui::ScrollArea {
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .scroll_source(
            egui::containers::scroll_area::ScrollSource::MOUSE_WHEEL
                | egui::containers::scroll_area::ScrollSource::SCROLL_BAR,
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, PointerButton, Pos2, Rect, Vec2};

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
        assert_scrollbar_drag(false);
    }

    #[test]
    fn floating_scrollbar_drag_scrolls_without_moving_window() {
        assert_scrollbar_drag(true);
    }

    fn assert_scrollbar_drag(floating: bool) {
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
                            let output = scroll_area().max_height(180.).show(ui, |ui| {
                                ui.allocate_space(Vec2::new(200., 1800.));
                            });
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
