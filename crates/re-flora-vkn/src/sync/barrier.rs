use ash::vk;
use std::ops::{BitOr, BitOrAssign};

use crate::{CommandBuffer, Device};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryAccess(vk::AccessFlags);

impl MemoryAccess {
    pub const TRANSFER_WRITE: Self = Self(vk::AccessFlags::TRANSFER_WRITE);
    pub const SHADER_READ: Self = Self(vk::AccessFlags::SHADER_READ);
    pub const SHADER_WRITE: Self = Self(vk::AccessFlags::SHADER_WRITE);
    pub const INDIRECT_COMMAND_READ: Self = Self(vk::AccessFlags::INDIRECT_COMMAND_READ);
    pub const HOST_READ: Self = Self(vk::AccessFlags::HOST_READ);

    pub const fn empty() -> Self {
        Self(vk::AccessFlags::empty())
    }

    pub(crate) fn as_raw(self) -> vk::AccessFlags {
        self.0
    }
}

impl BitOr for MemoryAccess {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for MemoryAccess {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl From<vk::AccessFlags> for MemoryAccess {
    fn from(value: vk::AccessFlags) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy)]
pub struct MemoryBarrier {
    src_access_mask: MemoryAccess,
    dst_access_mask: MemoryAccess,
}

impl MemoryBarrier {
    pub fn new(
        src_access_mask: impl Into<MemoryAccess>,
        dst_access_mask: impl Into<MemoryAccess>,
    ) -> Self {
        Self {
            src_access_mask: src_access_mask.into(),
            dst_access_mask: dst_access_mask.into(),
        }
    }

    /// Ensures the previous shader write is done before the next shader read/write.
    pub fn new_shader_access() -> Self {
        Self {
            src_access_mask: MemoryAccess::SHADER_WRITE,
            dst_access_mask: MemoryAccess::SHADER_READ | MemoryAccess::SHADER_WRITE,
        }
    }

    /// Ensures the previous shader write is done before reading the indirect command buffer.
    ///
    /// Useful when the previous shader writes to a buffer that is used as an indirect command buffer.
    pub fn new_indirect_access() -> Self {
        Self {
            src_access_mask: MemoryAccess::SHADER_WRITE,
            dst_access_mask: MemoryAccess::INDIRECT_COMMAND_READ,
        }
    }

    pub fn as_raw(&self) -> vk::MemoryBarrier<'_> {
        vk::MemoryBarrier::default()
            .src_access_mask(self.src_access_mask.as_raw())
            .dst_access_mask(self.dst_access_mask.as_raw())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PipelineStage(vk::PipelineStageFlags);

impl PipelineStage {
    pub const TOP_OF_PIPE: Self = Self(vk::PipelineStageFlags::TOP_OF_PIPE);
    pub const COMPUTE_SHADER: Self = Self(vk::PipelineStageFlags::COMPUTE_SHADER);
    pub const VERTEX_SHADER: Self = Self(vk::PipelineStageFlags::VERTEX_SHADER);
    pub const FRAGMENT_SHADER: Self = Self(vk::PipelineStageFlags::FRAGMENT_SHADER);
    pub const DRAW_INDIRECT: Self = Self(vk::PipelineStageFlags::DRAW_INDIRECT);
    pub const TRANSFER: Self = Self(vk::PipelineStageFlags::TRANSFER);
    pub const HOST: Self = Self(vk::PipelineStageFlags::HOST);
    pub const ALL_COMMANDS: Self = Self(vk::PipelineStageFlags::ALL_COMMANDS);

    pub const fn empty() -> Self {
        Self(vk::PipelineStageFlags::empty())
    }

    pub(crate) fn as_raw(self) -> vk::PipelineStageFlags {
        self.0
    }
}

impl BitOr for PipelineStage {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for PipelineStage {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl From<vk::PipelineStageFlags> for PipelineStage {
    fn from(value: vk::PipelineStageFlags) -> Self {
        Self(value)
    }
}

#[derive(Clone)]
pub struct PipelineBarrier {
    src_stage_mask: PipelineStage,
    dst_stage_mask: PipelineStage,
    memory_barriers: Vec<MemoryBarrier>,
}

impl PipelineBarrier {
    pub fn new(
        src_stage_mask: impl Into<PipelineStage>,
        dst_stage_mask: impl Into<PipelineStage>,
        memory_barriers: Vec<MemoryBarrier>,
    ) -> Self {
        Self {
            src_stage_mask: src_stage_mask.into(),
            dst_stage_mask: dst_stage_mask.into(),
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
                self.src_stage_mask.as_raw(),
                self.dst_stage_mask.as_raw(),
                vk::DependencyFlags::empty(),
                &memory_barriers,
                &[],
                &[],
            );
        }
    }
}
