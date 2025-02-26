use super::{Allocator, Device};
use ash::vk;
use std::ops::Deref;

pub struct Buffer {
    device: ash::Device,
    allocator: Allocator,
    buffer: vk::Buffer,
    memory: gpu_allocator::vulkan::Allocation,
}

impl Drop for Buffer {
    fn drop(&mut self) {
        let allocation = std::mem::take(&mut self.memory);
        self.allocator
            .destroy_buffer(&self.device, self.buffer, allocation);
    }
}

impl Deref for Buffer {
    type Target = vk::Buffer;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl Buffer {
    pub fn new_sized(
        device: &Device,
        allocator: &mut Allocator,
        usage: vk::BufferUsageFlags,
        buffer_size: usize,
    ) -> Self {
        let (buffer, memory) = allocator.create_buffer(device, buffer_size, usage);
        Self {
            device: device.as_raw().clone(),
            allocator: allocator.clone(),
            buffer,
            memory,
        }
    }

    pub fn fill<T: Copy>(&mut self, data: &[T]) {
        self.allocator
            .update_buffer(&self.device, &mut self.memory, data);
    }

    pub fn as_raw(&self) -> vk::Buffer {
        self.buffer
    }
}
