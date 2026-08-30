//! Resource-independent scheduling and identity for temporal DDGI transport.
//!
//! Geometry and density requests may preempt lower-priority convergence work. Radiance changes
//! instead coalesce while one immutable update epoch finishes, so a moving sun never rewrites an
//! in-flight snapshot or makes the last complete field disappear.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DdgiFieldState {
    Converging,
    Converged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DdgiFieldIdentityError {
    ZeroIdentityComponent,
    InvalidStateEpoch,
    UnexpectedSource,
    SourceSpacingMismatch,
    SourceEpochMismatch,
    SourceSerialReuse,
}

/// Stable identity of one complete current-revision field.
///
/// `update_epoch` is a temporal sample epoch, not a light-transport bounce or display frame. A new
/// geometry, density, or radiance revision starts at epoch zero. Each later epoch consumes the
/// previous complete field for the same revision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DdgiFieldKey {
    serial: u64,
    geometry_revision: u32,
    radiance_revision: u32,
    spacing_voxels: u32,
    state: DdgiFieldState,
    update_epoch: u32,
}

impl DdgiFieldKey {
    pub fn new(
        serial: u64,
        geometry_revision: u32,
        radiance_revision: u32,
        spacing_voxels: u32,
        state: DdgiFieldState,
        update_epoch: u32,
    ) -> Result<Self, DdgiFieldIdentityError> {
        if serial == 0 || radiance_revision == 0 || spacing_voxels == 0 {
            return Err(DdgiFieldIdentityError::ZeroIdentityComponent);
        }
        if state == DdgiFieldState::Converged && update_epoch == 0 {
            return Err(DdgiFieldIdentityError::InvalidStateEpoch);
        }
        Ok(Self {
            serial,
            geometry_revision,
            radiance_revision,
            spacing_voxels,
            state,
            update_epoch,
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

    pub fn state(self) -> DdgiFieldState {
        self.state
    }

    pub fn update_epoch(self) -> u32 {
        self.update_epoch
    }

    fn with_state(self, state: DdgiFieldState) -> Result<Self, DdgiFieldIdentityError> {
        Self::new(
            self.serial,
            self.geometry_revision,
            self.radiance_revision,
            self.spacing_voxels,
            state,
            self.update_epoch,
        )
    }
}

/// A complete field plus the immutable complete field from which its epoch was derived.
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
        match source {
            None => {
                if field.update_epoch != 0 || field.state != DdgiFieldState::Converging {
                    return Err(DdgiFieldIdentityError::UnexpectedSource);
                }
            }
            Some(source) => {
                validate_source_pair(field, source)?;
                let same_transport_revision = source.geometry_revision == field.geometry_revision
                    && source.radiance_revision == field.radiance_revision;
                let expected_epoch = if same_transport_revision {
                    source.update_epoch.saturating_add(1)
                } else {
                    0
                };
                if field.update_epoch != expected_epoch {
                    return Err(DdgiFieldIdentityError::SourceEpochMismatch);
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

    pub(crate) fn with_state(self, state: DdgiFieldState) -> Result<Self, DdgiFieldIdentityError> {
        Self::new(self.field.with_state(state)?, self.source)
    }
}

fn validate_source_pair(
    field: DdgiFieldKey,
    source: DdgiFieldKey,
) -> Result<(), DdgiFieldIdentityError> {
    if field.serial == source.serial {
        return Err(DdgiFieldIdentityError::SourceSerialReuse);
    }
    if field.spacing_voxels != source.spacing_voxels {
        return Err(DdgiFieldIdentityError::SourceSpacingMismatch);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
pub enum DdgiScheduledWorkKind {
    GeometryUpdate,
    DensityUpdate,
    RadianceUpdate,
    ConvergenceUpdate,
}

/// One immutable full-volume update epoch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DdgiScheduledWork {
    kind: DdgiScheduledWorkKind,
    transport_source: Option<DdgiFieldIdentity>,
    destination: DdgiFieldIdentity,
}

impl DdgiScheduledWork {
    pub fn kind(self) -> DdgiScheduledWorkKind {
        self.kind
    }

    pub fn destination(self) -> DdgiFieldIdentity {
        self.destination
    }

    pub fn transport_source(self) -> Option<DdgiFieldIdentity> {
        self.transport_source
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

/// Linear proof that the scheduler accepted one exact in-flight completion.
pub(crate) struct DdgiSchedulerCompletionPermit {
    work: DdgiScheduledWork,
    published: DdgiFieldIdentity,
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
    transport_source: Option<DdgiFieldIdentity>,
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
    convergence_requested: bool,
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
            convergence_requested: false,
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
        self.convergence_requested = published.field.state == DdgiFieldState::Converging;
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
        let transport_source = self
            .published
            .filter(|source| source.field.spacing_voxels == active_spacing_voxels);
        self.request_geometry_from(geometry_revision, active_spacing_voxels, transport_source)
    }

    pub fn request_geometry_from(
        &mut self,
        geometry_revision: u32,
        active_spacing_voxels: u32,
        transport_source: Option<DdgiFieldIdentity>,
    ) -> Option<DdgiScheduledWork> {
        assert_ne!(active_spacing_voxels, 0);
        assert!(transport_source
            .is_none_or(|source| { source.field.spacing_voxels == active_spacing_voxels }));
        let request = GeometryRequest {
            geometry_revision,
            spacing_voxels: active_spacing_voxels,
            transport_source,
        };
        if self.pending_geometry == Some(request)
            || self.in_flight.is_some_and(|work| {
                work.kind == DdgiScheduledWorkKind::GeometryUpdate
                    && work.destination.field.geometry_revision == geometry_revision
                    && work.destination.field.spacing_voxels == active_spacing_voxels
            })
        {
            return None;
        }

        self.pending_geometry = Some(request);
        self.pending_density_spacing_voxels = None;
        self.convergence_requested = false;
        self.in_flight.take()
    }

    /// Density is lower priority than geometry but preempts convergence. The old published spacing
    /// remains untouched until the new volume completes epoch zero and is published.
    pub fn request_density(&mut self, spacing_voxels: u32) -> Option<DdgiScheduledWork> {
        assert_ne!(spacing_voxels, 0);
        if self.in_flight.is_some_and(|work| {
            work.kind == DdgiScheduledWorkKind::DensityUpdate
                && work.destination.field.spacing_voxels == spacing_voxels
        }) {
            return None;
        }
        self.pending_density_spacing_voxels = Some(spacing_voxels);
        match self.in_flight {
            Some(work) if work.kind != DdgiScheduledWorkKind::GeometryUpdate => {
                self.in_flight.take()
            }
            _ => None,
        }
    }

    /// Records only the latest radiance request. It never mutates or preempts the current epoch.
    pub fn observe_radiance(&mut self, radiance_revision: u32) {
        assert_ne!(radiance_revision, 0);
        if self.latest_radiance_revision != Some(radiance_revision) {
            self.latest_radiance_revision = Some(radiance_revision);
        }
    }

    pub fn request_convergence(&mut self) {
        self.convergence_requested = true;
    }

    /// Claims exactly one highest-priority immutable full-volume update epoch.
    pub fn claim_next(&mut self) -> Result<Option<DdgiScheduledWork>, DdgiSchedulerError> {
        if self.in_flight.is_some() {
            return Ok(None);
        }
        let Some(radiance_revision) = self.latest_radiance_revision else {
            return Ok(None);
        };

        let next = if let Some(request) = self.pending_geometry.take() {
            Some(self.make_geometry_update(request, radiance_revision)?)
        } else if let Some(spacing_voxels) = self.pending_density_spacing_voxels.take() {
            let Some(published) = self.published else {
                self.pending_density_spacing_voxels = Some(spacing_voxels);
                return Ok(None);
            };
            Some(self.make_initial_update(
                DdgiScheduledWorkKind::DensityUpdate,
                published.field.geometry_revision,
                radiance_revision,
                spacing_voxels,
            )?)
        } else {
            self.make_temporal_update(radiance_revision)?
        };

        self.in_flight = next;
        Ok(next)
    }

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

    pub(crate) fn preflight_completion(
        &self,
        work: DdgiScheduledWork,
        published: DdgiFieldIdentity,
    ) -> Result<DdgiSchedulerCompletionPermit, DdgiSchedulerError> {
        self.validate_in_flight_completion(work, published)?;
        Ok(DdgiSchedulerCompletionPermit { work, published })
    }

    pub(crate) fn commit_completion(
        &mut self,
        permit: DdgiSchedulerCompletionPermit,
    ) -> DdgiFieldIdentity {
        assert_eq!(self.in_flight, Some(permit.work));
        self.in_flight = None;
        self.published = Some(permit.published);
        self.convergence_requested = permit.published.field.state == DdgiFieldState::Converging;
        permit.published
    }

    pub fn complete_in_flight(
        &mut self,
        work: DdgiScheduledWork,
        published: DdgiFieldIdentity,
    ) -> Result<DdgiFieldIdentity, DdgiSchedulerError> {
        let permit = self.preflight_completion(work, published)?;
        Ok(self.commit_completion(permit))
    }

    fn make_initial_update(
        &mut self,
        kind: DdgiScheduledWorkKind,
        geometry_revision: u32,
        radiance_revision: u32,
        spacing_voxels: u32,
    ) -> Result<DdgiScheduledWork, DdgiSchedulerError> {
        let destination_key = DdgiFieldKey::new(
            self.allocate_serial(),
            geometry_revision,
            radiance_revision,
            spacing_voxels,
            DdgiFieldState::Converging,
            0,
        )?;
        Ok(DdgiScheduledWork {
            kind,
            transport_source: None,
            destination: DdgiFieldIdentity::new(destination_key, None)?,
        })
    }

    fn make_geometry_update(
        &mut self,
        request: GeometryRequest,
        radiance_revision: u32,
    ) -> Result<DdgiScheduledWork, DdgiSchedulerError> {
        let transport_source = request.transport_source;
        let destination_key = DdgiFieldKey::new(
            self.allocate_serial(),
            request.geometry_revision,
            radiance_revision,
            request.spacing_voxels,
            DdgiFieldState::Converging,
            0,
        )?;
        Ok(DdgiScheduledWork {
            kind: DdgiScheduledWorkKind::GeometryUpdate,
            transport_source,
            destination: DdgiFieldIdentity::new(
                destination_key,
                transport_source.map(|source| source.field),
            )?,
        })
    }

    fn make_temporal_update(
        &mut self,
        radiance_revision: u32,
    ) -> Result<Option<DdgiScheduledWork>, DdgiSchedulerError> {
        let Some(source) = self.published else {
            return Ok(None);
        };
        let radiance_changed = source.field.radiance_revision != radiance_revision;
        if !radiance_changed
            && (!self.convergence_requested || source.field.state == DdgiFieldState::Converged)
        {
            return Ok(None);
        }
        let update_epoch = if radiance_changed {
            0
        } else {
            source.field.update_epoch.saturating_add(1)
        };
        let destination_key = DdgiFieldKey::new(
            self.allocate_serial(),
            source.field.geometry_revision,
            radiance_revision,
            source.field.spacing_voxels,
            DdgiFieldState::Converging,
            update_epoch,
        )?;
        self.convergence_requested = false;
        Ok(Some(DdgiScheduledWork {
            kind: if radiance_changed {
                DdgiScheduledWorkKind::RadianceUpdate
            } else {
                DdgiScheduledWorkKind::ConvergenceUpdate
            },
            transport_source: Some(source),
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
        || actual.update_epoch != planned.update_epoch
    {
        return Err(DdgiSchedulerError::InvalidCompletion);
    }
    let valid_state = match work.kind {
        DdgiScheduledWorkKind::GeometryUpdate | DdgiScheduledWorkKind::DensityUpdate => {
            actual.state == DdgiFieldState::Converging
        }
        DdgiScheduledWorkKind::RadianceUpdate | DdgiScheduledWorkKind::ConvergenceUpdate => {
            matches!(
                actual.state,
                DdgiFieldState::Converging | DdgiFieldState::Converged
            )
        }
    };
    if !valid_state {
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

    fn field(
        serial: u64,
        geometry_revision: u32,
        radiance_revision: u32,
        spacing_voxels: u32,
        update_epoch: u32,
        state: DdgiFieldState,
        source: Option<DdgiFieldKey>,
    ) -> DdgiFieldIdentity {
        DdgiFieldIdentity::new(
            DdgiFieldKey::new(
                serial,
                geometry_revision,
                radiance_revision,
                spacing_voxels,
                state,
                update_epoch,
            )
            .unwrap(),
            source,
        )
        .unwrap()
    }

    fn initial(
        serial: u64,
        geometry_revision: u32,
        radiance_revision: u32,
        spacing_voxels: u32,
    ) -> DdgiFieldIdentity {
        field(
            serial,
            geometry_revision,
            radiance_revision,
            spacing_voxels,
            0,
            DdgiFieldState::Converging,
            None,
        )
    }

    fn with_active() -> DdgiTransportScheduler {
        let mut scheduler = scheduler();
        scheduler.install_published(initial(1, 7, 1, 32)).unwrap();
        scheduler
    }

    fn classified(work: DdgiScheduledWork, state: DdgiFieldState) -> DdgiFieldIdentity {
        work.destination().with_state(state).unwrap()
    }

    #[test]
    fn field_identity_tracks_epoch_state_and_exact_source() {
        let source = initial(1, 7, 3, 32).field();
        for state in [DdgiFieldState::Converging, DdgiFieldState::Converged] {
            let identity = field(2, 7, 3, 32, 1, state, Some(source));
            assert_eq!(identity.field().update_epoch(), 1);
            assert_eq!(identity.field().state(), state);
            assert_eq!(identity.source(), Some(source));
        }
    }

    #[test]
    fn initial_geometry_revision_zero_is_valid_and_publishes_after_one_epoch() {
        let identity = initial(1, 0, 1, 32);
        assert_eq!(identity.field().geometry_revision(), 0);

        let mut scheduler = scheduler();
        scheduler.observe_radiance(1);
        scheduler.request_geometry(0, 32);
        let update = scheduler.claim_next().unwrap().unwrap();
        assert_eq!(update.kind(), DdgiScheduledWorkKind::GeometryUpdate);
        assert_eq!(update.destination().field().update_epoch(), 0);
        assert_eq!(update.destination().source(), None);
        scheduler
            .complete_in_flight(update, update.destination())
            .unwrap();
        assert_eq!(scheduler.published(), Some(update.destination()));
    }

    #[test]
    fn geometry_then_density_preempt_lower_priority_convergence() {
        let mut scheduler = with_active();
        let convergence = scheduler.claim_next().unwrap().unwrap();
        assert_eq!(convergence.kind(), DdgiScheduledWorkKind::ConvergenceUpdate);

        assert_eq!(scheduler.request_density(16), Some(convergence));
        let density = scheduler.claim_next().unwrap().unwrap();
        assert_eq!(density.kind(), DdgiScheduledWorkKind::DensityUpdate);
        assert_eq!(density.destination().field().spacing_voxels(), 16);

        assert_eq!(scheduler.request_geometry(8, 32), Some(density));
        let geometry = scheduler.claim_next().unwrap().unwrap();
        assert_eq!(geometry.kind(), DdgiScheduledWorkKind::GeometryUpdate);
        let geometry = scheduler
            .complete_in_flight(geometry, geometry.destination())
            .unwrap();
        assert_eq!(geometry.field().state(), DdgiFieldState::Converging);

        let convergence = scheduler.claim_next().unwrap().unwrap();
        assert_eq!(convergence.kind(), DdgiScheduledWorkKind::ConvergenceUpdate);
        assert_eq!(convergence.destination().field().geometry_revision(), 8);
        assert_eq!(convergence.destination().field().update_epoch(), 1);
    }

    #[test]
    fn geometry_update_carries_the_exact_published_transport_source() {
        let mut scheduler = with_active();
        let resident = scheduler.published().unwrap();
        scheduler.request_geometry(8, 32);
        let geometry = scheduler.claim_next().unwrap().unwrap();

        assert_eq!(geometry.kind(), DdgiScheduledWorkKind::GeometryUpdate);
        assert_eq!(geometry.transport_source(), Some(resident));
        assert_eq!(geometry.destination().source(), Some(resident.field()));
        assert_eq!(geometry.destination().field().update_epoch(), 0);
    }

    #[test]
    fn pending_density_requires_an_explicit_retry_after_geometry() {
        let mut scheduler = with_active();
        let convergence = scheduler.claim_next().unwrap().unwrap();
        assert_eq!(scheduler.request_density(16), Some(convergence));

        assert_eq!(scheduler.request_geometry(8, 32), None);
        let geometry = scheduler.claim_next().unwrap().unwrap();
        scheduler
            .complete_in_flight(geometry, geometry.destination())
            .unwrap();

        let next = scheduler.claim_next().unwrap().unwrap();
        assert_eq!(next.kind(), DdgiScheduledWorkKind::ConvergenceUpdate);
        assert_eq!(next.destination().field().spacing_voxels(), 32);
    }

    #[test]
    fn radiance_requests_coalesce_without_mutating_the_inflight_identity() {
        let mut scheduler = with_active();
        scheduler.observe_radiance(2);
        let revision_two = scheduler.claim_next().unwrap().unwrap();
        assert_eq!(revision_two.kind(), DdgiScheduledWorkKind::RadianceUpdate);
        assert_eq!(revision_two.destination().field().update_epoch(), 0);

        scheduler.observe_radiance(3);
        scheduler.observe_radiance(4);
        assert_eq!(scheduler.in_flight(), Some(revision_two));

        let published_two = scheduler
            .complete_in_flight(revision_two, revision_two.destination())
            .unwrap();
        let revision_four = scheduler.claim_next().unwrap().unwrap();
        assert_eq!(revision_four.kind(), DdgiScheduledWorkKind::RadianceUpdate);
        assert_eq!(revision_four.destination().field().radiance_revision(), 4);
        assert_eq!(revision_four.destination().field().update_epoch(), 0);
        assert_eq!(
            revision_four.destination().source(),
            Some(published_two.field())
        );
    }

    #[test]
    fn geometry_update_finishes_its_snapshot_then_schedules_latest_radiance() {
        let mut scheduler = with_active();
        scheduler.observe_radiance(2);
        scheduler.request_geometry(8, 32);
        let geometry = scheduler.claim_next().unwrap().unwrap();
        scheduler.observe_radiance(3);
        let geometry = scheduler
            .complete_in_flight(geometry, geometry.destination())
            .unwrap();
        assert_eq!(geometry.field().radiance_revision(), 2);

        let latest = scheduler.claim_next().unwrap().unwrap();
        assert_eq!(latest.kind(), DdgiScheduledWorkKind::RadianceUpdate);
        assert_eq!(latest.destination().field().radiance_revision(), 3);
    }

    #[test]
    fn density_preemption_keeps_the_old_published_field_visible() {
        let mut scheduler = with_active();
        let old = scheduler.published().unwrap();
        let convergence = scheduler.claim_next().unwrap().unwrap();

        assert_eq!(scheduler.request_density(16), Some(convergence));
        assert_eq!(scheduler.published(), Some(old));
        assert_eq!(
            scheduler.validate_in_flight_completion(convergence, convergence.destination()),
            Err(DdgiSchedulerError::NoInFlightWork),
        );
    }

    #[test]
    fn convergence_sleeps_until_a_tracked_change_wakes_it() {
        let mut scheduler = with_active();
        let epoch_one = scheduler.claim_next().unwrap().unwrap();
        let converged = scheduler
            .complete_in_flight(epoch_one, classified(epoch_one, DdgiFieldState::Converged))
            .unwrap();
        assert_eq!(converged.field().update_epoch(), 1);
        assert_eq!(scheduler.claim_next().unwrap(), None);

        scheduler.observe_radiance(2);
        let restarted = scheduler.claim_next().unwrap().unwrap();
        assert_eq!(restarted.kind(), DdgiScheduledWorkKind::RadianceUpdate);
        assert_eq!(restarted.destination().field().update_epoch(), 0);
        assert_eq!(restarted.destination().source(), Some(converged.field()));
    }

    #[test]
    fn completion_rejects_a_resource_result_with_the_wrong_identity() {
        let mut scheduler = with_active();
        let work = scheduler.claim_next().unwrap().unwrap();
        let expected = work.destination();
        let wrong_serial = field(
            expected.field().serial() + 1,
            expected.field().geometry_revision(),
            expected.field().radiance_revision(),
            expected.field().spacing_voxels(),
            expected.field().update_epoch(),
            DdgiFieldState::Converging,
            expected.source(),
        );

        assert_eq!(
            scheduler.complete_in_flight(work, wrong_serial),
            Err(DdgiSchedulerError::InvalidCompletion)
        );
        assert_eq!(scheduler.in_flight(), Some(work));
    }
}
