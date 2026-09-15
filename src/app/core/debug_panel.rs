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
    fn scrollbar_drag_scrolls_content_without_moving_window() {
        assert_scrollbar_drag(false);
    }

    #[test]
    fn floating_scrollbar_drag_scrolls_without_moving_window() {
        assert_scrollbar_drag(true);
    }

    fn assert_scrollbar_drag(floating: bool) {
        let context = egui::Context::default();
        context.style_mut(|style| {
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
