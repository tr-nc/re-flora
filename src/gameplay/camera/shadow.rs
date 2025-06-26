use crate::gameplay::Camera;
use glam::{Mat4, Vec3};

pub fn calculate_directional_light_matrices(
    player_camera: &Camera,
    light_direction: Vec3,
) -> (Mat4, Mat4) {
    const LIGHT_UP: Vec3 = Vec3::Y;

    let frustum_corners = player_camera.get_frustum_corners();

    let mut frustum_center = Vec3::ZERO;
    for corner in &frustum_corners {
        frustum_center += *corner;
    }
    frustum_center /= 8.0;

    let light_eye_pos = frustum_center - light_direction;
    let view_matrix = Mat4::look_at_rh(light_eye_pos, frustum_center, LIGHT_UP);

    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    for corner in &frustum_corners {
        let trf = view_matrix * corner.extend(1.0);
        min = min.min(trf.truncate());
        max = max.max(trf.truncate());
    }

    // https://learnopengl.com/Guest-Articles/2021/CSM
    // in right handed coordinate system, the camera is looking at the negative z axis
    // so for the near plane, we need to pull it closer to the camera
    // and for the far plane, we need to push it further away from the camera
    let z_mult = 10.0;
    if min.z < 0.0 {
        min.z *= z_mult;
    } else {
        min.z /= z_mult;
    }
    if max.z < 0.0 {
        max.z /= z_mult;
    } else {
        max.z *= z_mult;
    }

    let projection_matrix = Mat4::orthographic_rh(min.x, max.x, min.y, max.y, min.z, max.z);

    (view_matrix, projection_matrix)
}
