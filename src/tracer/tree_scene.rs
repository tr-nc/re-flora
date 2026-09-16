//! Refit-only triangle hierarchy. Color, shadow and ray queries consume one posed surface.
use anyhow::{ensure, Result};
use bytemuck::{Pod, Zeroable};
use glam::Vec3;

// Complete axis-aligned cubes retain interior faces that smooth surfaces omit.
pub const MAX_TREE_TRIANGLES: usize = 1 << 17;
pub const MAX_TREE_VERTICES: usize = 1 << 17;
pub const MAX_TREE_NODES: usize = MAX_TREE_TRIANGLES * 2;

#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
pub struct TreeSceneNode {
    pub min: [f32; 3],
    pub escape: u32,
    pub max: [f32; 3],
    pub triangle: u32,
}

#[derive(Default)]
pub struct TreeScene {
    pub nodes: Vec<TreeSceneNode>,
    pub triangles: Vec<[u32; 4]>,
}

impl TreeScene {
    pub fn new(indices: &[u32], positions: &[Vec3]) -> Result<Self> {
        ensure!(indices.len() % 3 == 0, "invalid tree triangle count");
        ensure!(
            positions.len() <= MAX_TREE_VERTICES && indices.len() / 3 <= MAX_TREE_TRIANGLES,
            "dynamic tree scene capacity exceeded: vertices={} / {}, triangles={} / {}",
            positions.len(),
            MAX_TREE_VERTICES,
            indices.len() / 3,
            MAX_TREE_TRIANGLES
        );
        ensure!(
            indices.iter().all(|&i| (i as usize) < positions.len()),
            "tree index out of range"
        );
        ensure!(
            positions.iter().all(|p| p.is_finite()),
            "nonfinite tree geometry"
        );
        let triangles: Vec<_> = indices
            .chunks_exact(3)
            .map(|t| [t[0], t[1], t[2], 0])
            .collect();
        let mut scene = Self {
            nodes: Vec::new(),
            triangles,
        };
        let mut order: Vec<u32> = (0..scene.triangles.len() as u32).collect();
        if !order.is_empty() {
            scene.build(&mut order, positions);
        }
        scene.refit(positions)?;
        Ok(scene)
    }
    fn build(&mut self, order: &mut [u32], positions: &[Vec3]) {
        let index = self.nodes.len();
        self.nodes.push(TreeSceneNode {
            triangle: u32::MAX,
            ..Default::default()
        });
        if order.len() == 1 {
            self.nodes[index].triangle = order[0];
        } else {
            let centroid = |i: u32| {
                let t = self.triangles[i as usize];
                (positions[t[0] as usize] + positions[t[1] as usize] + positions[t[2] as usize])
                    / 3.
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
            } else if node.triangle != u32::MAX {
                result.push(node.triangle);
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
            let triangle = self.nodes[index].triangle;
            let (min, max) = if triangle != u32::MAX {
                let t = self.triangles[triangle as usize];
                let a = *positions
                    .get(t[0] as usize)
                    .ok_or_else(|| anyhow::anyhow!("tree topology changed during refit"))?;
                let b = *positions
                    .get(t[1] as usize)
                    .ok_or_else(|| anyhow::anyhow!("tree topology changed during refit"))?;
                let c = *positions
                    .get(t[2] as usize)
                    .ok_or_else(|| anyhow::anyhow!("tree topology changed during refit"))?;
                (a.min(b).min(c), a.max(b).max(c))
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
        let topology: Vec<_> = tree.nodes.iter().map(|n| (n.escape, n.triangle)).collect();
        positions[4] += Vec3::new(3., 2., 4.);
        tree.refit(&positions).unwrap();
        assert_eq!(
            topology,
            tree.nodes
                .iter()
                .map(|n| (n.escape, n.triangle))
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
