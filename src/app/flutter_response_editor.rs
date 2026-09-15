//! Direct curve editing over declarative saved fields; no App-only state.
use super::{
    curve_preview::{draw_curve_preview, smoothstep_variant_response},
    gui_config::GuiAdjustables,
};
use egui::{Color32, Pos2, Rect, Sense, Vec2};

#[derive(Clone, Copy)]
pub(super) enum Kind {
    Amplitude,
    Frequency,
}

struct Fields<'a> {
    low: &'a mut f32,
    high: &'a mut f32,
    scale: f32,
    start: &'a mut f32,
    full: &'a mut f32,
    knee: &'a mut f32,
    min: f32,
    max: f32,
    label: &'static str,
    unit: &'static str,
}

fn fields(a: &mut GuiAdjustables, kind: Kind) -> Fields<'_> {
    match kind {
        Kind::Amplitude => Fields {
            low: &mut a.leaf_flutter_amplitude_low.value,
            high: &mut a.leaf_flutter_amplitude_high.value,
            scale: a.leaf_local_displacement_voxels.value,
            start: &mut a.leaf_flutter_wind_start.value,
            full: &mut a.leaf_flutter_wind_full.value,
            knee: &mut a.leaf_flutter_wind_knee.value,
            min: 0.,
            max: 1.,
            label: "Wind → Flutter Amplitude",
            unit: "voxel envelope",
        },
        Kind::Frequency => Fields {
            low: &mut a.leaf_flutter_frequency_low_hz.value,
            high: &mut a.leaf_flutter_frequency_high_hz.value,
            scale: a.leaf_flutter_frequency_multiplier.value,
            start: &mut a.leaf_flutter_frequency_start.value,
            full: &mut a.leaf_flutter_frequency_full.value,
            knee: &mut a.leaf_flutter_frequency_knee.value,
            min: 0.25,
            max: 24.,
            label: "Wind → Flutter Frequency (Hz)",
            unit: "Hz",
        },
    }
}

pub(super) fn draw(ui: &mut egui::Ui, a: &mut GuiAdjustables, kind: Kind) -> Rect {
    let mut f = fields(a, kind);
    let maximum = f.max;
    let (low, high, scale, start, full, knee) =
        (*f.low, *f.high, f.scale, *f.start, *f.full, *f.knee);
    let blend = smoothstep_variant_response((start + full) * 0.5, start, full, knee);
    let response = draw_curve_preview(
        ui,
        f.label,
        0.0..=4.0,
        0.0..=f.max * scale.max(0.001),
        &[],
        |wind| {
            (low + (high - low) * smoothstep_variant_response(wind, start, full, knee))
                * scale.max(0.001)
        },
    );
    let plot = response.rect.shrink2(Vec2::splat(8.0));
    let screen = |x: f32, y: f32| {
        Pos2::new(
            plot.left() + x / 4.0 * plot.width(),
            plot.bottom() - y / maximum * plot.height(),
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
            "{name}: wind {x:.2}, {:.2} {} · drag to edit",
            y * scale,
            f.unit
        ));
        if drag.dragged() {
            if let Some(pointer) = drag.interact_pointer_pos() {
                let wind = ((pointer.x - plot.left()) / plot.width() * 4.0).clamp(0.0, 4.0);
                let value =
                    ((plot.bottom() - pointer.y) / plot.height() * f.max).clamp(f.min, f.max);
                edit_fields(&mut f, index, wind, value);
                ui.ctx().request_repaint();
            }
        }
    }
    plot
}

fn edit_fields(f: &mut Fields<'_>, index: usize, wind: f32, value: f32) {
    match index {
        0 => {
            *f.start = wind.min(*f.full);
            *f.low = value;
        }
        1 => {
            *f.full = wind.max(*f.start);
            *f.high = value;
        }
        _ => {
            let low = *f.low;
            let span = *f.high - low;
            if span.abs() > 0.0001 {
                let target = ((value - low) / span).clamp(0.0, 1.0);
                let (mut lo, mut hi) = (-6.0, 6.0);
                for _ in 0..20 {
                    let mid = (lo + hi) * 0.5;
                    if smoothstep_variant_response(0.5, 0.0, 1.0, mid) > target {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                *f.knee = (lo + hi) * 0.5;
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
                edit_fields(
                    &mut fields(&mut a, Kind::Frequency),
                    2,
                    0.5,
                    low + (high - low) * blend,
                );
                let actual =
                    smoothstep_variant_response(0.5, 0., 1., a.leaf_flutter_frequency_knee.value);
                assert!((actual - blend).abs() < 0.00001);
            }
        }
    }
}
