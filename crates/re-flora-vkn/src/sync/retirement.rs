use std::{any::Any, collections::VecDeque};

/// Semantic identity for one ordered frame submission.
///
/// This is deliberately independent of raw fences, command buffers, and queue
/// handles so resource-generation diagnostics remain stable as the sync
/// implementation changes.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct FrameSubmissionId(u64);

impl FrameSubmissionId {
    pub(crate) fn new(serial: u64) -> Self {
        Self(serial)
    }

    pub fn serial(self) -> u64 {
        self.0
    }
}

/// One fence-observed frame completion on the ordered render queue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameCompletion {
    submission: FrameSubmissionId,
    frame_slot: usize,
}

impl FrameCompletion {
    pub(crate) fn new(submission: FrameSubmissionId, frame_slot: usize) -> Self {
        Self {
            submission,
            frame_slot,
        }
    }

    pub fn submission(self) -> FrameSubmissionId {
        self.submission
    }

    pub fn frame_slot(self) -> usize {
        self.frame_slot
    }
}

/// Stable, semantic identity for a runtime GPU-resource generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameResourceGeneration {
    name: &'static str,
    generation: u64,
}

impl FrameResourceGeneration {
    pub fn new(name: &'static str, generation: u64) -> Self {
        Self { name, generation }
    }

    pub fn name(self) -> &'static str {
        self.name
    }

    pub fn generation(self) -> u64 {
        self.generation
    }
}

/// An owned resource generation waiting to be assigned to the frame-completion
/// retirement clock.
///
/// The resident value is type-erased because frame retirement is concerned only
/// with ownership. Buffers, descriptor generations, and image/framebuffer
/// bundles can all use the same interface without a generic transient pool.
pub struct FrameRetirement {
    identity: FrameResourceGeneration,
    resident: Box<dyn Any>,
}

impl FrameRetirement {
    pub fn new<T: 'static>(name: &'static str, generation: u64, resident: T) -> Self {
        Self {
            identity: FrameResourceGeneration::new(name, generation),
            resident: Box::new(resident),
        }
    }

    pub fn identity(&self) -> FrameResourceGeneration {
        self.identity
    }

    pub(crate) fn into_resident(self) -> Box<dyn Any> {
        self.resident
    }
}

struct PendingFrameRetirement {
    retire_after: FrameSubmissionId,
    retirement: FrameRetirement,
}

/// Completion-ordered residency queue used by the swapchain frame manager.
///
/// Entries are retired against the latest frame submission that could have
/// referenced them. Observing a later submission's fence is sufficient because
/// every tracked frame is submitted to the same ordered render queue.
pub(crate) struct FrameRetirementQueue {
    pending: VecDeque<PendingFrameRetirement>,
    completed_through: Option<FrameCompletion>,
}

/// The single ordered submission/completion clock for frame-scoped retirement.
///
/// Submission identity allocation and generation retirement share this module
/// so callers cannot accidentally bind a generation to an unrelated counter.
pub(crate) struct FrameRetirementClock {
    next_submission_serial: u64,
    last_submission: Option<FrameSubmissionId>,
    retirements: FrameRetirementQueue,
}

impl FrameRetirementClock {
    pub(crate) fn new() -> Self {
        Self {
            next_submission_serial: 1,
            last_submission: None,
            retirements: FrameRetirementQueue::new(),
        }
    }

    pub(crate) fn record_submission(&mut self) -> FrameSubmissionId {
        let submission = FrameSubmissionId::new(self.next_submission_serial);
        self.next_submission_serial = self
            .next_submission_serial
            .checked_add(1)
            .expect("frame submission serial overflow");
        self.last_submission = Some(submission);
        submission
    }

    pub(crate) fn retire_after_last_submission(&mut self, retirement: FrameRetirement) {
        self.retirements
            .retire_after(retirement, self.last_submission);
    }

    pub(crate) fn observe_completion(&mut self, completion: FrameCompletion) {
        self.retirements.observe_completion(completion);
    }
}

impl FrameRetirementQueue {
    pub(crate) fn new() -> Self {
        Self {
            pending: VecDeque::new(),
            completed_through: None,
        }
    }

