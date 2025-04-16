use ash::vk;
use std::sync::Arc;

use crate::vkn::Device;

#[derive(Copy, Clone)]
pub struct ImageViewDesc {
    pub image: vk::Image,
    pub format: vk::Format,
    pub image_view_type: vk::ImageViewType,
    pub aspect: vk::ImageAspectFlags,
}

impl Default for ImageViewDesc {
    fn default() -> Self {
        Self {
            image: vk::Image::default(),
            format: vk::Format::UNDEFINED,
            image_view_type: vk::ImageViewType::TYPE_2D,
            aspect: vk::ImageAspectFlags::COLOR,
        }
    }
}

struct ImageViewInner {
    device: Device,
    image_view: vk::ImageView,
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
    pub fn new(device: Device, desc: ImageViewDesc) -> Self {
        let create_info = vk::ImageViewCreateInfo::default()
            .image(desc.image)
            .view_type(desc.image_view_type)
            .format(desc.format)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: desc.aspect,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        let image_view = unsafe { device.create_image_view(&create_info, None).unwrap() };

        Self(Arc::new(ImageViewInner {
            device: device,
            image_view,
        }))
    }

    pub fn as_raw(&self) -> vk::ImageView {
        self.0.image_view
    }
}
