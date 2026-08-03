use ash::prelude::VkResult;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::{
    sync::submit::MAX_SUBMIT_COMMAND_BUFFERS, CommandBuffer, Device, Fence, Queue, SubmitDesc,
    SubmitSignal, SubmitWait,
};

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
/// The token retains the backing fence and submitted command buffers, and only
/// exposes semantic polling/waiting. Prefer consuming a token with
/// `wait_complete` once the caller is done tracking a job; borrowed waits remain
/// available for flush paths that need to keep owning the submitted job record.
/// Dropping a token before completion has been observed is an invalid lifecycle
/// transition and fails fast.
pub struct GpuJobToken {
    name: &'static str,
    queue: QueueLane,
    fence: Option<Fence>,
    resident_command_buffers: [Option<CommandBuffer>; MAX_SUBMIT_COMMAND_BUFFERS],
    resident_command_buffer_count: usize,
    completion_observed: AtomicBool,
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
        let is_complete = self.fence().is_signaled()?;
        if is_complete {
            self.observe_completion();
        }
        Ok(is_complete)
    }

    pub fn wait(&self) -> VkResult<()> {
        crate::sync::diagnostics::record_gpu_job_wait(self.name, self.queue);
        self.fence().wait()?;
        self.observe_completion();
        Ok(())
    }

    pub fn wait_complete(mut self) -> VkResult<CompletedGpuJob> {
        crate::sync::diagnostics::record_gpu_job_wait(self.name, self.queue);
        if let Err(err) = self.fence().wait() {
            self.leak_after_completion_error();
            return Err(err);
        }
        self.observe_completion();
        Ok(self.into_completed())
    }

    pub fn complete_if_ready(mut self) -> VkResult<Result<CompletedGpuJob, Self>> {
        crate::sync::diagnostics::record_gpu_job_poll(self.name, self.queue);
        match self.fence().is_signaled() {
            Ok(true) => {
                self.observe_completion();
                Ok(Ok(self.into_completed()))
            }
            Ok(false) => Ok(Err(self)),
            Err(err) => {
                self.leak_after_completion_error();
                Err(err)
            }
        }
    }

    fn fence(&self) -> &Fence {
        self.fence
            .as_ref()
            .expect("managed GPU job fence disappeared before completion")
    }

    fn observe_completion(&self) {
        if !mark_completion_observed(&self.completion_observed) {
            return;
        }
        self.fence().mark_completed_for_reuse();
        crate::sync::diagnostics::record_gpu_job_completion(
            self.name,
            self.queue,
            self.resident_command_buffer_count,
        );
    }

    fn into_completed(mut self) -> CompletedGpuJob {
        debug_assert!(self.completion_observed.load(Ordering::Acquire));
        CompletedGpuJob {
            name: self.name,
            queue: self.queue,
            _fence: self
                .fence
                .take()
                .expect("managed GPU job fence disappeared before completion"),
        }
    }

    fn leak_after_completion_error(&mut self) {
        self.leak_pending_owners();
    }

    fn leak_pending_owners(&mut self) {
        leak_owner(&mut self.fence);
        for command_buffer in &mut self.resident_command_buffers {
            leak_owner(command_buffer);
        }
    }
}

impl Drop for GpuJobToken {
    fn drop(&mut self) {
        if self.fence.is_none() || self.completion_observed.load(Ordering::Acquire) {
            return;
        }

        fail_fast_invalid_abandonment(
            self.name,
            self.queue,
            &mut self.fence,
            &mut self.resident_command_buffers,
            self.resident_command_buffer_count,
        );
    }
}

#[cold]
#[inline(never)]
fn fail_fast_invalid_abandonment<F, C, const N: usize>(
    name: &'static str,
    queue: QueueLane,
    fence: &mut Option<F>,
    resident_command_buffers: &mut [Option<C>; N],
    resident_command_buffer_count: usize,
) -> ! {
    // Vulkan requires a submitted fence to remain alive until its submission
    // completes (VUID-vkDestroyFence-fence-01120), and a pending command
    // buffer cannot be freed (VUID-vkFreeCommandBuffers-pCommandBuffers-00047).
    // Leak both owners before panicking so unwinding cannot violate either
    // lifetime rule. Invalid abandonment is fatal; this is not recovery.
    leak_owner(fence);
    for command_buffer in resident_command_buffers {
        leak_owner(command_buffer);
    }
    crate::sync::diagnostics::record_gpu_job_invalid_abandonment(
        name,
        queue,
        resident_command_buffer_count,
    );
    panic!(
        "vkn managed GPU job '{}' was dropped before completion was observed; leaked its fence and {} resident command buffer(s) to preserve Vulkan pending-object lifetime rules",
        name, resident_command_buffer_count,
    );
}

fn leak_owner<T>(owner: &mut Option<T>) {
    if let Some(owner) = owner.take() {
        std::mem::forget(owner);
    }
}

fn mark_completion_observed(completion_observed: &AtomicBool) -> bool {
    !completion_observed.swap(true, Ordering::AcqRel)
}

/// Stateless entry point for vkn-managed GPU job submission.
///
/// A future implementation can replace this with pooled job slots without
/// changing builder/app call sites that hold `GpuJobToken`s.
pub struct GpuJobManager;

impl GpuJobManager {
    pub fn submit(device: &Device, queue: &Queue, desc: GpuJobDesc<'_>) -> VkResult<GpuJobToken> {
        assert!(
            desc.command_buffers.len() <= MAX_SUBMIT_COMMAND_BUFFERS,
            "managed GPU job '{}' has {} command buffers; max supported without allocation is {}",
            desc.name,
            desc.command_buffers.len(),
            MAX_SUBMIT_COMMAND_BUFFERS,
        );
        crate::sync::diagnostics::record_gpu_job_submit(&desc);
        let mut resident_command_buffers = std::array::from_fn(|_| None);
        for (resident, command_buffer) in resident_command_buffers
            .iter_mut()
            .zip(desc.command_buffers.iter())
        {
            *resident = Some((*command_buffer).clone());
        }
        let resident_command_buffer_count = desc.command_buffers.len();
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
            fence: Some(fence),
            resident_command_buffers,
            resident_command_buffer_count,
            completion_observed: AtomicBool::new(false),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{fail_fast_invalid_abandonment, mark_completion_observed, QueueLane};
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    };

    struct DropProbe(Arc<AtomicUsize>);

    impl Drop for DropProbe {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn invalid_abandonment_panics_after_leaking_every_pending_owner() {
        let drop_count = Arc::new(AtomicUsize::new(0));
        let mut fence = Some(DropProbe(drop_count.clone()));
        let mut command_buffers = [
            Some(DropProbe(drop_count.clone())),
            Some(DropProbe(drop_count.clone())),
        ];

        let panic = catch_unwind(AssertUnwindSafe(|| {
            fail_fast_invalid_abandonment(
                "test.pending",
                QueueLane::General,
                &mut fence,
                &mut command_buffers,
                2,
            )
        }));

        assert!(panic.is_err());
        assert!(fence.is_none());
        assert!(command_buffers.iter().all(Option::is_none));
        assert_eq!(drop_count.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn completion_is_observed_exactly_once() {
        let completion_observed = AtomicBool::new(false);

        assert!(mark_completion_observed(&completion_observed));
        assert!(!mark_completion_observed(&completion_observed));
    }
}
