use anyhow::Result;
use ash::vk;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Extent2D {
    pub width: u32,
    pub height: u32,
}

impl Extent2D {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    pub fn as_raw(&self) -> vk::Extent2D {
        vk::Extent2D {
            width: self.width,
            height: self.height,
        }
    }
}

impl Default for Extent2D {
    fn default() -> Self {
        Self {
            width: 0,
            height: 0,
        }
    }
}

impl From<vk::Extent2D> for Extent2D {
    fn from(extent: vk::Extent2D) -> Self {
        Self {
            width: extent.width,
            height: extent.height,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Extent3D {
    pub width: u32,
    pub height: u32,
    pub depth: u32,
}

impl Extent3D {
    pub fn new(width: u32, height: u32, depth: u32) -> Self {
        Self {
            width,
            height,
            depth,
        }
    }

    pub fn as_extent_2d(&self) -> Result<Extent2D> {
        if self.depth != 1 {
            return Err(anyhow::anyhow!("Extent is not 2D"));
        }
        Ok(Extent2D {
            width: self.width,
            height: self.height,
        })
    }

    pub fn as_raw(&self) -> vk::Extent3D {
        vk::Extent3D {
            width: self.width,
            height: self.height,
            depth: self.depth,
        }
    }
}

impl From<vk::Extent3D> for Extent3D {
    fn from(extent: vk::Extent3D) -> Self {
        Self {
            width: extent.width,
            height: extent.height,
            depth: extent.depth,
        }
    }
}

impl From<Extent2D> for Extent3D {
    fn from(extent: Extent2D) -> Self {
        Self {
            width: extent.width,
            height: extent.height,
            depth: 1,
        }
    }
}

impl Default for Extent3D {
    fn default() -> Self {
        Self {
            width: 0,
            height: 0,
            depth: 0,
        }
    }
}
