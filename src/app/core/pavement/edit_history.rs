//! Current-gesture terrain ownership and exact course receipts live together.
//! Retraction preflights every conflict before writing, publishes even a partially
//! written suffix, and retires receipts only after terrain and flora succeed.
use crate::{builder::FloraRemovalReceipt, geom::UAabb3};
use anyhow::{ensure, Context, Result};
use glam::UVec3;
use std::collections::{HashMap, HashSet};

/// Internal storage seam: the App and deterministic in-memory tests use the same
/// transaction. GPU publication and flora identity remain owned by their adapters.
pub(super) trait RetractionTarget<F> {
    fn read_voxels(&mut self, offset: UVec3, dim: UVec3) -> Result<Vec<u8>>;
    fn write_voxels(&mut self, offset: UVec3, dim: UVec3, bytes: &[u8]) -> Result<()>;
    fn publish_voxels(&mut self, bound: UAabb3) -> Result<()>;
    fn restore_flora(&mut self, receipt: &F) -> Result<()>;
}

pub(super) struct CourseReceipt(usize);

pub(super) struct EditHistory<F = FloraRemovalReceipt> {
    reversible: bool,
    owned: HashSet<UVec3>,
    courses: Vec<CourseEdit<F>>,
}
struct CourseEdit<F> {
    voxels: Vec<VoxelUndo>,
    flora: Vec<F>,
}
struct VoxelDiff {
    index: usize,
    before: u8,
    after: u8,
    owned_before: bool,
}
struct VoxelUndo {
    offset: UVec3,
    dim: UVec3,
    changes: Vec<VoxelDiff>,
}
struct AtlasRestore {
    offset: UVec3,
    dim: UVec3,
    bytes: Vec<u8>,
}
struct Rollback {
    regions: Vec<AtlasRestore>,
    ownership: HashMap<UVec3, bool>,
}

fn region_len(dim: UVec3) -> Result<usize> {
    ensure!(
        dim.cmpgt(UVec3::ZERO).all(),
        "empty pavement receipt region"
    );
    (dim.x as usize)
        .checked_mul(dim.y as usize)
        .and_then(|xy| xy.checked_mul(dim.z as usize))
        .context("pavement receipt region overflow")
}
fn local(index: usize, dim: UVec3) -> UVec3 {
    UVec3::new(
        (index % dim.x as usize) as u32,
        (index / dim.x as usize % dim.y as usize) as u32,
        (index / dim.x as usize / dim.y as usize) as u32,
    )
}

impl<F> EditHistory<F> {
    pub(super) fn new(reversible: bool) -> Self {
        Self {
            reversible,
            owned: HashSet::new(),
            courses: Vec::new(),
        }
    }
    /// Read-only protection facts; only recording/retracting can change ownership.
    pub(super) fn owned_voxels(&self) -> &HashSet<UVec3> {
        &self.owned
    }
    pub(super) fn checkpoint(&self) -> usize {
        self.courses.len()
    }
    pub(super) fn begin_course(&mut self) -> Option<CourseReceipt> {
        if !self.reversible {
            return None;
        }
        let receipt = CourseReceipt(self.courses.len());
        self.courses.push(CourseEdit {
            voxels: Vec::new(),
            flora: Vec::new(),
        });
        Some(receipt)
    }
    /// Both images must surround this individual stamp, not the whole course's
    /// preflight: adjacent stones may share voxelized edges.
    pub(super) fn record_stamp(
        &mut self,
        offset: UVec3,
        dim: UVec3,
        before: &[u8],
        after: &[u8],
        material: u32,
    ) -> Result<()> {
        let len = region_len(dim)?;
        ensure!(
            before.len() == len && after.len() == len,
            "pavement receipt size mismatch"
        );
        ensure!(
            offset.cmple(UVec3::MAX - dim).all(),
            "pavement receipt bounds overflow"
        );
        ensure!(
            !self.reversible || !self.courses.is_empty(),
            "course must precede its stamps"
        );
        let mut changes = Vec::new();
        for (index, (&before, &after)) in before.iter().zip(after).enumerate() {
            if before == after {
                continue;
            }
            let cell = offset + local(index, dim);
            if self.reversible {
                changes.push(VoxelDiff {
                    index,
                    before,
                    after,
                    owned_before: self.owned.contains(&cell),
                });
            }
            let old = before & crate::builder::VOXEL_TYPE_MASK;
            let new = after & crate::builder::VOXEL_TYPE_MASK;
            if u32::from(new) == material && old != new {
                self.owned.insert(cell);
            }
        }
        if self.reversible {
            self.courses.last_mut().unwrap().voxels.push(VoxelUndo {
                offset,
                dim,
                changes,
            });
        }
        Ok(())
    }
    pub(super) fn record_flora(&mut self, receipt: CourseReceipt, flora: Vec<F>) {
        self.courses[receipt.0].flora = flora;
    }
    /// A write/publication/restore failure is not atomic. The caller must abandon
    /// the gesture on error, not retry it as though the world had been rolled back.
    pub(super) fn retract(
        &mut self,
        keep: usize,
        target: &mut impl RetractionTarget<F>,
    ) -> Result<usize> {
        ensure!(keep <= self.courses.len(), "invalid pavement checkpoint");
        let rollback = self.prepare_rollback(keep, target)?;
        let mut bound: Option<UAabb3> = None;
        let writes: Result<()> = (|| {
            for region in rollback.regions {
                target.write_voxels(region.offset, region.dim, &region.bytes)?;
                let changed = UAabb3::new(region.offset, region.offset + region.dim);
                bound = Some(bound.map_or(changed, |b| b.union_with(&changed)));
            }
            Ok(())
        })();
        // Successful earlier writes still need a Visible Terrain Publication if
        // a later upload fails. Never restore flora before this publication.
        if let Some(bound) = bound {
            target.publish_voxels(bound)?;
        }
        writes?;
        for course in self.courses[keep..].iter().rev() {
            for receipt in course.flora.iter().rev() {
                target.restore_flora(receipt)?;
            }
        }
        for (cell, owned) in rollback.ownership {
            if owned {
                self.owned.insert(cell);
            } else {
                self.owned.remove(&cell);
            }
        }
        let removed = self.courses.len() - keep;
        self.courses.truncate(keep);
        Ok(removed)
    }

