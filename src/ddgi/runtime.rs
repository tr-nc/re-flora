use crate::environment_lighting::{
    AuthoredEnvironmentLightingFact, DdgiRadianceChange, DdgiRadianceChangeReason,
    DdgiRadianceDelta, DdgiRadianceHistoryPolicy, EnvironmentLightingState,
};
use crate::geom::UAabb3;
use anyhow::{Context, Result};
use std::time::Duration;

use super::resources::{
    DdgiConsumerResources, DdgiStatus, DdgiVolume, DdgiVolumeStatus, DdgiVolumes,
};
use super::scheduler::DdgiSchedulerCompletionPermit;
#[cfg(test)]
use super::DdgiSchedulerError;
use super::{
    DdgiAtlasValidationStats, DdgiBatchOrder, DdgiBuildKind, DdgiBuildToken, DdgiCaptureCheckpoint,
    DdgiCapturePublication, DdgiCaptureTarget, DdgiConvergenceReason, DdgiFieldIdentity,
    DdgiFilterConfigurationIdentity, DdgiFilterEpochAccumulator, DdgiFilterEpochProof,
    DdgiProbePriority, DdgiProbePriorityReason, DdgiRayBatch, DdgiRefreshState, DdgiResourceBytes,
    DdgiScheduledWork, DdgiScheduledWorkKind, DdgiTerrainRefresh, DdgiTraceStats,
    DdgiTransportScheduler, DdgiValidatedIterationOutcome, DdgiVerifiedBatchOutcome,
    DdgiVolumeGrid, DdgiVolumeStage, DDGI_CONVERGENCE_POLICY, DDGI_RAYS_PER_PROBE,
};

const DDGI_TRANSPORT_MIN_PUBLICATION_INTERVAL: Duration = Duration::from_millis(200);
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

/// Stable identity and physical DDGI work for one frame recording transaction.
///
/// This capsule contains no Vulkan types. The concrete Tracer encoder consumes it once and the
/// runtime settles the complete encoding result once; callers cannot advance individual Volume
/// stages or re-plan between passes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DdgiFrameWork {
    serial: u64,
    plan: DdgiFramePlan,
}

impl DdgiFrameWork {
    pub(crate) fn plan(self) -> DdgiFramePlan {
        self.plan
    }
}

/// Typed result of reconciling one deferred physical batch completion.
pub(crate) struct DdgiBatchCompletion {
    pub stats: Option<DdgiTraceStats>,
    pub radiance_snapshot: Option<crate::environment_lighting::DdgiRadianceSnapshot>,
    pub probe_count: u32,
    pub build_token: Option<DdgiBuildToken>,
    pub status: DdgiRuntimeVolumeStatus,
    validated_publication: Option<DdgiValidatedPublication>,
    pending_convergence_evidence: Option<convergence_evidence::Pending>,
    pub consumer_descriptor_generation: Option<u64>,
    pub capture_observed: bool,
}
::static_assertions::assert_not_impl_any!(
    DdgiBatchCompletion: ::core::fmt::Debug, ::core::fmt::Display, ::core::marker::Copy,
    ::core::clone::Clone, ::core::default::Default
);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DdgiValidatedPublication {
    work: DdgiScheduledWork,
    field: DdgiFieldIdentity,
    atlas_validation: DdgiAtlasValidationStats,
}

impl DdgiValidatedPublication {
    pub(crate) fn work(self) -> DdgiScheduledWork {
        self.work
    }

    pub(crate) fn field(self) -> DdgiFieldIdentity {
        self.field
    }

    pub(crate) fn atlas_validation(self) -> DdgiAtlasValidationStats {
        self.atlas_validation
    }
}

impl DdgiBatchCompletion {
    pub(crate) fn is_stale(&self) -> bool {
        self.stats.is_none()
    }

    pub(crate) fn validated_publication(&self) -> Option<DdgiValidatedPublication> {
        self.validated_publication
    }
}

mod convergence_evidence {
    use super::{
        DdgiAtlasValidationStats, DdgiConvergenceReason, DdgiValidatedIterationOutcome,
        DdgiValidatedPublication, DDGI_CONVERGENCE_POLICY,
    };

    const TARGET: &str = "re_flora::ddgi_convergence_evidence";
    const DECIMAL_PLACES: usize = 8;

    pub(super) struct Pending(Evidence);
    ::static_assertions::assert_not_impl_any!(
        Pending: ::core::fmt::Debug, ::core::fmt::Display, ::core::marker::Copy,
        ::core::clone::Clone, ::core::default::Default
    );

    pub(super) struct Prepared {
        pub(super) publication: DdgiValidatedPublication,
        pub(super) pending: Pending,
    }

    struct Evidence {
        publication: DdgiValidatedPublication,
        consecutive_below_threshold: u32,
        terminal_reason: Option<DdgiConvergenceReason>,
    }
    ::static_assertions::assert_not_impl_any!(
        Evidence: ::core::fmt::Debug, ::core::fmt::Display, ::core::marker::Copy,
        ::core::clone::Clone, ::core::default::Default
    );

    pub(super) fn prepare(
        outcome: DdgiValidatedIterationOutcome,
        atlas_validation: DdgiAtlasValidationStats,
    ) -> Prepared {
        let (work, field, consecutive_below_threshold, terminal_reason) = match outcome {
            DdgiValidatedIterationOutcome::Published {
                work,
                field,
                consecutive_below_threshold,
            } => (work, field, consecutive_below_threshold, None),
            DdgiValidatedIterationOutcome::Converged {
                work,
                field,
                consecutive_below_threshold,
                reason,
            } => (work, field, consecutive_below_threshold, Some(reason)),
        };
        let publication = DdgiValidatedPublication {
            work,
            field,
            atlas_validation,
        };
        Prepared {
            publication,
            pending: Pending(Evidence {
                publication,
                consecutive_below_threshold,
                terminal_reason,
            }),
        }
    }

    impl Pending {
        fn emit(self) {
            if ::log::log_enabled!(target: TARGET, ::log::Level::Debug) {
                for line in self.0.lines() {
                    ::log::debug!(target: TARGET, "{line}");
                }
            }
        }
    }

    impl Evidence {
        fn lines(self) -> Vec<String> {
            let identity = self.publication.field;
            let field = identity.field();
            let source_field_serial = identity
                .source()
                .map(|source| source.serial().to_string())
                .unwrap_or_else(|| "none".to_owned());
            let stats = self.publication.atlas_validation;
            let validation = format!(
                "[DDGI_CONVERGENCE_EVIDENCE] full-atlas validated field_serial={field_serial} source_field_serial={source_field_serial} geometry_revision={geometry_revision} radiance_revision={radiance_revision} spacing_voxels={spacing_voxels} state={state:?} update_epoch={update_epoch} max_abs_rgb_delta={max_absolute_rgb_delta:.precision$} max_rel_rgb_delta={max_relative_rgb_delta:.precision$} non_finite={non_finite_count} negative_rgb_texels={negative_rgb_texel_count} valid_texels={valid_texel_count} scanned_stored_texels={scanned_stored_texel_count} abs_threshold={absolute_threshold:.precision$} rel_threshold={relative_threshold:.precision$} consecutive_below={consecutive_below_threshold}/{required_consecutive_epochs}",
                field_serial = field.serial(),
                geometry_revision = field.geometry_revision(),
                radiance_revision = field.radiance_revision(),
                spacing_voxels = field.spacing_voxels(),
                state = field.state(),
                update_epoch = field.update_epoch(),
                max_absolute_rgb_delta = stats.max_absolute_rgb_delta,
                max_relative_rgb_delta = stats.max_relative_rgb_delta,
                non_finite_count = stats.non_finite_count,
                negative_rgb_texel_count = stats.negative_rgb_texel_count,
                valid_texel_count = stats.valid_texel_count,
                scanned_stored_texel_count = stats.scanned_stored_texel_count,
                absolute_threshold = DDGI_CONVERGENCE_POLICY.absolute_threshold,
                relative_threshold = DDGI_CONVERGENCE_POLICY.relative_threshold,
                consecutive_below_threshold = self.consecutive_below_threshold,
                required_consecutive_epochs = DDGI_CONVERGENCE_POLICY.consecutive_epochs,
                precision = DECIMAL_PLACES,
            );
            match self.terminal_reason {
                None => vec![validation],
                Some(reason) => vec![
                    validation,
                    format!(
                        "[DDGI_CONVERGENCE_EVIDENCE] terminal field_serial={field_serial} geometry_revision={geometry_revision} radiance_revision={radiance_revision} spacing_voxels={spacing_voxels} update_epoch={update_epoch} reason={reason:?}",
                        field_serial = field.serial(),
                        geometry_revision = field.geometry_revision(),
                        radiance_revision = field.radiance_revision(),
                        spacing_voxels = field.spacing_voxels(),
                        update_epoch = field.update_epoch(),
                    ),
                ],
            }
        }
    }

