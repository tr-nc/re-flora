pub struct TextureRegion2d {
    pub offset: [i32; 2],
    pub extent: [u32; 2],
}

impl Default for TextureRegion2d {
    fn default() -> Self {
        Self {
            offset: [0, 0],
            extent: [0, 0],
        }
    }
}

pub struct TextureRegion3d {
    pub offset: [i32; 3],
    pub extent: [u32; 3],
}

impl Default for TextureRegion3d {
    fn default() -> Self {
        Self {
            offset: [0, 0, 0],
            extent: [0, 0, 0],
        }
    }
}
