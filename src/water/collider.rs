use glam::Vec3;

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
        Self::new(Vec3::new(1.0, 1.0, 1.0), Vec3::new(2.0, 2.0, 2.0))
    }
}
