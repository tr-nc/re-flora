use ash::vk;
use std::sync::Arc;
use crate::vkn::Device;

struct CommandPoolInner {
    device: Device,
    command_pool: vk::CommandPool,
}

impl Drop for CommandPoolInner {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_command_pool(self.command_pool, None);
        }
    }
}

#[derive(Clone)]
pub struct CommandPool(Arc<CommandPoolInner>);

impl std::ops::Deref for CommandPool {
    type Target = vk::CommandPool;
    fn deref(&self) -> &Self::Target {
        &self.0.command_pool
    }
}

impl CommandPool {
    pub fn new(device: &Device, queue_family_index: u32) -> Self {
        let command_pool = create_command_pool(device, queue_family_index);
        Self(Arc::new(CommandPoolInner {
            device: device.clone(),
            command_pool,
        }))
    }

    pub fn as_raw(&self) -> vk::CommandPool {
        self.0.command_pool
    }
}

pub fn create_command_pool(device: &Device, queue_family_index: u32) -> vk::CommandPool {
    let command_pool_info = vk::CommandPoolCreateInfo::default()
        .queue_family_index(queue_family_index)
        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
    unsafe {
        device
            .create_command_pool(&command_pool_info, None)
            .expect("Failed to create command pool")
    }
}
