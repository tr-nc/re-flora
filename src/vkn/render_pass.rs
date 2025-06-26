use crate::vkn::{CommandBuffer, Device, Framebuffer, Texture};
use ash::vk;
use std::{ops::Deref, sync::Arc};

/// Describes a single attachment in a render pass.
#[derive(Clone, Debug)]
pub struct AttachmentDesc {
    pub format: vk::Format,
    pub samples: vk::SampleCountFlags,
    pub load_op: vk::AttachmentLoadOp,
    pub store_op: vk::AttachmentStoreOp,
    pub stencil_load_op: vk::AttachmentLoadOp,
    pub stencil_store_op: vk::AttachmentStoreOp,
    pub initial_layout: vk::ImageLayout,
    pub final_layout: vk::ImageLayout,
}

/// A reference to an attachment within a subpass, specifying the layout it will be in.
#[derive(Clone, Debug)]
pub struct AttachmentReference {
    /// Index into the `RenderPassDesc`'s `attachments` vector.
    pub attachment: u32,
    pub layout: vk::ImageLayout,
}

/// Describes a single subpass within a render pass.
#[derive(Clone, Debug, Default)]
pub struct SubpassDesc {
    pub color_attachments: Vec<AttachmentReference>,
    pub depth_stencil_attachment: Option<AttachmentReference>,
    // NOTE: Can be extended with input_attachments, resolve_attachments, etc.
}

/// A complete description of a render pass, its attachments, subpasses, and dependencies.
#[derive(Clone, Debug, Default)]
pub struct RenderPassDesc {
    pub attachments: Vec<AttachmentDesc>,
    pub subpasses: Vec<SubpassDesc>,
    pub dependencies: Vec<vk::SubpassDependency>,
}

struct RenderPassInner {
    device: Device,
    desc: RenderPassDesc,
    vk_renderpass: vk::RenderPass,
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
    /// Creates a "stateless" RenderPass from an explicit description.
    /// This offers maximum flexibility for defining attachments, subpasses, and dependencies.
    pub fn from_desc(device: Device, desc: RenderPassDesc) -> Self {
        Self::new(device, desc)
    }

    /// Creates a "stateful" RenderPass that is bound to specific Texture resources.
    /// It derives its format description from the textures. This is a convenience function
    /// for a common pattern of a single subpass with one color and one optional depth attachment.
    pub fn with_attachments(
        device: Device,
        color_texture: Texture,
        depth_texture: Option<Texture>,
        load_op: vk::AttachmentLoadOp,
        color_final_layout: vk::ImageLayout,
        depth_final_layout: Option<vk::ImageLayout>,
    ) -> Self {
        let mut attachments = Vec::new();
        let mut subpass_desc = SubpassDesc::default();
        let mut dst_access_mask = vk::AccessFlags::empty();
        let mut pipeline_stage_mask = vk::PipelineStageFlags::empty();

        // Color Attachment (index 0)
        attachments.push(AttachmentDesc {
            format: color_texture.get_image().get_desc().format,
            samples: vk::SampleCountFlags::TYPE_1,
            load_op,
            store_op: vk::AttachmentStoreOp::STORE,
            stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
            stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
            initial_layout: vk::ImageLayout::UNDEFINED,
            final_layout: color_final_layout,
        });
        subpass_desc.color_attachments.push(AttachmentReference {
            attachment: 0,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        });
        dst_access_mask |= vk::AccessFlags::COLOR_ATTACHMENT_WRITE;
        pipeline_stage_mask |= vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT;

        // Depth Attachment (index 1, if present)
        if let Some(ref depth) = depth_texture {
            attachments.push(AttachmentDesc {
                format: depth.get_image().get_desc().format,
                samples: vk::SampleCountFlags::TYPE_1,
                load_op: vk::AttachmentLoadOp::CLEAR,
                store_op: vk::AttachmentStoreOp::STORE,
                stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
                stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
                initial_layout: vk::ImageLayout::UNDEFINED,
                final_layout: depth_final_layout
                    .expect("Depth final layout must be provided when depth texture is present"),
            });
            subpass_desc.depth_stencil_attachment = Some(AttachmentReference {
                attachment: attachments.len() as u32 - 1,
                layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
            });
            dst_access_mask |= vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE;
            pipeline_stage_mask |= vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS;
        }

        let subpasses = vec![subpass_desc];

        // Dependency to transition layouts from external to our subpass
        let dependencies = vec![vk::SubpassDependency::default()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(pipeline_stage_mask)
            .dst_stage_mask(pipeline_stage_mask)
            .src_access_mask(vk::AccessFlags::empty())
            .dst_access_mask(dst_access_mask)];

        let desc = RenderPassDesc {
            attachments,
            subpasses,
            dependencies,
        };

        Self::new(device, desc)
    }

    fn new(device: Device, desc: RenderPassDesc) -> Self {
        let attachments: Vec<vk::AttachmentDescription> = desc
            .attachments
            .iter()
            .map(|att| {
                vk::AttachmentDescription::default()
                    .format(att.format)
                    .samples(att.samples)
                    .load_op(att.load_op)
                    .store_op(att.store_op)
                    .stencil_load_op(att.stencil_load_op)
                    .stencil_store_op(att.stencil_store_op)
                    .initial_layout(att.initial_layout)
                    .final_layout(att.final_layout)
            })
            .collect();

        // These collections hold the attachment references for the duration of the subpass creation.
        let subpass_color_refs: Vec<Vec<vk::AttachmentReference>> = desc
            .subpasses
            .iter()
            .map(|subpass| {
                subpass
                    .color_attachments
                    .iter()
                    .map(|r| {
                        vk::AttachmentReference::default()
                            .attachment(r.attachment)
                            .layout(r.layout)
                    })
                    .collect()
            })
            .collect();

        let subpass_depth_refs: Vec<Option<vk::AttachmentReference>> = desc
            .subpasses
            .iter()
            .map(|subpass| {
                subpass.depth_stencil_attachment.as_ref().map(|r| {
                    vk::AttachmentReference::default()
                        .attachment(r.attachment)
                        .layout(r.layout)
                })
            })
            .collect();

        let subpasses: Vec<vk::SubpassDescription> = desc
            .subpasses
            .iter()
            .enumerate()
            .map(|(i, _subpass_desc)| {
                let mut subpass_description = vk::SubpassDescription::default()
                    .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
                    .color_attachments(&subpass_color_refs[i]);

                if let Some(depth_ref) = &subpass_depth_refs[i] {
                    subpass_description = subpass_description.depth_stencil_attachment(depth_ref);
                }

                subpass_description
            })
            .collect();

        let render_pass_info = vk::RenderPassCreateInfo::default()
            .attachments(&attachments)
            .subpasses(&subpasses)
            .dependencies(&desc.dependencies);

        let vk_renderpass = unsafe { device.create_render_pass(&render_pass_info, None).unwrap() };

        let inner = RenderPassInner {
            device,
            desc,
            vk_renderpass,
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

    /// Ends the render pass. The caller is responsible for transitioning image layouts
    /// to their final state as specified in the `RenderPassDesc`.
    pub fn record_end(&self, cmdbuf: &CommandBuffer) {
        unsafe {
            self.0.device.cmd_end_render_pass(cmdbuf.as_raw());
        }
    }

    pub fn get_desc(&self) -> &RenderPassDesc {
        &self.0.desc
    }
}
