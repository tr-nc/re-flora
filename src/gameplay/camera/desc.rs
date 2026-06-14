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
pub struct CameraDesc {
    pub projection: CameraProjectionDesc,
    pub aspect_ratio: f32,
}

impl Default for CameraDesc {
    fn default() -> Self {
        Self {
            projection: CameraProjectionDesc::default(),
            aspect_ratio: 16.0 / 9.0,
        }
    }
}
