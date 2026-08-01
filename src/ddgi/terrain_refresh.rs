use crate::geom::UAabb3;
use glam::UVec3;

use super::DdgiVolumeGrid;

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
    spacing_voxels: u32,
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
        self.spacing_voxels
    }

    pub fn kind(self) -> DdgiBuildKind {
        self.kind
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
        spacing_voxels: u32,
    },
    BuildingDensity {
        candidate: DdgiBuildToken,
        queued_spacing_voxels: Option<u32>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DdgiTerrainRefreshPhase {
    AwaitingTerrain,
    Building,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DdgiTerrainRefreshRequest {
    terrain_revision: u32,
    edited_voxel_bound: UAabb3,
    invalidation_voxel_bound: UAabb3,
    phase: DdgiTerrainRefreshPhase,
}

/// Owns the single in-flight terrain revision supported by the first runtime-refresh milestone.
///
/// Edits that arrive before deferred terrain publication completes are combined. Edits that arrive
/// while a DDGI staging volume is already building are deliberately left to the later
/// latest-revision-wins milestone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DdgiTerrainRefresh {
    request: Option<DdgiTerrainRefreshRequest>,
    published_terrain_revision: Option<u32>,
    candidate: Option<DdgiBuildToken>,
    queued_density_spacing_voxels: Option<u32>,
    next_build_serial: u64,
}

impl DdgiTerrainRefresh {
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
            self.queued_density_spacing_voxels
                .get_or_insert(candidate.spacing_voxels);
        }
        let invalidation_voxel_bound = terrain_invalidation_bound(grid);
        match self.request.as_mut() {
            None => {
                self.request = Some(DdgiTerrainRefreshRequest {
                    terrain_revision,
                    edited_voxel_bound: edit_voxel_bound,
                    invalidation_voxel_bound,
                    phase: DdgiTerrainRefreshPhase::AwaitingTerrain,
                });
                true
            }
            Some(request) if request.phase == DdgiTerrainRefreshPhase::AwaitingTerrain => {
                request.terrain_revision = terrain_revision;
                request.edited_voxel_bound =
                    request.edited_voxel_bound.union_with(&edit_voxel_bound);
                request.invalidation_voxel_bound = invalidation_voxel_bound;
                true
            }
            Some(request) => {
                request.terrain_revision = terrain_revision;
                request.edited_voxel_bound =
                    request.edited_voxel_bound.union_with(&edit_voxel_bound);
                request.invalidation_voxel_bound = invalidation_voxel_bound;
                true
            }
        }
    }

    /// Records the exact terrain revision now visible to GPU consumers.
    pub fn mark_terrain_published(&mut self, terrain_revision: u32) {
        self.published_terrain_revision = Some(terrain_revision);
    }

    /// Queues a density rebuild. Terrain publication/builds always take priority.
    pub fn request_density_rebuild(&mut self, spacing_voxels: u32) {
        self.queued_density_spacing_voxels = Some(spacing_voxels);
    }

    /// Claims the next builder target. The caller must either install this token in a volume or
    /// report preparation failure through [`Self::mark_build_preparation_failed`].
    pub fn claim_next_build(
        &mut self,
        active_spacing_voxels: u32,
        active_terrain_revision: u32,
    ) -> Option<DdgiBuildToken> {
        if let Some(request) = self.request {
            let exact_terrain_is_published =
                self.published_terrain_revision == Some(request.terrain_revision);
            let candidate_matches_latest = self.candidate.is_some_and(|candidate| {
                candidate.kind == DdgiBuildKind::Terrain
                    && candidate.terrain_revision == request.terrain_revision
                    && candidate.spacing_voxels == active_spacing_voxels
            });
            if !exact_terrain_is_published || candidate_matches_latest {
                return None;
            }
            let token = self.allocate_token(
                request.terrain_revision,
                active_spacing_voxels,
                DdgiBuildKind::Terrain,
            );
            self.request
                .as_mut()
                .expect("terrain request was present above")
                .phase = DdgiTerrainRefreshPhase::Building;
            self.candidate = Some(token);
            return Some(token);
        }

        let spacing_voxels = self.queued_density_spacing_voxels.take()?;
        let token = self.allocate_token(
            active_terrain_revision,
            spacing_voxels,
            DdgiBuildKind::Density,
        );
        self.candidate = Some(token);
        Some(token)
    }

    fn allocate_token(
        &mut self,
        terrain_revision: u32,
        spacing_voxels: u32,
        kind: DdgiBuildKind,
    ) -> DdgiBuildToken {
        self.next_build_serial = self
            .next_build_serial
            .checked_add(1)
            .expect("DDGI build token serial exhausted");
        DdgiBuildToken {
            serial: self.next_build_serial,
            terrain_revision,
            spacing_voxels,
            kind,
        }
    }

    pub fn candidate(self) -> Option<DdgiBuildToken> {
        self.candidate
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
                queued_spacing_voxels: self.queued_density_spacing_voxels,
            },
            None => self
                .queued_density_spacing_voxels
                .map_or(DdgiRefreshState::Idle, |spacing_voxels| {
                    DdgiRefreshState::DensityQueued { spacing_voxels }
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
                self.request.is_none() && self.queued_density_spacing_voxels.is_none()
            }
        }
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

    /// Restores queued work when allocating or binding the claimed target failed.
    pub fn mark_build_preparation_failed(&mut self, token: DdgiBuildToken) -> bool {
        if self.candidate != Some(token) {
            return false;
        }
        self.candidate = None;
        match token.kind {
            DdgiBuildKind::Terrain => {
                if let Some(request) = self.request.as_mut() {
                    request.phase = DdgiTerrainRefreshPhase::AwaitingTerrain;
                }
            }
            DdgiBuildKind::Density => {
                self.queued_density_spacing_voxels
                    .get_or_insert(token.spacing_voxels);
            }
        }
        true
    }

    pub fn awaiting_terrain_revision(self) -> Option<u32> {
        self.request.and_then(|request| {
            (request.phase == DdgiTerrainRefreshPhase::AwaitingTerrain)
                .then_some(request.terrain_revision)
        })
    }

    pub fn mark_building(&mut self, terrain_revision: u32) -> bool {
        let Some(request) = self.request.as_mut() else {
            return false;
        };
        if request.phase != DdgiTerrainRefreshPhase::AwaitingTerrain
            || request.terrain_revision != terrain_revision
        {
            return false;
        }
        request.phase = DdgiTerrainRefreshPhase::Building;
        true
    }

    pub fn edited_voxel_bound(self) -> Option<UAabb3> {
        self.request.map(|request| request.edited_voxel_bound)
    }

    pub fn invalidation_voxel_bound(self) -> Option<UAabb3> {
        self.request.map(|request| request.invalidation_voxel_bound)
    }

    pub fn blocks_density_rebuild(self) -> bool {
        self.request.is_some()
    }

    pub fn clear_initial_revision(&mut self, terrain_revision: u32) {
        if self
            .request
            .is_some_and(|request| request.terrain_revision == terrain_revision)
        {
            self.request = None;
        }
    }

    pub fn clear_promoted_revision(&mut self, terrain_revision: u32) -> bool {
        let clears_request = self.request.is_some_and(|request| {
            request.phase == DdgiTerrainRefreshPhase::Building
                && request.terrain_revision == terrain_revision
        });
        if clears_request {
            self.request = None;
        }
        clears_request
    }
}

