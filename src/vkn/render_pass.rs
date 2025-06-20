use crate::vkn::Device;
use anyhow::Result;
use ash::vk;

pub struct RenderPass {
    device: Device,
    pub inner: vk::RenderPass,
}

impl RenderPass {
    pub fn new(device: Device, format: vk::Format, sample_count: vk::SampleCountFlags) -> Self {
        let attachment = [vk::AttachmentDescription::default()
            .format(format)
            .samples(sample_count)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)];

        let color_attachment_ref = [vk::AttachmentReference::default()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];

        let subpass = [vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_attachment_ref)];

        let dependency = [vk::SubpassDependency::default()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags::empty())
            .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)];

        let render_pass_info = vk::RenderPassCreateInfo::default()
            .attachments(&attachment)
            .subpasses(&subpass)
            .dependencies(&dependency);

        let inner = unsafe { device.create_render_pass(&render_pass_info, None).unwrap() };

        Self { device, inner }
    }

    pub fn create_framebuffer(
        &self,
        image_view: vk::ImageView,
        extent: vk::Extent2D,
    ) -> Result<vk::Framebuffer> {
        let attachments = [image_view];
        let framebuffer_info = vk::FramebufferCreateInfo::default()
            .render_pass(self.inner)
            .attachments(&attachments)
            .width(extent.width)
            .height(extent.height)
            .layers(1);

        unsafe {
            self.device
                .create_framebuffer(&framebuffer_info, None)
                .map_err(|e| anyhow::anyhow!("Failed to create framebuffer: {}", e))
        }
    }
}

impl Drop for RenderPass {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_render_pass(self.inner, None);
        }
    }
}
