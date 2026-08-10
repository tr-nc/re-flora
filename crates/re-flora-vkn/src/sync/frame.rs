use crate::{
    CommandBuffer, CommandPool, Device, Fence, FrameCompletion, FrameRetirementClock,
    FrameRetirementSink, FrameSubmissionId, Semaphore, Swapchain, SwapchainFrameError,
    VulkanContext,
};

/// Per-frame synchronization and command recording resources.
///
/// This is intentionally small: it mirrors the existing frame-in-flight model
/// while moving ownership of the frame semaphore/fence/command-buffer bundle
/// into vkn. Higher-level frame acquisition and image-in-flight tracking will be
/// layered on top in later sync-model steps.
pub struct FrameSync {
    image_available: Semaphore,
    fence: Fence,
    command_buffer: CommandBuffer,
    submission: Option<FrameSubmissionId>,
}

impl FrameSync {
    pub fn new(device: &Device, command_pool: &CommandPool) -> Self {
        Self {
            image_available: Semaphore::new(device),
            fence: Fence::new(device, true),
            command_buffer: CommandBuffer::new(device, command_pool),
            submission: None,
        }
    }

    pub fn create_frames(device: &Device, command_pool: &CommandPool, count: usize) -> Vec<Self> {
        (0..count)
            .map(|_| Self::new(device, command_pool))
            .collect()
    }

    pub fn image_available(&self) -> &Semaphore {
        &self.image_available
    }

    pub fn fence(&self) -> &Fence {
        &self.fence
    }

    pub fn command_buffer(&self) -> &CommandBuffer {
        &self.command_buffer
    }
}

/// Acquired swapchain frame with the sync handles needed by the existing render
/// submission path.
///
/// Handles are cheap Arc-backed clones, so this token does not borrow the frame
/// manager and the app can continue to record through `Swapchain` normally.
pub struct AcquiredFrame {
    frame_slot: usize,
    image_index: u32,
    command_buffer: CommandBuffer,
    image_available: Semaphore,
    render_finished: Semaphore,
    fence: Fence,
    acquire_suboptimal: bool,
}

impl AcquiredFrame {
    pub fn frame_slot(&self) -> usize {
        self.frame_slot
    }

    pub fn image_index(&self) -> u32 {
        self.image_index
    }

    pub fn command_buffer(&self) -> &CommandBuffer {
        &self.command_buffer
    }

    pub fn wait_until_complete(&self) -> ash::prelude::VkResult<()> {
        self.fence.wait()
    }

    pub fn acquire_suboptimal(&self) -> bool {
        self.acquire_suboptimal
    }
}

/// Owns frame-in-flight resources and swapchain image-in-flight tracking.
pub struct SwapchainFrameManager {
    frames: Vec<FrameSync>,
    current_frame: usize,
    image_render_finished_semaphores: Vec<Semaphore>,
    images_in_flight: Vec<Option<Fence>>,
    retirement_clock: FrameRetirementClock,
}

impl SwapchainFrameManager {
    pub fn new(
        device: &Device,
        command_pool: &CommandPool,
        frames_in_flight: usize,
        swapchain_image_count: usize,
    ) -> Self {
        Self {
            frames: FrameSync::create_frames(device, command_pool, frames_in_flight),
            current_frame: 0,
            image_render_finished_semaphores: Self::create_present_semaphores(
                device,
                swapchain_image_count,
            ),
            images_in_flight: vec![None; swapchain_image_count],
            retirement_clock: FrameRetirementClock::new(),
        }
    }

    pub fn recreate_swapchain_images(&mut self, device: &Device, swapchain_image_count: usize) {
        self.image_render_finished_semaphores =
            Self::create_present_semaphores(device, swapchain_image_count);
        self.images_in_flight = vec![None; swapchain_image_count];
    }

    /// Wait for every frame submission currently owned by this manager and observe completions in
    /// queue order. This is the resize/shutdown quiescence seam; it deliberately avoids a
    /// device-wide idle wait so unrelated one-time work remains outside the frame lifecycle.
    pub fn wait_for_all_submissions(&mut self) {
        self.retirement_clock.schedule_pending_retirements();
        let mut completed = Vec::with_capacity(self.frames.len());
        for (frame_slot, sync) in self.frames.iter_mut().enumerate() {
            sync.fence().wait().unwrap();
            if let Some(submission) = sync.submission.take() {
                completed.push((submission, frame_slot));
            }
        }
        completed.sort_by_key(|(submission, _)| *submission);
        for (submission, frame_slot) in completed {
            self.retirement_clock
                .observe_completion(FrameCompletion::new(submission, frame_slot));
        }
    }

    pub fn begin_frame(
        &mut self,
        swapchain: &mut Swapchain,
    ) -> Result<AcquiredFrame, SwapchainFrameError> {
        self.retirement_clock.schedule_pending_retirements();
        let frame_slot = self.current_frame;
        let completed_submission = {
            let sync = &mut self.frames[frame_slot];
            sync.fence().wait().unwrap();
            sync.submission.take()
        };
        if let Some(submission) = completed_submission {
            self.retirement_clock
                .observe_completion(FrameCompletion::new(submission, frame_slot));
        }

        let sync = &self.frames[frame_slot];
        let (image_index, acquire_suboptimal) =
            swapchain.acquire_next_image(sync.image_available())?;
        let image_slot = image_index as usize;
        if let Some(image_in_flight_fence) = &self.images_in_flight[image_slot] {
            image_in_flight_fence.wait().unwrap();
        }
        self.images_in_flight[image_slot] = Some(sync.fence().clone());

        sync.fence().reset().expect("Failed to reset fences");

        Ok(AcquiredFrame {
            frame_slot,
            image_index,
            command_buffer: sync.command_buffer().clone(),
            image_available: sync.image_available().clone(),
            render_finished: self.image_render_finished_semaphores[image_slot].clone(),
            fence: sync.fence().clone(),
            acquire_suboptimal,
        })
    }

    pub fn submit_and_present(
        &mut self,
        vulkan_ctx: &VulkanContext,
        swapchain: &mut Swapchain,
        frame: &AcquiredFrame,
    ) -> Result<bool, SwapchainFrameError> {
        vulkan_ctx
            .submit_render_commands(
                &frame.command_buffer,
                &frame.image_available,
                &frame.render_finished,
                &frame.fence,
            )
            .map_err(SwapchainFrameError::from)?;

        let submission = self.retirement_clock.record_submission();
        let replaced = self.frames[frame.frame_slot].submission.replace(submission);
        assert!(
            replaced.is_none(),
            "frame slot submitted again before its previous completion was observed"
        );
        let present_result = swapchain.present_after(&frame.render_finished, frame.image_index);
        self.advance_frame();
        present_result.map(|present_suboptimal| frame.acquire_suboptimal() || present_suboptimal)
    }

    pub fn advance_frame(&mut self) {
        self.current_frame = (self.current_frame + 1) % self.frames.len();
    }

    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Obtain a same-thread publisher for frame-scoped resource generations.
    ///
    /// Published generations are bound by `begin_frame` or
    /// `wait_for_all_submissions`, so callers do not need to know the current
    /// submission identity or coordinate a producer-specific drain.
    pub fn retirement_sink(&self) -> FrameRetirementSink {
        self.retirement_clock.retirement_sink()
    }

    fn create_present_semaphores(device: &Device, image_count: usize) -> Vec<Semaphore> {
        (0..image_count).map(|_| Semaphore::new(device)).collect()
    }
}
