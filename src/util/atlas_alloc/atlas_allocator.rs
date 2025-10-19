#![allow(dead_code)]

use glam::UVec3;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

/// A single allocation in the texture atlas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AtlasAllocation {
    pub id: u64,
    pub offset: UVec3,
    pub dim: UVec3,
}

/// Simple shelf-based 3-D texture-atlas allocator.
pub struct AtlasAllocator {
    atlas_dim: UVec3,

    // where the next allocation *could* be placed.
    cursor: Cell<UVec3>,
    // height (y) of the current shelf inside the current z-slice.
    row_height: Cell<u32>,

    // book keeping.
    next_id: Cell<u64>,
    allocations: RefCell<HashMap<u64, AtlasAllocation>>,
}

impl AtlasAllocator {
    /// Create an empty allocator that can fill a texture of `atlas_size`.
    pub fn new(atlas_dim: UVec3) -> Self {
        Self {
            atlas_dim,
            cursor: Cell::new(UVec3::ZERO),
            row_height: Cell::new(0),
            next_id: Cell::new(0),
            allocations: RefCell::new(HashMap::new()),
        }
    }

    /// Try to allocate `size` in the atlas.  Returns an `Allocation` on success.
    pub fn allocate(&self, dim: UVec3) -> Result<AtlasAllocation, String> {
        if dim.x == 0 || dim.y == 0 || dim.z == 0 {
            return Err("size must be non-zero in every dimension".into());
        }
        if any_gt(dim, self.atlas_dim) {
            return Err("requested block is larger than the whole atlas".into());
        }

        // try to place it.  `place()` updates internal cursors on success.
        let offset = self
            .place(dim)
            .ok_or_else(|| "atlas is full – no place could be found".to_string())?;

        let id = self.next_id.get();
        self.next_id.set(id + 1);

        let alloc = AtlasAllocation { id, offset, dim };
        self.allocations.borrow_mut().insert(id, alloc.clone());
        Ok(alloc)
    }

    /// Look up an allocation by id.
    pub fn lookup(&self, id: u64) -> Option<AtlasAllocation> {
        self.allocations.borrow().get(&id).cloned()
    }

    /// Frees an allocation.
    pub fn deallocate(&self, id: u64) -> Result<(), String> {
        let existed = self.allocations.borrow_mut().remove(&id).is_some();
        if existed {
            Ok(())
        } else {
            Err(format!("no allocation with id {id}"))
        }
    }

    /// Compacts the atlas so that all blocks become contiguous again.
    ///
    /// The relative order of allocations (by id) is preserved.
    pub fn cleanup(&self) {
        // snapshot and sort by id so the result is deterministic.
        let mut items: Vec<AtlasAllocation> = self.allocations.borrow().values().cloned().collect();
        items.sort_by_key(|a| a.id);

        // reset packing state.
        self.cursor.set(UVec3::ZERO);
        self.row_height.set(0);

        // re-pack one by one.
        let mut map = self.allocations.borrow_mut();
        for mut a in items {
            // `place_no_record` cannot fail because the atlas was big enough
            // for exactly these items before.
            a.offset = self
                .place_no_record(a.dim)
                .expect("re-packing failed – this is a bug");
            map.insert(a.id, a);
        }
    }

    /// Drops every allocation and rewinds the allocator to its initial state.
    pub fn reset(&self) {
        self.cursor.set(UVec3::ZERO);
        self.row_height.set(0);
        self.allocations.borrow_mut().clear();
    }

    /* --------------------------------------------------------------------- */
    /*                          internal helpers                             */
    /* --------------------------------------------------------------------- */

    /// Same algorithm that `allocate()` uses but *without* writing to the map.
    fn place_no_record(&self, dim: UVec3) -> Option<UVec3> {
        // temporarily remember old cursor in case we have to roll back.
        let saved_cursor = self.cursor.get();
        let saved_row_height = self.row_height.get();

        let res = self.place(dim);

        if res.is_none() {
            // rollback
            self.cursor.set(saved_cursor);
            self.row_height.set(saved_row_height);
        }
        res
    }

    /// Shelf allocator:
    /// 1. If it does not fit in the current row, start a new row.
    /// 2. If it does not fit in this z-slice, start a new slice.
    ///
    /// Updates `cursor` and `row_height` on success.
    fn place(&self, dim: UVec3) -> Option<UVec3> {
        let mut cur = self.cursor.get();
        let mut row_h = self.row_height.get();
        let atlas_size = self.atlas_dim;

        // helper that checks whether `size` fits starting at `cur`.
        let fits = |cur: UVec3| {
            cur.x + dim.x <= atlas_size.x
                && cur.y + dim.y <= atlas_size.y
                && cur.z + dim.z <= atlas_size.z
        };

        // start a new row if X overflow.
        if cur.x + dim.x > atlas_size.x {
            cur.x = 0;
            cur.y += row_h;
            row_h = 0;
        }
        // start a new slice if Y overflow.
        if cur.y + dim.y > atlas_size.y {
            cur.x = 0;
            cur.y = 0;
            row_h = 0;
            cur.z += 1;
        }

        if !fits(cur) {
            return None;
        }

        // success – record new cursor and row height.
        let offset = cur;
        cur.x += dim.x;
        row_h = row_h.max(dim.y);

        self.cursor.set(cur);
        self.row_height.set(row_h);
        Some(offset)
    }
}

