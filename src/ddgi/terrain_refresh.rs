use crate::geom::UAabb3;
use glam::UVec3;

use super::DdgiVolumeGrid;

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
}

impl DdgiTerrainRefresh {
    pub fn request(
        &mut self,
        terrain_revision: u32,
        edit_voxel_bound: UAabb3,
        grid: DdgiVolumeGrid,
    ) -> bool {
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
            Some(_) => false,
        }
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
    fn edit_during_build_is_left_for_latest_revision_wins_follow_up() {
        let mut refresh = DdgiTerrainRefresh::default();
        assert!(refresh.request(
            7,
            UAabb3::new(UVec3::splat(100), UVec3::splat(120)),
            grid(32),
        ));
        assert!(refresh.mark_building(7));
        let invalidation_before_second_edit = refresh.invalidation_voxel_bound();

        assert!(!refresh.request(
            8,
            UAabb3::new(UVec3::splat(200), UVec3::splat(220)),
            grid(32),
        ));
        assert_eq!(
            refresh.invalidation_voxel_bound(),
            invalidation_before_second_edit
        );
        assert!(!refresh.clear_promoted_revision(8));
        assert!(refresh.clear_promoted_revision(7));
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
}
