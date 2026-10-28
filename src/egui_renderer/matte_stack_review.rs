//! Opt-in native overlap fixture. No saved settings or imported assets.
use super::matte_glass::MatteFrameExt;

pub(crate) fn draw_if_requested(ctx: &egui::Context) -> bool {
    let Ok(mode) = std::env::var("RE_FLORA_MATTE_STACK_REVIEW") else {
        return false;
    };
    assert!(matches!(
        mode.as_str(),
        "red"
            | "blue"
            | "red-reverse"
            | "blue-reverse"
            | "red-fade"
            | "blue-fade"
            | "red-clip"
            | "blue-clip"
    ));
    ctx.set_pixels_per_point(1.0);
    let color = if mode.starts_with("red") {
        egui::Color32::from_rgb(230, 35, 35)
    } else {
        egui::Color32::from_rgb(35, 35, 230)
    };
    egui::Area::new(egui::Id::new("matte_stack_review"))
        .fixed_pos(egui::Pos2::ZERO)
        .order(egui::Order::Tooltip)
        .show(ctx, |ui| {
            ui.set_min_size(egui::vec2(640.0, 480.0));
            let origins = [
                egui::pos2(40.0, 40.0),
                egui::pos2(100.0, 100.0),
                egui::pos2(180.0, 160.0),
            ];
            let order = if mode.ends_with("-reverse") {
                [2, 1, 0]
            } else {
                [0, 1, 2]
            };
            for index in order {
                let origin = origins[index];
                let mut panel = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(egui::Rect::from_min_size(origin, egui::vec2(400.0, 260.0))),
                );
                if index == 2 && mode.ends_with("-fade") {
                    panel.set_opacity(0.5);
                }
                if index == 2 && mode.ends_with("-clip") {
                    panel.set_clip_rect(egui::Rect::from_min_max(origin, egui::pos2(280.0, 420.0)));
                }
                egui::Frame::new().show_matte(&mut panel, |ui| {
                    ui.set_min_size(egui::vec2(400.0, 260.0));
                    if index == 0 {
                        ui.painter()
                            .rect_filled(ui.min_rect().shrink(24.0), 0.0, color);
                    }
                    ui.label(format!(
                        "Glass layer {index}: text stays sharp on its own layer"
                    ));
                });
            }
        });
    true
}
