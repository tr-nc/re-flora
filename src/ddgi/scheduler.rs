//! Resource-independent scheduling and identity for deterministic DDGI transport.
//!
//! Geometry and density requests may preempt lower-priority feedback. Radiance changes instead
//! coalesce while one immutable iteration finishes, so a moving sun never rewrites an in-flight
//! snapshot or makes the last complete field disappear.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DdgiFieldStage {
    SeedSky,
    SingleBounce,
    Feedback,
    Converged,
    NonConverged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DdgiFieldIdentityError {
    ZeroIdentityComponent,
    InvalidStageIteration,
    MissingSource,
    UnexpectedSource,
    SourceGeometryOrSpacingMismatch,
    SourceIterationMismatch,
    SourceSerialReuse,
}

/// Stable identity of one complete field. Iteration remains explicit even for terminal states,
/// avoiding the loss of the last real bounce number in `Converged` and `NonConverged`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DdgiFieldKey {
    serial: u64,
    geometry_revision: u32,
    radiance_revision: u32,
    spacing_voxels: u32,
    stage: DdgiFieldStage,
    iteration: u32,
}

impl DdgiFieldKey {
    pub fn new(
        serial: u64,
        geometry_revision: u32,
        radiance_revision: u32,
        spacing_voxels: u32,
        stage: DdgiFieldStage,
        iteration: u32,
    ) -> Result<Self, DdgiFieldIdentityError> {
        if serial == 0 || radiance_revision == 0 || spacing_voxels == 0 {
            return Err(DdgiFieldIdentityError::ZeroIdentityComponent);
        }
        let valid_iteration = match stage {
            DdgiFieldStage::SeedSky => iteration == 0,
            DdgiFieldStage::SingleBounce => iteration == 1,
            DdgiFieldStage::Feedback | DdgiFieldStage::Converged | DdgiFieldStage::NonConverged => {
                iteration >= 2
            }
        };
        if !valid_iteration {
            return Err(DdgiFieldIdentityError::InvalidStageIteration);
        }
        Ok(Self {
            serial,
            geometry_revision,
            radiance_revision,
            spacing_voxels,
            stage,
            iteration,
        })
    }

    pub fn serial(self) -> u64 {
        self.serial
    }

    pub fn geometry_revision(self) -> u32 {
        self.geometry_revision
    }

    pub fn radiance_revision(self) -> u32 {
        self.radiance_revision
    }

    pub fn spacing_voxels(self) -> u32 {
        self.spacing_voxels
    }

    pub fn stage(self) -> DdgiFieldStage {
        self.stage
    }

    pub fn iteration(self) -> u32 {
        self.iteration
    }

    fn with_stage(self, stage: DdgiFieldStage) -> Result<Self, DdgiFieldIdentityError> {
        Self::new(
            self.serial,
            self.geometry_revision,
            self.radiance_revision,
            self.spacing_voxels,
            stage,
            self.iteration,
        )
    }
}

/// A field plus the immutable complete field from which it was derived.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DdgiFieldIdentity {
    field: DdgiFieldKey,
    source: Option<DdgiFieldKey>,
}

impl DdgiFieldIdentity {
    pub fn new(
        field: DdgiFieldKey,
        source: Option<DdgiFieldKey>,
    ) -> Result<Self, DdgiFieldIdentityError> {
        match field.stage {
            DdgiFieldStage::SeedSky => {
                if source.is_some() {
                    return Err(DdgiFieldIdentityError::UnexpectedSource);
                }
            }
            DdgiFieldStage::SingleBounce => {
                let source = source.ok_or(DdgiFieldIdentityError::MissingSource)?;
                validate_source_pair(field, source)?;
                if source.stage != DdgiFieldStage::SeedSky || source.iteration != 0 {
                    return Err(DdgiFieldIdentityError::SourceIterationMismatch);
                }
            }
            DdgiFieldStage::Feedback | DdgiFieldStage::Converged | DdgiFieldStage::NonConverged => {
                let source = source.ok_or(DdgiFieldIdentityError::MissingSource)?;
                validate_source_pair(field, source)?;
                let expected_iteration = if source.radiance_revision == field.radiance_revision {
                    source.iteration.saturating_add(1)
                } else {
                    // A new radiance epoch starts with its first previous-field update, S2.
                    2
                };
                if field.iteration != expected_iteration {
                    return Err(DdgiFieldIdentityError::SourceIterationMismatch);
                }
            }
        }
        Ok(Self { field, source })
    }

