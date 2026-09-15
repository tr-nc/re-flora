//! Direct curve editing over declarative saved fields; no App-only state.
use super::{
    curve_preview::{draw_curve_preview, smoothstep_variant_response},
    gui_config::GuiAdjustables,
};
use egui::{Color32, Pos2, Rect, Sense, Vec2};

pub(super) fn draw(ui: &mut egui::Ui, a: &mut GuiAdjustables) -> Rect {
    let low = a.leaf_flutter_frequency_low_hz.value;
    let high = a.leaf_flutter_frequency_high_hz.value;
    let scale = a.leaf_flutter_frequency_multiplier.value;
    let start = a.leaf_flutter_frequency_start.value;
    let full = a.leaf_flutter_frequency_full.value;
    let knee = a.leaf_flutter_frequency_knee.value;
    let blend = smoothstep_variant_response((start + full) * 0.5, start, full, knee);
    let response = draw_curve_preview(
        ui,
        "Wind → Flutter Frequency (Hz)",
        0.0..=4.0,
        0.0..=24.0 * scale,
        &[],
        |wind| (low + (high - low) * smoothstep_variant_response(wind, start, full, knee)) * scale,
    );
    let plot = response.rect.shrink2(Vec2::splat(8.0));
    let screen = |x: f32, y: f32| {
        Pos2::new(
            plot.left() + x / 4.0 * plot.width(),
            plot.bottom() - y / 24.0 * plot.height(),
        )
    };
    let handles = [
        (start, low, "Low"),
        (full, high, "High"),
        ((start + full) * 0.5, low + (high - low) * blend, "Shape"),
    ];
    for (index, (x, y, name)) in handles.into_iter().enumerate() {
        let point = screen(x, y);
        let drag = ui.interact(
            Rect::from_center_size(point, Vec2::splat(18.0)),
            response.id.with(index),
            Sense::drag(),
        );
        ui.painter().circle_filled(
            point,
            5.0,
            if index == 2 {
                Color32::GOLD
            } else {
                Color32::LIGHT_BLUE
            },
        );
        let drag = drag.on_hover_text(format!(
            "{name}: wind {x:.2}, {:.2} Hz · drag to edit",
            y * scale
        ));
        if drag.dragged() {
            if let Some(pointer) = drag.interact_pointer_pos() {
                let wind = ((pointer.x - plot.left()) / plot.width() * 4.0).clamp(0.0, 4.0);
                let hz = ((plot.bottom() - pointer.y) / plot.height() * 24.0).clamp(0.25, 24.0);
                edit_handle(a, index, wind, hz);
                ui.ctx().request_repaint();
            }
        }
    }
    plot
}

fn edit_handle(a: &mut GuiAdjustables, index: usize, wind: f32, hz: f32) {
    match index {
        0 => {
            a.leaf_flutter_frequency_start.value = wind.min(a.leaf_flutter_frequency_full.value);
            a.leaf_flutter_frequency_low_hz.value = hz;
        }
        1 => {
            a.leaf_flutter_frequency_full.value = wind.max(a.leaf_flutter_frequency_start.value);
            a.leaf_flutter_frequency_high_hz.value = hz;
        }
        _ => {
            let low = a.leaf_flutter_frequency_low_hz.value;
            let span = a.leaf_flutter_frequency_high_hz.value - low;
            if span.abs() > 0.0001 {
                let target = ((hz - low) / span).clamp(0.0, 1.0);
                let (mut lo, mut hi) = (-6.0, 6.0);
                for _ in 0..20 {
                    let mid = (lo + hi) * 0.5;
                    if smoothstep_variant_response(0.5, 0.0, 1.0, mid) > target {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                a.leaf_flutter_frequency_knee.value = (lo + hi) * 0.5;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shape_handle_supports_increasing_and_decreasing_curves() {
        let mut a =
            GuiAdjustables::from_config(&crate::app::gui_config_loader::GuiConfigLoader::load());
        for (low, high) in [(2., 8.), (8., 2.)] {
            a.leaf_flutter_frequency_low_hz.value = low;
            a.leaf_flutter_frequency_high_hz.value = high;
            for blend in [0.2, 0.5, 0.8] {
                edit_handle(&mut a, 2, 0.5, low + (high - low) * blend);
                let actual =
                    smoothstep_variant_response(0.5, 0., 1., a.leaf_flutter_frequency_knee.value);
                assert!((actual - blend).abs() < 0.00001);
            }
        }
    }
}
