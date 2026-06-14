use super::{vectors::CameraVectors, CameraDesc};
use crate::audio::SpatialSoundManager;
use anyhow::Result;
use glam::{Mat4, Vec2, Vec3, Vec4};
use verdarium_vkn::Extent2D;
use winit::{
    event::{ElementState, KeyEvent},
    keyboard::{KeyCode, PhysicalKey},
};

/// Tracks keyboard state for free-flight movement.
#[derive(Debug, Clone)]
struct FlyMovement {
    forward: bool,
    backward: bool,
    left: bool,
    right: bool,
    up: bool,
    down: bool,
    boost: bool,
}

impl FlyMovement {
    fn new() -> Self {
        Self {
            forward: false,
            backward: false,
            left: false,
            right: false,
            up: false,
            down: false,
            boost: false,
        }
    }

    fn reset(&mut self) {
        self.forward = false;
        self.backward = false;
        self.left = false;
        self.right = false;
        self.up = false;
        self.down = false;
        self.boost = false;
    }

    fn handle_key(&mut self, code: KeyCode, pressed: bool) {
        match code {
            KeyCode::KeyW => self.forward = pressed,
            KeyCode::KeyS => self.backward = pressed,
            KeyCode::KeyA => self.left = pressed,
            KeyCode::KeyD => self.right = pressed,
            KeyCode::Space => self.up = pressed,
            KeyCode::ControlLeft => self.down = pressed,
            KeyCode::ShiftLeft => self.boost = pressed,
            _ => {}
        }
    }

    fn velocity(&self, front: Vec3, right: Vec3, up: Vec3, speed: f32) -> Vec3 {
        let mut vel = Vec3::ZERO;
        if self.forward {
            vel += front;
        }
        if self.backward {
            vel -= front;
        }
        if self.left {
            vel -= right;
        }
        if self.right {
            vel += right;
        }
        if self.up {
            vel += up;
        }
        if self.down {
            vel -= up;
        }
        let speed = if self.boost { speed * 2.2 } else { speed };
        vel.normalize_or_zero() * speed
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraPose {
    pub position: Vec3,
    pub yaw_deg: f32,
    pub pitch_deg: f32,
    pub fov_deg: f32,
}

pub struct Camera {
    position: Vec3,
    yaw: f32,
    pitch: f32,
    vectors: CameraVectors,
    desc: CameraDesc,
    fly: FlyMovement,
    normal_speed: f32,
}

impl Camera {
    pub fn new(
        initial_position: Vec3,
        initial_yaw: f32,
        initial_pitch: f32,
        desc: CameraDesc,
        _spatial_sound_manager: SpatialSoundManager,
    ) -> Result<Self> {
        let mut camera = Self {
            position: initial_position,
            vectors: CameraVectors::new(),
            yaw: initial_yaw.to_radians(),
            pitch: initial_pitch.to_radians(),
            desc,
            fly: FlyMovement::new(),
            normal_speed: 0.25,
        };

        camera.vectors.update(camera.yaw, camera.pitch);
        Ok(camera)
    }

    pub fn set_footstep_volume_gain(&mut self, _volume_gain: f32) {}

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
        self.fly.reset();
    }

    // ---- free-flight input & update ----

    pub fn handle_keyboard(&mut self, key_event: &KeyEvent) {
        if let PhysicalKey::Code(code) = key_event.physical_key {
            self.fly
                .handle_key(code, key_event.state == ElementState::Pressed);
        }
    }

    pub fn handle_mouse(&mut self, delta: Vec2) {
        const SENSITIVITY_MULTIPLIER: f32 = 0.001;
        self.yaw += delta.x * SENSITIVITY_MULTIPLIER;
        self.pitch -= delta.y * SENSITIVITY_MULTIPLIER;
        self.limit_yaw();
        self.clamp_pitch();
        self.vectors.update(self.yaw, self.pitch);
    }

    pub fn reset_input(&mut self) {
        self.fly.reset();
    }

    pub fn reset_velocity(&mut self) {}

    pub fn update_transform(&mut self, frame_delta_time: f32) {
        self.position += self.fly.velocity(
            self.vectors.front,
            self.vectors.right,
            self.vectors.up,
            self.normal_speed,
        ) * frame_delta_time;
    }

    /// Limits the yaw to prevent the camera from spinning indefinitely.
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
}
