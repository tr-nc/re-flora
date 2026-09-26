//! Continuous, indexed wood surface compiled directly from the authored tree.
//!
//! A child grows out of a removed parent quad, reusing its four vertex indices.
//! This makes forks actual welded topology, not intersecting capped cylinders.
//! Binding those shared vertices once also keeps the junction closed under wind.
use super::{skin::SkinBinding, Tree};
use anyhow::{ensure, Result};
use glam::Vec3;

#[derive(Clone, Debug)]
pub struct WoodVertex {
    /// Tree-local generator units; the render adapter converts to world units.
    pub position: Vec3,
    pub normal: Vec3,
    pub binding: SkinBinding,
}

#[derive(Clone, Default, Debug)]
pub struct WoodMesh {
    pub vertices: Vec<WoodVertex>,
    pub indices: Vec<u32>,
}

impl WoodMesh {
    pub fn from_tree(tree: &Tree) -> Result<Self> {
        let mut mesh = Self::default();
        let mut cones = vec![Vec::new(); tree.branches().len()];
        for (cone, &branch) in tree.trunks().iter().zip(tree.trunk_branch_indices()) {
            cones[branch].push(cone);
        }
        let mut children = vec![0; cones.len()];
        for (i, branch) in tree.branches().iter().enumerate() {
            if !cones[i].is_empty() {
                if let Some(parent) = branch.parent {
                    children[parent] += 1;
                }
            }
        }
        let mut faces: Vec<Option<[u32; 4]>> = Vec::new();
        let mut branch_faces: Vec<Vec<usize>> = vec![Vec::new(); cones.len()];
        for (branch, path) in cones.iter().enumerate() {
            let Some(first) = path.first() else {
                continue;
            };
            let end = path.last().unwrap().center_b();
            let start = first.center_a();
            let Some(direction) = (end - start).try_normalize() else {
                continue;
            };
            if first.radius_a().max(path.last().unwrap().radius_b()) <= 0. {
                continue;
            }
            let parent = tree.branches()[branch].parent;
            let base = if let Some(parent) = parent.filter(|&p| !branch_faces[p].is_empty()) {
                let slot = branch_faces[parent]
                    .iter()
                    .copied()
                    .filter(|&f| faces[f].is_some())
                    .min_by(|&a, &b| {
                        let score = |f: usize| {
                            let p = faces[f]
                                .unwrap()
                                .map(|i| mesh.vertices[i as usize].position);
                            let center = p.iter().sum::<Vec3>() * 0.25;
                            let normal = (p[1] - p[0]).cross(p[3] - p[0]).normalize_or_zero();
                            // Prefer the nearest outward-facing aperture. Top caps are
                            // eligible, so terminal forks don't require another joint mesh.
                            let outward = normal.dot((end - center).normalize_or_zero());
                            (center - start).length_squared()
                                + (1. - outward) * (end - start).length_squared()
                        };
                        score(a).total_cmp(&score(b)).then(a.cmp(&b))
                    })
                    .ok_or_else(|| anyhow::anyhow!("tree branch has no free junction"))?;
                faces[slot].take().unwrap()
            } else {
                let (u, _) = crate::util::stable_perpendicular_basis(direction);
                let v = direction.cross(u).normalize();
                let base = mesh.ring(
                    [u, v, -u, -v].map(|r| start + r * first.radius_a()),
                    SkinBinding {
                        branch,
                        parent,
                        weight: 0.,
                    },
                )?;
                branch_faces[branch].push(faces.len());
                faces.push(Some([base[3], base[2], base[1], base[0]]));
                base
            };
            let base_positions = base.map(|i| mesh.vertices[i as usize].position);
            let base_center = base_positions.iter().sum::<Vec3>() * 0.25;
            let axis = (end - base_center).try_normalize().unwrap_or(direction);
            let offset = base_positions[0] - base_center;
            let u = (offset - axis * offset.dot(axis))
                .try_normalize()
                .unwrap_or_else(|| crate::util::stable_perpendicular_basis(axis).0);
            let v = axis.cross(u).normalize();
            // More child sockets require more *geometric* sections, not dropping
            // children when several authored branches share an attachment point.
            let sections = path.len().max(children[branch] + 1).max(2);
            let mut previous = base;
            for step in 1..=sections {
                let t = step as f32 / sections as f32;
                let sample = t * path.len() as f32;
                let segment = (sample.floor() as usize).min(path.len() - 1);
                let f = (sample - segment as f32).min(1.);
                let cone = path[segment];
                let center =
                    cone.center_a().lerp(cone.center_b(), f) + (base_center - start) * (1. - t);
                let radius = cone.radius_a() * (1. - f) + cone.radius_b() * f;
                let blend = (t * 4.).min(1.);
                let blend = blend * blend * (3. - 2. * blend);
                let radial = [u, v, -u, -v];
                let points = std::array::from_fn(|i| {
                    center + (base_positions[i] - base_center).lerp(radial[i] * radius, blend)
                });
                let ring = mesh.ring(
                    points,
                    SkinBinding {
                        branch,
                        parent,
                        weight: t * t * (3. - 2. * t),
                    },
                )?;
                for side in 0..4 {
                    let next = (side + 1) % 4;
                    branch_faces[branch].push(faces.len());
                    faces.push(Some([
                        previous[side],
                        previous[next],
                        ring[next],
                        ring[side],
                    ]));
                }
                previous = ring;
            }
            branch_faces[branch].push(faces.len());
            faces.push(Some(previous));
        }
        for face in faces.into_iter().flatten() {
            mesh.indices
                .extend([face[0], face[1], face[2], face[0], face[2], face[3]]);
        }
        for tri in mesh.indices.chunks_exact(3) {
            let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| mesh.vertices[i as usize].position);
            let normal = (b - a).cross(c - a);
            for &i in tri {
                mesh.vertices[i as usize].normal += normal;
            }
        }
        for v in &mut mesh.vertices {
            v.normal = v.normal.try_normalize().unwrap_or(Vec3::Y);
            ensure!(
                v.position.is_finite() && v.normal.is_finite(),
                "invalid procedural wood vertex"
            );
        }
        Ok(mesh)
    }

    fn ring(&mut self, points: [Vec3; 4], binding: SkinBinding) -> Result<[u32; 4]> {
        let base = u32::try_from(self.vertices.len())?;
        ensure!(base <= u32::MAX - 4, "wood vertex address space exhausted");
        self.vertices.extend(points.map(|position| WoodVertex {
            position,
            normal: Vec3::ZERO,
            binding,
        }));
        Ok([base, base + 1, base + 2, base + 3])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree_gen::{pose::TreePose, TreeDesc};
    use std::collections::BTreeMap;

    #[test]
    fn wood_is_closed_welded_and_deterministic_across_seeds_and_ages() {
        for seed in [1, 42, 122] {
            for age in [0., 0.5, 1.] {
                let mut desc = TreeDesc::default();
                desc.branching.seed = seed;
                let tree = Tree::new(desc.at_age(age));
                let mesh = WoodMesh::from_tree(&tree).unwrap();
                assert!(!mesh.indices.is_empty());
                let again = WoodMesh::from_tree(&tree).unwrap();
                assert_eq!(mesh.indices, again.indices);
                assert!(mesh
                    .vertices
                    .iter()
                    .zip(&again.vertices)
                    .all(|(a, b)| a.position == b.position
                        && a.normal == b.normal
                        && a.binding == b.binding));
                let mut edges = BTreeMap::<(u32, u32), (u32, i32)>::new();
                for t in mesh.indices.chunks_exact(3) {
                    for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                        let e = edges.entry((a.min(b), a.max(b))).or_default();
                        e.0 += 1;
                        e.1 += if a < b { 1 } else { -1 };
                    }
                }
                assert!(
                    edges.values().all(|&e| e == (2, 0)),
                    "open or inconsistently wound fork"
                );
                assert_eq!(
                    mesh.vertices.len() as i64 - edges.len() as i64
                        + (mesh.indices.len() / 3) as i64,
                    2
                );
            }
        }
    }

    #[test]
    fn shared_forks_stay_connected_during_wind_and_have_finite_normals() {
        let tree = Tree::new(TreeDesc::default());
        let mesh = WoodMesh::from_tree(&tree).unwrap();
        let mut pose = TreePose::new(tree.branches(), Vec3::ZERO).unwrap();
        for _ in 0..60 {
            pose.advance(
                &crate::wind_field::WindFieldFrame::uniform(glam::Vec2::new(5., 2.)),
                1. / 60.,
            )
            .unwrap();
        }
        for v in &mesh.vertices {
            let transform = v.binding.transform(pose.branches()).unwrap();
            assert!(transform.point(v.position / 256.).is_finite());
            assert!((transform.normal(v.normal).length() - 1.).abs() < 1e-5);
        }
        // Each vertex owns exactly one binding, including the parent/child aperture.
        assert!(mesh
            .indices
            .iter()
            .all(|&i| (i as usize) < mesh.vertices.len()));
    }
}
