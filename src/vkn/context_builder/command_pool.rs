use ash::vk;

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
