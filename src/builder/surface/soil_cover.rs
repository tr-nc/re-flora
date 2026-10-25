//! Derived soil appearance. Ordinary live grass roots are the only authority;
//! atlas bit 6 caches coverage, never a new material or a plant ownership flag.
use super::*;
use crate::builder::{PlainBuilder, VOXEL_TYPE_DIRT, VOXEL_TYPE_MASK};
use std::collections::HashMap;

use crate::builder::VOXEL_GRASS_COVER_BIT as GRASS_SOIL_COVER_BIT;
const COVER_RADIUS: i32 = 4;
const COVER_HEIGHT: i32 = 1;

type Regions = HashMap<UVec3, UAabb3>;

#[derive(Default)]
pub(super) struct SoilCover {
    previous: Regions,
    pub dirty: bool,
}

fn include_region(regions: &mut Regions, chunk: UVec3, min: UVec3, max: UVec3) {
    regions
        .entry(chunk)
        .and_modify(|b| *b = UAabb3::new(b.min().min(min), b.max().max(max)))
        .or_insert_with(|| UAabb3::new(min, max));
}

fn root_regions(roots: &[UVec3], chunk_dim: UVec3, extent: UVec3) -> Regions {
    let mut regions = Regions::new();
    for &root in roots {
        let min = root.saturating_sub(UVec3::new(
            COVER_RADIUS as u32,
            COVER_HEIGHT as u32,
            COVER_RADIUS as u32,
        ));
        // Include headroom for every candidate, including at a chunk boundary.
        let max = (root + UVec3::new(COVER_RADIUS as u32, 1, COVER_RADIUS as u32))
            .min(extent - UVec3::ONE);
        let first = min / chunk_dim;
        let last = max / chunk_dim;
        for z in first.z..=last.z {
            for y in first.y..=last.y {
                for x in first.x..=last.x {
                    let chunk = UVec3::new(x, y, z);
                    let origin = chunk * chunk_dim;
                    include_region(
                        &mut regions,
                        chunk,
                        min.max(origin),
                        max.min(origin + chunk_dim - UVec3::ONE),
                    );
                }
            }
        }
    }
    regions
}

struct SoilRegion {
    bounds: UAabb3,
    bytes: Vec<u8>,
}
impl SoilRegion {
    fn index(&self, p: UVec3) -> Option<usize> {
        if p.cmplt(self.bounds.min()).any() || p.cmpgt(self.bounds.max()).any() {
            return None;
        }
        let p = p - self.bounds.min();
        let dim = self.bounds.max() - self.bounds.min() + UVec3::ONE;
        Some((p.x + dim.x * (p.y + dim.y * p.z)) as usize)
    }
}

fn exposed_dirt(
    regions: &HashMap<UVec3, SoilRegion>,
    p: UVec3,
    chunk_dim: UVec3,
    extent: UVec3,
) -> bool {
    let sample = |p: UVec3| {
        regions
            .get(&(p / chunk_dim))
            .and_then(|r| r.index(p).map(|i| r.bytes[i] & VOXEL_TYPE_MASK))
    };
    sample(p) == Some(VOXEL_TYPE_DIRT as u8)
        && (p.y + 1 == extent.y || sample(p + UVec3::Y) == Some(0))
}

