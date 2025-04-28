use glam::Vec3;

use crate::geom::Aabb;

/// Descriptor for round cone connecting two spheres on each side
#[derive(Debug, Clone)]
pub struct RoundCone {
    radius_a: f32,
    center_a: Vec3,
    radius_b: f32,
    center_b: Vec3,
}

impl RoundCone {
    pub fn new(radius_a: f32, center_a: Vec3, radius_b: f32, center_b: Vec3) -> Self {
        RoundCone {
            radius_a,
            center_a,
            radius_b,
            center_b,
        }
    }

    pub fn radius_a(&self) -> f32 {
        self.radius_a
    }

    pub fn center_a(&self) -> Vec3 {
        self.center_a
    }

    pub fn radius_b(&self) -> f32 {
        self.radius_b
    }

    pub fn center_b(&self) -> Vec3 {
        self.center_b
    }

    pub fn transform(&mut self, offset: Vec3) {
        self.center_a += offset;
        self.center_b += offset;
    }

    pub fn scale(&mut self, scale: Vec3) {
        self.radius_a *= scale.x;
        self.radius_b *= scale.y;
        self.center_a *= scale;
        self.center_b *= scale;
    }

    pub fn get_aabb(&self) -> Aabb {
        // since the cone/ramp between them never “sticks out” past the larger of the two spherical caps,
        // the union of the two sphere bounds is sufficient.

        // AABB of sphere A
        let r_a = Vec3::splat(self.radius_a);
        let min_a = self.center_a - r_a;
        let max_a = self.center_a + r_a;
        let aabb_a = Aabb::new(min_a, max_a);

        // AABB of sphere B
        let r_b = Vec3::splat(self.radius_b);
        let min_b = self.center_b - r_b;
        let max_b = self.center_b + r_b;
        let aabb_b = Aabb::new(min_b, max_b);

        // union of the two
        aabb_a.union(&aabb_b)
    }
}
