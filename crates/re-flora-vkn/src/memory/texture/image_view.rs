use ash::vk;
use std::fmt;
use std::sync::Arc;

use crate::{Device, Image};

/// A Vulkan image whose storage and lifetime are owned outside `re-flora-vkn`.
///
/// This type is intentionally crate-private and can only be constructed through
/// a named external-owner boundary. In particular, dropping it never destroys
/// the Vulkan image.
#[derive(Copy, Clone, Debug)]
pub(crate) struct ExternalImage(vk::Image);

impl ExternalImage {
    pub(crate) fn from_swapchain(image: vk::Image) -> Self {
        Self(image)
    }

    fn as_raw(self) -> vk::Image {
        self.0
    }
}

#[derive(Copy, Clone, Debug)]
pub struct ImageViewDesc {
    pub format: vk::Format,
    pub image_view_type: vk::ImageViewType,
    pub aspect: vk::ImageAspectFlags,
    pub base_array_layer: u32,
    pub layer_count: u32,
}

impl Default for ImageViewDesc {
    fn default() -> Self {
        Self {
            format: vk::Format::UNDEFINED,
            image_view_type: vk::ImageViewType::TYPE_2D,
            aspect: vk::ImageAspectFlags::COLOR,
            base_array_layer: 0,
            layer_count: 1,
        }
    }
}

#[derive(Clone, Debug)]
enum ImageViewSource {
    Owned(Image),
    External(ExternalImage),
}

impl ImageViewSource {
    fn as_raw(&self) -> vk::Image {
        match self {
            Self::Owned(image) => image.as_raw(),
            Self::External(image) => image.as_raw(),
        }
    }

    fn ownership_name(&self) -> &'static str {
        match self {
            Self::Owned(_) => "owned",
            Self::External(_) => "external",
        }
    }
}

struct ImageViewInner {
    device: Device,
    image_view: vk::ImageView,
    source: ImageViewSource,
}

impl Drop for ImageViewInner {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_image_view(self.image_view, None);
        }
    }
}

#[derive(Clone)]
pub struct ImageView(Arc<ImageViewInner>);

impl std::ops::Deref for ImageView {
    type Target = vk::ImageView;
    fn deref(&self) -> &Self::Target {
        &self.0.image_view
    }
}

impl ImageView {
    /// Creates a view that retains the owned image for the view's full lifetime.
    pub fn new_owned(device: Device, image: &Image, desc: ImageViewDesc) -> Self {
        Self::new(device, ImageViewSource::Owned(image.clone()), desc)
    }

    /// Creates a view of a swapchain-owned image.
    ///
    /// The surrounding swapchain resource graph must destroy this view before
    /// destroying the swapchain that owns the image.
    pub(crate) fn new_external(device: Device, image: ExternalImage, desc: ImageViewDesc) -> Self {
        Self::new(device, ImageViewSource::External(image), desc)
    }

    fn new(device: Device, source: ImageViewSource, desc: ImageViewDesc) -> Self {
        let create_info = vk::ImageViewCreateInfo::default()
            .image(source.as_raw())
            .view_type(desc.image_view_type)
            .format(desc.format)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: desc.aspect,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: desc.base_array_layer,
                layer_count: desc.layer_count,
            });

        let image_view = unsafe { device.create_image_view(&create_info, None).unwrap() };

        Self(Arc::new(ImageViewInner {
            device,
            image_view,
            source,
        }))
    }

    pub fn as_raw(&self) -> vk::ImageView {
        self.0.image_view
    }

    pub(crate) fn source_image_raw(&self) -> vk::Image {
        self.0.source.as_raw()
    }
}

impl fmt::Debug for ImageView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImageView")
            .field("image_view", &self.0.image_view)
            .field("source_ownership", &self.0.source.ownership_name())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ash::vk::Handle;

    #[test]
    fn swapchain_image_is_explicitly_external() {
        let raw = vk::Image::from_raw(42);
        let image = ExternalImage::from_swapchain(raw);

        assert_eq!(image.as_raw(), raw);
    }
}
