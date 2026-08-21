use glam::{Quat, Vec3};

use crate::geom::Aabb3;

/// A solid torus whose ring lies in local XY and whose normal points along local Z.
#[derive(Debug, Clone)]
pub struct Torus {
    center: Vec3,
    major_radius: f32,
    tube_radius: f32,
    rotation: Quat,
}

impl Torus {
    pub fn new(center: Vec3, major_radius: f32, tube_radius: f32) -> Self {
        Self::new_oriented(center, major_radius, tube_radius, Quat::IDENTITY)
    }

    pub fn new_oriented(center: Vec3, major_radius: f32, tube_radius: f32, rotation: Quat) -> Self {
        assert!(center.is_finite(), "torus center must be finite");
        assert!(
            major_radius.is_finite() && major_radius > 0.0,
            "torus major radius must be finite and positive"
        );
        assert!(
            tube_radius.is_finite() && tube_radius > 0.0 && tube_radius < major_radius,
            "torus tube radius must be finite, positive, and smaller than its major radius"
        );
        assert!(
            rotation.is_finite() && rotation.length_squared() > f32::EPSILON,
            "torus rotation must be a finite non-zero quaternion"
        );
        Self {
            center,
            major_radius,
            tube_radius,
            rotation: rotation.normalize(),
        }
    }

    pub fn center(&self) -> Vec3 {
        self.center
    }

    pub fn major_radius(&self) -> f32 {
        self.major_radius
    }

    pub fn tube_radius(&self) -> f32 {
        self.tube_radius
    }

    pub fn rotation(&self) -> Quat {
        self.rotation
    }

    pub fn inner_radius(&self) -> f32 {
        self.major_radius - self.tube_radius
    }

    pub fn outer_radius(&self) -> f32 {
        self.major_radius + self.tube_radius
    }

    pub fn aabb(&self) -> Aabb3 {
        let local_extent = Vec3::new(self.outer_radius(), self.outer_radius(), self.tube_radius);
        let extent = (self.rotation * Vec3::X * local_extent.x).abs()
            + (self.rotation * Vec3::Y * local_extent.y).abs()
            + (self.rotation * Vec3::Z * local_extent.z).abs();
        Aabb3::new(self.center - extent, self.center + extent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    #[test]
    fn torus_reports_exact_inner_outer_radii_and_local_bound() {
        let torus = Torus::new(Vec3::new(10.0, 20.0, 30.0), 8.0, 2.0);

        assert_eq!(torus.inner_radius(), 6.0);
        assert_eq!(torus.outer_radius(), 10.0);
        assert_eq!(torus.aabb().min(), Vec3::new(0.0, 10.0, 28.0));
        assert_eq!(torus.aabb().max(), Vec3::new(20.0, 30.0, 32.0));
    }

    #[test]
    fn torus_bound_follows_its_oriented_normal() {
        let torus = Torus::new_oriented(Vec3::ZERO, 8.0, 2.0, Quat::from_rotation_y(FRAC_PI_2));
        let dimensions = torus.aabb().dimensions();

        assert!((dimensions.x - 4.0).abs() < 1.0e-4);
        assert!((dimensions.y - 20.0).abs() < 1.0e-4);
        assert!((dimensions.z - 20.0).abs() < 1.0e-4);
    }
}
