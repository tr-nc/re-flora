use crate::ddgi::{DdgiCaptureCheckpoint, DdgiDebugView};
use crate::util::TimeInfo;
use anyhow::{ensure, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RadianceCaptureCheckpoint {
    Baseline,
    R2NextFrame,
    R4NextFrame,
    Final,
}

impl RadianceCaptureCheckpoint {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::R2NextFrame => "r2-next-frame",
            Self::R4NextFrame => "r4-next-frame",
            Self::Final => "final",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RadianceCaptureRequest {
    pub(crate) checkpoint: RadianceCaptureCheckpoint,
    pub(crate) mutation_frame: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CaptureReadinessObservation {
    scene_ready: bool,
    ddgi: Option<DdgiCaptureCheckpoint>,
    radiance_request: Option<RadianceCaptureRequest>,
    inflight_target_revision: Option<u32>,
    inflight_checkpoint_ready: bool,
}

impl CaptureReadinessObservation {
    pub(crate) fn new(
        scene_ready: bool,
        ddgi: Option<DdgiCaptureCheckpoint>,
        radiance_request: Option<RadianceCaptureRequest>,
        inflight_target_revision: Option<u32>,
        inflight_checkpoint_ready: bool,
    ) -> Self {
        Self {
            scene_ready,
            ddgi,
            radiance_request,
            inflight_target_revision,
            inflight_checkpoint_ready,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ReadyCaptureCheckpoint {
    ddgi: DdgiCaptureCheckpoint,
    radiance_request: Option<RadianceCaptureRequest>,
    inflight_target_revision: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ArmedCheckpoint {
    checkpoint: ReadyCaptureCheckpoint,
    requested_view: DdgiDebugView,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum CaptureViewPhase {
    Disabled,
    WaitingForCheckpoint,
    Armed(ArmedCheckpoint),
    Recording,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CaptureFrameIdentity(u64);

struct PlannedCaptureFrame {
    identity: CaptureFrameIdentity,
    physical_frame_serial: u64,
    armed_checkpoint: Option<ArmedCheckpoint>,
}

pub(crate) struct CaptureCoordinator {
    requested_view: DdgiDebugView,
    phase: CaptureViewPhase,
    next_frame_identity: u64,
    next_readback_target_serial: u64,
    planned_frame: Option<PlannedCaptureFrame>,
}

pub(crate) struct CaptureFramePlan {
    identity: CaptureFrameIdentity,
    physical_frame_serial: u64,
    effective_view: DdgiDebugView,
}

pub(crate) struct CaptureBuffersReady {
    identity: CaptureFrameIdentity,
    physical_frame_serial: u64,
}

pub(crate) struct RenderedCaptureFrame {
    identity: CaptureFrameIdentity,
    physical_frame_serial: u64,
}

pub(crate) struct CaptureReadbackCandidate {
    rendered_frame: RenderedCaptureFrame,
    armed_checkpoint: ArmedCheckpoint,
}

pub(crate) trait CaptureReadbackTarget {
    fn capture_readback_byte_count(&self) -> u64;
}

pub(crate) struct CaptureReadbackPermit<T> {
    physical_frame_serial: u64,
    checkpoint: ReadyCaptureCheckpoint,
    target_serial: u64,
    target_byte_count: u64,
    target: T,
}

pub(super) struct CaptureReadbackPermitIdentity {
    pub(super) physical_frame_serial: u64,
    pub(super) checkpoint: DdgiCaptureCheckpoint,
    pub(super) radiance_request: Option<RadianceCaptureRequest>,
    pub(super) inflight_target_revision: Option<u32>,
    pub(super) target_serial: u64,
    pub(super) target_byte_count: u64,
}

pub(super) struct CaptureShadingView(DdgiDebugView);

impl CaptureShadingView {
    pub(super) fn as_u32(&self) -> u32 {
        self.0.as_u32()
    }
}

pub(super) trait CaptureBufferPublicationHost {
    fn publish_capture_shading_view(&mut self, view: CaptureShadingView) -> Result<()>;
}

pub(super) trait CaptureTraceRecordingHost {
    fn record_capture_trace_commands(&mut self) -> Result<()>;
}

impl CaptureCoordinator {
    pub(crate) fn new(enabled: bool, requested_view: DdgiDebugView) -> Self {
        Self {
            requested_view,
            phase: if enabled {
                CaptureViewPhase::WaitingForCheckpoint
            } else {
                CaptureViewPhase::Disabled
            },
            next_frame_identity: 1,
            next_readback_target_serial: 1,
            planned_frame: None,
        }
    }

    pub(crate) fn is_enabled(&self) -> bool {
        !matches!(self.phase, CaptureViewPhase::Disabled)
    }

    fn ready_checkpoint(
        observation: CaptureReadinessObservation,
    ) -> Option<ReadyCaptureCheckpoint> {
        if !observation.scene_ready || !observation.inflight_checkpoint_ready {
            return None;
        }
        Some(ReadyCaptureCheckpoint {
            ddgi: observation.ddgi?,
            radiance_request: observation.radiance_request,
            inflight_target_revision: observation.inflight_target_revision,
        })
    }

    fn armed_checkpoint(
        &self,
        observation: CaptureReadinessObservation,
    ) -> Option<ArmedCheckpoint> {
        Self::ready_checkpoint(observation).map(|checkpoint| ArmedCheckpoint {
            checkpoint,
            requested_view: self.requested_view,
        })
    }

    pub(crate) fn begin_frame(
        &mut self,
        time_info: &TimeInfo,
        observation: CaptureReadinessObservation,
    ) -> CaptureFramePlan {
        assert!(
            self.planned_frame.is_none(),
            "capture frame must finish before planning its successor"
        );
        let candidate = self.armed_checkpoint(observation);
        let (effective_view, armed_checkpoint) = match self.phase {
            CaptureViewPhase::Disabled
            | CaptureViewPhase::Recording
            | CaptureViewPhase::Complete => (self.requested_view, None),
            CaptureViewPhase::WaitingForCheckpoint | CaptureViewPhase::Armed(_) => {
                self.phase = candidate.map_or(
                    CaptureViewPhase::WaitingForCheckpoint,
                    CaptureViewPhase::Armed,
                );
                (
                    candidate.map_or(DdgiDebugView::Final, |armed| armed.requested_view),
                    candidate,
                )
            }
        };
        let identity = CaptureFrameIdentity(self.next_frame_identity);
        self.next_frame_identity = self.next_frame_identity.wrapping_add(1).max(1);
        let physical_frame_serial = time_info.total_frame_count();
        self.planned_frame = Some(PlannedCaptureFrame {
            identity,
            physical_frame_serial,
            armed_checkpoint,
        });
        CaptureFramePlan {
            identity,
            physical_frame_serial,
            effective_view,
        }
    }

    pub(crate) fn finish_frame(
        &mut self,
        rendered_frame: RenderedCaptureFrame,
        time_info: &TimeInfo,
        observation: CaptureReadinessObservation,
    ) -> Option<CaptureReadbackCandidate> {
        let planned_frame = self
            .planned_frame
            .take()
            .expect("capture frame must have been planned before it can finish");
        assert_eq!(
            planned_frame.identity, rendered_frame.identity,
            "rendered capture frame does not match the coordinator plan"
        );
        assert_eq!(
            planned_frame.physical_frame_serial, rendered_frame.physical_frame_serial,
            "rendered capture frame lost its planned physical frame"
        );

        let current_checkpoint = self.armed_checkpoint(observation);
        if rendered_frame.physical_frame_serial != time_info.total_frame_count() {
            self.phase = current_checkpoint.map_or(
                CaptureViewPhase::WaitingForCheckpoint,
                CaptureViewPhase::Armed,
            );
            return None;
        }
        if matches!(
            self.phase,
            CaptureViewPhase::Disabled | CaptureViewPhase::Recording | CaptureViewPhase::Complete
        ) {
            return None;
        }
        if let (
            CaptureViewPhase::Armed(phase_checkpoint),
            Some(frame_checkpoint),
            Some(current_checkpoint),
        ) = (
            self.phase,
            planned_frame.armed_checkpoint,
            current_checkpoint,
        ) {
            if phase_checkpoint == frame_checkpoint && frame_checkpoint == current_checkpoint {
                return Some(CaptureReadbackCandidate {
                    rendered_frame,
                    armed_checkpoint: current_checkpoint,
                });
            }
        }
        self.phase = current_checkpoint.map_or(
            CaptureViewPhase::WaitingForCheckpoint,
            CaptureViewPhase::Armed,
        );
        None
    }

    pub(crate) fn authorize_readback<T: CaptureReadbackTarget>(
        &mut self,
        candidate: CaptureReadbackCandidate,
        target: T,
    ) -> CaptureReadbackPermit<T> {
        assert_eq!(
            self.phase,
            CaptureViewPhase::Armed(candidate.armed_checkpoint),
            "readback candidate no longer matches the armed capture"
        );
        let target_serial = self.next_readback_target_serial;
        self.next_readback_target_serial = self.next_readback_target_serial.wrapping_add(1).max(1);
        self.phase = CaptureViewPhase::Recording;
        CaptureReadbackPermit {
            physical_frame_serial: candidate.rendered_frame.physical_frame_serial,
            checkpoint: candidate.armed_checkpoint.checkpoint,
            target_serial,
            target_byte_count: target.capture_readback_byte_count(),
            target,
        }
    }

    pub(crate) fn complete_recording(&mut self, sequence_complete: bool) -> bool {
        debug_assert_eq!(self.phase, CaptureViewPhase::Recording);
        self.phase = if sequence_complete {
            CaptureViewPhase::Complete
        } else {
            CaptureViewPhase::WaitingForCheckpoint
        };
        sequence_complete
    }
}

impl CaptureFramePlan {
    pub(super) fn publish_buffers(
        self,
        time_info: &TimeInfo,
        host: &mut impl CaptureBufferPublicationHost,
    ) -> Result<CaptureBuffersReady> {
        ensure!(
            self.physical_frame_serial == time_info.total_frame_count(),
            "capture buffer publication crossed physical frames: planned={} current={}",
            self.physical_frame_serial,
            time_info.total_frame_count(),
        );
        host.publish_capture_shading_view(CaptureShadingView(self.effective_view))?;
        Ok(CaptureBuffersReady {
            identity: self.identity,
            physical_frame_serial: self.physical_frame_serial,
        })
    }
}

impl CaptureBuffersReady {
    pub(super) fn record_trace(
        self,
        host: &mut impl CaptureTraceRecordingHost,
    ) -> Result<RenderedCaptureFrame> {
        host.record_capture_trace_commands()?;
        Ok(RenderedCaptureFrame {
            identity: self.identity,
            physical_frame_serial: self.physical_frame_serial,
        })
    }
}

impl CaptureReadbackCandidate {
    pub(crate) fn physical_frame_serial(&self) -> u64 {
        self.rendered_frame.physical_frame_serial
    }

    pub(crate) fn ddgi_checkpoint(&self) -> DdgiCaptureCheckpoint {
        self.armed_checkpoint.checkpoint.ddgi
    }

    pub(crate) fn radiance_request(&self) -> Option<RadianceCaptureRequest> {
        self.armed_checkpoint.checkpoint.radiance_request
    }

    pub(crate) fn inflight_target_revision(&self) -> Option<u32> {
        self.armed_checkpoint.checkpoint.inflight_target_revision
    }

    pub(crate) fn requested_view(&self) -> DdgiDebugView {
        self.armed_checkpoint.requested_view
    }
}

impl<T> CaptureReadbackPermit<T> {
    pub(super) fn into_parts(self) -> (T, CaptureReadbackPermitIdentity) {
        (
            self.target,
            CaptureReadbackPermitIdentity {
                physical_frame_serial: self.physical_frame_serial,
                checkpoint: self.checkpoint.ddgi,
                radiance_request: self.checkpoint.radiance_request,
                inflight_target_revision: self.checkpoint.inflight_target_revision,
                target_serial: self.target_serial,
                target_byte_count: self.target_byte_count,
            },
        )
    }
}

#[cfg(test)]
#[derive(Default)]
pub(crate) struct RecordingCaptureFrameHost {
    published_views: Vec<DdgiDebugView>,
    trace_records: usize,
}

#[cfg(test)]
impl CaptureBufferPublicationHost for RecordingCaptureFrameHost {
    fn publish_capture_shading_view(&mut self, view: CaptureShadingView) -> Result<()> {
        self.published_views.push(view.0);
        Ok(())
    }
}

#[cfg(test)]
impl CaptureTraceRecordingHost for RecordingCaptureFrameHost {
    fn record_capture_trace_commands(&mut self) -> Result<()> {
        self.trace_records += 1;
        Ok(())
    }
}

#[cfg(test)]
impl RecordingCaptureFrameHost {
    pub(crate) fn record(
        &mut self,
        plan: CaptureFramePlan,
        time_info: &TimeInfo,
    ) -> Result<RenderedCaptureFrame> {
        let buffers_ready = plan.publish_buffers(time_info, self)?;
        buffers_ready.record_trace(self)
    }

    pub(crate) fn published_views(&self) -> &[DdgiDebugView] {
        &self.published_views
    }

    pub(crate) fn trace_records(&self) -> usize {
        self.trace_records
    }
}

::static_assertions::assert_not_impl_any!(CaptureCoordinator: Clone, Copy);
::static_assertions::assert_not_impl_any!(CaptureFramePlan: Clone, Copy);
::static_assertions::assert_not_impl_any!(CaptureBuffersReady: Clone, Copy);
::static_assertions::assert_not_impl_any!(RenderedCaptureFrame: Clone, Copy);
::static_assertions::assert_not_impl_any!(CaptureReadbackCandidate: Clone, Copy);
::static_assertions::assert_not_impl_any!(CaptureReadbackPermit<()>: Clone, Copy);
::static_assertions::assert_not_impl_any!(CaptureShadingView: Clone, Copy);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ddgi::{
        DdgiAtlasValidationStats, DdgiBatchOrder, DdgiBuildKind, DdgiBuildToken,
        DdgiCapturePublication, DdgiFieldIdentity, DdgiFieldKey, DdgiFieldState,
    };

    #[derive(Default)]
    struct RecordingBufferHost {
        views: Vec<u32>,
        fail: bool,
    }

    impl CaptureBufferPublicationHost for RecordingBufferHost {
        fn publish_capture_shading_view(&mut self, view: CaptureShadingView) -> Result<()> {
            if self.fail {
                anyhow::bail!("buffer publication failed");
            }
            self.views.push(view.as_u32());
            Ok(())
        }
    }

    #[derive(Default)]
    struct RecordingTraceHost {
        records: usize,
        fail: bool,
    }

    impl CaptureTraceRecordingHost for RecordingTraceHost {
        fn record_capture_trace_commands(&mut self) -> Result<()> {
            if self.fail {
                anyhow::bail!("trace recording failed");
            }
            self.records += 1;
            Ok(())
        }
    }

    #[derive(Debug, PartialEq, Eq)]
    struct RecordingTarget {
        byte_count: u64,
    }

    impl CaptureReadbackTarget for RecordingTarget {
        fn capture_readback_byte_count(&self) -> u64 {
            self.byte_count
        }
    }

    fn checkpoint(serial: u64) -> DdgiCaptureCheckpoint {
        let field = DdgiFieldIdentity::new(
            DdgiFieldKey::new(serial, 41, 17, 16, DdgiFieldState::Converged, 6).unwrap(),
            Some(DdgiFieldKey::new(serial - 1, 41, 17, 16, DdgiFieldState::Converging, 5).unwrap()),
        )
        .unwrap();
        DdgiCaptureCheckpoint {
            build_token: DdgiBuildToken::for_test(serial + 1_000, 41, 16, DdgiBuildKind::Terrain),
            field,
            validation: DdgiAtlasValidationStats {
                max_absolute_rgb_delta: 0.01,
                max_relative_rgb_delta: 0.02,
                max_rgb_value: 1.0,
                non_finite_count: 0,
                negative_rgb_texel_count: 0,
                valid_texel_count: 42,
                scanned_stored_texel_count: 64,
            },
            publication: DdgiCapturePublication::Published,
            batch_order: DdgiBatchOrder::Forward,
        }
    }

    fn ready(serial: u64) -> CaptureReadinessObservation {
        CaptureReadinessObservation::new(true, Some(checkpoint(serial)), None, None, true)
    }

    fn rendered(
        coordinator: &mut CaptureCoordinator,
        time_info: &TimeInfo,
        observation: CaptureReadinessObservation,
    ) -> (DdgiDebugView, RenderedCaptureFrame) {
        let plan = coordinator.begin_frame(time_info, observation);
        let mut host = RecordingCaptureFrameHost::default();
        let rendered = host.record(plan, time_info).unwrap();
        let [view] = host.published_views() else {
            panic!("one capture view must be published");
        };
        (*view, rendered)
    }

    #[test]
    fn coordinator_is_the_only_planner_and_publishes_the_selected_view_before_trace() {
        let time_info = TimeInfo::default();
        let mut coordinator = CaptureCoordinator::new(true, DdgiDebugView::ExactVisibility);

        let (view, rendered) = rendered(&mut coordinator, &time_info, ready(89));

        assert_eq!(view, DdgiDebugView::ExactVisibility);
        assert!(coordinator
            .finish_frame(rendered, &time_info, ready(89))
            .is_some());
    }

    #[test]
    fn buffer_publication_rejects_a_plan_delayed_to_another_physical_frame() {
        let mut time_info = TimeInfo::default();
        let mut coordinator = CaptureCoordinator::new(false, DdgiDebugView::Final);
        let plan = coordinator.begin_frame(&time_info, ready(89));
        time_info.update(false);
        let mut buffers = RecordingBufferHost::default();

        assert!(plan.publish_buffers(&time_info, &mut buffers).is_err());
        assert!(buffers.views.is_empty());
    }

    #[test]
    fn coordinator_rejects_a_rendered_token_delayed_to_another_physical_frame() {
        let mut time_info = TimeInfo::default();
        let mut coordinator = CaptureCoordinator::new(true, DdgiDebugView::ExactIrradiance);
        let (_, rendered) = rendered(&mut coordinator, &time_info, ready(89));
        time_info.update(false);

        assert!(coordinator
            .finish_frame(rendered, &time_info, ready(89))
            .is_none());
    }

    #[test]
    fn same_frame_checkpoint_validation_issues_a_target_bound_readback_permit() {
        let time_info = TimeInfo::default();
        let mut coordinator = CaptureCoordinator::new(true, DdgiDebugView::ExactVisibility);
        let (_, rendered) = rendered(&mut coordinator, &time_info, ready(89));
        let candidate = coordinator
            .finish_frame(rendered, &time_info, ready(89))
            .unwrap();

        let permit = coordinator.authorize_readback(candidate, RecordingTarget { byte_count: 512 });
        let (target, identity) = permit.into_parts();

        assert_eq!(target, RecordingTarget { byte_count: 512 });
        assert_eq!(
            identity.physical_frame_serial,
            time_info.total_frame_count()
        );
        assert_eq!(identity.checkpoint, checkpoint(89));
        assert_eq!(identity.target_serial, 1);
        assert_eq!(identity.target_byte_count, 512);
    }

    #[test]
    fn failed_buffer_publication_or_trace_recording_cannot_produce_the_next_token() {
        let time_info = TimeInfo::default();
        let mut coordinator = CaptureCoordinator::new(false, DdgiDebugView::Final);
        let failed_plan = coordinator.begin_frame(&time_info, ready(89));
        let mut failed_buffers = RecordingBufferHost {
            fail: true,
            ..RecordingBufferHost::default()
        };
        assert!(failed_plan
            .publish_buffers(&time_info, &mut failed_buffers)
            .is_err());

        coordinator.planned_frame = None;
        let plan = coordinator.begin_frame(&time_info, ready(89));
        let mut buffers = RecordingBufferHost::default();
        let buffers_ready = plan.publish_buffers(&time_info, &mut buffers).unwrap();
        let mut failed_trace = RecordingTraceHost {
            fail: true,
            ..RecordingTraceHost::default()
        };
        assert!(buffers_ready.record_trace(&mut failed_trace).is_err());
    }
}
