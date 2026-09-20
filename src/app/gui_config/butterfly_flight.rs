//! Saved butterfly controls, owned by DebugSettings like the custom tree editor.
use super::saved_controls::SavedControls;
use crate::particles::ButterflyFlightTuning;

pub(crate) fn draw_butterfly_flight_controls(ui: &mut SavedControls<'_>) {
    ui.label("Flight");
    draw_butterfly_flight_tuning(ui);
}

pub(crate) fn draw_butterfly_flight_tuning(ui: &mut SavedControls<'_>) -> [egui::Response; 5] {
    ui.small("Saved with Debug Panel Save. Physics stays at 120 Hz; World Tick is unchanged.");
    ui.small("Butterfly Update FPS controls position, heading and wings together.");
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
    let changed = [&height, &vertical, &sharpness, &speed, &wind]
        .iter()
        .any(|r| r.changed());
    if changed {
        log::info!(
            "[BUTTERFLY_FLIGHT][TUNING] {:?}",
            ui.read(|s| &s.butterfly_flight.tuning)
        );
    }
    [height, vertical, sharpness, speed, wind]
}
