use crate::{context::VulkanContext, Extent2D, RenderPass, Texture};
use anyhow::Result;
use ash::vk;

pub struct Framebuffer {
    vulkan_ctx: VulkanContext,
    framebuffer: vk::Framebuffer,
    extent: Extent2D,
    attachments: Vec<Texture>,
}

impl Framebuffer {
    pub fn new(
        vulkan_ctx: VulkanContext,
        render_pass: &RenderPass,
        attachments: &[vk::ImageView],
        extent: Extent2D,
    ) -> Result<Self> {
        let framebuffer_info = vk::FramebufferCreateInfo::default()
            .render_pass(render_pass.as_raw())
            .attachments(attachments)
            .width(extent.width)
            .height(extent.height)
            .layers(1);

        unsafe {
            let framebuffer = vulkan_ctx
                .device()
                .create_framebuffer(&framebuffer_info, None)
                .map_err(|e| anyhow::anyhow!("Failed to create framebuffer: {}", e))?;

            Ok(Self {
                vulkan_ctx,
                framebuffer,
                extent,
                attachments: Vec::new(),
            })
        }
    }

    pub fn from_textures(
        vulkan_ctx: VulkanContext,
        render_pass: &RenderPass,
        textures: &[&Texture],
        extent: Extent2D,
    ) -> Result<Self> {
        let attachments = textures
            .iter()
            .map(|texture| texture.get_image_view().as_raw())
            .collect::<Vec<_>>();
        let mut framebuffer = Self::new(vulkan_ctx, render_pass, &attachments, extent)?;
        framebuffer.attachments = textures.iter().map(|texture| (*texture).clone()).collect();
        Ok(framebuffer)
    }

    pub fn as_raw(&self) -> vk::Framebuffer {
        self.framebuffer
    }

    pub fn get_extent(&self) -> Extent2D {
        self.extent
    }

    pub fn get_attachments(&self) -> &[Texture] {
        &self.attachments
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
