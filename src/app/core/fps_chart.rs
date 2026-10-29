//! Shared, bounded session telemetry for the FPS and VRAM tabs. Never saved.
use super::ui_style::{FLOWER_ACCENT, GOLD_ACCENT, PANEL_DARK, SAGE_ACCENT, SHADOW_COLOR};
use crate::egui_renderer::matte_glass::MatteFrameExt;
use std::collections::VecDeque;

// Refresh the numeric readout independently of the lower-frequency history.
pub(super) const DISPLAY_INTERVAL_MS: u64 = 100;
const HISTORY_SECONDS: f64 = 30.0;
const SAMPLE_SECONDS: f64 = 0.5;
const MAX_SAMPLES: usize = 62;
const CONTENT_WIDTH: f32 = 248.0;

#[derive(Clone, Copy, Debug)]
struct Sample {
    time: f64,
    value: f32,
}

#[derive(Default)]
pub(super) struct MetricHistory {
    samples: VecDeque<Sample>,
}

impl MetricHistory {
    pub(super) fn observe(&mut self, time: f64, value: f32) {
        if !time.is_finite() || time < 0.0 || !value.is_finite() || value < 0.0 {
            return;
        }
        if self.samples.back().is_some_and(|last| time < last.time) {
            self.samples.clear();
        }
        while self
            .samples
            .front()
            .is_some_and(|first| time - first.time > HISTORY_SECONDS)
        {
            self.samples.pop_front();
        }
        if self
            .samples
            .back()
            .is_some_and(|last| time - last.time < SAMPLE_SECONDS)
        {
            return;
        }
        self.samples.push_back(Sample { time, value });
        while self.samples.len() > MAX_SAMPLES {
            self.samples.pop_front();
        }
    }

    fn visible_samples(&self, now: f64) -> impl Iterator<Item = &Sample> {
        self.samples
            .iter()
            .filter(move |p| p.time >= now - HISTORY_SECONDS && p.time <= now)
    }

    fn range(&self, now: f64) -> (f64, f64) {
        let mut samples = self.visible_samples(now);
        let Some(first) = samples.next() else {
            return (0.0, 1.0);
        };
        let (minimum, maximum) = samples.fold(
            (f64::from(first.value), f64::from(first.value)),
            |(min, max), p| (min.min(f64::from(p.value)), max.max(f64::from(p.value))),
        );
        let padding = ((maximum - minimum) * 0.1).max(1.0).max(maximum * 1e-6);
        ((minimum - padding).max(0.0), maximum + padding)
    }

    // Interpolate the same line drawn on screen, without inventing samples
    // before/after the available history.
    fn sample_at(&self, now: f64, time: f64) -> Option<Sample> {
        let mut samples = self.visible_samples(now);
        let mut previous = *samples.next()?;
        if time < previous.time {
            return None;
        }
        if time == previous.time {
            return Some(previous);
        }
        for next in samples {
            if time <= next.time {
                let fraction = (time - previous.time) / (next.time - previous.time);
                let value = f64::from(previous.value)
                    + fraction * (f64::from(next.value) - f64::from(previous.value));
                return Some(Sample {
                    time,
                    value: value as f32,
                });
            }
            previous = *next;
        }
        None
    }

