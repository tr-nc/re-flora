use glam::Vec3;

use crate::geom::Aabb3;

/// Descriptor for a 3D rectangular prism (cuboid)
#[derive(Debug, Clone)]
pub struct Cuboid {
    center: Vec3,
    half_size: Vec3,
}

impl Cuboid {
    pub fn new(center: Vec3, half_size: Vec3) -> Self {
        Cuboid { center, half_size }
    }

    pub fn from_min_max(min: Vec3, max: Vec3) -> Self {
        let center = (min + max) * 0.5;
        let half_size = (max - min) * 0.5;
        Cuboid { center, half_size }
    }

    pub fn center(&self) -> Vec3 {
        self.center
    }

    pub fn half_size(&self) -> Vec3 {
        self.half_size
    }

    pub fn min(&self) -> Vec3 {
        self.center - self.half_size
    }

    pub fn max(&self) -> Vec3 {
        self.center + self.half_size
    }

    pub fn transform(&mut self, offset: Vec3) {
        self.center += offset;
    }

    pub fn scale(&mut self, scale: Vec3) {
        self.half_size *= scale;
        self.center *= scale;
    }

    pub fn aabb(&self) -> Aabb3 {
        // The AABB is simply defined by the min and max corners
        let min = self.center - self.half_size;
        let max = self.center + self.half_size;
        Aabb3::new(min, max)
    }

    pub fn width(&self) -> f32 {
        self.half_size.x * 2.0
    }

    pub fn height(&self) -> f32 {
        self.half_size.y * 2.0
    }

    pub fn depth(&self) -> f32 {
        self.half_size.z * 2.0
    }

    pub fn volume(&self) -> f32 {
        self.width() * self.height() * self.depth()
    }
}
