//! Complete axes compile to endpoint-attached segments. Sampling depth is not branch order.
use crate::branch_skeleton::{BranchSegment, BranchSegmentRole};
use glam::Vec3;

use super::{growth::rules::Architecture, TreeDesc};

mod garden;
mod legacy_conifer;

/// Architecture adapters know curve/layout rules, not species identity or saved-config policy.
pub(super) fn generate(desc: &TreeDesc, architecture: Architecture) -> Option<AxisSkeleton> {
    match architecture {
        Architecture::Recursive => None,
        Architecture::LegacyConifer => Some(legacy_conifer::generate(desc)),
        Architecture::Spreading(profile) => Some(garden::spreading(desc, profile)),
        Architecture::Layered(profile) => Some(garden::layered(desc, profile)),
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
        radii: &[f32],
        reveal: &[f32],
    ) -> Vec<usize> {
        assert!(points.len() >= 2, "an axis needs a root and a tip");
        let order = parent.map_or(0, |parent| {
            assert_eq!(
                points[0], self.skeleton.segments[parent].end,
                "disconnected axis"
            );
            self.skeleton.metadata[parent].order + 1
        });
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

#[cfg(test)]
mod tests {
    use super::legacy_conifer::generate as conifer;
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