fn apply_cover(
    regions: &mut HashMap<UVec3, SoilRegion>,
    roots: &[UVec3],
    chunk_dim: UVec3,
    extent: UVec3,
) {
    for region in regions.values_mut() {
        for byte in &mut region.bytes {
            if *byte & VOXEL_TYPE_MASK == VOXEL_TYPE_DIRT as u8 {
                *byte &= !GRASS_SOIL_COVER_BIT;
            }
        }
    }
    for &root in roots {
        // One layer of cover already hides the soil, independently of the
        // separate three-layer grass survival rule.
        if !exposed_dirt(regions, root, chunk_dim, extent) {
            continue;
        }
        for dz in -COVER_RADIUS..=COVER_RADIUS {
            for dx in -COVER_RADIUS..=COVER_RADIUS {
                if dx * dx + dz * dz > COVER_RADIUS * COVER_RADIUS {
                    continue;
                }
                // Coverage can follow a descending surface, but cannot climb
                // onto newly deposited soil above the living root's support.
                for dy in -COVER_HEIGHT..=0 {
                    let p = root.as_ivec3() + IVec3::new(dx, dy, dz);
                    if p.cmplt(IVec3::ZERO).any() || p.cmpge(extent.as_ivec3()).any() {
                        continue;
                    }
                    let p = p.as_uvec3();
                    if exposed_dirt(regions, p, chunk_dim, extent) {
                        let region = regions.get_mut(&(p / chunk_dim)).unwrap();
                        let i = region.index(p).unwrap();
                        region.bytes[i] |= GRASS_SOIL_COVER_BIT;
                    }
                }
            }
        }
    }
}

