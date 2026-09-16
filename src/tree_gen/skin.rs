//! Deterministic rest-space bindings shared by coincident surface vertices.
//! A hit retains the original triangle coordinates for editing after deformation.
use anyhow::{ensure, Result};
use glam::{Mat3, Vec3};

use super::{pose::BranchPose, Tree};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkinBinding {
    pub branch: usize,
    pub parent: Option<usize>,
    pub weight: f32,
}

impl SkinBinding {
    /// Bind once when the tree's rest shape changes, never after moving vertices.
    /// Cone ownership comes from the generator, including its subdivision/culling.
    pub fn at_rest_position(tree: &Tree, local_voxels: Vec3) -> Result<Self> {
        ensure!(local_voxels.is_finite(), "nonfinite skin position");
        let cone = tree
            .trunks()
            .iter()
            .enumerate()
            .min_by(|(ia, a), (ib, b)| {
                a.signed_distance(local_voxels)
                    .total_cmp(&b.signed_distance(local_voxels))
                    .then(ia.cmp(ib))
            })
            .map(|(i, _)| i)
            .ok_or_else(|| anyhow::anyhow!("cannot bind surface without tree wood"))?;
        let branch = tree.trunk_branch_indices()[cone];
        let segment = &tree.branches()[branch];
        let axis = segment.end - segment.start;
        let t = if axis.length_squared() > 1e-12 {
            ((local_voxels - segment.start).dot(axis) / axis.length_squared()).clamp(0., 1.)
        } else {
            0.
        };
        Ok(Self {
            branch,
            parent: segment.parent,
            weight: t * t * (3. - 2. * t),
        })
    }

    /// Attachments authored at a branch tip inherit the complete branch pose.
    pub fn branch_tip(tree: &Tree, branch: usize) -> Result<Self> {
        ensure!(
            branch < tree.branches().len(),
            "attachment branch out of range"
        );
        Ok(Self {
            branch,
            parent: tree.branches()[branch].parent,
            weight: 1.,
        })
    }

