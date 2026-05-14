use glam::{UVec2, Vec2, Vec3};

/// Axis-aligned world-space container used by the initial tiny pond test.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterBoxCollider {
    pub min_ws: Vec3,
    pub max_ws: Vec3,
}

impl WaterBoxCollider {
    pub const fn new(min_ws: Vec3, max_ws: Vec3) -> Self {
        Self { min_ws, max_ws }
    }

    pub fn extent(self) -> Vec3 {
        self.max_ws - self.min_ws
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn contains(self, point_ws: Vec3) -> bool {
        point_ws.cmpge(self.min_ws).all() && point_ws.cmple(self.max_ws).all()
    }

    pub fn clamp_point(self, point_ws: Vec3, padding: f32) -> Vec3 {
        point_ws.clamp(
            self.min_ws + Vec3::splat(padding),
            self.max_ws - Vec3::splat(padding),
        )
    }
}

impl Default for WaterBoxCollider {
    fn default() -> Self {
        Self::new(Vec3::new(1.0, 0.0, 1.0), Vec3::new(2.0, 1.0, 2.0))
    }
}

/// Sampled world-space terrain bottom for the tiny pond.
#[derive(Clone, Debug, PartialEq)]
pub struct WaterTerrainCollider {
    pub xz_dim: UVec2,
    pub bounds_min_ws: Vec3,
    pub bounds_max_ws: Vec3,
    pub heights_ws: Vec<f32>,
    pub margin: f32,
}

impl WaterTerrainCollider {
    pub fn validate(&self) {
        assert!(self.xz_dim.x >= 2 && self.xz_dim.y >= 2);
        assert!(self.bounds_max_ws.x > self.bounds_min_ws.x);
        assert!(self.bounds_max_ws.z > self.bounds_min_ws.z);
        assert!(self.bounds_min_ws.is_finite());
        assert!(self.bounds_max_ws.is_finite());
        assert!(self.margin.is_finite() && self.margin >= 0.0);

        let expected_len = self
            .xz_dim
            .x
            .checked_mul(self.xz_dim.y)
            .expect("terrain collider dimensions overflow") as usize;
        assert_eq!(self.heights_ws.len(), expected_len);
        assert!(self.heights_ws.iter().all(|height| height.is_finite()));
    }

    pub fn sample_height_ws(&self, xz_ws: Vec2) -> f32 {
        debug_assert!(self.xz_dim.x >= 2 && self.xz_dim.y >= 2);
        debug_assert_eq!(
            self.heights_ws.len(),
            (self.xz_dim.x as usize) * (self.xz_dim.y as usize)
        );

        let extent_x = self.bounds_max_ws.x - self.bounds_min_ws.x;
        let extent_z = self.bounds_max_ws.z - self.bounds_min_ws.z;
        debug_assert!(extent_x > 0.0 && extent_z > 0.0);

        let u = ((xz_ws.x - self.bounds_min_ws.x) / extent_x).clamp(0.0, 1.0);
        let v = ((xz_ws.y - self.bounds_min_ws.z) / extent_z).clamp(0.0, 1.0);
        let grid_x = u * (self.xz_dim.x - 1) as f32;
        let grid_z = v * (self.xz_dim.y - 1) as f32;

        let x0 = (grid_x.floor() as u32).min(self.xz_dim.x - 2);
        let z0 = (grid_z.floor() as u32).min(self.xz_dim.y - 2);
        let x1 = x0 + 1;
        let z1 = z0 + 1;
        let fx = grid_x - x0 as f32;
        let fz = grid_z - z0 as f32;

        let h00 = self.height_at(x0, z0);
        let h10 = self.height_at(x1, z0);
        let h01 = self.height_at(x0, z1);
        let h11 = self.height_at(x1, z1);
        let hx0 = h00 + (h10 - h00) * fx;
        let hx1 = h01 + (h11 - h01) * fx;
        hx0 + (hx1 - hx0) * fz
    }

    pub fn sample_normal_ws(&self, xz_ws: Vec2) -> Vec3 {
        self.sample_height_and_normal_ws(xz_ws).1
    }

