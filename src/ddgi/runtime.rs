use crate::environment_lighting::{DdgiRadianceHistoryPolicy, EnvironmentLightingState};
use crate::geom::UAabb3;
use anyhow::Result;

use super::resources::{DdgiStatus, DdgiVolume, DdgiVolumeStatus, DdgiVolumes};
use super::{
    DdgiAtlasValidationStats, DdgiBatchOrder, DdgiBuildKind, DdgiBuildToken, DdgiCaptureCheckpoint,
    DdgiCapturePublication, DdgiCaptureTarget, DdgiFieldIdentity, DdgiProbePriority,
    DdgiProbePriorityReason, DdgiRayBatch, DdgiRefreshState, DdgiResourceBytes, DdgiScheduledWork,
    DdgiScheduledWorkKind, DdgiSchedulerError, DdgiTerrainRefresh, DdgiTransportScheduler,
    DdgiVolumeGrid, DdgiVolumeStage,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DdgiRuntimeVolumeTarget {
    Active,
    Staging,
}

/// One runtime-authorized physical Volume allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DdgiRuntimeVolumeBuild {
    target: DdgiRuntimeVolumeTarget,
    token: DdgiBuildToken,
}

impl DdgiRuntimeVolumeBuild {
    pub(crate) fn target(self) -> DdgiRuntimeVolumeTarget {
        self.target
    }

    pub(crate) fn token(self) -> DdgiBuildToken {
        self.token
    }
}

/// One immutable transport decision paired with the authored lighting that produced its revision.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DdgiRuntimeWork {
    scheduled: DdgiScheduledWork,
    authored_lighting: EnvironmentLightingState,
    radiance_history_policy: Option<DdgiRadianceHistoryPolicy>,
    local_refresh_voxel_bound: Option<UAabb3>,
    probe_priority: Option<DdgiProbePriority>,
}

/// Logical DDGI work selected for one frame. The runtime owns sequencing decisions; the tracer
/// only records the Vulkan passes described by this plan and reports completion back here.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DdgiFramePlan {
    pub global_sky_needs_update: bool,
    pub relocation_terrain_revision: Option<u32>,
    pub visibility_preservation_needed: bool,
    pub ray_batch: Option<DdgiRayBatch>,
    pub iteration_will_complete: bool,
}

/// Resource-independent lighting state exposed for deterministic diagnostics and acceptance
/// harnesses. Physical atlas status remains separately available through `DdgiRuntimeStatus`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DdgiLightingDiagnostics {
    pub latest_transport_revision: Option<u32>,
    pub latest_source_live_revision: Option<u64>,
    pub scheduler_published_revision: Option<u32>,
    pub in_flight_revision: Option<u32>,
    pub coalesced_revisions: u64,
    pub has_mixed_in_flight_revision: bool,
}

impl DdgiLightingDiagnostics {
    pub fn scheduler_revision_lag(self) -> u32 {
        match (
            self.latest_transport_revision,
            self.scheduler_published_revision,
        ) {
            (Some(latest), Some(published)) => latest.saturating_sub(published),
            (Some(latest), None) => latest,
            _ => 0,
        }
    }
}

impl DdgiRuntimeWork {
    pub(crate) fn scheduled(self) -> DdgiScheduledWork {
        self.scheduled
    }

    pub(crate) fn authored_lighting(self) -> EnvironmentLightingState {
        self.authored_lighting
    }

    pub(crate) fn radiance_history_policy(self) -> Option<DdgiRadianceHistoryPolicy> {
        self.radiance_history_policy
    }

    pub(crate) fn local_refresh_voxel_bound(self) -> Option<UAabb3> {
        self.local_refresh_voxel_bound
    }

    pub(crate) fn probe_priority(self) -> Option<DdgiProbePriority> {
        self.probe_priority
    }
}

/// Owns DDGI terrain and radiance event handling independently from physical Vulkan execution.
///
/// Terrain reports only an already-published geometry revision and its edited bound. Authored
/// Environment Lighting remains the source of normalized live snapshots and stable revisions;
/// this runtime retains the exact snapshot chosen for in-flight work so later observations cannot
/// mutate it underneath the GPU.
pub(crate) struct DdgiRuntime {
    volumes: Option<DdgiVolumes>,
    active_grid: DdgiVolumeGrid,
    active_build_token: Option<DdgiBuildToken>,
    active_ready: bool,
    latest_visible_terrain_revision: Option<u32>,
    terrain_refresh: DdgiTerrainRefresh,
    transport_scheduler: DdgiTransportScheduler,
    live_authored_lighting: Option<EnvironmentLightingState>,
    in_flight_authored_lighting: Option<EnvironmentLightingState>,
    published_authored_lighting: Option<EnvironmentLightingState>,
    resident_active_field: Option<DdgiFieldIdentity>,
    resident_active_authored_lighting: Option<EnvironmentLightingState>,
    completed_staging_authored_lighting:
        Option<(DdgiBuildToken, DdgiFieldIdentity, EnvironmentLightingState)>,
    coalesced_radiance_revisions: u64,
    camera_probe_priority: Option<UAabb3>,
    lighting_impact_probe_priority: Option<(u32, UAabb3)>,
    capture_enabled: bool,
    capture_target: DdgiCaptureTarget,
    capture_batch_order: DdgiBatchOrder,
    capture_checkpoint: Option<DdgiCaptureCheckpoint>,
    resident_active_capture_checkpoint: Option<DdgiCaptureCheckpoint>,
}

impl DdgiRuntime {
    pub(crate) fn new(active_grid: DdgiVolumeGrid) -> Self {
        Self {
            volumes: None,
            active_grid,
            active_build_token: None,
            active_ready: false,
            latest_visible_terrain_revision: None,
            terrain_refresh: DdgiTerrainRefresh::default(),
            transport_scheduler: DdgiTransportScheduler::new(),
            live_authored_lighting: None,
            in_flight_authored_lighting: None,
            published_authored_lighting: None,
            resident_active_field: None,
            resident_active_authored_lighting: None,
            completed_staging_authored_lighting: None,
            coalesced_radiance_revisions: 0,
            camera_probe_priority: None,
            lighting_impact_probe_priority: None,
            capture_enabled: false,
            capture_target: DdgiCaptureTarget::default(),
            capture_batch_order: DdgiBatchOrder::default(),
            capture_checkpoint: None,
            resident_active_capture_checkpoint: None,
        }
    }

    pub(crate) fn volumes(&self) -> &DdgiVolumes {
        self.volumes
            .as_ref()
            .expect("DDGI physical volumes must be installed before use")
    }

