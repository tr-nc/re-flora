use super::{
    audio::PlayerAudioController, head_bob::HeadBob, movement::MovementState, stride::StrideCycle,
    vectors::CameraVectors, CameraDesc,
};
use crate::audio::SpatialSoundManager;
use anyhow::Result;
use glam::{Mat4, Vec2, Vec3, Vec4};
use re_flora_vkn::Extent2D;
use winit::event::KeyEvent;

const GROUNDED_CAMERA_HEIGHT_SMOOTHING_SPEED: f32 = 14.0;
const MAX_SMOOTHED_GROUND_VERTICAL_TRANSLATION: f32 = 16.5 / 256.0;
const MAX_GROUNDED_CAMERA_VERTICAL_LAG: f32 = 8.0 / 256.0;

fn smoothed_grounded_camera_y(
    current_y: f32,
    target_y: f32,
    vertical_translation: f32,
    frame_delta_time: f32,
) -> f32 {
    if !current_y.is_finite()
        || !target_y.is_finite()
        || !vertical_translation.is_finite()
        || !frame_delta_time.is_finite()
        || frame_delta_time <= f32::EPSILON
        || vertical_translation.abs() > MAX_SMOOTHED_GROUND_VERTICAL_TRANSLATION
    {
        return target_y;
    }

    let alpha = 1.0 - (-GROUNDED_CAMERA_HEIGHT_SMOOTHING_SPEED * frame_delta_time).exp();
    let smoothed = current_y + (target_y - current_y) * alpha;
    target_y
        + (smoothed - target_y).clamp(
            -MAX_GROUNDED_CAMERA_VERTICAL_LAG,
            MAX_GROUNDED_CAMERA_VERTICAL_LAG,
        )
}

fn walk_velocity_after_collision(control_velocity: Vec3, vertical_velocity: f32) -> Vec3 {
    Vec3::new(control_velocity.x, vertical_velocity, control_velocity.z)
}

#[derive(Debug, Clone)]
pub struct PlayerRigidBody {
    pub velocity: Vec3,
    pub is_grounded: bool,
    pub drag: f32,
}

