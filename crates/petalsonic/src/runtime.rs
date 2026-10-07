use crate::acoustic_propagation::AcousticPropagation;
use crate::acoustics::AcousticSceneSnapshot;
use crate::config::PetalSonicWorldDesc;
use crate::domain::{BusParams, Emitter, SpatialFrame, VoiceId};
use crate::engine::{EngineObservability, PetalSonicEngine, PreparedEngine};
use crate::error::{PetalSonicError, Result};
use crate::events::{
    AcousticTelemetryDiagnostics, AcousticTelemetryEvent, PetalSonicEvent, RenderTimingEvent,
    RuntimeCounters, RuntimeDiagnostics, RuntimeState, RuntimeStatus, VoiceTelemetryDiagnostics,
    VoiceTelemetryEvent,
};
#[cfg(test)]
use crate::output_session::RenderWorkerFaultInjector;
use crate::platform::output::{
    CpalOutputPlatform, OutputPlatform, OutputRecoveryRequest, OutputRecoveryResult,
};
use crate::playback::{AcceptedVoice, EmitterUpdate, PlaybackCommand};
use crate::realtime_latest::{Publisher, RealtimeLatest};
use crate::runtime_health::RuntimeFailurePublisher;
use crate::runtime_services::{ChildCancellation, RuntimeChildFailure, RuntimeServices};
use crossbeam_channel::{Receiver, Sender, TrySendError};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub(crate) const OUTPUT_RETRY_INTERVAL: Duration = Duration::from_millis(500);

/// A domain-validated discrete intent accepted by one World's runtime.
///
/// Queue selection and render-path preparation deliberately remain behind the
/// runtime seam; callers cannot acquire or route through the underlying endpoints.
// RuntimeIntent is converted immediately and is never stored. Boxing Play would add a control-
// path allocation solely to shrink this transient enum.
#[allow(clippy::large_enum_variant)]
pub(crate) enum RuntimeIntent {
    Play(AcceptedVoice),
    UpdateEmitter(EmitterUpdate),
    DestroyEmitter(Emitter),
    PauseEmitter(Emitter),
    ResumeEmitter(Emitter),
    StopEmitter(Emitter),
    SeekEmitter(Emitter, f32),
    PauseVoice(VoiceId),
    ResumeVoice(VoiceId),
    SetVoiceRate(VoiceId, f32),
    StopVoice(VoiceId),
    SeekVoice(VoiceId, f32),
    StopAll,
    UpdateBus(usize, BusParams),
}

impl RuntimeIntent {
    fn into_command(self, block_size: usize) -> PlaybackCommand {
        match self {
            Self::Play(voice) => PlaybackCommand::Play(voice.prepare(block_size)),
            Self::UpdateEmitter(update) => PlaybackCommand::UpdateEmitter(update),
            Self::DestroyEmitter(emitter) => PlaybackCommand::DestroyEmitter(emitter),
            Self::PauseEmitter(emitter) => PlaybackCommand::PauseEmitter(emitter),
            Self::ResumeEmitter(emitter) => PlaybackCommand::ResumeEmitter(emitter),
            Self::StopEmitter(emitter) => PlaybackCommand::StopEmitter(emitter),
            Self::SeekEmitter(emitter, progress) => PlaybackCommand::SeekEmitter(emitter, progress),
            Self::PauseVoice(voice) => PlaybackCommand::PauseVoice(voice),
            Self::ResumeVoice(voice) => PlaybackCommand::ResumeVoice(voice),
            Self::SetVoiceRate(voice, rate) => PlaybackCommand::SetVoiceRate(voice, rate),
            Self::StopVoice(voice) => PlaybackCommand::StopVoice(voice),
            Self::SeekVoice(voice, progress) => PlaybackCommand::SeekVoice(voice, progress),
            Self::StopAll => PlaybackCommand::StopAll,
            Self::UpdateBus(index, params) => PlaybackCommand::UpdateBus(index, params),
        }
    }
}

pub(crate) trait OutputRuntimeDriver {
    fn drain_retired_resources(&mut self);
    fn reconcile_output(&mut self, request: OutputRecoveryRequest) -> OutputRecoveryResult;
    fn emit_runtime_state(&self, state: RuntimeState);
}

impl OutputRuntimeDriver for PetalSonicEngine {
    fn drain_retired_resources(&mut self) {
        PetalSonicEngine::drain_retired_voice_resources(self);
    }

    fn reconcile_output(&mut self, request: OutputRecoveryRequest) -> OutputRecoveryResult {
        PetalSonicEngine::reconcile_output(self, request)
    }

    fn emit_runtime_state(&self, state: RuntimeState) {
        PetalSonicEngine::emit_runtime_state(self, state);
    }
}

pub(crate) struct SupervisorSchedule {
    next_retry: Instant,
    next_health_probe: Instant,
    last_advance: Instant,
}

#[derive(Clone, Debug)]
struct RuntimeCloseReport {
    failure: Option<String>,
}

impl RuntimeCloseReport {
    fn capture(result: Result<()>) -> Self {
        let failure = match result {
            Ok(()) => None,
            Err(PetalSonicError::Engine(detail)) => Some(detail),
            Err(error) => Some(error.to_string()),
        };
        Self { failure }
    }

    fn result(&self) -> Result<()> {
        match &self.failure {
            Some(detail) => Err(PetalSonicError::Engine(detail.clone())),
            None => Ok(()),
        }
    }
}

#[derive(Debug)]
enum RuntimeCloseState {
    Open,
    Closed(RuntimeCloseReport),
}

impl SupervisorSchedule {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            next_retry: now,
            next_health_probe: now,
            last_advance: now,
        }
    }
}

