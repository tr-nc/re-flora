use super::{build_bvh, Aabb3, BvhNode, RoundCone};
use glam::Vec3;

/// Read-only acceleration structure for exact clearance queries against tree wood geometry.
///
/// The index owns only its BVH; [`RoundCone`] remains the authoritative geometry and signed
/// distance implementation. This keeps canopy layout policy out of the geometry layer.
pub struct RoundConeClearanceIndex<'a> {
    cones: &'a [RoundCone],
    nodes: Vec<BvhNode>,
}

impl<'a> RoundConeClearanceIndex<'a> {
    pub fn new(cones: &'a [RoundCone]) -> Self {
        let nodes = if cones.is_empty() {
            Vec::new()
        } else {
            let aabbs = cones.iter().map(RoundCone::aabb).collect::<Vec<_>>();
            let leaf_indices = (0..cones.len())
                .map(|index| u32::try_from(index).expect("round-cone index exceeds u32"))
                .collect::<Vec<_>>();
            build_bvh(&aabbs, &leaf_indices).expect("equal non-empty cone and index arrays")
        };
        Self { cones, nodes }
    }

    /// Returns whether every wood surface is at least `required_clearance` from `point`.
    ///
    /// AABB distance rejects whole BVH subtrees. Round-cone signed distance remains the final
    /// authority for every node close enough to matter.
    pub fn has_minimum_clearance(&self, point: Vec3, required_clearance: f32) -> bool {
        if self.nodes.is_empty() {
            return true;
        }

        let mut pending = vec![0_usize];
        while let Some(node_index) = pending.pop() {
            let node = &self.nodes[node_index];
            let aabb_distance = distance_to_aabb(point, &node.aabb);
            if aabb_distance > 0.0 && aabb_distance >= required_clearance {
                continue;
            }
            if node.is_leaf {
                if self.cones[node.data_offset as usize].signed_distance(point) < required_clearance
                {
                    return false;
                }
            } else {
                let left = node.left as usize;
                pending.push(left);
                pending.push(left + 1);
            }
        }
        true
    }

    /// Exact signed-distance winner, breaking ties by original cone index.
    /// Outside a node's bounds, unsigned AABB distance is a lower bound. Inside,
    /// it is not a signed bound: visit every overlapping node, even after finding
    /// negative clearance. Keep equal-distance candidates for deterministic ties.
    pub fn nearest_cone(&self, point: Vec3) -> Option<usize> {
        if self.nodes.is_empty() {
            return None;
        }
        let mut best: Option<(usize, f32)> = None;
        let mut pending = vec![0_usize];
        while let Some(node_index) = pending.pop() {
            let node = &self.nodes[node_index];
            if let Some((_, distance)) = best {
                // Conservative allowance for the different floating-point arithmetic
                // in the cone SDF and AABB distance, especially at wood boundaries.
                let scale = point
                    .abs()
                    .max(node.aabb.min().abs())
                    .max(node.aabb.max().abs())
                    .max_element()
                    .max(1.);
                let slack = 32. * f32::EPSILON * scale;
                if distance_to_aabb(point, &node.aabb) > distance.max(0.) + slack {
                    continue;
                }
            }
            if node.is_leaf {
                let cone = node.data_offset as usize;
                let distance = self.cones[cone].signed_distance(point);
                if best.is_none_or(|(index, minimum)| {
                    distance.total_cmp(&minimum).then(cone.cmp(&index)).is_lt()
                }) {
                    best = Some((cone, distance));
                }
            } else {
                let left = node.left as usize;
                let right = left + 1;
                if distance_to_aabb(point, &self.nodes[left].aabb)
                    <= distance_to_aabb(point, &self.nodes[right].aabb)
                {
                    pending.push(right);
                    pending.push(left);
                } else {
                    pending.push(left);
                    pending.push(right);
                }
            }
        }
        best.map(|(index, _)| index)
    }

