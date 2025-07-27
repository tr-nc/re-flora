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

    /// Sets the current framebuffer index for convenience methods.
    pub fn set_current_framebuffer(&mut self, index: usize) {
        assert!(
            index < self.framebuffers.len(),
            "Framebuffer index {} out of bounds (max: {})",
            index,
            self.framebuffers.len() - 1
        );
        self.current_framebuffer_index = index;
    }

    /// Gets the current framebuffer index.
    pub fn get_current_framebuffer_index(&self) -> usize {
        self.current_framebuffer_index
    }

    /// Gets the number of framebuffers in this render target.
    pub fn get_framebuffer_count(&self) -> usize {
        self.framebuffers.len()
    }

    /// Gets a reference to a specific framebuffer by index.
    pub fn get_framebuffer_by_index(&self, index: usize) -> &Framebuffer {
        assert!(
            index < self.framebuffers.len(),
            "Framebuffer index {} out of bounds (max: {})",
            index,
            self.framebuffers.len() - 1
        );
        &self.framebuffers[index]
    }

    /// Gets a reference to the current framebuffer (backward compatibility).
    pub fn get_framebuffer(&self) -> &Framebuffer {
        &self.framebuffers[self.current_framebuffer_index]
    }

    /// Gets a reference to the current framebuffer.
    pub fn get_current_framebuffer(&self) -> &Framebuffer {
        &self.framebuffers[self.current_framebuffer_index]
    }

    /// Gets all framebuffers as a slice.
    pub fn get_framebuffers(&self) -> &[Framebuffer] {
        &self.framebuffers
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
