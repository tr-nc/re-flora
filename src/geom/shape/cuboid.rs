use glam::{Quat, Vec3};

use crate::geom::Aabb3;

/// Descriptor for a 3D rectangular prism (cuboid)
#[derive(Debug, Clone)]
pub struct Cuboid {
    center: Vec3,
    half_size: Vec3,
    rotation: Quat,
}

impl Cuboid {
    #[allow(dead_code)]
    pub fn new(center: Vec3, half_size: Vec3) -> Self {
        Self::new_oriented(center, half_size, Quat::IDENTITY)
    }

    pub fn new_oriented(center: Vec3, half_size: Vec3, rotation: Quat) -> Self {
        assert!(
            rotation.is_finite() && rotation.length_squared() > f32::EPSILON,
            "cuboid rotation must be a finite non-zero quaternion"
        );
        Cuboid {
            center,
            half_size,
            rotation: rotation.normalize(),
        }
    }

    #[allow(dead_code)]
    pub fn from_min_max(min: Vec3, max: Vec3) -> Self {
        let center = (min + max) * 0.5;
        let half_size = (max - min) * 0.5;
        Cuboid {
            center,
            half_size,
            rotation: Quat::IDENTITY,
        }
    }

    #[allow(dead_code)]
    pub fn center(&self) -> Vec3 {
        self.center
    }

    #[allow(dead_code)]
    pub fn half_size(&self) -> Vec3 {
        self.half_size
    }

    pub fn rotation(&self) -> Quat {
        self.rotation
    }

    #[allow(dead_code)]
    pub fn min(&self) -> Vec3 {
        self.aabb().min()
    }

    #[allow(dead_code)]
    pub fn max(&self) -> Vec3 {
        self.aabb().max()
    }

    #[allow(dead_code)]
    pub fn transform(&mut self, offset: Vec3) {
        self.center += offset;
    }

    #[allow(dead_code)]
    pub fn scale(&mut self, scale: Vec3) {
        self.half_size *= scale;
        self.center *= scale;
    }

    #[allow(dead_code)]
    pub fn aabb(&self) -> Aabb3 {
        let axis_x = self.rotation * Vec3::X * self.half_size.x;
        let axis_y = self.rotation * Vec3::Y * self.half_size.y;
        let axis_z = self.rotation * Vec3::Z * self.half_size.z;
        let extent = axis_x.abs() + axis_y.abs() + axis_z.abs();
        Aabb3::new(self.center - extent, self.center + extent)
    }

    #[cfg(test)]
    fn contains(&self, point: Vec3) -> bool {
        let local = self.rotation.conjugate() * (point - self.center);
        local.abs().cmple(self.half_size).all()
    }

    #[allow(dead_code)]
    pub fn width(&self) -> f32 {
        self.half_size.x * 2.0
    }

    #[allow(dead_code)]
    pub fn height(&self) -> f32 {
        self.half_size.y * 2.0
    }

    #[allow(dead_code)]
    pub fn depth(&self) -> f32 {
        self.half_size.z * 2.0
    }

    #[allow(dead_code)]
    pub fn volume(&self) -> f32 {
        self.width() * self.height() * self.depth()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_4;

    #[test]
    fn oriented_cuboid_contains_points_in_local_space() {
        let rotation = Quat::from_rotation_y(FRAC_PI_4);
        let cuboid = Cuboid::new_oriented(Vec3::ZERO, Vec3::new(2.0, 1.0, 0.5), rotation);

        assert!(cuboid.contains(rotation * Vec3::new(1.9, 0.9, 0.4)));
        assert!(!cuboid.contains(rotation * Vec3::new(2.1, 0.0, 0.0)));
        assert!(!cuboid.contains(rotation * Vec3::new(0.0, 0.0, 0.6)));
    }

    #[test]
    fn oriented_cuboid_aabb_covers_all_rotated_corners() {
        let rotation = Quat::from_rotation_y(FRAC_PI_4);
        let cuboid = Cuboid::new_oriented(
            Vec3::new(10.0, 20.0, 30.0),
            Vec3::new(4.0, 3.0, 2.0),
            rotation,
        );
        let aabb = cuboid.aabb();

        for x in [-4.0, 4.0] {
            for y in [-3.0, 3.0] {
                for z in [-2.0, 2.0] {
                    let corner = cuboid.center() + rotation * Vec3::new(x, y, z);
                    assert!(corner.cmpge(aabb.min()).all());
                    assert!(corner.cmple(aabb.max()).all());
                }
            }
        }
        assert!(aabb.min().x < cuboid.center().x - 4.0);
        assert!(aabb.max().z > cuboid.center().z + 4.0);
    }
}
