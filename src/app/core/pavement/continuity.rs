//! Actual publication stays consecutive. Candidate rejection is replaceable feedback,
//! not a return-only latch and not permission to skip courses.
use crate::app::path_band::PathSample;
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BlockReason {
    Turn,
    Gap,
    Ground,
    Overlap,
    Protected,
    Sampling,
}
impl BlockReason {
    pub(super) fn message(self) -> &'static str {
        match self {
            Self::Turn => "Turn too tight for this width. Keep holding and move to a gentler target; rejected points are ignored.",
            Self::Gap => "This continuation could not be built. Keep holding and choose another target.",
            Self::Ground => "No supported ground here. Keep holding and move to supported ground.",
            Self::Overlap => "This target crosses existing construction. Keep holding and choose a clear target.",
            Self::Protected => "Existing terrain blocks this target. Keep holding and choose a clear target.",
            Self::Sampling => "This candidate exceeds the sampling limit. Choose a nearer target or release.",
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct BlockedCourse {
    pub row: PathSample,
    pub reason: BlockReason,
}

#[derive(Default)]
pub(super) struct CourseContinuity {
    expected: u32,
    accepted: Option<PathSample>,
    blocked: Option<BlockedCourse>,
}
impl CourseContinuity {
    pub(super) fn blocked(&self) -> Option<BlockedCourse> {
        self.blocked
    }
    pub(super) fn expected(&self) -> u32 {
        self.expected
    }
    pub(super) fn accepted(&self) -> Option<PathSample> {
        self.accepted
    }
    pub(super) fn resume(&mut self) {
        self.blocked = None;
    }
    pub(super) fn next(&mut self, rows: &mut VecDeque<PathSample>) -> Option<PathSample> {
        if self.blocked.is_some() {
            rows.clear();
            return None;
        }
        let row = rows.pop_front()?;
        let reason = if row.index != self.expected {
            Some(BlockReason::Gap)
        } else if !row.valid {
            Some(BlockReason::Turn)
        } else {
            None
        };
        if let Some(reason) = reason {
            self.block(row, reason, rows);
            return None;
        }
        Some(row)
    }
    pub(super) fn accept(&mut self, row: PathSample) {
        debug_assert_eq!(row.index, self.expected);
        self.expected = self.expected.wrapping_add(1);
        self.accepted = Some(row);
    }
    pub(super) fn block(
        &mut self,
        row: PathSample,
        reason: BlockReason,
        rows: &mut VecDeque<PathSample>,
    ) {
        if self
            .blocked
            .is_none_or(|b| b.reason != reason || b.row.index != row.index)
        {
            log::info!("[PAVEMENT] rejected row={} reason={reason:?}; approved guide retained; retry=current_target no_retreat_required=true", row.index);
        }
        self.blocked = Some(BlockedCourse { row, reason });
        rows.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    fn row(index: u32) -> PathSample {
        PathSample::straight(
            Vec3::ONE + Vec3::X * index as f32 * 0.05,
            Vec3::X,
            index as f32 * 0.05,
            index,
            0.05,
        )
    }
    #[test]
    fn invalid_course_ends_the_prefix_instead_of_resuming_later() {
        let mut invalid = row(1);
        invalid.valid = false;
        let mut rows = VecDeque::from([row(0), invalid, row(2), row(3)]);
        let mut c = CourseContinuity::default();
        let first = c.next(&mut rows).unwrap();
        c.accept(first);
        assert!(
            c.next(&mut rows).is_none(),
            "an invalid course must stop publication"
        );
        assert!(c.blocked().is_some());
        assert!(rows.is_empty());
        rows.push_back(row(4));
        assert!(
            c.next(&mut rows).is_none(),
            "holding cannot restart beyond the blocked course"
        );
    }
    #[test]
    fn skipped_course_cannot_become_a_new_start_in_the_same_gesture() {
        let mut rows = VecDeque::from([row(0), row(2)]);
        let mut c = CourseContinuity::default();
        let first = c.next(&mut rows).unwrap();
        c.accept(first);
        assert!(c.next(&mut rows).is_none());
        assert_eq!(c.blocked().unwrap().reason, BlockReason::Gap);
    }
    #[test]
    fn provisional_corner_in_the_adaptable_tail_does_not_block_an_ordinary_turn() {
        use crate::app::path_band::{PathSampling, PathSettings, PathStroke};
        for width in [0.05_f32, 0.06, 0.12, 0.3] {
            let config = PathSettings {
                width,
                pitch: 0.05_f32.min(width),
                sampling: PathSampling::Courses,
            };
            let mut stroke = PathStroke::new(config, Vec3::ONE, Vec3::X).unwrap();
            let mut c = CourseContinuity::default();
            let mut saw_provisional_invalid = false;
            for point in (1..=60)
                .map(|i| Vec3::ONE + Vec3::X * (i as f32 * 0.01))
                .chain((1..=60).map(|i| Vec3::ONE + Vec3::X * 0.6 + Vec3::Z * (i as f32 * 0.01)))
            {
                assert!(stroke.push(point));
                saw_provisional_invalid |= stroke.pending_rows().iter().any(|r| !r.valid);
                assert!(stroke.prepare());
                while let Some(row) = c.next(&mut stroke.rows) {
                    c.accept(row);
                }
                assert!(
                    c.blocked().is_none(),
                    "ordinary 90-degree corner was blocked at width={width}"
                );
            }
            assert!(stroke.finish());
            while let Some(row) = c.next(&mut stroke.rows) {
                c.accept(row);
            }
            assert!(c.blocked().is_none());
            assert!(c.expected() >= 18);
            assert!(
                saw_provisional_invalid,
                "fixture must exercise a transient invalid tail"
            );
        }
    }

    #[test]
    fn candidate_retry_keeps_the_expected_index_after_world_rejection() {
        for reason in [
            BlockReason::Ground,
            BlockReason::Overlap,
            BlockReason::Protected,
            BlockReason::Sampling,
        ] {
            let mut c = CourseContinuity::default();
            let mut rows = VecDeque::from([row(0), row(1)]);
            let failed = c.next(&mut rows).unwrap();
            c.block(failed, reason, &mut rows);
            rows.push_back(row(1));
            assert!(c.next(&mut rows).is_none());
            assert!(rows.is_empty());
            c.resume(); // The adapter has a new validated proposal, not a new path.
            rows.push_back(row(0));
            let retried = c.next(&mut rows).unwrap();
            c.accept(retried);
            assert_eq!(c.expected(), 1);
            assert_eq!(c.accepted().unwrap().index, 0);
        }
    }
}
