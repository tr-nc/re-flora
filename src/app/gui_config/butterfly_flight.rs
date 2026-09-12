//! Saved butterfly controls, owned by DebugSettings like the custom tree editor.
use crate::particles::{ButterflyFlightTuning, ButterflyFlightVariant};

pub(crate) fn draw_butterfly_flight_ab_controls(
    ui: &mut egui::Ui,
    variant: &mut ButterflyFlightVariant,
    tuning: &mut ButterflyFlightTuning,
) -> egui::Response {
    ui.separator();
    ui.label("Flight appearance A/B");
    ui.small("Use Debug Panel Save to keep flight settings across restarts.");
    ui.small("Uncheck to compare original A (without wind drift).");
    let mut darting_block = variant.is_darting_block();
    let response = ui.checkbox(
        &mut darting_block,
        "B: Darting color blocks (A/B experiment)",
    );
    if response.changed() {
        *variant = if darting_block {
            ButterflyFlightVariant::DartingBlock
        } else {
            ButterflyFlightVariant::OriginalSprite
        };
        log::info!(
            "[BUTTERFLY_AB] variant={}",
            if darting_block {
                "B-darting-block"
            } else {
                "A-original-sprite"
            }
        );
    }
    ui.small(if darting_block {
        "B — single-color particle blocks with irregular acceleration bursts"
    } else {
        "A — original butterfly sprites and worm-noise flight"
    });
    ui.small("Switching keeps the same live butterflies, habitats, colors and hard count limit.");
    ui.add_enabled_ui(darting_block, |ui| {
        draw_butterfly_flight_tuning(ui, tuning);
    });
    response
}

pub(crate) fn draw_butterfly_flight_tuning(
    ui: &mut egui::Ui,
    tuning: &mut ButterflyFlightTuning,
) -> [egui::Response; 6] {
    ui.small(
        "B only — saved with Debug Panel Save. Physics stays at 120 Hz; World Tick is unchanged.",
    );
    let frequency = ui.add(egui::Slider::new(&mut tuning.flight_frequency_hz, ButterflyFlightTuning::FREQUENCY_RANGE)
        .text("Shared flight frequency (Hz)").logarithmic(true).step_by(0.05))
        .on_hover_text("One beat updates BOTH vertical intent and displayed position. No independent timer or timing jitter. 0 = continuous display, no voluntary vertical intent. Large displacement may advance BOTH together; display is limited by render FPS.");
    if tuning.flight_frequency_hz > 0.0 {
        ui.small(format!(
            "Shared interval: {:.1} ms — vertical intent + display",
            1000.0 / tuning.flight_frequency_hz
        ));
    } else {
        ui.small("Shared stepping off — continuous display, no vertical intent");
    }
    let height = ui
        .add(
            egui::Slider::new(
                &mut tuning.height_above_ground,
                ButterflyFlightTuning::HEIGHT_RANGE,
            )
            .text("Flight height above ground")
            .step_by(0.005),
        )
        .on_hover_text(
            "World units above the terrain below each butterfly. 0.08 matches the walking player's default eye height. Changes attract flight gradually, never teleport it. Tree-born butterflies descend naturally. Horizontal tempo is kept at its saved value.",
        );
    let vertical = ui.add(egui::Slider::new(&mut tuning.vertical_strength, ButterflyFlightTuning::VERTICAL_RANGE)
        .text("Vertical movement (x)").step_by(0.05))
        .on_hover_text("Higher = stronger up/down steering. 0 removes voluntary vertical motion, but terrain/habitat recovery still works.");
    let sharpness = ui.add(egui::Slider::new(&mut tuning.turn_sharpness, ButterflyFlightTuning::SHARPNESS_RANGE)
        .text("Turn sharpness (x)").step_by(0.05))
        .on_hover_text("Higher = acceleration changes faster; lower = softer turns. Position is always integrated.");
    let speed = ui.add(egui::Slider::new(&mut tuning.speed, ButterflyFlightTuning::SPEED_RANGE)
        .text("Self-flight speed (x)").step_by(0.05))
        .on_hover_text("Scales autonomous cruise, maneuver force and air-relative speed limits only. Wind drift has its own control.");
    let wind = ui.add(egui::Slider::new(&mut tuning.wind_drift, ButterflyFlightTuning::WIND_DRIFT_RANGE)
        .text("Wind drift (x)").step_by(0.05))
        .on_hover_text("Response to the same local wind field as plants and the Wind item. Independent of self-flight speed; 0 lets existing drift settle to zero.");
    let changed = [&frequency, &height, &vertical, &sharpness, &speed, &wind]
        .iter()
        .any(|r| r.changed());
    if changed {
        log::info!("[BUTTERFLY_AB][TUNING] {tuning:?}");
    }
    [frequency, height, vertical, sharpness, speed, wind]
}
