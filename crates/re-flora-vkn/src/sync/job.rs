use ash::prelude::VkResult;

use crate::{CommandBuffer, Device, Fence, Queue, SubmitDesc, SubmitSignal, SubmitWait};

/// Semantic queue lane for vkn-managed GPU jobs.
///
/// The first implementation submits to the queue supplied by the caller while
/// carrying the lane name for diagnostics and future multi-queue scheduling.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueLane {
    General,
}

/// Completion mechanism for a submitted GPU job.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobCompletion {
    Fence,
}

/// Named description for non-swapchain GPU work.
///
/// This covers compute/build/copy/readback jobs that are not part of the main
/// swapchain frame lifecycle. The descriptor is translated to `SubmitDesc` at
/// the vkn boundary so callers do not own raw fence submission behavior.
#[derive(Clone, Copy)]
pub struct GpuJobDesc<'a> {
    pub name: &'static str,
    pub queue: QueueLane,
    pub command_buffers: &'a [&'a CommandBuffer],
    pub waits: &'a [SubmitWait<'a>],
    pub signals: &'a [SubmitSignal<'a>],
    pub completion: JobCompletion,
}

impl<'a> GpuJobDesc<'a> {
    pub fn new(
        name: &'static str,
        queue: QueueLane,
        command_buffers: &'a [&'a CommandBuffer],
        waits: &'a [SubmitWait<'a>],
        signals: &'a [SubmitSignal<'a>],
        completion: JobCompletion,
    ) -> Self {
        Self {
            name,
            queue,
            command_buffers,
            waits,
            signals,
            completion,
        }
    }
}

/// Completion token for a submitted vkn-managed GPU job.
///
/// The token intentionally exposes semantic polling/waiting only. The backing
/// fence remains inside vkn. Prefer consuming a token with `wait_complete` once
/// the caller is done tracking a job; borrowed waits remain available for flush
/// paths that need to keep owning the submitted job record.
pub struct GpuJobToken {
    name: &'static str,
    queue: QueueLane,
    fence: Fence,
}

/// Proof that a vkn-managed GPU job has completed.
///
/// This keeps the completed backing fence owned by vkn, which gives future job
/// slot/fence pooling a single explicit handoff point without changing builder
/// call sites again.
pub struct CompletedGpuJob {
    name: &'static str,
    queue: QueueLane,
    _fence: Fence,
}

impl CompletedGpuJob {
    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn queue(&self) -> QueueLane {
        self.queue
    }
}

impl GpuJobToken {
    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn queue(&self) -> QueueLane {
        self.queue
    }

    pub fn is_complete(&self) -> VkResult<bool> {
        crate::sync::diagnostics::record_gpu_job_poll(self.name, self.queue);
        self.fence.is_signaled()
    }

    pub fn wait(&self) -> VkResult<()> {
        crate::sync::diagnostics::record_gpu_job_wait(self.name, self.queue);
        self.fence.wait()
    }

    pub fn wait_complete(self) -> VkResult<CompletedGpuJob> {
        crate::sync::diagnostics::record_gpu_job_wait(self.name, self.queue);
        self.fence.wait()?;
        Ok(self.into_completed())
    }

    pub fn complete_if_ready(self) -> VkResult<Result<CompletedGpuJob, Self>> {
        crate::sync::diagnostics::record_gpu_job_poll(self.name, self.queue);
        if self.fence.is_signaled()? {
            Ok(Ok(self.into_completed()))
        } else {
            Ok(Err(self))
        }
    }

    fn into_completed(self) -> CompletedGpuJob {
        self.fence.mark_completed_for_reuse();
        CompletedGpuJob {
            name: self.name,
            queue: self.queue,
            _fence: self.fence,
        }
    }
}

/// Stateless entry point for vkn-managed GPU job submission.
///
/// A future implementation can replace this with pooled job slots without
/// changing builder/app call sites that hold `GpuJobToken`s.
pub struct GpuJobManager;

impl GpuJobManager {
    pub fn submit(device: &Device, queue: &Queue, desc: GpuJobDesc<'_>) -> VkResult<GpuJobToken> {
        crate::sync::diagnostics::record_gpu_job_submit(&desc);
        let fence = Fence::new_pooled_gpu_job(device)?;
        let submit_desc = SubmitDesc::new(
            desc.name,
            desc.command_buffers,
            desc.waits,
            desc.signals,
            Some(&fence),
        );
        device.submit_to_queue(queue, submit_desc)?;
        Ok(GpuJobToken {
            name: desc.name,
            queue: desc.queue,
            fence,
        })
    }
}
