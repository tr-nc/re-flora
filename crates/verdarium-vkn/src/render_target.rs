use crate::{CommandBuffer, Framebuffer, RenderPass, RenderPassDesc};
use ash::vk;

#[derive(Clone, Copy, Debug)]
struct ActiveFramebuffer {
    index: usize,
}

/// A render target that combines a RenderPass with multiple Framebuffers for flexible rendering operations.
/// This abstraction follows the common Vulkan pattern of one RenderPass with multiple Framebuffers,
/// supporting use cases like multi-buffering, multi-target rendering, and swapchain-style operations.
pub struct RenderTarget {
    render_pass: RenderPass,
    framebuffers: Vec<Framebuffer>,
    current_framebuffer_index: usize,
    active_framebuffer: std::sync::Mutex<Option<ActiveFramebuffer>>,
}

impl RenderTarget {
    /// Creates a new RenderTarget with framebuffers.
    /// For single framebuffer use cases, pass a vector with one element: vec![framebuffer].
    pub fn new(render_pass: RenderPass, framebuffers: Vec<Framebuffer>) -> Self {
        assert!(
            !framebuffers.is_empty(),
            "RenderTarget must have at least one framebuffer"
        );
        Self {
            render_pass,
            framebuffers,
            current_framebuffer_index: 0,
            active_framebuffer: std::sync::Mutex::new(None),
        }
    }

    /// Begins the render pass with a specific framebuffer by index.
    /// This provides full control over which framebuffer to use.
    pub fn record_begin_with_index(
        &self,
        cmdbuf: &CommandBuffer,
        framebuffer_index: usize,
        clear_values: &[vk::ClearValue],
    ) {
        assert!(
            framebuffer_index < self.framebuffers.len(),
            "Framebuffer index {} out of bounds (max: {})",
            framebuffer_index,
            self.framebuffers.len() - 1
        );
        self.track_attachment_initial_layouts(framebuffer_index);
        self.render_pass
            .record_begin(cmdbuf, &self.framebuffers[framebuffer_index], clear_values);
        *self.active_framebuffer.lock().unwrap() = Some(ActiveFramebuffer {
            index: framebuffer_index,
        });
    }

    /// Begins the render pass with the current framebuffer.
    /// This maintains backward compatibility with the original API.
    pub fn record_begin(&self, cmdbuf: &CommandBuffer, clear_values: &[vk::ClearValue]) {
        self.record_begin_with_index(cmdbuf, self.current_framebuffer_index, clear_values);
    }

    /// Ends the render pass.
    pub fn record_end(&self, cmdbuf: &CommandBuffer) {
        self.render_pass.record_end(cmdbuf);
        if let Some(active_framebuffer) = self.active_framebuffer.lock().unwrap().take() {
            self.track_attachment_final_layouts(active_framebuffer.index);
        }
    }

    /// Gets the render pass description.
    pub fn get_desc(&self) -> &RenderPassDesc {
        self.render_pass.get_desc()
    }

    /// Gets a reference to the underlying render pass.
    pub fn get_render_pass(&self) -> &RenderPass {
        &self.render_pass
    }

    fn track_attachment_initial_layouts(&self, framebuffer_index: usize) {
        let framebuffer = &self.framebuffers[framebuffer_index];
        for (attachment_index, texture) in framebuffer.get_attachments().iter().enumerate() {
            if let Some(desc) = self.render_pass.get_desc().attachments.get(attachment_index) {
                texture.get_image().set_layout(0, desc.initial_layout);
            }
        }
    }

    fn track_attachment_final_layouts(&self, framebuffer_index: usize) {
        let framebuffer = &self.framebuffers[framebuffer_index];
        for (attachment_index, texture) in framebuffer.get_attachments().iter().enumerate() {
            if let Some(desc) = self.render_pass.get_desc().attachments.get(attachment_index) {
                texture.get_image().set_layout(0, desc.final_layout);
            }
        }
    }
}
