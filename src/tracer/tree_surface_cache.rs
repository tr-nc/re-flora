//! Validity of a compiled tree surface, independent of unrelated terrain publications.
use crate::geom::UAabb3;
use glam::UVec3;

/// The exact atlas footprint consumed by tree meshing, including its neighbor halo.
pub(crate) fn tree_surface_read_bound(bound: UAabb3, world_dim: UVec3) -> UAabb3 {
    UAabb3::new(
        bound.min().saturating_sub(UVec3::splat(2)),
        (bound.max() + UVec3::splat(3)).min(world_dim),
    )
}

#[derive(Default)]
pub(crate) struct TreeSurfaceCache {
    terrain_revision: Option<u32>,
    canonical_revision: u64,
    dirty: bool,
    read_bounds: Vec<UAabb3>,
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
        bounds: Vec<UAabb3>,
    ) {
        self.terrain_revision = Some(terrain_revision);
        self.canonical_revision = canonical_revision;
        self.dirty = false;
        self.read_bounds = bounds;
    }

    pub fn observe_terrain(&mut self, revision: u32, affected: UAabb3) {
        // Once invalid, subsequent unrelated edits cannot resurrect the old mesh.
        if self.terrain_revision.is_none() || self.dirty {
            return;
        }
        if self.read_bounds.iter().any(|b| b.intersects(&affected)) {
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

    fn bound(min: u32, max: u32) -> UAabb3 {
        UAabb3::new(UVec3::splat(min), UVec3::splat(max))
    }

    #[test]
    fn unrelated_edit_advances_validity_without_rebuilding() {
        let mut cache = TreeSurfaceCache::default();
        cache.compiled(1, 7, vec![bound(10, 20)]);
        cache.observe_terrain(2, bound(40, 50));
        assert!(cache.is_current(2, 7));
        assert_eq!(cache.terrain_revision(), Some(2));
    }

    #[test]
    fn intersecting_edit_stays_invalid_until_compiled_even_across_disabled_frames() {
        let mut cache = TreeSurfaceCache::default();
        cache.compiled(1, 7, vec![bound(10, 20)]);
        cache.observe_terrain(2, bound(19, 21));
        cache.observe_terrain(3, bound(40, 50));
        assert!(!cache.is_current(3, 7));
        assert_eq!(cache.terrain_revision(), Some(1));
        cache.compiled(3, 7, vec![bound(10, 20)]);
        assert!(cache.is_current(3, 7));
    }

    #[test]
    fn canonical_changes_require_rebuild_even_outside_old_read_bounds() {
        let mut cache = TreeSurfaceCache::default();
        cache.compiled(1, 7, vec![bound(10, 20)]);
        cache.observe_terrain(2, bound(40, 50));
        assert!(!cache.is_current(2, 8));
        assert!(!cache.is_current(1, 7));
    }

    #[test]
    fn all_tree_regions_and_meshing_halo_participate_in_invalidation() {
        let mut cache = TreeSurfaceCache::default();
        let read = tree_surface_read_bound(bound(10, 20), UVec3::splat(512));
        assert_eq!(read, bound(8, 23));
        cache.compiled(1, 7, vec![bound(40, 50), read]);
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