    pub fn transform(self, poses: &[BranchPose]) -> Result<SkinTransform> {
        ensure!(
            self.weight.is_finite() && (0. ..=1.).contains(&self.weight),
            "invalid skin weight"
        );
        let child = *poses
            .get(self.branch)
            .ok_or_else(|| anyhow::anyhow!("skin branch missing from pose"))?;
        let parent = match self.parent {
            Some(index) => *poses
                .get(index)
                .ok_or_else(|| anyhow::anyhow!("skin parent missing from pose"))?,
            None => BranchPose {
                rotation: glam::Quat::IDENTITY,
                translation: Vec3::ZERO,
            },
        };
        let linear = Mat3::from_quat(parent.rotation) * (1. - self.weight)
            + Mat3::from_quat(child.rotation) * self.weight;
        ensure!(
            linear.is_finite() && linear.determinant().abs() > 1e-6,
            "singular tree skin transform"
        );
        Ok(SkinTransform {
            linear,
            translation: parent.translation.lerp(child.translation, self.weight),
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SkinTransform {
    linear: Mat3,
    translation: Vec3,
}
impl SkinTransform {
    pub fn point(self, rest_world: Vec3) -> Vec3 {
        self.linear * rest_world + self.translation
    }
    pub fn normal(self, rest_normal: Vec3) -> Vec3 {
        (self.linear.inverse().transpose() * rest_normal).normalize_or_zero()
    }
    pub fn inverse_point(self, world: Vec3) -> Vec3 {
        self.linear.inverse() * (world - self.translation)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SurfaceHit {
    pub distance: f32,
    pub world_position: Vec3,
    pub rest_position: Vec3,
    pub normal: Vec3,
}

/// Double-sided triangle hit, including back-facing wood exposed by an edit.
/// Direction is normalized here so distance is always in world units.
/// Mapping with triangle barycentrics also works when the three vertices have
/// different bindings; inverting one bone at the hit point would be incorrect.
pub fn intersect_surface_triangle(
    origin: Vec3,
    direction: Vec3,
    posed: [Vec3; 3],
    rest: [Vec3; 3],
) -> Option<SurfaceHit> {
    if !origin.is_finite() || !direction.is_finite() {
        return None;
    }
    let direction = direction.normalize_or_zero();
    let e1 = posed[1] - posed[0];
    let e2 = posed[2] - posed[0];
    let p = direction.cross(e2);
    let determinant = e1.dot(p);
    if !determinant.is_finite() || determinant.abs() < 1e-12 {
        return None;
    }
    let s = origin - posed[0];
    let u = s.dot(p) / determinant;
    let q = s.cross(e1);
    let v = direction.dot(q) / determinant;
    let distance = e2.dot(q) / determinant;
    if u < -1e-6 || v < -1e-6 || u + v > 1. + 1e-6 || distance < 0. {
        return None;
    }
    Some(SurfaceHit {
        distance,
        world_position: origin + direction * distance,
        rest_position: rest[0] * (1. - u - v) + rest[1] * u + rest[2] * v,
        normal: e1.cross(e2).normalize_or_zero(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        tree_gen::{pose::TreePose, TreeDesc},
        wind_field::WindFieldFrame,
    };
    use glam::Vec2;
    #[test]
    fn rest_bindings_are_deterministic_and_deformation_is_invertible() {
        let tree = Tree::new(TreeDesc::default());
        let origin = Vec3::new(1., 0.3, 1.);
        let mut pose = TreePose::new(tree.branches(), origin).unwrap();
        for _ in 0..120 {
            pose.advance(&WindFieldFrame::uniform(Vec2::new(6., 2.)), 1. / 60.)
                .unwrap();
        }
        for cone in tree.trunks() {
            for point in [
                cone.center_a(),
                cone.center_b(),
                (cone.center_a() + cone.center_b()) * 0.5,
            ] {
                let binding = SkinBinding::at_rest_position(&tree, point).unwrap();
                // Independently generated copies at cell/region boundaries must agree.
                assert_eq!(
                    binding,
                    SkinBinding::at_rest_position(&tree, point).unwrap()
                );
                let transform = binding.transform(pose.branches()).unwrap();
                let rest = origin + point / 256.;
                assert!(
                    transform
                        .inverse_point(transform.point(rest))
                        .distance(rest)
                        < 2e-6
                );
                assert!((transform.normal(Vec3::Y).length() - 1.).abs() < 1e-6);
            }
        }
        let root = SkinBinding::at_rest_position(&tree, Vec3::ZERO).unwrap();
        assert!(
            root.transform(pose.branches())
                .unwrap()
                .point(origin)
                .distance(origin)
                < 1e-6
        );
    }
    #[test]
    fn attachments_use_authored_branch_identity() {
        let tree = Tree::new(TreeDesc::default());
        let mut pose = TreePose::new(tree.branches(), Vec3::ZERO).unwrap();
        pose.advance(&WindFieldFrame::uniform(Vec2::X * 4.), 0.2)
            .unwrap();
        for (&branch, leaf) in tree
            .leaf_branch_indices()
            .iter()
            .zip(tree.relative_leaf_placements())
        {
            let binding = SkinBinding::branch_tip(&tree, branch).unwrap();
            assert!(
                binding
                    .transform(pose.branches())
                    .unwrap()
                    .point(leaf.anchor / 256.)
                    .distance(pose.branches()[branch].transform_point(leaf.anchor / 256.))
                    < 1e-6
            );
        }
    }
    #[test]
    fn hit_maps_to_rest_triangle_after_nonrigid_deformation() {
        let rest = [Vec3::ZERO, Vec3::X, Vec3::Y];
        let posed = [
            Vec3::new(2., 0., 1.),
            Vec3::new(3., 0., 1.5),
            Vec3::new(2., 2., 1.3),
        ];
        let target = posed[0] * 0.5 + posed[1] * 0.2 + posed[2] * 0.3;
        for sign in [-1., 1.] {
            let hit =
                intersect_surface_triangle(target + Vec3::Z * sign, -Vec3::Z * sign, posed, rest)
                    .unwrap();
            assert!(hit.world_position.distance(target) < 1e-6);
            assert!(hit.rest_position.distance(Vec3::new(0.2, 0.3, 0.)) < 1e-6);
            assert!((hit.distance - 1.).abs() < 1e-6);
            assert!((hit.normal.length() - 1.).abs() < 1e-6);
        }
        assert!(intersect_surface_triangle(target, Vec3::ZERO, posed, rest).is_none());
        assert!(intersect_surface_triangle(Vec3::splat(100.), Vec3::Z, posed, rest).is_none());
    }
}
