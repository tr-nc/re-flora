use ash::vk;

use crate::{CommandBuffer, Fence, Semaphore};

const MAX_SUBMIT_COMMAND_BUFFERS: usize = 8;
const MAX_SUBMIT_WAITS: usize = 8;
const MAX_SUBMIT_SIGNALS: usize = 8;

/// Semantic wait-stage choices for queue submissions.
///
/// Keep this intentionally small and intent-oriented. Vkn translates these
/// variants to Vulkan stage masks at the submit boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipelineWaitStage {
    TopOfPipe,
    ColorAttachmentOutput,
    ComputeShader,
    Transfer,
    AllCommands,
}

impl PipelineWaitStage {
    pub(crate) fn as_raw(self) -> vk::PipelineStageFlags {
        match self {
            Self::TopOfPipe => vk::PipelineStageFlags::TOP_OF_PIPE,
            Self::ColorAttachmentOutput => vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            Self::ComputeShader => vk::PipelineStageFlags::COMPUTE_SHADER,
            Self::Transfer => vk::PipelineStageFlags::TRANSFER,
            Self::AllCommands => vk::PipelineStageFlags::ALL_COMMANDS,
        }
    }
}

/// A named binary semaphore wait edge for a queue submission.
#[derive(Clone, Copy)]
pub struct SubmitWait<'a> {
    pub name: &'static str,
    pub semaphore: &'a Semaphore,
    pub stage: PipelineWaitStage,
}

impl<'a> SubmitWait<'a> {
    pub fn new(name: &'static str, semaphore: &'a Semaphore, stage: PipelineWaitStage) -> Self {
        Self {
            name,
            semaphore,
            stage,
        }
    }
}

/// A named binary semaphore signal edge for a queue submission.
#[derive(Clone, Copy)]
pub struct SubmitSignal<'a> {
    pub name: &'static str,
    pub semaphore: &'a Semaphore,
}

impl<'a> SubmitSignal<'a> {
    pub fn new(name: &'static str, semaphore: &'a Semaphore) -> Self {
        Self { name, semaphore }
    }
}

/// Semantic queue submit description.
///
/// This is the vkn-owned boundary where command buffers, waits, signals, and
/// fences become raw Vulkan submit info. It deliberately carries stable names so
/// a future profiler can turn submissions and semaphore edges into timeline and
/// dependency events without changing call sites again.
#[derive(Clone, Copy)]
pub struct SubmitDesc<'a> {
    pub name: &'static str,
    pub command_buffers: &'a [&'a CommandBuffer],
    pub waits: &'a [SubmitWait<'a>],
    pub signals: &'a [SubmitSignal<'a>],
    pub fence: Option<&'a Fence>,
}

impl<'a> SubmitDesc<'a> {
    pub fn new(
        name: &'static str,
        command_buffers: &'a [&'a CommandBuffer],
        waits: &'a [SubmitWait<'a>],
        signals: &'a [SubmitSignal<'a>],
        fence: Option<&'a Fence>,
    ) -> Self {
        Self {
            name,
            command_buffers,
            waits,
            signals,
            fence,
        }
    }

    pub(crate) fn assert_supported_sizes(&self) {
        assert!(
            self.command_buffers.len() <= MAX_SUBMIT_COMMAND_BUFFERS,
            "submit '{}' has {} command buffers; max supported without allocation is {}",
            self.name,
            self.command_buffers.len(),
            MAX_SUBMIT_COMMAND_BUFFERS
        );
        assert!(
            self.waits.len() <= MAX_SUBMIT_WAITS,
            "submit '{}' has {} waits; max supported without allocation is {}",
            self.name,
            self.waits.len(),
            MAX_SUBMIT_WAITS
        );
        assert!(
            self.signals.len() <= MAX_SUBMIT_SIGNALS,
            "submit '{}' has {} signals; max supported without allocation is {}",
            self.name,
            self.signals.len(),
            MAX_SUBMIT_SIGNALS
        );
    }

    pub(crate) fn raw_command_buffers(&self) -> ([vk::CommandBuffer; MAX_SUBMIT_COMMAND_BUFFERS], usize) {
        let mut raw = [vk::CommandBuffer::null(); MAX_SUBMIT_COMMAND_BUFFERS];
        for (dst, command_buffer) in raw.iter_mut().zip(self.command_buffers.iter()) {
            *dst = command_buffer.as_raw();
        }
        (raw, self.command_buffers.len())
    }

    pub(crate) fn raw_waits(
        &self,
    ) -> (
        [vk::Semaphore; MAX_SUBMIT_WAITS],
        [vk::PipelineStageFlags; MAX_SUBMIT_WAITS],
        usize,
    ) {
        let mut semaphores = [vk::Semaphore::null(); MAX_SUBMIT_WAITS];
        let mut stages = [vk::PipelineStageFlags::empty(); MAX_SUBMIT_WAITS];
        for ((raw_semaphore, raw_stage), wait) in semaphores
            .iter_mut()
            .zip(stages.iter_mut())
            .zip(self.waits.iter())
        {
            *raw_semaphore = wait.semaphore.as_raw();
            *raw_stage = wait.stage.as_raw();
        }
        (semaphores, stages, self.waits.len())
    }

    pub(crate) fn raw_signals(&self) -> ([vk::Semaphore; MAX_SUBMIT_SIGNALS], usize) {
        let mut raw = [vk::Semaphore::null(); MAX_SUBMIT_SIGNALS];
        for (dst, signal) in raw.iter_mut().zip(self.signals.iter()) {
            *dst = signal.semaphore.as_raw();
        }
        (raw, self.signals.len())
    }
}
