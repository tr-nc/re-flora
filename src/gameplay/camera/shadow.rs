use crate::gameplay::Camera;
use glam::{Mat4, Vec3};

pub fn calculate_directional_light_matrices(
    player_camera: &Camera,
    light_direction: Vec3,
) -> (Mat4, Mat4) {
    const LIGHT_UP: Vec3 = Vec3::Y;

    // let frustum_corners = player_camera.get_frustum_corners();

    // let mut frustum_center = Vec3::ZERO;
    // for corner in &frustum_corners {
    //     frustum_center += *corner;
    // }
    // frustum_center /= 8.0;

    // let view_matrix = Mat4::look_at_rh(frustum_center + light_direction, frustum_center, LIGHT_UP);

    // log::debug!(
    //     "looking from: {:?}, looking at: {:?}",
    //     frustum_center + light_direction,
    //     frustum_center
    // );

    // let mut min = Vec3::splat(f32::MAX);
    // let mut max = Vec3::splat(f32::MIN);
    // for corner in &frustum_corners {
    //     let trf = view_matrix * corner.extend(1.0);
    //     min = min.min(trf.truncate());
    //     max = max.max(trf.truncate());
    // }

    // // https://learnopengl.com/Guest-Articles/2021/CSM
    // // in right handed coordinate system, the camera is looking at the negative z axis
    // // so for the near plane, we need to pull it closer to the camera
    // // and for the far plane, we need to push it further away from the camera
    // let z_mult = 10.0;
    // if min.z < 0.0 {
    //     min.z *= z_mult;
    // } else {
    //     min.z /= z_mult;
    // }
    // if max.z < 0.0 {
    //     max.z /= z_mult;
    // } else {
    //     max.z *= z_mult;
    // }

    let view_matrix = Mat4::look_at_rh(
        light_direction * 10.0 + player_camera.position(),
        player_camera.position(),
        LIGHT_UP,
    );

    const Z_NEAR: f32 = 1.0;
    const Z_FAR: f32 = 1000.0;

    const RANGE: f32 = 5.0;
    let proj = Mat4::orthographic_rh(-RANGE, RANGE, -RANGE, RANGE, Z_NEAR, Z_FAR);

    let flip_y = Mat4::from_scale(Vec3::new(1.0, -1.0, 1.0));
    let projection_matrix = flip_y * proj;

    (view_matrix, projection_matrix)
}
