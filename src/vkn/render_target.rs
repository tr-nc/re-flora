use crate::vkn::{CommandBuffer, Framebuffer, RenderPass, RenderPassDesc};
use ash::vk;

/// A render target that combines a RenderPass with multiple Framebuffers for flexible rendering operations.
/// This abstraction follows the common Vulkan pattern of one RenderPass with multiple Framebuffers,
/// supporting use cases like multi-buffering, multi-target rendering, and swapchain-style operations.
pub struct RenderTarget {
    render_pass: RenderPass,
    framebuffers: Vec<Framebuffer>,
    current_framebuffer_index: usize,
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
        self.render_pass
            .record_begin(cmdbuf, &self.framebuffers[framebuffer_index], clear_values);
    }

    /// Begins the render pass with the current framebuffer.
    /// This maintains backward compatibility with the original API.
    pub fn record_begin(&self, cmdbuf: &CommandBuffer, clear_values: &[vk::ClearValue]) {
        self.record_begin_with_index(cmdbuf, self.current_framebuffer_index, clear_values);
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
}
