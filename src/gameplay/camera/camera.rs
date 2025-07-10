use super::{
    audio::PlayerAudioController,
    movement::MovementState,
    vectors::CameraVectors,
    CameraDesc,
};
use crate::{
    audio::AudioEngine,
    vkn::Extent2D,
};
use anyhow::Result;
use glam::{Mat4, Vec2, Vec3, Vec4};
use winit::event::KeyEvent;

pub struct Camera {
    position: Vec3,

    /// The initial yaw of the camera in radians.
    yaw: f32,

    /// The initial pitch of the camera in radians.
    pitch: f32,

    vectors: CameraVectors,
    movement_state: MovementState,
    desc: CameraDesc,

    /// vertical velocity used by walk/gravity mode (m/s, +y up)
    vertical_velocity: f32,

    player_audio_controller: PlayerAudioController,
    was_on_ground: bool,
}

impl Camera {
    pub fn new(
        initial_position: Vec3,
        initial_yaw: f32,
        initial_pitch: f32,
        desc: CameraDesc,
        audio_engine: AudioEngine,
    ) -> Result<Self> {
        let mut camera = Self {
            position: initial_position,
            vectors: CameraVectors::new(),
            yaw: initial_yaw.to_radians(),
            pitch: initial_pitch.to_radians(),
            movement_state: MovementState::new(
                desc.movement.normal_speed,
                desc.movement.boosted_speed_mul,
            ),
            desc,
            vertical_velocity: 0.0,
            player_audio_controller: PlayerAudioController::new(audio_engine)?,
            was_on_ground: false,
        };

        camera.vectors.update(camera.yaw, camera.pitch);
        Ok(camera)
    }

    pub fn on_resize(&mut self, screen_extent: Extent2D) {
        self.desc.aspect_ratio = screen_extent.width as f32 / screen_extent.height as f32;
    }

    #[allow(dead_code)]
    pub fn position(&self) -> Vec3 {
        self.position
    }

    /// Returns the camera's position as a Vec4 with the w component set to 1.0.
    #[allow(dead_code)]
    pub fn position_vec4(&self) -> Vec4 {
        Vec4::new(self.position.x, self.position.y, self.position.z, 1.0)
    }

