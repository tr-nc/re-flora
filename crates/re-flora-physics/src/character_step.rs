//! Direction-preserving stair negotiation over the same terrain queries as Rapier's KCC.
//!
//! Rapier only autosteps contacts classified as walls. A capsule touching a voxel corner can
//! instead produce a walkable slope normal and slide sideways. Before accepting that slide,
//! test a bounded up/forward/down path for the entire capsule, keeping the requested XZ exactly.

use super::{capsule_ground_hit, character_support, CAPSULE_CHARACTER_COLLISION_OFFSET};
use rapier3d::parry::query::ShapeCastOptions;
use rapier3d::prelude::{Pose, QueryPipeline, Shape, Vector};

// Clear a tangent contact at the exact step limit without adding a voxel of reach.
const STEP_EPSILON: f32 = 1.0e-3;

pub(super) fn try_step(
    queries: &QueryPipeline<'_>,
    shape: &dyn Shape,
    pose: &Pose,
    horizontal: Vector,
    max_step_height: f32,
) -> Option<Vector> {
    let direction = horizontal.try_normalize()?;
    let options = |distance| ShapeCastOptions {
        max_time_of_impact: distance,
        target_distance: CAPSULE_CHARACTER_COLLISION_OFFSET,
        stop_at_penetration: false,
        compute_impact_geometry_on_penetration: true,
    };
    // A low ceiling limits the available rise, but need not prevent a smaller step.
    let sweep_height = max_step_height + STEP_EPSILON;
    let rise = queries
        .cast_shape(pose, Vector::Y, shape, options(sweep_height))
        .map_or(sweep_height, |(_, hit)| {
            (hit.time_of_impact - 1.0e-4).max(0.0)
        });
    if rise <= 1.0e-3 {
        return None;
    }
    let raised_pose = Pose::from_translation(Vector::Y * rise) * *pose;
    if queries
        .cast_shape(&raised_pose, direction, shape, options(horizontal.length()))
        .is_some()
    {
        return None;
    }

    let forward_pose = Pose::from_translation(horizontal) * raised_pose;
    // Require actual support at or above the starting height: this path cannot bridge a cliff,
    // turn a jump into a grounded move, or search arbitrarily far below for another floor.
    let ground = capsule_ground_hit(queries, shape, &forward_pose, rise)?;
    // Here the contact is the landing of an explicitly swept step. At the start of a stair,
    // its normal can be nearly horizontal because it touches the rounded capsule cap. Do not
    // classify that contact as a ramp again; require upward support and bound the actual voxel
    // contact height as well as the capsule's rise (the cap may extend below the step's top).
    let starting_bottom = pose.translation.y + shape.compute_local_aabb().mins.y;
    let contact = character_support::ground_contact(&forward_pose, &ground);
    let starting_support = starting_bottom - CAPSULE_CHARACTER_COLLISION_OFFSET;
    if ground.normal1.y <= 0.0 || contact.y - starting_support > max_step_height + STEP_EPSILON {
        return None;
    }
    if !character_support::has_foothold(queries, shape, contact, ground.normal1, horizontal) {
        return None;
    }
    Some(horizontal + Vector::Y * (rise - ground.time_of_impact))
}
