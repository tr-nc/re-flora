use glam::{Mat4, Vec3, Vec4};
use winit::{
    event::{ElementState, KeyEvent},
    keyboard::{KeyCode, PhysicalKey},
};

/// Configuration for the camera's basic settings.
#[derive(Debug)]
pub struct CameraBasicConfig {
    pub normal_speed: f32,
    pub boosted_speed_mul: f32,
    pub mouse_sensitivity: f32,
    pub vertical_fov: f32,
}

impl Default for CameraBasicConfig {
    fn default() -> Self {
        Self {
            normal_speed: 2.5,
            boosted_speed_mul: 6.0,
            mouse_sensitivity: 1.0,
            vertical_fov: 60.0,
        }
    }
}

impl CameraBasicConfig {
    fn validate(&self) -> Result<(), String> {
        if self.normal_speed <= 0.0 {
            return Err("normal_speed must be greater than 0".to_string());
        }
        if self.boosted_speed_mul <= 0.0 {
            return Err("boosted_speed_mul must be greater than 0".to_string());
        }
        if self.mouse_sensitivity <= 0.0 {
            return Err("mouse_sensitivity must be greater than 0".to_string());
        }
        if self.vertical_fov <= 0.0 || self.vertical_fov >= 180.0 {
            return Err("v_fov_deg must be in the range (0, 180)".to_string());
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct CameraCreateInfo {
    /// The initial position of the camera.
    pub position: Vec3,
    /// The initial yaw of the camera in degrees.
    ///
    /// Yaw is the angle between the yz-plane and the camera's forward vector,
    /// when the x component of the camera's forward vector is positive, yaw
    /// ranges from 0 to 180 degrees, otherwise it ranges from 0 to -180 degrees,
    pub yaw: f32,
    /// The initial pitch of the camera in degrees.
    ///
    /// Pitch is the angle between the xz-plane and the camera's forward vector,
    /// when the y component of the camera's forward vector is positive, pitch
    /// ranges from 0 to 90 degrees, otherwise it ranges from 0 to -90 degrees,
    pub pitch: f32,
    pub config: CameraBasicConfig,
}

impl Default for CameraCreateInfo {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            yaw: 180.0,
            pitch: 0.0,
            config: CameraBasicConfig::default(),
        }
    }
}

impl CameraCreateInfo {
    pub fn validate(&self) -> Result<(), String> {
        if let Err(msg) = self.config.validate() {
            return Err(msg);
        }
        if self.pitch < -90.0 || self.pitch > 90.0 {
            return Err("pitch must be in the range (-90, 90)".to_string());
        }
        if self.yaw < -180.0 || self.yaw > 180.0 {
            return Err("yaw must be in the range (-180, 180)".to_string());
        }
        Ok(())
    }
}

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
}

impl MovementState {
    fn new(normal_speed: f32, boosted_speed_mul: f32) -> Self {
        Self {
            normal_speed,
            boosted_speed_mul,
            is_boosted: false,
            axes: AxesState::default(),
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

pub struct Camera {
    position: Vec3,
    front: Vec3,
    up: Vec3,
    right: Vec3,
    /// The initial yaw of the camera in radians.
    ///
    /// Yaw is the angle between the yz-plane and the camera's forward vector,
    /// when the x component of the camera's forward vector is positive, yaw
    /// ranges from 0 to 180 degrees, otherwise it ranges from 0 to -180 degrees,
    pub yaw: f32,
    /// The initial pitch of the camera in radians.
    ///
    /// Pitch is the angle between the xz-plane and the camera's forward vector,
    /// when the y component of the camera's forward vector is positive, pitch
    /// ranges from 0 to 90 degrees, otherwise it ranges from 0 to -90 degrees,
    pub pitch: f32,
    config: CameraBasicConfig,

    movement_state: MovementState,
}

impl Default for Camera {
    fn default() -> Self {
        Camera::new(CameraCreateInfo::default())
    }
}

impl Camera {
    const WORLD_UP: Vec3 = Vec3::new(0.0, 1.0, 0.0);

    pub fn new(create_info: CameraCreateInfo) -> Self {
        if let Err(msg) = create_info.validate() {
            panic!("Failed to create camera: {}", msg);
        }

        let normal_speed = create_info.config.normal_speed;
        let boosted_speed_mul = create_info.config.boosted_speed_mul;

        let mut camera = Self {
            position: create_info.position,
            front: Vec3::ZERO,
            up: Vec3::ZERO,
            right: Vec3::ZERO,
            yaw: create_info.yaw.to_radians(),
            pitch: create_info.pitch.to_radians(),
            config: create_info.config,
            movement_state: MovementState::new(normal_speed, boosted_speed_mul),
        };
        camera.update_camera_vectors();
        camera
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

    #[allow(dead_code)]
    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.position, self.position + self.front, self.up)
    }

    #[allow(dead_code)]
    pub fn proj_matrix(&self, aspect_ratio: f32, z_near: f32, z_far: f32) -> Mat4 {
        Mat4::perspective_rh(
            self.config.vertical_fov.to_radians(),
            aspect_ratio,
            z_near,
            z_far,
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
                    KeyCode::ShiftLeft => {
                        self.movement_state.is_boosted = true;
                    }
                    KeyCode::KeyW => {
                        self.movement_state.axes.forward = true;
                    }
                    KeyCode::KeyS => {
                        self.movement_state.axes.backward = true;
                    }
                    KeyCode::KeyA => {
                        self.movement_state.axes.left = true;
                    }
                    KeyCode::KeyD => {
                        self.movement_state.axes.right = true;
                    }
                    KeyCode::Space => {
                        self.movement_state.axes.up = true;
                    }
                    KeyCode::ControlLeft => {
                        self.movement_state.axes.down = true;
                    }
                    _ => {}
                },
                ElementState::Released => match code {
                    KeyCode::ShiftLeft => {
                        self.movement_state.is_boosted = false;
                    }
                    KeyCode::KeyW => {
                        self.movement_state.axes.forward = false;
                    }
                    KeyCode::KeyS => {
                        self.movement_state.axes.backward = false;
                    }
                    KeyCode::KeyA => {
                        self.movement_state.axes.left = false;
                    }
                    KeyCode::KeyD => {
                        self.movement_state.axes.right = false;
                    }
                    KeyCode::Space => {
                        self.movement_state.axes.up = false;
                    }
                    KeyCode::ControlLeft => {
                        self.movement_state.axes.down = false;
                    }
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

    pub fn handle_mouse(&mut self, delta: &(f64, f64)) {
        const SENSITIVITY_MULTIPLIER: f32 = 0.001;
        // the delta is positive when moving the mouse to the right / down
        // so we need to invert the pitch delta so that when mouse is going up, pitch increases
        self.yaw += delta.0 as f32 * self.config.mouse_sensitivity * SENSITIVITY_MULTIPLIER;
        self.pitch -= delta.1 as f32 * self.config.mouse_sensitivity * SENSITIVITY_MULTIPLIER;

        self.limit_yaw();
        self.clamp_pitch();
        self.update_camera_vectors();
    }

    pub fn update_transform(&mut self, delta_time: f32) {
        self.position += self
            .movement_state
            .get_velocity(self.front, self.right, self.up)
            * delta_time;
    }

    /// Updates the camera's front, right, and up vectors based on the current yaw and pitch.
    fn update_camera_vectors(&mut self) {
        self.front = Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            -self.yaw.cos() * self.pitch.cos(),
        )
        .normalize();
        self.right = self.front.cross(Self::WORLD_UP).normalize();
        self.up = self.right.cross(self.front).normalize();
    }
}
