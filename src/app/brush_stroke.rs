//! Continuous brush sampling independent of the input device and edit backend.
//!
//! Only successful edits advance the anchor. Throttled samples leave it untouched;
//! a miss, rejection, or failure breaks the path instead of bridging empty space.
use super::world_edits::TerrainBrushEdit;
use glam::Vec3;
use std::time::{Duration, Instant};

#[derive(Debug, Default)]
pub(super) struct BrushStroke {
    last_dab_time: Option<Instant>,
    previous_center: Option<Vec3>,
}

impl BrushStroke {
    pub(super) fn ready(&self, now: Instant, interval: Duration) -> bool {
        self.last_dab_time
            .is_none_or(|last| now.saturating_duration_since(last) >= interval)
    }

    pub(super) fn previous_center(&self) -> Option<Vec3> {
        self.previous_center
    }

    pub(super) fn edit(&self, center: Vec3, radius: f32) -> TerrainBrushEdit {
        TerrainBrushEdit {
            start: self.previous_center.unwrap_or(center),
            end: center,
            radius,
        }
    }

    pub(super) fn record_dab(&mut self, now: Instant, center: Vec3) {
        self.last_dab_time = Some(now);
        self.previous_center = Some(center);
    }

    pub(super) fn defer(&mut self, now: Instant) {
        self.last_dab_time = Some(now);
        self.interrupt();
    }

    pub(super) fn interrupt(&mut self) {
        self.previous_center = None;
    }

    pub(super) fn restart(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skipped_samples_connect_from_the_last_successful_edit() {
        let mut stroke = BrushStroke::default();
        let now = Instant::now();
        let interval = Duration::from_millis(80);
        assert!(stroke.ready(now, interval));
        assert_eq!(stroke.edit(Vec3::ZERO, 1.).start, Vec3::ZERO);
        stroke.record_dab(now, Vec3::ZERO);
        assert!(!stroke.ready(now + interval / 2, interval));
        assert!(stroke.ready(now + interval, interval));
        let edit = stroke.edit(Vec3::X * 10., 1.);
        assert_eq!(edit.start, Vec3::ZERO);
        assert_eq!(edit.end, Vec3::X * 10.);
        // Preparing an edit does not commit it.
        assert_eq!(stroke.previous_center(), Some(Vec3::ZERO));
    }

    #[test]
    fn interruption_and_rejection_break_paths_but_keep_cooldown() {
        let mut stroke = BrushStroke::default();
        let now = Instant::now();
        let interval = Duration::from_millis(80);
        stroke.record_dab(now, Vec3::ZERO);
        stroke.interrupt();
        assert_eq!(stroke.edit(Vec3::X, 1.).start, Vec3::X);
        assert!(!stroke.ready(now, interval));
        stroke.defer(now + interval);
        assert!(!stroke.ready(now + interval, interval));
        stroke.restart();
        assert!(stroke.ready(now + interval, interval));
        assert_eq!(stroke.edit(Vec3::Y, 1.).start, Vec3::Y);
    }
}
