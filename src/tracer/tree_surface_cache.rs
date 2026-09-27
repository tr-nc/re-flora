//! Validity of a compiled tree surface, independent of unrelated terrain publications.
use crate::geom::{RoundCone, UAabb3};
use glam::UVec3;

/// Shared by CPU surface sampling and dependency construction (face neighbors fit inside it).
pub(super) const TREE_SURFACE_SAMPLE_RADIUS: i32 = 2;

/// Conservative support of the tree surface sampling function, not the bulk
/// readback box. Includes potential wood that is currently absent from the atlas,
/// so adding wood after an edit cannot bypass invalidation.
#[derive(Clone, Default)]
pub(crate) struct TreeSurfaceDependencies {
    regions: Vec<UAabb3>,
}

impl TreeSurfaceDependencies {
    pub(super) fn include_region(&mut self, origin: UVec3, dim: UVec3, cones: &[RoundCone]) {
        if dim.min_element() == 0 {
            return;
        }
        let read_max = origin + dim - UVec3::ONE;
        let halo = UVec3::splat(TREE_SURFACE_SAMPLE_RADIUS as u32);
        for cone in cones {
            let bounds = cone.aabb();
            let min = bounds
                .min()
                .floor()
                .as_uvec3()
                .saturating_sub(halo)
                .max(origin);
            let max = bounds
                .max()
                .ceil()
                .as_uvec3()
                .saturating_add(halo)
                .min(read_max);
            if min.cmple(max).all() {
                self.regions.push(UAabb3::new(min, max));
            }
        }
    }

    fn intersects(&self, affected: UAabb3) -> bool {
        // Voxel edit bounds may be inclusive (including a single voxel). Do not
        // use geometric positive-volume intersection, which drops boundary cells.
        self.regions.iter().any(|region| {
            region.min().cmple(affected.max()).all() && region.max().cmpge(affected.min()).all()
        })
    }
}

/// Bulk atlas readback extent; deliberately distinct from semantic dependencies.
pub(crate) fn tree_surface_read_bound(bound: UAabb3, world_dim: UVec3) -> UAabb3 {
    UAabb3::new(
        bound
            .min()
            .saturating_sub(UVec3::splat(TREE_SURFACE_SAMPLE_RADIUS as u32)),
        (bound.max() + UVec3::splat(TREE_SURFACE_SAMPLE_RADIUS as u32 + 1)).min(world_dim),
    )
}

#[derive(Default)]
pub(crate) struct TreeSurfaceCache {
    terrain_revision: Option<u32>,
    canonical_revision: u64,
    dirty: bool,
    dependencies: TreeSurfaceDependencies,
}

impl TreeSurfaceCache {
    pub fn terrain_revision(&self) -> Option<u32> {
        self.terrain_revision
    }

    pub fn is_current(&self, terrain_revision: u32, canonical_revision: u64) -> bool {
        !self.dirty
            && self.terrain_revision == Some(terrain_revision)
            && self.canonical_revision == canonical_revision
    }

    pub fn compiled(
        &mut self,
        terrain_revision: u32,
        canonical_revision: u64,
        dependencies: TreeSurfaceDependencies,
    ) {
        self.terrain_revision = Some(terrain_revision);
        self.canonical_revision = canonical_revision;
        self.dirty = false;
        self.dependencies = dependencies;
    }

    pub fn observe_terrain(&mut self, revision: u32, affected: UAabb3) {
        // Once invalid, subsequent unrelated edits cannot resurrect the old mesh.
        if self.terrain_revision.is_none() || self.dirty {
            return;
        }
        if self.dependencies.intersects(affected) {
            // Keep the old surface's identity until its replacement is uploaded.
            self.dirty = true;
        } else {
            // Geometry is unchanged, but it is validated against the new terrain publication.
            self.terrain_revision = Some(revision);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deps(regions: Vec<UAabb3>) -> TreeSurfaceDependencies {
        TreeSurfaceDependencies { regions }
    }

    fn bound(min: u32, max: u32) -> UAabb3 {
        UAabb3::new(UVec3::splat(min), UVec3::splat(max))
    }

    #[test]
    fn dependencies_keep_branch_halos_but_exclude_empty_space_between_branches() {
        use glam::Vec3;
        let cones = [
            RoundCone::new(1., Vec3::new(8., 8., 8.), 1., Vec3::new(8., 8., 8.)),
            RoundCone::new(1., Vec3::new(24., 8., 8.), 1., Vec3::new(24., 8., 8.)),
        ];
        let mut dependencies = TreeSurfaceDependencies::default();
        dependencies.include_region(UVec3::ZERO, UVec3::splat(32), &cones);
        let mut cache = TreeSurfaceCache::default();
        cache.compiled(1, 7, dependencies.clone());
        let gap = UVec3::new(16, 8, 8);
        cache.observe_terrain(2, UAabb3::new(gap, gap));
        assert!(cache.is_current(2, 7));
        // Dirt/stone in the normal halo matters, even though no wood was removed.
        let halo = UVec3::new(11, 8, 8);
        cache.observe_terrain(3, UAabb3::new(halo, halo));
        assert!(!cache.is_current(3, 7));
        // There need not be any currently occupied wood for new wood to invalidate.
        cache.compiled(4, 7, dependencies);
        let absent_wood = UVec3::new(24, 8, 8);
        cache.observe_terrain(5, UAabb3::new(absent_wood, absent_wood));
        assert!(!cache.is_current(5, 7));
    }

    #[test]
    fn unrelated_edit_advances_validity_without_rebuilding() {
        let mut cache = TreeSurfaceCache::default();
        cache.compiled(1, 7, deps(vec![bound(10, 20)]));
        cache.observe_terrain(2, bound(40, 50));
        assert!(cache.is_current(2, 7));
        assert_eq!(cache.terrain_revision(), Some(2));
    }

    #[test]
    fn intersecting_edit_stays_invalid_until_compiled_even_across_disabled_frames() {
        let mut cache = TreeSurfaceCache::default();
        cache.compiled(1, 7, deps(vec![bound(10, 20)]));
        cache.observe_terrain(2, bound(19, 21));
        cache.observe_terrain(3, bound(40, 50));
        assert!(!cache.is_current(3, 7));
        assert_eq!(cache.terrain_revision(), Some(1));
        cache.compiled(3, 7, deps(vec![bound(10, 20)]));
        assert!(cache.is_current(3, 7));
    }

    #[test]
    fn canonical_changes_require_rebuild_even_outside_old_read_bounds() {
        let mut cache = TreeSurfaceCache::default();
        cache.compiled(1, 7, deps(vec![bound(10, 20)]));
        cache.observe_terrain(2, bound(40, 50));
        assert!(!cache.is_current(2, 8));
        assert!(!cache.is_current(1, 7));
    }

    #[test]
    fn all_tree_regions_and_meshing_halo_participate_in_invalidation() {
        let mut cache = TreeSurfaceCache::default();
        let read = tree_surface_read_bound(bound(10, 20), UVec3::splat(512));
        assert_eq!(read, bound(8, 23));
        cache.compiled(1, 7, deps(vec![bound(40, 50), read]));
        cache.observe_terrain(2, bound(8, 9));
        assert!(!cache.is_current(2, 7));
        assert_eq!(
            tree_surface_read_bound(bound(0, 512), UVec3::splat(512)),
            bound(0, 512)
        );
    }

    #[test]
    fn unbuilt_cache_cannot_be_validated_by_an_edit() {
        let mut cache = TreeSurfaceCache::default();
        cache.observe_terrain(1, bound(40, 50));
        assert!(!cache.is_current(1, 0));
    }
}
