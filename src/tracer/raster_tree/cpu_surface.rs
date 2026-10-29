//! Exact interaction geometry: share only bit-identical rest points AND bindings.
//! The visual mesh keeps its per-cell normals/confidence and original GPU topology.
use super::{BranchPose, RasterTreeMesh, SkinBinding, Vec3};
use crate::tree_gen::skin::SkinTransform;
use anyhow::{ensure, Context, Result};
use std::{collections::HashMap, sync::Arc};

#[derive(Clone, Copy, Eq, PartialEq, Hash)]
struct BindingKey {
    tree: u32,
    branch: usize,
    parent: Option<usize>,
    weight: u32,
}
struct TransformSpec {
    tree: usize,
    binding: SkinBinding,
}
struct Point {
    rest: Vec3,
    transform: usize,
}

pub(super) struct CpuDeformationPlan {
    trees: Vec<u32>,
    transforms: Vec<TransformSpec>,
    points: Vec<Point>,
    visual_to_point: Vec<u32>,
    #[cfg(test)]
    triangles: Arc<[[u32; 3]]>,
}
impl CpuDeformationPlan {
    pub(super) fn compile(mesh: &RasterTreeMesh) -> Result<Self> {
        ensure!(
            mesh.bindings.len() == mesh.vertices.len(),
            "missing tree bindings"
        );
        ensure!(
            mesh.indices.len().is_multiple_of(3),
            "incomplete tree triangles"
        );
        let mut trees = Vec::new();
        let mut tree_ids = HashMap::new();
        let mut transforms = Vec::new();
        let mut transform_ids = HashMap::new();
        let mut points = Vec::new();
        let mut point_ids = HashMap::new();
        let mut visual_to_point = Vec::with_capacity(mesh.vertices.len());
        for (vertex, binding) in mesh.vertices.iter().zip(&mesh.bindings) {
            let (tree, binding) = binding.context("unbound GPU tree vertex")?;
            let tree_index = *tree_ids.entry(tree).or_insert_with(|| {
                trees.push(tree);
                trees.len() - 1
            });
            let key = BindingKey {
                tree,
                branch: binding.branch,
                parent: binding.parent,
                weight: binding.weight.to_bits(),
            };
            let transform = *transform_ids.entry(key).or_insert_with(|| {
                transforms.push(TransformSpec {
                    tree: tree_index,
                    binding,
                });
                transforms.len() - 1
            });
            let point = *point_ids
                .entry((vertex.position.map(f32::to_bits), transform))
                .or_insert_with(|| {
                    points.push(Point {
                        rest: Vec3::from_array(vertex.position),
                        transform,
                    });
                    points.len() - 1
                });
            visual_to_point
                .push(u32::try_from(point).context("CPU tree point addressing overflow")?);
        }
        ensure!(
            mesh.indices
                .iter()
                .all(|&i| (i as usize) < visual_to_point.len()),
            "CPU tree triangle out of range"
        );
        #[cfg(test)]
        let triangles = mesh
            .indices
            .chunks_exact(3)
            .map(|triangle| {
                Ok([
                    *visual_to_point
                        .get(triangle[0] as usize)
                        .context("CPU tree triangle out of range")?,
                    *visual_to_point
                        .get(triangle[1] as usize)
                        .context("CPU tree triangle out of range")?,
                    *visual_to_point
                        .get(triangle[2] as usize)
                        .context("CPU tree triangle out of range")?,
                ])
            })
            .collect::<Result<Vec<_>>>()?
            .into();
        Ok(Self {
            trees,
            transforms,
            points,
            visual_to_point,
            #[cfg(test)]
            triangles,
        })
    }

    pub(super) fn pose<'a>(
        self: &Arc<Self>,
        poses: impl Fn(u32) -> Option<&'a [BranchPose]>,
        scratch: Option<PosedTreeSurface>,
    ) -> Result<PosedTreeSurface> {
        let mut surface = scratch
            .filter(|s| Arc::ptr_eq(&s.plan, self))
            .unwrap_or_else(|| PosedTreeSurface {
                plan: self.clone(),
                positions: Vec::with_capacity(self.points.len()),
                transforms: Vec::with_capacity(self.transforms.len()),
                query_margin: Vec3::ZERO,
            });
        // Resolve the canonical pose only once per tree, not once per visual corner.
        let poses = self
            .trees
            .iter()
            .map(|&id| poses(id).context("tree pose missing"))
            .collect::<Result<Vec<_>>>()?;
        surface.transforms.clear();
        for spec in &self.transforms {
            surface
                .transforms
                .push(spec.binding.transform(poses[spec.tree])?);
        }
        surface.positions.clear();
        surface.query_margin = Vec3::ZERO;
        for point in &self.points {
            let position = surface.transforms[point.transform].point(point.rest);
            ensure!(position.is_finite(), "nonfinite posed tree position");
            // Bounds for read-only visual queries over the immutable rest BVH.
            // Every triangle lies in its rest AABB expanded by this componentwise
            // displacement; physics itself never receives the wind positions.
            surface.query_margin = surface.query_margin.max((position - point.rest).abs());
            surface.positions.push(position);
        }
        Ok(surface)
    }
    pub(super) fn counts(&self) -> (usize, usize) {
        (self.points.len(), self.transforms.len())
    }
}

