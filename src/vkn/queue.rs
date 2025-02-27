use std::ops::Deref;

#[derive(Debug, Clone, Copy)]
pub struct QueueFamilyIndices {
    /// Guaranteed to support GRAPHICS + PRESENT + COMPUTE + TRANSFER,
    /// and should be used for all main tasks
    pub general: u32,
    /// Exclusive to transfer operations, may be slower, but enables
    /// potential parallelism for background transfer operations
    pub transfer_only: u32,
}

impl QueueFamilyIndices {
    pub fn get_all_indices(&self) -> Vec<u32> {
        vec![self.general, self.transfer_only]
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Queue {
    queue: ash::vk::Queue,
}

impl Deref for Queue {
    type Target = ash::vk::Queue;
    fn deref(&self) -> &Self::Target {
        &self.queue
    }
}

impl Queue {
    pub fn new(queue: ash::vk::Queue) -> Self {
        Self { queue }
    }

    pub fn as_raw(&self) -> ash::vk::Queue {
        self.queue
    }
}
