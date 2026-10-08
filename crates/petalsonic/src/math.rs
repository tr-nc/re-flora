//! Math types for PetalSonic

pub use glam::{Quat, Vec3};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    pub position: Vec3,
    pub rotation: Quat,
}

impl Pose {
    pub fn new(position: Vec3, rotation: Quat) -> Self {
        Self { position, rotation }
    }

    pub fn identity() -> Self {
        Self {
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
        }
    }

    pub fn from_position(position: Vec3) -> Self {
        Self {
            position,
            rotation: Quat::IDENTITY,
        }
    }

    pub fn from_rotation(rotation: Quat) -> Self {
        Self {
            position: Vec3::ZERO,
            rotation,
        }
    }

    pub fn forward(&self) -> Vec3 {
        self.rotation * (-Vec3::Z)
    }

    pub fn up(&self) -> Vec3 {
        self.rotation * Vec3::Y
    }

    pub fn right(&self) -> Vec3 {
        self.rotation * Vec3::X
    }

    pub fn distance(&self, other: &Self) -> f32 {
        self.position.distance(other.position)
    }

    pub fn look_at(&mut self, target: Vec3, _up: Option<Vec3>) {
        let forward = (target - self.position).normalize();
        self.rotation = Quat::from_rotation_arc(Vec3::Z, -forward);
    }
}

impl Default for Pose {
    fn default() -> Self {
        Self::identity()
    }
}
