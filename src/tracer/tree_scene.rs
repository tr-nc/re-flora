//! Refit-only primitive hierarchy. Color, shadow and ray queries consume one posed surface.
use anyhow::{ensure, Result};
use bytemuck::{Pod, Zeroable};
use glam::Vec3;

// One query primitive per triangle or complete axis-aligned cube.
pub const MAX_TREE_PRIMITIVES: usize = 1 << 17;
pub const MAX_TREE_VERTICES: usize = 1 << 17;
pub const MAX_TREE_NODES: usize = MAX_TREE_PRIMITIVES * 2;

#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
pub struct TreeSceneNode {
    pub min: [f32; 3],
    pub escape: u32,
    pub max: [f32; 3],
    pub primitive: u32,
}

#[derive(Default)]
pub struct TreeScene {
    pub nodes: Vec<TreeSceneNode>,
    pub primitives: Vec<[u32; 4]>,
}

impl TreeScene {
    pub fn new(indices: &[u32], positions: &[Vec3]) -> Result<Self> {
        ensure!(indices.len() % 3 == 0, "invalid tree triangle count");
        ensure!(
            positions.len() <= MAX_TREE_VERTICES && indices.len() / 3 <= MAX_TREE_PRIMITIVES,
            "dynamic tree scene capacity exceeded: vertices={} / {}, primitives={} / {}",
            positions.len(),
            MAX_TREE_VERTICES,
            indices.len() / 3,
            MAX_TREE_PRIMITIVES
        );
        ensure!(
            indices.iter().all(|&i| (i as usize) < positions.len()),
            "tree index out of range"
        );
        ensure!(
            positions.iter().all(|p| p.is_finite()),
            "nonfinite tree geometry"
        );
        let primitives: Vec<_> = indices
            .chunks_exact(3)
            .map(|t| [t[0], t[1], t[2], 0])
            .collect();
        Self::from_primitives(primitives, positions)
    }
    /// Explicit minimum/maximum vertex indices keep query topology independent
    /// of the visual mesh's face count or vertex layout.
    pub fn boxes(bounds: &[[u32; 2]], positions: &[Vec3]) -> Result<Self> {
        ensure!(
            positions.len() <= MAX_TREE_VERTICES,
            "tree vertex capacity exceeded"
        );
        ensure!(
            bounds
                .iter()
                .flatten()
                .all(|&i| (i as usize) < positions.len()),
            "tree box index out of range"
        );
        let primitives = bounds.iter().map(|&[min, max]| [min, max, 0, 1]).collect();
        Self::from_primitives(primitives, positions)
    }
    fn from_primitives(primitives: Vec<[u32; 4]>, positions: &[Vec3]) -> Result<Self> {
        ensure!(
            primitives.len() <= MAX_TREE_PRIMITIVES,
            "tree query capacity exceeded"
        );
        ensure!(
            positions.iter().all(|p| p.is_finite()),
            "nonfinite tree geometry"
        );
        let mut scene = Self {
            nodes: Vec::new(),
            primitives,
        };
        let mut order: Vec<u32> = (0..scene.primitives.len() as u32).collect();
        if !order.is_empty() {
            scene.build(&mut order, positions);
        }
        scene.refit(positions)?;
        Ok(scene)
    }
    fn bounds(&self, primitive: usize, positions: &[Vec3]) -> Result<(Vec3, Vec3)> {
        let t = self.primitives[primitive];
        let point = |i: u32| {
            positions
                .get(i as usize)
                .copied()
                .ok_or_else(|| anyhow::anyhow!("tree topology changed during refit"))
        };
        let (a, b) = (point(t[0])?, point(t[1])?);
        if t[3] == 1 {
            Ok((a.min(b), a.max(b)))
        } else {
            let c = point(t[2])?;
            Ok((a.min(b).min(c), a.max(b).max(c)))
        }
    }
    fn build(&mut self, order: &mut [u32], positions: &[Vec3]) {
        let index = self.nodes.len();
        self.nodes.push(TreeSceneNode {
            primitive: u32::MAX,
            ..Default::default()
        });
        if order.len() == 1 {
            self.nodes[index].primitive = order[0];
        } else {
            let centroid = |i: u32| {
                let (min, max) = self
                    .bounds(i as usize, positions)
                    .expect("validated indices");
                (min + max) * 0.5
            };
            let mut min = Vec3::splat(f32::INFINITY);
            let mut max = Vec3::splat(f32::NEG_INFINITY);
            for &i in order.iter() {
                let c = centroid(i);
                min = min.min(c);
                max = max.max(c);
            }
            let span = max - min;
            let axis = if span.x >= span.y && span.x >= span.z {
                0
            } else if span.y >= span.z {
                1
            } else {
                2
            };
            let middle = order.len() / 2;
            order.select_nth_unstable_by(middle, |&a, &b| {
                centroid(a)[axis]
                    .total_cmp(&centroid(b)[axis])
                    .then(a.cmp(&b))
            });
            let (left, right) = order.split_at_mut(middle);
            self.build(left, positions);
            self.build(right, positions);
        }
        self.nodes[index].escape = self.nodes.len() as u32;
    }
    pub fn ray_candidates(&self, origin: Vec3, direction: Vec3) -> Vec<u32> {
        let mut result = Vec::new();
        let mut index = 0usize;
        while index < self.nodes.len() {
            let node = self.nodes[index];
            let mut lo: f32 = 0.;
            let mut hi: f32 = f32::INFINITY;
            for axis in 0..3 {
                if direction[axis].abs() < 1e-10 {
                    if origin[axis] < node.min[axis] || origin[axis] > node.max[axis] {
                        hi = -1.;
                    }
                } else {
                    let a = (node.min[axis] - origin[axis]) / direction[axis];
                    let b = (node.max[axis] - origin[axis]) / direction[axis];
                    lo = lo.max(a.min(b));
                    hi = hi.min(a.max(b));
                }
            }
            if lo > hi {
                index = node.escape as usize;
            } else if node.primitive != u32::MAX {
                result.push(node.primitive);
                index = node.escape as usize;
            } else {
                index += 1;
            }
        }
        result
    }

