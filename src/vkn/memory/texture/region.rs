use super::Image;
use crate::vkn::Extent3D;

pub struct TextureRegion {
    pub offset: [i32; 3],
    pub extent: Extent3D,
}

impl Default for TextureRegion {
    fn default() -> Self {
        Self {
            offset: [0, 0, 0],
            extent: Extent3D::default(),
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
            extent: image.get_desc().extent,
        }
    }
}
