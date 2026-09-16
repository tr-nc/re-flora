//! Exact, topology-stable composite collider updates, independent of tree rendering.
use super::to_rapier_vec;
use glam::Vec3;
use rapier3d::parry::{
    bounding_volume::{Aabb, BoundingSphere},
    mass_properties::MassProperties,
    partitioning::{Bvh, BvhBuildStrategy},
    query::{
        details::NormalConstraints, PointProjection, PointQuery, Ray, RayCast, RayIntersection,
    },
    shape::{
        CompositeShape, CompositeShapeRef, Cuboid, FeatureId, ShapeType, Triangle,
        TypedCompositeShape, TypedShape,
    },
};
use rapier3d::prelude::{Pose, Shape, Vector};

/// Geometry in physics units. Empty input removes the collider. An invalid update
/// is rejected before touching the previously published geometry.
pub enum DeformingGeometry<'a> {
    Triangles {
        positions: &'a [Vec3],
        indices: &'a [[u32; 3]],
    },
    Boxes {
        centers: &'a [Vec3],
        half_extents: Vec3,
    },
}
impl DeformingGeometry<'_> {
    pub(crate) fn len(&self) -> usize {
        match self {
            Self::Triangles { indices, .. } => indices.len(),
            Self::Boxes { centers, .. } => centers.len(),
        }
    }
    pub(crate) fn validate(&self) -> Result<(), String> {
        let valid = match self {
            Self::Triangles { positions, indices } => {
                positions.iter().all(|p| p.is_finite())
                    && indices
                        .iter()
                        .flatten()
                        .all(|&i| (i as usize) < positions.len())
            }
            Self::Boxes {
                centers,
                half_extents,
            } => {
                centers.iter().all(|p| p.is_finite())
                    && half_extents.is_finite()
                    && half_extents.cmpgt(Vec3::ZERO).all()
            }
        };
        if valid {
            Ok(())
        } else {
            Err("invalid deforming collider geometry".into())
        }
    }
}

