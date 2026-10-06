//! Compact themed FPS history. Samples the existing unscaled display FPS, not
//! simulation speed or GPU timings; bounded session telemetry is never saved.
use super::ui_style::{FLOWER_ACCENT, GOLD_ACCENT, PANEL_DARK, SAGE_ACCENT, SHADOW_COLOR};
use std::collections::VecDeque;

const HISTORY_SECONDS: f64 = 30.0;
const SAMPLE_SECONDS: f64 = 0.5;
const MAX_SAMPLES: usize = 62;
const CONTENT_WIDTH: f32 = 248.0;

#[derive(Clone, Copy, Debug)]
struct Sample {
    time: f64,
    fps: f32,
}

#[derive(Default)]
pub(super) struct FpsHistory {
    samples: VecDeque<Sample>,
}

impl FpsHistory {
    pub(super) fn observe(&mut self, time: f64, fps: f32) {
        if !time.is_finite() || time < 0.0 || !fps.is_finite() || fps <= 0.0 {
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
        self.samples.push_back(Sample { time, fps });
        while self.samples.len() > MAX_SAMPLES {
            self.samples.pop_front();
        }
    }

    fn ceiling(&self, current: f32) -> f32 {
        let maximum = self
            .samples
            .iter()
            .fold(current.max(0.0), |max, p| max.max(p.fps));
        ((f64::from(maximum) / 30.0).ceil().max(2.0) * 30.0).min(f64::from(f32::MAX)) as f32
    }

    fn points(&self, now: f64, ceiling: f32, plot: egui::Rect) -> Vec<egui::Pos2> {
        self.samples
            .iter()
            .filter(|p| now - p.time <= HISTORY_SECONDS)
            .map(|p| {
                let x = (1.0 - (now - p.time) / HISTORY_SECONDS).clamp(0.0, 1.0) as f32;
                let y = (p.fps / ceiling).clamp(0.0, 1.0);
                egui::pos2(
                    plot.left() + x * plot.width(),
                    plot.bottom() - y * plot.height(),
                )
            })
            .collect()
    }
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

pub(super) fn draw(ctx: &egui::Context, history: &FpsHistory, now: f64, fps: f32) {
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
        .interactable(false)
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
                stroke: egui::Stroke::new(2.0, FLOWER_ACCENT),
                ..Default::default()
            }
            .show(ui, |ui| {
                ui.set_width(width);
                ui.set_min_height(132.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("FPS")
                            .monospace()
                            .size(12.0)
                            .color(GOLD_ACCENT),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let value = if fps.is_finite() && fps > 0.0 {
                            format!("{fps:.1}")
                        } else {
                            "—".to_owned()
                        };
                        ui.label(
                            egui::RichText::new(value)
                                .monospace()
                                .size(16.0)
                                .strong()
                                .color(SAGE_ACCENT),
                        );
                    });
                });
                ui.add_space(4.0);
                let (bounds, _) =
                    ui.allocate_exact_size(egui::vec2(width, 64.0), egui::Sense::hover());
                let plot = egui::Rect::from_min_max(
                    bounds.left_top() + egui::vec2(30.0, 4.0),
                    bounds.right_bottom() - egui::vec2(3.0, 4.0),
                );
                let painter = ui.painter();
                let ceiling = history.ceiling(if fps.is_finite() { fps } else { 0.0 });
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
                        format!("{ceiling:.0}"),
                        egui::Align2::LEFT_TOP,
                        bounds.left_top(),
                    ),
                    (
                        "0".to_owned(),
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
                let points = history.points(now, ceiling, plot);
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
    fn history_is_bounded_time_based_and_samples_unchanged_fps() {
        let mut history = FpsHistory::default();
        for frame in 0..36_000 {
            history.observe(frame as f64 / 60.0, 60.0);
        }
        assert!(history.samples.len() <= MAX_SAMPLES);
        assert!(history.samples.len() >= 59);
        assert!(
            history.samples.back().unwrap().time - history.samples.front().unwrap().time
                <= HISTORY_SECONDS
        );
        assert!(history.samples.iter().all(|p| p.fps == 60.0));
    }

    #[test]
    fn invalid_samples_are_ignored_and_long_gaps_are_not_backfilled() {
        let mut history = FpsHistory::default();
        for (time, fps) in [
            (0.0, 0.0),
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
        let mut history = FpsHistory::default();
        history.observe(0.0, 30.0);
        history.observe(10.0, 144.0);
        history.observe(30.0, 60.0);
        let plot = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(300.0, 180.0));
        assert_eq!(history.ceiling(60.0), 150.0);
        let points = history.points(30.0, 150.0, plot);
        assert_eq!(points[0].x, 0.0);
        assert!((points[1].x - 100.0).abs() < 0.001);
        assert_eq!(points[2].x, 300.0);
        assert!(points.iter().all(|p| plot.contains(*p)));
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
            let mut history = FpsHistory::default();
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
                    |ui| draw(ui.ctx(), &history, 2.0, 45.0),
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