    pub(crate) fn volumes_mut(&mut self) -> &mut DdgiVolumes {
        self.volumes
            .as_mut()
            .expect("DDGI physical volumes must be installed before use")
    }

    pub(crate) fn install_volumes(&mut self, volumes: DdgiVolumes) {
        assert!(
            self.volumes.replace(volumes).is_none(),
            "DDGI physical volumes may only be installed once"
        );
    }

    /// Atomically promotes the validated physical staging Volume and its logical token.
    ///
    /// Descriptor publication remains the caller's frame-retirement concern, but the physical
    /// active/staging swap and runtime coordinator promotion cannot be performed independently.
    pub(crate) fn promote_ready_volume(
        &mut self,
        build_token: DdgiBuildToken,
    ) -> Result<DdgiVolume> {
        let retired_active = self.volumes_mut().promote_staging(build_token)?;
        assert!(
            self.mark_promoted(build_token),
            "promoted DDGI token must still be coordinator-authoritative"
        );
        Ok(retired_active)
    }

    pub(crate) fn configure_capture(
        &mut self,
        enabled: bool,
        target: DdgiCaptureTarget,
        batch_order: DdgiBatchOrder,
    ) {
        self.capture_enabled = enabled;
        self.capture_target = target;
        self.capture_batch_order = batch_order;
        self.capture_checkpoint = None;
        self.resident_active_capture_checkpoint = None;
    }

    /// Observes one authoritative terrain publication. Repeating the same publication is
    /// idempotent; a later observation supersedes all older terrain work.
    pub(crate) fn observe_visible_terrain(
        &mut self,
        geometry_revision: u32,
        edited_voxel_bound: UAabb3,
    ) -> bool {
        if self.latest_visible_terrain_revision == Some(geometry_revision) {
            return false;
        }
        self.latest_visible_terrain_revision = Some(geometry_revision);
        self.terrain_refresh
            .request(geometry_revision, edited_voxel_bound, self.active_grid);
        self.terrain_refresh
            .mark_terrain_published(geometry_revision);
        true
    }

    pub(crate) fn observe_authored_lighting(&mut self, lighting: EnvironmentLightingState) {
        assert_ne!(
            lighting.revision, 0,
            "Authored Environment Lighting revisions must be nonzero"
        );
        assert_eq!(
            lighting.snapshot.local_lights.info.transport_revision, lighting.revision,
            "DDGI local-light payload must be frozen to its authored transport revision",
        );
        if let Some(current) = self.live_authored_lighting {
            if current.revision == lighting.revision {
                assert_eq!(
                    current.snapshot, lighting.snapshot,
                    "Authored Environment Lighting reused revision {} for a different snapshot",
                    lighting.revision,
                );
                return;
            }
        }
        let previous_latest = self.transport_scheduler.latest_radiance_revision();
        let in_flight_revision = self
            .transport_scheduler
            .in_flight()
            .map(|work| work.destination().field().radiance_revision());
        let published_revision = self
            .transport_scheduler
            .published()
            .map(|field| field.field().radiance_revision());
        if previous_latest.is_some()
            && previous_latest != Some(lighting.revision)
            && previous_latest != in_flight_revision
            && previous_latest != published_revision
        {
            self.coalesced_radiance_revisions = self.coalesced_radiance_revisions.saturating_add(1);
        }
        self.live_authored_lighting = Some(lighting);
        self.transport_scheduler.observe_radiance(lighting.revision);
    }

    pub(crate) fn observe_camera_probe_priority(&mut self, voxel_bound: UAabb3) {
        self.camera_probe_priority = Some(voxel_bound);
    }

    /// Phase 2 local lights can publish their conservative DDGI influence bound through this seam.
    /// `None` clears the previous influence when no movable light affects the volume.
    #[allow(dead_code)]
    pub(crate) fn observe_lighting_impact_probe_priority(
        &mut self,
        radiance_revision: u32,
        voxel_bound: Option<UAabb3>,
    ) {
        self.lighting_impact_probe_priority = voxel_bound.map(|bound| (radiance_revision, bound));
    }

    pub(crate) fn request_density_rebuild(&mut self, spacing_voxels: u32) {
        self.terrain_refresh.request_density_rebuild(spacing_voxels);
    }

    /// Chooses the next physical Volume allocation and atomically installs its logical transport
    /// request. Preparation failure is fatal, so no rollback transition is exposed.
    pub(crate) fn claim_volume_build(&mut self) -> Option<DdgiRuntimeVolumeBuild> {
        if self.active_build_token.is_none() {
            let terrain_revision = self.latest_visible_terrain_revision?;
            let token = self
                .terrain_refresh
                .allocate_initial_build_token(terrain_revision, self.active_grid.spacing_voxels());
            self.terrain_refresh
                .consume_initial_revision(terrain_revision);
            self.active_build_token = Some(token);
            self.request_geometry_transport(token);
            return Some(DdgiRuntimeVolumeBuild {
                target: DdgiRuntimeVolumeTarget::Active,
                token,
            });
        }
        if !self.active_ready {
            return None;
        }

        let active_token = self
            .active_build_token
            .expect("initialized DDGI runtime must retain its active build token");
        let token = self.terrain_refresh.claim_next_build(
            self.active_grid.spacing_voxels(),
            active_token.terrain_revision(),
        )?;
        match token.kind() {
            DdgiBuildKind::Terrain => self.request_geometry_transport(token),
            DdgiBuildKind::Density => self.request_density_transport(token),
        }
        Some(DdgiRuntimeVolumeBuild {
            target: DdgiRuntimeVolumeTarget::Staging,
            token,
        })
    }

    fn request_geometry_transport(&mut self, token: DdgiBuildToken) {
        // Physical active residency is authoritative: the logical scheduler may currently publish
        // a private staging candidate that active descriptors cannot read. Runtime-owned identity
        // keeps that distinction testable without Vulkan resources.
        let transport_source = self
            .resident_active_field
            .filter(|source| source.field().spacing_voxels() == token.spacing_voxels());
        let preempted = self.transport_scheduler.request_geometry_from(
            token.terrain_revision(),
            token.spacing_voxels(),
            transport_source,
        );
        self.clear_preempted_snapshot(preempted);
    }

    fn request_density_transport(&mut self, token: DdgiBuildToken) {
        let preempted = self
            .transport_scheduler
            .request_density(token.spacing_voxels());
        self.clear_preempted_snapshot(preempted);
    }

