use ash::{vk, Device};

use super::{CommandPool, Queue};

pub struct CommandBuffer {
    pub command_buffer: vk::CommandBuffer,
}

// no need to manually drop here as it is handled by the command pool

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

pub fn execute_one_time_commands<R, F: FnOnce(&CommandBuffer) -> R>(
    device: &Device,
    queue: Queue,
    pool: &CommandPool,
    executor: F,
) -> R {
    let vk_command_buffer = {
        let alloc_info = vk::CommandBufferAllocateInfo::default()
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_pool(pool.as_raw())
            .command_buffer_count(1);

        unsafe { device.allocate_command_buffers(&alloc_info).unwrap()[0] }
    };

    let command_buffer = CommandBuffer {
        command_buffer: vk_command_buffer,
    };

    let command_buffers = [vk_command_buffer];
    {
        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe {
            device
                .begin_command_buffer(vk_command_buffer, &begin_info)
                .unwrap()
        };
    }

    let executor_result = executor(&command_buffer);

    unsafe { device.end_command_buffer(vk_command_buffer).unwrap() };

    {
        let submit_info = vk::SubmitInfo::default().command_buffers(&command_buffers);
        let submit_infos = [submit_info];
        unsafe {
            device
                .queue_submit(queue.as_raw(), &submit_infos, vk::Fence::null())
                .unwrap();
            device.queue_wait_idle(queue.as_raw()).unwrap();
        };
    }

    // Free
    unsafe { device.free_command_buffers(pool.as_raw(), &command_buffers) };

    executor_result
}
