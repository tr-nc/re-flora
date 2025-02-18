use ash::vk;
use egui::TextureId;
use thiserror::Error;

/// Crates error type.
#[derive(Debug, Error)]
pub enum RendererError {
    /// Errors coming from calls to Vulkan functions.
    #[error("A Vulkan error occured: {0}")]
    Vulkan(#[from] vk::Result),

    /// Io errors.
    #[error("A io error occured: {0}")]
    Io(#[from] std::io::Error),

    /// Initialization errors.
    #[error("An error occured when initializing the renderer: {0}")]
    Init(String),

    /// Texture lookup error.
    #[error("Bad texture ID: {0:?}")]
    BadTexture(TextureId),

    /// Allocator error
    #[error("A error occured when using the allocator: {0}")]
    Allocator(String),
}