    fn sample_height_and_normal_ws(&self, xz_ws: Vec2) -> (f32, Vec3) {
        debug_assert!(self.xz_dim.x >= 2 && self.xz_dim.y >= 2);
        debug_assert_eq!(
            self.heights_ws.len(),
            (self.xz_dim.x as usize) * (self.xz_dim.y as usize)
        );

        let extent_x = self.bounds_max_ws.x - self.bounds_min_ws.x;
        let extent_z = self.bounds_max_ws.z - self.bounds_min_ws.z;
        debug_assert!(extent_x > 0.0 && extent_z > 0.0);

        let u = ((xz_ws.x - self.bounds_min_ws.x) / extent_x).clamp(0.0, 1.0);
        let v = ((xz_ws.y - self.bounds_min_ws.z) / extent_z).clamp(0.0, 1.0);
        let grid_x = u * (self.xz_dim.x - 1) as f32;
        let grid_z = v * (self.xz_dim.y - 1) as f32;

        let x0 = (grid_x.floor() as u32).min(self.xz_dim.x - 2);
        let z0 = (grid_z.floor() as u32).min(self.xz_dim.y - 2);
        let x1 = x0 + 1;
        let z1 = z0 + 1;
        let fx = grid_x - x0 as f32;
        let fz = grid_z - z0 as f32;

        let h00 = self.height_at(x0, z0);
        let h10 = self.height_at(x1, z0);
        let h01 = self.height_at(x0, z1);
        let h11 = self.height_at(x1, z1);
        let hx0 = h00 + (h10 - h00) * fx;
        let hx1 = h01 + (h11 - h01) * fx;
        let height = hx0 + (hx1 - hx0) * fz;

        let cell_dx = extent_x / (self.xz_dim.x - 1) as f32;
        let cell_dz = extent_z / (self.xz_dim.y - 1) as f32;
        let dh_dx = ((h10 - h00) * (1.0 - fz) + (h11 - h01) * fz) / cell_dx;
        let dh_dz = ((h01 - h00) * (1.0 - fx) + (h11 - h10) * fx) / cell_dz;
        let normal = Vec3::new(-dh_dx, 1.0, -dh_dz).normalize_or_zero();
        let normal = if normal.is_finite() && normal.length_squared() > 0.0 {
            normal
        } else {
            Vec3::Y
        };

        (height, normal)
    }

    fn height_at(&self, x: u32, z: u32) -> f32 {
        self.heights_ws[(z as usize * self.xz_dim.x as usize) + x as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_collider_samples_bilinear_heights() {
        let collider = WaterTerrainCollider {
            xz_dim: UVec2::new(2, 2),
            bounds_min_ws: Vec3::ZERO,
            bounds_max_ws: Vec3::new(1.0, 1.0, 1.0),
            heights_ws: vec![1.0, 2.0, 3.0, 5.0],
            margin: 0.0,
        };

        collider.validate();
        assert!((collider.sample_height_ws(Vec2::new(0.5, 0.5)) - 2.75).abs() < 1.0e-6);
    }

    #[test]
    fn terrain_collider_clamps_sample_coordinates() {
        let collider = WaterTerrainCollider {
            xz_dim: UVec2::new(2, 2),
            bounds_min_ws: Vec3::ZERO,
            bounds_max_ws: Vec3::new(1.0, 1.0, 1.0),
            heights_ws: vec![1.0, 2.0, 3.0, 5.0],
            margin: 0.0,
        };

        assert_eq!(collider.sample_height_ws(Vec2::new(-10.0, 10.0)), 3.0);
    }

    #[test]
    fn terrain_collider_samples_flat_normal() {
        let collider = WaterTerrainCollider {
            xz_dim: UVec2::new(2, 2),
            bounds_min_ws: Vec3::ZERO,
            bounds_max_ws: Vec3::new(1.0, 1.0, 1.0),
            heights_ws: vec![0.25; 4],
            margin: 0.0,
        };

        assert_vec3_near(collider.sample_normal_ws(Vec2::new(0.5, 0.5)), Vec3::Y);
    }

    #[test]
    fn terrain_collider_samples_sloped_normal() {
        let collider = WaterTerrainCollider {
            xz_dim: UVec2::new(2, 2),
            bounds_min_ws: Vec3::ZERO,
            bounds_max_ws: Vec3::new(1.0, 1.0, 1.0),
            heights_ws: vec![0.0, 1.0, 0.0, 1.0],
            margin: 0.0,
        };

        assert_vec3_near(
            collider.sample_normal_ws(Vec2::new(0.5, 0.5)),
            Vec3::new(-1.0, 1.0, 0.0).normalize(),
        );
    }

    #[test]
    #[should_panic]
    fn terrain_collider_rejects_invalid_height_count() {
        WaterTerrainCollider {
            xz_dim: UVec2::new(2, 2),
            bounds_min_ws: Vec3::ZERO,
            bounds_max_ws: Vec3::new(1.0, 1.0, 1.0),
            heights_ws: vec![1.0, 2.0, 3.0],
            margin: 0.0,
        }
        .validate();
    }

    fn assert_vec3_near(actual: Vec3, expected: Vec3) {
        assert!(
            (actual - expected).length() < 1.0e-6,
            "actual {actual:?} expected {expected:?}"
        );
    }
}