    pub(crate) fn retire_after(
        &mut self,
        retirement: FrameRetirement,
        retire_after: Option<FrameSubmissionId>,
    ) {
        crate::sync::diagnostics::record_frame_retirement_scheduled(
            retirement.identity,
            retire_after,
        );

        let Some(retire_after) = retire_after else {
            Self::release(
                retirement,
                crate::sync::diagnostics::FrameRetirementCompletion::NoSubmission,
            );
            return;
        };

        if let Some(completed) = self.completed_through {
            if retire_after <= completed.submission() {
                Self::release(
                    retirement,
                    crate::sync::diagnostics::FrameRetirementCompletion::Frame(completed),
                );
                return;
            }
        }

        if let Some(last) = self.pending.back() {
            assert!(
                last.retire_after <= retire_after,
                "frame retirements must be scheduled against a monotonic submission clock"
            );
        }
        self.pending.push_back(PendingFrameRetirement {
            retire_after,
            retirement,
        });
    }

    pub(crate) fn observe_completion(&mut self, completion: FrameCompletion) {
        if let Some(previous) = self.completed_through {
            assert!(
                previous.submission() < completion.submission(),
                "frame completions must advance monotonically"
            );
        }
        self.completed_through = Some(completion);

        while self
            .pending
            .front()
            .is_some_and(|pending| pending.retire_after <= completion.submission())
        {
            let pending = self
                .pending
                .pop_front()
                .expect("checked frame retirement disappeared");
            Self::release(
                pending.retirement,
                crate::sync::diagnostics::FrameRetirementCompletion::Frame(completion),
            );
        }
    }

    fn release(
        retirement: FrameRetirement,
        completion: crate::sync::diagnostics::FrameRetirementCompletion,
    ) {
        let FrameRetirement { identity, resident } = retirement;
        drop(resident);
        crate::sync::diagnostics::record_frame_retirement_released(identity, completion);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FrameCompletion, FrameRetirement, FrameRetirementClock,
    };
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    struct DropProbe {
        generation: usize,
        dropped_generation_mask: Arc<AtomicUsize>,
    }

    impl Drop for DropProbe {
        fn drop(&mut self) {
            self.dropped_generation_mask
                .fetch_or(1 << self.generation, Ordering::Relaxed);
        }
    }

    fn retirement(generation: usize, dropped: &Arc<AtomicUsize>) -> FrameRetirement {
        FrameRetirement::new(
            "test.resource",
            generation as u64,
            DropProbe {
                generation,
                dropped_generation_mask: dropped.clone(),
            },
        )
    }

    #[test]
    fn generation_remains_resident_until_its_submission_completes() {
        let dropped = Arc::new(AtomicUsize::new(0));
        let mut clock = FrameRetirementClock::new();
        let first_submission = clock.record_submission();
        let second_submission = clock.record_submission();
        clock.retire_after_last_submission(retirement(1, &dropped));

        clock.observe_completion(FrameCompletion::new(first_submission, 0));
        assert_eq!(dropped.load(Ordering::Relaxed), 0);

        clock.observe_completion(FrameCompletion::new(second_submission, 1));
        assert_eq!(dropped.load(Ordering::Relaxed), 1 << 1);
    }

    #[test]
    fn adjacent_growth_retires_each_generation_at_its_own_completion() {
        let dropped = Arc::new(AtomicUsize::new(0));
        let mut clock = FrameRetirementClock::new();

        let first_submission = clock.record_submission();
        clock.retire_after_last_submission(retirement(1, &dropped));
        let adjacent_submission = clock.record_submission();
        clock.retire_after_last_submission(retirement(2, &dropped));

        clock.observe_completion(FrameCompletion::new(first_submission, 0));
        assert_eq!(dropped.load(Ordering::Relaxed), 1 << 1);

        clock.observe_completion(FrameCompletion::new(adjacent_submission, 1));
        assert_eq!(
            dropped.load(Ordering::Relaxed),
            (1 << 1) | (1 << 2)
        );
    }

    #[test]
    fn generation_with_no_prior_submission_retires_immediately() {
        let dropped = Arc::new(AtomicUsize::new(0));
        let mut clock = FrameRetirementClock::new();

        clock.retire_after_last_submission(retirement(3, &dropped));

        assert_eq!(dropped.load(Ordering::Relaxed), 1 << 3);
    }
}
