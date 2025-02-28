use ash::vk;

use super::{CommandPool, Device, Queue};

#[derive(Clone)]
pub struct CommandBuffer {
    device: Device,
    command_buffer: vk::CommandBuffer,
}

// no need to manually drop here as it is handled by the command pool

impl CommandBuffer {
    pub fn new(device: &Device, command_pool: &CommandPool) -> Self {
        let command_buffer = create_cmdbuf(device, command_pool.as_raw());
        Self {
            device: device.clone(),
            command_buffer,
        }
    }

    pub fn as_raw(&self) -> vk::CommandBuffer {
        self.command_buffer
    }

    pub fn begin_onetime(&self) {
        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe {
            self.device
                .begin_command_buffer(self.command_buffer, &begin_info)
                .unwrap()
        };
    }

    pub fn end(&self) {
        unsafe { self.device.end_command_buffer(self.command_buffer).unwrap() };
    }
}

fn create_cmdbuf(device: &Device, command_pool: vk::CommandPool) -> vk::CommandBuffer {
    let allocate_info = vk::CommandBufferAllocateInfo::default()
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_pool(command_pool)
        .command_buffer_count(1);
    unsafe { device.allocate_command_buffers(&allocate_info).unwrap()[0] }
}

pub fn execute_one_time_commands<R, F: FnOnce(&CommandBuffer) -> R>(
    device: &Device,
    queue: Queue,
    pool: &CommandPool,
    executor: F,
) -> R {
    let command_buffer = CommandBuffer::new(device, pool);

    command_buffer.begin_onetime();
    let result = executor(&command_buffer);
    command_buffer.end();

    let command_buffers = [command_buffer.as_raw()];
    {
        let submit_info = vk::SubmitInfo::default().command_buffers(&command_buffers);
        let submit_infos = [submit_info];
        unsafe {
            device
                .queue_submit(queue.as_raw(), &submit_infos, vk::Fence::null())
                .unwrap();
            device.wait_queue_idle(&queue);
        };
    }

    // Free
    unsafe { device.free_command_buffers(pool.as_raw(), &command_buffers) };

    result
}
