use super::*;

fn material_rect(shapes: &[egui::epaint::ClippedShape]) -> Option<Rect> {
    fn find(shape: &Shape) -> Option<Rect> {
        match shape {
            Shape::Callback(c) if c.callback.is::<BackdropBoundary>() => Some(c.rect),
            Shape::Vec(children) => children.iter().find_map(find),
            _ => None,
        }
    }
    shapes.iter().find_map(|s| find(&s.shape))
}

fn assert_grid(rect: Rect, settings: MatteSettings, scale: f32) {
    let cell = settings.cell_pixels(scale) as f32;
    for length in [rect.width(), rect.height()] {
        let cells = length * scale / cell;
        assert!(
            (cells - cells.round()).abs() < 0.002,
            "not whole cells: rect={rect:?}, scale={scale}, cell={cell}, cells={cells}"
        );
    }
}

#[test]
fn fixed_frames_reserve_whole_cells_without_moving_their_contents() {
    for scale in [0.75, 1.0, 1.3, 1.75, 2.0] {
        for cell_points in [4, 12, 21, 48] {
            let ctx = egui::Context::default();
            ctx.set_pixels_per_point(scale);
            let settings = MatteSettings { cell_points };
            settings.apply_to_context(&ctx);
            let mut plain = None;
            let mut glass = None;
            for is_glass in [false, true] {
                let output = ctx.run_ui(Default::default(), |ui| {
                    let frame = Frame::new()
                        .inner_margin(egui::Margin::symmetric(7, 9))
                        .outer_margin(3)
                        .stroke(egui::Stroke::new(1.5, Color32::WHITE));
                    let contents = |ui: &mut Ui| {
                        ui.set_min_size(egui::vec2(103.3, 44.7));
                        ui.button("Unscaled").rect
                    };
                    let shown = if is_glass {
                        frame.show_matte(ui, contents)
                    } else {
                        frame.show(ui, contents)
                    };
                    if is_glass {
                        glass = Some((shown.response.rect, shown.inner));
                    } else {
                        plain = Some((shown.response.rect, shown.inner));
                    }
                });
                if is_glass {
                    let rect = material_rect(&output.shapes).unwrap();
                    assert_grid(rect, settings, output.pixels_per_point);
                    let allocated = glass.unwrap().0 - egui::Margin::same(3);
                    // egui rounds interaction allocations to 1/32 point.
                    assert!(
                        (rect.min - allocated.min).abs().max_elem() <= egui::emath::GUI_ROUNDING
                    );
                    assert!(
                        (rect.max - allocated.max).abs().max_elem() <= egui::emath::GUI_ROUNDING
                    );
                }
            }
            let (plain, glass) = (plain.unwrap(), glass.unwrap());
            assert_eq!(plain.1, glass.1, "content moved or scaled");
            assert!(glass.0.width() >= plain.0.width() - 0.04);
            assert!(glass.0.height() >= plain.0.height() - 0.04);
        }
    }
}

struct WindowFixture {
    ctx: egui::Context,
    time: f64,
    settings: MatteSettings,
    collapsed: bool,
    resizable: bool,
    viewport: egui::Vec2,
}

impl WindowFixture {
    fn new(scale: f32) -> Self {
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(scale);
        Self {
            ctx,
            time: 0.0,
            settings: MatteSettings { cell_points: 12 },
            collapsed: false,
            resizable: true,
            viewport: egui::vec2(1200.0, 900.0),
        }
    }

