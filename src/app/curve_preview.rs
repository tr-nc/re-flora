use std::ops::RangeInclusive;

use egui::{Color32, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Vec2};

const DEFAULT_SAMPLE_COUNT: usize = 96;
const PREVIEW_HEIGHT: f32 = 116.0;
const GRID_LINE_COUNT: usize = 4;

pub(super) struct CurvePreviewMarker<'a> {
    pub x: f32,
    pub label: &'a str,
    pub color: Color32,
}

pub(super) fn smootherstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

pub(super) fn smoothstep_variant_response(
    input: f32,
    start: f32,
    full: f32,
    knee_bias: f32,
) -> f32 {
    let lo = start.min(full);
    let hi = start.max(full);
    let t = if hi - lo <= f32::EPSILON {
        if input >= hi {
            1.0
        } else {
            0.0
        }
    } else {
        ((input - lo) / (hi - lo)).clamp(0.0, 1.0)
    };
    let exponent = 2.0_f32.powf(knee_bias.clamp(-6.0, 6.0));
    smootherstep(t.powf(exponent))
}

pub(super) fn draw_curve_preview(
    ui: &mut egui::Ui,
    label: &str,
    x_range: RangeInclusive<f32>,
    y_range: RangeInclusive<f32>,
    markers: &[CurvePreviewMarker<'_>],
    evaluate: impl Fn(f32) -> f32,
) -> egui::Response {
    ui.label(label);
    let desired_size = Vec2::new(ui.available_width().max(160.0), PREVIEW_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::hover());
    let painter = ui.painter_at(rect);

    let visuals = ui.visuals();
    painter.rect_filled(rect, egui::CornerRadius::same(0), visuals.extreme_bg_color);
    painter.rect_stroke(
        rect,
        egui::CornerRadius::same(0),
        Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color),
        StrokeKind::Inside,
    );

    let plot_rect = rect.shrink2(Vec2::new(8.0, 8.0));
    draw_grid(ui, plot_rect);

    let x_min = *x_range.start();
    let x_max = *x_range.end();
    let y_min = *y_range.start();
    let y_max = *y_range.end();
    let x_span = (x_max - x_min).max(f32::EPSILON);
    let y_span = (y_max - y_min).max(f32::EPSILON);

    let to_screen = |x: f32, y: f32| -> Pos2 {
        let tx = ((x - x_min) / x_span).clamp(0.0, 1.0);
        let ty = ((y - y_min) / y_span).clamp(0.0, 1.0);
        Pos2::new(
            egui::lerp(plot_rect.left()..=plot_rect.right(), tx),
            egui::lerp(plot_rect.bottom()..=plot_rect.top(), ty),
        )
    };

    for marker in markers {
        if marker.x < x_min || marker.x > x_max {
            continue;
        }
        let x = to_screen(marker.x, y_min).x;
        painter.line_segment(
            [
                Pos2::new(x, plot_rect.top()),
                Pos2::new(x, plot_rect.bottom()),
            ],
            Stroke::new(1.0, marker.color),
        );
        painter.text(
            Pos2::new(x + 3.0, plot_rect.top() + 3.0),
            egui::Align2::LEFT_TOP,
            marker.label,
            egui::FontId::monospace(9.0),
            marker.color,
        );
    }

    let points = (0..DEFAULT_SAMPLE_COUNT)
        .map(|sample_index| {
            let t = sample_index as f32 / (DEFAULT_SAMPLE_COUNT - 1) as f32;
            let x = egui::lerp(x_min..=x_max, t);
            to_screen(x, evaluate(x).clamp(y_min, y_max))
        })
        .collect::<Vec<_>>();
    painter.add(Shape::line(
        points,
        Stroke::new(2.0, Color32::from_rgb(180, 230, 140)),
    ));

    painter.text(
        plot_rect.left_bottom() + Vec2::new(0.0, -2.0),
        egui::Align2::LEFT_BOTTOM,
        format!("{x_min:.1}"),
        egui::FontId::monospace(9.0),
        visuals.weak_text_color(),
    );
    painter.text(
        plot_rect.right_bottom() + Vec2::new(0.0, -2.0),
        egui::Align2::RIGHT_BOTTOM,
        format!("{x_max:.1}"),
        egui::FontId::monospace(9.0),
        visuals.weak_text_color(),
    );
    painter.text(
        plot_rect.right_top() + Vec2::new(0.0, 2.0),
        egui::Align2::RIGHT_TOP,
        format!("{y_max:.1}"),
        egui::FontId::monospace(9.0),
        visuals.weak_text_color(),
    );

    response
}

fn draw_grid(ui: &egui::Ui, rect: Rect) {
    let painter = ui.painter_at(rect);
    let color = ui.visuals().faint_bg_color;
    let stroke = Stroke::new(1.0, color);

    for index in 1..GRID_LINE_COUNT {
        let t = index as f32 / GRID_LINE_COUNT as f32;
        let x = egui::lerp(rect.left()..=rect.right(), t);
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            stroke,
        );

        let y = egui::lerp(rect.top()..=rect.bottom(), t);
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            stroke,
        );
    }
}