    impl super::DdgiBatchCompletion {
        pub(crate) fn commit_convergence_evidence(&mut self) {
            if let Some(pending) = self.pending_convergence_evidence.take() {
                pending.emit();
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::super::DdgiFieldIdentity;
        use super::*;
        use crate::ddgi::{DdgiFieldKey, DdgiFieldState, DdgiTransportScheduler, DdgiVolumeGrid};

        fn facts() -> (
            super::super::DdgiScheduledWork,
            DdgiFieldIdentity,
            DdgiAtlasValidationStats,
        ) {
            let mut scheduler = DdgiTransportScheduler::new();
            scheduler.observe_radiance(3);
            scheduler.request_geometry(7, 16);
            let work = scheduler.claim_next().unwrap().unwrap();
            let field = DdgiFieldIdentity::new(
                DdgiFieldKey::new(1, 7, 3, 16, DdgiFieldState::Converging, 0).unwrap(),
                None,
            )
            .unwrap();
            let stats = DdgiAtlasValidationStats {
                max_absolute_rgb_delta: 0.001,
                max_relative_rgb_delta: 0.005,
                valid_texel_count: 64,
                scanned_stored_texel_count: 100,
                ..Default::default()
            };
            (work, field, stats)
        }

        fn assert_wire_contract_matches_runtime_types(contract_source: &str) {
            let contract: toml::Value = toml::from_str(contract_source).unwrap();
            let integers = contract["validation_wire"]["integer_types"]
                .as_table()
                .unwrap();
            let floats = contract["validation_wire"]["float_types"]
                .as_table()
                .unwrap();
            let optional_integers = contract["validation_wire"]["optional_integer_types"]
                .as_table()
                .unwrap();
            assert_eq!(
                contract["validation_wire"]["decimal_places"].as_integer(),
                Some(DECIMAL_PLACES as i64)
            );
            let world_extent = contract["initialization_grid"]["world_extent_voxels"]
                .as_integer()
                .and_then(|value| u32::try_from(value).ok())
                .expect("initialization grid contract must carry a u32 world extent");
            assert_eq!(
                world_extent, 512,
                "acceptance grid must match the production terrain extent"
            );
            for (spacing, probe_count) in [(32, 4_913), (16, 35_937)] {
                let grid = DdgiVolumeGrid::new(glam::UVec3::splat(world_extent), spacing)
                    .expect("acceptance spacing must produce a runtime grid");
                assert_eq!(grid.probe_count(), probe_count);
            }
            let (work, field, stats) = facts();
            let key = field.field();
            let prepared = prepare(
                DdgiValidatedIterationOutcome::Published {
                    work,
                    field,
                    consecutive_below_threshold: 1,
                },
                stats,
            );

            let mut integer_count = 0;
            let mut optional_integer_count = 0;
            let mut float_count = 0;
            macro_rules! assert_wire_row {
                ($table:ident, $count:ident, $field:literal : $rust_type:ty = $value:expr) => {{
                    let _: $rust_type = $value;
                    assert_eq!(
                        $table[$field].as_str(),
                        Some(stringify!($rust_type)),
                        "wire type drift for {}",
                        $field
                    );
                    $count += 1;
                }};
            }
            macro_rules! assert_optional_wire_row {
                ($table:ident, $count:ident, $field:literal : $rust_type:ty = $value:expr) => {{
                    let _: Option<$rust_type> = $value;
                    assert_eq!(
                        $table[$field].as_str(),
                        Some(stringify!($rust_type)),
                        "wire type drift for {}",
                        $field
                    );
                    $count += 1;
                }};
            }
            assert_wire_row!(integers, integer_count, "field_serial": u64 = key.serial());
            assert_wire_row!(integers, integer_count, "geometry_revision": u32 = key.geometry_revision());
            assert_wire_row!(integers, integer_count, "radiance_revision": u32 = key.radiance_revision());
            assert_wire_row!(integers, integer_count, "spacing_voxels": u32 = key.spacing_voxels());
            assert_wire_row!(integers, integer_count, "update_epoch": u32 = key.update_epoch());
            assert_wire_row!(integers, integer_count, "nonfinite_count": u32 = stats.non_finite_count);
            assert_wire_row!(integers, integer_count, "negative_rgb_texel_count": u32 = stats.negative_rgb_texel_count);
            assert_wire_row!(integers, integer_count, "valid_texel_count": u32 = stats.valid_texel_count);
            assert_wire_row!(integers, integer_count, "scanned_stored_texel_count": u32 = stats.scanned_stored_texel_count);
            assert_wire_row!(integers, integer_count, "consecutive_below_threshold": u32 = prepared.pending.0.consecutive_below_threshold);
            assert_wire_row!(integers, integer_count, "required_consecutive_epochs": u32 = DDGI_CONVERGENCE_POLICY.consecutive_epochs);
            assert_optional_wire_row!(optional_integers, optional_integer_count, "source_field_serial": u64 = prepared.pending.0.publication.field.source().map(|source| source.serial()));
            assert_wire_row!(floats, float_count, "max_absolute_rgb_delta": f32 = stats.max_absolute_rgb_delta);
            assert_wire_row!(floats, float_count, "max_relative_rgb_delta": f32 = stats.max_relative_rgb_delta);
            assert_wire_row!(floats, float_count, "absolute_threshold": f32 = DDGI_CONVERGENCE_POLICY.absolute_threshold);
            assert_wire_row!(floats, float_count, "relative_threshold": f32 = DDGI_CONVERGENCE_POLICY.relative_threshold);
            assert_eq!(integers.len(), integer_count);
            assert_eq!(optional_integers.len(), optional_integer_count);
            assert_eq!(floats.len(), float_count);
        }

        #[test]
        fn private_evidence_lines_preserve_exact_count_content_and_order() {
            assert_wire_contract_matches_runtime_types(include_str!(
                "../../config/ddgi_convergence_acceptance.toml"
            ));
            let (work, field, stats) = facts();
            let published = prepare(
                DdgiValidatedIterationOutcome::Published {
                    work,
                    field,
                    consecutive_below_threshold: 0,
                },
                stats,
            );
            assert_eq!(published.publication.field(), field);
            assert_eq!(published.publication.atlas_validation(), stats);
            assert_eq!(
                published.pending.0.lines(),
                vec!["[DDGI_CONVERGENCE_EVIDENCE] full-atlas validated field_serial=1 source_field_serial=none geometry_revision=7 radiance_revision=3 spacing_voxels=16 state=Converging update_epoch=0 max_abs_rgb_delta=0.00100000 max_rel_rgb_delta=0.00500000 non_finite=0 negative_rgb_texels=0 valid_texels=64 scanned_stored_texels=100 abs_threshold=0.00250000 rel_threshold=0.02000000 consecutive_below=0/2"]
            );

            let sourced_field = DdgiFieldIdentity::new(
                DdgiFieldKey::new(2, 7, 3, 16, DdgiFieldState::Converging, 1).unwrap(),
                Some(field.field()),
            )
            .unwrap();
            let sourced = prepare(
                DdgiValidatedIterationOutcome::Published {
                    work,
                    field: sourced_field,
                    consecutive_below_threshold: 1,
                },
                stats,
            );
            assert_eq!(
                sourced.pending.0.lines(),
                vec!["[DDGI_CONVERGENCE_EVIDENCE] full-atlas validated field_serial=2 source_field_serial=1 geometry_revision=7 radiance_revision=3 spacing_voxels=16 state=Converging update_epoch=1 max_abs_rgb_delta=0.00100000 max_rel_rgb_delta=0.00500000 non_finite=0 negative_rgb_texels=0 valid_texels=64 scanned_stored_texels=100 abs_threshold=0.00250000 rel_threshold=0.02000000 consecutive_below=1/2"]
            );

            let mut converged_scheduler = DdgiTransportScheduler::new();
            converged_scheduler.observe_radiance(3);
            converged_scheduler.request_geometry(17, 16);
            let converged_work = converged_scheduler.claim_next().unwrap().unwrap();
            let converged_source =
                DdgiFieldKey::new(8, 17, 3, 16, DdgiFieldState::Converging, 6).unwrap();
            let converged_field = DdgiFieldIdentity::new(
                DdgiFieldKey::new(9, 17, 3, 16, DdgiFieldState::Converged, 7).unwrap(),
                Some(converged_source),
            )
            .unwrap();
            let converged = prepare(
                DdgiValidatedIterationOutcome::Converged {
                    work: converged_work,
                    field: converged_field,
                    consecutive_below_threshold: 2,
                    reason: DdgiConvergenceReason::Threshold,
                },
                stats,
            );
            assert_eq!(
                converged.pending.0.lines(),
                vec![
                    "[DDGI_CONVERGENCE_EVIDENCE] full-atlas validated field_serial=9 source_field_serial=8 geometry_revision=17 radiance_revision=3 spacing_voxels=16 state=Converged update_epoch=7 max_abs_rgb_delta=0.00100000 max_rel_rgb_delta=0.00500000 non_finite=0 negative_rgb_texels=0 valid_texels=64 scanned_stored_texels=100 abs_threshold=0.00250000 rel_threshold=0.02000000 consecutive_below=2/2",
                    "[DDGI_CONVERGENCE_EVIDENCE] terminal field_serial=9 geometry_revision=17 radiance_revision=3 spacing_voxels=16 update_epoch=7 reason=Threshold",
                ]
            );
        }

        #[test]
        fn validation_wire_labels_bind_to_their_distinct_runtime_facts() {
            let (work, field, mut stats) = facts();
            stats.non_finite_count = 7;
            stats.negative_rgb_texel_count = 11;
            let prepared = prepare(
                DdgiValidatedIterationOutcome::Published {
                    work,
                    field,
                    consecutive_below_threshold: 0,
                },
                stats,
            );

            assert!(prepared.pending.0.lines()[0]
                .contains("non_finite=7 negative_rgb_texels=11 valid_texels=64"));
        }

        #[test]
        #[should_panic(expected = "wire type drift for geometry_revision")]
        fn validation_wire_type_drift_is_rejected_by_its_typed_row() {
            let contract = include_str!("../../config/ddgi_convergence_acceptance.toml")
                .replace("geometry_revision = \"u32\"", "geometry_revision = \"u64\"");
            assert_wire_contract_matches_runtime_types(&contract);
        }

        #[test]
        #[should_panic(expected = "wire type drift for source_field_serial")]
        fn optional_source_wire_type_drift_is_rejected_by_its_typed_row() {
            let contract = include_str!("../../config/ddgi_convergence_acceptance.toml").replace(
                "source_field_serial = \"u64\"",
                "source_field_serial = \"u32\"",
            );
            assert_wire_contract_matches_runtime_types(&contract);
        }
    }
}

/// Result of one runtime-owned attempt to publish a complete Staging Volume.
pub(crate) enum DdgiVolumePublishOutcome {
    Idle,
    DiscardedObsolete(DdgiBuildToken),
    Published {
        token: DdgiBuildToken,
        retired_active: DdgiVolume,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PublicationDisposition<T> {
    Idle,
    Discard(T),
    Publish(T),
}

#[derive(Debug)]
enum PublicationTransactionOutcome<T, R> {
    Idle,
    Discarded(T),
    Published { candidate: T, committed: R },
}

fn execute_publication_transaction<S, T: Copy, P, R>(
    state: &mut S,
    disposition: PublicationDisposition<T>,
    publish: impl FnOnce(&S, T) -> Result<P>,
    commit: impl FnOnce(&mut S, T, P) -> R,
) -> Result<PublicationTransactionOutcome<T, R>> {
    match disposition {
        PublicationDisposition::Idle => Ok(PublicationTransactionOutcome::Idle),
        PublicationDisposition::Discard(candidate) => {
            Ok(PublicationTransactionOutcome::Discarded(candidate))
        }
        PublicationDisposition::Publish(candidate) => {
            let published = publish(state, candidate)?;
            Ok(PublicationTransactionOutcome::Published {
                candidate,
                committed: commit(state, candidate, published),
            })
        }
    }
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DdgiLightingObservation {
    pub transport: EnvironmentLightingState,
    pub transport_published: bool,
    pub coalesced_live_revisions: u64,
}

impl DdgiLightingObservation {
    pub fn revision_lag(self, authored: AuthoredEnvironmentLightingFact) -> u64 {
        authored
            .revision
            .saturating_sub(self.transport.source_live_revision())
    }

    pub fn transport_age(self, now: Duration) -> Duration {
        now.saturating_sub(self.transport.published_at())
    }
}

impl DdgiRuntimeWork {
    pub(crate) fn scheduled(self) -> DdgiScheduledWork {
        self.scheduled
    }

    pub(crate) fn authored_lighting(self) -> EnvironmentLightingState {
        self.authored_lighting
    }

    #[cfg(test)]
    pub(crate) fn radiance_history_policy(self) -> Option<DdgiRadianceHistoryPolicy> {
        self.radiance_history_policy
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
    latest_authored_fact: Option<AuthoredEnvironmentLightingFact>,
    pending_authored_fact: Option<AuthoredEnvironmentLightingFact>,
    current_transport_revision: u32,
    latest_transport_lighting: Option<EnvironmentLightingState>,
    in_flight_authored_lighting: Option<EnvironmentLightingState>,
    published_authored_lighting: Option<EnvironmentLightingState>,
    resident_active_field: Option<DdgiFieldIdentity>,
    resident_active_authored_lighting: Option<EnvironmentLightingState>,
    completed_staging_authored_lighting:
        Option<(DdgiBuildToken, DdgiFieldIdentity, EnvironmentLightingState)>,
    coalesced_live_revisions: u64,
    coalesced_radiance_revisions: u64,
    camera_probe_priority: Option<UAabb3>,
    lighting_impact_probe_priority: Option<(u32, UAabb3)>,
    capture_enabled: bool,
    capture_target: DdgiCaptureTarget,
    capture_batch_order: DdgiBatchOrder,
    capture_checkpoint: Option<DdgiCaptureCheckpoint>,
    resident_active_capture_checkpoint: Option<DdgiCaptureCheckpoint>,
    filter_evidence_accumulator: Option<DdgiFilterEpochAccumulator>,
    next_frame_work_serial: u64,
    pending_frame_work: Option<DdgiFrameWork>,
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
            latest_authored_fact: None,
            pending_authored_fact: None,
            current_transport_revision: 0,
            latest_transport_lighting: None,
            in_flight_authored_lighting: None,
            published_authored_lighting: None,
            resident_active_field: None,
            resident_active_authored_lighting: None,
            completed_staging_authored_lighting: None,
            coalesced_live_revisions: 0,
            coalesced_radiance_revisions: 0,
            camera_probe_priority: None,
            lighting_impact_probe_priority: None,
            capture_enabled: false,
            capture_target: DdgiCaptureTarget::default(),
            capture_batch_order: DdgiBatchOrder::default(),
            capture_checkpoint: None,
            resident_active_capture_checkpoint: None,
            filter_evidence_accumulator: None,
            next_frame_work_serial: 1,
            pending_frame_work: None,
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

    /// Publishes one complete Staging candidate as a synchronous transaction.
    ///
    /// Idle and obsolete candidates never call `publish_consumers`. A promotable candidate is
    /// fully checked before the closure receives its private Volume. If the closure fails, no
    /// physical or logical runtime state changes. Once the closure succeeds, the preflighted swap
    /// and logical promotion are fail-stop operations.
    pub(crate) fn publish_ready_volume(
        &mut self,
        publish_consumers: impl FnOnce(DdgiConsumerResources<'_>) -> Result<()>,
    ) -> Result<DdgiVolumePublishOutcome> {
        let status = self.volumes().status();
        let disposition = if !status.staging_is_ready() {
            PublicationDisposition::Idle
        } else {
            let token = status
                .staging()
                .and_then(|staging| staging.build_token)
                .expect("every complete DDGI staging Volume must carry its build token");
            if self.terrain_refresh.token_can_promote(token) {
                PublicationDisposition::Publish(token)
            } else {
                PublicationDisposition::Discard(token)
            }
        };
        let transaction = execute_publication_transaction(
            self,
            disposition,
            |runtime, token| {
                let permit = runtime.volumes().preflight_staging_promotion(token)?;
                let publication = permit.publication();
                let (lighting_token, lighting_field, lighting) = runtime
                    .completed_staging_authored_lighting
                    .context("promotable DDGI staging Volume lost its authored lighting")?;
                assert_eq!(
                    (lighting_token, lighting_field, lighting.revision()),
                    (
                        permit.token(),
                        publication.field(),
                        publication.field().field().radiance_revision(),
                    ),
                    "DDGI staging physical publication and authored lighting diverged"
                );
                let resources = runtime.volumes().staging_consumer_resources(&permit);
                publish_consumers(resources)?;
                Ok(permit)
            },
            |runtime, token, permit| {
                let publication = permit.publication();
                let retired_active = runtime.volumes_mut().promote_staging(permit);
                runtime.commit_promoted(token, publication.field());
                retired_active
            },
        )?;
        match transaction {
            PublicationTransactionOutcome::Idle => Ok(DdgiVolumePublishOutcome::Idle),
            PublicationTransactionOutcome::Discarded(token) => {
                anyhow::ensure!(
                    self.finish_obsolete_volume_build(token),
                    "completed obsolete DDGI staging token must release the single update slot"
                );
                Ok(DdgiVolumePublishOutcome::DiscardedObsolete(token))
            }
            PublicationTransactionOutcome::Published {
                candidate: token,
                committed: retired_active,
            } => Ok(DdgiVolumePublishOutcome::Published {
                token,
                retired_active,
            }),
        }
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
        self.filter_evidence_accumulator = None;
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

    /// Observes the latest normalized fact and freezes DDGI transport only when its own cadence or
    /// discontinuity policy requires it. Immediate consumers never read this transport snapshot.
    pub(crate) fn observe_authored_lighting(
        &mut self,
        authored: AuthoredEnvironmentLightingFact,
    ) -> DdgiLightingObservation {
        assert_ne!(
            authored.revision, 0,
            "Authored Environment Lighting live revisions must be nonzero"
        );
        let authored_changed = self
            .latest_authored_fact
            .is_none_or(|current| current.revision != authored.revision);
        if let Some(current) = self.latest_authored_fact {
            if current.revision == authored.revision {
                authored.assert_same_identity(current);
            }
        }
        self.latest_authored_fact = Some(authored);

        let mut transport_published = false;
        if self.latest_transport_lighting.is_none() {
            self.freeze_transport(
                authored,
                DdgiRadianceChange {
                    reason: DdgiRadianceChangeReason::Initial,
                    delta: DdgiRadianceDelta::default(),
                },
            );
            transport_published = true;
        } else if authored_changed {
            let current = self
                .latest_transport_lighting
                .expect("initialized DDGI lighting must retain its current transport");
            if let Some(change) = authored.change_from_transport(current) {
                if change.resets_irradiance_history() {
                    if self.pending_authored_fact.take().is_some() {
                        self.coalesced_live_revisions =
                            self.coalesced_live_revisions.saturating_add(1);
                    }
                    self.freeze_transport(authored, change);
                    transport_published = true;
                } else if self.pending_authored_fact.replace(authored).is_some() {
                    self.coalesced_live_revisions = self.coalesced_live_revisions.saturating_add(1);
                }
            } else {
                if self.pending_authored_fact.take().is_some() {
                    self.coalesced_live_revisions = self.coalesced_live_revisions.saturating_add(1);
                }
            }
        }

        let publication_due = self.latest_transport_lighting.is_some_and(|transport| {
            authored
                .observed_at
                .saturating_sub(transport.published_at())
                >= DDGI_TRANSPORT_MIN_PUBLICATION_INTERVAL
        });
        if !transport_published && publication_due && self.pending_authored_fact.is_some() {
            let current = self
                .latest_transport_lighting
                .expect("pending DDGI lighting requires a current transport");
            let change = authored
                .change_from_transport(current)
                .expect("pending DDGI lighting must differ from the current transport identity");
            self.freeze_transport(authored, change);
            transport_published = true;
        }

        DdgiLightingObservation {
            transport: self
                .latest_transport_lighting
                .expect("initial authored observation must freeze DDGI transport"),
            transport_published,
            coalesced_live_revisions: self.coalesced_live_revisions,
        }
    }

    fn freeze_transport(
        &mut self,
        authored: AuthoredEnvironmentLightingFact,
        change: DdgiRadianceChange,
    ) {
        self.current_transport_revision = self.current_transport_revision.wrapping_add(1).max(1);
        let lighting =
            EnvironmentLightingState::freeze(self.current_transport_revision, authored, change);
        self.pending_authored_fact = None;

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
            && previous_latest != Some(lighting.revision())
            && previous_latest != in_flight_revision
            && previous_latest != published_revision
        {
            self.coalesced_radiance_revisions = self.coalesced_radiance_revisions.saturating_add(1);
        }
        self.latest_transport_lighting = Some(lighting);
        self.transport_scheduler
            .observe_radiance(lighting.revision());
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

    /// Installs the physical allocation authorized by [`Self::claim_volume_build`] and performs
    /// the complete builder-stage initialization transaction.
    ///
    /// Allocation remains concrete Vulkan work outside the runtime. The caller hands the finished
    /// allocation back here and cannot assign tokens, select Active/Staging, or request stages.
    pub(crate) fn install_volume_build(
        &mut self,
        build: DdgiRuntimeVolumeBuild,
        staging: Option<DdgiVolume>,
    ) -> Result<Option<DdgiVolume>> {
        match build.target {
            DdgiRuntimeVolumeTarget::Active => {
                anyhow::ensure!(
                    staging.is_none(),
                    "initial DDGI build must use the installed Active allocation"
                );
                let builder = self.volumes_mut().builder_mut();
                anyhow::ensure!(
                    builder.status().build_token.is_none(),
                    "initial DDGI Volume must not already have a build token"
                );
                builder.assign_build_token(build.token);
                anyhow::ensure!(
                    builder.request_initialization(build.token.terrain_revision()),
                    "initial DDGI Volume must accept its authoritative terrain revision"
                );
                Ok(None)
            }
            DdgiRuntimeVolumeTarget::Staging => {
                let mut staging =
                    staging.context("staging DDGI build requires a new allocation")?;
                staging.assign_build_token(build.token);
                anyhow::ensure!(
                    staging.request_initialization(build.token.terrain_revision()),
                    "staging DDGI Volume must accept its authoritative terrain revision"
                );
                Ok(self.volumes_mut().prepare_staging(staging))
            }
        }
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
                lighting.revision(),
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
            .latest_transport_lighting
            .expect("DDGI transport work requires an Authored Environment Lighting observation");
        assert_eq!(
            authored_lighting.revision(),
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
                .find(|lighting| lighting.revision() == source_revision)
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

    /// Claims and installs the next logical transport epoch into the physical builder.
    pub(crate) fn begin_next_transport_work(&mut self) -> Result<Option<DdgiRuntimeWork>> {
        let Some(work) = self.claim_transport_work() else {
            return Ok(None);
        };
        let destination = work.scheduled.destination().field();
        let builder = self.volumes_mut().builder_mut();
        anyhow::ensure!(
            builder.status().grid.spacing_voxels() == destination.spacing_voxels(),
            "DDGI scheduler selected spacing {} for builder spacing {}",
            destination.spacing_voxels(),
            builder.status().grid.spacing_voxels(),
        );
        builder.begin_scheduled_work(
            work.scheduled,
            work.local_refresh_voxel_bound,
            work.radiance_history_policy,
            work.probe_priority,
        )?;
        Ok(Some(work))
    }

    #[cfg(test)]
    pub(crate) fn complete_transport_work(
        &mut self,
        work: DdgiScheduledWork,
        published: DdgiFieldIdentity,
        build_token: DdgiBuildToken,
    ) -> Result<DdgiFieldIdentity, DdgiSchedulerError> {
        let permit = self
            .transport_scheduler
            .preflight_completion(work, published)?;
        Ok(self.commit_transport_work(work, build_token, permit))
    }

    fn commit_transport_work(
        &mut self,
        work: DdgiScheduledWork,
        build_token: DdgiBuildToken,
        permit: DdgiSchedulerCompletionPermit,
    ) -> DdgiFieldIdentity {
        let lighting = self
            .in_flight_authored_lighting
            .take()
            .expect("completed DDGI work must retain its immutable authored lighting");
        assert_eq!(
            lighting.revision(),
            work.destination().field().radiance_revision(),
            "completed DDGI work and authored lighting revision diverged",
        );
        let published = self.transport_scheduler.commit_completion(permit);
        self.published_authored_lighting = Some(lighting);
        if self.active_build_token == Some(build_token) {
            self.resident_active_field = Some(published);
            self.resident_active_authored_lighting = Some(lighting);
            self.active_ready = true;
        } else {
            self.completed_staging_authored_lighting = Some((build_token, published, lighting));
        }
        published
    }

    #[cfg(test)]
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

    fn commit_promoted(&mut self, token: DdgiBuildToken, publication: DdgiFieldIdentity) {
        assert!(
            self.terrain_refresh.token_can_promote(token),
            "preflighted DDGI token lost coordinator authority"
        );
        assert!(self.terrain_refresh.mark_promoted(token));
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
        assert_eq!(
            field, publication,
            "promoted DDGI physical publication and scheduler field diverged"
        );
        self.resident_active_field = Some(field);
        self.resident_active_authored_lighting = Some(lighting);
        if self
            .capture_checkpoint
            .is_some_and(|checkpoint| checkpoint.build_token == token)
        {
            self.resident_active_capture_checkpoint = self.capture_checkpoint;
        }
    }

    pub(crate) fn status(&self) -> DdgiRuntimeStatus {
        self.status_from_physical(self.volumes().status())
    }

    fn status_from_physical(&self, volumes: DdgiStatus) -> DdgiRuntimeStatus {
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
                    active.publication.map(|publication| publication.field())
                        == Some(checkpoint.field)
                }
                DdgiCapturePublication::Unpublished => {
                    active.publication.is_none() && active.complete_field == Some(checkpoint.field)
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
        filter_proof: Option<DdgiFilterEpochProof>,
        publication: DdgiCapturePublication,
    ) -> bool {
        if !self.capture_enabled || !self.capture_target.matches_checkpoint(field, publication) {
            return false;
        }
        let checkpoint = DdgiCaptureCheckpoint {
            build_token,
            field,
            validation,
            filter_proof,
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

    pub(crate) fn lighting_revision_lag(&self) -> u64 {
        self.latest_authored_fact.map_or(0, |authored| {
            authored.revision.saturating_sub(
                self.latest_transport_lighting
                    .map_or(0, |transport| transport.source_live_revision()),
            )
        })
    }

    pub(crate) fn coalesced_live_revisions(&self) -> u64 {
        self.coalesced_live_revisions
    }

    pub(crate) fn lighting_diagnostics(&self) -> DdgiLightingDiagnostics {
        let in_flight_revision = self
            .transport_scheduler
            .in_flight()
            .map(|work| work.destination().field().radiance_revision());
        let authored_in_flight_revision = self
            .in_flight_authored_lighting
            .map(|lighting| lighting.revision());
        DdgiLightingDiagnostics {
            latest_transport_revision: self.transport_scheduler.latest_radiance_revision(),
            latest_source_live_revision: self
                .latest_transport_lighting
                .map(|lighting| lighting.source_live_revision()),
            scheduler_published_revision: self
                .transport_scheduler
                .published()
                .map(|field| field.field().radiance_revision()),
            in_flight_revision,
            coalesced_revisions: self.coalesced_radiance_revisions,
            has_mixed_in_flight_revision: in_flight_revision != authored_in_flight_revision,
        }
    }

    pub(crate) fn latest_transport_lighting(&self) -> Option<EnvironmentLightingState> {
        self.latest_transport_lighting
    }

    #[cfg(test)]
    pub(crate) fn in_flight_authored_lighting(&self) -> Option<EnvironmentLightingState> {
        self.in_flight_authored_lighting
    }

    /// Latches immutable lighting and selects the complete physical DDGI sequence once per frame.
    pub(crate) fn begin_frame_work(&mut self) -> Result<DdgiFrameWork> {
        anyhow::ensure!(
            self.pending_frame_work.is_none(),
            "DDGI frame work must be settled before another frame begins"
        );
        let builder = self.volumes().builder();
        let lighting_to_latch = self
            .in_flight_authored_lighting
            .filter(|lighting| builder.should_latch_radiance_snapshot(lighting.revision()));
        if let Some(lighting) = lighting_to_latch {
            self.volumes_mut()
                .builder_mut()
                .latch_radiance_snapshot(lighting.revision(), lighting.snapshot())?;
        }

        let builder = self.volumes().builder();
        let ray_batch = builder.projected_ray_batch_after_preparation();
        let plan = DdgiFramePlan {
            global_sky_needs_update: builder.global_sky_needs_update(),
            relocation_terrain_revision: builder.pending_relocation_terrain_revision(),
            visibility_preservation_needed: builder.visibility_preservation_needed(),
            iteration_will_complete: ray_batch
                .is_some_and(|batch| builder.projected_iteration_will_complete(batch)),
            ray_batch,
        };
        let work = DdgiFrameWork {
            serial: self.next_frame_work_serial,
            plan,
        };
        self.next_frame_work_serial = self.next_frame_work_serial.saturating_add(1);
        self.pending_frame_work = Some(work);
        Ok(work)
    }

    /// Settles all physical pass completions, or the single encoding failure, through one seam.
    pub(crate) fn finish_frame_work(
        &mut self,
        work: DdgiFrameWork,
        outcome: Result<()>,
    ) -> Result<()> {
        anyhow::ensure!(
            self.pending_frame_work == Some(work),
            "stale or out-of-order DDGI frame work completion"
        );
        self.pending_frame_work = None;
        outcome?;

        let builder = self.volumes_mut().builder_mut();
        let plan = work.plan;
        if plan.global_sky_needs_update {
            let revision = builder
                .status()
                .radiance_revision
                .expect("a DDGI global-sky pass requires a latched radiance snapshot");
            builder.mark_global_sky_ready(revision)?;
        }
        if let Some(terrain_revision) = plan.relocation_terrain_revision {
            builder.mark_relocated(terrain_revision)?;
        }
        if plan.visibility_preservation_needed {
            builder.mark_visibility_preserved();
        }
        if let Some(batch) = plan.ray_batch {
            builder.mark_ray_batch_ready(batch);
            builder.mark_ray_batch_filtered(batch);
        }
        Ok(())
    }

    /// Reconciles trace and optional full-atlas feedback as one physical completion transaction.
    /// The caller observes the returned typed evidence but cannot advance Volume or scheduler state.
    pub(crate) fn complete_pending_batch(
        &mut self,
        batch: DdgiRayBatch,
        filter_configuration: DdgiFilterConfigurationIdentity,
        publish_consumers: impl FnOnce(DdgiConsumerResources<'_>) -> Result<u64>,
    ) -> Result<DdgiBatchCompletion> {
        let before = self.volumes().builder().status();
        if !self.volumes().builder().pending_trace_stats_batch_is(batch) {
            return Ok(DdgiBatchCompletion {
                stats: None,
                radiance_snapshot: None,
                probe_count: before.grid.probe_count(),
                build_token: before.build_token,
                status: before.into(),
                validated_publication: None,
                pending_convergence_evidence: None,
                consumer_descriptor_generation: None,
                capture_observed: false,
            });
        }

        let stats = self
            .volumes()
            .builder()
            .update_trace_stats_from_readback()?;
        anyhow::ensure!(
            stats.ray_records == batch.probe_count * DDGI_RAYS_PER_PROBE,
            "DDGI trace produced {} records for a {}x{} batch",
            stats.ray_records,
            batch.probe_count,
            DDGI_RAYS_PER_PROBE,
        );
        anyhow::ensure!(
            stats.non_finite_records == 0,
            "DDGI trace produced non-finite records: {stats:?}"
        );
        let filter_batch_evidence = stats.filter_batch_evidence(batch, self.capture_enabled)?;
        if let Some(evidence) = filter_batch_evidence {
            let replace_accumulator = self
                .filter_evidence_accumulator
                .as_ref()
                .is_none_or(|accumulator| accumulator.field() != batch.logical());
            if replace_accumulator {
                self.filter_evidence_accumulator = Some(DdgiFilterEpochAccumulator::new(
                    batch.logical(),
                    filter_configuration,
                )?);
            }
            self.filter_evidence_accumulator
                .as_mut()
                .expect("capture-enabled DDGI batch must retain its epoch accumulator")
                .observe(batch, filter_configuration, evidence)?;
        }
        let radiance_snapshot = self
            .volumes()
            .builder()
            .radiance_snapshot()
            .context("DDGI trace-stat readback lost its immutable radiance snapshot")?;
        let outcome = self
            .volumes_mut()
            .builder_mut()
            .mark_trace_stats_verified(batch)?;
        let filter_epoch_proof = if matches!(
            outcome,
            DdgiVerifiedBatchOutcome::AwaitingAtlasValidation(_)
        ) && self.capture_enabled
        {
            Some(
                self.filter_evidence_accumulator
                    .take()
                    .context("completed DDGI capture epoch lost filter evidence")?
                    .finish()?,
            )
        } else {
            None
        };
        let mut validated_publication = None;
        let mut pending_convergence_evidence = None;
        let mut consumer_descriptor_generation = None;
        let mut capture_observed = false;
        if let DdgiVerifiedBatchOutcome::AwaitingAtlasValidation(identity) = outcome {
            let stats = self
                .volumes()
                .builder()
                .update_atlas_validation_from_readback()?;
            let volume_permit = self.volumes().builder().preflight_atlas_publication(
                identity,
                stats,
                DDGI_CONVERGENCE_POLICY,
            )?;
            let publication = volume_permit.publication();
            let classified = publication.field();
            let work = volume_permit.work();
            let status_work = self
                .volumes()
                .builder()
                .status()
                .scheduled_work
                .context("validated DDGI epoch must retain scheduled work")?;
            anyhow::ensure!(
                status_work == work,
                "DDGI physical publication permit lost its scheduled work"
            );
            let scheduler_permit = self
                .transport_scheduler
                .preflight_completion(work, classified)
                .map_err(|error| {
                    anyhow::anyhow!(
                        "DDGI scheduler rejected completion before publication: {error:?}"
                    )
                })?;
            let build_token = before
                .build_token
                .context("validated DDGI field has no volume build token")?;
            let observed_capture = if self.volumes().builder_is_active() {
                assert_eq!(
                    publication.generation().build_token(),
                    build_token,
                    "DDGI publication permit must retain the active candidate token"
                );
                let resources = self
                    .volumes()
                    .builder()
                    .candidate_consumer_resources(&volume_permit);
                let generation = publish_consumers(resources)?;
                let validated = self
                    .volumes_mut()
                    .builder_mut()
                    .commit_atlas_publication(volume_permit);
                let prepared = convergence_evidence::prepare(validated, stats);
                let publication = prepared.publication;
                let field = publication.field();
                assert_eq!(field, classified);
                assert_eq!(publication.work(), work);
                self.commit_transport_work(work, build_token, scheduler_permit);
                let capture_observed = self.observe_capture_checkpoint(
                    build_token,
                    field,
                    stats,
                    filter_epoch_proof,
                    DdgiCapturePublication::Published,
                );
                consumer_descriptor_generation = Some(generation);
                validated_publication = Some(prepared.publication);
                pending_convergence_evidence = Some(prepared.pending);
                capture_observed
            } else {
                let validated = self
                    .volumes_mut()
                    .builder_mut()
                    .commit_atlas_publication(volume_permit);
                let prepared = convergence_evidence::prepare(validated, stats);
                let publication = prepared.publication;
                let validated_work = publication.work();
                let field = publication.field();
                assert_eq!(
                    field, classified,
                    "private DDGI atlas classification changed during publication"
                );
                assert_eq!(
                    validated_work, work,
                    "private DDGI validated work changed during publication"
                );
                self.commit_transport_work(work, build_token, scheduler_permit);
                let capture_observed = self.observe_capture_checkpoint(
                    build_token,
                    field,
                    stats,
                    filter_epoch_proof,
                    DdgiCapturePublication::Published,
                );
                validated_publication = Some(publication);
                pending_convergence_evidence = Some(prepared.pending);
                capture_observed
            };
            capture_observed = observed_capture;
        }

        let after = self.volumes().builder().status();
        Ok(DdgiBatchCompletion {
            stats: Some(stats),
            radiance_snapshot: Some(radiance_snapshot),
            probe_count: after.grid.probe_count(),
            build_token: after.build_token,
            status: after.into(),
            validated_publication,
            pending_convergence_evidence,
            consumer_descriptor_generation,
            capture_observed,
        })
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
    pub publication: Option<super::DdgiFieldPublication>,
    pub building_field: Option<DdgiFieldIdentity>,
    pub last_atlas_validation: Option<DdgiAtlasValidationStats>,
    pub global_sky_revision: u32,
    pub radiance_revision: Option<u32>,
    pub relocated_terrain_revision: Option<u32>,
    pub filtered_probe_count: u32,
    pub probe_priority: Option<DdgiProbePriority>,
}

impl DdgiRuntimeVolumeStatus {
    pub fn is_ready(self) -> bool {
        self.publication.is_some()
    }

    pub fn published_field(self) -> Option<DdgiFieldIdentity> {
        self.publication.map(super::DdgiFieldPublication::field)
    }
}

impl From<DdgiVolumeStatus> for DdgiRuntimeVolumeStatus {
    fn from(status: DdgiVolumeStatus) -> Self {
        if let Some(publication) = status.publication {
            let generation = publication.generation();
            assert_eq!(
                status.build_token,
                Some(generation.build_token()),
                "DDGI runtime publication escaped its physical Volume build"
            );
            assert_eq!(
                generation.epoch_zero_field().field().update_epoch(),
                0,
                "DDGI runtime publication lost its epoch-zero generation root"
            );
        }
        Self {
            build_token: status.build_token,
            grid: status.grid,
            resource_bytes: status.resource_bytes,
            stage: status.stage,
            target_work: status.scheduled_work.map(Into::into),
            complete_field: status.complete_field,
            publication: status.publication,
            building_field: status.building_field,
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
#[allow(clippy::large_enum_variant)]
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
            format_ddgi_field(active.published_field()),
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
            format_ddgi_field(staging.published_field()),
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
    use crate::environment_lighting::{
        AuthoredEnvironmentLighting, AuthoredEnvironmentLightingFact,
        AuthoredEnvironmentLightingInput, DdgiRadianceSnapshot, DdgiVoxelPaletteSnapshot,
    };
    use crate::geom::UAabb3;
    use crate::lighting::{
        LocalLight, LocalLightBudget, LocalLightGpuPayload, LocalLightGpuSnapshot,
        LocalLightRegistry, PointLight,
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

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct FakePublicationState {
        active: &'static str,
        staging: &'static str,
        logical: &'static str,
        commit_calls: usize,
    }

    fn fake_publication_state() -> FakePublicationState {
        FakePublicationState {
            active: "old-active",
            staging: "ready-staging",
            logical: "old-logical",
            commit_calls: 0,
        }
    }

    #[test]
    fn publication_transaction_idle_and_discard_call_neither_phase() {
        let token = DdgiBuildToken::for_test(7, 3, 16, DdgiBuildKind::Terrain);
        for disposition in [
            PublicationDisposition::Idle,
            PublicationDisposition::Discard(token),
        ] {
            let mut state = fake_publication_state();
            let publish_calls = std::cell::Cell::new(0);
            execute_publication_transaction(
                &mut state,
                disposition,
                |_, _| {
                    publish_calls.set(publish_calls.get() + 1);
                    Ok(())
                },
                |state, _, ()| state.commit_calls += 1,
            )
            .unwrap();
            assert_eq!(publish_calls.get(), 0);
            assert_eq!(state.commit_calls, 0);
        }
    }

    #[test]
    fn publication_transaction_error_preserves_state_and_skips_commit() {
        let token = DdgiBuildToken::for_test(7, 3, 16, DdgiBuildKind::Terrain);
        let mut state = fake_publication_state();
        let before = state.clone();
        let publish_calls = std::cell::Cell::new(0);
        let error = execute_publication_transaction(
            &mut state,
            PublicationDisposition::Publish(token),
            |state, _| -> Result<()> {
                assert_eq!(state.active, "old-active");
                publish_calls.set(publish_calls.get() + 1);
                anyhow::bail!("injected consumer preflight failure")
            },
            |state, _, ()| state.commit_calls += 1,
        )
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("injected consumer preflight failure"));
        assert_eq!(state, before);
        assert_eq!(state.commit_calls, 0);
        assert_eq!(publish_calls.get(), 1);
    }

    #[test]
    fn publication_transaction_publishes_once_before_commit_and_returns_old_active() {
        let token = DdgiBuildToken::for_test(7, 3, 16, DdgiBuildKind::Terrain);
        let mut state = fake_publication_state();
        let publish_calls = std::cell::Cell::new(0);
        let generation = std::cell::Cell::new(41_u64);
        let events = std::cell::RefCell::new(Vec::new());
        let outcome = execute_publication_transaction(
            &mut state,
            PublicationDisposition::Publish(token),
            |state, _| {
                assert_eq!(state.active, "old-active");
                assert_eq!(state.logical, "old-logical");
                publish_calls.set(publish_calls.get() + 1);
                events.borrow_mut().push("publish-consumers");
                let published_generation = generation.get();
                generation.set(published_generation + 1);
                Ok(published_generation)
            },
            |state, _, generation| {
                assert_eq!(generation, 41);
                state.commit_calls += 1;
                let retired = std::mem::replace(&mut state.active, state.staging);
                events.borrow_mut().push("physical-swap");
                state.logical = "promoted-logical";
                events.borrow_mut().push("logical-promotion");
                retired
            },
        )
        .unwrap();

        assert!(matches!(
            outcome,
            PublicationTransactionOutcome::Published {
                committed: "old-active",
                ..
            }
        ));
        assert_eq!(publish_calls.get(), 1);
        assert_eq!(state.commit_calls, 1);
        assert_eq!(state.active, "ready-staging");
        assert_eq!(state.logical, "promoted-logical");
        assert_eq!(generation.get(), 42);
        assert_eq!(
            *events.borrow(),
            ["publish-consumers", "physical-swap", "logical-promotion"]
        );
    }

    fn lighting_snapshot(sun_luminance: f32) -> DdgiRadianceSnapshot {
        DdgiRadianceSnapshot {
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
                emissive_color: Vec3::new(1.0, 0.36, 0.08),
                emissive_radiance: 4.0,
            },
            local_lights: LocalLightGpuPayload::empty(0),
        }
    }

    fn lighting_at(
        revision: u32,
        observed_at: Duration,
        snapshot: DdgiRadianceSnapshot,
    ) -> AuthoredEnvironmentLightingFact {
        AuthoredEnvironmentLightingFact::for_test(u64::from(revision), observed_at, snapshot)
    }

    fn lighting(revision: u32, sun_luminance: f32) -> AuthoredEnvironmentLightingFact {
        lighting_at(
            revision,
            Duration::from_millis(u64::from(revision) * 10),
            lighting_snapshot(sun_luminance),
        )
    }

    fn authored_input(snapshot: DdgiRadianceSnapshot) -> AuthoredEnvironmentLightingInput {
        AuthoredEnvironmentLightingInput {
            sun_direction: snapshot.sun_direction,
            sun_color: snapshot.sun_color,
            sun_luminance: snapshot.sun_luminance,
            terrain_ray_origin_offset_world: snapshot.terrain_ray_origin_offset_world,
            ddgi_receiver_visibility_bias_world: snapshot.ddgi_receiver_visibility_bias_world,
            voxel_palette: snapshot.voxel_palette,
            local_lights: snapshot.local_lights,
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
            publication: token
                .map(|token| crate::ddgi::DdgiFieldPublication::for_test(token, identity)),
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
        let active_only = runtime.status_from_physical(DdgiStatus::new(active, None));
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
        let building = runtime.status_from_physical(DdgiStatus::new(active, Some(staging)));
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
        let filter_evidence = super::super::resources::DdgiFilterEpochEvidence {
            field: captured_field,
            probe_count: 4,
            irradiance: Default::default(),
            visibility_history: Default::default(),
            visibility_samples: Default::default(),
            visibility_written: true,
        };
        let filter_proof = DdgiFilterEpochProof {
            configuration: DdgiFilterConfigurationIdentity {
                grid_dimensions: [4, 1, 1],
                configured_history_retention_q16: 64_881,
            },
            evidence: filter_evidence,
        };
        runtime.configure_capture(true, DdgiCaptureTarget::Published, DdgiBatchOrder::Reverse);
        runtime.observe_capture_checkpoint(
            token,
            captured_field,
            DdgiAtlasValidationStats::default(),
            Some(filter_proof),
            DdgiCapturePublication::Published,
        );

        let active = volume_status(Some(token), 7, 3, DdgiVolumeStage::Ready);
        let checkpoint = runtime
            .status_from_physical(DdgiStatus::new(active, None))
            .capture_checkpoint()
            .expect("resident published field should expose the checkpoint");
        assert_eq!(checkpoint.field, captured_field);
        assert_eq!(checkpoint.batch_order, DdgiBatchOrder::Reverse);
        assert_eq!(checkpoint.filter_proof, Some(filter_proof));

        let wrong_token = DdgiBuildToken::for_test(2, 7, 16, DdgiBuildKind::Terrain);
        let staging_field = field(8, 4);
        runtime.observe_capture_checkpoint(
            wrong_token,
            staging_field,
            DdgiAtlasValidationStats::default(),
            None,
            DdgiCapturePublication::Published,
        );
        let active_after_staging_checkpoint = runtime
            .status_from_physical(DdgiStatus::new(active, None))
            .capture_checkpoint()
            .expect("staging capture evidence must not hide the resident active checkpoint");
        assert_eq!(active_after_staging_checkpoint.field, captured_field);

        let mismatched_active = volume_status(Some(wrong_token), 7, 3, DdgiVolumeStage::Ready);
        assert!(runtime
            .status_from_physical(DdgiStatus::new(mismatched_active, None))
            .capture_checkpoint()
            .is_none());
    }

    #[test]
    #[should_panic(expected = "DDGI staging status must have an immutable build token")]
    fn staging_without_a_build_token_fails_fast() {
        let (runtime, _, _) = initialized_runtime();
        let active = volume_status(None, 7, 3, DdgiVolumeStage::Ready);
        let staging = volume_status(None, 8, 4, DdgiVolumeStage::Rebuilding);
        runtime.status_from_physical(DdgiStatus::new(active, Some(staging)));
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
    fn frame_work_identity_is_stable_and_failure_settles_through_runtime_interface() {
        let grid = DdgiVolumeGrid::new(UVec3::splat(512), 32).unwrap();
        let mut runtime = DdgiRuntime::new(grid);
        let plan = DdgiFramePlan {
            global_sky_needs_update: true,
            relocation_terrain_revision: Some(7),
            visibility_preservation_needed: true,
            ray_batch: None,
            iteration_will_complete: false,
        };
        let work = DdgiFrameWork { serial: 41, plan };
        runtime.pending_frame_work = Some(work);

        runtime.observe_authored_lighting(lighting(1, 1.0));
        runtime.observe_visible_terrain(8, edit_bound(200, 220));
        assert_eq!(
            work.plan(),
            plan,
            "queued facts must not rewrite a frame capsule"
        );

        let error = runtime
            .finish_frame_work(
                work,
                Err(anyhow::anyhow!("injected Vulkan encoding failure")),
            )
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("injected Vulkan encoding failure"));
        assert_eq!(runtime.pending_frame_work, None);

        let stale = runtime
            .finish_frame_work(work, Ok(()))
            .expect_err("one frame capsule must settle at most once");
        assert!(stale
            .to_string()
            .contains("stale or out-of-order DDGI frame work completion"));
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
        assert_eq!(replacement.authored_lighting().snapshot(), r2.snapshot());
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
    fn continuous_sun_changes_coalesce_to_the_latest_cadenced_transport() {
        let grid = DdgiVolumeGrid::new(UVec3::splat(512), 16).unwrap();
        let mut runtime = DdgiRuntime::new(grid);
        let initial = lighting_at(1, Duration::ZERO, lighting_snapshot(1.0));
        let initial_transport = runtime.observe_authored_lighting(initial);
        assert!(initial_transport.transport_published);

        for (revision, degrees, elapsed_ms) in
            [(2_u32, 1.0_f32, 50_u64), (3, 2.0, 100), (4, 3.0, 150)]
        {
            let mut snapshot = lighting_snapshot(1.0);
            snapshot.sun_direction = glam::Quat::from_rotation_x(degrees.to_radians()) * Vec3::Y;
            let changed = lighting_at(revision, Duration::from_millis(elapsed_ms), snapshot);
            let pending = runtime.observe_authored_lighting(changed);
            assert!(!pending.transport_published);
            assert_eq!(pending.transport, initial_transport.transport);
            assert_eq!(runtime.lighting_revision_lag(), u64::from(revision - 1));
        }

        let mut snapshot = lighting_snapshot(1.0);
        snapshot.sun_direction = glam::Quat::from_rotation_x(4.0_f32.to_radians()) * Vec3::Y;
        let latest = lighting_at(5, DDGI_TRANSPORT_MIN_PUBLICATION_INTERVAL, snapshot);
        let published = runtime.observe_authored_lighting(latest);

        assert!(published.transport_published);
        assert_eq!(published.transport.revision(), 2);
        assert_eq!(published.transport.source_live_revision(), 5);
        assert_eq!(published.transport.snapshot(), latest.snapshot());
        assert_eq!(
            published.transport.change().reason,
            DdgiRadianceChangeReason::ContinuousSun
        );
        assert_eq!(published.coalesced_live_revisions, 3);
        assert_eq!(runtime.lighting_revision_lag(), 0);
    }

    #[test]
    fn metadata_only_local_light_observation_keeps_authored_revision_and_transport_stable() {
        let grid = DdgiVolumeGrid::new(UVec3::splat(512), 16).unwrap();
        let mut authored = AuthoredEnvironmentLighting::default();
        let mut runtime = DdgiRuntime::new(grid);

        let first = authored.observe(authored_input(lighting_snapshot(1.0)), Duration::ZERO);
        let initial = runtime.observe_authored_lighting(first);
        assert!(initial.transport_published);

        let mut metadata_only = first.snapshot();
        metadata_only.local_lights.info.source_revision_low = 7;
        metadata_only.local_lights.info.registry_revision_low = 11;
        metadata_only.local_lights.info.overflow_count = 1;
        metadata_only.local_lights.info.transport_revision = 99;
        let next = authored.observe(authored_input(metadata_only), Duration::from_millis(1));
        assert_eq!(next.revision, first.revision);
        assert_ne!(next.snapshot().local_lights, first.snapshot().local_lights);
        assert_eq!(next.snapshot(), first.snapshot());
        assert_ne!(
            next.snapshot().local_lights.source_revision(),
            first.snapshot().local_lights.source_revision()
        );
        assert_eq!(next.snapshot().local_lights.live_revision(), 0);

        let unchanged = runtime.observe_authored_lighting(next);
        assert!(!unchanged.transport_published);
        assert_eq!(unchanged.transport, initial.transport);
        assert_eq!(runtime.lighting_revision_lag(), 0);
    }

    #[test]
    fn authored_sky_identity_change_is_an_immediate_transport_input_step() {
        let grid = DdgiVolumeGrid::new(UVec3::splat(512), 16).unwrap();
        let mut authored = AuthoredEnvironmentLighting::default();
        let mut runtime = DdgiRuntime::new(grid);
        let input = authored_input(lighting_snapshot(1.0));

        let first = authored.observe_for_test_authored_sky(input, 41, Duration::ZERO);
        let initial = runtime.observe_authored_lighting(first);
        assert!(initial.transport_published);

        let changed = authored.observe_for_test_authored_sky(input, 42, Duration::from_millis(1));
        assert_eq!(changed.revision, first.revision + 1);
        assert_eq!(changed.snapshot(), first.snapshot());
        let published = runtime.observe_authored_lighting(changed);

        assert!(published.transport_published);
        assert_eq!(
            published.transport.revision(),
            initial.transport.revision() + 1
        );
        assert_eq!(published.transport.source_live_revision(), changed.revision);
        assert_eq!(
            published.transport.change().reason,
            DdgiRadianceChangeReason::TransportInputStep
        );
        assert!(published.transport.change().resets_irradiance_history());
    }

    #[test]
    #[should_panic(expected = "reused live revision 1 for a different identity")]
    fn runtime_rejects_reused_live_revision_for_a_different_authoritative_identity() {
        let grid = DdgiVolumeGrid::new(UVec3::splat(512), 16).unwrap();
        let mut runtime = DdgiRuntime::new(grid);
        runtime.observe_authored_lighting(lighting(1, 1.0));
        runtime.observe_authored_lighting(lighting_at(
            1,
            Duration::from_millis(1),
            lighting_snapshot(2.0),
        ));
    }

    #[test]
    fn large_sun_and_material_discontinuities_publish_immediately() {
        let grid = DdgiVolumeGrid::new(UVec3::splat(512), 16).unwrap();
        let mut runtime = DdgiRuntime::new(grid);
        let initial = runtime.observe_authored_lighting(lighting(1, 1.0));
        assert_eq!(initial.transport.revision(), 1);

        let mut snapshot = lighting_snapshot(1.0);
        snapshot.sun_direction = Vec3::Z;
        let large_sun_step = lighting_at(2, Duration::from_millis(20), snapshot);
        let large_sun_step = runtime.observe_authored_lighting(large_sun_step);
        assert!(large_sun_step.transport_published);
        assert_eq!(large_sun_step.transport.revision(), 2);
        assert_eq!(
            large_sun_step.transport.change().reason,
            DdgiRadianceChangeReason::LargeSunStep
        );

        snapshot.voxel_palette.rock_color.x += 0.01;
        let material_step = lighting_at(3, Duration::from_millis(30), snapshot);
        let material_step = runtime.observe_authored_lighting(material_step);
        assert!(material_step.transport_published);
        assert_eq!(material_step.transport.revision(), 3);
        assert_eq!(
            material_step.transport.change().reason,
            DdgiRadianceChangeReason::TransportInputStep
        );
    }

    #[test]
    fn local_lights_are_cadenced_latest_wins_and_retain_history() {
        let grid = DdgiVolumeGrid::new(UVec3::splat(512), 16).unwrap();
        let mut runtime = DdgiRuntime::new(grid);
        let initial = lighting_at(1, Duration::ZERO, lighting_snapshot(1.0));
        let initial = runtime.observe_authored_lighting(initial);
        assert!(initial.transport_published);
        assert_eq!(initial.transport.snapshot().local_lights.count(), 0);

        let mut lights = LocalLightRegistry::default();
        let id = lights.add(LocalLight::Point(
            PointLight::new(Vec3::new(1.0, 2.0, 3.0), Vec3::ONE, 4.0, 0.05, 0.5).unwrap(),
        ));
        let mut added_snapshot = lighting_snapshot(1.0);
        added_snapshot.local_lights = LocalLightGpuSnapshot::from_authoritative(
            &lights.snapshot(),
            LocalLightBudget::point_lights(1),
            0,
        )
        .payload();
        let added = lighting_at(2, Duration::from_millis(1), added_snapshot);
        let pending = runtime.observe_authored_lighting(added);
        assert!(!pending.transport_published);
        assert_eq!(added.snapshot().local_lights.count(), 1);
        assert_eq!(pending.transport.snapshot().local_lights.count(), 0);
        assert_eq!(runtime.lighting_revision_lag(), 1);

        lights
            .update(
                id,
                LocalLight::Point(
                    PointLight::new(
                        Vec3::new(2.0, 2.0, 3.0),
                        Vec3::new(0.5, 0.75, 1.0),
                        8.0,
                        0.05,
                        0.5,
                    )
                    .unwrap(),
                ),
            )
            .unwrap();
        let mut moved_snapshot = lighting_snapshot(1.0);
        moved_snapshot.local_lights = LocalLightGpuSnapshot::from_authoritative(
            &lights.snapshot(),
            LocalLightBudget::point_lights(1),
            0,
        )
        .payload();
        let moved = lighting_at(3, Duration::from_millis(2), moved_snapshot);
        let coalesced = runtime.observe_authored_lighting(moved);
        assert!(!coalesced.transport_published);
        assert_eq!(coalesced.transport.snapshot().local_lights.count(), 0);
        assert_eq!(coalesced.coalesced_live_revisions, 1);
        assert_eq!(runtime.lighting_revision_lag(), 2);

        let moved_at_cadence = lighting_at(3, Duration::from_millis(200), moved.snapshot());
        let published = runtime.observe_authored_lighting(moved_at_cadence);
        assert!(published.transport_published);
        assert_eq!(published.transport.revision(), 2);
        assert_eq!(published.transport.source_live_revision(), 3);
        assert_eq!(
            published.transport.change().reason,
            DdgiRadianceChangeReason::LocalLights
        );
        assert_eq!(published.transport.snapshot().local_lights.count(), 1);
        assert_eq!(
            published
                .transport
                .snapshot()
                .local_lights
                .source_revision(),
            lights.snapshot().revision()
        );
        assert_eq!(
            published
                .transport
                .snapshot()
                .local_lights
                .info
                .transport_revision,
            published.transport.revision()
        );
        let addition_history =
            DdgiRadianceHistoryPolicy::between(initial.transport, published.transport);
        assert_eq!(
            addition_history.change.reason,
            DdgiRadianceChangeReason::LocalLights
        );
        assert!(!addition_history.resets_history());
        assert!(addition_history.retention(0.99) > 0.0);

        lights.remove(id).unwrap();
        let mut removed_snapshot = lighting_snapshot(1.0);
        removed_snapshot.local_lights = LocalLightGpuSnapshot::from_authoritative(
            &lights.snapshot(),
            LocalLightBudget::point_lights(1),
            0,
        )
        .payload();
        let removed = lighting_at(4, Duration::from_millis(201), removed_snapshot);
        let removal_pending = runtime.observe_authored_lighting(removed);
        assert!(!removal_pending.transport_published);
        assert_eq!(removed.snapshot().local_lights.count(), 0);
        assert_eq!(removal_pending.transport.snapshot().local_lights.count(), 1);
        assert_eq!(runtime.lighting_revision_lag(), 1);

        let removed_at_cadence = lighting_at(4, Duration::from_millis(400), removed.snapshot());
        let removal_published = runtime.observe_authored_lighting(removed_at_cadence);
        assert!(removal_published.transport_published);
        assert_eq!(removal_published.transport.revision(), 3);
        assert_eq!(removal_published.transport.source_live_revision(), 4);
        assert_eq!(
            removal_published.transport.change().reason,
            DdgiRadianceChangeReason::LocalLights
        );
        assert_eq!(
            removal_published.transport.snapshot().local_lights.count(),
            0
        );
        assert_eq!(
            removal_published
                .transport
                .snapshot()
                .local_lights
                .source_revision(),
            lights.snapshot().revision()
        );
        assert_eq!(
            removal_published
                .transport
                .snapshot()
                .local_lights
                .info
                .transport_revision,
            removal_published.transport.revision()
        );
        let removal_history =
            DdgiRadianceHistoryPolicy::between(published.transport, removal_published.transport);
        assert_eq!(
            removal_history.change.reason,
            DdgiRadianceChangeReason::LocalLights
        );
        assert!(!removal_history.resets_history());
        assert!(removal_history.retention(0.99) > 0.0);
    }

    #[test]
    fn radiance_observations_coalesce_without_mutating_the_in_flight_snapshot() {
        let (mut runtime, active_token, _) = initialized_runtime();
        let mut lights = LocalLightRegistry::default();
        let id = lights.add(LocalLight::Point(
            PointLight::new(Vec3::ONE, Vec3::ONE, 4.0, 0.05, 0.5).unwrap(),
        ));
        let mut r2_snapshot = lighting_snapshot(2.0);
        r2_snapshot.local_lights = LocalLightGpuSnapshot::from_authoritative(
            &lights.snapshot(),
            LocalLightBudget::point_lights(1),
            2,
        )
        .payload();
        let r2 = lighting_at(2, Duration::from_millis(20), r2_snapshot);
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
        assert_eq!(r2_work.authored_lighting().snapshot(), r2.snapshot());

        runtime.observe_authored_lighting(lighting(3, 3.0));
        lights.remove(id).unwrap();
        let mut r4_snapshot = lighting_snapshot(4.0);
        r4_snapshot.local_lights = LocalLightGpuSnapshot::from_authoritative(
            &lights.snapshot(),
            LocalLightBudget::point_lights(1),
            4,
        )
        .payload();
        let r4 = lighting_at(4, Duration::from_millis(40), r4_snapshot);
        runtime.observe_authored_lighting(r4);
        let pending = runtime.lighting_diagnostics();
        assert_eq!(runtime.latest_radiance_revision(), Some(3));
        assert_eq!(pending.latest_transport_revision, Some(3));
        assert_eq!(pending.latest_source_live_revision, Some(4));
        assert_eq!(pending.scheduler_published_revision, Some(1));
        assert_eq!(pending.in_flight_revision, Some(2));
        assert_eq!(pending.coalesced_revisions, 0);
        assert_eq!(runtime.coalesced_live_revisions(), 1);
        assert_eq!(pending.scheduler_revision_lag(), 2);
        assert!(!pending.has_mixed_in_flight_revision);
        assert_eq!(
            runtime.in_flight_authored_lighting().unwrap().snapshot(),
            r2.snapshot(),
        );
        assert_eq!(
            runtime
                .in_flight_authored_lighting()
                .unwrap()
                .snapshot()
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
            3
        );
        assert_eq!(claimed.scheduler_published_revision, Some(2));
        assert_eq!(claimed.in_flight_revision, Some(3));
        assert_eq!(claimed.scheduler_revision_lag(), 1);
        assert!(!claimed.has_mixed_in_flight_revision);
        assert_eq!(latest.authored_lighting().snapshot(), r4.snapshot());
        assert_eq!(
            latest.authored_lighting().snapshot().local_lights.count(),
            0
        );
        assert_eq!(
            latest.scheduled().destination().source(),
            Some(r2_scheduled.destination().field())
        );
    }

    #[test]
    fn radiance_work_derives_history_from_the_actual_published_source() {
        let (mut runtime, _, _) = initialized_runtime();
        let mut continuous_snapshot = lighting_snapshot(1.0);
        continuous_snapshot.sun_direction =
            glam::Quat::from_rotation_x(1.0_f32.to_radians()) * Vec3::Y;
        let continuous = lighting_at(2, Duration::from_millis(210), continuous_snapshot);
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
