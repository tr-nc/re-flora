//! Immutable, derived grass sound coverage. Never owns editable vegetation.
use glam::{UVec3, Vec3};
use std::{collections::BTreeMap, sync::Arc};

pub(crate) const PATCH_VOXELS: u32 = 64;
pub(crate) const MAX_CANDIDATES: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct PatchId(pub [u32; 3]);

#[derive(Clone, Debug)]
pub(crate) struct GrassSample {
    pub id: u64,
    pub position: Vec3,
    pub mass: f32,
}
#[derive(Clone, Debug)]
pub(crate) struct GrassPatch {
    pub id: PatchId,
    pub samples: Vec<GrassSample>,
}
impl GrassPatch {
    pub fn center(&self) -> Vec3 {
        (UVec3::from_array(self.id.0).as_vec3() + Vec3::splat(0.5)) * PATCH_VOXELS as f32 / 256.0
    }
}

fn hash(mut value: u32) -> u32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846ca68b);
    value ^ (value >> 16)
}

/// CPU counterpart of sampleGrassHeight / grassGrownHeight. Seed is at the stem,
/// one voxel above the stored supporting root, exactly as the rendering path.
pub(crate) fn visible_height(
    root: UVec3,
    short: bool,
    progress: u32,
    potential: f32,
    hydration: f32,
    growth_override: Option<f32>,
) -> f32 {
    let stem = root + UVec3::Y;
    let mut seed = stem.x ^ (stem.y << 10) ^ (stem.z << 20);
    seed ^= seed >> 16;
    seed ^= seed << 5;
    seed ^= seed >> 11;
    let uniform = |v| f32::from_bits((hash(v) & 0x007fffff) | 0x3f800000) - 1.0;
    let normal = (uniform(seed ^ 0xA511E9B3) + uniform(seed ^ 0x63D83595) - 1.0) * 2.449_489_8;
    let height = if short {
        (3.0 + normal * 0.6).clamp(2.0, 4.0)
    } else {
        (5.0 + normal).clamp(3.0, 8.0)
    }
    .round();
    let level = hydration.clamp(0.0, 1.0) * 3.0;
    let lower = level.floor() as usize;
    let factors = super::species::species()[usize::from(short)].moisture_growth_factors;
    let environment =
        factors[lower] + (factors[(lower + 1).min(3)] - factors[lower]) * level.fract();
    let limit = growth_override
        .unwrap_or(potential.min(environment))
        .clamp(0.0, 1.0);
    let growth = (progress.min(255) as f32 / 255.0).min(limit);
    (height * growth + 0.001).floor()
}

#[derive(Default)]
struct Bin {
    mass: f32,
    // A deterministic real root, not a centroid in an empty part of the tile.
    representative: Option<(u32, UVec3, f32)>,
}
#[derive(Default)]
pub(crate) struct PatchBuilder(BTreeMap<PatchId, [Bin; 4]>);
impl PatchBuilder {
    pub fn add(&mut self, root: UVec3, height: f32) {
        if height <= 0.0 {
            return;
        }
        let id = PatchId((root / PATCH_VOXELS).to_array());
        let local = root % PATCH_VOXELS;
        let bin = (local.x / (PATCH_VOXELS / 2) + 2 * (local.z / (PATCH_VOXELS / 2))) as usize;
        let bins = self.0.entry(id).or_default();
        let b = &mut bins[bin];
        b.mass += height / 8.0;
        let score = hash(root.x ^ root.y.wrapping_mul(31337) ^ root.z.wrapping_mul(9187));
        if b.representative
            .is_none_or(|(old, position, _)| (score, root.to_array()) < (old, position.to_array()))
        {
            b.representative = Some((score, root, height));
        }
    }
    pub fn finish(self) -> Vec<Arc<GrassPatch>> {
        self.0
            .into_iter()
            .map(|(id, bins)| {
                Arc::new(GrassPatch {
                    id,
                    samples: bins
                        .into_iter()
                        .enumerate()
                        .filter_map(|(id, b)| {
                            b.representative.map(|(_, root, height)| GrassSample {
                                id: id as u64,
                                position: (root.as_vec3()
                                    + Vec3::new(0.5, 1.0 + height * 0.5, 0.5))
                                    / 256.0,
                                // Saturation is local, independent of how many other patches play.
                                mass: b.mass.min(256.0),
                            })
                        })
                        .collect(),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_roots_stable_order_vertical_separation_and_removal() {
        let roots = [
            UVec3::new(1, 2, 3),
            UVec3::new(35, 2, 35),
            UVec3::new(1, 90, 3),
        ];
        let build = |roots: &[UVec3]| {
            let mut b = PatchBuilder::default();
            for &r in roots {
                b.add(r, 4.0);
            }
            b.finish()
        };
        let a = build(&roots);
        let b = build(&roots.into_iter().rev().collect::<Vec<_>>());
        assert_eq!(a.len(), 2);
        assert_eq!(a[0].samples.len(), 2);
        for (a, b) in a.iter().zip(&b) {
            assert_eq!(a.id, b.id);
            for (a, b) in a.samples.iter().zip(&b.samples) {
                assert_eq!(a.position, b.position);
                assert_eq!(a.mass, b.mass);
            }
        }
        assert_eq!(build(&roots[..2]).len(), 1);
        assert!(build(&[]).is_empty());
        let mut seed = PatchBuilder::default();
        seed.add(roots[0], 0.0);
        assert!(seed.finish().is_empty());
    }
    #[test]
    fn grass_shader_geometry_contract() {
        let mut checksum = 0u32;
        for x in 0..64 {
            let root = UVec3::new(x * 7, x * 3 + 1, x * 11);
            for short in [false, true] {
                for progress in [0, 32, 64, 128, 255] {
                    for potential in [0.0, 0.3, 1.0] {
                        let h = visible_height(root, short, progress, potential, 1.0, None);
                        checksum = checksum.wrapping_mul(31).wrapping_add(h as u32);
                    }
                }
            }
        }
        // Independently executed by shader/tests/grass_acoustic_geometry.slang.
        assert_eq!(checksum, 192_770_237);
    }
    #[test]
    fn visible_mass_respects_seed_mowing_competition_water_and_override() {
        for root in [UVec3::ZERO, UVec3::new(103, 47, 299), UVec3::splat(511)] {
            for short in [false, true] {
                let height = |p, c, h, o| visible_height(root, short, p, c, h, o);
                assert_eq!(height(0, 1.0, 1.0, None), 0.0);
                assert_eq!(height(255, 1.0, 1.0, Some(0.0)), 0.0);
                assert!(height(32, 1.0, 1.0, None) < height(255, 1.0, 1.0, None));
                assert!(height(255, 0.2, 1.0, None) < height(255, 1.0, 1.0, None));
                assert!(height(255, 1.0, 0.0, None) <= height(255, 1.0, 1.0, None));
                assert_eq!(
                    height(255, 0.0, 0.0, Some(1.0)),
                    height(255, 1.0, 1.0, None)
                );
            }
        }
    }
}