#[derive(Clone)]
enum Parts {
    Triangles {
        positions: Vec<Vec3>,
        indices: Vec<[u32; 3]>,
    },
    Boxes {
        centers: Vec<Vec3>,
        cuboid: Cuboid,
    },
}
#[derive(Clone)]
pub(crate) struct DeformingShape {
    parts: Parts,
    bvh: Bvh,
}
impl DeformingShape {
    pub(crate) fn new(input: &DeformingGeometry<'_>) -> Self {
        let parts = match input {
            DeformingGeometry::Triangles { positions, indices } => Parts::Triangles {
                positions: positions.to_vec(),
                indices: indices.to_vec(),
            },
            DeformingGeometry::Boxes {
                centers,
                half_extents,
            } => Parts::Boxes {
                centers: centers.to_vec(),
                cuboid: Cuboid::new(to_rapier_vec(*half_extents)),
            },
        };
        let mut shape = Self {
            parts,
            bvh: Bvh::default(),
        };
        shape.bvh = Bvh::from_iter(
            BvhBuildStrategy::Binned,
            (0..input.len()).map(|i| (i, shape.part_aabb(i))),
        );
        shape
    }
    pub(crate) fn matches_topology(&self, input: &DeformingGeometry<'_>) -> bool {
        match (&self.parts, input) {
            (
                Parts::Triangles {
                    positions: old,
                    indices: old_indices,
                },
                DeformingGeometry::Triangles { positions, indices },
            ) => old.len() == positions.len() && old_indices == indices,
            (
                Parts::Boxes {
                    centers: old,
                    cuboid,
                },
                DeformingGeometry::Boxes {
                    centers,
                    half_extents,
                },
            ) => old.len() == centers.len() && cuboid.half_extents == to_rapier_vec(*half_extents),
            _ => false,
        }
    }
    pub(crate) fn update(&mut self, input: &DeformingGeometry<'_>) {
        match (&mut self.parts, input) {
            (
                Parts::Triangles { positions: old, .. },
                DeformingGeometry::Triangles { positions, .. },
            ) => old.copy_from_slice(positions),
            (Parts::Boxes { centers: old, .. }, DeformingGeometry::Boxes { centers, .. }) => {
                old.copy_from_slice(centers)
            }
            _ => unreachable!("topology checked before update"),
        }
        for i in 0..input.len() {
            self.bvh
                .insert_or_update_partially(self.part_aabb(i), i as u32, 0.);
        }
        self.bvh.refit_without_opt();
    }
    fn part_aabb(&self, i: usize) -> Aabb {
        match &self.parts {
            Parts::Triangles { positions, indices } => {
                let [a, b, c] = indices[i].map(|j| positions[j as usize]);
                Aabb::new(
                    to_rapier_vec(a.min(b).min(c)),
                    to_rapier_vec(a.max(b).max(c)),
                )
            }
            Parts::Boxes { centers, cuboid } => Aabb::new(
                to_rapier_vec(centers[i]) - cuboid.half_extents,
                to_rapier_vec(centers[i]) + cuboid.half_extents,
            ),
        }
    }
    fn composite(&self) -> CompositeShapeRef<'_, Self> {
        CompositeShapeRef(self)
    }
}
impl CompositeShape for DeformingShape {
    fn map_part_at(
        &self,
        id: u32,
        f: &mut dyn FnMut(Option<&Pose>, &dyn Shape, Option<&dyn NormalConstraints>),
    ) {
        match &self.parts {
            Parts::Triangles { positions, indices } => {
                if let Some(t) = indices.get(id as usize) {
                    let [a, b, c] = t.map(|j| positions[j as usize]);
                    f(
                        None,
                        &Triangle::new(to_rapier_vec(a), to_rapier_vec(b), to_rapier_vec(c)),
                        None,
                    );
                }
            }
            Parts::Boxes { centers, cuboid } => {
                if let Some(center) = centers.get(id as usize) {
                    f(
                        Some(&Pose::translation(center.x, center.y, center.z)),
                        cuboid,
                        None,
                    );
                }
            }
        }
    }
    fn bvh(&self) -> &Bvh {
        &self.bvh
    }
}
impl TypedCompositeShape for DeformingShape {
    type PartShape = dyn Shape;
    type PartNormalConstraints = dyn NormalConstraints;
    fn map_typed_part_at<T>(
        &self,
        id: u32,
        mut f: impl FnMut(Option<&Pose>, &Self::PartShape, Option<&Self::PartNormalConstraints>) -> T,
    ) -> Option<T> {
        let mut result = None;
        self.map_part_at(id, &mut |pose, part, normals| {
            result = Some(f(pose, part, normals));
        });
        result
    }
    fn map_untyped_part_at<T>(
        &self,
        id: u32,
        f: impl FnMut(Option<&Pose>, &dyn Shape, Option<&dyn NormalConstraints>) -> T,
    ) -> Option<T> {
        self.map_typed_part_at(id, f)
    }
}
impl RayCast for DeformingShape {
    fn cast_local_ray_and_get_normal(
        &self,
        ray: &Ray,
        max: f32,
        solid: bool,
    ) -> Option<RayIntersection> {
        self.composite()
            .cast_local_ray_and_get_normal(ray, max, solid)
            .map(|hit| hit.1)
    }
}
impl PointQuery for DeformingShape {
    fn project_local_point(&self, point: Vector, solid: bool) -> PointProjection {
        self.composite()
            .project_local_point(point, f32::MAX, solid)
            .expect("nonempty geometry")
            .1
    }
    fn project_local_point_and_get_feature(&self, point: Vector) -> (PointProjection, FeatureId) {
        (self.project_local_point(point, false), FeatureId::Unknown)
    }
}
impl Shape for DeformingShape {
    fn clone_dyn(&self) -> Box<dyn Shape> {
        Box::new(self.clone())
    }
    // This private shape is only installed on fixed colliders in physics units.
    fn scale_dyn(&self, _: Vector, _: u32) -> Option<Box<dyn Shape>> {
        None
    }
    fn compute_local_aabb(&self) -> Aabb {
        self.bvh.root_aabb()
    }
    fn compute_local_bounding_sphere(&self) -> BoundingSphere {
        self.compute_local_aabb().bounding_sphere()
    }
    fn mass_properties(&self, _: f32) -> MassProperties {
        MassProperties::default()
    }
    fn shape_type(&self) -> ShapeType {
        ShapeType::Custom
    }
    fn as_typed_shape(&self) -> TypedShape<'_> {
        TypedShape::Custom(self)
    }
    fn ccd_thickness(&self) -> f32 {
        match &self.parts {
            Parts::Triangles { .. } => 0.,
            Parts::Boxes { cuboid, .. } => cuboid.half_extents.min_element(),
        }
    }
    fn ccd_angular_thickness(&self) -> f32 {
        std::f32::consts::FRAC_PI_2
    }
    fn as_composite_shape(&self) -> Option<&dyn CompositeShape> {
        Some(self)
    }
}
