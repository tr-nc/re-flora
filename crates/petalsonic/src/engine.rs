use crate::acoustic_propagation::{AcousticResponse, AcousticVoiceInput};
use crate::config::PetalSonicWorldDesc;
use crate::domain::{BusParams, SpatialFrame, VoiceId};
use crate::error::Result;
use crate::events::{
    PetalSonicEvent, RenderTimingEvent, RuntimeCounters, RuntimeState, VoiceTelemetryDiagnostics,
    VoiceTelemetryEvent,
};
use crate::output_session::OutputSession;
#[cfg(test)]
use crate::output_session::RenderWorkerFaultInjector;
use crate::platform::output::{OutputPlatform, OutputRecoveryRequest, OutputRecoveryResult};
use crate::playback::PlaybackCommand;
use crate::realtime_latest::RealtimeConsumer;
use crate::render::{PreparedRender, RenderQuantum, VoiceRetirement};
use crate::runtime_health::RuntimeFailurePublisher;
use crossbeam_channel::{Receiver, Sender};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

pub(crate) struct EngineObservability {
    frames_processed: Arc<AtomicUsize>,
    underrun_count: Arc<AtomicUsize>,
    active_device_name: Arc<Mutex<Option<String>>>,
    event_receiver: Receiver<PetalSonicEvent>,
    voice_telemetry_receiver: Receiver<VoiceTelemetryEvent>,
    timing_receiver: Receiver<RenderTimingEvent>,
    counters: Arc<RuntimeCounters>,
}

impl EngineObservability {
    pub(crate) fn counters(&self) -> &RuntimeCounters {
        &self.counters
    }

    pub(crate) fn frames_processed(&self) -> usize {
        self.frames_processed.load(Ordering::Relaxed)
    }

    pub(crate) fn underrun_count(&self) -> usize {
        self.underrun_count.load(Ordering::Relaxed)
    }

    pub(crate) fn active_device_name(&self) -> Option<String> {
        self.active_device_name
            .lock()
            .map(|device| device.clone())
            .unwrap_or_default()
    }

    pub(crate) fn event_queue_depth(&self) -> usize {
        self.event_receiver.len()
    }

    pub(crate) fn timing_queue_depth(&self) -> usize {
        self.timing_receiver.len()
    }

    pub(crate) fn drain_events(&self) -> Vec<PetalSonicEvent> {
        self.event_receiver.try_iter().collect()
    }

    pub(crate) fn drain_voice_telemetry(&self) -> Vec<VoiceTelemetryEvent> {
        self.voice_telemetry_receiver.try_iter().collect()
    }

    pub(crate) fn voice_telemetry_diagnostics(&self) -> VoiceTelemetryDiagnostics {
        VoiceTelemetryDiagnostics {
            queue_depth: self.voice_telemetry_receiver.len(),
            queue_high_water: self
                .counters
                .voice_telemetry_queue_high_water
                .load(Ordering::Relaxed),
            dropped_events: self
                .counters
                .dropped_voice_telemetry
                .load(Ordering::Relaxed),
        }
    }

    pub(crate) fn drain_timing_events(&self) -> Vec<RenderTimingEvent> {
        self.timing_receiver.try_iter().collect()
    }
}

/// A linear startup capability for one output-owned render engine.
///
/// Runtime can create and move this value, but only this module can inspect or split its
/// render/output ownership. `PetalSonicEngine::new_with_output` consumes it exactly once.
pub(crate) struct PreparedEngine {
    desc: PetalSonicWorldDesc,
    render: PreparedRender,
    frames_processed: Arc<AtomicUsize>,
    underrun_count: Arc<AtomicUsize>,
    active_device_name: Arc<Mutex<Option<String>>>,
    event_sender: Sender<PetalSonicEvent>,
    counters: Arc<RuntimeCounters>,
}

impl PreparedEngine {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        desc: PetalSonicWorldDesc,
        active_voice_count: Arc<AtomicUsize>,
        retirement_sender: Sender<VoiceId>,
        spatial_frames: RealtimeConsumer<SpatialFrame>,
        acoustic_responses: RealtimeConsumer<AcousticResponse>,
        acoustic_voice_input: AcousticVoiceInput,
        acoustic_scene_version: Arc<AtomicU64>,
        environmental_acoustics_enabled: Arc<AtomicBool>,
        regular_commands: Receiver<PlaybackCommand>,
        lifecycle_commands: Receiver<PlaybackCommand>,
    ) -> (Self, EngineObservability) {
        let frames_processed = Arc::new(AtomicUsize::new(0));
        let underrun_count = Arc::new(AtomicUsize::new(0));
        let active_device_name = Arc::new(Mutex::new(None));
        let counters = Arc::new(RuntimeCounters::default());
        let (event_sender, event_receiver) = crossbeam_channel::bounded(desc.event_queue_capacity);
        let (voice_telemetry_sender, voice_telemetry_receiver) =
            crossbeam_channel::bounded(desc.event_queue_capacity);
        let (timing_sender, timing_receiver) =
            crossbeam_channel::bounded(desc.timing_queue_capacity);
        let render = PreparedRender::new(
            desc.clone(),
            active_voice_count,
            retirement_sender,
            spatial_frames,
            acoustic_responses,
            acoustic_voice_input,
            acoustic_scene_version,
            environmental_acoustics_enabled,
            regular_commands,
            lifecycle_commands,
            event_sender.clone(),
            voice_telemetry_sender,
            timing_sender,
            counters.clone(),
        );
        let observability = EngineObservability {
            frames_processed: frames_processed.clone(),
            underrun_count: underrun_count.clone(),
            active_device_name: active_device_name.clone(),
            event_receiver,
            voice_telemetry_receiver,
            timing_receiver,
            counters: counters.clone(),
        };
        (
            Self {
                desc,
                render,
                frames_processed,
                underrun_count,
                active_device_name,
                event_sender,
                counters,
            },
            observability,
        )
    }
}