    /// Returns the exact minimum signed distance to any indexed round cone.
    pub fn minimum_clearance(&self, point: Vec3) -> f32 {
        if self.nodes.is_empty() {
            return f32::INFINITY;
        }

        let mut minimum = f32::INFINITY;
        let mut pending = vec![0_usize];
        while let Some(node_index) = pending.pop() {
            let node = &self.nodes[node_index];
            // AABB distance is non-negative. It is a valid lower bound while the current exact
            // minimum is outside wood. Once a negative value is found, visit the remaining tree
            // so overlapping wood volumes still return the exact signed minimum.
            if minimum > 0.0 && distance_to_aabb(point, &node.aabb) >= minimum {
                continue;
            }
            if node.is_leaf {
                minimum = minimum.min(self.cones[node.data_offset as usize].signed_distance(point));
            } else {
                let left = node.left as usize;
                let right = left + 1;
                let left_distance = distance_to_aabb(point, &self.nodes[left].aabb);
                let right_distance = distance_to_aabb(point, &self.nodes[right].aabb);
                if left_distance <= right_distance {
                    pending.push(right);
                    pending.push(left);
                } else {
                    pending.push(left);
                    pending.push(right);
                }
            }
        }
        minimum
    }
}

fn distance_to_aabb(point: Vec3, aabb: &Aabb3) -> f32 {
    point.clamp(aabb.min(), aabb.max()).distance(point)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexed_queries_match_authoritative_round_cone_distances() {
        let cones = [
            RoundCone::new(
                1.0,
                Vec3::new(-10.0, -2.0, 0.0),
                1.0,
                Vec3::new(-10.0, 2.0, 0.0),
            ),
            RoundCone::new(
                2.0,
                Vec3::new(8.0, -3.0, 1.0),
                0.5,
                Vec3::new(8.0, 3.0, 1.0),
            ),
        ];
        let index = RoundConeClearanceIndex::new(&cones);

        for x in [-10.0, -4.0, 0.0, 7.0, 12.0] {
            for y in [-4.0, 0.0, 4.0] {
                let point = Vec3::new(x, y, 0.5);
                let expected = cones
                    .iter()
                    .map(|cone| cone.signed_distance(point))
                    .fold(f32::INFINITY, f32::min);
                let measured = index.minimum_clearance(point);
                assert!((expected - measured).abs() < 1.0e-5);
                for required in [-0.5, 0.0, 2.0, 5.0] {
                    assert_eq!(
                        index.has_minimum_clearance(point, required),
                        expected >= required,
                        "point={point:?} required={required} expected={expected}"
                    );
                }
            }
        }
    }

    #[test]
    fn nearest_cone_preserves_signed_minimum_and_original_index_ties() {
        let cones = [
            RoundCone::new(2., Vec3::ZERO, 2., Vec3::Y * 4.),
            RoundCone::new(2., Vec3::ZERO, 2., Vec3::Y * 4.), // exact tie
            RoundCone::new(3., Vec3::X, 0.1, Vec3::new(1., 6., 0.)),
            RoundCone::new(1., Vec3::ZERO, 0.2, Vec3::ZERO), // collapsed
            RoundCone::new(0.25, Vec3::splat(12.), 0.05, Vec3::splat(13.)),
        ];
        let index = RoundConeClearanceIndex::new(&cones);
        for x in -5..=14 {
            for y in -3..=15 {
                for z in [-2., -0.001, 0., 0.001, 2., 12.] {
                    let point = Vec3::new(x as f32, y as f32, z);
                    let expected = cones
                        .iter()
                        .enumerate()
                        .min_by(|(ia, a), (ib, b)| {
                            a.signed_distance(point)
                                .total_cmp(&b.signed_distance(point))
                                .then(ia.cmp(ib))
                        })
                        .map(|(i, _)| i);
                    assert_eq!(index.nearest_cone(point), expected, "point={point:?}");
                }
            }
        }
        // Both deepest overlapping wood and equidistant disjoint wood must keep index order.
        let tied = [cones[0].clone(), cones[1].clone()];
        assert_eq!(
            RoundConeClearanceIndex::new(&tied).nearest_cone(Vec3::ZERO),
            Some(0)
        );
        assert_eq!(
            RoundConeClearanceIndex::new(&tied).nearest_cone(Vec3::splat(100.)),
            Some(0)
        );
        let disjoint = [
            RoundCone::new(1., Vec3::X * 4., 1., Vec3::X * 4.),
            RoundCone::new(1., Vec3::NEG_X * 4., 1., Vec3::NEG_X * 4.),
        ];
        assert_eq!(
            RoundConeClearanceIndex::new(&disjoint).nearest_cone(Vec3::ZERO),
            Some(0)
        );
    }

    #[test]
    fn empty_index_has_unbounded_clearance() {
        let index = RoundConeClearanceIndex::new(&[]);

        assert_eq!(index.nearest_cone(Vec3::ZERO), None);
        assert!(index.minimum_clearance(Vec3::ZERO).is_infinite());
        assert!(index.has_minimum_clearance(Vec3::ZERO, 100.0));
    }
}
