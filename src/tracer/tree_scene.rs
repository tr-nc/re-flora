//! Refit-only primitive hierarchy. Color, shadow and ray queries consume one posed surface.
use anyhow::{ensure, Result};
use bytemuck::{Pod, Zeroable};
use glam::Vec3;

// One query primitive per exposed surface triangle.
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
pub struct TreeRefitSchedule {
    /// [node index, escape index, primitive index, reserved], deepest level first.
    pub records: Vec<[u32; 4]>,
    pub levels: Vec<[u32; 2]>,
}

#[derive(Default, Clone)]
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
        primitive_bounds(self.primitives[primitive], &|i: u32| {
            positions
                .get(i as usize)
                .copied()
                .ok_or_else(|| anyhow::anyhow!("tree topology changed during refit"))
        })
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
    /// Children must finish before a parent dispatch. This schedule is immutable
    /// for a topology generation and has no per-frame CPU refit cost.
    pub fn refit_schedule(&self) -> TreeRefitSchedule {
        let mut levels: Vec<Vec<[u32; 4]>> = Vec::new();
        let mut pending = if self.nodes.is_empty() {
            Vec::new()
        } else {
            vec![(0usize, 0usize)]
        };
        while let Some((index, depth)) = pending.pop() {
            if levels.len() <= depth {
                levels.resize_with(depth + 1, Vec::new);
            }
            let node = self.nodes[index];
            levels[depth].push([index as u32, node.escape, node.primitive, 0]);
            if node.primitive == u32::MAX {
                pending.push((index + 1, depth + 1));
                pending.push((self.nodes[index + 1].escape as usize, depth + 1));
            }
        }
        let mut schedule = TreeRefitSchedule::default();
        for level in levels.into_iter().rev() {
            schedule
                .levels
                .push([schedule.records.len() as u32, level.len() as u32]);
            schedule.records.extend(level);
        }
        schedule
    }

    #[cfg(test)]
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
        self.refit_by(|i| {
            positions
                .get(i as usize)
                .copied()
                .ok_or_else(|| anyhow::anyhow!("tree topology changed during refit"))
        })
    }

    pub fn refit_by(&mut self, point: impl Fn(u32) -> Result<Vec3>) -> Result<()> {
        for index in (0..self.nodes.len()).rev() {
            let triangle = self.nodes[index].primitive;
            let (min, max) = if triangle != u32::MAX {
                primitive_bounds(self.primitives[triangle as usize], &point)?
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

fn primitive_bounds(t: [u32; 4], point: &impl Fn(u32) -> Result<Vec3>) -> Result<(Vec3, Vec3)> {
    let (a, b, c) = (point(t[0])?, point(t[1])?, point(t[2])?);
    Ok((a.min(b).min(c), a.max(b).max(c)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gpu_refit_schedule_covers_nodes_once_and_publishes_children_first() {
        let points: Vec<_> = (0..51)
            .map(|i| Vec3::new(i as f32, (i % 5) as f32, 0.))
            .collect();
        let scene = TreeScene::new(&(0..51).collect::<Vec<_>>(), &points).unwrap();
        let schedule = scene.refit_schedule();
        assert_eq!(schedule.records.len(), scene.nodes.len());
        let mut completed = std::collections::BTreeSet::new();
        for [start, count] in schedule.levels {
            let records = &schedule.records[start as usize..(start + count) as usize];
            for &[index, escape, primitive, _] in records {
                assert!(!completed.contains(&index));
                assert_eq!(escape, scene.nodes[index as usize].escape);
                assert_eq!(primitive, scene.nodes[index as usize].primitive);
                if primitive == u32::MAX {
                    assert!(completed.contains(&(index + 1)));
                    assert!(completed.contains(&scene.nodes[index as usize + 1].escape));
                }
            }
            completed.extend(records.iter().map(|r| r[0]));
        }
        assert_eq!(completed.len(), scene.nodes.len());
        assert!(TreeScene::default().refit_schedule().levels.is_empty());
    }

    #[test]
    fn accelerated_triangles_match_brute_force_boundaries_after_motion() {
        use crate::tracer::voxel_geometry::{CUBE_INDICES, VOXEL_VERTICES};
        use crate::tree_gen::skin::intersect_surface_triangle;
        let rest: Vec<_> = VOXEL_VERTICES.iter().map(|v| v.as_vec3() / 256.).collect();
        let offset = Vec3::new(1.037, 0.683, 0.954);
        let positions: Vec<_> = rest.iter().map(|v| *v + offset).collect();
        let mut scene = TreeScene::new(&CUBE_INDICES, &rest).unwrap();
        scene.refit(&positions).unwrap();
        assert_eq!(scene.primitives.len(), 12);
        assert_eq!(scene.nodes.len(), 23);
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
            let direct = scene
                .ray_candidates(origin, direction)
                .into_iter()
                .filter_map(|i| {
                    let t = scene.primitives[i as usize];
                    let ids = [t[0] as usize, t[1] as usize, t[2] as usize];
                    intersect_surface_triangle(
                        origin,
                        direction,
                        ids.map(|i| positions[i]),
                        ids.map(|i| rest[i]),
                    )
                })
                .filter(|h| h.distance >= 1e-6)
                .min_by(|a, b| a.distance.total_cmp(&b.distance));
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
            if let (Some(direct), Some(reference)) = (direct, reference) {
                assert!(
                    (direct.distance - reference.distance).abs() < 2e-6,
                    "ray {i}"
                );
                assert!(direct
                    .rest_position
                    .abs_diff_eq(reference.rest_position, 2e-6));
                assert!((direct.world_position - offset).abs_diff_eq(reference.rest_position, 2e-6));
            }
        }
        assert!(scene.ray_candidates(center + Vec3::X, Vec3::Y).is_empty());
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