    fn draw(&mut self, events: Vec<egui::Event>) -> Rect {
        self.time += 0.1;
        self.settings.apply_to_context(&self.ctx);
        let mut response = None;
        let output = self.ctx.run_ui(
            egui::RawInput {
                time: Some(self.time),
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, self.viewport)),
                events,
                ..Default::default()
            },
            |_| {
                response = egui::Window::new("Grid window")
                    .default_pos(egui::pos2(100.0, 80.0))
                    .default_size(egui::vec2(217.3, 151.7))
                    .default_open(!self.collapsed)
                    .resizable(self.resizable)
                    .fade_in(false)
                    .show_matte(
                        &self.ctx,
                        Id::new("grid_window"),
                        Frame::window(&self.ctx.global_style()),
                        |ui| {
                            if self.resizable {
                                ui.set_min_size(ui.max_rect().size());
                            }
                            ui.label("Sharp, ordinary text");
                            ui.set_min_size(egui::vec2(125.0, 63.0));
                        },
                    )
                    .map(|shown| shown.response.rect);
            },
        );
        let response = response.unwrap();
        // egui's first area pass measures invisibly before establishing its size.
        let Some(rect) = material_rect(&output.shapes) else {
            assert!(self.time <= 0.1, "window material disappeared after sizing");
            return response;
        };
        assert_grid(rect, self.settings, output.pixels_per_point);
        assert!((response.min - rect.min).abs().max_elem() < 0.04);
        assert!((response.max - rect.max).abs().max_elem() < 0.04);
        rect
    }

    fn settle(&mut self) -> Rect {
        let mut rect = Rect::NOTHING;
        for _ in 0..5 {
            rect = self.draw(vec![]);
        }
        let next = self.draw(vec![]);
        assert_eq!(next, rect, "size creeps without input");
        rect
    }

    fn press(&mut self, pos: egui::Pos2) {
        self.draw(vec![egui::Event::PointerMoved(pos)]);
        self.draw(vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
    }
}

#[test]
fn windows_include_titles_and_margins_in_grid_size_even_when_collapsed() {
    for scale in [0.75, 1.0, 1.3, 1.75, 2.0] {
        for collapsed in [false, true] {
            for resizable in [false, true] {
                let mut fixture = WindowFixture::new(scale);
                fixture.collapsed = collapsed;
                fixture.resizable = resizable;
                fixture.settle();
                fixture.settings.cell_points = 21;
                fixture.settle();
                fixture.ctx.set_pixels_per_point(1.25);
                fixture.settle();
            }
        }
    }
}

#[test]
fn border_resize_uses_cell_steps_on_all_four_sides_and_keeps_opposite_edge() {
    for (scale, cell_points) in [(1.0, 12), (1.3, 21), (1.75, 12)] {
        for axis in 0..2 {
            for minimum_edge in [false, true] {
                let mut fixture = WindowFixture::new(scale);
                fixture.settings.cell_points = cell_points;
                let before = fixture.settle();
                let step = fixture.settings.cell_pixels(scale) as f32 / scale;
                let mut pointer = before.center();
                pointer[axis] = if minimum_edge {
                    before.min[axis]
                } else {
                    before.max[axis]
                };
                fixture.press(pointer);
                for fraction in [0.2, 0.6, 0.9, 1.1, 1.6, 2.1, 0.6, -0.6, -1.1, -2.1] {
                    let distance = fraction * step;
                    let mut delta = egui::Vec2::ZERO;
                    delta[axis] = if minimum_edge { -distance } else { distance };
                    let after = fixture.draw(vec![egui::Event::PointerMoved(pointer + delta)]);
                    let expected = fraction.round() * step;
                    assert!((after.size()[axis] - before.size()[axis] - expected).abs() < 0.04,
                    "axis={axis}, minimum_edge={minimum_edge}, delta={distance}, before={before:?}, after={after:?}");
                    let fixed = if minimum_edge {
                        after.max[axis] - before.max[axis]
                    } else {
                        after.min[axis] - before.min[axis]
                    };
                    assert!(fixed.abs() < 0.04, "opposite edge moved by {fixed}");
                }
            }
        }
    }
}

#[test]
fn whole_cell_window_size_fits_a_non_multiple_viewport() {
    let mut fixture = WindowFixture::new(1.0);
    fixture.viewport = egui::vec2(401.0, 299.0);
    let before = fixture.settle();
    let pointer = before.right_bottom();
    fixture.press(pointer);
    fixture.draw(vec![egui::Event::PointerMoved(
        pointer + egui::vec2(800.0, 800.0),
    )]);
    let rect = fixture.settle();
    assert!(
        rect.width() <= fixture.viewport.x && rect.height() <= fixture.viewport.y,
        "{rect:?}"
    );
}
