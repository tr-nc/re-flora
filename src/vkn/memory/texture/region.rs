use super::Image;

pub struct TextureRegion {
    pub offset: [i32; 3],
    pub extent: [u32; 3],
}

impl Default for TextureRegion {
    fn default() -> Self {
        Self {
            offset: [0, 0, 0],
            extent: [0, 0, 0],
        }
    }
}

impl TextureRegion {
    /// Creates a new `TextureRegion3d` from an `Image`.
    ///
    /// The created region represents the entire image's region
    pub fn from_image(image: &Image) -> Self {
        Self {
            offset: [0, 0, 0],
            extent: [
                image.get_desc().extent[0],
                image.get_desc().extent[1],
                image.get_desc().extent[2],
            ],
        }
    }
}
