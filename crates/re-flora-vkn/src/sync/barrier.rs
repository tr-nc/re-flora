use ash::vk;
use std::ops::{BitOr, BitOrAssign};

use crate::{CommandBuffer, Device};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextureLayout(vk::ImageLayout);

impl TextureLayout {
    pub const UNDEFINED: Self = Self(vk::ImageLayout::UNDEFINED);
    pub const PREINITIALIZED: Self = Self(vk::ImageLayout::PREINITIALIZED);
    pub const GENERAL: Self = Self(vk::ImageLayout::GENERAL);
    pub const TRANSFER_SRC: Self = Self(vk::ImageLayout::TRANSFER_SRC_OPTIMAL);
    pub const TRANSFER_DST: Self = Self(vk::ImageLayout::TRANSFER_DST_OPTIMAL);
    pub const SHADER_READ_ONLY: Self = Self(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
    pub const COLOR_ATTACHMENT: Self = Self(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
    pub const DEPTH_STENCIL_ATTACHMENT: Self =
        Self(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);
    pub const PRESENT_SRC: Self = Self(vk::ImageLayout::PRESENT_SRC_KHR);

    pub(crate) fn from_raw(layout: vk::ImageLayout) -> Self {
        Self(layout)
    }

    pub(crate) fn as_raw(self) -> vk::ImageLayout {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryAccess(vk::AccessFlags);

impl MemoryAccess {
    pub const TRANSFER_READ: Self = Self(vk::AccessFlags::TRANSFER_READ);
    pub const TRANSFER_WRITE: Self = Self(vk::AccessFlags::TRANSFER_WRITE);
    pub const SHADER_READ: Self = Self(vk::AccessFlags::SHADER_READ);
    pub const SHADER_WRITE: Self = Self(vk::AccessFlags::SHADER_WRITE);
    pub const INDIRECT_COMMAND_READ: Self = Self(vk::AccessFlags::INDIRECT_COMMAND_READ);
    pub const COLOR_ATTACHMENT_READ: Self = Self(vk::AccessFlags::COLOR_ATTACHMENT_READ);
    pub const COLOR_ATTACHMENT_WRITE: Self = Self(vk::AccessFlags::COLOR_ATTACHMENT_WRITE);
    pub const DEPTH_STENCIL_ATTACHMENT_WRITE: Self =
        Self(vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE);
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

#[derive(Clone, Copy)]
pub struct MemoryBarrier {
    src_access_mask: MemoryAccess,
    dst_access_mask: MemoryAccess,
}

impl MemoryBarrier {
    pub fn new(src_access_mask: MemoryAccess, dst_access_mask: MemoryAccess) -> Self {
        Self {
            src_access_mask,
            dst_access_mask,
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
    pub const BOTTOM_OF_PIPE: Self = Self(vk::PipelineStageFlags::BOTTOM_OF_PIPE);
    pub const COMPUTE_SHADER: Self = Self(vk::PipelineStageFlags::COMPUTE_SHADER);
    pub const VERTEX_SHADER: Self = Self(vk::PipelineStageFlags::VERTEX_SHADER);
    pub const FRAGMENT_SHADER: Self = Self(vk::PipelineStageFlags::FRAGMENT_SHADER);
    pub const DRAW_INDIRECT: Self = Self(vk::PipelineStageFlags::DRAW_INDIRECT);
    pub const TRANSFER: Self = Self(vk::PipelineStageFlags::TRANSFER);
    pub const COLOR_ATTACHMENT_OUTPUT: Self =
        Self(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT);
    pub const EARLY_FRAGMENT_TESTS: Self = Self(vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS);
    pub const LATE_FRAGMENT_TESTS: Self = Self(vk::PipelineStageFlags::LATE_FRAGMENT_TESTS);
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceState {
    layout: TextureLayout,
    stage: PipelineStage,
    access: MemoryAccess,
}

impl ResourceState {
    pub fn new(layout: TextureLayout, stage: PipelineStage, access: MemoryAccess) -> Self {
        Self {
            layout,
            stage,
            access,
        }
    }

    pub fn layout(self) -> TextureLayout {
        self.layout
    }

    pub fn stage(self) -> PipelineStage {
        self.stage
    }

    pub fn access(self) -> MemoryAccess {
        self.access
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextureTransition {
    old_state: ResourceState,
    new_state: ResourceState,
}

impl TextureTransition {
    pub fn new(old_state: ResourceState, new_state: ResourceState) -> Self {
        Self {
            old_state,
            new_state,
        }
    }

    pub fn from_layouts(old_layout: TextureLayout, new_layout: TextureLayout) -> Self {
        Self {
            old_state: texture_source_state(old_layout),
            new_state: texture_destination_state(new_layout),
        }
    }

    pub fn old_state(self) -> ResourceState {
        self.old_state
    }

    pub fn new_state(self) -> ResourceState {
        self.new_state
    }

    pub(crate) fn old_layout(self) -> vk::ImageLayout {
        self.old_state.layout.as_raw()
    }

    pub(crate) fn new_layout(self) -> vk::ImageLayout {
        self.new_state.layout.as_raw()
    }

    pub(crate) fn src_stage(self) -> vk::PipelineStageFlags {
        self.old_state.stage.as_raw()
    }

    pub(crate) fn dst_stage(self) -> vk::PipelineStageFlags {
        self.new_state.stage.as_raw()
    }

    pub(crate) fn src_access(self) -> vk::AccessFlags {
        self.old_state.access.as_raw()
    }

    pub(crate) fn dst_access(self) -> vk::AccessFlags {
        self.new_state.access.as_raw()
    }
}

fn general_shader_stages() -> PipelineStage {
    PipelineStage::COMPUTE_SHADER | PipelineStage::VERTEX_SHADER | PipelineStage::FRAGMENT_SHADER
}

fn texture_source_state(layout: TextureLayout) -> ResourceState {
    match layout.as_raw() {
        vk::ImageLayout::UNDEFINED => ResourceState::new(
            TextureLayout::UNDEFINED,
            PipelineStage::TOP_OF_PIPE,
            MemoryAccess::empty(),
        ),
        vk::ImageLayout::GENERAL => ResourceState::new(
            TextureLayout::GENERAL,
            general_shader_stages(),
            MemoryAccess::SHADER_WRITE,
        ),
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL => ResourceState::new(
            TextureLayout::TRANSFER_SRC,
            PipelineStage::TRANSFER,
            MemoryAccess::empty(),
        ),
        vk::ImageLayout::TRANSFER_DST_OPTIMAL => ResourceState::new(
            TextureLayout::TRANSFER_DST,
            PipelineStage::TRANSFER,
            MemoryAccess::TRANSFER_WRITE,
        ),
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL => ResourceState::new(
            TextureLayout::SHADER_READ_ONLY,
            general_shader_stages(),
            MemoryAccess::empty(),
        ),
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL => ResourceState::new(
            TextureLayout::COLOR_ATTACHMENT,
            PipelineStage::COLOR_ATTACHMENT_OUTPUT,
            MemoryAccess::COLOR_ATTACHMENT_WRITE,
        ),
        vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL => ResourceState::new(
            TextureLayout::DEPTH_STENCIL_ATTACHMENT,
            PipelineStage::EARLY_FRAGMENT_TESTS | PipelineStage::LATE_FRAGMENT_TESTS,
            MemoryAccess::DEPTH_STENCIL_ATTACHMENT_WRITE,
        ),
        vk::ImageLayout::PRESENT_SRC_KHR => ResourceState::new(
            TextureLayout::PRESENT_SRC,
            PipelineStage::COLOR_ATTACHMENT_OUTPUT,
            MemoryAccess::COLOR_ATTACHMENT_WRITE,
        ),
        raw_layout => {
            panic!("Unsupported old_layout transition from: {:?}", raw_layout);
        }
    }
}

fn texture_destination_state(layout: TextureLayout) -> ResourceState {
    match layout.as_raw() {
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL => ResourceState::new(
            TextureLayout::SHADER_READ_ONLY,
            general_shader_stages(),
            MemoryAccess::SHADER_READ,
        ),
        vk::ImageLayout::GENERAL => ResourceState::new(
            TextureLayout::GENERAL,
            general_shader_stages(),
            MemoryAccess::SHADER_READ | MemoryAccess::SHADER_WRITE,
        ),
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL => ResourceState::new(
            TextureLayout::TRANSFER_SRC,
            PipelineStage::TRANSFER,
            MemoryAccess::TRANSFER_READ,
        ),
        vk::ImageLayout::TRANSFER_DST_OPTIMAL => ResourceState::new(
            TextureLayout::TRANSFER_DST,
            PipelineStage::TRANSFER,
            MemoryAccess::TRANSFER_WRITE,
        ),
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL => ResourceState::new(
            TextureLayout::COLOR_ATTACHMENT,
            PipelineStage::COLOR_ATTACHMENT_OUTPUT,
            MemoryAccess::COLOR_ATTACHMENT_READ | MemoryAccess::COLOR_ATTACHMENT_WRITE,
        ),
        vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL => ResourceState::new(
            TextureLayout::DEPTH_STENCIL_ATTACHMENT,
            PipelineStage::EARLY_FRAGMENT_TESTS | PipelineStage::LATE_FRAGMENT_TESTS,
            MemoryAccess::DEPTH_STENCIL_ATTACHMENT_WRITE,
        ),
        vk::ImageLayout::PRESENT_SRC_KHR => ResourceState::new(
            TextureLayout::PRESENT_SRC,
            PipelineStage::BOTTOM_OF_PIPE,
            MemoryAccess::empty(),
        ),
        raw_layout => {
            panic!("Unsupported new_layout transition to: {:?}", raw_layout);
        }
    }
}

#[derive(Clone)]
pub struct PipelineBarrier<const MEMORY_BARRIER_COUNT: usize> {
    src_stage_mask: PipelineStage,
    dst_stage_mask: PipelineStage,
    memory_barriers: [MemoryBarrier; MEMORY_BARRIER_COUNT],
}

impl<const MEMORY_BARRIER_COUNT: usize> PipelineBarrier<MEMORY_BARRIER_COUNT> {
    pub fn new(
        src_stage_mask: PipelineStage,
        dst_stage_mask: PipelineStage,
        memory_barriers: [MemoryBarrier; MEMORY_BARRIER_COUNT],
    ) -> Self {
        Self {
            src_stage_mask,
            dst_stage_mask,
            memory_barriers,
        }
    }

    pub fn record_insert(&self, device: &Device, cmdbuf: &CommandBuffer) {
        let memory_barriers: [vk::MemoryBarrier<'_>; MEMORY_BARRIER_COUNT] =
            std::array::from_fn(|i| self.memory_barriers[i].as_raw());

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
