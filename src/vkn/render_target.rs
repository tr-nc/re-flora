use crate::vkn::{CommandBuffer, Framebuffer, RenderPass, RenderPassDesc};
use ash::vk;

/// A render target that combines a RenderPass and Framebuffer for simplified rendering operations.
/// This abstraction provides a cleaner API by encapsulating the render pass/framebuffer relationship.
pub struct RenderTarget {
    render_pass: RenderPass,
    framebuffer: Framebuffer,
}

impl RenderTarget {
    /// Creates a new RenderTarget with the given render pass and framebuffer.
    pub fn new(render_pass: RenderPass, framebuffer: Framebuffer) -> Self {
        Self {
            render_pass,
            framebuffer,
        }
    }

    /// Begins the render pass with the associated framebuffer.
    /// This is a convenience method that combines render pass and framebuffer operations.
    pub fn record_begin(&self, cmdbuf: &CommandBuffer, clear_values: &[vk::ClearValue]) {
        self.render_pass
            .record_begin(cmdbuf, &self.framebuffer, clear_values);
    }

    /// Ends the render pass.
    pub fn record_end(&self, cmdbuf: &CommandBuffer) {
        self.render_pass.record_end(cmdbuf);
    }

    /// Gets the render pass description.
    pub fn get_desc(&self) -> &RenderPassDesc {
        self.render_pass.get_desc()
    }

    /// Gets a reference to the underlying render pass.
    pub fn get_render_pass(&self) -> &RenderPass {
        &self.render_pass
    }

    /// Gets a reference to the underlying framebuffer.
    pub fn get_framebuffer(&self) -> &Framebuffer {
        &self.framebuffer
    }
}
