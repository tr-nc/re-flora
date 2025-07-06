use glam::{Mat4, Vec2, Vec3, Vec4};
use winit::{
    event::{ElementState, KeyEvent},
    keyboard::{KeyCode, PhysicalKey},
};

use crate::vkn::Extent2D;

use super::CameraDesc;

#[derive(Debug)]
struct AxesState {
    forward: bool,
    backward: bool,
    left: bool,
    right: bool,
    up: bool,
    down: bool,
}

impl Default for AxesState {
    fn default() -> Self {
        Self {
            forward: false,
            backward: false,
            left: false,
            right: false,
            up: false,
            down: false,
        }
    }
}

/// Stores the current state of the camera's movement.
#[derive(Debug)]
struct MovementState {
    normal_speed: f32,
    boosted_speed_mul: f32,
    is_boosted: bool,
    axes: AxesState,
    // indicates that a jump should be performed in the next physics update
    jump_requested: bool,
}

impl MovementState {
    fn new(normal_speed: f32, boosted_speed_mul: f32) -> Self {
        Self {
            normal_speed,
            boosted_speed_mul,
            is_boosted: false,
            axes: AxesState::default(),
            jump_requested: false,
        }
    }

    fn get_velocity(&self, front: Vec3, right: Vec3, up: Vec3) -> Vec3 {
        let mut velocity = Vec3::ZERO;
        if self.axes.forward {
            velocity += front;
        }
        if self.axes.backward {
            velocity -= front;
        }
        if self.axes.left {
            velocity -= right;
        }
        if self.axes.right {
            velocity += right;
        }
        if self.axes.up {
            velocity += up;
        }
        if self.axes.down {
            velocity -= up;
        }
        velocity.normalize_or_zero() * self.current_speed()
    }

    fn current_speed(&self) -> f32 {
        if self.is_boosted {
            self.normal_speed * self.boosted_speed_mul
        } else {
            self.normal_speed
        }
    }
}

struct CameraVectors {
    front: Vec3,
    up: Vec3,
    right: Vec3,
}

impl CameraVectors {
    const WORLD_UP: Vec3 = Vec3::new(0.0, 1.0, 0.0);

    fn new() -> Self {
        Self {
            front: Vec3::ZERO,
            up: Vec3::ZERO,
            right: Vec3::ZERO,
        }
    }

    /// Updates the camera's front, right, and up vectors based on the current yaw and pitch.
    fn update(&mut self, yaw: f32, pitch: f32) {
        self.front = Vec3::new(
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            -yaw.cos() * pitch.cos(),
        )
        .normalize();
        self.right = self.front.cross(Self::WORLD_UP).normalize();
        self.up = self.right.cross(self.front).normalize();
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
}

impl Camera {
    pub fn new(
        initial_position: Vec3,
        initial_yaw: f32,
        initial_pitch: f32,
        desc: CameraDesc,
    ) -> Self {
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
        };

        camera.vectors.update(camera.yaw, camera.pitch);
        camera
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
        if let PhysicalKey::Code(code) = key_event.physical_key {
            if key_event.repeat {
                return;
            }

            match key_event.state {
                ElementState::Pressed => match code {
                    KeyCode::ShiftLeft => self.movement_state.is_boosted = true,
                    KeyCode::KeyW => self.movement_state.axes.forward = true,
                    KeyCode::KeyS => self.movement_state.axes.backward = true,
                    KeyCode::KeyA => self.movement_state.axes.left = true,
                    KeyCode::KeyD => self.movement_state.axes.right = true,
                    KeyCode::Space => {
                        self.movement_state.axes.up = true;
                        self.movement_state.jump_requested = true;
                    }
                    KeyCode::ControlLeft => self.movement_state.axes.down = true,
                    _ => {}
                },
                ElementState::Released => match code {
                    KeyCode::ShiftLeft => self.movement_state.is_boosted = false,
                    KeyCode::KeyW => self.movement_state.axes.forward = false,
                    KeyCode::KeyS => self.movement_state.axes.backward = false,
                    KeyCode::KeyA => self.movement_state.axes.left = false,
                    KeyCode::KeyD => self.movement_state.axes.right = false,
                    KeyCode::Space => self.movement_state.axes.up = false,
                    KeyCode::ControlLeft => self.movement_state.axes.down = false,
                    _ => {}
                },
            }
        }
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

    // pub fn update_transform_fly_mode(&mut self, frame_delta_time: f32) {
    //     let (front, right) = self.movement_basis();
    //     self.position += self.movement_state.get_velocity(front, right, Vec3::Y) * frame_delta_time;
    // }

    pub fn update_transform_fly_mode(&mut self, frame_delta_time: f32) {
        // move in the camera's local axes (front/right/up)
        self.position += self.movement_state.get_velocity(
            self.vectors.front,
            self.vectors.right,
            self.vectors.up,
        ) * frame_delta_time;
    }

    pub fn update_transform_walk_mode(&mut self, frame_delta_time: f32, ground_distance: f32) {
        const GRAVITY_G: f32 = 2.0;
        const JUMP_SPEED: f32 = 0.5;
        const GROUND_EPSILON: f32 = 0.001;

        let (front, right) = self.movement_basis();
        let mut movement_velocity = self.movement_state.get_velocity(front, right, Vec3::Y);
        // ensure pure horizontal movement for walking
        movement_velocity.y = 0.0;

        self.vertical_velocity -= GRAVITY_G * frame_delta_time;

        if self.movement_state.jump_requested
            && ground_distance <= self.desc.camera_height + GROUND_EPSILON
        {
            self.vertical_velocity = JUMP_SPEED;
        }
        self.movement_state.jump_requested = false;

        let total_velocity = movement_velocity + Vec3::new(0.0, self.vertical_velocity, 0.0);
        self.position += total_velocity * frame_delta_time;

        if ground_distance < self.desc.camera_height && self.vertical_velocity < 0.0 {
            let correction = self.desc.camera_height - ground_distance;
            self.position.y += correction;
            self.vertical_velocity = 0.0;
        }
    }

    #[allow(dead_code)]
    pub fn get_frustum_corners(&self) -> [Vec3; 8] {
        let view_proj_inv = (Self::calculate_proj_mat(
            self.desc.projection.v_fov,
            self.desc.aspect_ratio,
            self.desc.projection.z_near,
            1.0,
        ) * self.get_view_mat())
        .inverse();

        let mut corners = [Vec3::ZERO; 8];
        let mut i = 0;
        for z in &[0.0, 1.0] {
            // Near, Far
            for y in &[-1.0, 1.0] {
                // Bottom, Top
                for x in &[-1.0, 1.0] {
                    // Left, Right
                    // From normalized device coordinates (NDC) to world space
                    let p = view_proj_inv * Vec4::new(*x, *y, *z, 1.0);
                    corners[i] = p.truncate() / p.w;
                    i += 1;
                }
            }
        }
        corners
    }
}
