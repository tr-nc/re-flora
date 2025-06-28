use crate::gameplay::Camera;
use glam::{Mat4, Vec3};

/// Returns: (view_matrix, projection_matrix)
pub fn calculate_directional_light_matrices(
    player_camera: &Camera,
    light_direction: Vec3,
) -> (Mat4, Mat4) {
    const LIGHT_UP: Vec3 = Vec3::Y;
    const TOLERENCE_XY: f32 = 0.1;
    const TOLERENCE_Z: f32 = 10.0;

    let frustum_corners = player_camera.get_frustum_corners();

    let mut frustum_center = Vec3::ZERO;
    for corner in &frustum_corners {
        frustum_center += *corner;
    }
    frustum_center /= 8.0;

    let view_matrix = Mat4::look_at_rh(frustum_center + light_direction, frustum_center, LIGHT_UP);

    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    for corner in &frustum_corners {
        let trf = view_matrix * corner.extend(1.0);
        min = min.min(trf.truncate());
        max = max.max(trf.truncate());
    }

    // https://learnopengl.com/Guest-Articles/2021/CSM

    let proj = Mat4::orthographic_rh(
        min.x - TOLERENCE_XY,
        max.x + TOLERENCE_XY,
        min.y - TOLERENCE_XY,
        max.y + TOLERENCE_XY,
        min.z - TOLERENCE_Z,
        max.z + TOLERENCE_Z,
    );

    let flip_y = Mat4::from_scale(Vec3::new(1.0, -1.0, 1.0));
    let projection_matrix = flip_y * proj;

    (view_matrix, projection_matrix)
}
