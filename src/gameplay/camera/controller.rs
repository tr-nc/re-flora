use super::{
    audio::PlayerAudioController, movement::MovementState, vectors::CameraVectors, CameraDesc,
};
use crate::{audio::SpatialSoundManager, tracer::PlayerCollisionResult, vkn::Extent2D};
use anyhow::Result;
use glam::{Mat4, Vec2, Vec3, Vec4};
use winit::event::KeyEvent;

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

    /// Rigidbody physics state for collision response
    rigidbody: PlayerRigidBody,

    /// Speed just before landing (for landing sound volume)
    pre_landing_speed: f32,
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
        };

        camera.vectors.update(camera.yaw, camera.pitch);
        camera
            .player_audio_controller
            .set_footstep_volume_gain(-12.0);
        Ok(camera)
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

    pub fn vectors(&self) -> &CameraVectors {
        &self.vectors
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

    pub fn update_transform_walk_mode(
        &mut self,
        frame_delta_time: f32,
        collision_result: PlayerCollisionResult,
    ) {
        const GRAVITY_G: f32 = 2.0; // gravity acceleration (m/s²)
        const JUMP_IMPULSE: f32 = 0.5; // initial jump velocity (m/s)
        const GROUND_EPSILON: f32 = 0.01; // tolerance when comparing to ground
        const Y_SMOOTHING_ALPHA: f32 = 0.2; // fraction used to lerp camera height to ground
        const COLLISION_THRESHOLD: f32 = 0.03; // minimum distance to obstacle before stopping
        const MAX_COLLISION_ITERATIONS: usize = 3; // maximum collision resolution iterations

        // compute horizontal movement basis (XZ plane)
        let (front, right) = self.movement_basis();
        let input_velocity = self.movement_state.get_velocity(front, right, Vec3::ZERO);

        // detect whether the player is on the ground
        let is_on_ground = collision_result.ground_distance
            <= self.desc.camera_height + GROUND_EPSILON
            && self.rigidbody.velocity.y <= 0.0;

        // update rigidbody grounded state
        self.rigidbody.is_grounded = is_on_ground;

        // convert input to acceleration forces
        const GROUND_ACCELERATION: f32 = 20.0; // m/s² - how fast you accelerate on ground
        const AIR_ACCELERATION: f32 = 10.0; // m/s² - how fast you accelerate in air

        let acceleration_force = if is_on_ground {
            GROUND_ACCELERATION
        } else {
            AIR_ACCELERATION
        };

        // calculate desired horizontal velocity
        let desired_horizontal_velocity = Vec3::new(input_velocity.x, 0.0, input_velocity.z);
        let current_horizontal_velocity =
            Vec3::new(self.rigidbody.velocity.x, 0.0, self.rigidbody.velocity.z);

        // calculate velocity difference and convert to acceleration
        let velocity_difference = desired_horizontal_velocity - current_horizontal_velocity;
        let acceleration = velocity_difference * acceleration_force;

        // apply acceleration to velocity using delta time
        self.rigidbody.velocity.x += acceleration.x * frame_delta_time;
        self.rigidbody.velocity.z += acceleration.z * frame_delta_time;

        // handle vertical physics
        if is_on_ground {
            // clamp any remaining downward velocity when touching ground
            if self.vertical_velocity < 0.0 {
                self.vertical_velocity = 0.0;
            }

            if self.movement_state.jump_requested {
                // launch the jump
                self.vertical_velocity = JUMP_IMPULSE;
                self.rigidbody.velocity.y = JUMP_IMPULSE;
                // play jump sound once, immediately when leaving the ground
                let current_speed = self.rigidbody.velocity.length();
                let foot_position = Vec3::new(
                    self.position.x,
                    self.position.y - self.desc.camera_height,
                    self.position.z,
                );
                self.player_audio_controller
                    .play_jumping(current_speed, foot_position);
            } else {
                // stick to ground smoothly
                let ground_level_y = self.position.y - collision_result.ground_distance;
                let target_camera_y = ground_level_y + self.desc.camera_height;
                self.position.y += (target_camera_y - self.position.y) * Y_SMOOTHING_ALPHA;
                self.rigidbody.velocity.y = 0.0;
            }
        } else {
            // airborne: apply gravity
            self.vertical_velocity -= GRAVITY_G * frame_delta_time;
            self.rigidbody.velocity.y = self.vertical_velocity;
            // track speed before landing for landing sound volume
            self.pre_landing_speed = self.rigidbody.velocity.length();
        }

        // reset the jump flag after use
        self.movement_state.reset_jump_request();

        // apply drag to horizontal velocity
        self.rigidbody.velocity.x *= self.rigidbody.drag;
        self.rigidbody.velocity.z *= self.rigidbody.drag;

        // resolve horizontal collisions only (preserve vertical velocity for gravity)
        let mut horizontal_velocity =
            Vec3::new(self.rigidbody.velocity.x, 0.0, self.rigidbody.velocity.z);
        let vertical_velocity = self.rigidbody.velocity.y;
        let mut collision_iterations = 0;

        while collision_iterations < MAX_COLLISION_ITERATIONS {
            let collision_detected = self.resolve_horizontal_collision_step(
                &mut horizontal_velocity,
                &collision_result,
                COLLISION_THRESHOLD,
                frame_delta_time,
            );

            if !collision_detected {
                break;
            }

            collision_iterations += 1;
        }

        if collision_iterations >= MAX_COLLISION_ITERATIONS {
            horizontal_velocity = Vec3::ZERO;
        }

        // combine resolved horizontal velocity with preserved vertical velocity
        let resolved_velocity = Vec3::new(
            horizontal_velocity.x,
            vertical_velocity,
            horizontal_velocity.z,
        );
        self.rigidbody.velocity = resolved_velocity;

        // integrate position
        let position_delta = self.rigidbody.velocity * frame_delta_time;
        self.position += position_delta;

        // audio: foot-steps, jump, land
        let is_moving = self.movement_state.is_moving_horizontally();
        let is_running = self.movement_state.is_boosted;

        let just_landed = is_on_ground && !self.was_on_ground;
        if just_landed {
            if is_moving {
                // moving when touching ground: treat as an immediate step
                let current_speed = self.rigidbody.velocity.length();
                let foot_position = Vec3::new(
                    self.position.x,
                    self.position.y - self.desc.camera_height,
                    self.position.z,
                );
                self.player_audio_controller
                    .play_step(is_running, current_speed, foot_position);
                // reset timer so下一次步伐重新计时
                self.player_audio_controller.reset_walk_timer();
            } else {
                // still: play landing sound only
                let foot_position = Vec3::new(
                    self.position.x,
                    self.position.y - self.desc.camera_height,
                    self.position.z,
                );
                self.player_audio_controller
                    .play_landing(self.pre_landing_speed, foot_position);
                // 不重置计时器，让 update_walk_sound 在静止状态保持间隔满值
            }
        }

        // per-frame update for regular walk/run sounds
        let current_speed = self.rigidbody.velocity.length();
        let foot_position = Vec3::new(
            self.position.x,
            self.position.y - self.desc.camera_height,
            self.position.z,
        );
        self.player_audio_controller.update_walk_sound(
            is_on_ground,
            is_moving,
            is_running,
            current_speed,
            frame_delta_time,
            foot_position,
        );

        self.was_on_ground = is_on_ground;
    }

    /// Resets the rigidbody velocity and vertical velocity when switching modes
    pub fn reset_velocity(&mut self) {
        self.rigidbody.velocity = Vec3::ZERO;
        self.vertical_velocity = 0.0;
    }

    // /// Updates the spatial sound manager for the camera's audio controller
    // pub fn set_spatial_sound_manager(&mut self, spatial_sound_manager: SpatialSoundManager) {
    //     self.player_audio_controller
    //         .set_spatial_sound_manager(spatial_sound_manager);
    // }

    /// Resolve a single horizontal collision step using the 32-ray collision system
    /// Returns true if a collision was detected and resolved
    fn resolve_horizontal_collision_step(
        &self,
        horizontal_velocity: &mut Vec3,
        collision_result: &PlayerCollisionResult,
        collision_threshold: f32,
        frame_delta_time: f32,
    ) -> bool {
        let num_rings = collision_result.ring_distances.len();
        if num_rings == 0 {
            return false;
        }

        let velocity_magnitude = horizontal_velocity.length();

        // skip collision if velocity is too small
        if velocity_magnitude < 0.001 {
            return false;
        }

        // calculate movement direction in XZ plane
        let (front, right) = self.movement_basis();
        let camera_front_2d = Vec2::new(front.x, front.z).normalize();
        let camera_right_2d = Vec2::new(right.x, right.z).normalize();
        let movement_2d = Vec2::new(horizontal_velocity.x, horizontal_velocity.z);

        // project movement onto camera basis
        let forward_component = movement_2d.dot(camera_front_2d);
        let right_component = movement_2d.dot(camera_right_2d);

        // calculate movement angle relative to camera front
        let movement_angle = right_component.atan2(forward_component);

        let mut collision_detected = false;
        let mut collision_normal = Vec3::ZERO;
        let mut min_distance = f32::INFINITY;

        // check all ring rays for collisions
        for i in 0..num_rings {
            let ring_distance = collision_result.ring_distances[i];

            // calculate ring direction angle
            let ring_angle = if i == 0 {
                0.0 // forward direction
            } else {
                2.0 * std::f32::consts::PI * (i - 1) as f32 / (num_rings - 1) as f32
            };

            // calculate angle difference between movement and ring
            let mut angle_diff = (movement_angle - ring_angle).abs();
            angle_diff = angle_diff.min(2.0 * std::f32::consts::PI - angle_diff);

            // use a wider collision cone for better collision detection
            let collision_cone_angle = std::f32::consts::PI * 0.6; // 108 degrees
            let weight = (1.0 - angle_diff / collision_cone_angle).max(0.0);

            if weight > 0.0 && ring_distance < collision_threshold {
                collision_detected = true;

                // calculate horizontal collision normal from ring direction
                let ring_direction = Vec3::new(
                    front.x * ring_angle.cos() + right.x * ring_angle.sin(),
                    0.0, // Keep collision normal horizontal only
                    front.z * ring_angle.cos() + right.z * ring_angle.sin(),
                );

                // weight the collision normal by the angular alignment
                collision_normal += ring_direction * weight;
                min_distance = min_distance.min(ring_distance);
            }
        }

        if collision_detected {
            // normalize the collision normal (horizontal only)
            collision_normal = collision_normal.normalize();

            // calculate penetration depth
            let penetration_depth = collision_threshold - min_distance;

            // apply horizontal collision response for sliding
            if penetration_depth > 0.0 {
                // separate from collision by moving away from collision normal
                let separation_distance = penetration_depth * 1.1; // Add small margin
                let separation_velocity = collision_normal * separation_distance / frame_delta_time;

                // project horizontal velocity onto collision normal to get collision component
                let velocity_along_normal = horizontal_velocity.dot(collision_normal);

                if velocity_along_normal < 0.0 {
                    // project velocity onto the collision tangent plane for sliding
                    let collision_velocity = collision_normal * velocity_along_normal;
                    *horizontal_velocity -= collision_velocity; // Remove the component going into the wall

                    // add separation velocity to prevent interpenetration
                    *horizontal_velocity += separation_velocity;
                }
            } else {
                // simple velocity projection for approaching collision (sliding)
                let velocity_along_normal = horizontal_velocity.dot(collision_normal);
                if velocity_along_normal < 0.0 {
                    // project velocity onto tangent plane to allow sliding
                    let collision_velocity = collision_normal * velocity_along_normal;
                    *horizontal_velocity -= collision_velocity;
                }
            }
        }

        collision_detected
    }
}
