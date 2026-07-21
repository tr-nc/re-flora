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

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec2;

    fn projected(view_projection: Mat4, point: Vec3) -> Vec3 {
        let clip = view_projection * point.extend(1.0);
        clip.truncate() / clip.w
    }

    #[test]
    fn transformed_normal_gradient_matches_projected_world_plane() {
        let world_bound = Aabb3::new(Vec3::ZERO, Vec3::splat(2.0));
        let light_direction = Vec3::new(-0.56, 0.70, 0.44).normalize();
        let (view, projection) =
            calculate_directional_light_matrices(world_bound, light_direction, 2048);
        let view_projection = projection * view;

        let anchor = Vec3::new(1.0, 0.4, 1.0);
        let normal = Vec3::new(0.23, 0.94, -0.25).normalize();
        let tangent = normal.cross(Vec3::X).normalize();
        let point_on_plane = anchor + tangent * 0.37;
        let anchor_ndc = projected(view_projection, anchor);
        let point_ndc = projected(view_projection, point_on_plane);

        let clip_normal = view_projection.inverse().transpose() * normal.extend(0.0);
        assert!(clip_normal.z.abs() > 1.0e-6);
        let depth_gradient_uv = -2.0 * Vec2::new(clip_normal.x, clip_normal.y) / clip_normal.z;
        let anchor_uv = anchor_ndc.truncate() * 0.5 + Vec2::splat(0.5);
        let point_uv = point_ndc.truncate() * 0.5 + Vec2::splat(0.5);
        let predicted_depth = anchor_ndc.z + depth_gradient_uv.dot(point_uv - anchor_uv);

        assert!((predicted_depth - point_ndc.z).abs() < 1.0e-5);
        assert!(normal.dot(point_on_plane - anchor).abs() < 1.0e-5);
    }
}
