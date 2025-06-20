use crate::vkn::{context::VulkanContext, RenderPass};
use anyhow::Result;
use ash::vk;

pub struct Framebuffer {
    vulkan_ctx: VulkanContext,
    framebuffer: vk::Framebuffer,
    image_view: vk::ImageView,
    extent: vk::Extent2D,
}

impl Framebuffer {
    pub fn new(
        vulkan_ctx: VulkanContext,
        render_pass: &RenderPass,
        image_view: vk::ImageView,
        extent: vk::Extent2D,
    ) -> Result<Self> {
        let attachments = [image_view];
        let framebuffer_info = vk::FramebufferCreateInfo::default()
            .render_pass(render_pass.as_raw())
            .attachments(&attachments)
            .width(extent.width)
            .height(extent.height)
            .layers(1);

        unsafe {
            let framebuffer = vulkan_ctx
                .device()
                .create_framebuffer(&framebuffer_info, None)
                .map_err(|e| anyhow::anyhow!("Failed to create framebuffer: {}", e))?;

            return Ok(Self {
                vulkan_ctx,
                framebuffer,
                image_view,
                extent,
            });
        }
    }

    pub fn as_raw(&self) -> vk::Framebuffer {
        self.framebuffer
    }

    pub fn get_image_view(&self) -> vk::ImageView {
        self.image_view
    }

    pub fn get_extent(&self) -> vk::Extent2D {
        self.extent
    }
}

impl Drop for Framebuffer {
    fn drop(&mut self) {
        unsafe {
            self.vulkan_ctx
                .device()
                .destroy_framebuffer(self.framebuffer, None);
        }
    }
}
