pub struct TextureRegion {
    pub offset: [i32; 2],
    pub extent: [u32; 2],
}

impl Default for TextureRegion {
    fn default() -> Self {
        Self {
            offset: [0, 0],
            extent: [0, 0],
        }
    }
}