    fn clear_preempted_snapshot(&mut self, preempted: Option<DdgiScheduledWork>) {
        if let Some(preempted) = preempted {
            let lighting = self
                .in_flight_authored_lighting
                .take()
                .expect("preempted DDGI work must retain its immutable authored lighting");
            assert_eq!(
                lighting.revision,
                preempted.destination().field().radiance_revision(),
                "preempted DDGI work and authored lighting revision diverged",
            );
        }
    }

    pub(crate) fn claim_transport_work(&mut self) -> Option<DdgiRuntimeWork> {
        let scheduled = self
            .transport_scheduler
            .claim_next()
            .unwrap_or_else(|error| panic!("DDGI transport claim failed: {error:?}"))?;
        assert!(
            self.in_flight_authored_lighting.is_none(),
            "DDGI runtime cannot replace an in-flight authored-lighting snapshot"
        );
        let authored_lighting = self
            .live_authored_lighting
            .expect("DDGI transport work requires an Authored Environment Lighting observation");
        assert_eq!(
            authored_lighting.revision,
            scheduled.destination().field().radiance_revision(),
            "DDGI transport work revision does not match live Authored Environment Lighting",
        );
        self.in_flight_authored_lighting = Some(authored_lighting);
        let radiance_history_policy = scheduled.transport_source().and_then(|source| {
            (source.field().radiance_revision()
                != scheduled.destination().field().radiance_revision())
            .then(|| {
                let source_revision = source.field().radiance_revision();
                let source_lighting = [
                    self.resident_active_authored_lighting,
                    self.published_authored_lighting,
                ]
                .into_iter()
                .flatten()
                .find(|lighting| lighting.revision == source_revision)
                .unwrap_or_else(|| {
                    panic!(
                        "DDGI has no immutable authored lighting for scheduled source revision {}",
                        source_revision,
                    )
                });
                DdgiRadianceHistoryPolicy::between(source_lighting, authored_lighting)
            })
        });
        let local_refresh_voxel_bound = (scheduled.kind() == DdgiScheduledWorkKind::GeometryUpdate)
            .then(|| self.terrain_refresh.invalidation_voxel_bound())
            .flatten();
        let lighting_impact_priority = self
            .lighting_impact_probe_priority
            .filter(|(revision, _)| {
                *revision == scheduled.destination().field().radiance_revision()
            })
            .map(|(_, bound)| {
                DdgiProbePriority::new(bound, DdgiProbePriorityReason::LightingImpact)
            });
        if lighting_impact_priority.is_some() {
            self.lighting_impact_probe_priority = None;
        }
        let probe_priority = local_refresh_voxel_bound
            .map(|bound| DdgiProbePriority::new(bound, DdgiProbePriorityReason::TerrainEdit))
            .or(lighting_impact_priority)
            .or_else(|| {
                self.camera_probe_priority
                    .map(|bound| DdgiProbePriority::new(bound, DdgiProbePriorityReason::Camera))
            });
        Some(DdgiRuntimeWork {
            scheduled,
            authored_lighting,
            radiance_history_policy,
            local_refresh_voxel_bound,
            probe_priority,
        })
    }

    pub(crate) fn validate_transport_completion(
        &self,
        work: DdgiScheduledWork,
        published: DdgiFieldIdentity,
    ) -> Result<(), DdgiSchedulerError> {
        self.transport_scheduler
            .validate_in_flight_completion(work, published)
    }

    pub(crate) fn complete_transport_work(
        &mut self,
        work: DdgiScheduledWork,
        published: DdgiFieldIdentity,
        build_token: DdgiBuildToken,
    ) -> Result<DdgiFieldIdentity, DdgiSchedulerError> {
        // Validate before taking the snapshot: a stale completion may arrive after newer work has
        // been claimed and must not consume that newer work's immutable authored lighting.
        self.transport_scheduler
            .validate_in_flight_completion(work, published)?;
        let lighting = self
            .in_flight_authored_lighting
            .take()
            .expect("completed DDGI work must retain its immutable authored lighting");
        assert_eq!(
            lighting.revision,
            work.destination().field().radiance_revision(),
            "completed DDGI work and authored lighting revision diverged",
        );
        let published = self
            .transport_scheduler
            .complete_in_flight(work, published)?;
        self.published_authored_lighting = Some(lighting);
        if self.active_build_token == Some(build_token) {
            self.resident_active_field = Some(published);
            self.resident_active_authored_lighting = Some(lighting);
            self.active_ready = true;
        } else {
            self.completed_staging_authored_lighting = Some((build_token, published, lighting));
        }
        Ok(published)
    }

    pub(crate) fn token_can_promote(&self, token: DdgiBuildToken) -> bool {
        self.terrain_refresh.token_can_promote(token)
    }

    pub(crate) fn finish_obsolete_volume_build(&mut self, token: DdgiBuildToken) -> bool {
        if !self.terrain_refresh.finish_obsolete_candidate(token) {
            return false;
        }
        if self
            .completed_staging_authored_lighting
            .is_some_and(|(candidate, _, _)| candidate == token)
        {
            self.completed_staging_authored_lighting = None;
        }
        true
    }

    pub(crate) fn mark_promoted(&mut self, token: DdgiBuildToken) -> bool {
        if !self.terrain_refresh.mark_promoted(token) {
            return false;
        }
        self.active_grid = DdgiVolumeGrid::new(
            self.active_grid.world_extent_voxels(),
            token.spacing_voxels(),
        )
        .expect("promoted DDGI token must retain a supported Volume grid");
        self.active_build_token = Some(token);
        self.active_ready = true;
        let (candidate_token, field, lighting) = self
            .completed_staging_authored_lighting
            .take()
            .expect("promoted DDGI staging token must retain its immutable authored lighting");
        assert_eq!(
            candidate_token, token,
            "promoted DDGI token and completed staging lighting diverged"
        );
        self.resident_active_field = Some(field);
        self.resident_active_authored_lighting = Some(lighting);
        if self
            .capture_checkpoint
            .is_some_and(|checkpoint| checkpoint.build_token == token)
        {
            self.resident_active_capture_checkpoint = self.capture_checkpoint;
        }
        true
    }

    pub(crate) fn status(&self, volumes: DdgiStatus) -> DdgiRuntimeStatus {
        let capture_checkpoint = self.capture_checkpoint(volumes);
        DdgiRuntimeStatus::from_parts(volumes, self.terrain_refresh, capture_checkpoint)
    }

