use super::{vectors::CameraVectors, CameraDesc};
use crate::audio::SpatialSoundManager;
use anyhow::Result;
use glam::{Mat4, Vec3, Vec4};
use verdarium_vkn::Extent2D;

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
}
