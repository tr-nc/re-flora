use crate::vkn::{CommandBuffer, Device, Framebuffer};
use ash::vk;

#[derive(Clone, Debug)]
pub struct RenderPassDesc {
    pub format: vk::Format,
    pub sample_count: vk::SampleCountFlags,
    pub load_op: vk::AttachmentLoadOp,
    pub store_op: vk::AttachmentStoreOp,
    pub stencil_load_op: vk::AttachmentLoadOp,
    pub stencil_store_op: vk::AttachmentStoreOp,
    pub initial_layout: vk::ImageLayout,
    pub final_layout: vk::ImageLayout,
    pub dst_access_mask: vk::AccessFlags,
}

impl Default for RenderPassDesc {
    fn default() -> Self {
        Self {
            format: vk::Format::UNDEFINED,
            sample_count: vk::SampleCountFlags::TYPE_1,
            load_op: vk::AttachmentLoadOp::CLEAR,
            store_op: vk::AttachmentStoreOp::STORE,
            stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
            stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
            initial_layout: vk::ImageLayout::UNDEFINED,
            final_layout: vk::ImageLayout::PRESENT_SRC_KHR,
            dst_access_mask: vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
        }
    }
}

pub struct RenderPass {
    device: Device,
    vk_renderpass: vk::RenderPass,
}

impl RenderPass {
    pub fn new(device: Device, desc: &RenderPassDesc) -> Self {
        let attachment = [vk::AttachmentDescription::default()
            .format(desc.format)
            .samples(desc.sample_count)
            .load_op(desc.load_op)
            .store_op(desc.store_op)
            .stencil_load_op(desc.stencil_load_op)
            .stencil_store_op(desc.stencil_store_op)
            .initial_layout(desc.initial_layout)
            .final_layout(desc.final_layout)];

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
            .dst_access_mask(desc.dst_access_mask)];

        let render_pass_info = vk::RenderPassCreateInfo::default()
            .attachments(&attachment)
            .subpasses(&subpass)
            .dependencies(&dependency);

        let vk_renderpass = unsafe { device.create_render_pass(&render_pass_info, None).unwrap() };

        Self {
            device,
            vk_renderpass,
        }
    }

    pub fn as_raw(&self) -> vk::RenderPass {
        self.vk_renderpass
    }

    pub fn record_begin(
        &self,
        cmdbuf: &CommandBuffer,
        framebuffer: &Framebuffer,
        clear_color: &[f32; 4],
    ) {
        let clear_values = [vk::ClearValue {
            color: vk::ClearColorValue {
                float32: *clear_color,
            },
        }];

        let render_pass_begin_info = vk::RenderPassBeginInfo::default()
            .render_pass(self.as_raw())
            .framebuffer(framebuffer.as_raw())
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: framebuffer.get_extent(),
            })
            .clear_values(&clear_values);

        unsafe {
            self.device.cmd_begin_render_pass(
                cmdbuf.as_raw(),
                &render_pass_begin_info,
                vk::SubpassContents::INLINE,
            );
        }
    }

    pub fn record_end(&self, cmdbuf: &CommandBuffer) {
        unsafe {
            self.device.cmd_end_render_pass(cmdbuf.as_raw());
        }
    }
}

impl Drop for RenderPass {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_render_pass(self.vk_renderpass, None);
        }
    }
}
