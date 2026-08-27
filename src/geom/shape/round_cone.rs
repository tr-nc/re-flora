use glam::Vec3;

use crate::geom::Aabb3;

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

    /// Signed distance to the same capped round-cone volume stamped by the tree shader.
    /// Negative values are inside wood and positive values are outside it.
    pub fn signed_distance(&self, point: Vec3) -> f32 {
        let direction = self.center_b - self.center_a;
        let length_squared = direction.length_squared();
        let radius_difference = self.radius_a - self.radius_b;

        // When one endpoint sphere contains the other, the enclosing round cone collapses to
        // their union. Handling this explicitly also avoids the shader formula's divisions by a
        // zero-length axis in geometry fixtures.
        if length_squared <= radius_difference * radius_difference || length_squared <= f32::EPSILON
        {
            return (point.distance(self.center_a) - self.radius_a)
                .min(point.distance(self.center_b) - self.radius_b);
        }

        let a_squared = length_squared - radius_difference * radius_difference;
        let inverse_length_squared = length_squared.recip();
        let from_a = point - self.center_a;
        let y = from_a.dot(direction);
        let z = y - length_squared;
        let x_vector = from_a * length_squared - direction * y;
        let x_squared = x_vector.length_squared();
        let y_squared = y * y * length_squared;
        let z_squared = z * z * length_squared;
        let sign = |value: f32| {
            if value > 0.0 {
                1.0
            } else if value < 0.0 {
                -1.0
            } else {
                0.0
            }
        };
        let k = sign(radius_difference) * radius_difference * radius_difference * x_squared;

        if sign(z) * a_squared * z_squared > k {
            return (x_squared + z_squared).sqrt() * inverse_length_squared - self.radius_b;
        }
        if sign(y) * a_squared * y_squared < k {
            return (x_squared + y_squared).sqrt() * inverse_length_squared - self.radius_a;
        }

        (x_squared * a_squared * inverse_length_squared).sqrt() * inverse_length_squared
            + y * radius_difference * inverse_length_squared
            - self.radius_a
    }

    #[allow(dead_code)]
    pub fn scale(&mut self, scale: Vec3) {
        self.radius_a *= scale.x;
        self.radius_b *= scale.y;
        self.center_a *= scale;
        self.center_b *= scale;
    }

    pub fn aabb(&self) -> Aabb3 {
        // since the cone/ramp between them never “sticks out” past the larger of the two spherical caps,
        // the union of the two sphere bounds is sufficient.

        // AABB of sphere A
        let r_a = Vec3::splat(self.radius_a);
        let min_a = self.center_a - r_a;
        let max_a = self.center_a + r_a;
        let aabb_a = Aabb3::new(min_a, max_a);

        // AABB of sphere B
        let r_b = Vec3::splat(self.radius_b);
        let min_b = self.center_b - r_b;
        let max_b = self.center_b + r_b;
        let aabb_b = Aabb3::new(min_b, max_b);

        // union of the two
        aabb_a.union(&aabb_b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_distance_matches_a_capped_cylinder() {
        let cone = RoundCone::new(1.0, Vec3::ZERO, 1.0, Vec3::Y * 2.0);

        assert!((cone.signed_distance(Vec3::new(0.0, 1.0, 0.0)) + 1.0).abs() < 1.0e-5);
        assert!((cone.signed_distance(Vec3::new(2.0, 1.0, 0.0)) - 1.0).abs() < 1.0e-5);
        assert!((cone.signed_distance(Vec3::new(0.0, -2.0, 0.0)) - 1.0).abs() < 1.0e-5);
    }
}
