#[derive(Debug)]
pub struct CameraMovementDesc {
    pub normal_speed: f32,
    pub boosted_speed_mul: f32,
    pub mouse_sensitivity: f32,
}

impl Default for CameraMovementDesc {
    fn default() -> Self {
        Self {
            normal_speed: 0.2,
            boosted_speed_mul: 4.0,
            mouse_sensitivity: 1.0,
        }
    }
}

#[derive(Debug)]
pub struct CameraProjectionDesc {
    pub v_fov: f32,
    pub z_near: f32,
    pub z_far: f32,
}

impl Default for CameraProjectionDesc {
    fn default() -> Self {
        Self {
            v_fov: 60.0,
            z_near: 0.1,
            z_far: 100000.0,
        }
    }
}

#[derive(Debug)]
pub struct CameraDesc {
    pub movement: CameraMovementDesc,
    pub projection: CameraProjectionDesc,
    pub aspect_ratio: f32,
}
