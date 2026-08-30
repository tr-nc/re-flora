use crate::geom::UAabb3;
use glam::UVec3;

use super::{DdgiProbeSpacing, DdgiVolumeGrid};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DdgiBuildKind {
    Terrain,
    Density,
}

/// Immutable identity for one allocated DDGI builder target.
///
/// The serial prevents an old Ready notification from becoming valid again when terrain revision,
/// spacing, and build kind happen to repeat (the classic ABA case).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DdgiBuildToken {
    serial: u64,
    terrain_revision: u32,
    spacing: DdgiProbeSpacing,
    kind: DdgiBuildKind,
}

impl DdgiBuildToken {
    pub fn serial(self) -> u64 {
        self.serial
    }

    pub fn terrain_revision(self) -> u32 {
        self.terrain_revision
    }

    pub fn spacing_voxels(self) -> u32 {
        self.spacing.voxels()
    }

    pub(crate) fn spacing(self) -> DdgiProbeSpacing {
        self.spacing
    }

    pub fn kind(self) -> DdgiBuildKind {
        self.kind
    }

    #[cfg(test)]
    pub(crate) fn for_test(
        serial: u64,
        terrain_revision: u32,
        spacing_voxels: u32,
        kind: DdgiBuildKind,
    ) -> Self {
        Self {
            serial,
            terrain_revision,
            spacing: DdgiProbeSpacing::try_from(spacing_voxels)
                .expect("test DDGI build tokens require supported spacing"),
            kind,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DdgiRefreshState {
    Idle,
    AwaitingTerrain {
        latest_terrain_revision: u32,
    },
    BuildingTerrain {
        candidate: DdgiBuildToken,
        latest_terrain_revision: u32,
    },
    DensityQueued {
        spacing: DdgiProbeSpacing,
    },
    BuildingDensity {
        candidate: DdgiBuildToken,
        queued_spacing: Option<DdgiProbeSpacing>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DdgiTerrainRefreshRequest {
    terrain_revision: u32,
    edited_voxel_bound: UAabb3,
    invalidation_voxel_bound: UAabb3,
}

/// Arbitrates runtime terrain refreshes and density rebuilds around one physical staging volume.
/// Terrain always wins; superseded GPU work finishes harmlessly and cannot become consumer-visible.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DdgiTerrainRefresh {
    request: Option<DdgiTerrainRefreshRequest>,
    published_terrain_revision: Option<u32>,
    candidate: Option<DdgiBuildToken>,
    queued_density_spacing: Option<DdgiProbeSpacing>,
    next_build_serial: u64,
}

impl DdgiTerrainRefresh {
    /// Allocates the initial active-volume identity from the same monotonic serial sequence used
    /// by later staging builds. It is not a staging candidate and therefore is never promotable.
    pub(crate) fn allocate_initial_build_token(
        &mut self,
        terrain_revision: u32,
        spacing: DdgiProbeSpacing,
    ) -> DdgiBuildToken {
        self.allocate_token(terrain_revision, spacing, DdgiBuildKind::Terrain)
    }

    pub fn request(
        &mut self,
        terrain_revision: u32,
        edit_voxel_bound: UAabb3,
        grid: DdgiVolumeGrid,
    ) -> bool {
        if let Some(candidate) = self
            .candidate
            .filter(|candidate| candidate.kind == DdgiBuildKind::Density)
        {
            self.queued_density_spacing.get_or_insert(candidate.spacing);
        }
        let invalidation_voxel_bound = terrain_invalidation_bound(edit_voxel_bound, grid);
        match self.request.as_mut() {
            None => {
                self.request = Some(DdgiTerrainRefreshRequest {
                    terrain_revision,
                    edited_voxel_bound: edit_voxel_bound,
                    invalidation_voxel_bound,
                });
                true
            }
            Some(request) => {
                request.terrain_revision = terrain_revision;
                request.edited_voxel_bound =
                    request.edited_voxel_bound.union_with(&edit_voxel_bound);
                request.invalidation_voxel_bound =
                    terrain_invalidation_bound(request.edited_voxel_bound, grid);
                true
            }
        }
    }

    /// Records the exact terrain revision now visible to GPU consumers.
    pub fn mark_terrain_published(&mut self, terrain_revision: u32) {
        self.published_terrain_revision = Some(terrain_revision);
    }

    /// Queues a density rebuild. Terrain publication/builds always take priority.
    pub fn request_density_rebuild(&mut self, spacing: DdgiProbeSpacing) {
        self.queued_density_spacing = Some(spacing);
    }

    /// Claims the next builder target. Once claimed, preparation must succeed or the process fails
    /// fast; there is no recovery transition that makes the token claimable again.
    pub fn claim_next_build(
        &mut self,
        active_spacing: DdgiProbeSpacing,
        active_terrain_revision: u32,
    ) -> Option<DdgiBuildToken> {
        // One physical staging volume owns one update at a time. A newer terrain release may
        // replace the pending request, but it cannot allocate another token until the current
        // candidate has either promoted or finished obsolete.
        if self.candidate.is_some() {
            return None;
        }
        if let Some(request) = self.request {
            let exact_terrain_is_published =
                self.published_terrain_revision == Some(request.terrain_revision);
            if !exact_terrain_is_published {
                return None;
            }
            let token = self.allocate_token(
                request.terrain_revision,
                active_spacing,
                DdgiBuildKind::Terrain,
            );
            self.candidate = Some(token);
            return Some(token);
        }

        let spacing = self.queued_density_spacing.take()?;
        let token = self.allocate_token(active_terrain_revision, spacing, DdgiBuildKind::Density);
        self.candidate = Some(token);
        Some(token)
    }

    fn allocate_token(
        &mut self,
        terrain_revision: u32,
        spacing: DdgiProbeSpacing,
        kind: DdgiBuildKind,
    ) -> DdgiBuildToken {
        self.next_build_serial = self
            .next_build_serial
            .checked_add(1)
            .expect("DDGI build token serial exhausted");
        DdgiBuildToken {
            serial: self.next_build_serial,
            terrain_revision,
            spacing,
            kind,
        }
    }

    pub fn state(self) -> DdgiRefreshState {
        if let Some(request) = self.request {
            return match self.candidate {
                Some(candidate) if candidate.kind == DdgiBuildKind::Terrain => {
                    DdgiRefreshState::BuildingTerrain {
                        candidate,
                        latest_terrain_revision: request.terrain_revision,
                    }
                }
                _ => DdgiRefreshState::AwaitingTerrain {
                    latest_terrain_revision: request.terrain_revision,
                },
            };
        }
        match self.candidate {
            Some(candidate) => DdgiRefreshState::BuildingDensity {
                candidate,
                queued_spacing: self.queued_density_spacing,
            },
            None => self
                .queued_density_spacing
                .map_or(DdgiRefreshState::Idle, |spacing| {
                    DdgiRefreshState::DensityQueued { spacing }
                }),
        }
    }

    pub fn token_can_promote(self, token: DdgiBuildToken) -> bool {
        if self.candidate != Some(token) {
            return false;
        }
        match token.kind {
            DdgiBuildKind::Terrain => self.request.is_some_and(|request| {
                request.terrain_revision == token.terrain_revision
                    && self.published_terrain_revision == Some(token.terrain_revision)
            }),
            DdgiBuildKind::Density => {
                self.request.is_none() && self.queued_density_spacing.is_none()
            }
        }
    }

    pub(crate) fn token_is_obsolete_candidate(self, token: DdgiBuildToken) -> bool {
        self.candidate == Some(token) && !self.token_can_promote(token)
    }

    /// Completes the authoritative candidate. Only exact latest-terrain promotion clears the
    /// conservative invalidation domain.
    pub fn mark_promoted(&mut self, token: DdgiBuildToken) -> bool {
        if !self.token_can_promote(token) {
            return false;
        }
        self.candidate = None;
        if token.kind == DdgiBuildKind::Terrain {
            self.request = None;
        }
        true
    }

    /// Releases the single candidate slot after a completed build became obsolete. The latest
    /// terrain or density request remains queued and may be claimed on the next frame.
    pub fn finish_obsolete_candidate(&mut self, token: DdgiBuildToken) -> bool {
        if self.candidate != Some(token) || self.token_can_promote(token) {
            return false;
        }
        self.candidate = None;
        true
    }

    pub fn edited_voxel_bound(self) -> Option<UAabb3> {
        self.request.map(|request| request.edited_voxel_bound)
    }

    pub fn invalidation_voxel_bound(self) -> Option<UAabb3> {
        self.request.map(|request| request.invalidation_voxel_bound)
    }

    pub fn queued_density_spacing_voxels(self) -> Option<u32> {
        self.queued_density_spacing.map(DdgiProbeSpacing::voxels)
    }

    pub fn consume_initial_revision(&mut self, terrain_revision: u32) {
        if self
            .request
            .is_some_and(|request| request.terrain_revision == terrain_revision)
        {
            self.request = None;
        }
    }
}

fn terrain_invalidation_bound(edit_voxel_bound: UAabb3, grid: DdgiVolumeGrid) -> UAabb3 {
    // Match the production DDGI wake-up rule: immediately fail closed around the edited object
    // plus one probe cell. The retained field remains available elsewhere while the bounded
    // full-volume sweep eventually observes non-local transport changes.
    let margin = UVec3::splat(grid.spacing_voxels());
    UAabb3::new(
        edit_voxel_bound.min().saturating_sub(margin),
        edit_voxel_bound
            .max()
            .saturating_add(margin)
            .min(grid.world_extent_voxels()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spacing(voxels: u32) -> DdgiProbeSpacing {
        DdgiProbeSpacing::try_from(voxels).unwrap()
    }

    fn grid(spacing_voxels: u32) -> DdgiVolumeGrid {
        DdgiVolumeGrid::new(
            UVec3::splat(512),
            DdgiProbeSpacing::try_from(spacing_voxels).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn terrain_refresh_invalidates_only_the_edit_plus_one_probe_cell() {
        let edit = UAabb3::new(UVec3::splat(100), UVec3::splat(120));
        let spacing_32 = terrain_invalidation_bound(edit, grid(32));
        assert_eq!(spacing_32.min(), UVec3::splat(68));
        assert_eq!(spacing_32.max(), UVec3::splat(152));

        let edge = UAabb3::new(UVec3::splat(4), UVec3::splat(500));
        let clamped = terrain_invalidation_bound(edge, grid(32));
        assert_eq!(clamped.min(), UVec3::ZERO);
        assert_eq!(clamped.max(), UVec3::splat(512));
    }

    #[test]
    fn invalidation_survives_build_until_the_matching_revision_is_promoted() {
        let mut refresh = DdgiTerrainRefresh::default();
        assert!(refresh.request(
            7,
            UAabb3::new(UVec3::splat(100), UVec3::splat(120)),
            grid(32),
        ));
        assert_eq!(
            refresh.state(),
            DdgiRefreshState::AwaitingTerrain {
                latest_terrain_revision: 7
            }
        );
        assert!(refresh.invalidation_voxel_bound().is_some());

        refresh.mark_terrain_published(7);
        let token = refresh.claim_next_build(spacing(32), 6).unwrap();
        assert!(refresh.invalidation_voxel_bound().is_some());
        assert!(refresh.mark_promoted(token));
        assert_eq!(refresh.invalidation_voxel_bound(), None);
    }

    #[test]
    fn coordinator_state_maps_latest_revision_density_queue_and_invalidation() {
        let mut refresh = DdgiTerrainRefresh::default();
        refresh.request_density_rebuild(spacing(16));
        assert_eq!(
            refresh.state(),
            DdgiRefreshState::DensityQueued {
                spacing: DdgiProbeSpacing::try_from(16).unwrap()
            }
        );
        assert_eq!(refresh.queued_density_spacing_voxels(), Some(16));
        assert_eq!(refresh.invalidation_voxel_bound(), None);

        refresh.request(
            7,
            UAabb3::new(UVec3::splat(100), UVec3::splat(120)),
            grid(32),
        );
        assert_eq!(
            refresh.state(),
            DdgiRefreshState::AwaitingTerrain {
                latest_terrain_revision: 7,
            }
        );
        assert_eq!(refresh.queued_density_spacing_voxels(), Some(16));
        assert_eq!(
            refresh.invalidation_voxel_bound(),
            Some(UAabb3::new(UVec3::splat(68), UVec3::splat(152)))
        );
    }

    #[test]
    fn edits_before_terrain_publication_coalesce_revision_and_influence_bound() {
        let mut refresh = DdgiTerrainRefresh::default();
        assert!(refresh.request(
            7,
            UAabb3::new(UVec3::splat(100), UVec3::splat(110)),
            grid(16),
        ));
        assert!(refresh.request(
            8,
            UAabb3::new(UVec3::splat(200), UVec3::splat(210)),
            grid(16),
        ));

        assert_eq!(
            refresh.state(),
            DdgiRefreshState::AwaitingTerrain {
                latest_terrain_revision: 8
            }
        );
        let edited = refresh.edited_voxel_bound().unwrap();
        assert_eq!(edited.min(), UVec3::splat(100));
        assert_eq!(edited.max(), UVec3::splat(210));
        let invalidation = refresh.invalidation_voxel_bound().unwrap();
        assert_eq!(invalidation.min(), UVec3::splat(84));
        assert_eq!(invalidation.max(), UVec3::splat(226));
    }

    #[test]
    fn edit_during_build_obsoletes_the_old_candidate_without_clearing_invalidation() {
        let mut refresh = DdgiTerrainRefresh::default();
        assert!(refresh.request(
            7,
            UAabb3::new(UVec3::splat(100), UVec3::splat(120)),
            grid(32),
        ));
        refresh.mark_terrain_published(7);
        let first = refresh.claim_next_build(spacing(32), 6).unwrap();

        assert!(refresh.request(
            8,
            UAabb3::new(UVec3::splat(200), UVec3::splat(220)),
            grid(32),
        ));
        assert!(!refresh.token_can_promote(first));
        assert!(matches!(
            refresh.state(),
            DdgiRefreshState::BuildingTerrain {
                candidate,
                latest_terrain_revision: 8,
            } if candidate == first
        ));
        assert!(refresh.invalidation_voxel_bound().is_some());
    }

    #[test]
    fn terrain_probe_refresh_serializes_releases_while_one_build_is_active() {
        let mut refresh = DdgiTerrainRefresh::default();
        assert!(refresh.request(
            7,
            UAabb3::new(UVec3::splat(100), UVec3::splat(120)),
            grid(32),
        ));
        refresh.mark_terrain_published(7);
        let first = refresh.claim_next_build(spacing(32), 6).unwrap();

        assert!(refresh.request(
            8,
            UAabb3::new(UVec3::splat(200), UVec3::splat(220)),
            grid(32),
        ));
        refresh.mark_terrain_published(8);

        assert_eq!(refresh.claim_next_build(spacing(32), 6), None);
        assert_eq!(refresh.candidate, Some(first));
        assert!(refresh.finish_obsolete_candidate(first));
        let second = refresh.claim_next_build(spacing(32), 6).unwrap();
        assert_eq!(second.terrain_revision(), 8);
    }

    #[test]
    fn initial_build_consumes_only_its_matching_pending_refresh() {
        let mut refresh = DdgiTerrainRefresh::default();
        assert!(refresh.request(
            7,
            UAabb3::new(UVec3::splat(100), UVec3::splat(120)),
            grid(32),
        ));
        refresh.consume_initial_revision(6);
        assert!(matches!(
            refresh.state(),
            DdgiRefreshState::AwaitingTerrain {
                latest_terrain_revision: 7
            }
        ));
        refresh.consume_initial_revision(7);
        assert_eq!(refresh.state(), DdgiRefreshState::Idle);
        assert_eq!(refresh.invalidation_voxel_bound(), None);
    }

    #[test]
    fn pending_and_building_terrain_keeps_density_queued_until_promotion() {
        let mut refresh = DdgiTerrainRefresh::default();
        assert!(refresh.request(
            7,
            UAabb3::new(UVec3::splat(100), UVec3::splat(120)),
            grid(32),
        ));
        refresh.request_density_rebuild(spacing(16));
        refresh.mark_terrain_published(7);
        let terrain = refresh.claim_next_build(spacing(32), 6).unwrap();
        assert_eq!(terrain.kind(), DdgiBuildKind::Terrain);
        assert!(refresh.mark_promoted(terrain));
        let density = refresh.claim_next_build(spacing(32), 7).unwrap();
        assert_eq!(density.kind(), DdgiBuildKind::Density);
        assert_eq!(density.spacing_voxels(), 16);
    }

    #[test]
    fn a_new_edit_can_start_after_the_previous_revision_is_promoted() {
        let mut refresh = DdgiTerrainRefresh::default();
        assert!(refresh.request(
            7,
            UAabb3::new(UVec3::splat(100), UVec3::splat(120)),
            grid(32),
        ));
        refresh.mark_terrain_published(7);
        let first = refresh.claim_next_build(spacing(32), 6).unwrap();
        assert!(refresh.mark_promoted(first));
        assert!(refresh.request(
            8,
            UAabb3::new(UVec3::splat(200), UVec3::splat(220)),
            grid(32),
        ));
        assert!(matches!(
            refresh.state(),
            DdgiRefreshState::AwaitingTerrain {
                latest_terrain_revision: 8
            }
        ));
    }

    #[test]
    fn token_serial_prevents_aba_when_all_build_parameters_repeat() {
        let mut refresh = DdgiTerrainRefresh::default();
        refresh.request_density_rebuild(spacing(32));
        let first = refresh.claim_next_build(spacing(32), 7).unwrap();
        assert!(refresh.mark_promoted(first));

        refresh.request_density_rebuild(spacing(32));
        let second = refresh.claim_next_build(spacing(32), 7).unwrap();
        assert_ne!(first.serial(), second.serial());
        assert_eq!(first.terrain_revision(), second.terrain_revision());
        assert_eq!(first.spacing_voxels(), second.spacing_voxels());
        assert_eq!(first.kind(), second.kind());
        assert!(!refresh.token_can_promote(first));
        assert!(refresh.token_can_promote(second));
    }

    #[test]
    fn initial_and_runtime_tokens_share_one_nonzero_monotonic_sequence() {
        let mut refresh = DdgiTerrainRefresh::default();
        let initial = refresh.allocate_initial_build_token(7, spacing(32));
        assert_eq!(initial.serial(), 1);
        assert_eq!(initial.terrain_revision(), 7);
        assert_eq!(initial.spacing_voxels(), 32);
        assert_eq!(initial.kind(), DdgiBuildKind::Terrain);

        refresh.request_density_rebuild(spacing(16));
        assert_eq!(
            refresh.claim_next_build(spacing(32), 7).unwrap().serial(),
            2
        );
    }

    #[test]
    fn awaiting_edits_coalesce_until_the_exact_latest_terrain_is_published() {
        let mut refresh = DdgiTerrainRefresh::default();
        assert!(refresh.request(7, UAabb3::new(UVec3::splat(10), UVec3::splat(20)), grid(32),));
        assert!(refresh.request(8, UAabb3::new(UVec3::splat(30), UVec3::splat(40)), grid(32),));
        refresh.mark_terrain_published(7);
        assert_eq!(refresh.claim_next_build(spacing(32), 6), None);
        refresh.mark_terrain_published(8);
        let token = refresh.claim_next_build(spacing(32), 6).unwrap();
        assert_eq!(token.terrain_revision(), 8);
        let edited = refresh.edited_voxel_bound().unwrap();
        assert_eq!(edited.min(), UVec3::splat(10));
        assert_eq!(edited.max(), UVec3::splat(40));
    }

    #[test]
    fn obsolete_ready_token_cannot_promote_or_clear_the_latest_invalidation() {
        let mut refresh = DdgiTerrainRefresh::default();
        assert!(refresh.request(7, UAabb3::new(UVec3::splat(10), UVec3::splat(20)), grid(16),));
        refresh.mark_terrain_published(7);
        let first = refresh.claim_next_build(spacing(16), 6).unwrap();
        assert!(refresh.request(8, UAabb3::new(UVec3::splat(30), UVec3::splat(40)), grid(16),));

        assert!(!refresh.mark_promoted(first));
        assert!(refresh.invalidation_voxel_bound().is_some());
        refresh.mark_terrain_published(8);
        assert!(refresh.finish_obsolete_candidate(first));
        let second = refresh.claim_next_build(spacing(16), 6).unwrap();
        assert!(refresh.token_can_promote(second));
        assert!(refresh.mark_promoted(second));
        assert_eq!(refresh.invalidation_voxel_bound(), None);
    }

    #[test]
    fn density_and_terrain_arbitration_never_publishes_stale_metadata() {
        let mut refresh = DdgiTerrainRefresh::default();
        refresh.request_density_rebuild(spacing(16));
        let density = refresh.claim_next_build(spacing(32), 7).unwrap();
        assert_eq!(density.kind(), DdgiBuildKind::Density);

        assert!(refresh.request(8, UAabb3::new(UVec3::splat(10), UVec3::splat(20)), grid(32),));
        assert!(!refresh.token_can_promote(density));
        refresh.mark_terrain_published(8);
        assert_eq!(refresh.claim_next_build(spacing(32), 7), None);
        assert!(refresh.finish_obsolete_candidate(density));
        let terrain = refresh.claim_next_build(spacing(32), 7).unwrap();
        assert_eq!(terrain.kind(), DdgiBuildKind::Terrain);
        assert!(refresh.mark_promoted(terrain));

        let density_retry = refresh.claim_next_build(spacing(32), 8).unwrap();
        assert_eq!(density_retry.kind(), DdgiBuildKind::Density);
        assert_eq!(density_retry.terrain_revision(), 8);
        assert_ne!(density_retry.serial(), density.serial());
    }

    #[test]
    fn latest_density_request_supersedes_an_inflight_density_candidate() {
        let mut refresh = DdgiTerrainRefresh::default();
        refresh.request_density_rebuild(spacing(32));
        let first = refresh.claim_next_build(spacing(32), 7).unwrap();
        refresh.request_density_rebuild(spacing(16));
        assert!(!refresh.token_can_promote(first));

        assert_eq!(refresh.claim_next_build(spacing(32), 7), None);
        assert!(refresh.finish_obsolete_candidate(first));
        let second = refresh.claim_next_build(spacing(32), 7).unwrap();
        assert_eq!(second.spacing_voxels(), 16);
        assert_ne!(second.serial(), first.serial());
        assert!(!refresh.token_can_promote(first));
        assert!(refresh.token_can_promote(second));
    }

    #[test]
    fn exact_publication_ack_handles_wrapped_terrain_revision() {
        let mut refresh = DdgiTerrainRefresh::default();
        assert!(refresh.request(1, UAabb3::new(UVec3::splat(10), UVec3::splat(20)), grid(32),));
        refresh.mark_terrain_published(u32::MAX);
        assert_eq!(refresh.claim_next_build(spacing(32), u32::MAX), None);
        refresh.mark_terrain_published(1);
        assert_eq!(
            refresh
                .claim_next_build(spacing(32), u32::MAX)
                .unwrap()
                .terrain_revision(),
            1
        );
    }

    #[test]
    fn claimed_build_remains_authoritative_until_promotion() {
        let mut refresh = DdgiTerrainRefresh::default();
        assert!(refresh.request(7, UAabb3::new(UVec3::splat(10), UVec3::splat(20)), grid(32),));
        refresh.mark_terrain_published(7);
        let terrain = refresh.claim_next_build(spacing(32), 6).unwrap();
        assert!(matches!(
            refresh.state(),
            DdgiRefreshState::BuildingTerrain {
                candidate,
                latest_terrain_revision: 7,
            } if candidate == terrain
        ));
        assert_eq!(refresh.claim_next_build(spacing(32), 6), None);
        assert!(refresh.invalidation_voxel_bound().is_some());
        assert!(refresh.mark_promoted(terrain));

        refresh.request_density_rebuild(spacing(16));
        let density = refresh.claim_next_build(spacing(32), 7).unwrap();
        assert_eq!(
            refresh.state(),
            DdgiRefreshState::BuildingDensity {
                candidate: density,
                queued_spacing: None,
            }
        );
        assert_eq!(refresh.claim_next_build(spacing(32), 7), None);
        assert!(refresh.mark_promoted(density));
        assert_eq!(refresh.state(), DdgiRefreshState::Idle);
    }
}