    fn prepare_rollback(
        &self,
        keep: usize,
        target: &mut impl RetractionTarget<F>,
    ) -> Result<Rollback> {
        let mut shadow = HashMap::new();
        let mut ownership = HashMap::new();
        let mut regions = Vec::new();
        for undo in self.courses[keep..].iter().flat_map(|c| &c.voxels).rev() {
            if undo.changes.is_empty() {
                continue;
            }
            let bytes = target.read_voxels(undo.offset, undo.dim)?;
            ensure!(
                bytes.len() == region_len(undo.dim)?,
                "pavement readback size mismatch"
            );
            for change in undo.changes.iter().rev() {
                let cell = undo.offset + local(change.index, undo.dim);
                let live = *shadow.get(&cell).unwrap_or(&bytes[change.index]);
                ensure!(
                    live == change.after,
                    "pavement rollback conflict at {cell:?}; foreign terrain was not overwritten"
                );
                shadow.insert(cell, change.before);
                ownership.insert(cell, change.owned_before);
            }
            regions.push(AtlasRestore {
                offset: undo.offset,
                dim: undo.dim,
                bytes,
            });
        }
        // Overlapping stamps restore one final before-image, not stale rectangular
        // snapshots. Untouched cells retain their live bytes.
        for region in &mut regions {
            for (index, byte) in region.bytes.iter_mut().enumerate() {
                if let Some(before) = shadow.get(&(region.offset + local(index, region.dim))) {
                    *byte = *before;
                }
            }
        }
        Ok(Rollback { regions, ownership })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MemoryTarget {
        bytes: Vec<u8>,
        events: Vec<&'static str>,
        restored: Vec<u32>,
        published: Vec<UAabb3>,
        fail_write: Option<usize>,
        writes: usize,
        fail_publish: bool,
        fail_flora: bool,
    }
    impl MemoryTarget {
        fn new(bytes: &[u8]) -> Self {
            Self {
                bytes: bytes.to_vec(),
                events: Vec::new(),
                restored: Vec::new(),
                published: Vec::new(),
                fail_write: None,
                writes: 0,
                fail_publish: false,
                fail_flora: false,
            }
        }
    }
    impl RetractionTarget<u32> for MemoryTarget {
        fn read_voxels(&mut self, offset: UVec3, dim: UVec3) -> Result<Vec<u8>> {
            self.events.push("read");
            Ok(self.bytes[offset.x as usize..offset.x as usize + region_len(dim)?].to_vec())
        }
        fn write_voxels(&mut self, offset: UVec3, dim: UVec3, bytes: &[u8]) -> Result<()> {
            self.writes += 1;
            ensure!(
                self.fail_write != Some(self.writes),
                "injected write failure"
            );
            self.events.push("write");
            self.bytes[offset.x as usize..offset.x as usize + region_len(dim)?]
                .copy_from_slice(bytes);
            Ok(())
        }
        fn publish_voxels(&mut self, bound: UAabb3) -> Result<()> {
            self.events.push("publish");
            ensure!(!self.fail_publish, "injected publication failure");
            self.published.push(bound);
            Ok(())
        }
        fn restore_flora(&mut self, receipt: &u32) -> Result<()> {
            self.events.push("flora");
            ensure!(!self.fail_flora, "injected flora failure");
            self.restored.push(*receipt);
            Ok(())
        }
    }
    fn stamp(
        history: &mut EditHistory<u32>,
        before: &[u8],
        after: &[u8],
        material: u32,
        flora: u32,
    ) {
        let receipt = history.begin_course().unwrap();
        history
            .record_stamp(
                UVec3::ZERO,
                UVec3::new(before.len() as u32, 1, 1),
                before,
                after,
                material,
            )
            .unwrap();
        history.record_flora(receipt, vec![flora]);
    }
    fn overlapping() -> EditHistory<u32> {
        let mut history = EditHistory::new(true);
        stamp(&mut history, &[3, 3, 3, 3], &[4, 4, 3, 3], 4, 10);
        stamp(&mut history, &[4, 4, 3, 3], &[4, 5, 5, 3], 5, 20);
        history
    }
    #[test]
    fn retracting_suffix_preserves_shared_prefix_ownership_and_live_foreign_cells() {
        let mut history = overlapping();
        let mut target = MemoryTarget::new(&[4, 5, 5, 9]);
        assert_eq!(history.retract(1, &mut target).unwrap(), 1);
        assert_eq!(target.bytes, [4, 4, 3, 9]);
        assert_eq!(target.restored, [20]);
        assert_eq!(
            history.owned_voxels(),
            &HashSet::from([UVec3::ZERO, UVec3::X])
        );
        assert_eq!(history.checkpoint(), 1);
        assert_eq!(history.retract(0, &mut target).unwrap(), 1);
        assert_eq!(target.bytes, [3, 3, 3, 9]);
        assert_eq!(target.restored, [20, 10]);
        assert!(history.owned_voxels().is_empty());
    }
    #[test]
    fn complete_retraction_preflights_all_stamps_then_publishes_once_before_flora() {
        let mut history = overlapping();
        let mut target = MemoryTarget::new(&[4, 5, 5, 9]);
        assert_eq!(history.retract(0, &mut target).unwrap(), 2);
        assert_eq!(target.bytes, [3, 3, 3, 9]);
        assert_eq!(
            target.events,
            ["read", "read", "write", "write", "publish", "flora", "flora"]
        );
        assert_eq!(target.restored, [20, 10]);
        assert_eq!(target.published[0].min(), UVec3::ZERO);
        assert_eq!(target.published[0].max(), UVec3::new(4, 1, 1));
        assert!(history.owned_voxels().is_empty());
        let events = target.events.len();
        assert_eq!(history.retract(0, &mut target).unwrap(), 0);
        assert_eq!(target.events.len(), events);
    }
    #[test]
    fn conflict_in_an_older_receipt_prevents_every_write_and_flora_restore() {
        let mut history = overlapping();
        let mut target = MemoryTarget::new(&[9, 5, 5, 3]);
        assert!(history
            .retract(0, &mut target)
            .unwrap_err()
            .to_string()
            .contains("conflict"));
        assert_eq!(target.events, ["read", "read"]);
        assert_eq!(target.bytes, [9, 5, 5, 3]);
        assert_eq!(history.checkpoint(), 2);
        assert_eq!(history.owned_voxels().len(), 3);
    }
    #[test]
    fn write_failure_publishes_only_successful_regions_without_retiring_history() {
        for failure in [1, 2] {
            let mut history: EditHistory<u32> = EditHistory::new(true);
            for (x, material) in [(0, 4), (3, 5)] {
                history.begin_course();
                history
                    .record_stamp(
                        UVec3::new(x, 0, 0),
                        UVec3::ONE,
                        &[3],
                        &[material],
                        material.into(),
                    )
                    .unwrap();
            }
            let mut target = MemoryTarget::new(&[4, 3, 3, 5]);
            target.fail_write = Some(failure);
            assert!(history
                .retract(0, &mut target)
                .unwrap_err()
                .to_string()
                .contains("write failure"));
            if failure == 1 {
                assert_eq!(target.events, ["read", "read"]);
                assert_eq!(target.bytes, [4, 3, 3, 5]);
                assert!(target.published.is_empty());
            } else {
                assert_eq!(target.events, ["read", "read", "write", "publish"]);
                assert_eq!(target.bytes, [4, 3, 3, 3]);
                assert_eq!(
                    target.published,
                    [UAabb3::new(UVec3::new(3, 0, 0), UVec3::new(4, 1, 1))]
                );
            }
            assert!(target.restored.is_empty());
            assert_eq!(history.checkpoint(), 2);
        }
    }
    #[test]
    fn overlapping_rectangles_within_one_course_restore_each_immediate_before_image() {
        let mut history: EditHistory<u32> = EditHistory::new(true);
        let receipt = history.begin_course().unwrap();
        history
            .record_stamp(UVec3::ZERO, UVec3::new(3, 1, 1), &[3, 3, 3], &[4, 4, 3], 4)
            .unwrap();
        history
            .record_stamp(UVec3::X, UVec3::new(3, 1, 1), &[4, 3, 3], &[5, 5, 3], 5)
            .unwrap();
        history.record_flora(receipt, vec![10, 20]);
        let mut target = MemoryTarget::new(&[4, 5, 5, 9]);
        assert_eq!(history.retract(0, &mut target).unwrap(), 1);
        assert_eq!(target.bytes, [3, 3, 3, 9]);
        assert_eq!(target.restored, [20, 10]);
        assert!(history.owned_voxels().is_empty());
    }
    #[test]
    fn voxel_ownership_uses_atlas_xyz_order_and_region_offset() {
        let mut history: EditHistory<u32> = EditHistory::new(false);
        let offset = UVec3::new(10, 20, 30);
        history
            .record_stamp(
                offset,
                UVec3::splat(2),
                &[3; 8],
                &[3, 4, 4, 3, 3, 3, 4, 3],
                4,
            )
            .unwrap();
        assert_eq!(
            history.owned_voxels(),
            &HashSet::from([
                offset + UVec3::X,
                offset + UVec3::Y,
                offset + UVec3::Y + UVec3::Z
            ])
        );
    }
    #[test]
    fn publication_or_flora_failure_never_reports_a_retired_suffix() {
        for fail_publish in [true, false] {
            let mut history = overlapping();
            let mut target = MemoryTarget::new(&[4, 5, 5, 3]);
            target.fail_publish = fail_publish;
            target.fail_flora = !fail_publish;
            assert!(history.retract(0, &mut target).is_err());
            assert_eq!(history.checkpoint(), 2);
            assert_eq!(history.owned_voxels().len(), 3);
            assert!(target.restored.is_empty());
            if fail_publish {
                assert!(!target.events.contains(&"flora"));
            }
        }
    }
    #[test]
    fn recording_without_retraction_still_owns_only_new_material_not_flags_or_old_stones() {
        let mut history: EditHistory<u32> = EditHistory::new(false);
        assert!(history.begin_course().is_none());
        history
            .record_stamp(
                UVec3::ZERO,
                UVec3::new(3, 1, 1),
                &[3, 4, 4],
                &[4, 4, 4 | 0x80],
                4,
            )
            .unwrap();
        assert_eq!(history.owned_voxels(), &HashSet::from([UVec3::ZERO]));
        assert_eq!(history.checkpoint(), 0);
    }
    #[test]
    fn malformed_images_and_missing_course_do_not_mutate_ownership() {
        let mut history: EditHistory<u32> = EditHistory::new(true);
        assert!(history
            .record_stamp(UVec3::ZERO, UVec3::ONE, &[3], &[4], 4)
            .is_err());
        history.begin_course();
        for (dim, before, after) in [
            (UVec3::ZERO, &[3][..], &[4][..]),
            (UVec3::new(2, 1, 1), &[3][..], &[4, 4][..]),
            (UVec3::ONE, &[3][..], &[][..]),
        ] {
            assert!(history
                .record_stamp(UVec3::ZERO, dim, before, after, 4)
                .is_err());
        }
        assert!(history.owned_voxels().is_empty());
        let mut target = MemoryTarget::new(&[3]);
        assert!(history.retract(2, &mut target).is_err());
        assert!(target.events.is_empty());
    }
}
