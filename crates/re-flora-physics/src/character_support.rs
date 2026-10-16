//! A contact can block the capsule without being a usable foothold for walking uphill.
//!
//! Search for a small, body-scaled patch beside the contact, not the entire capsule
//! footprint at its current position. This admits the rounded-cap transition onto a
//! real stair (including walking beside its edge). A bounded ground-detail allowance
//! permits a low bump above that patch, not an arbitrarily tall narrow ridge. Each
//! probe must reach real support within the allowance BELOW THE CONTACT, not below
//! the already elevated capsule. Rechecking this terrain-relative bound prevents
//! repeated small moves from ratcheting up a wall. All probes use the published
//! collision world; there is no second terrain cache or accumulated climb budget.
use super::{
    CAPSULE_CHARACTER_COLLISION_OFFSET, CAPSULE_CHARACTER_GROUND_NORMAL_MIN_DOT,
    CAPSULE_CHARACTER_MAX_SLOPE_CLIMB_ANGLE,
};
use rapier3d::prelude::{Pose, QueryPipeline, Ray, Shape, ShapeCastHit, Vector};

pub(super) fn ground_contact(pose: &Pose, hit: &ShapeCastHit) -> Vector {
    // witness1 can be voxel-local on composite terrain. witness2 is capsule-local.
    *pose * hit.witness2
        - Vector::Y * hit.time_of_impact
        - hit.normal1 * CAPSULE_CHARACTER_COLLISION_OFFSET
}

pub(super) fn has_foothold(
    queries: &QueryPipeline<'_>,
    shape: &dyn Shape,
    contact: Vector,
    normal: Vector,
    horizontal: Vector,
    max_ground_detail_height: f32,
) -> bool {
    let inward = Vector::new(-normal.x, 0.0, -normal.z);
    let inward = if inward.length_squared() > 1.0e-4 {
        Some(inward.normalize())
    } else {
        horizontal.try_normalize()
    };
    let Some(inward) = inward else {
        return false;
    };
    let extents = shape.compute_local_aabb().half_extents();
    let radius = extents.x;
    let probe_rise = extents
        .y
        .min(1.5 * radius * CAPSULE_CHARACTER_MAX_SLOPE_CLIMB_ANGLE.tan())
        + CAPSULE_CHARACTER_COLLISION_OFFSET;
    // A rounded corner's normal depends on the approach, not on the available
    // platform's orientation. Search its inward half-plane rather than demanding
    // a patch centered on that one normal (which would reject broad stair corners).
    let diagonal = std::f32::consts::FRAC_1_SQRT_2;
    let directions = [
        Vector::X,
        Vector::new(diagonal, 0.0, diagonal),
        Vector::Z,
        Vector::new(-diagonal, 0.0, diagonal),
        -Vector::X,
        Vector::new(-diagonal, 0.0, -diagonal),
        -Vector::Z,
        Vector::new(diagonal, 0.0, -diagonal),
    ];
    std::iter::once(inward)
        .chain(directions)
        .filter(|direction| direction.dot(inward) >= 0.0)
        .any(|direction| {
            [0.0, -0.5, 0.5].into_iter().any(|side_shift| {
                let side = direction.cross(Vector::Y);
                let edge = contact
                    + side * (side_shift * (radius + 2.0 * CAPSULE_CHARACTER_COLLISION_OFFSET));
                patch_is_supported(
                    queries,
                    edge,
                    direction,
                    radius,
                    probe_rise,
                    max_ground_detail_height,
                )
            })
        })
}

fn patch_is_supported(
    queries: &QueryPipeline<'_>,
    contact: Vector,
    inward: Vector,
    radius: f32,
    probe_rise: f32,
    max_ground_detail_height: f32,
) -> bool {
    let side = inward.cross(Vector::Y);
    // Minimum foothold width AND depth are one capsule radius (four player voxels).
    // Start at the contacted edge, not half a radius beyond it: that would reject
    // a valid platform near its far end. Side shifts let the contact lie at either
    // corner or the middle of the patch's front edge during a stair transition.
    for forward in [0.0, 0.5, 1.0] {
        for across in [-0.5, 0.0, 0.5] {
            let origin = contact
                + inward * (forward * radius + CAPSULE_CHARACTER_COLLISION_OFFSET)
                + side * (across * radius)
                + Vector::Y * probe_rise;
            let Some((_, hit)) = queries.cast_ray_and_get_normal(
                &Ray::new(origin, -Vector::Y),
                probe_rise + max_ground_detail_height + CAPSULE_CHARACTER_COLLISION_OFFSET,
                true,
            ) else {
                return false;
            };
            // Starting inside a wall/ceiling is not proof of a top surface. Every
            // sample must land in this contact-relative height band. The lower floor
            // can support a small bump, but stops qualifying as soon as a thin ridge
            // exceeds the allowance. A miss is never filled with character height.
            // With zero allowance this is exactly the original strict support test.
            if hit.time_of_impact <= 1.0e-3
                || hit.normal.y < CAPSULE_CHARACTER_GROUND_NORMAL_MIN_DOT
            {
                return false;
            }
        }
    }
    true
}