fn terrain_invalidation_bound(grid: DdgiVolumeGrid) -> UAabb3 {
    // Any probe can trace through the edited geometry, so no subset of the old probe field is
    // generally trustworthy. Local dependency tracking is a future optimization.
    UAabb3::new(UVec3::ZERO, grid.world_extent_voxels())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(spacing_voxels: u32) -> DdgiVolumeGrid {
        DdgiVolumeGrid::new(UVec3::splat(512), spacing_voxels).unwrap()
    }

    #[test]
    fn terrain_refresh_invalidates_the_full_ddgi_world_domain() {
        for spacing_voxels in [32, 16] {
            let invalidation = terrain_invalidation_bound(grid(spacing_voxels));
            assert_eq!(invalidation.min(), UVec3::ZERO);
            assert_eq!(invalidation.max(), UVec3::splat(512));
        }
    }

    #[test]
    fn invalidation_survives_build_until_the_matching_revision_is_promoted() {
        let mut refresh = DdgiTerrainRefresh::default();
        assert!(refresh.request(
            7,
            UAabb3::new(UVec3::splat(100), UVec3::splat(120)),
            grid(32),
        ));
        assert_eq!(refresh.awaiting_terrain_revision(), Some(7));
        assert!(refresh.invalidation_voxel_bound().is_some());

        assert!(refresh.mark_building(7));
        assert_eq!(refresh.awaiting_terrain_revision(), None);
        assert!(!refresh.clear_promoted_revision(6));
        assert!(refresh.invalidation_voxel_bound().is_some());
        assert!(refresh.clear_promoted_revision(7));
        assert_eq!(refresh.invalidation_voxel_bound(), None);
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

        assert_eq!(refresh.awaiting_terrain_revision(), Some(8));
        let edited = refresh.edited_voxel_bound().unwrap();
        assert_eq!(edited.min(), UVec3::splat(100));
        assert_eq!(edited.max(), UVec3::splat(210));
        let invalidation = refresh.invalidation_voxel_bound().unwrap();
        assert_eq!(invalidation.min(), UVec3::ZERO);
        assert_eq!(invalidation.max(), UVec3::splat(512));
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
        let first = refresh.claim_next_build(32, 6).unwrap();

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
    fn initial_build_consumes_only_its_matching_pending_refresh() {
        let mut refresh = DdgiTerrainRefresh::default();
        assert!(refresh.request(
            7,
            UAabb3::new(UVec3::splat(100), UVec3::splat(120)),
            grid(32),
        ));
        refresh.clear_initial_revision(6);
        assert_eq!(refresh.awaiting_terrain_revision(), Some(7));
        refresh.clear_initial_revision(7);
        assert_eq!(refresh.awaiting_terrain_revision(), None);
        assert_eq!(refresh.invalidation_voxel_bound(), None);
    }

    #[test]
    fn pending_and_building_refreshes_block_density_rebuilds_until_promotion() {
        let mut refresh = DdgiTerrainRefresh::default();
        assert!(!refresh.blocks_density_rebuild());
        assert!(refresh.request(
            7,
            UAabb3::new(UVec3::splat(100), UVec3::splat(120)),
            grid(32),
        ));
        assert!(refresh.blocks_density_rebuild());
        assert!(refresh.mark_building(7));
        assert!(refresh.blocks_density_rebuild());
        assert!(refresh.clear_promoted_revision(7));
        assert!(!refresh.blocks_density_rebuild());
    }

    #[test]
    fn a_new_edit_can_start_after_the_previous_revision_is_promoted() {
        let mut refresh = DdgiTerrainRefresh::default();
        assert!(refresh.request(
            7,
            UAabb3::new(UVec3::splat(100), UVec3::splat(120)),
            grid(32),
        ));
        assert!(refresh.mark_building(7));
        assert!(refresh.clear_promoted_revision(7));
        assert!(refresh.request(
            8,
            UAabb3::new(UVec3::splat(200), UVec3::splat(220)),
            grid(32),
        ));
        assert_eq!(refresh.awaiting_terrain_revision(), Some(8));
    }

    #[test]
    fn token_serial_prevents_aba_when_all_build_parameters_repeat() {
        let mut refresh = DdgiTerrainRefresh::default();
        refresh.request_density_rebuild(32);
        let first = refresh.claim_next_build(32, 7).unwrap();
        assert!(refresh.mark_promoted(first));

        refresh.request_density_rebuild(32);
        let second = refresh.claim_next_build(32, 7).unwrap();
        assert_ne!(first.serial(), second.serial());
        assert_eq!(first.terrain_revision(), second.terrain_revision());
        assert_eq!(first.spacing_voxels(), second.spacing_voxels());
        assert_eq!(first.kind(), second.kind());
        assert!(!refresh.token_can_promote(first));
        assert!(refresh.token_can_promote(second));
    }

    #[test]
    fn awaiting_edits_coalesce_until_the_exact_latest_terrain_is_published() {
        let mut refresh = DdgiTerrainRefresh::default();
        assert!(refresh.request(7, UAabb3::new(UVec3::splat(10), UVec3::splat(20)), grid(32),));
        assert!(refresh.request(8, UAabb3::new(UVec3::splat(30), UVec3::splat(40)), grid(32),));
        refresh.mark_terrain_published(7);
        assert_eq!(refresh.claim_next_build(32, 6), None);
        refresh.mark_terrain_published(8);
        let token = refresh.claim_next_build(32, 6).unwrap();
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
        let first = refresh.claim_next_build(16, 6).unwrap();
        assert!(refresh.request(8, UAabb3::new(UVec3::splat(30), UVec3::splat(40)), grid(16),));

        assert!(!refresh.mark_promoted(first));
        assert!(refresh.invalidation_voxel_bound().is_some());
        refresh.mark_terrain_published(8);
        let second = refresh.claim_next_build(16, 6).unwrap();
        assert!(refresh.token_can_promote(second));
        assert!(refresh.mark_promoted(second));
        assert_eq!(refresh.invalidation_voxel_bound(), None);
    }

    #[test]
    fn density_and_terrain_arbitration_never_publishes_stale_metadata() {
        let mut refresh = DdgiTerrainRefresh::default();
        refresh.request_density_rebuild(16);
        let density = refresh.claim_next_build(32, 7).unwrap();
        assert_eq!(density.kind(), DdgiBuildKind::Density);

        assert!(refresh.request(8, UAabb3::new(UVec3::splat(10), UVec3::splat(20)), grid(32),));
        assert!(!refresh.token_can_promote(density));
        refresh.mark_terrain_published(8);
        let terrain = refresh.claim_next_build(32, 7).unwrap();
        assert_eq!(terrain.kind(), DdgiBuildKind::Terrain);
        assert!(refresh.mark_promoted(terrain));

        let density_retry = refresh.claim_next_build(32, 8).unwrap();
        assert_eq!(density_retry.kind(), DdgiBuildKind::Density);
        assert_eq!(density_retry.terrain_revision(), 8);
        assert_ne!(density_retry.serial(), density.serial());
    }

    #[test]
    fn latest_density_request_supersedes_an_inflight_density_candidate() {
        let mut refresh = DdgiTerrainRefresh::default();
        refresh.request_density_rebuild(32);
        let first = refresh.claim_next_build(32, 7).unwrap();
        refresh.request_density_rebuild(16);
        assert!(!refresh.token_can_promote(first));

        let second = refresh.claim_next_build(32, 7).unwrap();
        assert_eq!(second.spacing_voxels(), 16);
        assert_ne!(second.serial(), first.serial());
        assert!(!refresh.token_can_promote(first));
        assert!(refresh.token_can_promote(second));
    }

    #[test]
    fn failed_obsolete_density_preparation_does_not_overwrite_newer_queue() {
        let mut refresh = DdgiTerrainRefresh::default();
        refresh.request_density_rebuild(32);
        let first = refresh.claim_next_build(32, 7).unwrap();
        refresh.request_density_rebuild(16);
        assert!(refresh.mark_build_preparation_failed(first));
        assert_eq!(
            refresh.state(),
            DdgiRefreshState::DensityQueued { spacing_voxels: 16 }
        );
    }

    #[test]
    fn exact_publication_ack_handles_wrapped_terrain_revision() {
        let mut refresh = DdgiTerrainRefresh::default();
        assert!(refresh.request(1, UAabb3::new(UVec3::splat(10), UVec3::splat(20)), grid(32),));
        refresh.mark_terrain_published(u32::MAX);
        assert_eq!(refresh.claim_next_build(32, u32::MAX), None);
        refresh.mark_terrain_published(1);
        assert_eq!(
            refresh
                .claim_next_build(32, u32::MAX)
                .unwrap()
                .terrain_revision(),
            1
        );
    }

    #[test]
    fn preparation_failure_preserves_terrain_and_density_work() {
        let mut refresh = DdgiTerrainRefresh::default();
        assert!(refresh.request(7, UAabb3::new(UVec3::splat(10), UVec3::splat(20)), grid(32),));
        refresh.mark_terrain_published(7);
        let terrain = refresh.claim_next_build(32, 6).unwrap();
        assert!(refresh.mark_build_preparation_failed(terrain));
        assert!(matches!(
            refresh.state(),
            DdgiRefreshState::AwaitingTerrain {
                latest_terrain_revision: 7
            }
        ));
        assert!(refresh.invalidation_voxel_bound().is_some());

        let terrain_retry = refresh.claim_next_build(32, 6).unwrap();
        assert!(refresh.mark_promoted(terrain_retry));
        refresh.request_density_rebuild(16);
        let density = refresh.claim_next_build(32, 7).unwrap();
        assert!(refresh.mark_build_preparation_failed(density));
        assert_eq!(
            refresh.state(),
            DdgiRefreshState::DensityQueued { spacing_voxels: 16 }
        );
    }
}