    pub(crate) fn capture_checkpoint(&self, volumes: DdgiStatus) -> Option<DdgiCaptureCheckpoint> {
        let active = volumes.active();
        [
            self.capture_checkpoint,
            self.resident_active_capture_checkpoint,
        ]
        .into_iter()
        .flatten()
        .find(|checkpoint| {
            if active.build_token != Some(checkpoint.build_token) {
                return false;
            }
            match checkpoint.publication {
                DdgiCapturePublication::Published => {
                    active.published_field == Some(checkpoint.field)
                }
                DdgiCapturePublication::Unpublished => {
                    active.published_field.is_none()
                        && active.complete_field == Some(checkpoint.field)
                }
            }
        })
    }

    pub(crate) fn capture_target(&self) -> DdgiCaptureTarget {
        self.capture_target
    }

    pub(crate) fn observe_capture_checkpoint(
        &mut self,
        build_token: DdgiBuildToken,
        field: DdgiFieldIdentity,
        validation: DdgiAtlasValidationStats,
        publication: DdgiCapturePublication,
    ) -> bool {
        if !self.capture_enabled || !self.capture_target.matches_checkpoint(field, publication) {
            return false;
        }
        let checkpoint = DdgiCaptureCheckpoint {
            build_token,
            field,
            validation,
            publication,
            batch_order: self.capture_batch_order,
        };
        self.capture_checkpoint = Some(checkpoint);
        if self.active_build_token == Some(build_token) {
            self.resident_active_capture_checkpoint = Some(checkpoint);
        }
        true
    }

    pub(crate) fn edited_voxel_bound(&self) -> Option<UAabb3> {
        self.terrain_refresh.edited_voxel_bound()
    }

    pub(crate) fn invalidation_voxel_bound(&self) -> Option<UAabb3> {
        self.terrain_refresh.invalidation_voxel_bound()
    }

    pub(crate) fn refresh_state(&self) -> DdgiRefreshState {
        self.terrain_refresh.state()
    }

    pub(crate) fn latest_radiance_revision(&self) -> Option<u32> {
        self.transport_scheduler.latest_radiance_revision()
    }

    pub(crate) fn lighting_diagnostics(&self) -> DdgiLightingDiagnostics {
        let in_flight_revision = self
            .transport_scheduler
            .in_flight()
            .map(|work| work.destination().field().radiance_revision());
        let authored_in_flight_revision = self
            .in_flight_authored_lighting
            .map(|lighting| lighting.revision);
        DdgiLightingDiagnostics {
            latest_transport_revision: self.transport_scheduler.latest_radiance_revision(),
            latest_source_live_revision: self
                .live_authored_lighting
                .map(|lighting| lighting.source_live_revision),
            scheduler_published_revision: self
                .transport_scheduler
                .published()
                .map(|field| field.field().radiance_revision()),
            in_flight_revision,
            coalesced_revisions: self.coalesced_radiance_revisions,
            has_mixed_in_flight_revision: in_flight_revision != authored_in_flight_revision,
        }
    }

    pub(crate) fn live_authored_lighting(&self) -> Option<EnvironmentLightingState> {
        self.live_authored_lighting
    }

    pub(crate) fn in_flight_authored_lighting(&self) -> Option<EnvironmentLightingState> {
        self.in_flight_authored_lighting
    }

    /// Selects all DDGI logical work for the current frame without exposing physical atlas
    /// ownership to the caller that records Vulkan commands.
    pub(crate) fn frame_plan(&self) -> DdgiFramePlan {
        let builder = self.volumes().builder();
        let ray_batch = builder.next_ray_batch_to_trace();
        DdgiFramePlan {
            global_sky_needs_update: builder.global_sky_needs_update(),
            relocation_terrain_revision: builder.pending_relocation_terrain_revision(),
            visibility_preservation_needed: builder.visibility_preservation_needed(),
            iteration_will_complete: ray_batch
                .is_some_and(|batch| builder.iteration_will_complete(batch)),
            ray_batch,
        }
    }

    pub(crate) fn mark_global_sky_ready(&mut self, environment_revision: u32) -> Result<()> {
        self.volumes_mut()
            .builder_mut()
            .mark_global_sky_ready(environment_revision)
    }

    pub(crate) fn mark_relocated(&mut self, terrain_revision: u32) -> Result<()> {
        self.volumes_mut()
            .builder_mut()
            .mark_relocated(terrain_revision)
    }

    pub(crate) fn mark_visibility_preserved(&mut self) {
        self.volumes_mut().builder_mut().mark_visibility_preserved();
    }

    pub(crate) fn mark_ray_batch_ready(&mut self, batch: DdgiRayBatch) {
        self.volumes_mut().builder_mut().mark_ray_batch_ready(batch);
    }

    pub(crate) fn mark_ray_batch_filtered(&mut self, batch: DdgiRayBatch) {
        self.volumes_mut()
            .builder_mut()
            .mark_ray_batch_filtered(batch);
    }
}

/// Semantic work identity exposed by the runtime without exposing scheduler operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DdgiRuntimeTargetWork {
    kind: DdgiScheduledWorkKind,
    destination: DdgiFieldIdentity,
}

impl DdgiRuntimeTargetWork {
    pub fn kind(self) -> DdgiScheduledWorkKind {
        self.kind
    }

    pub fn destination(self) -> DdgiFieldIdentity {
        self.destination
    }
}

impl From<DdgiScheduledWork> for DdgiRuntimeTargetWork {
    fn from(work: DdgiScheduledWork) -> Self {
        Self {
            kind: work.kind(),
            destination: work.destination(),
        }
    }
}

/// Semantic evidence for one resident DDGI Volume.
///
/// Physical atlas layouts, ping-pong slots, ray-batch traversal, descriptors, and pipeline state
/// remain private to the DDGI implementation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DdgiRuntimeVolumeStatus {
    pub build_token: Option<DdgiBuildToken>,
    pub grid: DdgiVolumeGrid,
    pub resource_bytes: DdgiResourceBytes,
    pub stage: DdgiVolumeStage,
    pub target_work: Option<DdgiRuntimeTargetWork>,
    pub complete_field: Option<DdgiFieldIdentity>,
    pub published_field: Option<DdgiFieldIdentity>,
    pub building_field: Option<DdgiFieldIdentity>,
    pub consecutive_below_threshold: u32,
    pub last_atlas_validation: Option<DdgiAtlasValidationStats>,
    pub global_sky_revision: u32,
    pub radiance_revision: Option<u32>,
    pub relocated_terrain_revision: Option<u32>,
    pub filtered_probe_count: u32,
    pub probe_priority: Option<DdgiProbePriority>,
}

impl DdgiRuntimeVolumeStatus {
    pub fn is_ready(self) -> bool {
        self.published_field.is_some()
    }
}

