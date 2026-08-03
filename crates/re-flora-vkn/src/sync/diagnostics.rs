//! Optional synchronization diagnostics hooks.
//!
//! The default build keeps these hooks as no-ops so the managed sync path has no
//! logging, allocation, timestamp queries, or GPU waits. The event shapes are
//! compiled only with the `sync_diagnostics` feature and are intended as the seam
//! for a future profiler/sink.

use crate::{
    FrameCompletion, FrameResourceGeneration, FrameSubmissionId, PresentDesc, QueueLane,
    SubmitDesc, TextureTransition,
};
use ash::vk;
#[cfg(feature = "sync_diagnostics")]
use std::sync::atomic::{AtomicBool, Ordering};

pub type TextureTransitionDiagnosticsSink = fn(TextureTransitionDiagnostics);

pub type BufferTransitionDiagnosticsSink = fn(BufferTransitionDiagnostics);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextureTransitionDiagnostics {
    pub image: vk::Image,
    pub old_state: crate::ResourceState,
    pub new_state: crate::ResourceState,
    pub aspect_mask: vk::ImageAspectFlags,
    pub base_array_layer: u32,
    pub layer_count: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BufferTransitionDiagnostics {
    pub buffer: vk::Buffer,
    pub old_state: crate::BufferState,
    pub new_state: crate::BufferState,
}

#[cfg(feature = "sync_diagnostics")]
static TEXTURE_TRANSITION_SINK: std::sync::OnceLock<TextureTransitionDiagnosticsSink> =
    std::sync::OnceLock::new();

#[cfg(feature = "sync_diagnostics")]
static BUFFER_TRANSITION_SINK: std::sync::OnceLock<BufferTransitionDiagnosticsSink> =
    std::sync::OnceLock::new();

#[cfg(feature = "sync_diagnostics")]
static TEXTURE_TRANSITION_LOGGING_ENABLED: AtomicBool = AtomicBool::new(false);

#[cfg(feature = "sync_diagnostics")]
pub fn set_texture_transition_diagnostics_sink(sink: TextureTransitionDiagnosticsSink) -> bool {
    TEXTURE_TRANSITION_SINK.set(sink).is_ok()
}

#[cfg(not(feature = "sync_diagnostics"))]
pub fn set_texture_transition_diagnostics_sink(_sink: TextureTransitionDiagnosticsSink) -> bool {
    false
}

#[cfg(feature = "sync_diagnostics")]
pub fn set_buffer_transition_diagnostics_sink(sink: BufferTransitionDiagnosticsSink) -> bool {
    BUFFER_TRANSITION_SINK.set(sink).is_ok()
}

#[cfg(not(feature = "sync_diagnostics"))]
pub fn set_buffer_transition_diagnostics_sink(_sink: BufferTransitionDiagnosticsSink) -> bool {
    false
}

#[cfg(feature = "sync_diagnostics")]
pub fn set_texture_transition_logging_enabled(enabled: bool) -> bool {
    TEXTURE_TRANSITION_LOGGING_ENABLED.swap(enabled, Ordering::Relaxed)
}

#[cfg(not(feature = "sync_diagnostics"))]
pub fn set_texture_transition_logging_enabled(_enabled: bool) -> bool {
    false
}

#[cfg(feature = "sync_diagnostics")]
pub fn texture_transition_logging_enabled() -> bool {
    TEXTURE_TRANSITION_LOGGING_ENABLED.load(Ordering::Relaxed)
}

#[cfg(not(feature = "sync_diagnostics"))]
pub fn texture_transition_logging_enabled() -> bool {
    false
}

#[cfg(feature = "sync_diagnostics")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SubmitDiagnostics {
    pub name: &'static str,
    pub command_buffer_count: usize,
    pub wait_count: usize,
    pub signal_count: usize,
    pub has_fence: bool,
}

#[cfg(feature = "sync_diagnostics")]
impl SubmitDiagnostics {
    fn from_desc(desc: &SubmitDesc<'_>) -> Self {
        Self {
            name: desc.name,
            command_buffer_count: desc.command_buffers.len(),
            wait_count: desc.waits.len(),
            signal_count: desc.signals.len(),
            has_fence: desc.fence.is_some(),
        }
    }
}

#[cfg(feature = "sync_diagnostics")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SubmitWaitDiagnostics {
    pub submit_name: &'static str,
    pub wait_name: &'static str,
    pub stage: crate::PipelineWaitStage,
}

