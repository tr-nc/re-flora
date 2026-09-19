//! Latch runtime A/B controls at complete-field boundaries. A capture proof and all batches of
//! one field must describe one history policy, even if the saved GUI controls change mid-sweep.
use super::DdgiFieldIdentity;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DdgiExperimentSettings {
    pub continuous_sampling: bool,
    pub aggregate_history: bool,
}

#[derive(Default)]
pub(crate) struct DdgiExperimentLatch {
    field: Option<(DdgiFieldIdentity, DdgiExperimentSettings)>,
}

impl DdgiExperimentLatch {
    pub fn for_field(
        &mut self,
        field: Option<DdgiFieldIdentity>,
        requested: DdgiExperimentSettings,
    ) -> DdgiExperimentSettings {
        let Some(field) = field else { return requested };
        if self.field.is_none_or(|(previous, _)| previous != field) {
            log::info!(
                "[DDGI][EXPERIMENT] field={field:?} continuous_sampling={} aggregate_history={}",
                requested.continuous_sampling,
                requested.aggregate_history
            );
            self.field = Some((field, requested));
        }
        self.field.expect("latched field").1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ddgi::{DdgiProbeSpacing, DdgiTransportScheduler};

    #[test]
    fn live_modes_change_at_the_next_field_not_mid_proof_or_retry() {
        let mut scheduler = DdgiTransportScheduler::new();
        scheduler.request_geometry(1, DdgiProbeSpacing::try_from(32).unwrap());
        // The scheduler needs authored radiance before it can issue a field.
        scheduler.observe_radiance(1);
        let first = scheduler.claim_next().unwrap().unwrap().destination();
        let mut latch = DdgiExperimentLatch::default();
        let original = DdgiExperimentSettings::default();
        let candidate = DdgiExperimentSettings {
            continuous_sampling: true,
            aggregate_history: true,
        };
        assert_eq!(latch.for_field(Some(first), original), original);
        assert_eq!(latch.for_field(Some(first), candidate), original);
        assert_eq!(latch.for_field(None, candidate), candidate);
        assert_eq!(latch.for_field(Some(first), candidate), original);
        let mut next_scheduler = DdgiTransportScheduler::new();
        next_scheduler.request_geometry(2, DdgiProbeSpacing::try_from(32).unwrap());
        next_scheduler.observe_radiance(1);
        let next = next_scheduler.claim_next().unwrap().unwrap().destination();
        assert_eq!(latch.for_field(Some(next), candidate), candidate);
        assert_eq!(latch.for_field(Some(next), original), candidate);
        assert_eq!(latch.for_field(Some(first), original), original);
    }
}
