//! Backport Parry's native voxel prediction fix without upgrading unrelated query algorithms.
use rapier3d::parry::math::{Pose, Real, Vector};
use rapier3d::parry::query::details::NormalConstraints;
use rapier3d::parry::query::{
    ClosestPoints, Contact, ContactManifold, ContactManifoldsWorkspace, DefaultQueryDispatcher,
    NonlinearRigidMotion, PersistentQueryDispatcher, QueryDispatcher, ShapeCastHit,
    ShapeCastOptions, Unsupported,
};
use rapier3d::parry::shape::Shape;

mod manifolds;
mod reduction;

pub(super) struct ContactPrediction;

// All non-manifold queries, including character and CCD shape casts, are unchanged.
impl QueryDispatcher for ContactPrediction {
    fn intersection_test(
        &self,
        pos12: &Pose,
        g1: &dyn Shape,
        g2: &dyn Shape,
    ) -> Result<bool, Unsupported> {
        DefaultQueryDispatcher.intersection_test(pos12, g1, g2)
    }

    fn distance(&self, pos12: &Pose, g1: &dyn Shape, g2: &dyn Shape) -> Result<Real, Unsupported> {
        DefaultQueryDispatcher.distance(pos12, g1, g2)
    }

    fn contact(
        &self,
        pos12: &Pose,
        g1: &dyn Shape,
        g2: &dyn Shape,
        prediction: Real,
    ) -> Result<Option<Contact>, Unsupported> {
        DefaultQueryDispatcher.contact(pos12, g1, g2, prediction)
    }

    fn closest_points(
        &self,
        pos12: &Pose,
        g1: &dyn Shape,
        g2: &dyn Shape,
        max_dist: Real,
    ) -> Result<ClosestPoints, Unsupported> {
        DefaultQueryDispatcher.closest_points(pos12, g1, g2, max_dist)
    }

    fn cast_shapes(
        &self,
        pos12: &Pose,
        local_vel12: Vector,
        g1: &dyn Shape,
        g2: &dyn Shape,
        options: ShapeCastOptions,
    ) -> Result<Option<ShapeCastHit>, Unsupported> {
        DefaultQueryDispatcher.cast_shapes(pos12, local_vel12, g1, g2, options)
    }

    fn cast_shapes_nonlinear(
        &self,
        motion1: &NonlinearRigidMotion,
        g1: &dyn Shape,
        motion2: &NonlinearRigidMotion,
        g2: &dyn Shape,
        start_time: Real,
        end_time: Real,
        stop_at_penetration: bool,
    ) -> Result<Option<ShapeCastHit>, Unsupported> {
        DefaultQueryDispatcher.cast_shapes_nonlinear(
            motion1,
            g1,
            motion2,
            g2,
            start_time,
            end_time,
            stop_at_penetration,
        )
    }
}

impl<ManifoldData: Default + Clone, ContactData: Default + Copy>
    PersistentQueryDispatcher<ManifoldData, ContactData> for ContactPrediction
{
    fn contact_manifolds(
        &self,
        pos12: &Pose,
        g1: &dyn Shape,
        g2: &dyn Shape,
        prediction: Real,
        manifolds: &mut Vec<ContactManifold<ManifoldData, ContactData>>,
        workspace: &mut Option<ContactManifoldsWorkspace>,
    ) -> Result<(), Unsupported> {
        // The specialized voxel/ball generator already includes prediction. Override only
        // the convex pairs that otherwise reach Parry 0.29's faulty voxels/shape generator.
        let convex =
            |shape: &dyn Shape| shape.as_support_map().is_some() && shape.as_ball().is_none();
        if (g1.as_voxels().is_some() && convex(g2)) || (g2.as_voxels().is_some() && convex(g1)) {
            manifolds::contact_manifolds_voxels_shape_shapes(
                &DefaultQueryDispatcher,
                pos12,
                g1,
                g2,
                prediction,
                manifolds,
                workspace,
            );
        } else {
            DefaultQueryDispatcher
                .contact_manifolds(pos12, g1, g2, prediction, manifolds, workspace)?;
        }
        // Rapier 0.34 reduces manifolds with its bare prediction distance, discarding
        // support inside the skin before the solver subtracts that skin from distances.
        // Use the same four-point selection here with the full distance Rapier supplied.
        // The solver still receives the original geometric points/distances and material.
        for manifold in manifolds {
            reduction::reduce(manifold, prediction);
        }
        Ok(())
    }

    fn contact_manifold_convex_convex(
        &self,
        pos12: &Pose,
        g1: &dyn Shape,
        g2: &dyn Shape,
        normal_constraints1: Option<&dyn NormalConstraints>,
        normal_constraints2: Option<&dyn NormalConstraints>,
        prediction: Real,
        manifold: &mut ContactManifold<ManifoldData, ContactData>,
    ) -> Result<(), Unsupported> {
        DefaultQueryDispatcher.contact_manifold_convex_convex(
            pos12,
            g1,
            g2,
            normal_constraints1,
            normal_constraints2,
            prediction,
            manifold,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rapier3d::parry::math::IVector;
    use rapier3d::parry::shape::{Cuboid, Voxels};

    #[test]
    fn voxel_convex_contacts_cover_the_prediction_gap_in_both_orders() {
        let coordinates = (0..8)
            .flat_map(|x| (0..8).map(move |z| IVector::new(x, 0, z)))
            .collect::<Vec<_>>();
        let floor = Voxels::new(Vector::ONE, &coordinates);
        let body = Cuboid::new(Vector::splat(2.0));
        for flipped in [false, true] {
            let mut manifolds: Vec<ContactManifold<(), ()>> = Vec::new();
            let mut workspace = None;
            // Maintain the same manifold cache from penetration through skin separation,
            // then remove out-of-range contacts and re-enter the prediction band.
            for gap in [-0.05, 0.05, 0.15, 0.25, 0.05] {
                let pose = Pose::translation(4.0, 3.0 + gap, 4.0);
                let (relative, g1, g2): (_, &dyn Shape, &dyn Shape) = if flipped {
                    (pose.inverse(), &body, &floor)
                } else {
                    (pose, &floor, &body)
                };
                ContactPrediction
                    .contact_manifolds(&relative, g1, g2, 0.2, &mut manifolds, &mut workspace)
                    .unwrap();
                let closest = manifolds
                    .iter()
                    .flat_map(|m| m.points.iter())
                    .map(|p| p.dist)
                    .min_by(f32::total_cmp);
                if gap < 0.2 {
                    let distance = closest.expect("predictive voxel support disappeared");
                    assert!(
                        (distance - gap).abs() < 1e-4,
                        "gap={gap} distance={distance} flipped={flipped}"
                    );
                } else {
                    assert!(closest.is_none(), "contact beyond prediction band");
                }
            }
        }
    }
}
