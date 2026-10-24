//! Complete axes compile to endpoint-attached segments. Sampling depth is not branch order.
use crate::branch_skeleton::{BranchSegment, BranchSegmentRole};
use glam::Vec3;
use rand::{rngs::StdRng, RngExt, SeedableRng};
use std::f32::consts::TAU;

use super::{TreeDesc, TreeSpecies};

mod garden;

/// The legacy compiler stays untouched so saved Conifers retain their original topology.
/// New species always use axes; the old A/B bit applies only to legacy Conifers.
pub(super) fn generate(desc: &TreeDesc) -> Option<AxisSkeleton> {
    match desc.species {
        TreeSpecies::GardenTree => None,
        TreeSpecies::Conifer if !desc.axis_model => None,
        TreeSpecies::Conifer => Some(conifer(desc)),
        TreeSpecies::AppleTree => Some(garden::broadleaf(desc, false)),
        TreeSpecies::Pine => Some(garden::pine(desc)),
        TreeSpecies::WeepingWillow => Some(garden::broadleaf(desc, true)),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxisSegment {
    pub axis_id: u32,
    pub order: u32,
    pub u_start: f32,
    pub u_end: f32,
    pub radius_start: f32,
    pub radius_end: f32,
    /// Mature-topology reveal time; every child is revealed no earlier than its parent.
    pub reveal_age: f32,
}

#[derive(Default)]
pub(super) struct AxisSkeleton {
    pub segments: Vec<BranchSegment>,
    pub metadata: Vec<AxisSegment>,
}

/// An authored architectural profile, distinct from geometry sampling resolution.
struct ConiferProfile {
    height: f32,
    crown_radius: f32,
    stations: u32,
    branches_per_station: u32,
    branch_radius_ratio: f32,
    twig_radius_ratio: f32,
}

impl Default for ConiferProfile {
    fn default() -> Self {
        Self {
            height: 170.,
            crown_radius: 64.,
            stations: 9,
            branches_per_station: 4,
            branch_radius_ratio: 0.30,
            twig_radius_ratio: 0.32,
        }
    }
}

pub(crate) const MAX_AXIS_SEGMENTS: usize = 4096;

#[derive(Default)]
struct Compiler {
    skeleton: AxisSkeleton,
    next_axis: u32,
}

impl Compiler {
    /// Sampling a complete axis does not consume branching depth or change its order.
    fn axis(
        &mut self,
        parent: Option<usize>,
        points: &[Vec3],
        order: u32,
        radii: &[f32],
        reveal: &[f32],
    ) -> Vec<usize> {
        assert_eq!(points.len(), radii.len());
        assert_eq!(points.len(), reveal.len());
        assert!(self.skeleton.segments.len() + points.len() - 1 <= MAX_AXIS_SEGMENTS);
        let axis_id = self.next_axis;
        self.next_axis += 1;
        let length = points.windows(2).map(|p| p[0].distance(p[1])).sum::<f32>();
        let mut arc = 0.;
        let mut parent = parent;
        let mut indices = Vec::new();
        for i in 0..points.len() - 1 {
            let next_arc = arc + points[i].distance(points[i + 1]);
            let index = self.skeleton.segments.len();
            let parent_age = parent.map_or(0., |p| self.skeleton.metadata[p].reveal_age);
            self.skeleton.segments.push(BranchSegment {
                parent,
                start: points[i],
                end: points[i + 1],
                level: order,
                role: if order > 0 && i == 0 {
                    BranchSegmentRole::Lateral
                } else {
                    BranchSegmentRole::MainAxis
                },
            });
            self.skeleton.metadata.push(AxisSegment {
                axis_id,
                order,
                u_start: if length > 0. { arc / length } else { 0. },
                u_end: if length > 0. { next_arc / length } else { 1. },
                radius_start: radii[i].max(0.),
                radius_end: radii[i + 1].max(0.),
                reveal_age: reveal[i].max(parent_age),
            });
            indices.push(index);
            parent = Some(index);
            arc = next_arc;
        }
        indices
    }
}

pub(super) fn conifer(desc: &TreeDesc) -> AxisSkeleton {
    let profile = ConiferProfile::default();
    let scale = (desc.size / 30.).max(0.);
    let height = profile.height * scale;
    let base_radius = (desc.size * desc.trunk_thickness).max(0.);
    let mut rng = StdRng::seed_from_u64(desc.branching.seed ^ 0xC091_FE42_A315_7721);
    let mut compiler = Compiler {
        skeleton: AxisSkeleton {
            segments: Vec::new(),
            metadata: Vec::new(),
        },
        next_axis: 0,
    };
    // Every whorl is an explicit endpoint on the same continuous trunk axis.
    let steps = profile.stations + 3;
    let lean = Vec3::new(
        rng.random_range(-0.018..=0.018),
        0.,
        rng.random_range(-0.018..=0.018),
    );
    let points = (0..=steps)
        .map(|i| {
            let u = i as f32 / steps as f32;
            Vec3::Y * height * u + lean * height * u * u
        })
        .collect::<Vec<_>>();
    let radii = (0..=steps)
        .map(|i| {
            let u = i as f32 / steps as f32;
            base_radius * (1. - 0.97 * u).powf(0.85)
        })
        .collect::<Vec<_>>();
    let reveal = (0..=steps)
        .map(|i| i as f32 / steps as f32 * 0.55)
        .collect::<Vec<_>>();
    let trunk = compiler.axis(None, &points, 0, &radii, &reveal);
    let phase = rng.random_range(0.0..TAU);

    for station in 2..=profile.stations + 1 {
        let u = station as f32 / steps as f32;
        let parent = trunk[station as usize - 1];
        let base = points[station as usize];
        // Separate crown envelope from trunk internode length: lower limbs are wider.
        let crown_width = profile.crown_radius * scale * ((1. - u) / (1. - 2. / steps as f32));
        for branch in 0..profile.branches_per_station {
            let angle = phase
                + station as f32 * 2.399963
                + branch as f32 * TAU / profile.branches_per_station as f32
                + rng.random_range(-0.13..=0.13);
            let direction = Vec3::new(angle.cos(), 0., angle.sin());
            let length = crown_width * rng.random_range(0.88..=1.06);
            let axis_points = (0..=3)
                .map(|i| {
                    let v = i as f32 / 3.;
                    base + direction * length * v + Vec3::Y * length * (0.10 * v + 0.16 * v * v)
                })
                .collect::<Vec<_>>();
            let branch_radius = radii[station as usize] * profile.branch_radius_ratio;
            let axis_radii = (0..=3)
                .map(|i| branch_radius * (1. - 0.9 * i as f32 / 3.))
                .collect::<Vec<_>>();
            let birth = 0.12 + u * 0.55;
            let axis_reveal = (0..=3)
                .map(|i| birth + i as f32 / 3. * 0.12)
                .collect::<Vec<_>>();
            let lateral = compiler.axis(Some(parent), &axis_points, 1, &axis_radii, &axis_reveal);

            // Feathered side sprays fill a layered conical crown without branching explosion.
            for site in 1..=3 {
                let side = Vec3::new(-direction.z, 0., direction.x);
                for sign in [-1., 1.] {
                    let twig_dir =
                        (direction * 0.55 + side * sign * 0.8 + Vec3::Y * 0.15).normalize();
                    let twig_length =
                        length * (0.33 - 0.04 * site as f32) * rng.random_range(0.85..=1.1);
                    let root = axis_points[site];
                    let twig_points = [
                        root,
                        root + twig_dir * twig_length * 0.5,
                        root + twig_dir * twig_length,
                    ];
                    let radius = axis_radii[site] * profile.twig_radius_ratio;
                    let radii = [radius, radius * 0.55, radius * 0.12];
                    let reveal = [
                        axis_reveal[site].min(0.9),
                        (axis_reveal[site] + 0.06).min(0.96),
                        1.,
                    ];
                    compiler.axis(Some(lateral[site - 1]), &twig_points, 2, &radii, &reveal);
                }
            }
        }
    }
    compiler.skeleton
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree_gen::TreeSpecies;

    #[test]
    fn conifer_has_one_continuous_trunk_independent_of_branch_order() {
        let desc = TreeSpecies::Conifer.mature_desc(&TreeDesc::default());
        let tree = conifer(&desc);
        let trunk = tree
            .metadata
            .iter()
            .enumerate()
            .filter(|(_, m)| m.order == 0)
            .collect::<Vec<_>>();
        assert_eq!(trunk.len(), 12);
        assert!((tree.segments[trunk.last().unwrap().0].end.y - 170.).abs() < 1e-4);
        for pair in trunk.windows(2) {
            assert_eq!(pair[0].1.axis_id, pair[1].1.axis_id);
            assert_eq!(pair[0].1.radius_end, pair[1].1.radius_start);
            assert_eq!(tree.segments[pair[1].0].parent, Some(pair[0].0));
        }
        for (i, (segment, meta)) in tree.segments.iter().zip(&tree.metadata).enumerate() {
            assert_eq!(segment.level, meta.order);
            assert!(meta.radius_start.is_finite() && meta.radius_end >= 0.);
            if let Some(parent) = segment.parent {
                assert!(parent < i);
                assert_eq!(segment.start, tree.segments[parent].end);
                assert!(meta.reveal_age >= tree.metadata[parent].reveal_age);
                if meta.axis_id != tree.metadata[parent].axis_id {
                    assert_eq!(meta.order, tree.metadata[parent].order + 1);
                    assert!(meta.radius_start < tree.metadata[parent].radius_end);
                }
            }
        }
        assert!(tree.segments.len() < MAX_AXIS_SEGMENTS);
    }

    #[test]
    fn different_seeds_vary_shape_without_changing_architecture() {
        let desc = TreeSpecies::Conifer.mature_desc(&TreeDesc::default());
        let first = conifer(&desc);
        let second = conifer(&desc);
        assert_eq!(first.segments, second.segments);
        assert_eq!(first.metadata, second.metadata);
        let mut other = desc.clone();
        other.branching.seed += 1;
        assert_ne!(first.segments, conifer(&other).segments);
    }
}