impl SurfaceBuilder {
    /// Called before rendering, after flora/terrain publication. Read all source
    /// regions together before writing: neighboring chunks never erase each
    /// other's contribution, and moisture is preserved byte-for-byte.
    pub fn refresh_grass_soil_cover(&mut self, plain: &mut PlainBuilder) -> Result<()> {
        if !self.soil_cover.dirty
            && !self
                .resources
                .instances
                .chunk_flora_instances
                .iter()
                .any(|(_, r)| r.soil_cover_dirty)
        {
            return Ok(());
        }
        let started = Instant::now();
        let roots: Vec<_> = self
            .resources
            .instances
            .chunk_flora_instances
            .iter()
            .flat_map(|(_, r)| {
                r.hydration[..species::MODEL_FLOWER_FIRST_SPECIES as usize]
                    .iter()
                    .flatten()
                    .map(move |&(root, _)| {
                        r.chunk_world_offset
                            + UVec3::new(root & 255, (root >> 8) & 255, (root >> 16) & 255)
                    })
            })
            .collect();
        let extent = self.chunk_bound.max() * self.voxel_dim_per_chunk;
        let next = root_regions(&roots, self.voxel_dim_per_chunk, extent);
        let mut affected = next.clone();
        for (&chunk, b) in &self.soil_cover.previous {
            include_region(&mut affected, chunk, b.min(), b.max());
        }
        let plans: Vec<_> = affected.iter().map(|(&chunk, b)| (chunk, *b)).collect();
        let reads: Vec<_> = plans
            .iter()
            .map(|(_, b)| (b.min(), b.max() - b.min() + UVec3::ONE))
            .collect();
        let bytes = if reads.is_empty() {
            Vec::new()
        } else {
            plain.read_chunk_atlas_regions(&reads)?
        };
        let mut regions: HashMap<_, _> = plans
            .iter()
            .zip(bytes)
            .map(|(&(chunk, bounds), bytes)| (chunk, SoilRegion { bounds, bytes }))
            .collect();
        let previous: HashMap<_, _> = regions
            .iter()
            .map(|(&chunk, r)| (chunk, r.bytes.clone()))
            .collect();
        apply_cover(&mut regions, &roots, self.voxel_dim_per_chunk, extent);
        let mut written = 0;
        for (chunk, r) in &regions {
            if r.bytes != previous[chunk] {
                plain.write_chunk_atlas_region(
                    r.bounds.min(),
                    r.bounds.max() - r.bounds.min() + UVec3::ONE,
                    &r.bytes,
                )?;
                written += r.bytes.len();
            }
        }
        self.soil_cover.previous = next;
        self.soil_cover.dirty = false;
        for (_, r) in &mut self.resources.instances.chunk_flora_instances {
            r.soil_cover_dirty = false;
        }
        log::debug!(
            "[SOIL_COVER] roots={} regions={} uploaded_bytes={} elapsed_ms={:.3}",
            roots.len(),
            regions.len(),
            written,
            started.elapsed().as_secs_f64() * 1000.
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn terrain(dim: UVec3) -> HashMap<UVec3, SoilRegion> {
        let bounds = UAabb3::new(UVec3::ZERO, dim - UVec3::ONE);
        let mut bytes = vec![0; dim.element_product() as usize];
        for z in 0..dim.z {
            for x in 0..dim.x {
                bytes[(x + dim.x * z * dim.y) as usize] = 0x32;
            }
        }
        HashMap::from([(UVec3::ZERO, SoilRegion { bounds, bytes })])
    }
    fn byte(regions: &HashMap<UVec3, SoilRegion>, p: UVec3, dim: UVec3) -> u8 {
        let r = &regions[&(p / dim)];
        r.bytes[r.index(p).unwrap()]
    }
    #[test]
    fn grass_patch_overlap_removal_and_moisture() {
        let dim = UVec3::splat(16);
        let mut regions = terrain(dim);
        let a = UVec3::new(5, 0, 5);
        let b = UVec3::new(9, 0, 5);
        apply_cover(&mut regions, &[a, b], dim, dim);
        assert_eq!(byte(&regions, a + UVec3::X * 2, dim), 0x72);
        assert_eq!(byte(&regions, UVec3::new(15, 0, 15), dim), 0x32);
        apply_cover(&mut regions, &[b], dim, dim);
        assert_eq!(byte(&regions, a, dim), 0x72);
        assert_eq!(byte(&regions, a - UVec3::X, dim), 0x32);
        apply_cover(&mut regions, &[], dim, dim);
        assert!(regions[&UVec3::ZERO]
            .bytes
            .iter()
            .all(|b| b & GRASS_SOIL_COVER_BIT == 0));
    }
    #[test]
    fn cover_requires_exposed_dirt_at_root_and_destination() {
        let dim = UVec3::splat(16);
        let mut regions = terrain(dim);
        let root = UVec3::new(5, 0, 5);
        let p = root + UVec3::X;
        let r = regions.get_mut(&UVec3::ZERO).unwrap();
        let above = r.index(p + UVec3::Y).unwrap();
        r.bytes[above] = 2;
        apply_cover(&mut regions, &[root], dim, dim);
        assert_eq!(byte(&regions, p, dim), 0x32);
        assert_eq!(byte(&regions, p + UVec3::Y, dim), 2);
        let r = regions.get_mut(&UVec3::ZERO).unwrap();
        let above = r.index(root + UVec3::Y).unwrap();
        r.bytes[above] = 2;
        apply_cover(&mut regions, &[root], dim, dim);
        assert_eq!(byte(&regions, root, dim), 0x32);
        assert_eq!(byte(&regions, root - UVec3::X, dim), 0x32);
    }
    #[test]
    fn other_materials_and_fixed_scene_flags_are_unchanged() {
        let dim = UVec3::splat(16);
        for material in [0x33, 0x87, 0x85, 0x01] {
            let mut regions = terrain(dim);
            let root = UVec3::new(5, 0, 5);
            let r = regions.get_mut(&UVec3::ZERO).unwrap();
            let index = r.index(root + UVec3::X).unwrap();
            r.bytes[index] = material;
            apply_cover(&mut regions, &[root], dim, dim);
            assert_eq!(byte(&regions, root + UVec3::X, dim), material);
            let r = regions.get_mut(&UVec3::ZERO).unwrap();
            let index = r.index(root).unwrap();
            r.bytes[index] = material;
            apply_cover(&mut regions, &[root], dim, dim);
            assert_eq!(byte(&regions, root - UVec3::X, dim), 0x32);
        }
    }

    #[test]
    fn footprint_and_headroom_cross_chunk_boundaries() {
        let chunk = UVec3::splat(8);
        let extent = UVec3::splat(16);
        let root = UVec3::new(7, 7, 7);
        let regions = root_regions(&[root], chunk, extent);
        assert_eq!(regions.len(), 8);
        assert_eq!(regions[&UVec3::ONE].max(), UVec3::new(11, 8, 11));
    }
}
