use crate::{
    geom::{RoundCone, RoundConeClearanceIndex},
    tree_gen::LeafPlacement,
};
use glam::Vec3;
use std::cmp::Ordering;

const CANOPY_CONTENT_SEED_DOMAIN: u64 = 0x6c65_6166_5f61_7564;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CanopyAcousticSampleId(u64);

impl CanopyAcousticSampleId {
    pub fn value(self) -> u64 {
        self.0
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanopyAcousticSampleProvenance {
    LeafPlacement,
    ExtrapolatedLeafSprayFallback,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CanopyAcousticSample {
    id: CanopyAcousticSampleId,
    position_tree_voxels: Vec3,
    clearance_voxels: f32,
    weight: f32,
    content_seed: u64,
    phase: f32,
    provenance: CanopyAcousticSampleProvenance,
}

impl CanopyAcousticSample {
    pub fn id(&self) -> CanopyAcousticSampleId {
        self.id
    }

    pub fn position_tree_voxels(&self) -> Vec3 {
        self.position_tree_voxels
    }

    pub fn clearance_voxels(&self) -> f32 {
        self.clearance_voxels
    }

    pub fn weight(&self) -> f32 {
        self.weight
    }

    pub fn content_seed(&self) -> u64 {
        self.content_seed
    }

    pub fn phase(&self) -> f32 {
        self.phase
    }

    pub fn provenance(&self) -> CanopyAcousticSampleProvenance {
        self.provenance
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CanopyAcousticDescriptor {
    generation: u64,
    tree_origin_world: Vec3,
    tree_seed: u64,
    content_seed: u64,
    phase: f32,
    samples: Vec<CanopyAcousticSample>,
}

impl CanopyAcousticDescriptor {
    pub const MAX_SAMPLES: usize = 8;
    /// This is conservatively above PetalSonic 0.7's 0.853-voxel source endpoint epsilon plus a
    /// one-voxel safety margin for Re: Flora's voxelized tree geometry.
    pub const MIN_WOOD_CLEARANCE_VOXELS: f32 = 2.0;

    pub fn build(
        generation: u64,
        tree_origin_world: Vec3,
        tree_seed: u64,
        leaf_placements: &[LeafPlacement],
        trunks: &[RoundCone],
    ) -> Self {
        let mut candidates = leaf_placements
            .iter()
            .copied()
            .filter(|leaf| leaf.position.is_finite() && leaf.anchor.is_finite())
            .collect::<Vec<_>>();
        candidates.sort_by(compare_leaf_placements);

        let samples = if candidates.is_empty() {
            Vec::new()
        } else {
            let center = canopy_center(&candidates);
            let clearance_index = RoundConeClearanceIndex::new(trunks);
            let mut sector_counts = [0_usize; Self::MAX_SAMPLES];
            for leaf in &candidates {
                sector_counts[octant(leaf.position, center)] += 1;
            }

            let mut ranked_candidates = candidates.clone();
            ranked_candidates.sort_by(|left, right| {
                right
                    .position
                    .distance_squared(center)
                    .total_cmp(&left.position.distance_squared(center))
                    .then_with(|| compare_leaf_placements(left, right))
            });
            let best_global_candidate = || {
                ranked_candidates.iter().copied().find(|leaf| {
                    clearance_index
                        .has_minimum_clearance(leaf.position, Self::MIN_WOOD_CLEARANCE_VOXELS)
                })
            };
            let mut selected = Vec::<CanopyAcousticSample>::new();

            for (sector, population) in sector_counts.into_iter().enumerate() {
                if population == 0 {
                    continue;
                }
                let Some(leaf) = ranked_candidates
                    .iter()
                    .copied()
                    .filter(|leaf| octant(leaf.position, center) == sector)
                    .find(|leaf| {
                        clearance_index
                            .has_minimum_clearance(leaf.position, Self::MIN_WOOD_CLEARANCE_VOXELS)
                    })
                    .or_else(|| best_global_candidate())
                else {
                    continue;
                };
                let id = sample_id(tree_seed, leaf.position);
                let weight = population as f32 / candidates.len() as f32;
                if let Some(existing) = selected.iter_mut().find(|sample| sample.id == id) {
                    existing.weight += weight;
                    continue;
                }
                let content_seed = mix_u64(CANOPY_CONTENT_SEED_DOMAIN ^ tree_seed ^ id.0);
                selected.push(CanopyAcousticSample {
                    id,
                    position_tree_voxels: leaf.position,
                    clearance_voxels: clearance_index.minimum_clearance(leaf.position),
                    weight,
                    content_seed,
                    phase: unit_from_u64(mix_u64(content_seed ^ 0x7068_6173_655f_3031)),
                    provenance: CanopyAcousticSampleProvenance::LeafPlacement,
                });
            }
            if selected.is_empty() {
                if let Some((position, clearance)) =
                    clear_leaf_spray_fallback(&candidates, trunks, &clearance_index, center)
                {
                    let id = sample_id(tree_seed, position);
                    let content_seed = mix_u64(CANOPY_CONTENT_SEED_DOMAIN ^ tree_seed ^ id.0);
                    selected.push(CanopyAcousticSample {
                        id,
                        position_tree_voxels: position,
                        clearance_voxels: clearance,
                        weight: 1.0,
                        content_seed,
                        phase: unit_from_u64(mix_u64(content_seed ^ 0x7068_6173_655f_3031)),
                        provenance: CanopyAcousticSampleProvenance::ExtrapolatedLeafSprayFallback,
                    });
                }
            }
            selected.sort_by_key(|sample| sample.id);
            selected
        };

        let content_seed = mix_u64(CANOPY_CONTENT_SEED_DOMAIN ^ tree_seed);
        Self {
            generation,
            tree_origin_world,
            tree_seed,
            content_seed,
            phase: unit_from_u64(mix_u64(content_seed ^ 0x766f_6963_655f_7068)),
            samples,
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn tree_origin_world(&self) -> Vec3 {
        self.tree_origin_world
    }

    #[allow(dead_code)]
    pub fn tree_seed(&self) -> u64 {
        self.tree_seed
    }

    pub fn content_seed(&self) -> u64 {
        self.content_seed
    }

    pub fn phase(&self) -> f32 {
        self.phase
    }

    pub fn samples(&self) -> &[CanopyAcousticSample] {
        &self.samples
    }

    #[allow(dead_code)]
    pub fn sample_world_position(&self, sample: &CanopyAcousticSample) -> Vec3 {
        self.tree_origin_world + sample.position_tree_voxels / 256.0
    }

    pub fn total_weight(&self) -> f32 {
        self.samples.iter().map(CanopyAcousticSample::weight).sum()
    }
}

fn compare_leaf_placements(left: &LeafPlacement, right: &LeafPlacement) -> Ordering {
    [
        left.position.x.total_cmp(&right.position.x),
        left.position.y.total_cmp(&right.position.y),
        left.position.z.total_cmp(&right.position.z),
        left.anchor.x.total_cmp(&right.anchor.x),
        left.anchor.y.total_cmp(&right.anchor.y),
        left.anchor.z.total_cmp(&right.anchor.z),
    ]
    .into_iter()
    .find(|ordering| *ordering != Ordering::Equal)
    .unwrap_or(Ordering::Equal)
}

fn canopy_center(candidates: &[LeafPlacement]) -> Vec3 {
    let mut minimum = Vec3::splat(f32::INFINITY);
    let mut maximum = Vec3::splat(f32::NEG_INFINITY);
    for leaf in candidates {
        minimum = minimum.min(leaf.position);
        maximum = maximum.max(leaf.position);
    }
    (minimum + maximum) * 0.5
}

fn octant(position: Vec3, center: Vec3) -> usize {
    usize::from(position.x >= center.x)
        | (usize::from(position.y >= center.y) << 1)
        | (usize::from(position.z >= center.z) << 2)
}

fn clear_leaf_spray_fallback(
    candidates: &[LeafPlacement],
    trunks: &[RoundCone],
    clearance_index: &RoundConeClearanceIndex<'_>,
    center: Vec3,
) -> Option<(Vec3, f32)> {
    let mut sector_candidates = Vec::new();
    for sector in 0..CanopyAcousticDescriptor::MAX_SAMPLES {
        let candidate = candidates
            .iter()
            .filter(|leaf| octant(leaf.position, center) == sector)
            .max_by(|left, right| {
                left.position
                    .distance_squared(center)
                    .total_cmp(&right.position.distance_squared(center))
                    .then_with(|| compare_leaf_placements(right, left))
            })
            .copied();
        if let Some(candidate) = candidate {
            sector_candidates.push(candidate);
        }
    }

    sector_candidates
        .into_iter()
        .filter_map(|leaf| {
            let direction = fallback_direction(leaf, center);
            let limit = fallback_search_limit(leaf.position, trunks);
            let mut low = 0.0_f32;
            let mut high = 1.0_f32.min(limit);
            while high < limit
                && !clearance_index.has_minimum_clearance(
                    leaf.position + direction * high,
                    CanopyAcousticDescriptor::MIN_WOOD_CLEARANCE_VOXELS,
                )
            {
                low = high;
                high = (high * 2.0).min(limit);
            }

            if !clearance_index.has_minimum_clearance(
                leaf.position + direction * high,
                CanopyAcousticDescriptor::MIN_WOOD_CLEARANCE_VOXELS,
            ) {
                return None;
            }
            for _ in 0..20 {
                let middle = (low + high) * 0.5;
                if clearance_index.has_minimum_clearance(
                    leaf.position + direction * middle,
                    CanopyAcousticDescriptor::MIN_WOOD_CLEARANCE_VOXELS,
                ) {
                    high = middle;
                } else {
                    low = middle;
                }
            }
            let position = leaf.position + direction * high;
            Some((
                leaf,
                high,
                position,
                clearance_index.minimum_clearance(position),
            ))
        })
        .min_by(|(left, left_distance, ..), (right, right_distance, ..)| {
            left_distance
                .total_cmp(right_distance)
                .then_with(|| compare_leaf_placements(left, right))
        })
        .map(|(_, _, position, clearance)| (position, clearance))
}

fn fallback_direction(leaf: LeafPlacement, center: Vec3) -> Vec3 {
    let from_center = (leaf.position - center).normalize_or_zero();
    if from_center.length_squared() > 0.0 {
        return from_center;
    }
    let along_spray = (leaf.position - leaf.anchor).normalize_or_zero();
    if along_spray.length_squared() > 0.0 {
        return along_spray;
    }

    let sector = octant(leaf.position, center);
    Vec3::new(
        if sector & 1 == 0 { -1.0 } else { 1.0 },
        if sector & 2 == 0 { -1.0 } else { 1.0 },
        if sector & 4 == 0 { -1.0 } else { 1.0 },
    )
    .normalize()
}

fn fallback_search_limit(position: Vec3, trunks: &[RoundCone]) -> f32 {
    trunks
        .iter()
        .map(|trunk| {
            let bound = trunk.aabb();
            Vec3::new(
                (position.x - bound.min().x)
                    .abs()
                    .max((position.x - bound.max().x).abs()),
                (position.y - bound.min().y)
                    .abs()
                    .max((position.y - bound.max().y).abs()),
                (position.z - bound.min().z)
                    .abs()
                    .max((position.z - bound.max().z).abs()),
            )
            .length()
        })
        .fold(0.0_f32, f32::max)
        + CanopyAcousticDescriptor::MIN_WOOD_CLEARANCE_VOXELS
        + 1.0
}

fn sample_id(tree_seed: u64, position: Vec3) -> CanopyAcousticSampleId {
    let mut value = mix_u64(tree_seed ^ 0x7361_6d70_6c65_5f69);
    value = mix_u64(value ^ u64::from(position.x.to_bits()));
    value = mix_u64(value ^ (u64::from(position.y.to_bits()) << 1));
    value = mix_u64(value ^ (u64::from(position.z.to_bits()) << 2));
    CanopyAcousticSampleId(value)
}

fn mix_u64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn unit_from_u64(value: u64) -> f32 {
    ((value >> 40) as f32) / ((1_u32 << 24) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        geom::RoundCone,
        tree_gen::{LeafPlacement, Tree, TreeDesc},
    };
    use glam::Vec3;
    use std::collections::HashSet;

    fn octant_leaf_placements() -> Vec<LeafPlacement> {
        [-6.0, 6.0]
            .into_iter()
            .flat_map(|x| {
                [-6.0, 6.0].into_iter().flat_map(move |y| {
                    [-6.0, 6.0].into_iter().map(move |z| {
                        let position = Vec3::new(x, y, z);
                        LeafPlacement {
                            position,
                            anchor: Vec3::ZERO,
                        }
                    })
                })
            })
            .collect()
    }

    #[test]
    fn descriptor_is_bounded_normalized_and_independent_of_leaf_traversal_order() {
        let leaves = octant_leaf_placements();
        let mut reversed = leaves.clone();
        reversed.reverse();
        let trunks = [RoundCone::new(1.0, Vec3::NEG_Y * 2.0, 1.0, Vec3::Y * 2.0)];

        let first =
            CanopyAcousticDescriptor::build(11, Vec3::new(1.0, 2.0, 3.0), 42, &leaves, &trunks);
        let second =
            CanopyAcousticDescriptor::build(11, Vec3::new(1.0, 2.0, 3.0), 42, &reversed, &trunks);
        let next_generation =
            CanopyAcousticDescriptor::build(12, Vec3::new(1.0, 2.0, 3.0), 42, &leaves, &trunks);

        assert_eq!(first.samples(), second.samples());
        assert_eq!(first.samples(), next_generation.samples());
        assert_eq!(first.content_seed(), second.content_seed());
        assert_eq!(first.content_seed(), next_generation.content_seed());
        assert_eq!(first.phase(), second.phase());
        assert_eq!(first.phase(), next_generation.phase());
        assert_eq!(first.generation(), 11);
        assert_eq!(next_generation.generation(), 12);
        assert_eq!(first.samples().len(), 8);
        assert!(first.samples().len() <= CanopyAcousticDescriptor::MAX_SAMPLES);
        assert!(first
            .samples()
            .iter()
            .all(|sample| (sample.weight() - 0.125).abs() < 1.0e-6));
        assert!((first.total_weight() - 1.0).abs() < 1.0e-6);
        assert_eq!(
            first
                .samples()
                .iter()
                .map(CanopyAcousticSample::id)
                .collect::<HashSet<_>>()
                .len(),
            first.samples().len(),
        );
        assert!(first
            .samples()
            .iter()
            .all(|sample| (0.0..1.0).contains(&sample.phase())));
    }

    #[test]
    fn descriptor_uses_a_deterministic_clear_leaf_spray_fallback() {
        let leaves = [
            LeafPlacement {
                position: Vec3::new(0.0, -0.25, 0.0),
                anchor: Vec3::new(0.0, 0.75, 0.0),
            },
            LeafPlacement {
                position: Vec3::new(0.0, 0.25, 0.0),
                anchor: Vec3::new(0.0, -0.75, 0.0),
            },
        ];
        let reversed = [leaves[1], leaves[0]];
        let trunks = [RoundCone::new(5.0, Vec3::NEG_Y, 5.0, Vec3::Y)];

        let first = CanopyAcousticDescriptor::build(3, Vec3::ZERO, 99, &leaves, &trunks);
        let second = CanopyAcousticDescriptor::build(3, Vec3::ZERO, 99, &reversed, &trunks);

        assert_eq!(first.samples(), second.samples());
        assert_eq!(first.samples().len(), 1);
        let sample = &first.samples()[0];
        assert_eq!(
            sample.provenance(),
            CanopyAcousticSampleProvenance::ExtrapolatedLeafSprayFallback
        );
        assert!((sample.weight() - 1.0).abs() < 1.0e-6);
        assert!(sample.clearance_voxels() >= CanopyAcousticDescriptor::MIN_WOOD_CLEARANCE_VOXELS);
        assert!(trunks
            .iter()
            .all(|trunk| trunk.signed_distance(sample.position_tree_voxels())
                >= CanopyAcousticDescriptor::MIN_WOOD_CLEARANCE_VOXELS));
    }

    #[test]
    fn generated_tree_publishes_only_real_clear_leaf_samples() {
        let mut tree_desc = TreeDesc::default();
        tree_desc.branching.iterations = 3;
        tree_desc.size = 12.0;
        tree_desc.leaf_offset = 0;
        tree_desc.leaf_density = 0.01;
        tree_desc.enable_subdivision = false;
        let tree = Tree::new(tree_desc.clone());
        let descriptor = CanopyAcousticDescriptor::build(
            1,
            Vec3::ZERO,
            tree_desc.branching.seed,
            tree.relative_leaf_placements(),
            tree.trunks(),
        );

        assert!(!descriptor.samples().is_empty());
        assert!(descriptor.samples().len() <= CanopyAcousticDescriptor::MAX_SAMPLES);
        assert!((descriptor.total_weight() - 1.0).abs() < 1.0e-6);
        for sample in descriptor.samples() {
            assert_eq!(
                sample.provenance(),
                CanopyAcousticSampleProvenance::LeafPlacement
            );
            assert!(tree
                .relative_leaf_placements()
                .iter()
                .any(|leaf| leaf.position == sample.position_tree_voxels()));
            let measured_clearance = tree
                .trunks()
                .iter()
                .map(|trunk| trunk.signed_distance(sample.position_tree_voxels()))
                .fold(f32::INFINITY, f32::min);
            assert!(measured_clearance >= CanopyAcousticDescriptor::MIN_WOOD_CLEARANCE_VOXELS);
            assert!((measured_clearance - sample.clearance_voxels()).abs() < 1.0e-5);
        }
    }
}
