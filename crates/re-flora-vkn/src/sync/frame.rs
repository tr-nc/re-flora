use crate::{CommandBuffer, CommandPool, Device, Fence, Semaphore};

/// Per-frame synchronization and command recording resources.
///
/// This is intentionally small: it mirrors the existing frame-in-flight model
/// while moving ownership of the frame semaphore/fence/command-buffer bundle
/// into vkn. Higher-level frame acquisition and image-in-flight tracking will be
/// layered on top in later sync-model steps.
pub struct FrameSync {
    image_available: Semaphore,
    fence: Fence,
    command_buffer: CommandBuffer,
}

impl FrameSync {
    pub fn new(device: &Device, command_pool: &CommandPool) -> Self {
        Self {
            image_available: Semaphore::new(device),
            fence: Fence::new(device, true),
            command_buffer: CommandBuffer::new(device, command_pool),
        }
    }

    pub fn create_frames(device: &Device, command_pool: &CommandPool, count: usize) -> Vec<Self> {
        (0..count)
            .map(|_| Self::new(device, command_pool))
            .collect()
    }

    pub fn image_available(&self) -> &Semaphore {
        &self.image_available
    }

    pub fn fence(&self) -> &Fence {
        &self.fence
    }

    pub fn command_buffer(&self) -> &CommandBuffer {
        &self.command_buffer
    }
}
