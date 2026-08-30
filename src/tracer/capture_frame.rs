use crate::ddgi::DdgiDebugView;
use anyhow::Result;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CaptureFrameIdentity(u64);

pub(crate) struct CaptureFramePlanner {
    next_identity: u64,
}

impl CaptureFramePlanner {
    pub(crate) fn new() -> Self {
        Self { next_identity: 1 }
    }

    pub(crate) fn plan(
        &mut self,
        effective_view: DdgiDebugView,
    ) -> (CaptureFrameIdentity, CaptureFramePlan) {
        let identity = CaptureFrameIdentity(self.next_identity);
        self.next_identity = self.next_identity.wrapping_add(1).max(1);
        (
            identity,
            CaptureFramePlan {
                identity,
                effective_view,
            },
        )
    }
}

pub(crate) struct CaptureFramePlan {
    identity: CaptureFrameIdentity,
    effective_view: DdgiDebugView,
}

pub(crate) struct CaptureBuffersReady {
    identity: CaptureFrameIdentity,
}

pub(crate) struct RenderedCaptureFrame {
    identity: CaptureFrameIdentity,
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

impl CaptureFramePlan {
    pub(super) fn publish_buffers(
        self,
        host: &mut impl CaptureBufferPublicationHost,
    ) -> Result<CaptureBuffersReady> {
        host.publish_capture_shading_view(CaptureShadingView(self.effective_view))?;
        Ok(CaptureBuffersReady {
            identity: self.identity,
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
        })
    }
}

impl RenderedCaptureFrame {
    pub(crate) fn identity(&self) -> CaptureFrameIdentity {
        self.identity
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
    pub(crate) fn record(&mut self, plan: CaptureFramePlan) -> Result<RenderedCaptureFrame> {
        let buffers_ready = plan.publish_buffers(self)?;
        buffers_ready.record_trace(self)
    }

    pub(crate) fn published_views(&self) -> &[DdgiDebugView] {
        &self.published_views
    }

    pub(crate) fn trace_records(&self) -> usize {
        self.trace_records
    }
}

::static_assertions::assert_not_impl_any!(CaptureFramePlanner: Clone, Copy);
::static_assertions::assert_not_impl_any!(CaptureFramePlan: Clone, Copy);
::static_assertions::assert_not_impl_any!(CaptureBuffersReady: Clone, Copy);
::static_assertions::assert_not_impl_any!(RenderedCaptureFrame: Clone, Copy);
::static_assertions::assert_not_impl_any!(CaptureShadingView: Clone, Copy);

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn owner_pipeline_publishes_the_planned_view_then_records_trace_once() {
        let mut planner = CaptureFramePlanner::new();
        let (identity, plan) = planner.plan(DdgiDebugView::ExactVisibility);
        let mut host = RecordingCaptureFrameHost::default();

        let rendered = host.record(plan).unwrap();

        assert_eq!(host.published_views(), &[DdgiDebugView::ExactVisibility]);
        assert_eq!(host.trace_records(), 1);
        assert_eq!(rendered.identity(), identity);
    }

    #[test]
    fn failed_buffer_publication_or_trace_recording_cannot_produce_the_next_token() {
        let mut planner = CaptureFramePlanner::new();
        let (_, failed_plan) = planner.plan(DdgiDebugView::Final);
        let mut failed_buffers = RecordingBufferHost {
            fail: true,
            ..RecordingBufferHost::default()
        };
        assert!(failed_plan.publish_buffers(&mut failed_buffers).is_err());

        let (_, plan) = planner.plan(DdgiDebugView::Final);
        let mut buffers = RecordingBufferHost::default();
        let buffers_ready = plan.publish_buffers(&mut buffers).unwrap();
        let mut failed_trace = RecordingTraceHost {
            fail: true,
            ..RecordingTraceHost::default()
        };
        assert!(buffers_ready.record_trace(&mut failed_trace).is_err());
    }
}