    pub fn refit(&mut self, positions: &[Vec3]) -> Result<()> {
        ensure!(
            positions.iter().all(|p| p.is_finite()),
            "nonfinite posed tree geometry"
        );
        for index in (0..self.nodes.len()).rev() {
            let triangle = self.nodes[index].primitive;
            let (min, max) = if triangle != u32::MAX {
                self.bounds(triangle as usize, positions)?
            } else {
                let left = self.nodes[index + 1];
                let right = self.nodes[left.escape as usize];
                (
                    Vec3::from(left.min).min(Vec3::from(right.min)),
                    Vec3::from(left.max).max(Vec3::from(right.max)),
                )
            };
            self.nodes[index].min = (min - Vec3::splat(1e-6)).to_array();
            self.nodes[index].max = (max + Vec3::splat(1e-6)).to_array();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn direct_boxes_match_complete_triangle_boundaries_after_motion() {
        use crate::tracer::voxel_geometry::{CUBE_INDICES, VOXEL_VERTICES};
        use crate::tree_gen::skin::intersect_surface_triangle;
        let rest: Vec<_> = VOXEL_VERTICES.iter().map(|v| v.as_vec3() / 256.).collect();
        let offset = Vec3::new(1.037, 0.683, 0.954);
        let positions: Vec<_> = rest.iter().map(|v| *v + offset).collect();
        let mut scene = TreeScene::boxes(&[[0, 6]], &rest).unwrap();
        scene.refit(&positions).unwrap();
        assert_eq!(scene.primitives.len(), 1);
        assert_eq!(scene.nodes.len(), 1);
        let center = (positions[0] + positions[6]) * 0.5;
        for i in 0..96 {
            let v = Vec3::new(
                (i as f32 * 1.3).sin(),
                (i as f32 * 0.7).cos(),
                (i as f32 * 2.1).cos(),
            )
            .normalize();
            let origin = if i % 3 == 0 { center } else { center + v * 0.1 };
            let direction = if i % 3 == 0 || i % 5 == 0 { v } else { -v };
            let direct = intersect_box(origin, direction, positions[0], positions[6]);
            let reference = CUBE_INDICES
                .chunks_exact(3)
                .filter_map(|ids| {
                    let ids = [ids[0] as usize, ids[1] as usize, ids[2] as usize];
                    intersect_surface_triangle(
                        origin,
                        direction,
                        ids.map(|i| positions[i]),
                        ids.map(|i| rest[i]),
                    )
                })
                .filter(|h| h.distance >= 1e-6)
                .min_by(|a, b| a.distance.total_cmp(&b.distance));
            assert_eq!(direct.is_some(), reference.is_some(), "ray {i}");
            if let (Some((distance, _)), Some(reference)) = (direct, reference) {
                assert!((distance - reference.distance).abs() < 2e-6, "ray {i}");
                assert!((origin + direction * distance - offset)
                    .abs_diff_eq(reference.rest_position, 2e-6));
                assert_eq!(scene.ray_candidates(origin, direction), vec![0]);
            }
        }
        assert!(intersect_box(center + Vec3::X, Vec3::Y, positions[0], positions[6]).is_none());
        assert!(intersect_box(center, Vec3::ZERO, positions[0], positions[6]).is_none());
    }

    #[test]
    fn refit_preserves_topology_and_contains_every_moving_triangle() {
        let mut positions = vec![
            Vec3::ZERO,
            Vec3::X,
            Vec3::Y,
            Vec3::Z * 3.,
            Vec3::X + Vec3::Z * 3.,
            Vec3::Y + Vec3::Z * 3.,
        ];
        let mut tree = TreeScene::new(&[0, 1, 2, 3, 4, 5], &positions).unwrap();
        assert_eq!(tree.nodes.len(), 3);
        let topology: Vec<_> = tree.nodes.iter().map(|n| (n.escape, n.primitive)).collect();
        positions[4] += Vec3::new(3., 2., 4.);
        tree.refit(&positions).unwrap();
        assert_eq!(
            topology,
            tree.nodes
                .iter()
                .map(|n| (n.escape, n.primitive))
                .collect::<Vec<_>>()
        );
        for p in positions {
            assert!(
                p.cmpge(Vec3::from(tree.nodes[0].min)).all()
                    && p.cmple(Vec3::from(tree.nodes[0].max)).all()
            );
        }
        assert!(TreeScene::new(&[7, 0, 1], &[Vec3::ZERO]).is_err());
    }
}

pub const MAX_TREE_ATTACHMENTS: usize = 1 << 16;
pub struct TreeAttachment {
    pub anchor: glam::UVec3,
    pub tree_id: u32,
    pub branch: usize,
}

/// Returns the first forward boundary, including exit hits for inside origins.
pub fn intersect_box(origin: Vec3, direction: Vec3, min: Vec3, max: Vec3) -> Option<(f32, Vec3)> {
    if !origin.is_finite() || !direction.is_finite() || direction.length_squared() == 0. {
        return None;
    }
    let direction = direction.normalize();
    let mut near = f32::NEG_INFINITY;
    let mut far = f32::INFINITY;
    let mut near_normal = Vec3::ZERO;
    let mut far_normal = Vec3::ZERO;
    for axis in 0..3 {
        if direction[axis].abs() < 1e-10 {
            if origin[axis] < min[axis] || origin[axis] > max[axis] {
                return None;
            }
        } else {
            let a = (min[axis] - origin[axis]) / direction[axis];
            let b = (max[axis] - origin[axis]) / direction[axis];
            let mut normal = Vec3::ZERO;
            normal[axis] = -direction[axis].signum();
            if a.min(b) > near {
                near = a.min(b);
                near_normal = normal;
            }
            if a.max(b) < far {
                far = a.max(b);
                far_normal = -normal;
            }
        }
    }
    if near > far || far < 1e-6 {
        None
    } else if near >= 1e-6 {
        Some((near, near_normal))
    } else {
        Some((far, far_normal))
    }
}
