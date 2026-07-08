#[derive(Debug, Clone)]
pub struct CameraMovementDesc {
    pub normal_speed: f32,
    pub boosted_speed_mul: f32,
    pub mouse_sensitivity: f32,
}

impl Default for CameraMovementDesc {
    fn default() -> Self {
        Self {
            normal_speed: 0.25,
            boosted_speed_mul: 2.2,
            mouse_sensitivity: 1.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CameraProjectionDesc {
    pub v_fov: f32,
    pub z_near: f32,
    pub z_far: f32,
}

impl Default for CameraProjectionDesc {
    fn default() -> Self {
        Self {
            v_fov: 60.0,
            // do not go smaller, or the projection matrix will be unstable!
            z_near: 0.01,
            z_far: 10.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CameraHeadBobDesc {
    pub vertical_amplitude: f32,
    pub horizontal_amplitude: f32,
    pub roll_amplitude_deg: f32,
    pub sprint_amplitude_mul: f32,
    pub smoothing_speed: f32,
}

impl Default for CameraHeadBobDesc {
    fn default() -> Self {
        Self {
            vertical_amplitude: 0.003,
            horizontal_amplitude: 0.0015,
            roll_amplitude_deg: 0.3,
            sprint_amplitude_mul: 1.5,
            smoothing_speed: 8.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CameraDesc {
    pub movement: CameraMovementDesc,
    pub projection: CameraProjectionDesc,
    pub head_bob: CameraHeadBobDesc,
    pub aspect_ratio: f32,
    pub camera_height: f32,
}

impl Default for CameraDesc {
    fn default() -> Self {
        Self {
            movement: CameraMovementDesc::default(),
            projection: CameraProjectionDesc::default(),
            head_bob: CameraHeadBobDesc::default(),
            aspect_ratio: 16.0 / 9.0,
            camera_height: 0.08,
        }
    }
}
