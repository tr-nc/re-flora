use super::CommandPool;
use crate::{
    Buffer, DescriptorSet, Device, Extent2D, GpuJobManager, GraphicsPipeline, Queue, QueueLane,
    SubmitDesc, Viewport,
};
use ash::vk;
use std::sync::Arc;

const MAX_VERTEX_BUFFER_BINDINGS: usize = 8;

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

    pub fn end_render_pass(&self) {
        self.0.device.cmd_end_render_pass_raw(self.0.command_buffer);
    }

    pub fn bind_graphics_pipeline(&self, pipeline: &GraphicsPipeline) {
        self.0
            .device
            .cmd_bind_pipeline_graphics_raw(self.0.command_buffer, pipeline.as_raw());
    }

    pub fn set_viewport_from_extent(&self, extent: Extent2D) {
        self.0.device.cmd_set_viewport_raw(
            self.0.command_buffer,
            0,
            &[Viewport::from_extent(extent).as_raw()],
        );
    }

    pub fn push_vertex_constants(&self, pipeline: &GraphicsPipeline, constants: &[u8]) {
        self.0.device.cmd_push_constants_raw(
            self.0.command_buffer,
            pipeline.get_layout().as_raw(),
            vk::ShaderStageFlags::VERTEX,
            0,
            constants,
        );
    }

    pub fn bind_index_buffer_u32(&self, buffer: &Buffer) {
        self.0.device.cmd_bind_index_buffer_raw(
            self.0.command_buffer,
            buffer.as_raw(),
            0,
            vk::IndexType::UINT32,
        );
    }

    pub fn bind_vertex_buffers(&self, first_binding: u32, buffers: &[&Buffer]) {
        assert!(
            buffers.len() <= MAX_VERTEX_BUFFER_BINDINGS,
            "binding {} vertex buffers exceeds fixed stack capacity {}",
            buffers.len(),
            MAX_VERTEX_BUFFER_BINDINGS
        );

        let mut raw_buffers = [vk::Buffer::null(); MAX_VERTEX_BUFFER_BINDINGS];
        let offsets = [0u64; MAX_VERTEX_BUFFER_BINDINGS];
        for (dst, buffer) in raw_buffers.iter_mut().zip(buffers.iter()) {
            *dst = buffer.as_raw();
        }

        self.0.device.cmd_bind_vertex_buffers_raw(
            self.0.command_buffer,
            first_binding,
            &raw_buffers[..buffers.len()],
            &offsets[..buffers.len()],
        );
    }

    pub fn set_scissor(&self, offset: [i32; 2], extent: Extent2D) {
        let scissors = [vk::Rect2D {
            offset: vk::Offset2D {
                x: offset[0],
                y: offset[1],
            },
            extent: extent.as_raw(),
        }];
        self.0
            .device
            .cmd_set_scissor_raw(self.0.command_buffer, 0, &scissors);
    }

    pub fn bind_graphics_descriptor_set(
        &self,
        pipeline: &GraphicsPipeline,
        first_set: u32,
        descriptor_set: &DescriptorSet,
    ) {
        self.0.device.cmd_bind_descriptor_sets_graphics_raw(
            self.0.command_buffer,
            pipeline.get_layout().as_raw(),
            first_set,
            &[descriptor_set.as_raw()],
        );
    }

    pub fn draw_indexed(
        &self,
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        vertex_offset: i32,
        first_instance: u32,
    ) {
        self.0.device.cmd_draw_indexed_raw(
            self.0.command_buffer,
            index_count,
            instance_count,
            first_index,
            vertex_offset,
            first_instance,
        );
    }

    /// Submit this off-frame command buffer and transfer its residency to the
    /// returned managed job. Clone a deliberately reusable prerecorded command
    /// buffer before calling this method.
    pub fn submit_gpu_job(
        self,
        queue: &Queue,
        name: &'static str,
    ) -> ash::prelude::VkResult<crate::GpuJobToken> {
        let device = self.0.device.clone();
        GpuJobManager::submit(&device, queue, name, QueueLane::General, self)
    }
}

fn create_cmdbuf(device: &Device, command_pool: vk::CommandPool) -> vk::CommandBuffer {
    let allocate_info = vk::CommandBufferAllocateInfo::default()
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_pool(command_pool)
        .command_buffer_count(1);
    unsafe { device.allocate_command_buffers(&allocate_info).unwrap()[0] }
}

/// Execute a one-time command buffer, workload is guaranteed to be finished before return.
/// Uses wait_queue_idle to ensure all prior GPU work on the queue is complete.
/// On MoltenVK this flushes the entire Metal command pipeline, but fence-based
/// wait caused Bus Error crashes — likely a MoltenVK command buffer recycling issue.
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

    let command_buffers = [&command_buffer];
    let desc = SubmitDesc::new("execute_one_time_command", &command_buffers, &[], &[], None);
    device.submit_to_queue(queue, desc).unwrap();
    device.wait_queue_idle(queue);
    result
}

/// Execute a one-time command buffer as a managed GPU job and wait only for
/// that job to complete.
///
/// This avoids idling the entire queue, which is useful for small compute jobs
/// like batched terrain queries that need synchronous CPU readback.
pub fn execute_one_time_gpu_job<R, F: FnOnce(&CommandBuffer) -> R>(
    device: &Device,
    pool: &CommandPool,
    queue: &Queue,
    executor: F,
) -> R {
    let command_buffer = CommandBuffer::new(device, pool);
    command_buffer.begin(true);
    let result = executor(&command_buffer);
    command_buffer.end();

    let job = command_buffer
        .submit_gpu_job(queue, "execute_one_time_gpu_job")
        .unwrap();
    let _completed_job = job.wait_complete().unwrap();
    result
}