    pub fn get_view_mat(&self) -> Mat4 {
        Mat4::look_at_rh(
            self.position,
            self.position + self.vectors.front,
            self.vectors.up,
        )
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

    /// Only controls the camera's movement state based on the key event.
    pub fn handle_keyboard(&mut self, key_event: &KeyEvent) {
        self.movement_state.handle_keyboard(key_event);
    }

    /// Limits the yaw to prevent the camera from spinning indefinitely.
    /// The yaw is clamped to the range (-π, π).
    fn limit_yaw(&mut self) {
        if self.yaw > std::f32::consts::PI {
            self.yaw -= 2.0 * std::f32::consts::PI;
        }
        if self.yaw < -std::f32::consts::PI {
            self.yaw += 2.0 * std::f32::consts::PI;
        }
    }

    /// Clamps the pitch to prevent the camera from flipping.
    fn clamp_pitch(&mut self) {
        const CAMERA_LIM_RAD: f32 = std::f32::consts::FRAC_PI_2 - 0.01;
        if self.pitch > CAMERA_LIM_RAD {
            self.pitch = CAMERA_LIM_RAD;
        }
        if self.pitch < -CAMERA_LIM_RAD {
            self.pitch = -CAMERA_LIM_RAD;
        }
    }

    pub fn handle_mouse(&mut self, delta: Vec2) {
        const SENSITIVITY_MULTIPLIER: f32 = 0.001;
        // the delta is positive when moving the mouse to the right / down
        // so we need to invert the pitch delta so that when mouse is going up, pitch increases
        self.yaw += delta.x as f32 * self.desc.movement.mouse_sensitivity * SENSITIVITY_MULTIPLIER;
        self.pitch -=
            delta.y as f32 * self.desc.movement.mouse_sensitivity * SENSITIVITY_MULTIPLIER;

        self.limit_yaw();
        self.clamp_pitch();

        self.vectors.update(self.yaw, self.pitch);
    }

    fn movement_basis(&self) -> (Vec3, Vec3) {
        // discard vertical component so movement happens in world-space XZ plane
        let mut horizontal_front = Vec3::new(self.vectors.front.x, 0.0, self.vectors.front.z);
        // if the camera looks straight up/down, fallback to yaw to keep movement responsive
        if horizontal_front.length_squared() < f32::EPSILON {
            horizontal_front = Vec3::new(self.yaw.sin(), 0.0, -self.yaw.cos());
        }
        horizontal_front = horizontal_front.normalize();

        let horizontal_right = horizontal_front.cross(Vec3::Y).normalize();
        (horizontal_front, horizontal_right)
    }

    #[allow(dead_code)]
    pub fn update_transform_fly_mode(&mut self, frame_delta_time: f32) {
        // move in the camera's local axes (front/right/up)
        self.position += self.movement_state.get_velocity(
            self.vectors.front,
            self.vectors.right,
            self.vectors.up,
        ) * frame_delta_time;
    }

    pub fn update_transform_walk_mode(&mut self, frame_delta_time: f32, ground_distance: f32) {
        const GRAVITY_G: f32 = 2.0; // gravity acceleration (m/s²)
        const JUMP_IMPULSE: f32 = 0.4; // initial jump velocity (m/s)
        const GROUND_EPSILON: f32 = 0.01; // tolerance when comparing to ground
        const Y_SMOOTHING_ALPHA: f32 = 0.2; // fraction used to lerp camera height to ground

        // compute horizontal movement basis (XZ plane)
        let (front, right) = self.movement_basis();
        let horizontal_velocity = self.movement_state.get_velocity(front, right, Vec3::ZERO);

        // detect whether the player is on the ground
        let is_on_ground = ground_distance <= self.desc.camera_height + GROUND_EPSILON;

        // === vertical motion & jump handling ===
        if is_on_ground {
            // clamp any remaining downward velocity when touching ground
            if self.vertical_velocity < 0.0 {
                self.vertical_velocity = 0.0;
            }

            if self.movement_state.jump_requested {
                // launch the jump
                self.vertical_velocity = JUMP_IMPULSE;
                // play jump sound once, immediately when leaving the ground
                self.player_audio_controller.play_jump();
            } else {
                // stick to ground smoothly
                let ground_level_y = self.position.y - ground_distance;
                let target_camera_y = ground_level_y + self.desc.camera_height;
                self.position.y += (target_camera_y - self.position.y) * Y_SMOOTHING_ALPHA;
            }
        } else {
            // airborne: apply gravity
            self.vertical_velocity -= GRAVITY_G * frame_delta_time;
        }

        // reset the jump flag after use
        self.movement_state.reset_jump_request();

        // === integrate total velocity ===
        let total_velocity = horizontal_velocity + Vec3::new(0.0, self.vertical_velocity, 0.0);
        self.position += total_velocity * frame_delta_time;

        // === audio: foot-steps, jump, land ===
        let is_moving = self.movement_state.is_moving_horizontally();

        let is_running = self.movement_state.is_boosted;

        let just_landed = is_on_ground && !self.was_on_ground;
        if just_landed {
            if is_moving {
                // moving when touching ground: treat as an immediate step
                self.player_audio_controller.play_step(is_running);
                // reset timer so下一次步伐重新计时
                self.player_audio_controller.reset_walk_timer();
            } else {
                // still: play landing sound only
                self.player_audio_controller.play_land();
                // 不重置计时器，让 update_walk_sound 在静止状态保持间隔满值
            }
        }

        // per-frame update for regular walk/run sounds
        self.player_audio_controller.update_walk_sound(
            is_on_ground,
            is_moving,
            is_running,
            frame_delta_time,
        );

        self.was_on_ground = is_on_ground;
    }
}
