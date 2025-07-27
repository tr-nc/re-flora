use crate::geom::Aabb3;
use glam::{Mat4, Vec3};

/// Returns (view_matrix, projection_matrix)
pub fn calculate_directional_light_matrices(
    world_bound: Aabb3,
    light_direction: Vec3,
) -> (Mat4, Mat4) {
    const TOLERANCE: f32 = 0.1;
    // cos(ϑ) threshold for “almost parallel”
    const PARALLEL_EPS: f32 = 0.999;

    let frustum_corners = world_bound.get_corners();

    let mut frustum_center = Vec3::ZERO;
    for c in &frustum_corners {
        frustum_center += *c;
    }
    frustum_center *= 1.0 / 8.0;

    let dir_n = light_direction.normalize();
    let mut up = Vec3::Y;
    if dir_n.dot(up).abs() > PARALLEL_EPS {
        // if parallel to y, fall back to z.  (z can also be x, any axis works.)
        up = Vec3::Z;
    }

    let view_matrix = Mat4::look_at_rh(frustum_center + light_direction, frustum_center, up);

    // 4. Transform the corners to light space and compute the ortho bounds
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    for c in &frustum_corners {
        let p = (view_matrix * c.extend(1.0)).truncate();
        min = min.min(p);
        max = max.max(p);
    }

    // because the viewing center is at the middle of the frustum, min.z is
    // the far plane and max.z is the near plane

    // in a right-handed camera space the camera looks along the -z axis
    let proj = Mat4::orthographic_rh(
        min.x - TOLERANCE,
        max.x + TOLERANCE,
        min.y - TOLERANCE,
        max.y + TOLERANCE,
        -(max.z + TOLERANCE),
        -(min.z - TOLERANCE),
    );

    let proj_matrix = Mat4::from_scale(Vec3::new(1.0, -1.0, 1.0)) * proj;

    (view_matrix, proj_matrix)
}