/// Visual vertex indexing is preserved for GPU validation, rays, and rest-space editing.
/// Visual queries preserve the same ordered triangles over a smaller exact point array.
/// This complete visual oracle must not determine the physical trunk-only surface.
pub struct PosedTreeSurface {
    plan: Arc<CpuDeformationPlan>,
    positions: Vec<Vec3>,
    transforms: Vec<SkinTransform>,
    query_margin: Vec3,
}
impl PosedTreeSurface {
    #[cfg(test)]
    pub(crate) fn query_margin(&self) -> Vec3 {
        if self.query_margin == Vec3::ZERO {
            return Vec3::ZERO;
        }
        // Displacement subtraction can round down; bound it outward too.
        Vec3::new(
            self.query_margin.x.next_up(),
            self.query_margin.y.next_up(),
            self.query_margin.z.next_up(),
        )
    }
    pub fn vertex_count(&self) -> usize {
        self.plan.visual_to_point.len()
    }
    pub fn position(&self, index: usize) -> Vec3 {
        self.positions[self.plan.visual_to_point[index] as usize]
    }
    pub fn positions(&self) -> impl Iterator<Item = Vec3> + '_ {
        self.plan
            .visual_to_point
            .iter()
            .map(|&i| self.positions[i as usize])
    }
    pub fn is_finite(&self) -> bool {
        self.positions.iter().all(|p| p.is_finite())
    }
    #[cfg(test)]
    pub(crate) fn collision_positions(&self) -> &[Vec3] {
        &self.positions
    }
    #[cfg(test)]
    pub(crate) fn collision_triangles(&self) -> &Arc<[[u32; 3]]> {
        &self.plan.triangles
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracer::raster_tree::RasterTreeVertex;
    use glam::Quat;
    use std::cell::Cell;

    #[test]
    fn compact_points_preserve_full_binding_identity_and_scalar_results_bit_for_bit() {
        let skin = SkinBinding {
            branch: 0,
            parent: None,
            weight: 1.,
        };
        let mut mesh = RasterTreeMesh {
            vertices: [
                Vec3::ZERO,
                Vec3::X,
                Vec3::Y,
                Vec3::ZERO,
                Vec3::ZERO,
                Vec3::ZERO,
                Vec3::ZERO,
            ]
            .map(|p| RasterTreeVertex {
                position: p.to_array(),
                center: p.to_array(),
                normal: [0., 1., 0.],
                normal_confidence: 1.,
            })
            .to_vec(),
            ..Default::default()
        };
        mesh.vertices[3].normal_confidence = 0.; // Never merge visual shading facts.
        mesh.bindings = vec![Some((1, skin)); 7];
        mesh.bindings[4] = Some((2, skin));
        mesh.bindings[5] = Some((
            1,
            SkinBinding {
                weight: 0.25,
                ..skin
            },
        ));
        mesh.bindings[6] = Some((
            1,
            SkinBinding {
                parent: Some(1),
                ..skin
            },
        ));
        mesh.indices = vec![0, 1, 2, 3, 1, 2, 4, 1, 2, 5, 1, 2, 6, 1, 2];
        mesh.compile_cpu_deformation().unwrap();
        let plan = mesh.cpu_plan.as_ref().unwrap();
        assert_eq!(plan.counts(), (6, 4));
        let before = mesh.rest_fingerprint();
        let mut scratch = None;
        let mut pointer = None;
        for step in 0..12 {
            let branches = [
                BranchPose {
                    rotation: Quat::from_rotation_z(step as f32 * 0.13),
                    translation: Vec3::X * 0.1,
                },
                BranchPose {
                    rotation: Quat::from_rotation_x(0.2),
                    translation: Vec3::Z * 0.2,
                },
            ];
            let calls = Cell::new(0);
            let surface = mesh
                .posed_surface_reusing(
                    |id| {
                        calls.set(calls.get() + 1);
                        (id == 1 || id == 2).then_some(branches.as_slice())
                    },
                    scratch,
                )
                .unwrap();
            assert_eq!(calls.get(), 2);
            assert_eq!(surface.vertex_count(), 7);
            assert_eq!(surface.collision_positions().len(), 6);
            assert_eq!(surface.collision_triangles().len(), 5);
            for (i, (vertex, binding)) in mesh.vertices.iter().zip(&mesh.bindings).enumerate() {
                let (_, skin) = binding.unwrap();
                let scalar = skin
                    .transform(&branches)
                    .unwrap()
                    .point(Vec3::from_array(vertex.position));
                assert_eq!(
                    surface.position(i).to_array().map(f32::to_bits),
                    scalar.to_array().map(f32::to_bits)
                );
            }
            assert_eq!(
                surface.collision_triangles()[0],
                surface.collision_triangles()[1]
            );
            if let Some(old) = pointer {
                assert_eq!(old, surface.positions.as_ptr());
            }
            pointer = Some(surface.positions.as_ptr());
            scratch = Some(surface);
        }
        assert_eq!(before, mesh.rest_fingerprint());
        // Missing canonical branches and singular transforms still reject before publication.
        assert!(mesh.posed_surface(|_| Some(&[])).is_err());
        assert!(mesh.posed_surface(|_| None).is_err());
    }
}