impl From<DdgiVolumeStatus> for DdgiRuntimeVolumeStatus {
    fn from(status: DdgiVolumeStatus) -> Self {
        Self {
            build_token: status.build_token,
            grid: status.grid,
            resource_bytes: status.resource_bytes,
            stage: status.stage,
            target_work: status.scheduled_work.map(Into::into),
            complete_field: status.complete_field,
            published_field: status.published_field,
            building_field: status.building_field,
            consecutive_below_threshold: status.consecutive_below_threshold,
            last_atlas_validation: status.last_atlas_validation,
            global_sky_revision: status.global_sky_revision,
            radiance_revision: status.radiance_revision,
            relocated_terrain_revision: status.relocated_terrain_revision,
            filtered_probe_count: status.filtered_probe_count,
            probe_priority: status.probe_priority,
        }
    }
}

/// Canonical observation of the DDGI Volume runtime lifecycle.
///
/// The variants make staging evidence available only while a staging volume exists. Callers can
/// observe logical identities, progress, and active-field availability without learning atlas
/// ownership or descriptor publication ordering.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DdgiRuntimeStatus {
    state: DdgiRuntimeState,
    capture_checkpoint: Option<DdgiCaptureCheckpoint>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum DdgiRuntimeState {
    Active {
        active: DdgiRuntimeVolumeStatus,
        coordinator: DdgiRefreshState,
        deferred_density_spacing_voxels: Option<u32>,
    },
    Staging {
        active: DdgiRuntimeVolumeStatus,
        staging: DdgiRuntimeVolumeStatus,
        coordinator: DdgiRefreshState,
        deferred_density_spacing_voxels: Option<u32>,
    },
}

impl DdgiRuntimeStatus {
    fn from_parts(
        volumes: DdgiStatus,
        refresh: DdgiTerrainRefresh,
        capture_checkpoint: Option<DdgiCaptureCheckpoint>,
    ) -> Self {
        let active = volumes.active().into();
        let coordinator = refresh.state();
        let deferred_density_spacing_voxels = refresh.queued_density_spacing_voxels();
        match volumes.staging() {
            Some(staging) => {
                assert!(
                    staging.build_token.is_some(),
                    "DDGI staging status must have an immutable build token"
                );
                Self {
                    state: DdgiRuntimeState::Staging {
                        active,
                        staging: staging.into(),
                        coordinator,
                        deferred_density_spacing_voxels,
                    },
                    capture_checkpoint,
                }
            }
            None => Self {
                state: DdgiRuntimeState::Active {
                    active,
                    coordinator,
                    deferred_density_spacing_voxels,
                },
                capture_checkpoint,
            },
        }
    }

    pub fn capture_checkpoint(self) -> Option<DdgiCaptureCheckpoint> {
        self.capture_checkpoint
    }

    pub fn active(self) -> DdgiRuntimeVolumeStatus {
        match self.state {
            DdgiRuntimeState::Active { active, .. } | DdgiRuntimeState::Staging { active, .. } => {
                active
            }
        }
    }

    pub fn staging(self) -> Option<DdgiRuntimeVolumeStatus> {
        match self.state {
            DdgiRuntimeState::Active { .. } => None,
            DdgiRuntimeState::Staging { staging, .. } => Some(staging),
        }
    }

    pub fn builder(self) -> DdgiRuntimeVolumeStatus {
        self.staging().unwrap_or_else(|| self.active())
    }

    pub fn coordinator(self) -> DdgiRefreshState {
        match self.state {
            DdgiRuntimeState::Active { coordinator, .. }
            | DdgiRuntimeState::Staging { coordinator, .. } => coordinator,
        }
    }

    pub fn target_terrain_revision(self) -> Option<u32> {
        match self.coordinator() {
            DdgiRefreshState::AwaitingTerrain {
                latest_terrain_revision,
            }
            | DdgiRefreshState::BuildingTerrain {
                latest_terrain_revision,
                ..
            } => Some(latest_terrain_revision),
            DdgiRefreshState::Idle
            | DdgiRefreshState::DensityQueued { .. }
            | DdgiRefreshState::BuildingDensity { .. } => None,
        }
    }

    pub fn deferred_density_spacing_voxels(self) -> Option<u32> {
        match self.state {
            DdgiRuntimeState::Active {
                deferred_density_spacing_voxels,
                ..
            }
            | DdgiRuntimeState::Staging {
                deferred_density_spacing_voxels,
                ..
            } => deferred_density_spacing_voxels,
        }
    }

    pub fn active_consumers_are_available(self) -> bool {
        self.active().is_ready()
    }

    pub fn active_token_serial(self) -> Option<u64> {
        self.active().build_token.map(DdgiBuildToken::serial)
    }

    pub fn staging_token(self) -> Option<DdgiBuildToken> {
        self.staging().map(|staging| {
            staging
                .build_token
                .expect("DDGI staging status was validated at construction")
        })
    }

    pub fn active_line(self) -> String {
        let active = self.active();
        format!(
            "Active token {} · terrain {} · radiance {} · {} vox · {:?} · published {}",
            format_optional(self.active_token_serial()),
            format_optional(active.relocated_terrain_revision),
            format_optional(active.radiance_revision),
            active.grid.spacing_voxels(),
            active.stage,
            format_ddgi_field(active.published_field),
        )
    }

    pub fn builder_line(self) -> String {
        let Some(staging) = self.staging() else {
            return "Builder none".to_owned();
        };
        let token = self
            .staging_token()
            .expect("staging volume must expose its build token");
        format!(
            "Builder token {} · {:?} · terrain {} · radiance {} · {} vox · {}/{} filtered · {:?} · complete {} · building {} · published {}",
            token.serial(),
            token.kind(),
            format_optional(staging.relocated_terrain_revision),
            format_optional(staging.radiance_revision),
            staging.grid.spacing_voxels(),
            staging.filtered_probe_count,
            staging.grid.probe_count(),
            staging.stage,
            format_ddgi_field(staging.complete_field),
            format_ddgi_field(staging.building_field),
            format_ddgi_field(staging.published_field),
        )
    }

    pub fn coordinator_line(self) -> String {
        format!(
            "Target terrain {} · coordinator {:?} · density queued {}",
            format_optional(self.target_terrain_revision()),
            self.coordinator(),
            format_optional(self.deferred_density_spacing_voxels()),
        )
    }

    pub fn availability_line(self) -> &'static str {
        if self.target_terrain_revision().is_some() && self.active_consumers_are_available() {
            "Probes: active field available · replacement pending"
        } else if self.active_consumers_are_available() {
            "Probes: active field available"
        } else {
            "Probes: initializing"
        }
    }
}