/// The active runtime owned by exactly one [`PetalSonicWorld`](crate::PetalSonicWorld).
///
/// This module owns all thread, endpoint, propagation, output-recovery, and shutdown
/// coordination. The World facade only validates domain intent and maintains caller-facing
/// handle registries.
pub(crate) struct AudioRuntime {
    block_size: usize,
    max_voices: usize,
    bus_params: Arc<Mutex<Vec<BusParams>>>,
    active_voice_count: Arc<AtomicUsize>,
    retirement_receiver: Receiver<VoiceId>,
    spatial_frames: Publisher<SpatialFrame>,
    spatial_frame_revision: AtomicU64,
    spatial_sim_time_bits: AtomicU64,
    acoustic_propagation: AcousticPropagation,
    acoustic_scene_version: Arc<AtomicU64>,
    environmental_acoustics_enabled: Arc<AtomicBool>,
    command_sender: Sender<PlaybackCommand>,
    lifecycle_sender: Sender<PlaybackCommand>,
    observability: EngineObservability,
    acoustic_telemetry_receiver: Receiver<AcousticTelemetryEvent>,
    runtime_state: Arc<AtomicU8>,
    recovery_attempts: Arc<AtomicU64>,
    services: RuntimeServices,
    #[cfg(test)]
    render_worker_fault: RenderWorkerFaultInjector,
    close_state: Mutex<RuntimeCloseState>,
}

impl AudioRuntime {
    pub(crate) fn start(config: &PetalSonicWorldDesc) -> Result<Self> {
        Self::start_with_output_factory(
            config,
            Box::new(|| Ok(Box::new(CpalOutputPlatform::new()?) as Box<dyn OutputPlatform>)),
        )
    }

    #[cfg(test)]
    pub(crate) fn start_with_output(
        config: &PetalSonicWorldDesc,
        output: impl FnOnce() -> Result<Box<dyn OutputPlatform>> + Send + 'static,
    ) -> Result<Self> {
        Self::start_with_output_factory(config, Box::new(output))
    }

    fn start_with_output_factory(
        config: &PetalSonicWorldDesc,
        output: Box<dyn FnOnce() -> Result<Box<dyn OutputPlatform>> + Send>,
    ) -> Result<Self> {
        let (command_sender, command_receiver) =
            crossbeam_channel::bounded(config.control_queue_capacity);
        let (lifecycle_sender, lifecycle_receiver) =
            crossbeam_channel::bounded(config.lifecycle_queue_capacity);
        let active_voice_count = Arc::new(AtomicUsize::new(0));
        let (retirement_sender, retirement_receiver) =
            crossbeam_channel::bounded(config.max_voices);
        let (spatial_frames, spatial_frame_consumer) = RealtimeLatest::bounded(1);
        let initial_acoustic_scene = config.acoustic_scene.clone().map(Arc::new);
        let acoustic_scene_version = Arc::new(AtomicU64::new(
            initial_acoustic_scene
                .as_ref()
                .map(|scene| scene.version())
                .unwrap_or(0),
        ));
        let environmental_acoustics_enabled =
            Arc::new(AtomicBool::new(config.environmental_acoustics_enabled));
        let runtime_state = Arc::new(AtomicU8::new(RuntimeState::Recovering as u8));
        let runtime_failure = RuntimeFailurePublisher::new(runtime_state.clone());
        let recovery_attempts = Arc::new(AtomicU64::new(0));
        let mut services = RuntimeServices::new(runtime_failure.clone());
        #[cfg(test)]
        let render_worker_fault = RenderWorkerFaultInjector::default();
        let (acoustic_propagation, acoustic_worker) = AcousticPropagation::prepare(
            config.distance_scaler,
            environmental_acoustics_enabled.clone(),
            config.environmental_acoustics_quality,
            config.environmental_acoustics_budget,
            config.max_voices,
            config.event_queue_capacity,
        );
        acoustic_worker.start(&mut services)?;
        let acoustic_telemetry_receiver = acoustic_propagation.telemetry_receiver();
        if let Some(scene) = initial_acoustic_scene {
            acoustic_propagation.publish_scene(scene).map_err(|_| {
                PetalSonicError::Engine("Failed to publish initial acoustic scene".into())
            })?;
        }
        let bus_params = Arc::new(Mutex::new(
            std::iter::once(BusParams::default())
                .chain(config.buses.iter().map(|bus| bus.params()))
                .collect::<Vec<_>>(),
        ));
        let (startup, observability) = PreparedEngine::new(
            config.clone(),
            active_voice_count.clone(),
            retirement_sender,
            spatial_frame_consumer,
            acoustic_propagation
                .take_response_consumer()
                .ok_or_else(|| {
                    PetalSonicError::Engine(
                        "Acoustic response consumer is already connected".into(),
                    )
                })?,
            acoustic_propagation.voice_input(),
            acoustic_scene_version.clone(),
            environmental_acoustics_enabled.clone(),
            command_receiver,
            lifecycle_receiver,
        );
        Self::start_output_child(
            &mut services,
            startup,
            output,
            bus_params.clone(),
            runtime_state.clone(),
            recovery_attempts.clone(),
            runtime_failure,
            #[cfg(test)]
            render_worker_fault.clone(),
        )?;

        Ok(Self {
            block_size: config.block_size,
            max_voices: config.max_voices,
            bus_params,
            active_voice_count,
            retirement_receiver,
            spatial_frames,
            spatial_frame_revision: AtomicU64::new(0),
            spatial_sim_time_bits: AtomicU64::new(0.0f64.to_bits()),
            acoustic_propagation,
            acoustic_scene_version,
            environmental_acoustics_enabled,
            command_sender,
            lifecycle_sender,
            observability,
            acoustic_telemetry_receiver,
            runtime_state,
            recovery_attempts,
            services,
            #[cfg(test)]
            render_worker_fault,
            close_state: Mutex::new(RuntimeCloseState::Open),
        })
    }

