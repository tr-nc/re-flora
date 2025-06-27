use ash::vk;

use crate::vkn::Extent2D;

#[derive(Copy, Clone, Debug)]
pub struct Viewport {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub min_depth: f32,
    pub max_depth: f32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            min_depth: 0.0,
            max_depth: 1.0,
        }
    }
}

impl Viewport {
    pub fn from_extent(extent: Extent2D) -> Self {
        Self {
            width: extent.width as f32,
            height: extent.height as f32,
            ..Default::default()
        }
    }

    pub fn as_raw(&self) -> vk::Viewport {
        vk::Viewport {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
            min_depth: self.min_depth,
            max_depth: self.max_depth,
        }
    }
}
