//! Exact rollback of stamped wood, including pre-existing edits/material/soil state.
use crate::{app::world_edits::VoxelEdit, geom::UAabb3};
use anyhow::{ensure, Context, Result};
use glam::UVec3;
use std::{collections::BTreeMap, sync::Arc};

const CHECKPOINT_BUDGET_BYTES: usize = 256 * 1024 * 1024;

pub(super) struct TreeAtlasCheckpoint {
    regions: Vec<(UVec3, UVec3, Arc<[u8]>)>,
}
impl TreeAtlasCheckpoint {
    pub(super) fn capture(
        bounds: &[UAabb3],
        world_dim: UVec3,
        mut read: impl FnMut(UVec3, UVec3) -> Result<Vec<u8>>,
    ) -> Result<Self> {
        let mut regions = BTreeMap::new();
        for &bound in bounds {
            let origin = bound.min().min(world_dim);
            // Inclusive author bounds; capture every dispatched boundary cell.
            let max = bound.max().saturating_add(UVec3::ONE).min(world_dim);
            let dim = max.saturating_sub(origin);
            if dim.min_element() != 0 {
                regions.insert((origin.to_array(), dim.to_array()), (origin, dim));
            }
        }
        let mut bytes = 0usize;
        for (_, dim) in regions.values() {
            let count = dim
                .to_array()
                .into_iter()
                .try_fold(1usize, |n, d| n.checked_mul(d as usize))
                .context("tree atlas checkpoint size overflow")?;
            bytes = bytes
                .checked_add(count)
                .context("tree atlas checkpoint total overflow")?;
        }
        ensure!(
            bytes <= CHECKPOINT_BUDGET_BYTES,
            "tree atlas checkpoint exceeds memory budget"
        );
        let mut captured = Vec::with_capacity(regions.len());
        for (_, (origin, dim)) in regions {
            let data = read(origin, dim)?;
            let count = dim.x as usize * dim.y as usize * dim.z as usize;
            ensure!(data.len() == count, "incomplete tree atlas checkpoint read");
            captured.push((origin, dim, Arc::from(data)));
        }
        Ok(Self { regions: captured })
    }

    pub(super) fn restore_edits(&self) -> Vec<VoxelEdit> {
        self.regions
            .iter()
            .map(|(origin, dim, data)| VoxelEdit::RestoreAtlasRegion {
                origin: *origin,
                dim: *dim,
                data: data.clone(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restore_preserves_empty_cuts_other_materials_and_state_bits_not_just_authored_wood() {
        let original = vec![0, 0x21, 0x35, 0x41, 0x65, 3, 5, 0];
        let bound = UAabb3::new(UVec3::ZERO, UVec3::ONE);
        let checkpoint =
            TreeAtlasCheckpoint::capture(&[bound, bound], UVec3::splat(2), |origin, dim| {
                assert_eq!(origin, UVec3::ZERO);
                assert_eq!(dim, UVec3::splat(2));
                Ok(original.clone())
            })
            .unwrap();
        let edits = checkpoint.restore_edits();
        assert_eq!(edits.len(), 1); // Identical old/retained/new read boxes do not duplicate memory.
        let VoxelEdit::RestoreAtlasRegion { origin, dim, data } = &edits[0] else {
            panic!("not exact restoration")
        };
        assert_eq!(*origin, UVec3::ZERO);
        assert_eq!(*dim, UVec3::splat(2));
        assert_eq!(data.as_ref(), original);
    }
    #[test]
    fn clipped_and_failed_reads_cannot_publish_a_partial_checkpoint() {
        let bound = UAabb3::new(UVec3::ONE, UVec3::splat(99));
        assert!(
            TreeAtlasCheckpoint::capture(&[bound], UVec3::splat(2), |_, _| Ok(vec![])).is_err()
        );
        assert!(
            TreeAtlasCheckpoint::capture(&[bound], UVec3::splat(2), |_, _| anyhow::bail!(
                "read failed"
            ))
            .is_err()
        );
        assert!(TreeAtlasCheckpoint::capture(
            &[UAabb3::new(UVec3::ZERO, UVec3::splat(1024))],
            UVec3::splat(1024),
            |_, _| panic!("budget must be checked before reads")
        )
        .is_err());
    }
}
