use super::{CommandPool, Device, Fence, Queue};
use ash::vk;
use std::sync::Arc;

struct CommandBufferInner {
    device: Device,
    command_pool: CommandPool,
    command_buffer: vk::CommandBuffer,
}

impl Drop for CommandBufferInner {
    fn drop(&mut self) {
        unsafe {
            self.device
                .free_command_buffers(self.command_pool.as_raw(), &[self.command_buffer]);
        }
    }
}

#[derive(Clone)]
pub struct CommandBuffer(Arc<CommandBufferInner>);

impl std::ops::Deref for CommandBuffer {
    type Target = vk::CommandBuffer;
    fn deref(&self) -> &Self::Target {
        &self.0.command_buffer
    }
}

impl CommandBuffer {
    pub fn new(device: &Device, command_pool: &CommandPool) -> Self {
        let command_buffer = create_cmdbuf(device, command_pool.as_raw());
        Self(Arc::new(CommandBufferInner {
            device: device.clone(),
            command_pool: command_pool.clone(),
            command_buffer,
        }))
    }

    pub fn as_raw(&self) -> vk::CommandBuffer {
        self.0.command_buffer
    }

    /// Begin recording command buffer, if the command buffer is in not in initial state (being recorded before), begin will reset the command buffer implicitly
    pub fn begin(&self, is_onetime: bool) {
        let flags = if is_onetime {
            vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT
        } else {
            vk::CommandBufferUsageFlags::empty()
        };

        let begin_info = vk::CommandBufferBeginInfo::default().flags(flags);
        unsafe {
            self.0
                .device
                .begin_command_buffer(self.0.command_buffer, &begin_info)
                .unwrap()
        };
    }

    pub fn end(&self) {
        unsafe {
            self.0
                .device
                .end_command_buffer(self.0.command_buffer)
                .unwrap()
        };
    }

    pub fn submit(&self, queue: &Queue, fence: Option<&Fence>) {
        let command_buffers = [self.as_raw()];
        let submit_info = vk::SubmitInfo::default().command_buffers(&command_buffers);

        let vk_fence = fence
            .as_ref()
            .map(|f| f.as_raw())
            .unwrap_or(vk::Fence::null());
        unsafe {
            self.0
                .device
                .queue_submit(queue.as_raw(), &[submit_info], vk_fence)
                .unwrap();
        }
    }
}

fn create_cmdbuf(device: &Device, command_pool: vk::CommandPool) -> vk::CommandBuffer {
    let allocate_info = vk::CommandBufferAllocateInfo::default()
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_pool(command_pool)
        .command_buffer_count(1);
    unsafe { device.allocate_command_buffers(&allocate_info).unwrap()[0] }
}

pub fn execute_one_time_command<R, F: FnOnce(&CommandBuffer) -> R>(
    device: &Device,
    pool: &CommandPool,
    queue: &Queue,
    executor: F,
) -> R {
    let command_buffer = CommandBuffer::new(device, pool);

    command_buffer.begin(true);
    let result = executor(&command_buffer);
    command_buffer.end();

    command_buffer.submit(queue, None);
    device.wait_queue_idle(&queue);
    result
}
