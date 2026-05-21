use crate::geom::Aabb3;
use glam::{Mat4, Vec3};

/// Returns (view_matrix, projection_matrix)
pub fn calculate_directional_light_matrices(
    world_bound: Aabb3,
    light_direction: Vec3,
    shadow_map_resolution: u32,
) -> (Mat4, Mat4) {
    const TOLERANCE: f32 = 0.1;
    // cos(ϑ) threshold for “almost parallel”
    const PARALLEL_EPS: f32 = 0.999;
    const MIN_LIGHT_DIR_LEN_SQ: f32 = 1e-8;

    let frustum_corners = world_bound.get_corners();
    let frustum_center = world_bound.center();

    let mut radius = 0.0f32;
    for c in &frustum_corners {
        radius = radius.max(c.distance(frustum_center));
    }
    radius += TOLERANCE;

    let resolution = shadow_map_resolution.max(1) as f32;
    let texel_size = (radius * 2.0) / resolution;
    // Snapping can move the projection by up to one texel when using floor()
    // below, so reserve that margin in the fixed orthographic bounds.
    radius += texel_size;

    let dir_n = if light_direction.length_squared() > MIN_LIGHT_DIR_LEN_SQ {
        light_direction.normalize()
    } else {
        Vec3::Y
    };
    let mut up = Vec3::Y;
    if dir_n.dot(up).abs() > PARALLEL_EPS {
        // if parallel to y, fall back to z.  (z can also be x, any axis works.)
        up = Vec3::Z;
    }

    let diameter = radius * 2.0;
    let world_units_per_texel = diameter / resolution;

    // Build a light-space basis without anchoring it to the current center.
    // This lets us snap the center in light space instead of continuously
    // sliding the shadow texel grid across the world.
    let light_space_basis = Mat4::look_at_rh(dir_n, Vec3::ZERO, up);
    let center_ls = (light_space_basis * frustum_center.extend(1.0)).truncate();
    let snapped_center_ls = Vec3::new(
        (center_ls.x / world_units_per_texel).floor() * world_units_per_texel,
        (center_ls.y / world_units_per_texel).floor() * world_units_per_texel,
        center_ls.z,
    );
    let snapped_center = (light_space_basis.inverse() * snapped_center_ls.extend(1.0)).truncate();

    // The ortho projection is sphere-fit instead of tight AABB-fit. Its size
    // stays stable as the light rotates, avoiding projection “breathing”.
    let view_matrix = Mat4::look_at_rh(snapped_center + dir_n * radius, snapped_center, up);
    let proj = Mat4::orthographic_rh(-radius, radius, -radius, radius, 0.0, diameter);

    let proj_matrix = Mat4::from_scale(Vec3::new(1.0, -1.0, 1.0)) * proj;

    (view_matrix, proj_matrix)
}
