//! Transient per-flower hover feedback; never part of saved game settings.
use std::collections::BTreeMap;

pub(super) const ANGLE: f32 = 32.0 * std::f32::consts::PI / 180.0;
type Cell = (i32, i32);

#[derive(Default)]
pub(super) struct Hover {
    responses: BTreeMap<Cell, Response>,
    last_seconds: Option<f64>,
    animating: bool,
}

struct Response {
    value: f32,
    tilt: f32,
}

impl Hover {
    pub(super) fn update(&mut self, hit: Option<(Cell, f32)>, seconds: f64) {
        let dt = self
            .last_seconds
            .replace(seconds)
            .map_or(0.0, |previous| (seconds - previous).clamp(0.0, 0.1));
        let blend = (1.0 - (-dt * 20.0).exp()) as f32;
        if let Some((cell, side)) = hit {
            self.responses.entry(cell).or_insert(Response {
                value: 0.0,
                tilt: side * ANGLE,
            });
        }
        self.animating = false;
        self.responses.retain(|&cell, response| {
            let hovered = hit.is_some_and(|(key, _)| key == cell);
            let goal = if hovered { 1.0 } else { 0.0 };
            let tilt_goal = hit
                .filter(|(key, _)| *key == cell)
                .map_or(response.tilt, |(_, side)| side * ANGLE);
            response.value += (goal - response.value) * blend;
            response.tilt += (tilt_goal - response.tilt) * blend;
            if (response.value - goal).abs() < 0.001 {
                response.value = goal;
            }
            if (response.tilt - tilt_goal).abs() < 0.0001 {
                response.tilt = tilt_goal;
            }
            self.animating |= response.value != goal || response.tilt != tilt_goal;
            response.value != 0.0 || hovered
        });
    }

    pub(super) fn offset(&self, cell: Cell, base: f32) -> f32 {
        self.responses
            .get(&cell)
            .map_or(0.0, |r| r.value * (r.tilt - base))
    }

    pub(super) fn is_animating(&self) -> bool {
        self.animating
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hover_is_local_tracks_side_and_returns_to_baseline() {
        let mut hover = Hover::default();
        hover.update(Some(((1, 2), 1.0)), 0.0);
        hover.update(Some(((1, 2), 1.0)), 0.016);
        assert!(hover.offset((1, 2), 0.0) > 0.0);
        assert_eq!(hover.offset((2, 2), 0.0), 0.0);
        for i in 2..40 {
            hover.update(Some(((1, 2), 1.0)), i as f64 * 0.016);
        }
        assert_eq!(hover.offset((1, 2), 0.0), ANGLE);
        for i in 40..80 {
            hover.update(Some(((1, 2), -1.0)), i as f64 * 0.016);
        }
        assert_eq!(hover.offset((1, 2), 0.0), -ANGLE);
        for i in 80..120 {
            hover.update(None, i as f64 * 0.016);
        }
        assert_eq!(hover.offset((1, 2), 0.0), 0.0);
        assert!(!hover.is_animating());
        assert!(hover.responses.is_empty());
    }
}