/// Return `true` if any component of `a` is larger than that of `b`.
fn any_gt(a: UVec3, b: UVec3) -> bool {
    a.x > b.x || a.y > b.y || a.z > b.z
}

/* ------------------------------------------------------------------------- */
/*                                    Tests                                 */
/* ------------------------------------------------------------------------- */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_allocation() {
        let atlas = AtlasAllocator::new(UVec3::new(16, 16, 1));

        let a = atlas.allocate(UVec3::new(8, 8, 1)).unwrap();
        assert_eq!(a.offset, UVec3::ZERO);

        let b = atlas.allocate(UVec3::new(4, 8, 1)).unwrap();
        assert_eq!(b.offset, UVec3::new(8, 0, 0));

        let c = atlas.allocate(UVec3::new(8, 4, 1)).unwrap();
        assert_eq!(c.offset, UVec3::new(0, 8, 0));
    }

    #[test]
    fn deallocate_and_cleanup() {
        let atlas = AtlasAllocator::new(UVec3::new(16, 16, 1));

        let a = atlas.allocate(UVec3::new(8, 8, 1)).unwrap();
        let b = atlas.allocate(UVec3::new(4, 8, 1)).unwrap();
        let _c = atlas.allocate(UVec3::new(8, 4, 1)).unwrap();

        atlas.deallocate(b.id).unwrap();

        // after cleanup the remaining allocations should be packed tightly:
        atlas.cleanup();
        let a2 = atlas.lookup(a.id).unwrap();
        assert_eq!(a2.offset, UVec3::ZERO);
        // the third allocation (`_c`) should now sit right after `a`.
        let c2 = atlas
            .allocations
            .borrow()
            .values()
            .find(|al| al.dim == UVec3::new(8, 4, 1))
            .cloned()
            .unwrap();
        assert_eq!(c2.offset, UVec3::new(8, 0, 0));
    }

    #[test]
    fn reset_empties_everything() {
        let atlas = AtlasAllocator::new(UVec3::new(8, 8, 1));
        atlas.allocate(UVec3::new(4, 4, 1)).unwrap();
        assert!(!atlas.allocations.borrow().is_empty());

        atlas.reset();
        assert!(atlas.allocations.borrow().is_empty());
        assert_eq!(atlas.cursor.get(), UVec3::ZERO);
    }

    // ---------------------------------------------------------------------
    // 3-D behaviour
    // ---------------------------------------------------------------------

    /// Filling one whole slice and then spilling into the next.
    #[test]
    fn three_d_spill_to_next_slice() {
        // two slices of 4×4 texels each.
        let atlas = AtlasAllocator::new(UVec3::new(4, 4, 2));

        // first slice (z == 0).
        let a0 = atlas.allocate(UVec3::new(4, 4, 1)).unwrap();
        assert_eq!(a0.offset, UVec3::new(0, 0, 0));

        // second slice (z == 1).
        let a1 = atlas.allocate(UVec3::new(4, 4, 1)).unwrap();
        assert_eq!(a1.offset, UVec3::new(0, 0, 1));

        // the third identical allocation must fail because the atlas
        // only has two z-slices.
        assert!(atlas.allocate(UVec3::new(4, 4, 1)).is_err());
    }

    /// Many small blocks that wrap first in X, then in Y, and finally in Z.
    #[test]
    fn three_d_small_blocks_wrap_every_axis() {
        let atlas = AtlasAllocator::new(UVec3::new(4, 4, 2));

        // slice 0 ──────────────────────────────────────────────
        // row 0
        assert_eq!(
            atlas.allocate(UVec3::new(2, 2, 1)).unwrap().offset,
            UVec3::new(0, 0, 0)
        );
        assert_eq!(
            atlas.allocate(UVec3::new(2, 2, 1)).unwrap().offset,
            UVec3::new(2, 0, 0)
        );
        // row 1
        assert_eq!(
            atlas.allocate(UVec3::new(2, 2, 1)).unwrap().offset,
            UVec3::new(0, 2, 0)
        );
        assert_eq!(
            atlas.allocate(UVec3::new(2, 2, 1)).unwrap().offset,
            UVec3::new(2, 2, 0)
        );

        // slice 1 ──────────────────────────────────────────────
        assert_eq!(
            atlas.allocate(UVec3::new(2, 2, 1)).unwrap().offset,
            UVec3::new(0, 0, 1)
        );
    }

    /// Deallocate something in slice 0, compact, and ensure everything
    /// is re-packed tightly across Z as well.
    #[test]
    fn three_d_cleanup_repacks_across_slices() {
        let atlas = AtlasAllocator::new(UVec3::new(4, 4, 2));

        let a = atlas.allocate(UVec3::new(4, 4, 1)).unwrap(); // z = 0
        let b = atlas.allocate(UVec3::new(4, 2, 1)).unwrap(); // z = 1, y-height = 2
        let c = atlas.allocate(UVec3::new(4, 2, 1)).unwrap(); // z = 1, y-offset = 2

        // remove the first big block in slice 0.
        atlas.deallocate(a.id).unwrap();

        // after compacting, `b` must now sit at z = 0.
        atlas.cleanup();
        assert_eq!(atlas.lookup(b.id).unwrap().offset, UVec3::new(0, 0, 0));
        assert_eq!(atlas.lookup(c.id).unwrap().offset, UVec3::new(0, 2, 0));
    }
}
