//! Approved input endpoints and ONE replaceable cursor proposal.
//! Observation never commits history. The adapter approves only buildable candidates.
use super::{xz, VOXEL_SIZE};
use glam::Vec3;
const MAX_POINTS: usize = 4096;

pub(in crate::app) struct PathIntent {
    fixed: Vec<Vec3>,
    published: Vec<Vec3>,
    cursor: Vec3,
    moved: bool,
}
impl PathIntent {
    pub(in crate::app) fn new(start: Vec3) -> Self {
        Self {
            fixed: vec![start],
            published: vec![start],
            cursor: start,
            moved: false,
        }
    }
    pub(in crate::app) fn origin(&self) -> Vec3 {
        self.fixed[0]
    }
    pub(in crate::app) fn anchor(&self) -> Vec3 {
        *self.fixed.last().unwrap()
    }
    pub(in crate::app) fn cursor(&self) -> Vec3 {
        self.cursor
    }
    pub(in crate::app) fn points(&self) -> Vec<Vec3> {
        let mut points = self.fixed.clone();
        if self.cursor != self.anchor() {
            points.push(self.cursor);
        }
        points
    }
    pub(in crate::app) fn checkpoint(&self) -> (usize, usize) {
        (self.fixed.len(), self.published.len())
    }
    pub(in crate::app) fn restore_prefix(&mut self, lengths: (usize, usize), cursor: Vec3) {
        self.fixed.truncate(lengths.0);
        self.published.truncate(lengths.1);
        self.cursor = cursor;
        self.moved = true;
    }
    pub(in crate::app) fn floor(&self) -> usize {
        self.fixed.len() - 1
    }
    pub(in crate::app) fn observe(&mut self, point: Vec3) -> bool {
        if !point.is_finite() || point == self.cursor {
            return false;
        }
        self.cursor = point;
        self.moved = true;
        true
    }
    pub(in crate::app) fn take_motion(&mut self) -> bool {
        std::mem::take(&mut self.moved)
    }
    pub(in crate::app) fn can_fix(&self) -> bool {
        self.fixed.len() < MAX_POINTS
    }
    pub(in crate::app) fn spaced(&self, pitch: f32) -> bool {
        xz(self.cursor - self.anchor()).length() >= (pitch * 0.5).max(VOXEL_SIZE * 2.)
    }
    pub(in crate::app) fn fix(&mut self, point: Vec3) {
        assert!(point.is_finite());
        if point != self.anchor() {
            assert!(self.can_fix());
            self.fixed.push(point);
        }
    }
    pub(in crate::app) fn published(&mut self, point: Vec3) {
        if self.published.len() < MAX_POINTS && self.published.last() != Some(&point) {
            self.published.push(point);
        }
    }
    /// Exceptional publication rejection drops only unbaked planning; baked history survives.
    pub(in crate::app) fn discard_unpublished(&mut self) {
        self.fixed.clone_from(&self.published);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejected_points_are_not_fixed_or_revisited_by_the_next_proposal() {
        let mut i = PathIntent::new(Vec3::ONE);
        i.observe(Vec3::ONE + Vec3::X * 0.3);
        i.fix(i.cursor());
        let approved = i.fixed.clone();
        for p in [Vec3::ONE + Vec3::NEG_X, Vec3::ONE + Vec3::Z] {
            assert!(i.observe(p));
            assert_eq!(i.fixed, approved);
            assert_eq!(i.points().len(), approved.len() + 1);
        }
        i.observe(Vec3::ONE + Vec3::X * 0.6);
        i.fix(i.cursor()); // Approval, no retreat.
        assert_eq!(
            i.fixed,
            vec![
                Vec3::ONE,
                Vec3::ONE + Vec3::X * 0.3,
                Vec3::ONE + Vec3::X * 0.6
            ]
        );
    }
    #[test]
    fn stationary_and_nonfinite_observation_never_grow_fixed_history() {
        let mut i = PathIntent::new(Vec3::ONE);
        for p in [Vec3::ONE, Vec3::NAN, Vec3::INFINITY] {
            assert!(!i.observe(p));
        }
        assert_eq!(i.points(), [Vec3::ONE]);
    }
    #[test]
    fn current_preview_is_continuous_but_short_riser_samples_are_not_fixed() {
        let mut i = PathIntent::new(Vec3::ONE);
        assert!(i.observe(Vec3::ONE + Vec3::X * 0.01));
        assert!(!i.spaced(0.05));
        assert_eq!(i.points().len(), 2);
        assert_eq!(i.floor(), 0);
        i.observe(Vec3::ONE + Vec3::X * 0.03);
        assert!(i.spaced(0.05));
    }
    #[test]
    fn publication_failure_keeps_origin_and_actual_published_history_only() {
        let mut i = PathIntent::new(Vec3::ONE);
        i.observe(Vec3::ONE + Vec3::X * 0.6);
        i.fix(i.cursor());
        let baked = Vec3::ONE + Vec3::X * 0.2;
        i.published(baked);
        i.discard_unpublished();
        assert_eq!(i.origin(), Vec3::ONE);
        assert_eq!(i.anchor(), baked);
        assert_eq!(i.cursor(), Vec3::ONE + Vec3::X * 0.6);
    }
    #[test]
    fn capacity_and_huge_cursor_values_cannot_allocate_a_trail() {
        let mut i = PathIntent::new(Vec3::ZERO);
        for k in 1..MAX_POINTS {
            i.observe(Vec3::X * k as f32);
            i.fix(i.cursor());
        }
        assert!(!i.can_fix());
        i.observe(Vec3::splat(f32::MAX));
        assert_eq!(i.fixed.len(), MAX_POINTS);
        assert_eq!(i.points().len(), MAX_POINTS + 1);
        i.observe(Vec3::ZERO);
        assert_eq!(i.fixed.len(), MAX_POINTS);
    }
}
