//! Optional synchronization diagnostics hooks.
//!
//! The default build keeps these hooks as no-ops so the managed sync path has no
//! logging, allocation, timestamp queries, or GPU waits. The event shapes are
//! compiled only with the `sync_diagnostics` feature and are intended as the seam
//! for a future profiler/sink.

use crate::{GpuJobDesc, PresentDesc, QueueLane, SubmitDesc};

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

#[cfg(feature = "sync_diagnostics")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuJobSubmitDiagnostics {
    pub name: &'static str,
    pub queue: QueueLane,
    pub command_buffer_count: usize,
    pub wait_count: usize,
    pub signal_count: usize,
}

#[cfg(feature = "sync_diagnostics")]
impl GpuJobSubmitDiagnostics {
    fn from_desc(desc: &GpuJobDesc<'_>) -> Self {
        Self {
            name: desc.name,
            queue: desc.queue,
            command_buffer_count: desc.command_buffers.len(),
            wait_count: desc.waits.len(),
            signal_count: desc.signals.len(),
        }
    }
}

#[cfg(feature = "sync_diagnostics")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GpuJobProbeKind {
    Poll,
    Wait,
}

#[cfg(feature = "sync_diagnostics")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuJobProbeDiagnostics {
    pub name: &'static str,
    pub queue: QueueLane,
    pub kind: GpuJobProbeKind,
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
pub(crate) fn record_gpu_job_submit(desc: &GpuJobDesc<'_>) {
    let _job = GpuJobSubmitDiagnostics::from_desc(desc);
}

#[cfg(not(feature = "sync_diagnostics"))]
#[inline(always)]
pub(crate) fn record_gpu_job_submit(_desc: &GpuJobDesc<'_>) {}

#[cfg(feature = "sync_diagnostics")]
#[inline(always)]
pub(crate) fn record_gpu_job_poll(name: &'static str, queue: QueueLane) {
    let _poll = GpuJobProbeDiagnostics {
        name,
        queue,
        kind: GpuJobProbeKind::Poll,
    };
}

#[cfg(not(feature = "sync_diagnostics"))]
#[inline(always)]
pub(crate) fn record_gpu_job_poll(_name: &'static str, _queue: QueueLane) {}

#[cfg(feature = "sync_diagnostics")]
#[inline(always)]
pub(crate) fn record_gpu_job_wait(name: &'static str, queue: QueueLane) {
    let _wait = GpuJobProbeDiagnostics {
        name,
        queue,
        kind: GpuJobProbeKind::Wait,
    };
}

#[cfg(not(feature = "sync_diagnostics"))]
#[inline(always)]
pub(crate) fn record_gpu_job_wait(_name: &'static str, _queue: QueueLane) {}