    fn points(&self, now: f64, range: (f64, f64), plot: egui::Rect) -> Vec<egui::Pos2> {
        self.visible_samples(now)
            .map(|p| {
                let x = (1.0 - (now - p.time) / HISTORY_SECONDS).clamp(0.0, 1.0) as f32;
                let y =
                    ((f64::from(p.value) - range.0) / (range.1 - range.0)).clamp(0.0, 1.0) as f32;
                egui::pos2(
                    plot.left() + x * plot.width(),
                    plot.bottom() - y * plot.height(),
                )
            })
            .collect()
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Metric {
    #[default]
    Fps,
    Vram,
}

impl Metric {
    fn unit(self) -> &'static str {
        match self {
            Self::Fps => "FPS",
            Self::Vram => "MiB",
        }
    }

    fn format(self, value: f32) -> String {
        match self {
            Self::Fps if value <= 0.0 => "—".to_owned(),
            Self::Fps => format!("{value:.1}"),
            Self::Vram => format!("{value:.0} MiB"),
        }
    }
}

#[derive(Default)]
pub(super) struct TelemetryPanel {
    fps: MetricHistory,
    vram: MetricHistory,
    current_fps: f32,
    current_vram: f32,
    selected: Metric,
}

impl TelemetryPanel {
    pub(super) fn observe(&mut self, time: f64, fps: f32, vram_bytes: u64) {
        self.current_fps = fps;
        self.current_vram = (vram_bytes as f64 / (1024.0 * 1024.0)) as f32;
        if fps.is_finite() && fps > 0.0 {
            self.fps.observe(time, fps);
        }
        // Sample both metrics even when their tab is not selected.
        self.vram.observe(time, self.current_vram);
    }
}

pub(super) fn draw(ctx: &egui::Context, panel: &mut TelemetryPanel, now: f64) {
    draw_chart(
        ctx,
        &panel.fps,
        &panel.vram,
        now,
        panel.current_fps,
        panel.current_vram,
        &mut panel.selected,
    );
}

fn panel_offset(viewport: egui::Rect, toolbar: Option<egui::Rect>, size: egui::Vec2) -> egui::Vec2 {
    let candidate = egui::Rect::from_min_size(
        viewport.right_bottom() - egui::vec2(16.0, 16.0) - size,
        size,
    );
    let lift = toolbar
        .filter(|bar| bar.intersects(candidate))
        .map_or(16.0, |bar| viewport.bottom() - bar.top() + 12.0);
    egui::vec2(-16.0, -lift)
}

fn draw_chart(
    ctx: &egui::Context,
    fps_history: &MetricHistory,
    vram_history: &MetricHistory,
    now: f64,
    fps: f32,
    vram: f32,
    selected: &mut Metric,
) {
    let viewport = ctx.content_rect();
    if viewport.width() < 160.0 || viewport.height() < 192.0 {
        return;
    }
    let width = CONTENT_WIDTH.min(viewport.width() - 56.0);
    let toolbar = ctx.memory(|memory| memory.area_rect("item_panel"));
    // Include frame margins/stroke; a conservative initial height prevents the
    // first sizing pass from putting the expanded panel below the viewport.
    let initial_size = egui::vec2(width + 24.0, 160.0);
    let offset = panel_offset(viewport, toolbar, initial_size);
    egui::Area::new("fps_counter".into())
        .default_size(initial_size)
        .anchor(egui::Align2::RIGHT_BOTTOM, offset)
        .interactable(true)
        .movable(false)
        .sense(egui::Sense::hover())
        .show(ctx, |ui| {
            egui::Frame {
                fill: PANEL_DARK,
                inner_margin: egui::Margin::symmetric(10, 8),
                corner_radius: egui::CornerRadius::same(0),
                shadow: egui::epaint::Shadow {
                    offset: [4, 4],
                    blur: 0,
                    spread: 0,
                    color: SHADOW_COLOR,
                },
                stroke: super::ui_style::panel_stroke(ctx, 2.0, FLOWER_ACCENT),
                ..Default::default()
            }
            .show_matte(ui, |ui| {
                ui.set_width(width);
                ui.set_min_height(132.0);
                ui.horizontal(|ui| {
                    for (metric, label) in [(Metric::Fps, "FPS"), (Metric::Vram, "VRAM")] {
                        let response = ui.selectable_value(
                            selected, metric,
                            egui::RichText::new(label).monospace().size(12.0).color(GOLD_ACCENT),
                        );
                        if metric == Metric::Vram {
                            response.on_hover_text(
                                "This game's live device-local resource allocations, not whole-GPU usage. \
                                 Excludes allocator pool slack, swapchain images and driver overhead. \
                                 On integrated GPUs this may be shared system RAM.",
                            );
                        }
                    }
                    let current = match selected {
                        Metric::Fps => fps,
                        Metric::Vram => vram,
                    };
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(selected.format(current))
                                .monospace().size(16.0).strong().color(SAGE_ACCENT),
                        );
                    });
                });
                let history = match selected {
                    Metric::Fps => fps_history,
                    Metric::Vram => vram_history,
                };
                ui.add_space(4.0);
                let (bounds, response) =
                    ui.allocate_exact_size(egui::vec2(width, 64.0), egui::Sense::hover());
                let plot = egui::Rect::from_min_max(
                    bounds.left_top() + egui::vec2(42.0, 4.0),
                    bounds.right_bottom() - egui::vec2(3.0, 4.0),
                );
                let painter = ui.painter();
                let range = history.range(now);
                for fraction in [0.0, 0.5, 1.0] {
                    let y = plot.bottom() - fraction * plot.height();
                    painter.line_segment(
                        [egui::pos2(plot.left(), y), egui::pos2(plot.right(), y)],
                        egui::Stroke::new(1.0, FLOWER_ACCENT.gamma_multiply(0.2)),
                    );
                }
                for fraction in [0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0] {
                    let x = plot.left() + fraction * plot.width();
                    painter.line_segment(
                        [egui::pos2(x, plot.top()), egui::pos2(x, plot.bottom())],
                        egui::Stroke::new(1.0, SAGE_ACCENT.gamma_multiply(0.15)),
                    );
                }
                for (value, anchor, position) in [
                    (
                        format!("{:.1}", range.1),
                        egui::Align2::LEFT_TOP,
                        bounds.left_top(),
                    ),
                    (
                        format!("{:.1}", range.0),
                        egui::Align2::LEFT_BOTTOM,
                        bounds.left_bottom(),
                    ),
                ] {
                    painter.text(
                        position,
                        anchor,
                        value,
                        egui::FontId::monospace(10.0),
                        SAGE_ACCENT,
                    );
                }
                let points = history.points(now, range, plot);
                let latest = points.last().copied();
                if points.len() > 1 {
                    painter.add(egui::Shape::line(
                        points,
                        egui::Stroke::new(2.0, SAGE_ACCENT),
                    ));
                }
                if let Some(point) = latest {
                    painter.rect_filled(
                        egui::Rect::from_center_size(point, egui::vec2(4.0, 4.0)),
                        0.0,
                        GOLD_ACCENT,
                    );
                }
                if let Some(pointer) = response.hover_pos().filter(|p| plot.contains(*p)) {
                    let fraction = f64::from((pointer.x - plot.left()) / plot.width());
                    let time = now - HISTORY_SECONDS + fraction * HISTORY_SECONDS;
                    if let Some(sample) = history.sample_at(now, time) {
                        let y = ((f64::from(sample.value) - range.0) / (range.1 - range.0)) as f32;
                        let point = egui::pos2(pointer.x, plot.bottom() - y * plot.height());
                        painter.line_segment(
                            [
                                egui::pos2(pointer.x, plot.top()),
                                egui::pos2(pointer.x, plot.bottom()),
                            ],
                            egui::Stroke::new(1.0, GOLD_ACCENT),
                        );
                        painter.circle_filled(point, 3.0, GOLD_ACCENT);
                        response.on_hover_text(format!(
                            "{:.1} {} · {:.1}s ago",
                            sample.value,
                            selected.unit(),
                            now - time
                        ));
                    }
                }
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("-30s")
                            .monospace()
                            .size(10.0)
                            .color(SAGE_ACCENT),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new("now")
                                .monospace()
                                .size(10.0)
                                .color(SAGE_ACCENT),
                        );
                    });
                });
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_tabs_use_scene_backed_glass() {
        let ctx = egui::Context::default();
        for selected in [Metric::Fps, Metric::Vram] {
            let mut panel = TelemetryPanel {
                selected,
                ..Default::default()
            };
            panel.observe(0.0, 60.0, 512 * 1024 * 1024);
            let mut output = None;
            for frame in 0..3 {
                output = Some(ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1280.0, 720.0),
                        )),
                        time: Some(frame as f64),
                        ..Default::default()
                    },
                    |_| draw(&ctx, &mut panel, frame as f64),
                ));
            }
            let output = output.unwrap();
            let meshes = ctx.tessellate(output.shapes, output.pixels_per_point);
            assert!(meshes.iter().any(|primitive| matches!(&primitive.primitive,
                egui::epaint::Primitive::Mesh(mesh) if mesh.texture_id == crate::egui_renderer::matte_glass::TEXTURE_ID)),
                "both telemetry tabs must use the same glass as Debug Panel");
        }
    }

    #[test]
    fn both_tabs_sample_in_background_and_vram_accepts_zero() {
        let mut panel = TelemetryPanel::default();
        panel.observe(0.0, 60.0, 0);
        panel.observe(0.1, 90.0, 1024 * 1024);
        assert_eq!(panel.current_vram, 1.0);
        assert_eq!(panel.vram.samples.len(), 1);
        panel.observe(0.5, 120.0, 512 * 1024 * 1024);
        assert_eq!(panel.fps.samples.len(), 2);
        assert_eq!(panel.vram.samples.len(), 2);
        assert_eq!(panel.vram.samples[0].value, 0.0);
        assert_eq!(panel.vram.samples[1].value, 512.0);
        panel.selected = Metric::Vram;
        panel.observe(1.0, 100.0, 256 * 1024 * 1024);
        assert_eq!(panel.fps.samples.len(), 3);
        assert_eq!(panel.vram.samples.len(), 3);
        assert_eq!(panel.vram.sample_at(1.0, 0.75).unwrap().value, 384.0);
    }

    #[test]
    fn clicking_vram_tab_switches_the_shared_chart() {
        let ctx = egui::Context::default();
        let mut panel = TelemetryPanel::default();
        panel.observe(0.0, 60.0, 512 * 1024 * 1024);
        let mut target = None;
        let mut vram_readout_seen = false;
        for frame in 0..10 {
            let mut events = Vec::new();
            if let Some(pos) = target {
                events.push(egui::Event::PointerMoved(pos));
                if frame == 3 || frame == 4 {
                    events.push(egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: frame == 3,
                        modifiers: egui::Modifiers::default(),
                    });
                }
            }
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280.0, 720.0),
                    )),
                    time: Some(frame as f64 * 0.1),
                    events,
                    ..Default::default()
                },
                |ui| draw(ui.ctx(), &mut panel, 0.0),
            );
            for clipped in output.shapes {
                if let egui::Shape::Text(text) = clipped.shape {
                    if text.galley.text() == "VRAM" {
                        target = Some(text.pos + text.galley.size() * 0.5);
                    }
                    vram_readout_seen |= text.galley.text() == "512 MiB";
                }
            }
        }
        assert!(panel.selected == Metric::Vram);
        assert!(vram_readout_seen);
    }

    #[test]
    fn numeric_readout_refreshes_faster_than_history_sampling() {
        let interval = DISPLAY_INTERVAL_MS as f64 / 1000.0;
        assert!(interval > 0.0 && interval < SAMPLE_SECONDS);
    }

    #[test]
    fn history_is_bounded_time_based_and_samples_unchanged_fps() {
        let mut history = MetricHistory::default();
        for frame in 0..36_000 {
            history.observe(frame as f64 / 60.0, 60.0);
        }
        assert!(history.samples.len() <= MAX_SAMPLES);
        assert!(history.samples.len() >= 59);
        assert!(
            history.samples.back().unwrap().time - history.samples.front().unwrap().time
                <= HISTORY_SECONDS
        );
        assert!(history.samples.iter().all(|p| p.value == 60.0));
    }

    #[test]
    fn invalid_samples_are_ignored_and_long_gaps_are_not_backfilled() {
        let mut history = MetricHistory::default();
        for (time, fps) in [
            (0.0, -1.0),
            (f64::NAN, 60.0),
            (0.0, f32::NAN),
            (0.0, f32::INFINITY),
            (-1.0, 60.0),
        ] {
            history.observe(time, fps);
        }
        assert!(history.samples.is_empty());
        history.observe(1.0, 60.0);
        history.observe(1.1, 45.0);
        assert_eq!(history.samples.len(), 1);
        history.observe(40.0, 20.0);
        assert_eq!(history.samples.len(), 1);
        history.observe(0.0, 30.0);
        assert_eq!(history.samples.len(), 1);
        assert_eq!(history.samples[0].time, 0.0);
    }

    #[test]
    fn plot_uses_elapsed_seconds_and_includes_values_above_60() {
        let mut history = MetricHistory::default();
        history.observe(0.0, 30.0);
        history.observe(10.0, 144.0);
        history.observe(30.0, 60.0);
        let plot = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(300.0, 180.0));
        assert_eq!(history.range(30.0), (18.6, 155.4));
        let points = history.points(30.0, history.range(30.0), plot);
        assert_eq!(points[0].x, 0.0);
        assert!((points[1].x - 100.0).abs() < 0.001);
        assert_eq!(points[2].x, 300.0);
        assert!(points.iter().all(|p| plot.contains(*p)));
    }

    #[test]
    fn range_tracks_only_visible_extrema_with_padding_and_handles_flat_history() {
        let mut history = MetricHistory::default();
        assert_eq!(history.range(0.0), (0.0, 1.0));
        history.observe(0.0, 5.0);
        history.observe(10.0, 100.0);
        history.observe(20.0, 110.0);
        assert_eq!(history.range(35.0), (99.0, 111.0));
        assert_eq!(history.range(45.0), (109.0, 111.0));
        assert_eq!(history.range(51.0), (0.0, 1.0));
        // Samples beyond the displayed time must not influence the axis.
        assert_eq!(history.range(15.0), (0.0, 109.5));
        let mut extreme = MetricHistory::default();
        extreme.observe(0.0, f32::MAX);
        let range = extreme.range(0.0);
        assert!(range.0.is_finite() && range.1.is_finite() && range.1 > range.0);
    }

    #[test]
    fn hover_interpolates_fps_at_x_and_does_not_extrapolate() {
        let mut history = MetricHistory::default();
        history.observe(0.0, 20.0);
        history.observe(10.0, 100.0);
        history.observe(20.0, 120.0);
        assert_eq!(history.sample_at(20.0, 0.0).unwrap().value, 20.0);
        assert_eq!(history.sample_at(20.0, 5.0).unwrap().value, 60.0);
        assert_eq!(history.sample_at(20.0, 15.0).unwrap().value, 110.0);
        assert_eq!(history.sample_at(20.0, 20.0).unwrap().value, 120.0);
        assert!(history.sample_at(20.0, -1.0).is_none());
        assert!(history.sample_at(20.0, 21.0).is_none());
        assert!(history.sample_at(35.0, 5.0).is_none());
        assert_eq!(history.sample_at(35.0, 15.0).unwrap().value, 110.0);
        assert!(MetricHistory::default().sample_at(0.0, 0.0).is_none());
    }

    #[test]
    fn chart_hover_shows_fps_tooltip_through_the_actual_area() {
        let ctx = egui::Context::default();
        ctx.global_style_mut(|style| style.interaction.tooltip_delay = 0.0);
        let mut history = MetricHistory::default();
        history.observe(0.0, 60.0);
        history.observe(15.0, 90.0);
        history.observe(30.0, 120.0);
        let mut pointer = None;
        let mut tooltip_seen = false;
        for frame in 0..10 {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280.0, 720.0),
                    )),
                    time: Some(frame as f64 * 0.1),
                    events: pointer.map(egui::Event::PointerMoved).into_iter().collect(),
                    ..Default::default()
                },
                |ui| {
                    draw_chart(
                        ui.ctx(),
                        &history,
                        &history,
                        30.0,
                        120.0,
                        512.0,
                        &mut Metric::Fps,
                    )
                },
            );
            for clipped in output.shapes {
                match clipped.shape {
                    egui::Shape::Path(path) if path.points.len() == 3 => {
                        pointer = Some(path.points[1]);
                    }
                    egui::Shape::Text(text) => {
                        tooltip_seen |= text.galley.text().contains("90.0 FPS");
                    }
                    _ => {}
                }
            }
        }
        assert!(pointer.is_some(), "history line was not painted");
        assert!(
            tooltip_seen,
            "hover did not show FPS at the cursor's X position"
        );
    }

    #[test]
    fn themed_panel_stays_bounded_and_does_not_grow_each_frame() {
        for (width, height) in [
            (320.0, 240.0),
            (640.0, 360.0),
            (1280.0, 720.0),
            (1920.0, 1080.0),
        ] {
            let ctx = egui::Context::default();
            ctx.global_style_mut(super::super::ui_style::apply_gui_style);
            let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
            let mut history = MetricHistory::default();
            history.observe(1.0, 60.0);
            history.observe(2.0, 45.0);
            let mut previous = None;
            for frame in 0..10 {
                let _ = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(viewport),
                        time: Some(frame as f64 / 60.0),
                        ..Default::default()
                    },
                    |ui| {
                        draw_chart(
                            ui.ctx(),
                            &history,
                            &history,
                            2.0,
                            45.0,
                            512.0,
                            &mut Metric::Fps,
                        )
                    },
                );
                let rect = ctx
                    .memory(|memory| memory.area_rect("fps_counter"))
                    .unwrap();
                assert!(
                    viewport.contains_rect(rect),
                    "frame={frame} viewport={viewport:?} panel={rect:?}"
                );
                assert!(rect.width() < 290.0);
                assert!(rect.height() < 170.0);
                if frame > 2 {
                    assert_eq!(Some(rect.size()), previous);
                }
                previous = Some(rect.size());
            }
        }
    }

    #[test]
    fn narrow_layout_lifts_chart_above_toolbar_without_moving_wide_layout() {
        let size = egui::vec2(268.0, 132.0);
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0));
        let toolbar = egui::Rect::from_min_max(egui::pos2(100.0, 600.0), egui::pos2(1180.0, 704.0));
        let offset = panel_offset(viewport, Some(toolbar), size);
        let chart = egui::Rect::from_min_size(viewport.right_bottom() + offset - size, size);
        assert!(chart.bottom() < toolbar.top());
        let wide = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(2560.0, 1440.0));
        assert_eq!(
            panel_offset(wide, Some(toolbar), size),
            egui::vec2(-16.0, -16.0)
        );
    }
}
