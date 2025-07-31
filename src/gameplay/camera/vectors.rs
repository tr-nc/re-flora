use glam::Vec3;

#[derive(Debug, Clone, PartialEq)]
pub struct CameraVectors {
    pub front: Vec3,
    pub up: Vec3,
    pub right: Vec3,
}

impl CameraVectors {
    const WORLD_UP: Vec3 = Vec3::new(0.0, 1.0, 0.0);

    pub fn new() -> Self {
        Self {
            front: Vec3::ZERO,
            up: Vec3::ZERO,
            right: Vec3::ZERO,
        }
    }

    /// Updates the camera's front, right, and up vectors based on the current yaw and pitch.
    pub fn update(&mut self, yaw: f32, pitch: f32) {
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