    pub fn field(self) -> DdgiFieldKey {
        self.field
    }

    pub fn source(self) -> Option<DdgiFieldKey> {
        self.source
    }

    pub(crate) fn with_stage(self, stage: DdgiFieldStage) -> Result<Self, DdgiFieldIdentityError> {
        Self::new(self.field.with_stage(stage)?, self.source)
    }
}

fn validate_source_pair(
    field: DdgiFieldKey,
    source: DdgiFieldKey,
) -> Result<(), DdgiFieldIdentityError> {
    if field.serial == source.serial {
        return Err(DdgiFieldIdentityError::SourceSerialReuse);
    }
    if field.geometry_revision != source.geometry_revision
        || field.spacing_voxels != source.spacing_voxels
    {
        return Err(DdgiFieldIdentityError::SourceGeometryOrSpacingMismatch);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DdgiScheduledWorkKind {
    GeometryBootstrap,
    DensityBootstrap,
    RadianceFeedback,
    Feedback,
}

/// One immutable unit of scheduled work. Bootstrap owns both S0 and its S1 destination; feedback
/// owns only a destination because its source is the currently published complete field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DdgiScheduledWork {
    kind: DdgiScheduledWorkKind,
    seed: Option<DdgiFieldIdentity>,
    destination: DdgiFieldIdentity,
}

impl DdgiScheduledWork {
    pub fn kind(self) -> DdgiScheduledWorkKind {
        self.kind
    }

    pub fn seed(self) -> Option<DdgiFieldIdentity> {
        self.seed
    }

    pub fn destination(self) -> DdgiFieldIdentity {
        self.destination
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DdgiSchedulerError {
    Busy,
    NoInFlightWork,
    StaleCompletion,
    InvalidCompletion,
    InvalidIdentity(DdgiFieldIdentityError),
}

impl From<DdgiFieldIdentityError> for DdgiSchedulerError {
    fn from(value: DdgiFieldIdentityError) -> Self {
        Self::InvalidIdentity(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GeometryRequest {
    geometry_revision: u32,
    spacing_voxels: u32,
}

/// Pure DDGI work scheduler. GPU resources, descriptor ownership, and publication barriers remain
/// the volume's responsibility; this type only decides which immutable identity is allowed next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DdgiTransportScheduler {
    next_serial: u64,
    published: Option<DdgiFieldIdentity>,
    in_flight: Option<DdgiScheduledWork>,
    pending_geometry: Option<GeometryRequest>,
    pending_density_spacing_voxels: Option<u32>,
    latest_radiance_revision: Option<u32>,
    feedback_requested: bool,
}

impl DdgiTransportScheduler {
    pub fn new() -> Self {
        Self {
            next_serial: 0,
            published: None,
            in_flight: None,
            pending_geometry: None,
            pending_density_spacing_voxels: None,
            latest_radiance_revision: None,
            feedback_requested: false,
        }
    }

    pub fn install_published(
        &mut self,
        published: DdgiFieldIdentity,
    ) -> Result<(), DdgiSchedulerError> {
        if self.in_flight.is_some() {
            return Err(DdgiSchedulerError::Busy);
        }
        self.next_serial = self
            .next_serial
            .max(published.field.serial)
            .max(published.source.map_or(0, |source| source.serial));
        self.latest_radiance_revision = Some(published.field.radiance_revision);
        self.published = Some(published);
        self.feedback_requested = matches!(
            published.field.stage,
            DdgiFieldStage::SingleBounce | DdgiFieldStage::Feedback
        );
        Ok(())
    }

    pub fn published(self) -> Option<DdgiFieldIdentity> {
        self.published
    }

    pub fn in_flight(self) -> Option<DdgiScheduledWork> {
        self.in_flight
    }

    pub fn latest_radiance_revision(self) -> Option<u32> {
        self.latest_radiance_revision
    }

    /// Geometry is strict latest-wins. Any older work becomes immediately unpublishable.
    ///
    /// Density retries are owned by the physical-volume coordinator. Geometry therefore drops
    /// both pending and in-flight logical density work; the coordinator requests density again
    /// only after it has installed a matching-spacing staging volume.
    pub fn request_geometry(
        &mut self,
        geometry_revision: u32,
        active_spacing_voxels: u32,
    ) -> Option<DdgiScheduledWork> {
        assert_ne!(active_spacing_voxels, 0);
        let request = GeometryRequest {
            geometry_revision,
            spacing_voxels: active_spacing_voxels,
        };
        if self.pending_geometry == Some(request)
            || self.in_flight.is_some_and(|work| {
                work.kind == DdgiScheduledWorkKind::GeometryBootstrap
                    && work.destination.field.geometry_revision == geometry_revision
                    && work.destination.field.spacing_voxels == active_spacing_voxels
            })
        {
            return None;
        }

        self.pending_geometry = Some(request);
        self.pending_density_spacing_voxels = None;
        self.feedback_requested = false;
        self.in_flight.take()
    }

    /// Density is lower priority than geometry but preempts feedback. The old published spacing
    /// is deliberately untouched until this bootstrap is later completed and published.
    pub fn request_density(&mut self, spacing_voxels: u32) -> Option<DdgiScheduledWork> {
        assert_ne!(spacing_voxels, 0);
        if self.in_flight.is_some_and(|work| {
            work.kind == DdgiScheduledWorkKind::DensityBootstrap
                && work.destination.field.spacing_voxels == spacing_voxels
        }) {
            return None;
        }
        self.pending_density_spacing_voxels = Some(spacing_voxels);
        match self.in_flight {
            Some(work) if work.kind != DdgiScheduledWorkKind::GeometryBootstrap => {
                self.in_flight.take()
            }
            _ => None,
        }
    }

    /// Records only the latest radiance request. It never mutates or preempts the current work.
    pub fn observe_radiance(&mut self, radiance_revision: u32) {
        assert_ne!(radiance_revision, 0);
        if self.latest_radiance_revision == Some(radiance_revision) {
            return;
        }
        self.latest_radiance_revision = Some(radiance_revision);
    }

    pub fn request_feedback(&mut self) {
        self.feedback_requested = true;
    }

    /// Claims exactly one highest-priority immutable work item.
    pub fn claim_next(&mut self) -> Result<Option<DdgiScheduledWork>, DdgiSchedulerError> {
        if self.in_flight.is_some() {
            return Ok(None);
        }
        let Some(radiance_revision) = self.latest_radiance_revision else {
            return Ok(None);
        };

        let next = if let Some(request) = self.pending_geometry.take() {
            Some(self.make_bootstrap(
                DdgiScheduledWorkKind::GeometryBootstrap,
                request.geometry_revision,
                radiance_revision,
                request.spacing_voxels,
            )?)
        } else if let Some(spacing_voxels) = self.pending_density_spacing_voxels.take() {
            let Some(published) = self.published else {
                self.pending_density_spacing_voxels = Some(spacing_voxels);
                return Ok(None);
            };
            Some(self.make_bootstrap(
                DdgiScheduledWorkKind::DensityBootstrap,
                published.field.geometry_revision,
                radiance_revision,
                spacing_voxels,
            )?)
        } else {
            self.make_feedback(radiance_revision)?
        };

        self.in_flight = next;
        Ok(next)
    }

    /// Completes work only after the caller has validated and atomically published its resources.
    pub fn validate_in_flight_completion(
        &self,
        work: DdgiScheduledWork,
        published: DdgiFieldIdentity,
    ) -> Result<(), DdgiSchedulerError> {
        let current = self.in_flight.ok_or(DdgiSchedulerError::NoInFlightWork)?;
        if current != work {
            return Err(DdgiSchedulerError::StaleCompletion);
        }
        validate_completion(work, published)
    }

    /// Completes work only after the caller has validated and atomically published its resources.
    pub fn complete_in_flight(
        &mut self,
        work: DdgiScheduledWork,
        published: DdgiFieldIdentity,
    ) -> Result<DdgiFieldIdentity, DdgiSchedulerError> {
        self.validate_in_flight_completion(work, published)?;

        self.in_flight = None;
        self.published = Some(published);
        self.feedback_requested = matches!(
            published.field.stage,
            DdgiFieldStage::SingleBounce | DdgiFieldStage::Feedback
        );
        Ok(published)
    }

    fn make_bootstrap(
        &mut self,
        kind: DdgiScheduledWorkKind,
        geometry_revision: u32,
        radiance_revision: u32,
        spacing_voxels: u32,
    ) -> Result<DdgiScheduledWork, DdgiSchedulerError> {
        let seed_key = DdgiFieldKey::new(
            self.allocate_serial(),
            geometry_revision,
            radiance_revision,
            spacing_voxels,
            DdgiFieldStage::SeedSky,
            0,
        )?;
        let seed = DdgiFieldIdentity::new(seed_key, None)?;
        let destination_key = DdgiFieldKey::new(
            self.allocate_serial(),
            geometry_revision,
            radiance_revision,
            spacing_voxels,
            DdgiFieldStage::SingleBounce,
            1,
        )?;
        Ok(DdgiScheduledWork {
            kind,
            seed: Some(seed),
            destination: DdgiFieldIdentity::new(destination_key, Some(seed_key))?,
        })
    }

    fn make_feedback(
        &mut self,
        radiance_revision: u32,
    ) -> Result<Option<DdgiScheduledWork>, DdgiSchedulerError> {
        let Some(source) = self.published else {
            return Ok(None);
        };
        let radiance_changed = source.field.radiance_revision != radiance_revision;
        if !radiance_changed
            && (!self.feedback_requested
                || matches!(
                    source.field.stage,
                    DdgiFieldStage::Converged | DdgiFieldStage::NonConverged
                ))
        {
            return Ok(None);
        }
        let iteration = if radiance_changed {
            2
        } else {
            source.field.iteration.saturating_add(1)
        };
        let destination_key = DdgiFieldKey::new(
            self.allocate_serial(),
            source.field.geometry_revision,
            radiance_revision,
            source.field.spacing_voxels,
            DdgiFieldStage::Feedback,
            iteration,
        )?;
        self.feedback_requested = false;
        Ok(Some(DdgiScheduledWork {
            kind: if radiance_changed {
                DdgiScheduledWorkKind::RadianceFeedback
            } else {
                DdgiScheduledWorkKind::Feedback
            },
            seed: None,
            destination: DdgiFieldIdentity::new(destination_key, Some(source.field))?,
        }))
    }

    fn allocate_serial(&mut self) -> u64 {
        self.next_serial = self
            .next_serial
            .checked_add(1)
            .expect("DDGI transport identity serial exhausted");
        self.next_serial
    }
}

impl Default for DdgiTransportScheduler {
    fn default() -> Self {
        Self::new()
    }
}

fn validate_completion(
    work: DdgiScheduledWork,
    published: DdgiFieldIdentity,
) -> Result<(), DdgiSchedulerError> {
    let expected = work.destination;
    if published.source != expected.source {
        return Err(DdgiSchedulerError::InvalidCompletion);
    }
    let actual = published.field;
    let planned = expected.field;
    if actual.serial != planned.serial
        || actual.geometry_revision != planned.geometry_revision
        || actual.radiance_revision != planned.radiance_revision
        || actual.spacing_voxels != planned.spacing_voxels
        || actual.iteration != planned.iteration
    {
        return Err(DdgiSchedulerError::InvalidCompletion);
    }
    let valid_stage = match work.kind {
        DdgiScheduledWorkKind::GeometryBootstrap | DdgiScheduledWorkKind::DensityBootstrap => {
            actual.stage == DdgiFieldStage::SingleBounce
        }
        DdgiScheduledWorkKind::RadianceFeedback | DdgiScheduledWorkKind::Feedback => matches!(
            actual.stage,
            DdgiFieldStage::Feedback | DdgiFieldStage::Converged | DdgiFieldStage::NonConverged
        ),
    };
    if !valid_stage {
        return Err(DdgiSchedulerError::InvalidCompletion);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scheduler() -> DdgiTransportScheduler {
        DdgiTransportScheduler::new()
    }

    fn single_bounce(
        seed_serial: u64,
        serial: u64,
        geometry_revision: u32,
        radiance_revision: u32,
        spacing_voxels: u32,
    ) -> DdgiFieldIdentity {
        let seed = DdgiFieldKey::new(
            seed_serial,
            geometry_revision,
            radiance_revision,
            spacing_voxels,
            DdgiFieldStage::SeedSky,
            0,
        )
        .unwrap();
        DdgiFieldIdentity::new(
            DdgiFieldKey::new(
                serial,
                geometry_revision,
                radiance_revision,
                spacing_voxels,
                DdgiFieldStage::SingleBounce,
                1,
            )
            .unwrap(),
            Some(seed),
        )
        .unwrap()
    }

    fn with_active() -> DdgiTransportScheduler {
        let mut scheduler = scheduler();
        scheduler
            .install_published(single_bounce(1, 2, 7, 1, 32))
            .unwrap();
        scheduler
    }

    fn classified(work: DdgiScheduledWork, stage: DdgiFieldStage) -> DdgiFieldIdentity {
        work.destination().with_stage(stage).unwrap()
    }

    #[test]
    fn field_identity_preserves_terminal_iteration_and_exact_source() {
        let source = single_bounce(1, 2, 7, 3, 32).field();
        for stage in [
            DdgiFieldStage::Feedback,
            DdgiFieldStage::Converged,
            DdgiFieldStage::NonConverged,
        ] {
            let identity = DdgiFieldIdentity::new(
                DdgiFieldKey::new(3, 7, 3, 32, stage, 2).unwrap(),
                Some(source),
            )
            .unwrap();
            assert_eq!(identity.field().iteration(), 2);
            assert_eq!(identity.field().stage(), stage);
            assert_eq!(identity.source(), Some(source));
        }
    }

    #[test]
    fn initial_geometry_revision_zero_is_a_valid_field_identity() {
        let identity = single_bounce(1, 2, 0, 1, 32);
        assert_eq!(identity.field().geometry_revision(), 0);

        let mut scheduler = scheduler();
        scheduler.install_published(identity).unwrap();
        scheduler.request_geometry(0, 32);
        let bootstrap = scheduler.claim_next().unwrap().unwrap();
        assert_eq!(bootstrap.destination().field().geometry_revision(), 0);
    }

    #[test]
    fn geometry_then_density_preempt_lower_priority_feedback() {
        let mut scheduler = with_active();
        let feedback = scheduler.claim_next().unwrap().unwrap();
        assert_eq!(feedback.kind(), DdgiScheduledWorkKind::Feedback);

        let preempted = scheduler.request_density(16).unwrap();
        assert_eq!(preempted, feedback);
        let density = scheduler.claim_next().unwrap().unwrap();
        assert_eq!(density.kind(), DdgiScheduledWorkKind::DensityBootstrap);
        assert_eq!(density.destination().field().spacing_voxels(), 16);

        let preempted = scheduler.request_geometry(8, 32).unwrap();
        assert_eq!(preempted, density);
        let geometry = scheduler.claim_next().unwrap().unwrap();
        assert_eq!(geometry.kind(), DdgiScheduledWorkKind::GeometryBootstrap);
        assert_eq!(geometry.destination().field().geometry_revision(), 8);

        let geometry = scheduler
            .complete_in_flight(geometry, geometry.destination())
            .unwrap();
        assert_eq!(geometry.field().stage(), DdgiFieldStage::SingleBounce);
        let feedback = scheduler.claim_next().unwrap().unwrap();
        assert_eq!(feedback.kind(), DdgiScheduledWorkKind::Feedback);
        assert_eq!(feedback.destination().field().geometry_revision(), 8);
        assert_eq!(feedback.destination().field().spacing_voxels(), 32);

        assert_eq!(scheduler.request_density(16), Some(feedback));
        let retried_density = scheduler.claim_next().unwrap().unwrap();
        assert_eq!(
            retried_density.kind(),
            DdgiScheduledWorkKind::DensityBootstrap
        );
        assert_eq!(retried_density.destination().field().geometry_revision(), 8);
        assert_eq!(retried_density.destination().field().spacing_voxels(), 16);
    }

    #[test]
    fn pending_density_requires_an_explicit_retry_after_geometry() {
        let mut scheduler = with_active();
        let feedback = scheduler.claim_next().unwrap().unwrap();
        assert_eq!(scheduler.request_density(16), Some(feedback));

        assert_eq!(scheduler.request_geometry(8, 32), None);
        let geometry = scheduler.claim_next().unwrap().unwrap();
        assert_eq!(geometry.kind(), DdgiScheduledWorkKind::GeometryBootstrap);
        scheduler
            .complete_in_flight(geometry, geometry.destination())
            .unwrap();

        let next = scheduler.claim_next().unwrap().unwrap();
        assert_eq!(next.kind(), DdgiScheduledWorkKind::Feedback);
        assert_eq!(next.destination().field().spacing_voxels(), 32);
    }

    #[test]
    fn radiance_requests_coalesce_without_mutating_the_inflight_identity() {
        let mut scheduler = with_active();
        scheduler.observe_radiance(2);
        let revision_two = scheduler.claim_next().unwrap().unwrap();
        assert_eq!(revision_two.kind(), DdgiScheduledWorkKind::RadianceFeedback);
        assert_eq!(revision_two.destination().field().radiance_revision(), 2);

        scheduler.observe_radiance(3);
        scheduler.observe_radiance(4);
        assert_eq!(scheduler.in_flight(), Some(revision_two));
        assert_eq!(scheduler.latest_radiance_revision(), Some(4));

        let published_two = scheduler
            .complete_in_flight(revision_two, revision_two.destination())
            .unwrap();
        assert_eq!(published_two.field().radiance_revision(), 2);
        let revision_four = scheduler.claim_next().unwrap().unwrap();
        assert_eq!(
            revision_four.kind(),
            DdgiScheduledWorkKind::RadianceFeedback
        );
        assert_eq!(revision_four.destination().field().radiance_revision(), 4);
        assert_eq!(revision_four.destination().field().iteration(), 2);
        assert_eq!(
            revision_four.destination().source(),
            Some(published_two.field())
        );
    }

    #[test]
    fn geometry_bootstrap_finishes_its_snapshot_then_schedules_latest_radiance() {
        let mut scheduler = with_active();
        scheduler.observe_radiance(2);
        scheduler.request_geometry(8, 32);
        let geometry = scheduler.claim_next().unwrap().unwrap();
        assert_eq!(geometry.destination().field().radiance_revision(), 2);

        scheduler.observe_radiance(3);
        assert_eq!(scheduler.in_flight(), Some(geometry));
        let geometry = scheduler
            .complete_in_flight(geometry, geometry.destination())
            .unwrap();
        assert_eq!(geometry.field().geometry_revision(), 8);
        assert_eq!(geometry.field().radiance_revision(), 2);

        let latest = scheduler.claim_next().unwrap().unwrap();
        assert_eq!(latest.kind(), DdgiScheduledWorkKind::RadianceFeedback);
        assert_eq!(latest.destination().field().geometry_revision(), 8);
        assert_eq!(latest.destination().field().radiance_revision(), 3);
    }

    #[test]
    fn density_preemption_keeps_the_old_published_field_visible() {
        let mut scheduler = with_active();
        let old = scheduler.published().unwrap();
        let feedback = scheduler.claim_next().unwrap().unwrap();

        assert_eq!(scheduler.request_density(16), Some(feedback));
        assert_eq!(scheduler.published(), Some(old));
        assert_eq!(
            scheduler.validate_in_flight_completion(feedback, feedback.destination()),
            Err(DdgiSchedulerError::NoInFlightWork),
        );
        assert_eq!(
            scheduler.complete_in_flight(feedback, feedback.destination()),
            Err(DdgiSchedulerError::NoInFlightWork),
        );

        let density = scheduler.claim_next().unwrap().unwrap();
        assert_eq!(density.kind(), DdgiScheduledWorkKind::DensityBootstrap);
        assert_eq!(scheduler.published(), Some(old));
    }

    #[test]
    fn resource_classification_preserves_the_real_terminal_iteration() {
        let mut scheduler = with_active();
        let s2 = scheduler.claim_next().unwrap().unwrap();
        let s2 = scheduler
            .complete_in_flight(s2, classified(s2, DdgiFieldStage::Feedback))
            .unwrap();
        assert_eq!(s2.field().stage(), DdgiFieldStage::Feedback);
        let s3 = scheduler.claim_next().unwrap().unwrap();
        let converged = scheduler
            .complete_in_flight(s3, classified(s3, DdgiFieldStage::Converged))
            .unwrap();
        assert_eq!(converged.field().stage(), DdgiFieldStage::Converged);
        assert_eq!(converged.field().iteration(), 3);
        assert_eq!(scheduler.claim_next().unwrap(), None);
    }

    #[test]
    fn a_new_radiance_epoch_starts_at_s2_from_a_terminal_field() {
        let mut scheduler = with_active();
        let s2 = scheduler.claim_next().unwrap().unwrap();
        let s2 = scheduler
            .complete_in_flight(s2, classified(s2, DdgiFieldStage::Converged))
            .unwrap();
        scheduler.observe_radiance(2);
        let restarted = scheduler.claim_next().unwrap().unwrap();
        assert_eq!(restarted.destination().field().iteration(), 2);
        assert_eq!(restarted.destination().source(), Some(s2.field()));
    }

    #[test]
    fn completion_rejects_a_resource_result_with_the_wrong_identity() {
        let mut scheduler = with_active();
        let work = scheduler.claim_next().unwrap().unwrap();
        let expected = work.destination();
        let wrong_serial = DdgiFieldIdentity::new(
            DdgiFieldKey::new(
                expected.field().serial() + 1,
                expected.field().geometry_revision(),
                expected.field().radiance_revision(),
                expected.field().spacing_voxels(),
                DdgiFieldStage::Feedback,
                expected.field().iteration(),
            )
            .unwrap(),
            expected.source(),
        )
        .unwrap();

        assert_eq!(
            scheduler.complete_in_flight(work, wrong_serial),
            Err(DdgiSchedulerError::InvalidCompletion)
        );
        assert_eq!(scheduler.in_flight(), Some(work));
    }
}
