use std::ops::Deref;
use ash::vk;

#[derive(Debug, Clone, Copy)]
pub struct Queue {
    queue: vk::Queue,
}

impl Deref for Queue {
    type Target = vk::Queue;
    fn deref(&self) -> &Self::Target {
        &self.queue
    }
}

impl Queue {
    pub fn new(queue: vk::Queue) -> Self {
        Self { queue }
    }

    pub fn as_raw(&self) -> vk::Queue {
        self.queue
    }
}