    pub(crate) fn ensure_open(&self) -> Result<()> {
        match RuntimeState::load(&self.runtime_state) {
            RuntimeState::Failed => Err(PetalSonicError::RuntimeFailed),
            RuntimeState::Closing | RuntimeState::Closed => Err(PetalSonicError::RuntimeClosed),
            _ => Ok(()),
        }
    }

    pub(crate) fn reserve_voice(&self) -> Result<()> {
        self.active_voice_count
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |active| {
                (active < self.max_voices).then_some(active + 1)
            })
            .map(|_| ())
            .map_err(|_| PetalSonicError::CapacityExceeded {
                resource: "voice",
                limit: self.max_voices,
            })
    }

    pub(crate) fn release_reserved_voice(&self) {
        self.active_voice_count.fetch_sub(1, Ordering::AcqRel);
    }

    pub(crate) fn try_submit(&self, intent: RuntimeIntent) -> Result<()> {
        self.ensure_open()?;
        let command = intent.into_command(self.block_size);
        let lifecycle = matches!(
            &command,
            PlaybackCommand::StopVoice(_)
                | PlaybackCommand::StopEmitter(_)
                | PlaybackCommand::DestroyEmitter(_)
                | PlaybackCommand::StopAll
        );
        let sender = if lifecycle {
            &self.lifecycle_sender
        } else {
            &self.command_sender
        };
        match sender.try_send(command) {
            Ok(()) => {
                let high_water = if lifecycle {
                    &self.observability.counters().lifecycle_queue_high_water
                } else {
                    &self.observability.counters().control_queue_high_water
                };
                RuntimeCounters::observe_high_water(high_water, sender.len());
                Ok(())
            }
            Err(TrySendError::Full(_)) => {
                self.observability
                    .counters()
                    .rejected_commands
                    .fetch_add(1, Ordering::Relaxed);
                Err(PetalSonicError::QueuePressure)
            }
            Err(TrySendError::Disconnected(_)) => {
                self.observability
                    .counters()
                    .rejected_commands
                    .fetch_add(1, Ordering::Relaxed);
                Err(PetalSonicError::RuntimeClosed)
            }
        }
    }

    pub(crate) fn spatial_cursor(&self) -> (u64, f64) {
        (
            self.spatial_frame_revision.load(Ordering::Acquire),
            f64::from_bits(self.spatial_sim_time_bits.load(Ordering::Acquire)),
        )
    }

    pub(crate) fn publish_spatial_frame(&self, frame: Arc<SpatialFrame>) -> Result<()> {
        self.ensure_open()?;
        let publication = self
            .spatial_frames
            .prepare_latest(frame.clone())
            .map_err(|_| PetalSonicError::QueuePressure)?;
        self.acoustic_propagation
            .publish_spatial_frame(frame.clone())
            .map_err(|_| PetalSonicError::QueuePressure)?;
        self.spatial_frame_revision
            .store(frame.revision(), Ordering::Release);
        self.spatial_sim_time_bits
            .store(frame.sim_time_seconds().to_bits(), Ordering::Release);
        publication.commit();
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn with_spatial_publication_blocked<T>(&self, operation: impl FnOnce() -> T) -> T {
        self.spatial_frames.with_publication_blocked(operation)
    }

    #[cfg(test)]
    pub(crate) fn acoustic_wake_generation_for_test(&self) -> u64 {
        self.acoustic_propagation.wake_generation_for_test()
    }

    #[cfg(test)]
    pub(crate) fn acoustic_voice_spatial_for_test(
        &self,
        voice_id: VoiceId,
    ) -> Option<(crate::math::Pose, f32, bool)> {
        self.acoustic_propagation.voice_spatial_for_test(voice_id)
    }

    #[cfg(test)]
    pub(crate) fn rendered_spatial_revision_for_test(&self) -> u64 {
        self.observability
            .counters()
            .rendered_spatial_revision
            .load(Ordering::Acquire)
    }

    pub(crate) fn acoustic_scene_version(&self) -> u64 {
        self.acoustic_scene_version.load(Ordering::Acquire)
    }

    pub(crate) fn publish_acoustic_scene(
        &self,
        snapshot: Arc<AcousticSceneSnapshot>,
    ) -> Result<()> {
        self.ensure_open()?;
        self.acoustic_propagation
            .publish_scene(snapshot.clone())
            .map_err(|_| PetalSonicError::QueuePressure)?;
        self.acoustic_scene_version
            .store(snapshot.version(), Ordering::Release);
        Ok(())
    }

    pub(crate) fn set_environmental_acoustics_enabled(&self, enabled: bool) -> Result<()> {
        self.ensure_open()?;
        self.acoustic_propagation.set_enabled(enabled);
        Ok(())
    }

    pub(crate) fn environmental_acoustics_enabled(&self) -> bool {
        self.environmental_acoustics_enabled.load(Ordering::Acquire)
    }

    pub(crate) fn set_environmental_acoustics_quality(&self, quality: f32) -> Result<()> {
        self.ensure_open()?;
        self.acoustic_propagation.set_quality(quality);
        Ok(())
    }

    pub(crate) fn environmental_acoustics_quality(&self) -> f32 {
        self.acoustic_propagation.quality()
    }

    #[cfg(test)]
    pub(crate) fn fail_acoustic_worker_for_test(&self) {
        self.acoustic_propagation.fail_worker_for_test();
    }

    #[cfg(test)]
    pub(crate) fn panic_acoustic_worker_for_test(&self) {
        self.acoustic_propagation.panic_worker_for_test();
    }

    #[cfg(test)]
    pub(crate) fn panic_render_worker_for_test(&self) {
        self.render_worker_fault.panic();
    }

    pub(crate) fn set_bus_params(&self, index: usize, params: BusParams) -> Result<()> {
        let mut current = self
            .bus_params
            .lock()
            .map_err(|_| PetalSonicError::Engine("Bus state is poisoned".into()))?;
        self.try_submit(RuntimeIntent::UpdateBus(index, params))?;
        current[index] = params;
        Ok(())
    }

    pub(crate) fn bus_params(&self, index: usize) -> Result<BusParams> {
        self.bus_params
            .lock()
            .map_err(|_| PetalSonicError::Engine("Bus state is poisoned".into()))?
            .get(index)
            .copied()
            .ok_or(PetalSonicError::StaleBus)
    }

    pub(crate) fn runtime_status(&self) -> RuntimeStatus {
        RuntimeStatus {
            state: RuntimeState::load(&self.runtime_state),
            recovery_attempts: self.recovery_attempts.load(Ordering::Relaxed),
            active_output_device: self.observability.active_device_name(),
        }
    }

    pub(crate) fn diagnostics(
        &self,
        active_emitters: usize,
        desc: &PetalSonicWorldDesc,
    ) -> RuntimeDiagnostics {
        let (
            render_iterations,
            render_time_p50_us,
            render_time_p95_us,
            render_time_p99_us,
            render_time_max_us,
        ) = self.observability.counters().render_summary();
        let counters = self.observability.counters();
        let acoustics = self.acoustic_propagation.diagnostics();
        RuntimeDiagnostics {
            frames_processed: self.observability.frames_processed(),
            underrun_count: self.observability.underrun_count(),
            active_emitters,
            active_voices: self.active_voice_count.load(Ordering::Acquire),
            control_queue_depth: self.command_sender.len(),
            control_queue_high_water: counters.control_queue_high_water.load(Ordering::Relaxed),
            lifecycle_queue_depth: self.lifecycle_sender.len(),
            lifecycle_queue_high_water: counters.lifecycle_queue_high_water.load(Ordering::Relaxed),
            event_queue_depth: self.observability.event_queue_depth(),
            event_queue_high_water: counters.event_queue_high_water.load(Ordering::Relaxed),
            timing_queue_depth: self.observability.timing_queue_depth(),
            timing_queue_high_water: counters.timing_queue_high_water.load(Ordering::Relaxed),
            rejected_commands: counters.rejected_commands.load(Ordering::Relaxed),
            dropped_events: counters.dropped_events.load(Ordering::Relaxed),
            dropped_timing_events: counters.dropped_timing_events.load(Ordering::Relaxed),
            render_iterations,
            render_time_p50_us,
            render_time_p95_us,
            render_time_p99_us,
            render_time_max_us,
            acoustic_solve_count: acoustics.solve_count,
            acoustic_superseded_solve_count: acoustics.superseded_solve_count,
            acoustic_published_response_count: acoustics.published_response_count,
            acoustic_response_spatial_revision: acoustics.latest_spatial_revision,
            acoustic_response_geometry_version: acoustics.latest_geometry_version,
            acoustic_last_solve_time_us: acoustics.last_solve_time_us,
            acoustic_solve_time_p50_us: acoustics.solve_time_p50_us,
            acoustic_solve_time_p95_us: acoustics.solve_time_p95_us,
            acoustic_solve_time_p99_us: acoustics.solve_time_p99_us,
            acoustic_solve_time_max_us: acoustics.solve_time_max_us,
            acoustic_response_age_ms: acoustics.response_age_ms,
            acoustic_direct_ray_count: acoustics.direct_ray_count,
            acoustic_sample_cache_hit_count: acoustics.cache_hit_count,
            acoustic_processed_extent_count: acoustics.processed_extent_count,
            acoustic_lobe_count: acoustics.lobe_count,
            acoustic_retained_response_count: acoustics.retained_response_count,
            acoustic_deferred_response_count: acoustics.deferred_response_count,
            acoustic_render_rejected_response_count: counters
                .acoustic_render_rejected_responses
                .load(Ordering::Relaxed),
            device_generation: counters.device_generation.load(Ordering::Relaxed),
            recovery_attempts: self.recovery_attempts.load(Ordering::Relaxed),
            output_sample_rate: counters.output_sample_rate.load(Ordering::Relaxed) as u32,
            output_channels: counters.output_channels.load(Ordering::Relaxed) as u16,
            spatial_quality: desc.spatial_quality,
            latency_profile: desc.latency_profile,
        }
    }

    pub(crate) fn active_voice_count(&self) -> usize {
        self.active_voice_count.load(Ordering::Acquire)
    }

    pub(crate) fn frames_processed(&self) -> usize {
        self.observability.frames_processed()
    }

    pub(crate) fn underrun_count(&self) -> usize {
        self.observability.underrun_count()
    }

    pub(crate) fn drain_events(&self) -> Vec<PetalSonicEvent> {
        self.observability.drain_events()
    }

    pub(crate) fn drain_voice_telemetry(&self) -> Vec<VoiceTelemetryEvent> {
        self.observability.drain_voice_telemetry()
    }

    pub(crate) fn voice_telemetry_diagnostics(&self) -> VoiceTelemetryDiagnostics {
        self.observability.voice_telemetry_diagnostics()
    }

    pub(crate) fn drain_acoustic_telemetry(&self) -> Vec<AcousticTelemetryEvent> {
        self.acoustic_telemetry_receiver.try_iter().collect()
    }

    pub(crate) fn acoustic_telemetry_diagnostics(&self) -> AcousticTelemetryDiagnostics {
        let (queue_high_water, dropped_events) = self.acoustic_propagation.telemetry_pressure();
        AcousticTelemetryDiagnostics {
            queue_depth: self.acoustic_telemetry_receiver.len(),
            queue_high_water,
            dropped_events,
        }
    }

    pub(crate) fn drain_timing_events(&self) -> Vec<RenderTimingEvent> {
        self.observability.drain_timing_events()
    }

    pub(crate) fn drain_retired_voice_ids(&self) -> Vec<VoiceId> {
        self.retirement_receiver.try_iter().collect()
    }

    pub(crate) fn close(&self) -> Result<()> {
        let mut close_state = self
            .close_state
            .lock()
            .map_err(|_| PetalSonicError::Engine("Runtime close lock is poisoned".into()))?;
        if let RuntimeCloseState::Closed(report) = &*close_state {
            return report.result();
        }
        self.runtime_state
            .store(RuntimeState::Closing as u8, Ordering::Release);
        let report = RuntimeCloseReport::capture(self.services.close());
        self.spatial_frames.close();
        self.acoustic_propagation.close_publication();
        self.active_voice_count.store(0, Ordering::Release);
        *close_state = RuntimeCloseState::Closed(report);
        self.runtime_state
            .store(RuntimeState::Closed as u8, Ordering::Release);
        let RuntimeCloseState::Closed(report) = &*close_state else {
            unreachable!("the close transaction committed its terminal report")
        };
        report.result()
    }

    pub(crate) fn supervisor_tick<D: OutputRuntimeDriver>(
        driver: &mut D,
        runtime_state: &AtomicU8,
        recovery_attempts: &AtomicU64,
        schedule: &mut SupervisorSchedule,
        now: Instant,
    ) {
        driver.drain_retired_resources();
        let state = RuntimeState::load(runtime_state);
        let probe = state == RuntimeState::Running && now >= schedule.next_health_probe;
        if state == RuntimeState::Running && !probe {
            return;
        }
        if !matches!(state, RuntimeState::Running | RuntimeState::Recovering) {
            return;
        }

        if state == RuntimeState::Running {
            schedule.next_health_probe = now + OUTPUT_RETRY_INTERVAL;
        }
        let elapsed = if state == RuntimeState::Recovering {
            now.saturating_duration_since(schedule.last_advance)
        } else {
            Duration::ZERO
        };
        schedule.last_advance = now;
        let retry_now = state == RuntimeState::Running || now >= schedule.next_retry;
        let result = driver.reconcile_output(OutputRecoveryRequest {
            probe,
            retry_now,
            elapsed_without_output: elapsed,
        });

        if retry_now && !matches!(result, OutputRecoveryResult::Stable) {
            recovery_attempts.fetch_add(1, Ordering::Relaxed);
        }
        match result {
            OutputRecoveryResult::Stable => {}
            OutputRecoveryResult::Running(_device) => {
                if Self::transition_runtime_state(runtime_state, state, RuntimeState::Running) {
                    if state == RuntimeState::Running {
                        driver.emit_runtime_state(RuntimeState::Recovering);
                    }
                    schedule.next_health_probe = now + OUTPUT_RETRY_INTERVAL;
                    driver.emit_runtime_state(RuntimeState::Running);
                }
            }
            OutputRecoveryResult::Recovering(_cause) => {
                if state == RuntimeState::Running
                    && Self::transition_runtime_state(
                        runtime_state,
                        state,
                        RuntimeState::Recovering,
                    )
                {
                    driver.emit_runtime_state(RuntimeState::Recovering);
                }
                if retry_now {
                    schedule.next_retry = now + OUTPUT_RETRY_INTERVAL;
                }
            }
            OutputRecoveryResult::Failed(_failure) => {
                if Self::transition_runtime_state(runtime_state, state, RuntimeState::Failed) {
                    if state == RuntimeState::Running {
                        driver.emit_runtime_state(RuntimeState::Recovering);
                    }
                    driver.emit_runtime_state(RuntimeState::Failed);
                }
            }
        }
    }

    fn transition_runtime_state(
        runtime_state: &AtomicU8,
        observed: RuntimeState,
        next: RuntimeState,
    ) -> bool {
        runtime_state
            .compare_exchange(
                observed as u8,
                next as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    // The closure captures these linear startup capabilities exactly once. A generic dependency
    // bag would weaken that ownership boundary and is not reused anywhere else.
    #[allow(clippy::too_many_arguments)]
    fn start_output_child(
        services: &mut RuntimeServices,
        startup: PreparedEngine,
        output: Box<dyn FnOnce() -> Result<Box<dyn OutputPlatform>> + Send>,
        bus_params: Arc<Mutex<Vec<BusParams>>>,
        runtime_state: Arc<AtomicU8>,
        recovery_attempts: Arc<AtomicU64>,
        runtime_failure: RuntimeFailurePublisher,
        #[cfg(test)] render_worker_fault: RenderWorkerFaultInjector,
    ) -> Result<()> {
        services.start_output(
            "petalsonic-output",
            ChildCancellation::passive(),
            move |child_startup, cancellation| {
                let output = match output() {
                    Ok(output) => output,
                    Err(error) => return Err(child_startup.failed(error)),
                };
                let initial_buses = bus_params
                    .lock()
                    .map(|buses| buses.clone())
                    .unwrap_or_else(|_| vec![BusParams::default()]);
                let mut engine = match PetalSonicEngine::new_with_output(
                    startup,
                    output,
                    initial_buses,
                    runtime_failure,
                    #[cfg(test)]
                    render_worker_fault,
                ) {
                    Ok(engine) => engine,
                    Err(error) => return Err(child_startup.failed(error)),
                };
                child_startup.ready()?;
                let poll_interval = Duration::from_millis(20);
                let mut schedule = SupervisorSchedule::new(Instant::now());
                engine.emit_runtime_state(RuntimeState::Recovering);

                while !cancellation.is_requested() {
                    Self::supervisor_tick(
                        &mut engine,
                        &runtime_state,
                        &recovery_attempts,
                        &mut schedule,
                        Instant::now(),
                    );
                    std::thread::park_timeout(poll_interval);
                }
                engine.close().map_err(|error| {
                    RuntimeChildFailure::new(format!("failed to close output engine: {error}"))
                })
            },
        )
    }
}

impl Drop for AudioRuntime {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acoustic_propagation::{
        AcousticVoiceInput, AcousticVoiceRetirementObservation, AcousticVoiceRetirementWitness,
    };
    use crate::audio_data::PetalSonicAudioData;
    use crate::domain::{
        Emitter, EmitterDesc, ExtentSample, ExtentSampleId, PlayOptions, SourceExtent,
    };
    use crate::platform::output::{
        OutputDeviceState, OutputRecoveryCause, OutputRecoveryRequest, OutputRecoveryResult,
        fake::{FakeDevice as PlatformFakeDevice, FakeOutputHandle, FakeOutputPlatform},
    };
    use crate::playback::prepare_test_voice;
    use crate::realtime_latest::RealtimeLatest;
    use std::sync::atomic::{AtomicU8, AtomicU64, AtomicUsize};

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct FakeDevice {
        name: &'static str,
        sample_rate: u32,
        channels: u16,
    }

    struct RetirementEngineFixture {
        engine: PetalSonicEngine,
        commands: Sender<PlaybackCommand>,
        active_voice_count: Arc<AtomicUsize>,
        runtime_state: Arc<AtomicU8>,
        recovery_attempts: Arc<AtomicU64>,
        schedule: SupervisorSchedule,
        now: Instant,
        output: FakeOutputHandle,
    }

    fn retirement_engine_fixture(max_voices: usize) -> RetirementEngineFixture {
        retirement_engine_fixture_with_output(max_voices, Vec::new(), None)
    }

    fn retirement_engine_fixture_with_output(
        max_voices: usize,
        devices: Vec<PlatformFakeDevice>,
        selected: Option<usize>,
    ) -> RetirementEngineFixture {
        let desc = PetalSonicWorldDesc {
            block_size: 64,
            max_voices,
            control_queue_capacity: max_voices.max(1),
            lifecycle_queue_capacity: max_voices.max(1),
            event_queue_capacity: 16,
            timing_queue_capacity: 16,
            ..PetalSonicWorldDesc::default()
        };
        let (commands, regular_commands) = crossbeam_channel::bounded(desc.control_queue_capacity);
        let (_lifecycle_sender, lifecycle_commands) =
            crossbeam_channel::bounded(desc.lifecycle_queue_capacity);
        let active_voice_count = Arc::new(AtomicUsize::new(0));
        let (control_retirement_sender, _) = crossbeam_channel::bounded(desc.max_voices);
        let (_spatial_publisher, spatial_frames) = RealtimeLatest::bounded(1);
        let (_response_publisher, acoustic_responses) = RealtimeLatest::bounded(2);
        let (startup, _) = PreparedEngine::new(
            desc.clone(),
            active_voice_count.clone(),
            control_retirement_sender,
            spatial_frames,
            acoustic_responses,
            AcousticVoiceInput::isolated(desc.max_voices),
            Arc::new(AtomicU64::new(0)),
            Arc::new(AtomicBool::new(true)),
            regular_commands,
            lifecycle_commands,
        );
        let (platform, output) = FakeOutputPlatform::scripted(devices, selected);
        let runtime_state = Arc::new(AtomicU8::new(RuntimeState::Recovering as u8));
        let recovery_attempts = Arc::new(AtomicU64::new(0));
        let now = Instant::now();
        RetirementEngineFixture {
            engine: PetalSonicEngine::new_with_output(
                startup,
                Box::new(platform),
                vec![BusParams::default()],
                RuntimeFailurePublisher::new(runtime_state.clone()),
                RenderWorkerFaultInjector::default(),
            )
            .unwrap(),
            commands,
            active_voice_count,
            runtime_state,
            recovery_attempts,
            schedule: SupervisorSchedule::new(now),
            now,
            output,
        }
    }

    fn witnessed_voice(
        identity: u64,
        observation: Arc<AcousticVoiceRetirementObservation>,
        options: PlayOptions,
        audio_frames: usize,
    ) -> PlaybackCommand {
        let extent = SourceExtent::weighted_samples(vec![
            ExtentSample::new(ExtentSampleId(1), crate::math::Vec3::X, 1.0).unwrap(),
            ExtentSample::new(ExtentSampleId(2), crate::math::Vec3::Y, 3.0).unwrap(),
        ])
        .unwrap();
        let emitter = Emitter {
            world_id: 1,
            index: identity as u32,
            generation: 1,
        };
        let audio = Arc::new(PetalSonicAudioData::new(
            vec![0.25; audio_frames],
            48_000,
            1,
            Duration::from_secs_f64(audio_frames as f64 / 48_000.0),
        ));
        PlaybackCommand::Play(
            prepare_test_voice(
                VoiceId::from(identity),
                emitter,
                audio,
                EmitterDesc::spatial(crate::math::Pose::identity()).with_extent(extent),
                options.detached(),
                None,
                0,
                64,
            )
            .with_acoustic_retirement_witness(AcousticVoiceRetirementWitness::new(
                identity,
                observation,
            )),
        )
    }

    fn witnessed_once_voice(
        identity: u64,
        observation: Arc<AcousticVoiceRetirementObservation>,
    ) -> PlaybackCommand {
        witnessed_voice(identity, observation, PlayOptions::once(), 96)
    }

    fn retire_once_without_output(
        fixture: &mut RetirementEngineFixture,
        identity: u64,
        observation: Arc<AcousticVoiceRetirementObservation>,
    ) -> usize {
        fixture.active_voice_count.fetch_add(1, Ordering::AcqRel);
        fixture
            .commands
            .try_send(witnessed_once_voice(identity, observation))
            .unwrap();
        crate::test_support::realtime_memory_activity(|| {
            fixture
                .engine
                .advance_without_output_for_test(Duration::from_millis(2));
        })
    }

    #[test]
    fn supervisor_receives_the_original_acoustic_voice_before_dropping_it_off_realtime() {
        let mut fixture = retirement_engine_fixture(1);
        let observation = Arc::new(AcousticVoiceRetirementObservation::default());
        fixture.engine.prepare_logical_output_for_test().unwrap();
        fixture.active_voice_count.fetch_add(1, Ordering::AcqRel);
        fixture
            .commands
            .try_send(witnessed_voice(
                901,
                observation.clone(),
                PlayOptions::once(),
                64 * 3,
            ))
            .unwrap();
        fixture.engine.render_once_for_test();
        fixture.engine.drain_logical_output_for_test();

        let render_activity = crate::test_support::realtime_memory_activity(|| {
            fixture.engine.render_once_for_test();
        });

        assert_eq!(observation.observed(), (0, 0, 0));
        assert_eq!(render_activity, 0);
        AudioRuntime::supervisor_tick(
            &mut fixture.engine,
            &fixture.runtime_state,
            &fixture.recovery_attempts,
            &mut fixture.schedule,
            fixture.now,
        );
        assert_eq!(observation.observed(), (901, 1, 0));
    }

    #[test]
    fn explicit_stop_retires_the_complete_voice_through_the_supervisor() {
        let mut fixture = retirement_engine_fixture(1);
        let observation = Arc::new(AcousticVoiceRetirementObservation::default());
        fixture.active_voice_count.fetch_add(1, Ordering::AcqRel);
        fixture
            .commands
            .try_send(witnessed_voice(
                902,
                observation.clone(),
                PlayOptions::looping(),
                4_096,
            ))
            .unwrap();
        fixture
            .engine
            .advance_without_output_for_test(Duration::ZERO);
        fixture
            .commands
            .try_send(PlaybackCommand::StopVoice(VoiceId::from(902)))
            .unwrap();

        let render_activity = crate::test_support::realtime_memory_activity(|| {
            fixture
                .engine
                .advance_without_output_for_test(Duration::from_millis(10));
        });

        assert_eq!(observation.observed(), (0, 0, 0));
        assert_eq!(render_activity, 0);
        AudioRuntime::supervisor_tick(
            &mut fixture.engine,
            &fixture.runtime_state,
            &fixture.recovery_attempts,
            &mut fixture.schedule,
            fixture.now,
        );
        assert_eq!(observation.observed(), (902, 1, 0));
    }

    #[test]
    fn full_retirement_queue_drains_pending_payloads_on_later_supervisor_ticks() {
        let mut fixture = retirement_engine_fixture(1);
        let first = Arc::new(AcousticVoiceRetirementObservation::default());
        let second = Arc::new(AcousticVoiceRetirementObservation::default());

        assert_eq!(
            retire_once_without_output(&mut fixture, 911, first.clone()),
            0
        );
        assert_eq!(
            retire_once_without_output(&mut fixture, 912, second.clone()),
            0
        );
        assert_eq!(first.observed(), (0, 0, 0));
        assert_eq!(second.observed(), (0, 0, 0));

        AudioRuntime::supervisor_tick(
            &mut fixture.engine,
            &fixture.runtime_state,
            &fixture.recovery_attempts,
            &mut fixture.schedule,
            fixture.now,
        );
        assert_eq!(first.observed(), (911, 1, 0));
        assert_eq!(second.observed(), (0, 0, 0));

        AudioRuntime::supervisor_tick(
            &mut fixture.engine,
            &fixture.runtime_state,
            &fixture.recovery_attempts,
            &mut fixture.schedule,
            fixture.now,
        );
        assert_eq!(second.observed(), (912, 1, 0));
    }

    #[test]
    fn disconnected_retirement_channel_is_reclaimed_after_render_quiesces() {
        let mut fixture = retirement_engine_fixture(1);
        let observation = Arc::new(AcousticVoiceRetirementObservation::default());
        fixture
            .engine
            .disconnect_voice_retirement_receiver_for_test();

        assert_eq!(
            retire_once_without_output(&mut fixture, 921, observation.clone()),
            0
        );
        assert_eq!(observation.observed(), (0, 0, 0));
        AudioRuntime::supervisor_tick(
            &mut fixture.engine,
            &fixture.runtime_state,
            &fixture.recovery_attempts,
            &mut fixture.schedule,
            fixture.now,
        );
        assert_eq!(observation.observed(), (0, 0, 0));

        fixture.engine.close().unwrap();
        assert_eq!(observation.observed(), (921, 1, 0));
        assert_eq!(fixture.active_voice_count.load(Ordering::Acquire), 0);
    }

    #[test]
    fn persistent_output_cleanup_failure_still_reclaims_retirements_once() {
        let mut fixture = retirement_engine_fixture_with_output(
            1,
            vec![PlatformFakeDevice::stereo("A", 48_000)],
            Some(0),
        );
        let observation = Arc::new(AcousticVoiceRetirementObservation::default());
        assert_eq!(
            retire_once_without_output(&mut fixture, 931, observation.clone()),
            0
        );
        assert!(matches!(
            fixture.engine.reconcile_output(OutputRecoveryRequest {
                probe: false,
                retry_now: true,
                elapsed_without_output: Duration::ZERO,
            }),
            OutputRecoveryResult::Running(_)
        ));
        fixture.output.fail_all_stops();

        assert!(fixture.engine.close().is_err());
        assert_eq!(observation.observed(), (931, 1, 0));
        assert_eq!(fixture.active_voice_count.load(Ordering::Acquire), 0);
        assert!(fixture.engine.close().is_err());
        assert_eq!(observation.observed(), (931, 1, 0));

        fixture.output.allow_stops();
        fixture.engine.close().unwrap();
        assert_eq!(observation.observed(), (931, 1, 0));
    }

    struct SupervisorFakeDriver {
        active: Option<FakeDevice>,
        selected: Option<FakeDevice>,
        stream_failed: bool,
        prepared: Option<FakeDevice>,
        advanced: Duration,
        loop_cursor_frames: usize,
        loop_length_frames: usize,
        one_shot_remaining_frames: usize,
        one_shot_completed: bool,
    }

    impl SupervisorFakeDriver {
        fn with_active(active: FakeDevice) -> Self {
            Self {
                active: Some(active),
                selected: Some(active),
                stream_failed: false,
                prepared: None,
                advanced: Duration::ZERO,
                loop_cursor_frames: 0,
                loop_length_frames: 12_000,
                one_shot_remaining_frames: 4_800,
                one_shot_completed: false,
            }
        }
    }

    impl OutputRuntimeDriver for SupervisorFakeDriver {
        fn drain_retired_resources(&mut self) {}

        fn reconcile_output(&mut self, request: OutputRecoveryRequest) -> OutputRecoveryResult {
            if self.active.is_some() && request.probe {
                if !self.stream_failed && self.active == self.selected {
                    return OutputRecoveryResult::Stable;
                }
                if !self.stream_failed {
                    let Some(selected) = self.selected else {
                        return OutputRecoveryResult::Stable;
                    };
                    self.prepared = Some(selected);
                }
                self.active = None;
            }
            self.advanced += request.elapsed_without_output;
            let frames = (request.elapsed_without_output.as_secs_f64() * 48_000.0).floor() as usize;
            self.loop_cursor_frames = (self.loop_cursor_frames + frames) % self.loop_length_frames;
            if frames >= self.one_shot_remaining_frames {
                self.one_shot_remaining_frames = 0;
                self.one_shot_completed = true;
            } else {
                self.one_shot_remaining_frames -= frames;
            }
            if !request.retry_now {
                return OutputRecoveryResult::Recovering(OutputRecoveryCause::DeviceUnavailable);
            }
            let Some(selected) = self.prepared.take().or(self.selected) else {
                return OutputRecoveryResult::Recovering(OutputRecoveryCause::DeviceUnavailable);
            };
            self.active = Some(selected);
            self.stream_failed = false;
            OutputRecoveryResult::Running(OutputDeviceState {
                diagnostic_name: selected.name.to_string(),
                sample_rate: selected.sample_rate,
                physical_channels: selected.channels,
            })
        }

        fn emit_runtime_state(&self, _state: RuntimeState) {}
    }

    struct ChildFailureDuringOutputRecovery<'a> {
        runtime_state: &'a AtomicU8,
    }

    impl OutputRuntimeDriver for ChildFailureDuringOutputRecovery<'_> {
        fn drain_retired_resources(&mut self) {}

        fn reconcile_output(&mut self, _request: OutputRecoveryRequest) -> OutputRecoveryResult {
            self.runtime_state
                .store(RuntimeState::Failed as u8, Ordering::Release);
            OutputRecoveryResult::Running(OutputDeviceState {
                diagnostic_name: "A".into(),
                sample_rate: 48_000,
                physical_channels: 2,
            })
        }

        fn emit_runtime_state(&self, _state: RuntimeState) {}
    }

    #[test]
    fn output_recovery_cannot_overwrite_a_concurrent_child_failure() {
        let runtime_state = AtomicU8::new(RuntimeState::Recovering as u8);
        let recovery_attempts = AtomicU64::new(0);
        let now = Instant::now();
        let mut schedule = SupervisorSchedule::new(now);
        let mut driver = ChildFailureDuringOutputRecovery {
            runtime_state: &runtime_state,
        };

        AudioRuntime::supervisor_tick(
            &mut driver,
            &runtime_state,
            &recovery_attempts,
            &mut schedule,
            now,
        );

        assert_eq!(RuntimeState::load(&runtime_state), RuntimeState::Failed);
    }

    #[test]
    fn supervisor_publishes_running_after_default_device_recovery() {
        let a = FakeDevice {
            name: "A",
            sample_rate: 48_000,
            channels: 2,
        };
        let b = FakeDevice {
            name: "B",
            sample_rate: 44_100,
            channels: 6,
        };
        let mut driver = SupervisorFakeDriver::with_active(a);
        driver.selected = Some(b);
        let runtime_state = AtomicU8::new(RuntimeState::Running as u8);
        let recovery_attempts = AtomicU64::new(0);
        let now = Instant::now();
        let mut schedule = SupervisorSchedule::new(now);

        AudioRuntime::supervisor_tick(
            &mut driver,
            &runtime_state,
            &recovery_attempts,
            &mut schedule,
            now,
        );

        assert_eq!(driver.active, Some(b));
        assert_eq!(RuntimeState::load(&runtime_state), RuntimeState::Running);
        assert_eq!(recovery_attempts.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn recovery_accumulates_elapsed_time_and_advances_voice_timelines() {
        let a = FakeDevice {
            name: "A",
            sample_rate: 48_000,
            channels: 2,
        };
        let b = FakeDevice {
            name: "B",
            sample_rate: 96_000,
            channels: 2,
        };
        let mut driver = SupervisorFakeDriver::with_active(a);
        driver.stream_failed = true;
        driver.selected = None;
        let runtime_state = AtomicU8::new(RuntimeState::Running as u8);
        let recovery_attempts = AtomicU64::new(0);
        let now = Instant::now();
        let mut schedule = SupervisorSchedule::new(now);

        AudioRuntime::supervisor_tick(
            &mut driver,
            &runtime_state,
            &recovery_attempts,
            &mut schedule,
            now,
        );
        assert_eq!(RuntimeState::load(&runtime_state), RuntimeState::Recovering);
        assert_eq!(recovery_attempts.load(Ordering::Relaxed), 1);

        driver.selected = Some(b);
        AudioRuntime::supervisor_tick(
            &mut driver,
            &runtime_state,
            &recovery_attempts,
            &mut schedule,
            now + OUTPUT_RETRY_INTERVAL,
        );

        assert_eq!(driver.active, Some(b));
        assert_eq!(driver.advanced, OUTPUT_RETRY_INTERVAL);
        assert_eq!(driver.loop_cursor_frames, 0);
        assert!(driver.one_shot_completed);
        assert_eq!(RuntimeState::load(&runtime_state), RuntimeState::Running);
        assert_eq!(recovery_attempts.load(Ordering::Relaxed), 2);
    }
}
