use ash::vk;

use super::Device;

pub struct CommandPool {
    device: ash::Device,
    pub command_pool: vk::CommandPool,
}

impl Drop for CommandPool {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_command_pool(self.command_pool, None);
        }
    }
}

impl CommandPool {
    pub fn new(device: &Device, queue_family_index: u32) -> Self {
        let command_pool = create_command_pool(device, queue_family_index);
        Self {
            device: device.as_raw().clone(),
            command_pool,
        }
    }

    pub fn as_raw(&self) -> vk::CommandPool {
        self.command_pool
    }
}

pub fn create_command_pool(device: &ash::Device, queue_family_index: u32) -> vk::CommandPool {
    let command_pool_info = vk::CommandPoolCreateInfo::default()
        .queue_family_index(queue_family_index)
        .flags(vk::CommandPoolCreateFlags::empty());
    unsafe {
        device
            .create_command_pool(&command_pool_info, None)
            .expect("Failed to create command pool")
    }
}
