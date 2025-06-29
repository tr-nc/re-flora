use crate::geom::Aabb3;
use glam::{Mat4, Vec3};

/// Returns (view_matrix, projection_matrix)
pub fn calculate_directional_light_matrices(
    world_bound: Aabb3,
    light_direction: Vec3,
) -> (Mat4, Mat4) {
    const TOLERANCE: f32 = 0.5;
    const PARALLEL_EPS: f32 = 0.999; // cos(ϑ) threshold for “almost parallel”

    // 1. Collect the world-space frustum corners and their centre
    let frustum_corners = world_bound.get_corners();
    let mut frustum_center = Vec3::ZERO;
    for c in &frustum_corners {
        frustum_center += *c;
    }
    frustum_center *= 1.0 / 8.0;

    // 2. Pick a safe up vector
    let dir_n = light_direction.normalize();
    let mut up = Vec3::Y;
    if dir_n.dot(up).abs() > PARALLEL_EPS {
        // If parallel to Y, fall back to Z.  (Z can also be X, any axis works.)
        up = Vec3::Z;
    }

    // 3. Build the view matrix
    //    eye = centre + light_dir (any distance works for directional lights)
    let view_matrix = Mat4::look_at_rh(frustum_center + light_direction, frustum_center, up);

    // 4. Transform the corners to light space and compute the ortho bounds
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    for c in &frustum_corners {
        let p = (view_matrix * c.extend(1.0)).truncate();
        min = min.min(p);
        max = max.max(p);
    }

    // 5. Build the orthographic projection and flip Y so it matches GL conventions
    let proj = Mat4::orthographic_rh(
        min.x - TOLERANCE,
        max.x + TOLERANCE,
        min.y - TOLERANCE,
        max.y + TOLERANCE,
        min.z - TOLERANCE,
        max.z + TOLERANCE,
    );

    let projection_matrix = Mat4::from_scale(Vec3::new(1.0, -1.0, 1.0)) * proj;
    (view_matrix, projection_matrix)
}