impl PlayerRigidBody {
    pub fn new() -> Self {
        Self {
            velocity: Vec3::ZERO,
            is_grounded: false,
            drag: 0.98,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraPose {
    pub position: Vec3,
    pub yaw_deg: f32,
    pub pitch_deg: f32,
    pub fov_deg: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerWalkMovementRequest {
    pub camera_position: Vec3,
    pub camera_height: f32,
    pub desired_translation: Vec3,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerWalkMovementResult {
    pub translation: Vec3,
    pub grounded: bool,
}

impl PlayerWalkMovementResult {
    pub const BLOCKED: Self = Self {
        translation: Vec3::ZERO,
        grounded: false,
    };
}

pub struct Camera {
    position: Vec3,

    /// Authoritative eye-height anchor derived from the collision capsule. The rendered camera may
    /// lag behind this position vertically while traversing voxel-sized ground steps.
    walk_collision_position: Option<Vec3>,

    /// The yaw of the camera in radians.
    yaw: f32,

    /// The pitch of the camera in radians.
    pitch: f32,

    vectors: CameraVectors,
    movement_state: MovementState,
    desc: CameraDesc,

    /// Vertical velocity used by walk/gravity mode (m/s, +y up).
    vertical_velocity: f32,

    player_audio_controller: PlayerAudioController,
    was_on_ground: bool,

    /// Rigidbody physics state for collision response.
    rigidbody: PlayerRigidBody,

    /// Speed just before landing (for landing sound volume).
    pre_landing_speed: f32,

    head_bob: HeadBob,
    stride_cycle: StrideCycle,
}

impl Camera {
    pub fn new(
        initial_position: Vec3,
        initial_yaw: f32,
        initial_pitch: f32,
        desc: CameraDesc,
        spatial_sound_manager: SpatialSoundManager,
    ) -> Result<Self> {
        let mut camera = Self {
            position: initial_position,
            walk_collision_position: None,
            vectors: CameraVectors::new(),
            yaw: initial_yaw.to_radians(),
            pitch: initial_pitch.to_radians(),
            movement_state: MovementState::new(
                desc.movement.normal_speed,
                desc.movement.boosted_speed_mul,
            ),
            desc,
            vertical_velocity: 0.0,
            player_audio_controller: PlayerAudioController::new(spatial_sound_manager)?,
            was_on_ground: false,
            rigidbody: PlayerRigidBody::new(),
            pre_landing_speed: 0.0,
            head_bob: HeadBob::new(),
            stride_cycle: StrideCycle::new(),
        };

        camera.vectors.update(camera.yaw, camera.pitch);
        camera
            .player_audio_controller
            .set_footstep_volume_gain(-40.0);
        Ok(camera)
    }

    pub fn set_footstep_volume_gain(&mut self, volume_gain: f32) {
        self.player_audio_controller
            .set_footstep_volume_gain(volume_gain);
    }

    pub fn on_resize(&mut self, screen_extent: Extent2D) {
        self.desc.aspect_ratio = screen_extent.width as f32 / screen_extent.height as f32;
    }

    #[allow(dead_code)]
    pub fn position(&self) -> Vec3 {
        self.position
    }

    pub fn front(&self) -> Vec3 {
        self.vectors.front
    }

    pub fn ray_from_screen_position(
        &self,
        screen_pos_physical: Vec2,
        screen_extent: Extent2D,
    ) -> Option<(Vec3, Vec3)> {
        let width = screen_extent.width as f32;
        let height = screen_extent.height as f32;
        if width <= 0.0 || height <= 0.0 {
            return None;
        }

        let ndc_x = (screen_pos_physical.x / width) * 2.0 - 1.0;
        let ndc_y = 1.0 - (screen_pos_physical.y / height) * 2.0;
        let half_fov_y = (self.desc.projection.v_fov.to_radians() * 0.5).tan();
        let aspect = width / height;
        let direction = (self.vectors.front
            + self.vectors.right * (ndc_x * half_fov_y * aspect)
            + self.vectors.up * (ndc_y * half_fov_y))
            .normalize_or_zero();
        (direction.length_squared() > f32::EPSILON).then_some((self.position, direction))
    }

    pub fn set_pose_looking_at(&mut self, position: Vec3, target: Vec3) -> bool {
        let direction = (target - position).normalize_or_zero();
        if direction.length_squared() <= f32::EPSILON {
            return false;
        }

        self.position = position;
        self.walk_collision_position = None;
        self.yaw = direction.x.atan2(-direction.z);
        self.pitch = direction
            .y
            .atan2(Vec2::new(direction.x, direction.z).length());
        self.limit_yaw();
        self.clamp_pitch();
        self.vectors.update(self.yaw, self.pitch);
        true
    }

    pub fn vectors(&self) -> &CameraVectors {
        &self.vectors
    }

    /// Returns the camera's position as a Vec4 with the w component set to 1.0.
    #[allow(dead_code)]
    pub fn position_vec4(&self) -> Vec4 {
        Vec4::new(self.position.x, self.position.y, self.position.z, 1.0)
    }

    pub fn get_view_mat(&self) -> Mat4 {
        let base_view = Mat4::look_at_rh(
            self.position,
            self.position + self.vectors.front,
            self.vectors.up,
        );
        self.head_bob
            .apply_to_view_mat(base_view, self.vectors.right, self.vectors.up)
    }

    pub fn calculate_proj_mat(v_fov: f32, aspect_ratio: f32, z_near: f32, z_far: f32) -> Mat4 {
        let proj = Mat4::perspective_rh(v_fov.to_radians(), aspect_ratio, z_near, z_far);
        let flip_y = Mat4::from_scale(Vec3::new(1.0, -1.0, 1.0));
        flip_y * proj
    }

    pub fn get_proj_mat(&self) -> Mat4 {
        Self::calculate_proj_mat(
            self.desc.projection.v_fov,
            self.desc.aspect_ratio,
            self.desc.projection.z_near,
            self.desc.projection.z_far,
        )
    }

    pub fn pose(&self) -> CameraPose {
        CameraPose {
            position: self.position,
            yaw_deg: self.yaw.to_degrees(),
            pitch_deg: self.pitch.to_degrees(),
            fov_deg: self.desc.projection.v_fov,
        }
    }

    pub fn apply_pose(&mut self, pose: CameraPose) {
        self.position = pose.position;
        self.yaw = pose.yaw_deg.to_radians();
        self.pitch = pose.pitch_deg.to_radians();
        self.desc.projection.v_fov = pose.fov_deg.clamp(1.0, 170.0);

        self.limit_yaw();
        self.clamp_pitch();
        self.vectors.update(self.yaw, self.pitch);
        self.reset_velocity();
        self.movement_state.reset_input();
    }

    /// Only controls the camera's movement state based on the key event.
    pub fn handle_keyboard(&mut self, key_event: &KeyEvent) {
        self.movement_state.handle_keyboard(key_event);
    }

    pub fn reset_input(&mut self) {
        self.movement_state.reset_input();
    }

    /// Limits the yaw to prevent the camera from spinning indefinitely.
    /// The yaw is clamped to the range (-π, π).
    fn limit_yaw(&mut self) {
        if self.yaw > std::f32::consts::PI {
            self.yaw -= std::f32::consts::TAU;
        } else if self.yaw < -std::f32::consts::PI {
            self.yaw += std::f32::consts::TAU;
        }
    }

    /// Clamps the pitch to prevent the camera from flipping.
    fn clamp_pitch(&mut self) {
        const CAMERA_LIM_RAD: f32 = std::f32::consts::FRAC_PI_2 - 0.01;
        self.pitch = self.pitch.clamp(-CAMERA_LIM_RAD, CAMERA_LIM_RAD);
    }

    pub fn handle_mouse(&mut self, delta: Vec2) {
        const SENSITIVITY_MULTIPLIER: f32 = 0.001;
        // the delta is positive when moving the mouse to the right / down
        // so we need to invert the pitch delta so that when mouse is going up, pitch increases
        self.yaw += delta.x * self.desc.movement.mouse_sensitivity * SENSITIVITY_MULTIPLIER;
        self.pitch -= delta.y * self.desc.movement.mouse_sensitivity * SENSITIVITY_MULTIPLIER;

        self.limit_yaw();
        self.clamp_pitch();

        self.vectors.update(self.yaw, self.pitch);
    }

    fn movement_basis(&self) -> (Vec3, Vec3) {
        // Discard vertical component so movement happens in world-space XZ plane.
        let mut horizontal_front = Vec3::new(self.vectors.front.x, 0.0, self.vectors.front.z);
        // If the camera looks straight up/down, fallback to yaw to keep movement responsive.
        if horizontal_front.length_squared() < f32::EPSILON {
            horizontal_front = Vec3::new(self.yaw.sin(), 0.0, -self.yaw.cos());
        }
        horizontal_front = horizontal_front.normalize();

        let horizontal_right = horizontal_front.cross(Vec3::Y).normalize();
        (horizontal_front, horizontal_right)
    }

    pub fn update_transform_fly_mode(&mut self, frame_delta_time: f32) {
        self.walk_collision_position = None;
        // Move in the camera's local axes (front/right/up).
        self.position += self.movement_state.get_velocity(
            self.vectors.front,
            self.vectors.right,
            self.vectors.up,
        ) * frame_delta_time;
        self.head_bob.reset();
        self.stride_cycle.reset();
    }

    pub fn prepare_walk_movement(&mut self, frame_delta_time: f32) -> PlayerWalkMovementRequest {
        const GRAVITY_G: f32 = 2.0;
        const JUMP_IMPULSE: f32 = 0.5;
        const GROUND_STICK_VELOCITY: f32 = 0.05;
        const GROUND_ACCELERATION: f32 = 20.0;
        const AIR_ACCELERATION: f32 = 10.0;

        if frame_delta_time <= f32::EPSILON || !frame_delta_time.is_finite() {
            return PlayerWalkMovementRequest {
                camera_position: self.walk_collision_position.unwrap_or(self.position),
                camera_height: self.desc.camera_height,
                desired_translation: Vec3::ZERO,
            };
        }

        let collision_position = *self.walk_collision_position.get_or_insert(self.position);

        let (front, right) = self.movement_basis();
        let input_velocity = self.movement_state.get_velocity(front, right, Vec3::ZERO);
        let acceleration_force = if self.rigidbody.is_grounded {
            GROUND_ACCELERATION
        } else {
            AIR_ACCELERATION
        };
        let desired_horizontal_velocity = Vec3::new(input_velocity.x, 0.0, input_velocity.z);
        let current_horizontal_velocity =
            Vec3::new(self.rigidbody.velocity.x, 0.0, self.rigidbody.velocity.z);
        let acceleration =
            (desired_horizontal_velocity - current_horizontal_velocity) * acceleration_force;
        self.rigidbody.velocity.x += acceleration.x * frame_delta_time;
        self.rigidbody.velocity.z += acceleration.z * frame_delta_time;

        if self.rigidbody.is_grounded && self.movement_state.jump_requested {
            self.vertical_velocity = JUMP_IMPULSE;
            self.rigidbody.is_grounded = false;
            let current_speed = self.rigidbody.velocity.length();
            let foot_position = Vec3::new(
                collision_position.x,
                collision_position.y - self.desc.camera_height,
                collision_position.z,
            );
            self.player_audio_controller
                .play_jumping(current_speed, foot_position);
        } else if self.rigidbody.is_grounded {
            // Keep a slight downward component so the KCC can preserve stable ground contact.
            self.vertical_velocity = -GROUND_STICK_VELOCITY;
        } else {
            self.vertical_velocity -= GRAVITY_G * frame_delta_time;
        }
        self.rigidbody.velocity.y = self.vertical_velocity;

        self.rigidbody.velocity.x *= self.rigidbody.drag;
        self.rigidbody.velocity.z *= self.rigidbody.drag;
        if !self.rigidbody.is_grounded {
            self.pre_landing_speed = self.rigidbody.velocity.length();
        }

        PlayerWalkMovementRequest {
            camera_position: collision_position,
            camera_height: self.desc.camera_height,
            desired_translation: self.rigidbody.velocity * frame_delta_time,
        }
    }

    pub fn apply_walk_movement(
        &mut self,
        frame_delta_time: f32,
        request: PlayerWalkMovementRequest,
        result: PlayerWalkMovementResult,
    ) {
        if frame_delta_time <= f32::EPSILON || !frame_delta_time.is_finite() {
            return;
        }

        let grounded = result.grounded && self.vertical_velocity <= 0.0;
        let upward_blocked = request.desired_translation.y > 0.0
            && result.translation.y + f32::EPSILON < request.desired_translation.y;
        if grounded || upward_blocked {
            self.vertical_velocity = 0.0;
        }
        self.rigidbody.velocity =
            walk_velocity_after_collision(self.rigidbody.velocity, self.vertical_velocity);
        self.rigidbody.is_grounded = grounded;

        let collision_position = self
            .walk_collision_position
            .get_or_insert(request.camera_position);
        *collision_position += result.translation;
        let collision_position = *collision_position;
        self.position.x = collision_position.x;
        self.position.z = collision_position.z;
        self.position.y = if grounded {
            smoothed_grounded_camera_y(
                self.position.y,
                collision_position.y,
                result.translation.y,
                frame_delta_time,
            )
        } else {
            collision_position.y
        };

        let is_moving = self.movement_state.is_moving_horizontally();
        let is_running = self.movement_state.is_boosted;
        let foot_position = Vec3::new(
            collision_position.x,
            collision_position.y - self.desc.camera_height,
            collision_position.z,
        );
        let just_landed = grounded && !self.was_on_ground;
        if just_landed {
            if is_moving {
                self.player_audio_controller.play_step(
                    is_running,
                    self.rigidbody.velocity.length(),
                    foot_position,
                );
                self.stride_cycle.restart_after_step();
            } else {
                self.player_audio_controller
                    .play_landing(self.pre_landing_speed, foot_position);
            }
        }

        let stride = self.stride_cycle.update(
            frame_delta_time,
            grounded && is_moving,
            is_running,
            self.player_audio_controller.walk_interval(),
            self.player_audio_controller.run_interval(),
        );
        if stride.just_step {
            self.player_audio_controller.play_step(
                is_running,
                self.rigidbody.velocity.length(),
                foot_position,
            );
        }
        self.head_bob.update(
            stride.phase,
            grounded && is_moving,
            is_running,
            &self.desc.head_bob,
            frame_delta_time,
        );
        self.was_on_ground = grounded;
    }

    #[allow(dead_code)]
    pub fn set_head_bob_params(
        &mut self,
        vertical_amp: f32,
        horizontal_amp: f32,
        roll_amp_deg: f32,
        sprint_amp_mul: f32,
    ) {
        self.desc.head_bob.vertical_amplitude = vertical_amp;
        self.desc.head_bob.horizontal_amplitude = horizontal_amp;
        self.desc.head_bob.roll_amplitude_deg = roll_amp_deg;
        self.desc.head_bob.sprint_amplitude_mul = sprint_amp_mul;
    }

    /// Resets the rigidbody velocity and vertical velocity when switching modes.
    pub fn reset_velocity(&mut self) {
        self.rigidbody.velocity = Vec3::ZERO;
        self.walk_collision_position = None;
        self.vertical_velocity = 0.0;
        self.was_on_ground = false;
        self.pre_landing_speed = 0.0;
        self.head_bob.reset();
        self.stride_cycle.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grounded_camera_smooths_a_sixteen_voxel_step_without_overshooting() {
        let current_y = 1.0;
        let target_y = current_y + 16.0 / 256.0;
        let smoothed = smoothed_grounded_camera_y(current_y, target_y, 16.0 / 256.0, 1.0 / 60.0);

        assert!(smoothed > current_y);
        assert!(smoothed < target_y);
    }

    #[test]
    fn grounded_camera_smoothing_is_frame_rate_independent() {
        fn simulate(frame_delta_time: f32, frames: usize) -> f32 {
            let mut current_y = 0.0;
            for _ in 0..frames {
                current_y = smoothed_grounded_camera_y(current_y, 0.01, 0.0, frame_delta_time);
            }
            current_y
        }

        let at_30_hz = simulate(1.0 / 30.0, 30);
        let at_120_hz = simulate(1.0 / 120.0, 120);
        assert!((at_30_hz - at_120_hz).abs() < 1.0e-6);
    }

    #[test]
    fn grounded_camera_does_not_smooth_large_vertical_discontinuities() {
        assert_eq!(smoothed_grounded_camera_y(0.0, 1.0, 0.25, 1.0 / 60.0), 1.0);
    }

    #[test]
    fn grounded_camera_limits_accumulated_vertical_lag() {
        let target_y = 1.0;
        let smoothed = smoothed_grounded_camera_y(0.0, target_y, 1.0 / 256.0, 1.0 / 60.0);

        assert!((target_y - smoothed).abs() <= MAX_GROUNDED_CAMERA_VERTICAL_LAG);
    }

    #[test]
    fn collision_response_preserves_horizontal_control_velocity() {
        let control_velocity = Vec3::new(0.2, -1.0, -0.1);
        let resolved = walk_velocity_after_collision(control_velocity, 0.0);

        assert_eq!(resolved, Vec3::new(0.2, 0.0, -0.1));
    }
}
