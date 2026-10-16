//! One packed transfer for independent R8 atlas regions, preserving request order.
use anyhow::{ensure, Context, Result};
use glam::UVec3;
use re_flora_vkn::{Extent3D, TextureRegion};

pub(super) struct AtlasReadPlan {
    pub copies: Vec<(u64, TextureRegion)>,
    pub byte_count: u64,
    ranges: Vec<(u64, u64)>,
}

impl AtlasReadPlan {
    pub fn new(regions: &[(UVec3, UVec3)], atlas_size: UVec3) -> Result<Self> {
        let mut plan = Self {
            copies: Vec::with_capacity(regions.len()),
            byte_count: 0,
            ranges: Vec::with_capacity(regions.len()),
        };
        for &(offset, dim) in regions {
            // Preserve the single-region interface's empty-read behavior.
            if dim.cmpeq(UVec3::ZERO).any() {
                plan.ranges.push((0, 0));
                continue;
            }
            ensure!(offset.cmple(atlas_size).all() && dim.cmple(atlas_size - offset).all(),
                "chunk atlas read outside bounds: offset={offset:?} dim={dim:?} atlas={atlas_size:?}");
            let count = u64::from(dim.x)
                .checked_mul(u64::from(dim.y))
                .and_then(|xy| xy.checked_mul(u64::from(dim.z)))
                .context("chunk atlas read size overflow")?;
            // Four-byte alignment is safe for the R8 texel format and transfer copies.
            let start = plan
                .byte_count
                .checked_add(3)
                .context("atlas read alignment overflow")?
                & !3;
            plan.byte_count = start
                .checked_add(count)
                .context("atlas read buffer size overflow")?;
            let image_offset = [
                i32::try_from(offset.x)?,
                i32::try_from(offset.y)?,
                i32::try_from(offset.z)?,
            ];
            plan.copies.push((
                start,
                TextureRegion {
                    offset: image_offset,
                    extent: Extent3D::new(dim.x, dim.y, dim.z),
                },
            ));
            plan.ranges.push((start, count));
        }
        Ok(plan)
    }

    /// Host reads happen only after the one transfer has completed. Empty entries
    /// require no mapped access, and padding is never returned to callers.
    pub fn read_results(
        &self,
        mut read: impl FnMut(u64, u64) -> Result<Vec<u8>>,
    ) -> Result<Vec<Vec<u8>>> {
        self.ranges
            .iter()
            .map(|&(offset, len)| {
                if len == 0 {
                    Ok(Vec::new())
                } else {
                    read(offset, len)
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_reads_preserve_overlaps_order_and_empty_entries() {
        let size = UVec3::new(8, 4, 2);
        let requests = [
            (UVec3::new(1, 2, 0), UVec3::new(3, 1, 1)),
            (UVec3::new(2, 2, 0), UVec3::new(3, 2, 2)),
            (UVec3::MAX, UVec3::new(0, 3, 1)),
            (UVec3::new(7, 3, 1), UVec3::ONE),
        ];
        let plan = AtlasReadPlan::new(&requests, size).unwrap();
        assert_eq!(plan.copies.len(), 3);
        let mut mapped = vec![0xff; plan.byte_count as usize];
        let sample = |p: UVec3| ((p.z * size.y + p.y) * size.x + p.x) as u8;
        for (start, region) in &plan.copies {
            assert_eq!(start % 4, 0);
            let mut at = *start as usize;
            for z in 0..region.extent.depth {
                for y in 0..region.extent.height {
                    for x in 0..region.extent.width {
                        let p = UVec3::new(
                            region.offset[0] as u32 + x,
                            region.offset[1] as u32 + y,
                            region.offset[2] as u32 + z,
                        );
                        mapped[at] = sample(p);
                        at += 1;
                    }
                }
            }
        }
        let results = plan
            .read_results(|offset, count| {
                Ok(mapped[offset as usize..(offset + count) as usize].to_vec())
            })
            .unwrap();
        let expected: Vec<Vec<u8>> = requests
            .into_iter()
            .map(|(offset, dim)| {
                (0..dim.z)
                    .flat_map(|z| {
                        (0..dim.y).flat_map(move |y| {
                            (0..dim.x).map(move |x| sample(offset + UVec3::new(x, y, z)))
                        })
                    })
                    .collect()
            })
            .collect();
        assert_eq!(results, expected);
    }

    #[test]
    fn validates_every_region_before_any_transfer_and_rejects_wrapping_bounds() {
        let valid = (UVec3::ZERO, UVec3::ONE);
        for invalid in [
            (UVec3::splat(8), UVec3::ONE),
            (UVec3::splat(7), UVec3::splat(2)),
            (UVec3::MAX, UVec3::splat(2)),
        ] {
            assert!(AtlasReadPlan::new(&[valid, invalid], UVec3::splat(8)).is_err());
        }
        assert!(AtlasReadPlan::new(&[(UVec3::ZERO, UVec3::MAX)], UVec3::MAX).is_err());
    }

    #[test]
    fn empty_batches_need_neither_transfer_nor_host_reads() {
        for requests in [vec![], vec![(UVec3::MAX, UVec3::ZERO); 3]] {
            let plan = AtlasReadPlan::new(&requests, UVec3::ONE).unwrap();
            assert!(plan.copies.is_empty());
            assert_eq!(plan.byte_count, 0);
            assert_eq!(
                plan.read_results(|_, _| panic!("empty read")).unwrap(),
                vec![Vec::<u8>::new(); requests.len()]
            );
        }
    }
}