fn format_optional<T: std::fmt::Display>(value: Option<T>) -> String {
    value.map_or_else(|| "none".to_owned(), |value| value.to_string())
}

fn format_ddgi_field(value: Option<DdgiFieldIdentity>) -> String {
    value.map_or_else(
        || "none".to_owned(),
        |identity| {
            let field = identity.field();
            format!(
                "#{}/E{} {:?} <- {:?}",
                field.serial(),
                field.update_epoch(),
                field.state(),
                identity.source().map(|source| source.serial()),
            )
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ddgi::{
        DdgiAtlasLayout, DdgiBuildKind, DdgiFieldKey, DdgiFieldState, DdgiResourceBytes,
        DdgiVolumeGrid, DdgiVolumeStage,
    };
    use crate::environment_lighting::{DdgiRadianceSnapshot, DdgiVoxelPaletteSnapshot};
    use crate::geom::UAabb3;
    use crate::lighting::{
        LocalLight, LocalLightBudget, LocalLightDomain, LocalLightGpuPayload,
        LocalLightGpuSnapshot, PointLight,
    };
    use glam::{UVec3, Vec3};

    fn field(geometry_revision: u32, radiance_revision: u32) -> super::DdgiFieldIdentity {
        DdgiFieldIdentity::new(
            DdgiFieldKey::new(
                1,
                geometry_revision,
                radiance_revision,
                16,
                DdgiFieldState::Converging,
                0,
            )
            .unwrap(),
            None,
        )
        .unwrap()
    }

    fn lighting(revision: u32, sun_luminance: f32) -> EnvironmentLightingState {
        EnvironmentLightingState {
            revision,
            source_live_revision: u64::from(revision),
            published_at: std::time::Duration::from_millis(u64::from(revision) * 10),
            change: crate::environment_lighting::DdgiRadianceChange::default(),
            snapshot: DdgiRadianceSnapshot {
                sun_direction: Vec3::Y,
                sun_color: Vec3::new(1.0, 0.9, 0.8),
                sun_luminance,
                terrain_ray_origin_offset_world: 0.005,
                ddgi_receiver_visibility_bias_world: 0.001,
                voxel_palette: DdgiVoxelPaletteSnapshot {
                    dirt_color: Vec3::new(0.1, 0.2, 0.3),
                    sand_color: Vec3::new(0.4, 0.5, 0.6),
                    cherry_wood_color: Vec3::new(0.7, 0.2, 0.1),
                    oak_wood_color: Vec3::new(0.2, 0.3, 0.1),
                    rock_color: Vec3::splat(0.4),
                    hash_color_variance: 0.5,
                },
                local_lights: LocalLightGpuPayload::empty(0).with_transport_revision(revision),
            },
        }
    }

    fn edit_bound(min: u32, max: u32) -> UAabb3 {
        UAabb3::new(UVec3::splat(min), UVec3::splat(max))
    }

    fn initialized_runtime() -> (DdgiRuntime, DdgiBuildToken, DdgiFieldIdentity) {
        let grid = DdgiVolumeGrid::new(UVec3::splat(512), 16).unwrap();
        let mut runtime = DdgiRuntime::new(grid);
        runtime.observe_authored_lighting(lighting(1, 1.0));
        assert!(runtime.observe_visible_terrain(7, edit_bound(100, 120)));
        let build = runtime.claim_volume_build().unwrap();
        assert_eq!(build.target(), DdgiRuntimeVolumeTarget::Active);
        let token = build.token();
        let work = runtime.claim_transport_work().unwrap().scheduled();
        let published = work.destination();
        runtime
            .complete_transport_work(work, published, token)
            .unwrap();
        (runtime, token, published)
    }

    fn volume_status(
        token: Option<DdgiBuildToken>,
        geometry_revision: u32,
        radiance_revision: u32,
        stage: DdgiVolumeStage,
    ) -> DdgiVolumeStatus {
        let grid = DdgiVolumeGrid::new(UVec3::splat(512), 16).unwrap();
        let identity = field(geometry_revision, radiance_revision);
        DdgiVolumeStatus {
            build_token: token,
            grid,
            irradiance_layout: DdgiAtlasLayout::new(grid.probe_count(), 6).unwrap(),
            visibility_layout: DdgiAtlasLayout::new(grid.probe_count(), 14).unwrap(),
            resource_bytes: DdgiResourceBytes::for_grid(grid).unwrap(),
            stage,
            scheduled_work: None,
            complete_field: Some(identity),
            published_field: Some(identity),
            building_field: Some(identity),
            consecutive_below_threshold: 0,
            last_atlas_validation: None,
            global_sky_revision: radiance_revision,
            radiance_revision: Some(radiance_revision),
            relocated_terrain_revision: Some(geometry_revision),
            active_ray_batch: None,
            filtered_probe_count: 2048,
            probe_priority: None,
            promotion_ready: true,
        }
    }

    #[test]
    fn terrain_probe_refresh_keeps_active_consumers_available_while_staging() {
        let (mut runtime, active_token, _) = initialized_runtime();
        let active = volume_status(Some(active_token), 7, 3, DdgiVolumeStage::Ready);
        let active_only = runtime.status(DdgiStatus::new(active, None));
        assert!(matches!(active_only.state, DdgiRuntimeState::Active { .. }));
        assert!(active_only.staging().is_none());
        assert_eq!(
            active_only.active_line(),
            "Active token 1 · terrain 7 · radiance 3 · 16 vox · Ready · published #1/E0 Converging <- None"
        );
        assert_eq!(active_only.builder_line(), "Builder none");
        assert_eq!(
            active_only.availability_line(),
            "Probes: active field available"
        );

        runtime.request_density_rebuild(32);
        assert!(runtime.observe_visible_terrain(8, edit_bound(200, 220)));
        let token = runtime.claim_volume_build().unwrap().token();
        let staging = volume_status(Some(token), 8, 4, DdgiVolumeStage::Rebuilding);
        let building = runtime.status(DdgiStatus::new(active, Some(staging)));
        assert!(matches!(building.state, DdgiRuntimeState::Staging { .. }));
        assert_eq!(building.staging_token(), Some(token));
        assert!(building
            .builder_line()
            .starts_with("Builder token 2 · Terrain"));
        assert!(building.coordinator_line().contains("Target terrain 8"));
        assert!(building.coordinator_line().contains("density queued 32"));
        assert!(building.active_consumers_are_available());
        assert_eq!(
            building.availability_line(),
            "Probes: active field available · replacement pending"
        );
    }

    #[test]
    fn capture_checkpoint_is_runtime_owned_and_requires_resident_active_field() {
        let (mut runtime, token, _) = initialized_runtime();
        let captured_field = field(7, 3);
        runtime.configure_capture(true, DdgiCaptureTarget::Published, DdgiBatchOrder::Reverse);
        runtime.observe_capture_checkpoint(
            token,
            captured_field,
            DdgiAtlasValidationStats::default(),
            DdgiCapturePublication::Published,
        );

        let active = volume_status(Some(token), 7, 3, DdgiVolumeStage::Ready);
        let checkpoint = runtime
            .status(DdgiStatus::new(active, None))
            .capture_checkpoint()
            .expect("resident published field should expose the checkpoint");
        assert_eq!(checkpoint.field, captured_field);
        assert_eq!(checkpoint.batch_order, DdgiBatchOrder::Reverse);

        let wrong_token = DdgiBuildToken::for_test(2, 7, 16, DdgiBuildKind::Terrain);
        let staging_field = field(8, 4);
        runtime.observe_capture_checkpoint(
            wrong_token,
            staging_field,
            DdgiAtlasValidationStats::default(),
            DdgiCapturePublication::Published,
        );
        let active_after_staging_checkpoint = runtime
            .status(DdgiStatus::new(active, None))
            .capture_checkpoint()
            .expect("staging capture evidence must not hide the resident active checkpoint");
        assert_eq!(active_after_staging_checkpoint.field, captured_field);

        let mismatched_active = volume_status(Some(wrong_token), 7, 3, DdgiVolumeStage::Ready);
        assert!(runtime
            .status(DdgiStatus::new(mismatched_active, None))
            .capture_checkpoint()
            .is_none());
    }

    #[test]
    #[should_panic(expected = "DDGI staging status must have an immutable build token")]
    fn staging_without_a_build_token_fails_fast() {
        let (runtime, _, _) = initialized_runtime();
        let active = volume_status(None, 7, 3, DdgiVolumeStage::Ready);
        let staging = volume_status(None, 8, 4, DdgiVolumeStage::Rebuilding);
        runtime.status(DdgiStatus::new(active, Some(staging)));
    }

    #[test]
    fn terrain_observation_drives_initialization_and_local_invalidation() {
        let grid = DdgiVolumeGrid::new(UVec3::splat(512), 32).unwrap();
        let mut runtime = DdgiRuntime::new(grid);
        runtime.observe_authored_lighting(lighting(1, 1.0));

        assert!(runtime.observe_visible_terrain(7, edit_bound(100, 120)));
        assert!(!runtime.observe_visible_terrain(7, edit_bound(200, 220)));
        assert_eq!(
            runtime.refresh_state(),
            DdgiRefreshState::AwaitingTerrain {
                latest_terrain_revision: 7,
            }
        );
        assert_eq!(
            runtime.invalidation_voxel_bound(),
            Some(UAabb3::new(UVec3::splat(68), UVec3::splat(152)))
        );

        let build = runtime.claim_volume_build().unwrap();
        assert_eq!(build.target(), DdgiRuntimeVolumeTarget::Active);
        assert_eq!(build.token().terrain_revision(), 7);
        assert_eq!(build.token().spacing_voxels(), 32);
        assert_eq!(runtime.refresh_state(), DdgiRefreshState::Idle);
        assert_eq!(runtime.invalidation_voxel_bound(), None);
    }

    #[test]
    fn latest_terrain_revision_waits_for_older_in_flight_work_to_finish() {
        let (mut runtime, _, _) = initialized_runtime();
        assert!(runtime.observe_visible_terrain(8, edit_bound(100, 120)));
        let first = runtime.claim_volume_build().unwrap().token();
        let first_work = runtime.claim_transport_work().unwrap();
        assert_eq!(
            first_work
                .scheduled()
                .destination()
                .field()
                .geometry_revision(),
            8
        );

        assert!(runtime.observe_visible_terrain(9, edit_bound(200, 220)));
        assert_eq!(runtime.claim_volume_build(), None);
        assert!(!runtime.token_can_promote(first));
        let first_published = first_work.scheduled().destination();
        runtime
            .complete_transport_work(first_work.scheduled(), first_published, first)
            .unwrap();
        assert!(runtime.finish_obsolete_volume_build(first));
        let latest = runtime.claim_volume_build().unwrap().token();
        assert!(runtime.token_can_promote(latest));
        assert_eq!(latest.terrain_revision(), 9);
        assert_eq!(runtime.in_flight_authored_lighting(), None);
        let latest_work = runtime.claim_transport_work().unwrap();
        assert_eq!(
            latest_work
                .scheduled()
                .destination()
                .field()
                .geometry_revision(),
            9
        );
    }

    #[test]
    fn terrain_refresh_reuses_the_resident_field_as_temporal_history() {
        let (mut runtime, _, resident) = initialized_runtime();
        assert!(runtime.observe_visible_terrain(8, edit_bound(100, 120)));
        let build = runtime.claim_volume_build().unwrap();
        assert_eq!(build.target(), DdgiRuntimeVolumeTarget::Staging);

        let refresh = runtime.claim_transport_work().unwrap().scheduled();
        assert_eq!(refresh.kind(), DdgiScheduledWorkKind::GeometryUpdate);
        assert_eq!(refresh.destination().field().geometry_revision(), 8);
        assert_eq!(refresh.destination().field().update_epoch(), 0);
        assert_eq!(refresh.destination().source(), Some(resident.field()));
    }

    #[test]
    fn superseded_geometry_keeps_resident_lighting_for_the_replacement_source() {
        let (mut runtime, _, resident) = initialized_runtime();
        let r2 = lighting(2, 2.0);
        runtime.observe_authored_lighting(r2);

        assert!(runtime.observe_visible_terrain(8, edit_bound(100, 120)));
        let obsolete_token = runtime.claim_volume_build().unwrap().token();
        let obsolete_work = runtime.claim_transport_work().unwrap();
        assert_eq!(
            obsolete_work.scheduled().destination().source(),
            Some(resident.field())
        );

        assert!(runtime.observe_visible_terrain(9, edit_bound(200, 220)));
        runtime
            .complete_transport_work(
                obsolete_work.scheduled(),
                obsolete_work.scheduled().destination(),
                obsolete_token,
            )
            .unwrap();
        assert!(runtime.finish_obsolete_volume_build(obsolete_token));

        let replacement_token = runtime.claim_volume_build().unwrap().token();
        assert_eq!(replacement_token.terrain_revision(), 9);
        let replacement = runtime.claim_transport_work().unwrap();
        assert_eq!(
            replacement.scheduled().destination().source(),
            Some(resident.field()),
            "replacement geometry must inherit the physically resident field, not obsolete staging"
        );
        assert_eq!(replacement.authored_lighting().snapshot, r2.snapshot);
        let history = replacement
            .radiance_history_policy()
            .expect("resident r1 to live r2 must retain an explicit radiance history policy");
        assert_eq!(history.elapsed, std::time::Duration::from_millis(10));
    }

    #[test]
    fn density_request_uses_the_active_geometry_and_requested_spacing() {
        let (mut runtime, active_token, _) = initialized_runtime();
        runtime.request_density_rebuild(32);
        let build = runtime.claim_volume_build().unwrap();
        assert_eq!(build.target(), DdgiRuntimeVolumeTarget::Staging);
        assert_eq!(build.token().kind(), DdgiBuildKind::Density);
        assert_eq!(
            build.token().terrain_revision(),
            active_token.terrain_revision()
        );
        assert_eq!(build.token().spacing_voxels(), 32);
        let work = runtime.claim_transport_work().unwrap().scheduled();
        assert_eq!(work.kind(), DdgiScheduledWorkKind::DensityUpdate);
        assert_eq!(work.destination().field().spacing_voxels(), 32);
    }

    #[test]
    fn radiance_observations_coalesce_without_mutating_the_in_flight_snapshot() {
        let (mut runtime, active_token, _) = initialized_runtime();
        let mut lights = LocalLightDomain::default();
        let id = lights.add(LocalLight::Point(
            PointLight::new(Vec3::ONE, Vec3::ONE, 4.0, 0.05, 0.5).unwrap(),
        ));
        let mut r2 = lighting(2, 2.0);
        r2.snapshot.local_lights = LocalLightGpuSnapshot::from_authoritative(
            &lights.snapshot(),
            LocalLightBudget::point_lights(1),
            2,
        )
        .payload();
        runtime.observe_authored_lighting(r2);
        let r2_work = runtime.claim_transport_work().unwrap();
        assert_eq!(
            r2_work
                .scheduled()
                .destination()
                .field()
                .radiance_revision(),
            2
        );
        assert_eq!(r2_work.authored_lighting().snapshot, r2.snapshot);

        runtime.observe_authored_lighting(lighting(3, 3.0));
        lights.remove(id).unwrap();
        let mut r4 = lighting(4, 4.0);
        r4.snapshot.local_lights = LocalLightGpuSnapshot::from_authoritative(
            &lights.snapshot(),
            LocalLightBudget::point_lights(1),
            4,
        )
        .payload();
        runtime.observe_authored_lighting(r4);
        let pending = runtime.lighting_diagnostics();
        assert_eq!(runtime.latest_radiance_revision(), Some(4));
        assert_eq!(pending.latest_transport_revision, Some(4));
        assert_eq!(pending.scheduler_published_revision, Some(1));
        assert_eq!(pending.in_flight_revision, Some(2));
        assert_eq!(pending.coalesced_revisions, 1);
        assert_eq!(pending.scheduler_revision_lag(), 3);
        assert!(!pending.has_mixed_in_flight_revision);
        assert_eq!(
            runtime.in_flight_authored_lighting().unwrap().snapshot,
            r2.snapshot,
        );
        assert_eq!(
            runtime
                .in_flight_authored_lighting()
                .unwrap()
                .snapshot
                .local_lights
                .count(),
            1
        );

        let r2_scheduled = r2_work.scheduled();
        runtime
            .complete_transport_work(r2_scheduled, r2_scheduled.destination(), active_token)
            .unwrap();
        let latest = runtime.claim_transport_work().unwrap();
        let claimed = runtime.lighting_diagnostics();
        assert_eq!(
            latest.scheduled().destination().field().radiance_revision(),
            4
        );
        assert_eq!(claimed.scheduler_published_revision, Some(2));
        assert_eq!(claimed.in_flight_revision, Some(4));
        assert_eq!(claimed.scheduler_revision_lag(), 2);
        assert!(!claimed.has_mixed_in_flight_revision);
        assert_eq!(latest.authored_lighting().snapshot, r4.snapshot);
        assert_eq!(latest.authored_lighting().snapshot.local_lights.count(), 0);
        assert_eq!(
            latest.scheduled().destination().source(),
            Some(r2_scheduled.destination().field())
        );
    }

    #[test]
    fn radiance_work_derives_history_from_the_actual_published_source() {
        let (mut runtime, _, _) = initialized_runtime();
        let mut continuous = lighting(2, 1.0);
        continuous.published_at = std::time::Duration::from_millis(210);
        continuous.snapshot.sun_direction =
            glam::Quat::from_rotation_x(1.0_f32.to_radians()) * Vec3::Y;
        runtime.observe_authored_lighting(continuous);

        let work = runtime.claim_transport_work().unwrap();
        let history = work
            .radiance_history_policy()
            .expect("radiance update must name its temporal policy");
        assert_eq!(history.elapsed, std::time::Duration::from_millis(200));
        assert_eq!(
            history.change.reason,
            crate::environment_lighting::DdgiRadianceChangeReason::ContinuousSun
        );
        assert!(!history.resets_history());
        assert!(history.retention(0.99) > 0.0);
    }

    #[test]
    fn transport_work_latches_impact_then_camera_priority_without_changing_sweep_scope() {
        let (mut runtime, active_token, _) = initialized_runtime();
        let camera = edit_bound(32, 32);
        let impact = edit_bound(256, 288);
        runtime.observe_camera_probe_priority(camera);
        runtime.observe_authored_lighting(lighting(2, 2.0));
        runtime.observe_lighting_impact_probe_priority(2, Some(impact));

        let impact_work = runtime.claim_transport_work().unwrap();
        assert_eq!(
            impact_work.probe_priority(),
            Some(DdgiProbePriority::new(
                impact,
                DdgiProbePriorityReason::LightingImpact
            ))
        );
        runtime
            .complete_transport_work(
                impact_work.scheduled(),
                impact_work.scheduled().destination(),
                active_token,
            )
            .unwrap();

        runtime.observe_authored_lighting(lighting(3, 3.0));
        runtime.observe_lighting_impact_probe_priority(3, None);
        let camera_work = runtime.claim_transport_work().unwrap();
        assert_eq!(
            camera_work.probe_priority(),
            Some(DdgiProbePriority::new(
                camera,
                DdgiProbePriorityReason::Camera
            ))
        );
    }
}