#[cfg(feature = "sync_diagnostics")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SubmitSignalDiagnostics {
    pub submit_name: &'static str,
    pub signal_name: &'static str,
}

#[cfg(feature = "sync_diagnostics")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresentDiagnostics {
    pub name: &'static str,
    pub image_index: u32,
    pub wait_count: usize,
}

#[cfg(feature = "sync_diagnostics")]
impl PresentDiagnostics {
    fn from_desc(desc: &PresentDesc<'_>) -> Self {
        Self {
            name: desc.name,
            image_index: desc.image_index,
            wait_count: desc.waits.len(),
        }
    }
}

#[cfg(feature = "sync_diagnostics")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresentWaitDiagnostics {
    pub present_name: &'static str,
    pub wait_name: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuJobSubmitDiagnostics {
    pub name: &'static str,
    pub queue: QueueLane,
    pub command_buffer_count: usize,
    pub wait_count: usize,
    pub signal_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GpuJobProbeKind {
    Poll,
    Wait,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuJobProbeDiagnostics {
    pub name: &'static str,
    pub queue: QueueLane,
    pub kind: GpuJobProbeKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuJobCompletionDiagnostics {
    pub name: &'static str,
    pub queue: QueueLane,
    pub resident_command_buffer_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuJobInvalidAbandonmentDiagnostics {
    pub name: &'static str,
    pub queue: QueueLane,
    pub resident_command_buffer_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GpuJobDiagnostics {
    Submitted(GpuJobSubmitDiagnostics),
    Probed(GpuJobProbeDiagnostics),
    Completed(GpuJobCompletionDiagnostics),
    InvalidAbandonment(GpuJobInvalidAbandonmentDiagnostics),
}

pub type GpuJobDiagnosticsSink = fn(GpuJobDiagnostics);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameRetirementCompletion {
    NoSubmission,
    Frame(FrameCompletion),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameRetirementDiagnostics {
    Scheduled {
        resource: FrameResourceGeneration,
        retire_after: Option<FrameSubmissionId>,
    },
    Released {
        resource: FrameResourceGeneration,
        completion: FrameRetirementCompletion,
    },
}

pub type FrameRetirementDiagnosticsSink = fn(FrameRetirementDiagnostics);

#[cfg(feature = "sync_diagnostics")]
static GPU_JOB_DIAGNOSTICS_SINK: std::sync::OnceLock<GpuJobDiagnosticsSink> =
    std::sync::OnceLock::new();

#[cfg(feature = "sync_diagnostics")]
static FRAME_RETIREMENT_DIAGNOSTICS_SINK: std::sync::OnceLock<FrameRetirementDiagnosticsSink> =
    std::sync::OnceLock::new();

#[cfg(feature = "sync_diagnostics")]
pub fn set_gpu_job_diagnostics_sink(sink: GpuJobDiagnosticsSink) -> bool {
    GPU_JOB_DIAGNOSTICS_SINK.set(sink).is_ok()
}

#[cfg(not(feature = "sync_diagnostics"))]
pub fn set_gpu_job_diagnostics_sink(_sink: GpuJobDiagnosticsSink) -> bool {
    false
}

#[cfg(feature = "sync_diagnostics")]
pub fn set_frame_retirement_diagnostics_sink(sink: FrameRetirementDiagnosticsSink) -> bool {
    FRAME_RETIREMENT_DIAGNOSTICS_SINK.set(sink).is_ok()
}

#[cfg(not(feature = "sync_diagnostics"))]
pub fn set_frame_retirement_diagnostics_sink(_sink: FrameRetirementDiagnosticsSink) -> bool {
    false
}

#[cfg(feature = "sync_diagnostics")]
#[inline(always)]
fn emit_gpu_job_diagnostics(event: GpuJobDiagnostics) {
    if let Some(sink) = GPU_JOB_DIAGNOSTICS_SINK.get() {
        sink(event);
    }
}

#[cfg(feature = "sync_diagnostics")]
#[inline(always)]
fn emit_frame_retirement_diagnostics(event: FrameRetirementDiagnostics) {
    if let Some(sink) = FRAME_RETIREMENT_DIAGNOSTICS_SINK.get() {
        sink(event);
    }
}

#[cfg(feature = "sync_diagnostics")]
#[inline(always)]
pub(crate) fn record_frame_retirement_scheduled(
    resource: FrameResourceGeneration,
    retire_after: Option<FrameSubmissionId>,
) {
    emit_frame_retirement_diagnostics(FrameRetirementDiagnostics::Scheduled {
        resource,
        retire_after,
    });
}

#[cfg(not(feature = "sync_diagnostics"))]
#[inline(always)]
pub(crate) fn record_frame_retirement_scheduled(
    _resource: FrameResourceGeneration,
    _retire_after: Option<FrameSubmissionId>,
) {
}

#[cfg(feature = "sync_diagnostics")]
#[inline(always)]
pub(crate) fn record_frame_retirement_released(
    resource: FrameResourceGeneration,
    completion: FrameRetirementCompletion,
) {
    emit_frame_retirement_diagnostics(FrameRetirementDiagnostics::Released {
        resource,
        completion,
    });
}

#[cfg(not(feature = "sync_diagnostics"))]
#[inline(always)]
pub(crate) fn record_frame_retirement_released(
    _resource: FrameResourceGeneration,
    _completion: FrameRetirementCompletion,
) {
}

#[cfg(feature = "sync_diagnostics")]
#[inline(always)]
pub(crate) fn record_submit(desc: &SubmitDesc<'_>) {
    let _submit = SubmitDiagnostics::from_desc(desc);
    for wait in desc.waits {
        let _wait = SubmitWaitDiagnostics {
            submit_name: desc.name,
            wait_name: wait.name,
            stage: wait.stage,
        };
    }
    for signal in desc.signals {
        let _signal = SubmitSignalDiagnostics {
            submit_name: desc.name,
            signal_name: signal.name,
        };
    }
}

#[cfg(not(feature = "sync_diagnostics"))]
#[inline(always)]
pub(crate) fn record_submit(_desc: &SubmitDesc<'_>) {}

#[cfg(feature = "sync_diagnostics")]
#[inline(always)]
pub(crate) fn record_present(desc: &PresentDesc<'_>) {
    let _present = PresentDiagnostics::from_desc(desc);
    for wait in desc.waits {
        let _wait = PresentWaitDiagnostics {
            present_name: desc.name,
            wait_name: wait.name,
        };
    }
}

#[cfg(not(feature = "sync_diagnostics"))]
#[inline(always)]
pub(crate) fn record_present(_desc: &PresentDesc<'_>) {}

#[cfg(feature = "sync_diagnostics")]
#[inline(always)]
pub(crate) fn record_gpu_job_submit(
    name: &'static str,
    queue: QueueLane,
) {
    emit_gpu_job_diagnostics(GpuJobDiagnostics::Submitted(GpuJobSubmitDiagnostics {
        name,
        queue,
        command_buffer_count: 1,
        wait_count: 0,
        signal_count: 0,
    }));
}

#[cfg(not(feature = "sync_diagnostics"))]
#[inline(always)]
pub(crate) fn record_gpu_job_submit(_name: &'static str, _queue: QueueLane) {}

#[cfg(feature = "sync_diagnostics")]
#[inline(always)]
pub(crate) fn record_gpu_job_poll(name: &'static str, queue: QueueLane) {
    emit_gpu_job_diagnostics(GpuJobDiagnostics::Probed(GpuJobProbeDiagnostics {
        name,
        queue,
        kind: GpuJobProbeKind::Poll,
    }));
}

#[cfg(not(feature = "sync_diagnostics"))]
#[inline(always)]
pub(crate) fn record_gpu_job_poll(_name: &'static str, _queue: QueueLane) {}

#[cfg(feature = "sync_diagnostics")]
#[inline(always)]
pub(crate) fn record_gpu_job_wait(name: &'static str, queue: QueueLane) {
    emit_gpu_job_diagnostics(GpuJobDiagnostics::Probed(GpuJobProbeDiagnostics {
        name,
        queue,
        kind: GpuJobProbeKind::Wait,
    }));
}

#[cfg(not(feature = "sync_diagnostics"))]
#[inline(always)]
pub(crate) fn record_gpu_job_wait(_name: &'static str, _queue: QueueLane) {}

#[cfg(feature = "sync_diagnostics")]
#[inline(always)]
pub(crate) fn record_gpu_job_completion(
    name: &'static str,
    queue: QueueLane,
    resident_command_buffer_count: usize,
) {
    emit_gpu_job_diagnostics(GpuJobDiagnostics::Completed(GpuJobCompletionDiagnostics {
        name,
        queue,
        resident_command_buffer_count,
    }));
}

#[cfg(not(feature = "sync_diagnostics"))]
#[inline(always)]
pub(crate) fn record_gpu_job_completion(
    _name: &'static str,
    _queue: QueueLane,
    _resident_command_buffer_count: usize,
) {
}

#[cfg(feature = "sync_diagnostics")]
#[inline(always)]
pub(crate) fn record_gpu_job_invalid_abandonment(
    name: &'static str,
    queue: QueueLane,
    resident_command_buffer_count: usize,
) {
    emit_gpu_job_diagnostics(GpuJobDiagnostics::InvalidAbandonment(
        GpuJobInvalidAbandonmentDiagnostics {
            name,
            queue,
            resident_command_buffer_count,
        },
    ));
}

#[cfg(not(feature = "sync_diagnostics"))]
#[inline(always)]
pub(crate) fn record_gpu_job_invalid_abandonment(
    _name: &'static str,
    _queue: QueueLane,
    _resident_command_buffer_count: usize,
) {
}

#[cfg(feature = "sync_diagnostics")]
#[inline(always)]
pub(crate) fn record_texture_transition(
    image: vk::Image,
    transition: TextureTransition,
    aspect_mask: vk::ImageAspectFlags,
    base_array_layer: u32,
    layer_count: u32,
) {
    let transition = TextureTransitionDiagnostics {
        image,
        old_state: transition.old_state(),
        new_state: transition.new_state(),
        aspect_mask,
        base_array_layer,
        layer_count,
    };
    if TEXTURE_TRANSITION_LOGGING_ENABLED.load(Ordering::Relaxed) {
        log::trace!(
            target: "re_flora_vkn::sync::texture_transition",
            "image={:?} aspect={:?} layers={}..{} {:?}->{:?}",
            transition.image,
            transition.aspect_mask,
            transition.base_array_layer,
            transition.base_array_layer + transition.layer_count,
            transition.old_state,
            transition.new_state,
        );
    }
    if let Some(sink) = TEXTURE_TRANSITION_SINK.get() {
        sink(transition);
    }
}

#[cfg(feature = "sync_diagnostics")]
#[inline(always)]
pub(crate) fn record_buffer_transition(
    buffer: vk::Buffer,
    old_state: crate::BufferState,
    new_state: crate::BufferState,
) {
    if let Some(sink) = BUFFER_TRANSITION_SINK.get() {
        sink(BufferTransitionDiagnostics {
            buffer,
            old_state,
            new_state,
        });
    }
}

#[cfg(not(feature = "sync_diagnostics"))]
#[inline(always)]
pub(crate) fn record_buffer_transition(
    _buffer: vk::Buffer,
    _old_state: crate::BufferState,
    _new_state: crate::BufferState,
) {
}

#[cfg(not(feature = "sync_diagnostics"))]
#[inline(always)]
pub(crate) fn record_texture_transition(
    _image: vk::Image,
    _transition: TextureTransition,
    _aspect_mask: vk::ImageAspectFlags,
    _base_array_layer: u32,
    _layer_count: u32,
) {
}

#[cfg(all(test, feature = "sync_diagnostics"))]
mod tests {
    use super::{
        record_gpu_job_completion, record_gpu_job_invalid_abandonment, record_gpu_job_submit,
        record_frame_retirement_released, record_frame_retirement_scheduled,
        set_frame_retirement_diagnostics_sink, set_gpu_job_diagnostics_sink,
        FrameRetirementCompletion, FrameRetirementDiagnostics, GpuJobDiagnostics,
    };
    use crate::{FrameCompletion, FrameResourceGeneration, FrameSubmissionId, QueueLane};
    use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

    static COMPLETED_RESIDENT_COUNT: AtomicUsize = AtomicUsize::new(usize::MAX);
    static ABANDONED_RESIDENT_COUNT: AtomicUsize = AtomicUsize::new(usize::MAX);
    static SUBMITTED_COMMAND_BUFFER_COUNT: AtomicUsize = AtomicUsize::new(usize::MAX);
    static RETIREMENT_SCHEDULED_GENERATION: AtomicU64 = AtomicU64::new(u64::MAX);
    static RETIREMENT_SCHEDULED_AFTER: AtomicU64 = AtomicU64::new(u64::MAX);
    static RETIREMENT_RELEASED_GENERATION: AtomicU64 = AtomicU64::new(u64::MAX);
    static RETIREMENT_RELEASED_SUBMISSION: AtomicU64 = AtomicU64::new(u64::MAX);
    static RETIREMENT_RELEASED_FRAME_SLOT: AtomicUsize = AtomicUsize::new(usize::MAX);
    static RETIREMENT_NAME_MATCHED: AtomicBool = AtomicBool::new(false);

    fn capture_gpu_job_event(event: GpuJobDiagnostics) {
        match event {
            GpuJobDiagnostics::Completed(event) => {
                COMPLETED_RESIDENT_COUNT
                    .store(event.resident_command_buffer_count, Ordering::Relaxed);
            }
            GpuJobDiagnostics::InvalidAbandonment(event) => {
                ABANDONED_RESIDENT_COUNT
                    .store(event.resident_command_buffer_count, Ordering::Relaxed);
            }
            GpuJobDiagnostics::Submitted(event) => {
                SUBMITTED_COMMAND_BUFFER_COUNT.store(event.command_buffer_count, Ordering::Relaxed);
            }
            GpuJobDiagnostics::Probed(_) => {}
        }
    }

    fn capture_frame_retirement_event(event: FrameRetirementDiagnostics) {
        match event {
            FrameRetirementDiagnostics::Scheduled {
                resource,
                retire_after,
            } => {
                RETIREMENT_SCHEDULED_GENERATION
                    .store(resource.generation(), Ordering::Relaxed);
                RETIREMENT_SCHEDULED_AFTER.store(
                    retire_after.map_or(u64::MAX, FrameSubmissionId::serial),
                    Ordering::Relaxed,
                );
                RETIREMENT_NAME_MATCHED.store(resource.name() == "test.buffer", Ordering::Relaxed);
            }
            FrameRetirementDiagnostics::Released {
                resource,
                completion: FrameRetirementCompletion::Frame(completion),
            } => {
                RETIREMENT_RELEASED_GENERATION.store(resource.generation(), Ordering::Relaxed);
                RETIREMENT_RELEASED_SUBMISSION
                    .store(completion.submission().serial(), Ordering::Relaxed);
                RETIREMENT_RELEASED_FRAME_SLOT
                    .store(completion.frame_slot(), Ordering::Relaxed);
            }
            FrameRetirementDiagnostics::Released {
                completion: FrameRetirementCompletion::NoSubmission,
                ..
            } => {}
        }
    }

    #[test]
    fn lifecycle_sink_observes_completion_and_invalid_abandonment() {
        assert!(set_gpu_job_diagnostics_sink(capture_gpu_job_event));

        record_gpu_job_submit("test.submit", QueueLane::General);
        record_gpu_job_completion("test.complete", QueueLane::General, 2);
        record_gpu_job_invalid_abandonment("test.abandon", QueueLane::General, 3);

        assert_eq!(SUBMITTED_COMMAND_BUFFER_COUNT.load(Ordering::Relaxed), 1);
        assert_eq!(COMPLETED_RESIDENT_COUNT.load(Ordering::Relaxed), 2);
        assert_eq!(ABANDONED_RESIDENT_COUNT.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn frame_retirement_sink_uses_semantic_generation_and_completion_identity() {
        assert!(set_frame_retirement_diagnostics_sink(
            capture_frame_retirement_event
        ));

        let resource = FrameResourceGeneration::new("test.buffer", 7);
        let submission = FrameSubmissionId::new(11);
        let completion = FrameCompletion::new(submission, 1);
        record_frame_retirement_scheduled(resource, Some(submission));
        record_frame_retirement_released(
            resource,
            FrameRetirementCompletion::Frame(completion),
        );

        assert_eq!(RETIREMENT_SCHEDULED_GENERATION.load(Ordering::Relaxed), 7);
        assert_eq!(RETIREMENT_SCHEDULED_AFTER.load(Ordering::Relaxed), 11);
        assert_eq!(RETIREMENT_RELEASED_GENERATION.load(Ordering::Relaxed), 7);
        assert_eq!(
            RETIREMENT_RELEASED_SUBMISSION.load(Ordering::Relaxed),
            11
        );
        assert_eq!(RETIREMENT_RELEASED_FRAME_SLOT.load(Ordering::Relaxed), 1);
        assert!(RETIREMENT_NAME_MATCHED.load(Ordering::Relaxed));
    }
}
