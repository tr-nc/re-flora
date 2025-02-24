use ash::{vk, Device};

use super::CommandPool;

pub struct CommandBuffer {
    pub command_buffer: vk::CommandBuffer,
}

// no need to manually drop the command buffer
// as it is automatically handled by the command pool

impl CommandBuffer {
    pub fn new(device: &Device, command_pool: &CommandPool) -> Self {
        let command_buffer = create_cmdbuf(device, command_pool.as_raw());
        Self { command_buffer }
    }

    pub fn as_raw(&self) -> vk::CommandBuffer {
        self.command_buffer
    }
}

fn create_cmdbuf(device: &Device, command_pool: vk::CommandPool) -> vk::CommandBuffer {
    let allocate_info = vk::CommandBufferAllocateInfo::default()
        .command_pool(command_pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(1);
    unsafe { device.allocate_command_buffers(&allocate_info).unwrap()[0] }
}
