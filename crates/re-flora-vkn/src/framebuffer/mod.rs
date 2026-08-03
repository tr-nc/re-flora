use crate::{context::VulkanContext, Extent2D, ImageView, RenderPass, Texture};
use anyhow::Result;
use ash::vk;

enum FramebufferAttachments {
    Textures(Vec<Texture>),
    ExternalImageViews { _image_views: Vec<ImageView> },
}

pub struct Framebuffer {
    vulkan_ctx: VulkanContext,
    framebuffer: vk::Framebuffer,
    extent: Extent2D,
    _render_pass: RenderPass,
    attachments: FramebufferAttachments,
}

impl Framebuffer {
    fn new(
        vulkan_ctx: VulkanContext,
        render_pass: &RenderPass,
        attachments: &[vk::ImageView],
        retained_attachments: FramebufferAttachments,
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
                _render_pass: render_pass.clone(),
                attachments: retained_attachments,
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
        let retained_textures = textures.iter().map(|texture| (*texture).clone()).collect();
        Self::new(
            vulkan_ctx,
            render_pass,
            &attachments,
            FramebufferAttachments::Textures(retained_textures),
            extent,
        )
    }

    pub(crate) fn from_external_image_views(
        vulkan_ctx: VulkanContext,
        render_pass: &RenderPass,
        image_views: &[ImageView],
        extent: Extent2D,
    ) -> Result<Self> {
        let attachments = image_views
            .iter()
            .map(ImageView::as_raw)
            .collect::<Vec<_>>();
        Self::new(
            vulkan_ctx,
            render_pass,
            &attachments,
            FramebufferAttachments::ExternalImageViews {
                _image_views: image_views.to_vec(),
            },
            extent,
        )
    }

    pub fn as_raw(&self) -> vk::Framebuffer {
        self.framebuffer
    }

    pub fn get_extent(&self) -> Extent2D {
        self.extent
    }

    pub fn get_attachments(&self) -> &[Texture] {
        match &self.attachments {
            FramebufferAttachments::Textures(textures) => textures,
            FramebufferAttachments::ExternalImageViews { .. } => &[],
        }
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