/// Schedules logical stereo rendering and delegates physical output lifecycle.
pub(crate) struct PetalSonicEngine {
    output: OutputSession,
    event_sender: Sender<PetalSonicEvent>,
    counters: Arc<RuntimeCounters>,
    voice_retirements: VoiceRetirementSupervisor,
}

/// The only non-realtime owner allowed to destroy a complete retired Voice.
///
/// Ordinary render completion crosses the bounded channel. If that channel is full or gone,
/// `RenderQuantum` retains the same linear payload until output is quiesced, then this owner takes
/// it directly. Platform cleanup failure cannot bypass either path.
struct VoiceRetirementSupervisor {
    receiver: Option<Receiver<VoiceRetirement>>,
}

impl VoiceRetirementSupervisor {
    fn new(receiver: Receiver<VoiceRetirement>) -> Self {
        Self {
            receiver: Some(receiver),
        }
    }

    fn drain(&mut self) {
        let Some(receiver) = self.receiver.as_ref() else {
            return;
        };
        while receiver.try_recv().is_ok() {}
    }

    fn reclaim_after_output_quiesces(&mut self, output: &mut OutputSession) {
        self.drain();
        drop(output.take_quiesced_voice_retirements());
    }

    #[cfg(test)]
    fn disconnect(&mut self) {
        self.receiver = None;
    }
}

impl PetalSonicEngine {
    #[cfg(test)]
    pub(crate) fn advance_without_output_for_test(&mut self, elapsed: std::time::Duration) {
        self.output.advance_without_output_for_test(elapsed);
    }

    #[cfg(test)]
    pub(crate) fn prepare_logical_output_for_test(&mut self) -> Result<()> {
        self.output.prepare_logical_output_for_test()
    }

    #[cfg(test)]
    pub(crate) fn render_once_for_test(&mut self) {
        self.output.render_once_for_test();
    }

    #[cfg(test)]
    pub(crate) fn drain_logical_output_for_test(&mut self) {
        self.output.drain_logical_output_for_test();
    }

    #[cfg(test)]
    pub(crate) fn disconnect_voice_retirement_receiver_for_test(&mut self) {
        self.voice_retirements.disconnect();
    }

    pub(crate) fn new_with_output(
        startup: PreparedEngine,
        output: Box<dyn OutputPlatform>,
        buses: Vec<BusParams>,
        runtime_failure: RuntimeFailurePublisher,
        #[cfg(test)] render_worker_fault: RenderWorkerFaultInjector,
    ) -> Result<Self> {
        let PreparedEngine {
            desc,
            render,
            frames_processed,
            underrun_count,
            active_device_name: current_device_name,
            event_sender,
            counters,
        } = startup;
        let (retirement_sender, retirement_receiver) = crossbeam_channel::bounded(desc.max_voices);
        let render = Arc::new(Mutex::new(RenderQuantum::new(
            render,
            buses,
            retirement_sender,
        )?));
        Ok(Self {
            output: OutputSession::new(
                desc,
                output,
                render,
                frames_processed,
                underrun_count,
                current_device_name,
                counters.clone(),
                runtime_failure,
                #[cfg(test)]
                render_worker_fault,
            ),
            event_sender,
            counters,
            voice_retirements: VoiceRetirementSupervisor::new(retirement_receiver),
        })
    }

    pub(crate) fn drain_retired_voice_resources(&mut self) {
        self.voice_retirements.drain();
    }

    pub(crate) fn reconcile_output(
        &mut self,
        request: OutputRecoveryRequest,
    ) -> OutputRecoveryResult {
        self.output.reconcile(request)
    }

    pub(crate) fn close(&mut self) -> Result<()> {
        let close_result = self.output.close();
        self.voice_retirements
            .reclaim_after_output_quiesces(&mut self.output);
        close_result
    }

    pub(crate) fn emit_runtime_state(&self, state: RuntimeState) {
        if self
            .event_sender
            .try_send(PetalSonicEvent::RuntimeStateChanged(state))
            .is_ok()
        {
            RuntimeCounters::observe_high_water(
                &self.counters.event_queue_high_water,
                self.event_sender.len(),
            );
        } else {
            self.counters.dropped_events.fetch_add(1, Ordering::Relaxed);
        }
    }
}

impl Drop for PetalSonicEngine {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
