use ash::vk;

use crate::vkn::{CommandBuffer, Device};

#[derive(Clone, Copy)]
pub struct MemoryBarrier {
    src_access_mask: vk::AccessFlags,
    dst_access_mask: vk::AccessFlags,
}

impl MemoryBarrier {
    #[allow(dead_code)]
    pub fn new(src_access_mask: vk::AccessFlags, dst_access_mask: vk::AccessFlags) -> Self {
        Self {
            src_access_mask,
            dst_access_mask,
        }
    }

    /// Ensures the previous shader write is done before the next shader read/write.
    pub fn new_shader_access() -> Self {
        Self {
            src_access_mask: vk::AccessFlags::SHADER_WRITE,
            dst_access_mask: vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE,
        }
    }

    /// Ensures the previous shader write is done before reading the indirect command buffer.
    ///
    /// Useful when the previous shader writes to a buffer that is used as an indirect command buffer.
    pub fn new_indirect_access() -> Self {
        Self {
            src_access_mask: vk::AccessFlags::SHADER_WRITE,
            dst_access_mask: vk::AccessFlags::INDIRECT_COMMAND_READ,
        }
    }

    pub fn as_raw(&self) -> vk::MemoryBarrier {
        vk::MemoryBarrier::default()
            .src_access_mask(self.src_access_mask)
            .dst_access_mask(self.dst_access_mask)
    }
}

// TODO: this is incomplete for now.

#[derive(Clone)]
pub struct PipelineBarrier {
    pub src_stage_mask: vk::PipelineStageFlags,
    pub dst_stage_mask: vk::PipelineStageFlags,
    pub memory_barriers: Vec<MemoryBarrier>,
}

impl PipelineBarrier {
    pub fn new(
        src_stage_mask: vk::PipelineStageFlags,
        dst_stage_mask: vk::PipelineStageFlags,
        memory_barriers: Vec<MemoryBarrier>,
    ) -> Self {
        Self {
            src_stage_mask,
            dst_stage_mask,
            memory_barriers,
        }
    }

    pub fn record_insert(&self, device: &Device, cmdbuf: &CommandBuffer) {
        let memory_barriers = self
            .memory_barriers
            .iter()
            .map(|mb| mb.as_raw())
            .collect::<Vec<_>>();

        unsafe {
            device.cmd_pipeline_barrier(
                cmdbuf.as_raw(),
                self.src_stage_mask,
                self.dst_stage_mask,
                vk::DependencyFlags::empty(),
                &memory_barriers,
                &[],
                &[],
            );
        }
    }
}
