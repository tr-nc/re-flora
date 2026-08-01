use crate::geom::UAabb3;
use glam::UVec3;

use super::DdgiVolumeGrid;

/// A terrain edit can change the probe selected anywhere in the adjacent interpolation cage.
/// One probe spacing covers that cage, another half spacing covers relocation, and four voxels
/// cover the relocation clearance scan.
const DDGI_TERRAIN_INVALIDATION_CLEARANCE_VOXELS: u32 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DdgiTerrainRefreshPhase {
    AwaitingTerrain,
    Building,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DdgiTerrainRefreshRequest {
    terrain_revision: u32,
    influence_voxel_bound: UAabb3,
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
        let influence_voxel_bound = terrain_edit_influence_bound(edit_voxel_bound, grid);
        match self.request.as_mut() {
            None => {
                self.request = Some(DdgiTerrainRefreshRequest {
                    terrain_revision,
                    influence_voxel_bound,
                    phase: DdgiTerrainRefreshPhase::AwaitingTerrain,
                });
                true
            }
            Some(request) if request.phase == DdgiTerrainRefreshPhase::AwaitingTerrain => {
                request.terrain_revision = terrain_revision;
                request.influence_voxel_bound = request
                    .influence_voxel_bound
                    .union_with(&influence_voxel_bound);
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

    pub fn influence_voxel_bound(self) -> Option<UAabb3> {
        self.request.map(|request| request.influence_voxel_bound)
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

fn terrain_edit_influence_bound(edit_voxel_bound: UAabb3, grid: DdgiVolumeGrid) -> UAabb3 {
    let spacing_voxels = grid.spacing_voxels();
    let margin = UVec3::splat(
        spacing_voxels
            .saturating_add(spacing_voxels / 2)
            .saturating_add(DDGI_TERRAIN_INVALIDATION_CLEARANCE_VOXELS),
    );
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

    fn grid(spacing_voxels: u32) -> DdgiVolumeGrid {
        DdgiVolumeGrid::new(UVec3::splat(512), spacing_voxels).unwrap()
    }

    #[test]
    fn edit_influence_covers_cage_relocation_and_clearance_and_clamps_to_the_volume() {
        let interior =
            terrain_edit_influence_bound(UAabb3::new(UVec3::splat(50), UVec3::splat(60)), grid(16));
        assert_eq!(interior.min(), UVec3::splat(22));
        assert_eq!(interior.max(), UVec3::splat(88));

        let edge =
            terrain_edit_influence_bound(UAabb3::new(UVec3::splat(8), UVec3::splat(500)), grid(32));
        assert_eq!(edge.min(), UVec3::ZERO);
        assert_eq!(edge.max(), UVec3::splat(512));
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
        assert!(refresh.influence_voxel_bound().is_some());

        assert!(refresh.mark_building(7));
        assert_eq!(refresh.awaiting_terrain_revision(), None);
        assert!(!refresh.clear_promoted_revision(6));
        assert!(refresh.influence_voxel_bound().is_some());
        assert!(refresh.clear_promoted_revision(7));
        assert_eq!(refresh.influence_voxel_bound(), None);
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
        let influence = refresh.influence_voxel_bound().unwrap();
        assert_eq!(influence.min(), UVec3::splat(72));
        assert_eq!(influence.max(), UVec3::splat(238));
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
        let influence_before_second_edit = refresh.influence_voxel_bound();

        assert!(!refresh.request(
            8,
            UAabb3::new(UVec3::splat(200), UVec3::splat(220)),
            grid(32),
        ));
        assert_eq!(
            refresh.influence_voxel_bound(),
            influence_before_second_edit
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
        assert_eq!(refresh.influence_voxel_bound(), None);
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
