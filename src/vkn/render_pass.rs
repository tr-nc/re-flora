use crate::vkn::{CommandBuffer, Device, Framebuffer, Texture};
use ash::vk;
use std::{ops::Deref, sync::Arc};

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
    pub depth_format: Option<vk::Format>,
    pub depth_final_layout: Option<vk::ImageLayout>,
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
            depth_format: None,
            depth_final_layout: None,
        }
    }
}

struct RenderPassInner {
    device: Device,
    desc: RenderPassDesc,
    vk_renderpass: vk::RenderPass,
    color_attachment: Option<Texture>,
    depth_attachment: Option<Texture>,
}

impl Drop for RenderPassInner {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_render_pass(self.vk_renderpass, None);
        }
    }
}

#[derive(Clone)]
pub struct RenderPass(Arc<RenderPassInner>);

impl Deref for RenderPass {
    type Target = vk::RenderPass;

    fn deref(&self) -> &Self::Target {
        &self.0.vk_renderpass
    }
}

impl RenderPass {
    /// Creates a "stateless" RenderPass from explicit format descriptions.
    /// Ideal for swapchains where the target ImageView changes each frame.
    pub fn from_formats(device: Device, desc: RenderPassDesc) -> Self {
        Self::new_internal(device, desc, None, None)
    }

    /// Creates a "stateful" RenderPass that is bound to specific Texture resources.
    /// It derives its format description from the textures and will manage their
    /// image layouts automatically.
    pub fn with_attachments(
        device: Device,
        color_attachment: Texture,
        depth_attachment: Option<Texture>,
        load_op: vk::AttachmentLoadOp,
        final_layout: vk::ImageLayout,
    ) -> Self {
        let mut desc = RenderPassDesc {
            format: color_attachment.get_image().get_desc().format,
            load_op,
            final_layout,
            ..Default::default()
        };

        if let Some(ref depth) = depth_attachment {
            desc.depth_format = Some(depth.get_image().get_desc().format);
            desc.depth_final_layout = Some(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);
            desc.dst_access_mask |= vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE;
        }

        Self::new_internal(device, desc, Some(color_attachment), depth_attachment)
    }

    fn new_internal(
        device: Device,
        desc: RenderPassDesc,
        color_attachment: Option<Texture>,
        depth_attachment: Option<Texture>,
    ) -> Self {
        let mut attachments = vec![];
        let mut subpass_description =
            vk::SubpassDescription::default().pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS);

        let color_attachment_desc = vk::AttachmentDescription::default()
            .format(desc.format)
            .samples(desc.sample_count)
            .load_op(desc.load_op)
            .store_op(desc.store_op)
            .stencil_load_op(desc.stencil_load_op)
            .stencil_store_op(desc.stencil_store_op)
            .initial_layout(desc.initial_layout)
            .final_layout(desc.final_layout);
        attachments.push(color_attachment_desc);

        let color_attachment_ref = vk::AttachmentReference::default()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
        subpass_description =
            subpass_description.color_attachments(std::slice::from_ref(&color_attachment_ref));

        let depth_attachment_ref_storage;
        if let Some(depth_format) = desc.depth_format {
            let depth_attachment_desc = vk::AttachmentDescription::default()
                .format(depth_format)
                .samples(vk::SampleCountFlags::TYPE_1)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .final_layout(
                    desc.depth_final_layout
                        .unwrap_or(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL),
                );
            attachments.push(depth_attachment_desc);

            depth_attachment_ref_storage = vk::AttachmentReference::default()
                .attachment(attachments.len() as u32 - 1)
                .layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);
            subpass_description =
                subpass_description.depth_stencil_attachment(&depth_attachment_ref_storage);
        }

        let subpass = [subpass_description];
        let dependency = [vk::SubpassDependency::default()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                    | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
            )
            .src_access_mask(vk::AccessFlags::empty())
            .dst_stage_mask(
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                    | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
            )
            .dst_access_mask(desc.dst_access_mask)];

        let render_pass_info = vk::RenderPassCreateInfo::default()
            .attachments(&attachments)
            .subpasses(&subpass)
            .dependencies(&dependency);

        let vk_renderpass = unsafe { device.create_render_pass(&render_pass_info, None).unwrap() };

        let inner = RenderPassInner {
            device,
            desc,
            vk_renderpass,
            color_attachment,
            depth_attachment,
        };
        Self(Arc::new(inner))
    }

    pub fn as_raw(&self) -> vk::RenderPass {
        self.0.vk_renderpass
    }

    pub fn record_begin(
        &self,
        cmdbuf: &CommandBuffer,
        framebuffer: &Framebuffer,
        clear_values: &[vk::ClearValue],
    ) {
        let render_pass_begin_info = vk::RenderPassBeginInfo::default()
            .render_pass(self.as_raw())
            .framebuffer(framebuffer.as_raw())
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: framebuffer.get_extent(),
            })
            .clear_values(&clear_values);

        unsafe {
            self.0.device.cmd_begin_render_pass(
                cmdbuf.as_raw(),
                &render_pass_begin_info,
                vk::SubpassContents::INLINE,
            );
        }
    }

    /// Ends the render pass and, if bound to textures, updates their internal layout state.
    pub fn record_end(&self, cmdbuf: &CommandBuffer) {
        unsafe {
            self.0.device.cmd_end_render_pass(cmdbuf.as_raw());
        }

        if let Some(color) = &self.0.color_attachment {
            color.get_image().set_layout(0, self.0.desc.final_layout);
        }
        if let Some(depth) = &self.0.depth_attachment {
            if let Some(layout) = self.0.desc.depth_final_layout {
                depth.get_image().set_layout(0, layout);
            }
        }
    }
}
