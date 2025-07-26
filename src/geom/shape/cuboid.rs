use glam::Vec3;

use crate::geom::Aabb3;

/// Descriptor for a 3D rectangular prism (cuboid)
#[derive(Debug, Clone)]
pub struct Cuboid {
    center: Vec3,
    half_size: Vec3,
}

impl Cuboid {
    #[allow(dead_code)]
    pub fn new(center: Vec3, half_size: Vec3) -> Self {
        Cuboid { center, half_size }
    }

    #[allow(dead_code)]
    pub fn from_min_max(min: Vec3, max: Vec3) -> Self {
        let center = (min + max) * 0.5;
        let half_size = (max - min) * 0.5;
        Cuboid { center, half_size }
    }

    #[allow(dead_code)]
    pub fn center(&self) -> Vec3 {
        self.center
    }

    #[allow(dead_code)]
    pub fn half_size(&self) -> Vec3 {
        self.half_size
    }

    #[allow(dead_code)]
    pub fn min(&self) -> Vec3 {
        self.center - self.half_size
    }

    #[allow(dead_code)]
    pub fn max(&self) -> Vec3 {
        self.center + self.half_size
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
        // The AABB is simply defined by the min and max corners
        let min = self.center - self.half_size;
        let max = self.center + self.half_size;
        Aabb3::new(min, max)
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
