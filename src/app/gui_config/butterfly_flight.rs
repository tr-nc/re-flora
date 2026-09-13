//! Saved butterfly controls, owned by DebugSettings like the custom tree editor.
use super::saved_controls::SavedControls;
use crate::particles::{ButterflyFlightTuning, ButterflyFlightVariant};

pub(crate) fn draw_butterfly_flight_ab_controls(ui: &mut SavedControls<'_>) -> egui::Response {
    ui.label("Flight appearance A/B");
    ui.small("Use Debug Panel Save to keep flight settings across restarts.");
    ui.small("Both appearances use the same flight, wind and rhythm.");
    ui.small("Animated sprites compensate for transparent padding; block size stays unchanged.");
    let response = ui.toggle(
        |s| &mut s.butterfly_flight.variant,
        ButterflyFlightVariant::DartingBlock,
        ButterflyFlightVariant::DartingSprite,
        "B: Color blocks (unchecked: animated butterflies)",
    );
    let darting_block = ui.read(|s| &s.butterfly_flight.variant).is_darting_block();
    if response.changed() {
        log::info!(
            "[BUTTERFLY_AB] variant={}",
            if darting_block {
                "B-darting-block"
            } else {
                "A-darting-sprite"
            }
        );
    }
    ui.small(if darting_block {
        "B — single-color particle blocks with irregular acceleration bursts"
    } else {
        "A — animated butterfly sprites with the current darting flight"
    });
    ui.small("Switching keeps the same live butterflies, habitats, colors and hard count limit.");
    ui.enabled(true, |ui| {
        draw_butterfly_flight_tuning(ui);
    });
    response
}

pub(crate) fn draw_butterfly_flight_tuning(ui: &mut SavedControls<'_>) -> [egui::Response; 6] {
    ui.small(
        "Both appearances — saved with Debug Panel Save. Physics stays at 120 Hz; World Tick is unchanged.",
    );
    let frequency = ui.slider(|s| &mut s.butterfly_flight.tuning.flight_frequency_hz, ButterflyFlightTuning::FREQUENCY_RANGE, "Shared flight frequency (Hz)", 0.05, true)
        .on_hover_text("One beat updates BOTH vertical intent and displayed position. No independent timer or timing jitter. 0 = continuous display, no voluntary vertical intent. Large displacement may advance BOTH together; display is limited by render FPS.");
    let tuning = ui.read(|s| &s.butterfly_flight.tuning);
    if tuning.flight_frequency_hz > 0.0 {
        ui.small(format!(
            "Shared interval: {:.1} ms — vertical intent + display",
            1000.0 / tuning.flight_frequency_hz
        ));
    } else {
        ui.small("Shared stepping off — continuous display, no vertical intent");
    }
    let height = ui.slider(|s| &mut s.butterfly_flight.tuning.height_above_ground, ButterflyFlightTuning::HEIGHT_RANGE, "Flight height above ground", 0.005, false)
        .on_hover_text(
            "World units above the terrain below each butterfly. 0.08 matches the walking player's default eye height. Changes attract flight gradually, never teleport it. Tree-born butterflies descend naturally. Horizontal tempo is kept at its saved value.",
        );
    let vertical = ui.slider(|s| &mut s.butterfly_flight.tuning.vertical_strength, ButterflyFlightTuning::VERTICAL_RANGE, "Vertical movement (x)", 0.05, false)
        .on_hover_text("Higher = stronger up/down steering. 0 removes voluntary vertical motion, but terrain/habitat recovery still works.");
    let sharpness = ui.slider(|s| &mut s.butterfly_flight.tuning.turn_sharpness, ButterflyFlightTuning::SHARPNESS_RANGE, "Turn sharpness (x)", 0.05, false)
        .on_hover_text("Higher = acceleration changes faster; lower = softer turns. Position is always integrated.");
    let speed = ui.slider(|s| &mut s.butterfly_flight.tuning.speed, ButterflyFlightTuning::SPEED_RANGE, "Self-flight speed (x)", 0.05, false)
        .on_hover_text("Scales autonomous cruise, maneuver force and air-relative speed limits only. Wind drift has its own control.");
    let wind = ui.slider(|s| &mut s.butterfly_flight.tuning.wind_drift, ButterflyFlightTuning::WIND_DRIFT_RANGE, "Wind drift (x)", 0.05, false)
        .on_hover_text("Response to the same local wind field as plants and the Wind item. Independent of self-flight speed; 0 lets existing drift settle to zero.");
    let changed = [&frequency, &height, &vertical, &sharpness, &speed, &wind]
        .iter()
        .any(|r| r.changed());
    if changed {
        log::info!(
            "[BUTTERFLY_AB][TUNING] {:?}",
            ui.read(|s| &s.butterfly_flight.tuning)
        );
    }
    [frequency, height, vertical, sharpness, speed, wind]
}
