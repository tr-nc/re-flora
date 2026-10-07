//! One logical-stereo render quantum and all mutable state it advances.
//!
//! The output engine schedules this module; it does not reach through the seam to
//! coordinate Voices, DSP processors, buffers, telemetry, or retirement.

use crate::acoustic_propagation::{
    AcousticResponse, AcousticVoice, AcousticVoiceInput, AcousticVoiceRegistry,
};
use crate::audio_data::{ResamplerType, StreamingResampler};
use crate::config::{LatencyProfile, SpatialQuality};
use crate::domain::{BusParams, PlaybackControl, SpatialFrame, VoiceId};
use crate::error::{PetalSonicError, Result};
use crate::events::{PetalSonicEvent, RenderTimingEvent, RuntimeCounters, VoiceTelemetryEvent};
use crate::platform::output::StereoFrame;
use crate::playback::{PlayState, PlaybackCommand, PlaybackInstance, PlaybackReclaim};
use crate::realtime_latest::RealtimeConsumer;
#[cfg(test)]
use crate::realtime_latest::RealtimeLatest;
use crate::spatial::{
    AcousticResponseReplacement, RetiredSpatialSource, SpatialProcessingMetrics, SpatialProcessor,
    SpatialProcessorConfig, SpatialRenderContext,
};
use crossbeam_channel::{Receiver, Sender};
use ringbuf::{
    HeapCons, HeapProd, HeapRb,
    traits::{Observer, Producer, Split},
};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const LOGICAL_CHANNELS: u16 = 2;
pub(crate) const MASTER_HEADROOM_DB: f32 = -6.0;

/// The single-consume owner of every endpoint required by one render quantum.
///
/// Fields stay private so only the render module can interpret or partially consume the
/// producer-to-consumer topology assembled by the engine owner.
pub(crate) struct PreparedRender {
    desc: crate::config::PetalSonicWorldDesc,
    active_voice_count: Arc<AtomicUsize>,
    retirement_sender: Sender<VoiceId>,
    spatial_frames: RealtimeConsumer<SpatialFrame>,
    acoustic_responses: RealtimeConsumer<AcousticResponse>,
    acoustic_voice_input: AcousticVoiceInput,
    acoustic_scene_version: Arc<AtomicU64>,
    environmental_acoustics_enabled: Arc<AtomicBool>,
    regular_commands: Receiver<PlaybackCommand>,
    lifecycle_commands: Receiver<PlaybackCommand>,
    event_sender: Sender<PetalSonicEvent>,
    voice_telemetry_sender: Sender<VoiceTelemetryEvent>,
    timing_sender: Sender<RenderTimingEvent>,
    counters: Arc<RuntimeCounters>,
}

impl PreparedRender {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        desc: crate::config::PetalSonicWorldDesc,
        active_voice_count: Arc<AtomicUsize>,
        retirement_sender: Sender<VoiceId>,
        spatial_frames: RealtimeConsumer<SpatialFrame>,
        acoustic_responses: RealtimeConsumer<AcousticResponse>,
        acoustic_voice_input: AcousticVoiceInput,
        acoustic_scene_version: Arc<AtomicU64>,
        environmental_acoustics_enabled: Arc<AtomicBool>,
        regular_commands: Receiver<PlaybackCommand>,
        lifecycle_commands: Receiver<PlaybackCommand>,
        event_sender: Sender<PetalSonicEvent>,
        voice_telemetry_sender: Sender<VoiceTelemetryEvent>,
        timing_sender: Sender<RenderTimingEvent>,
        counters: Arc<RuntimeCounters>,
    ) -> Self {
        Self {
            desc,
            active_voice_count,
            retirement_sender,
            spatial_frames,
            acoustic_responses,
            acoustic_voice_input,
            acoustic_scene_version,
            environmental_acoustics_enabled,
            regular_commands,
            lifecycle_commands,
            event_sender,
            voice_telemetry_sender,
            timing_sender,
            counters,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SpatialBackendPlan {
    pub(crate) use_ambisonics: bool,
}

impl SpatialBackendPlan {
    pub(crate) fn for_quality(quality: SpatialQuality) -> Self {
        match quality {
            SpatialQuality::LowLatency => Self {
                use_ambisonics: false,
            },
            SpatialQuality::Balanced | SpatialQuality::HighQuality => Self {
                use_ambisonics: true,
            },
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct RenderSchedule {
    ring_blocks: usize,
    low_water_blocks: usize,
    high_water_blocks: usize,
    normal_chunk_blocks: usize,
    catch_up_chunk_blocks: usize,
    wake_divisor: u32,
}

impl RenderSchedule {
    pub(crate) fn for_profile(profile: LatencyProfile) -> Self {
        match profile {
            LatencyProfile::Responsive => Self {
                ring_blocks: 4,
                low_water_blocks: 1,
                high_water_blocks: 2,
                normal_chunk_blocks: 1,
                catch_up_chunk_blocks: 1,
                wake_divisor: 8,
            },
            LatencyProfile::Balanced => Self {
                ring_blocks: 8,
                low_water_blocks: 2,
                high_water_blocks: 3,
                normal_chunk_blocks: 1,
                catch_up_chunk_blocks: 2,
                wake_divisor: 4,
            },
            LatencyProfile::Robust => Self {
                ring_blocks: 12,
                low_water_blocks: 3,
                high_water_blocks: 5,
                normal_chunk_blocks: 2,
                catch_up_chunk_blocks: 3,
                wake_divisor: 3,
            },
        }
    }

    pub(crate) fn wake_interval(self, block_size: usize, sample_rate: u32) -> Duration {
        let block_duration = Duration::from_secs_f64(block_size as f64 / sample_rate as f64);
        (block_duration / self.wake_divisor)
            .clamp(Duration::from_micros(250), Duration::from_millis(2))
    }
}

struct LogicalStereoOutput {
    resampler: StreamingResampler,
    producer: HeapProd<StereoFrame>,
    world_buffer: Vec<f32>,
    resampled_buffer: Vec<f32>,
}

#[derive(Clone, Copy, Debug)]
struct CompletedPlayback {
    voice_id: VoiceId,
    emitter: crate::domain::Emitter,
    completion_tag: Option<crate::domain::PlaybackTag>,
}

impl CompletedPlayback {
    fn from_reclaim(voice_id: VoiceId, reclaim: PlaybackReclaim) -> Self {
        Self {
            voice_id,
            emitter: reclaim.emitter,
            completion_tag: reclaim.completion_tag,
        }
    }
}

/// Preallocated identity and telemetry storage reused by the render owner every quantum.
struct VoiceMixScratch {
    spatial_voice_ids: Vec<VoiceId>,
    muted_spatial_voice_ids: Vec<VoiceId>,
    non_spatial_voice_ids: Vec<VoiceId>,
    voice_telemetry: Vec<VoiceTelemetryEvent>,
}

impl VoiceMixScratch {
    fn new(max_voices: usize) -> Self {
        Self {
            spatial_voice_ids: Vec::with_capacity(max_voices),
            muted_spatial_voice_ids: Vec::with_capacity(max_voices),
            non_spatial_voice_ids: Vec::with_capacity(max_voices),
            voice_telemetry: Vec::with_capacity(max_voices.saturating_mul(4)),
        }
    }

    fn clear(&mut self) {
        self.spatial_voice_ids.clear();
        self.muted_spatial_voice_ids.clear();
        self.non_spatial_voice_ids.clear();
        self.voice_telemetry.clear();
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct MixProfilingSummary {
    direct_mix_time_us: u64,
    spatial_mix_time_us: u64,
    spatial_metrics: Option<SpatialProcessingMetrics>,
}

/// The concrete, crate-private render module.
///
/// Every field mutated by a quantum lives here without an inner lock. The sole
/// outer mutex only transfers exclusive execution between the render thread and
/// the output supervisor after that thread has stopped.
pub(crate) struct RenderQuantum {
    sample_rate: u32,
    block_size: usize,
    max_voices: usize,
    schedule: RenderSchedule,
    master_gain_linear: f32,
    active_playback: HashMap<VoiceId, PlaybackInstance>,
    active_voice_count: Arc<AtomicUsize>,
    processor: SpatialProcessor,
    buses: Vec<BusParams>,
    regular_commands: Receiver<PlaybackCommand>,
    lifecycle_commands: Receiver<PlaybackCommand>,
    spatial_frames: RealtimeConsumer<SpatialFrame>,
    current_spatial_frame: Option<Arc<SpatialFrame>>,
    acoustic_responses: RealtimeConsumer<AcousticResponse>,
    acoustic_voice_input: AcousticVoiceInput,
    acoustic_voices: AcousticVoiceRegistry,
    acoustic_scene_version: Arc<std::sync::atomic::AtomicU64>,
    retirement_sender: Sender<VoiceId>,
    event_sender: Sender<PetalSonicEvent>,
    voice_telemetry_sender: Sender<VoiceTelemetryEvent>,
    timing_sender: Sender<RenderTimingEvent>,
    counters: Arc<RuntimeCounters>,
    voice_retirement_sender: Sender<VoiceRetirement>,
    pending_voice_retirements: Vec<VoiceRetirement>,
    mixer_scratch: VoiceMixScratch,
    completed_playbacks: Vec<CompletedPlayback>,
    render_block_index: u64,
    output: Option<LogicalStereoOutput>,
}

/// One complete Voice lifetime removed from active rendering and transferred linearly to the
/// output supervisor. The payload is intentionally not cloneable: render code may only move it
/// into the bounded retirement path, and destruction happens when the non-realtime owner drains
/// that path.
pub(crate) struct VoiceRetirement {
    _voice_id: VoiceId,
    _playback: PlaybackInstance,
    _acoustic: Option<AcousticVoice>,
    _spatial: Option<RetiredSpatialSource>,
}

impl RenderQuantum {
    pub(crate) fn new(
        startup: PreparedRender,
        buses: Vec<BusParams>,
        voice_retirement_sender: Sender<VoiceRetirement>,
    ) -> Result<Self> {
        let PreparedRender {
            desc,
            active_voice_count,
            retirement_sender,
            spatial_frames,
            acoustic_responses,
            acoustic_voice_input,
            acoustic_scene_version,
            environmental_acoustics_enabled,
            regular_commands,
            lifecycle_commands,
            event_sender,
            voice_telemetry_sender,
            timing_sender,
            counters,
        } = startup;
        let backend_plan = SpatialBackendPlan::for_quality(desc.spatial_quality);
        let processor = SpatialProcessor::new(SpatialProcessorConfig {
            sample_rate: desc.sample_rate,
            frame_size: desc.block_size,
            max_voices: desc.max_voices,
            distance_scaler: desc.distance_scaler,
            native_hrtf_path: desc.native_hrtf_path.clone(),
            hrtf_gain: desc.hrtf_gain,
            use_ambisonics: backend_plan.use_ambisonics,
            environmental_acoustics_enabled,
        })
        .map_err(|error| PetalSonicError::BackendUnavailable {
            backend: "spatial renderer",
            reason: error.to_string(),
        })?;
        let max_voices = desc.max_voices;
        Ok(Self {
            sample_rate: desc.sample_rate,
            block_size: desc.block_size,
            max_voices,
            schedule: RenderSchedule::for_profile(desc.latency_profile),
            master_gain_linear: crate::gain::db_to_linear(MASTER_HEADROOM_DB),
            active_playback: HashMap::with_capacity(max_voices),
            active_voice_count,
            processor,
            buses,
            regular_commands,
            lifecycle_commands,
            spatial_frames,
            current_spatial_frame: None,
            acoustic_responses,
            acoustic_voice_input,
            acoustic_voices: AcousticVoiceRegistry::new(max_voices),
            acoustic_scene_version,
            retirement_sender,
            event_sender,
            voice_telemetry_sender,
            timing_sender,
            counters,
            voice_retirement_sender,
            pending_voice_retirements: Vec::with_capacity(max_voices),
            mixer_scratch: VoiceMixScratch::new(max_voices),
            completed_playbacks: Vec::with_capacity(max_voices),
            render_block_index: 0,
            output: None,
        })
    }

    pub(crate) fn schedule(&self) -> RenderSchedule {
        self.schedule
    }

    pub(crate) fn connect_output(
        &mut self,
        device_sample_rate: u32,
    ) -> Result<HeapCons<StereoFrame>> {
        let ring_size = self.block_size * self.schedule.ring_blocks;
        let (producer, consumer) = HeapRb::<StereoFrame>::new(ring_size).split();
        let resampler = StreamingResampler::new(
            self.sample_rate,
            device_sample_rate,
            LOGICAL_CHANNELS,
            self.block_size,
            Some(ResamplerType::Fast),
        )?;
        self.output = Some(LogicalStereoOutput {
            resampler,
            producer,
            world_buffer: vec![0.0; self.block_size * LOGICAL_CHANNELS as usize],
            resampled_buffer: vec![
                0.0;
                ((self.block_size as f64 * device_sample_rate as f64 / self.sample_rate as f64)
                    .ceil() as usize
                    + 10)
                    * LOGICAL_CHANNELS as usize
            ],
        });
        Ok(consumer)
    }

    pub(crate) fn disconnect_output(&mut self) {
        self.output = None;
    }

    pub(crate) fn render(&mut self) {
        self.flush_voice_retirements();
        self.consume_latest_spatial_frame();
        self.consume_latest_acoustic_response();
        let listener = self
            .current_spatial_frame
            .as_ref()
            .map_or_else(crate::math::Pose::default, |frame| frame.listener());
        let _ = self.processor.set_listener_pose(listener);
        self.process_commands();

        let Some(output) = self.output.as_ref() else {
            return;
        };
        let target = self.block_size * self.schedule.high_water_blocks;
        let occupied = output.producer.occupied_len();
        if occupied >= target {
            return;
        }
        let free = output.producer.vacant_len();
        let max_chunk = self.block_size
            * if occupied < self.block_size * self.schedule.low_water_blocks {
                self.schedule.catch_up_chunk_blocks
            } else {
                self.schedule.normal_chunk_blocks
            };
        let frames = free.min(target.saturating_sub(occupied)).min(max_chunk);
        if frames == 0 {
            return;
        }

        let timing = self.generate_samples(frames);
        self.counters.record_render_time(timing.total_time_us);
        if self.timing_sender.try_send(timing).is_ok() {
            RuntimeCounters::observe_high_water(
                &self.counters.timing_queue_high_water,
                self.timing_sender.len(),
            );
        } else {
            self.counters
                .dropped_timing_events
                .fetch_add(1, Ordering::Relaxed);
        }
        self.retire_completed_voices();
    }

    pub(crate) fn advance_without_output(&mut self, elapsed: Duration) {
        self.flush_voice_retirements();
        self.process_commands();
        let frames = (elapsed.as_secs_f64() * self.sample_rate as f64).floor() as usize;
        if frames == 0 {
            return;
        }
        self.completed_playbacks.clear();
        for (voice_id, instance) in &mut self.active_playback {
            if !matches!(instance.info.play_state, PlayState::Playing) {
                continue;
            }
            let bus = effective_bus_params(instance.bus_index, &self.buses);
            if bus.paused {
                continue;
            }
            instance.set_mix_parameters(bus);
            instance.advance_silently(frames);
            let _ = instance.check_and_clear_end_flag();
            if let Some(reclaim) = instance.take_reclaim() {
                self.completed_playbacks
                    .push(CompletedPlayback::from_reclaim(*voice_id, reclaim));
            }
        }
        self.retire_completed_voices();
    }

    fn process_commands(&mut self) {
        for _ in 0..self.regular_commands.capacity().unwrap_or(1) {
            let Ok(command) = self.regular_commands.try_recv() else {
                break;
            };
            self.apply_command(command);
        }
        while let Ok(command) = self.lifecycle_commands.try_recv() {
            self.apply_command(command);
        }
        self.publish_acoustic_voices();
    }

    fn apply_command(&mut self, command: PlaybackCommand) {
        match command {
            PlaybackCommand::Play(prepared) => {
                let (voice_id, instance, acoustic_voice) = prepared.start();
                if self.active_playback.insert(voice_id, instance).is_some() {
                    self.active_voice_count.fetch_sub(1, Ordering::AcqRel);
                }
                if let Some(acoustic_voice) = acoustic_voice {
                    self.acoustic_voices
                        .activate(acoustic_voice, self.current_spatial_frame.as_deref());
                }
            }
            PlaybackCommand::PauseVoice(id) => self.with_voice(id, PlaybackInstance::pause),
            PlaybackCommand::StopVoice(id) => self.with_voice(id, PlaybackInstance::begin_fade_out),
            PlaybackCommand::SeekVoice(id, progress) => {
                self.with_voice(id, |voice| voice.seek(progress))
            }
            PlaybackCommand::ResumeVoice(id) => self.with_voice(id, PlaybackInstance::resume),
            PlaybackCommand::SetVoiceRate(id, rate) => {
                self.with_voice(id, |voice| voice.set_playback_rate(rate))
            }
            PlaybackCommand::PauseEmitter(emitter) => {
                self.for_emitter(emitter, false, PlaybackInstance::pause)
            }
            PlaybackCommand::ResumeEmitter(emitter) => {
                self.for_emitter(emitter, false, PlaybackInstance::resume)
            }
            PlaybackCommand::StopEmitter(emitter) => {
                self.for_emitter(emitter, false, PlaybackInstance::begin_fade_out)
            }
            PlaybackCommand::SeekEmitter(emitter, progress) => {
                self.for_emitter(emitter, false, |voice| voice.seek(progress))
            }
            PlaybackCommand::DestroyEmitter(emitter) => {
                self.for_emitter(emitter, true, PlaybackInstance::begin_fade_out)
            }
            PlaybackCommand::UpdateEmitter(update) => {
                let (emitter, render_state, bus_index) = update.into_parts();
                self.acoustic_voices
                    .update_emitter_audibility(emitter, render_state.volume_linear());
                for voice in self.active_playback.values_mut() {
                    if voice.emitter == emitter && !voice.detached {
                        voice.render_state = render_state.clone();
                        voice.bus_index = bus_index;
                    }
                }
            }
            PlaybackCommand::UpdateBus(index, params) => {
                if let Some(bus) = self.buses.get_mut(index) {
                    *bus = params;
                }
            }
            PlaybackCommand::StopAll => {
                for voice in self.active_playback.values_mut() {
                    voice.begin_fade_out();
                }
            }
        }
    }

    fn with_voice(&mut self, id: VoiceId, operation: impl FnOnce(&mut PlaybackInstance)) {
        if let Some(voice) = self.active_playback.get_mut(&id) {
            operation(voice);
        }
    }

    fn for_emitter(
        &mut self,
        emitter: crate::domain::Emitter,
        attached_only: bool,
        mut operation: impl FnMut(&mut PlaybackInstance),
    ) {
        for voice in self.active_playback.values_mut() {
            if voice.emitter == emitter && (!attached_only || !voice.detached) {
                operation(voice);
            }
        }
    }

    fn consume_latest_spatial_frame(&mut self) {
        let Some(next) = self.spatial_frames.consume() else {
            return;
        };
        Self::apply_spatial_frame_to_voices(&next, &mut self.active_playback);
        #[cfg(test)]
        self.counters
            .rendered_spatial_revision
            .store(next.revision(), Ordering::Release);
        if let Some(previous) = self.current_spatial_frame.replace(next) {
            self.spatial_frames.retire(previous);
        }
    }

    fn apply_spatial_frame_to_voices(
        frame: &SpatialFrame,
        voices: &mut HashMap<VoiceId, PlaybackInstance>,
    ) {
        for voice in voices.values_mut() {
            if voice.detached {
                continue;
            }
            if let Some(spatial) = frame.emitter_state(voice.emitter) {
                voice.render_state.set_spatial_pose(spatial.pose);
            }
        }
    }

    fn consume_latest_acoustic_response(&mut self) {
        let Some(next) = self.acoustic_responses.consume() else {
            return;
        };
        let required_geometry_version = self.acoustic_scene_version.load(Ordering::Acquire);
        let retired = match self
            .processor
            .replace_acoustic_response_for_scene(next, required_geometry_version)
        {
            AcousticResponseReplacement::Accepted(previous) => previous,
            AcousticResponseReplacement::Rejected(rejected) => {
                self.counters
                    .acoustic_render_rejected_responses
                    .fetch_add(1, Ordering::Relaxed);
                Some(rejected)
            }
        };
        if let Some(retired) = retired {
            self.acoustic_responses.retire(retired);
        }
    }

    fn generate_samples(&mut self, samples_needed: usize) -> RenderTimingEvent {
        let total_start = Instant::now();
        let mut timing = RenderTimingEvent {
            mixing_time_us: 0,
            spatial_time_us: 0,
            direct_mixing_time_us: 0,
            spatial_source_count: 0,
            spatial_simulation_time_us: 0,
            direct_processing_time_us: 0,
            ambisonics_encoding_time_us: 0,
            ambisonics_decoding_time_us: 0,
            hrtf_rendering_time_us: 0,
            late_reverb_time_us: 0,
            early_reflection_time_us: 0,
            native_hrtf_direction_lookup_time_us: 0,
            native_hrtf_convolution_time_us: 0,
            resampling_time_us: 0,
            total_time_us: 0,
        };
        self.completed_playbacks.clear();
        let spatial_revision = self
            .current_spatial_frame
            .as_ref()
            .map_or(0, |frame| frame.revision());
        let mut total_generated = 0;
        while total_generated < samples_needed {
            self.output
                .as_mut()
                .expect("output checked before rendering")
                .world_buffer
                .fill(0.0);
            let generated_before = total_generated;
            let mixing_start = Instant::now();
            let profiling = self.mix_output_block(SpatialRenderContext {
                render_block_index: self.render_block_index,
                spatial_revision,
            });
            self.render_block_index = self.render_block_index.wrapping_add(1);
            for event in self.mixer_scratch.voice_telemetry.drain(..) {
                Self::try_send_voice_telemetry(&self.voice_telemetry_sender, &self.counters, event);
            }
            timing.mixing_time_us += mixing_start.elapsed().as_micros() as u64;
            timing.direct_mixing_time_us += profiling.direct_mix_time_us;
            timing.spatial_time_us += profiling.spatial_mix_time_us;
            if let Some(metrics) = profiling.spatial_metrics {
                timing.spatial_source_count += metrics.spatial_source_count;
                timing.spatial_simulation_time_us += metrics.physics_simulation_time_us;
                timing.direct_processing_time_us += metrics.direct_processing_time_us;
                timing.ambisonics_encoding_time_us += metrics.ambisonics_encoding_time_us;
                timing.ambisonics_decoding_time_us += metrics.ambisonics_decoding_time_us;
                timing.hrtf_rendering_time_us += metrics.hrtf_rendering_time_us;
                timing.late_reverb_time_us += metrics.late_reverb_time_us;
                timing.early_reflection_time_us += metrics.early_reflection_time_us;
                timing.native_hrtf_direction_lookup_time_us +=
                    metrics.native_hrtf_direction_lookup_time_us;
                timing.native_hrtf_convolution_time_us += metrics.native_hrtf_convolution_time_us;
            }
            let output = self
                .output
                .as_mut()
                .expect("output checked before rendering");
            let resampling_start = Instant::now();
            if let Ok((frames_out, _)) = output
                .resampler
                .process_interleaved(&output.world_buffer, &mut output.resampled_buffer)
            {
                timing.resampling_time_us += resampling_start.elapsed().as_micros() as u64;
                apply_master_gain_and_limit(
                    &mut output.resampled_buffer,
                    frames_out,
                    LOGICAL_CHANNELS as usize,
                    self.master_gain_linear,
                );
                let mut pushed = 0;
                for samples in output
                    .resampled_buffer
                    .as_chunks::<{ LOGICAL_CHANNELS as usize }>()
                    .0
                    .iter()
                    .take(frames_out)
                {
                    if output
                        .producer
                        .try_push(StereoFrame {
                            left: samples[0],
                            right: samples[1],
                        })
                        .is_ok()
                    {
                        pushed += 1;
                    } else {
                        break;
                    }
                }
                total_generated += pushed;
            }
            if total_generated == generated_before {
                break;
            }
        }
        timing.total_time_us = total_start.elapsed().as_micros() as u64;
        timing
    }

    /// Advances every audible Voice for one logical output block. All mutable mixing state is
    /// owned here, so callers supply only the immutable frame context rather than coordinating
    /// eight borrowed implementation details.
    fn mix_output_block(&mut self, render_context: SpatialRenderContext) -> MixProfilingSummary {
        self.mixer_scratch.clear();
        let output_frames = self
            .output
            .as_ref()
            .expect("output checked before mixing")
            .world_buffer
            .len()
            / LOGICAL_CHANNELS as usize;
        for (voice_id, instance) in &mut self.active_playback {
            if !matches!(instance.info.play_state, PlayState::Playing) {
                continue;
            }
            let bus = effective_bus_params(instance.bus_index, &self.buses);
            if bus.paused {
                continue;
            }
            instance.set_mix_parameters(bus);
            let is_spatial = instance.render_state.is_spatial();
            if bus.muted || crate::gain::db_to_linear(bus.gain_db) == 0.0 {
                instance.advance_silently(output_frames);
                if is_spatial {
                    self.mixer_scratch.muted_spatial_voice_ids.push(*voice_id);
                }
                continue;
            }
            if is_spatial {
                self.mixer_scratch.spatial_voice_ids.push(*voice_id);
            } else {
                self.mixer_scratch.non_spatial_voice_ids.push(*voice_id);
            }
        }

        let mut profiling = MixProfilingSummary::default();
        let direct_start = Instant::now();
        for voice_id in &self.mixer_scratch.non_spatial_voice_ids {
            if let Some(instance) = self.active_playback.get_mut(voice_id) {
                instance.fill_buffer(
                    &mut self
                        .output
                        .as_mut()
                        .expect("output checked before mixing")
                        .world_buffer,
                    LOGICAL_CHANNELS,
                );
            }
        }
        profiling.direct_mix_time_us = direct_start.elapsed().as_micros() as u64;

        for voice_id in &self.mixer_scratch.muted_spatial_voice_ids {
            self.processor.silence_voice_state(*voice_id);
        }
        if !self.mixer_scratch.spatial_voice_ids.is_empty()
            || self.processor.has_environment_tail()
            || self.processor.has_pending_voice_telemetry()
        {
            let spatial_start = Instant::now();
            if let Ok(metrics) = self.processor.process_spatial_sources_with_metrics(
                &self.mixer_scratch.spatial_voice_ids,
                &mut self.active_playback,
                &mut self
                    .output
                    .as_mut()
                    .expect("output checked before mixing")
                    .world_buffer,
                render_context,
                &mut self.mixer_scratch.voice_telemetry,
            ) {
                profiling.spatial_mix_time_us = spatial_start.elapsed().as_micros() as u64;
                profiling.spatial_metrics = Some(metrics);
            }
        }

        for (voice_id, instance) in &mut self.active_playback {
            let _ = instance.check_and_clear_end_flag();
            if let Some(reclaim) = instance.take_reclaim() {
                self.completed_playbacks
                    .push(CompletedPlayback::from_reclaim(*voice_id, reclaim));
            }
        }
        profiling
    }

    fn retire_completed_voices(&mut self) {
        while let Some(completed) = self.completed_playbacks.pop() {
            let playback = self
                .active_playback
                .remove(&completed.voice_id)
                .expect("mixer completion must still name one active Voice");
            let spatial = self.processor.retire_voice(completed.voice_id);
            let acoustic = self.acoustic_voices.retire(completed.voice_id);
            if let Some(tag) = completed.completion_tag {
                let _ = self.retirement_sender.try_send(completed.voice_id);
                Self::try_send_event(
                    &self.event_sender,
                    &self.counters,
                    PetalSonicEvent::PlaybackCompleted {
                        emitter: completed.emitter,
                        control: PlaybackControl {
                            world_id: completed.emitter.world_id,
                            voice_id: completed.voice_id,
                        },
                        tag,
                    },
                );
            }
            self.enqueue_voice_retirement(VoiceRetirement {
                _voice_id: completed.voice_id,
                _playback: playback,
                _acoustic: acoustic,
                _spatial: spatial,
            });
        }
        self.publish_acoustic_voices();
    }

    fn publish_acoustic_voices(&mut self) {
        let _ = self
            .acoustic_voice_input
            .try_publish(&mut self.acoustic_voices);
    }

    fn enqueue_voice_retirement(&mut self, retirement: VoiceRetirement) {
        if let Err(error) = self.voice_retirement_sender.try_send(retirement) {
            assert!(self.pending_voice_retirements.len() < self.max_voices);
            self.pending_voice_retirements.push(error.into_inner());
            return;
        }
        self.active_voice_count.fetch_sub(1, Ordering::AcqRel);
    }

    fn flush_voice_retirements(&mut self) {
        while let Some(retirement) = self.pending_voice_retirements.pop() {
            if let Err(error) = self.voice_retirement_sender.try_send(retirement) {
                self.pending_voice_retirements.push(error.into_inner());
                break;
            }
            self.active_voice_count.fetch_sub(1, Ordering::AcqRel);
        }
    }

    /// Transfers render-owned overflow only after its scheduling owner has quiesced output.
    /// Replacing the buffer preserves the no-allocation contract if cleanup is retried and the
    /// engine is subsequently reconciled again.
    pub(crate) fn take_quiesced_voice_retirements(&mut self) -> Vec<VoiceRetirement> {
        let retirements = std::mem::replace(
            &mut self.pending_voice_retirements,
            Vec::with_capacity(self.max_voices),
        );
        if !retirements.is_empty() {
            self.active_voice_count
                .fetch_sub(retirements.len(), Ordering::AcqRel);
        }
        retirements
    }

    fn try_send_event(
        sender: &Sender<PetalSonicEvent>,
        counters: &RuntimeCounters,
        event: PetalSonicEvent,
    ) {
        if sender.try_send(event).is_ok() {
            RuntimeCounters::observe_high_water(&counters.event_queue_high_water, sender.len());
        } else {
            counters.dropped_events.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn try_send_voice_telemetry(
        sender: &Sender<VoiceTelemetryEvent>,
        counters: &RuntimeCounters,
        event: VoiceTelemetryEvent,
    ) {
        if sender.try_send(event).is_ok() {
            RuntimeCounters::observe_high_water(
                &counters.voice_telemetry_queue_high_water,
                sender.len(),
            );
        } else {
            counters
                .dropped_voice_telemetry
                .fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn effective_bus_params(index: usize, buses: &[BusParams]) -> BusParams {
    let master = buses.first().copied().unwrap_or_default();
    let selected = buses.get(index).copied().unwrap_or(master);
    if index == 0 {
        return master;
    }
    BusParams {
        gain_db: master.gain_db + selected.gain_db,
        muted: master.muted || selected.muted,
        paused: master.paused || selected.paused,
        playback_rate: master.playback_rate * selected.playback_rate,
    }
}

fn apply_master_gain_and_limit(buffer: &mut [f32], frames_out: usize, channels: usize, gain: f32) {
    for sample in buffer.iter_mut().take(frames_out * channels) {
        *sample = (*sample * gain).clamp(-1.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acoustic_propagation::AcousticVoice;
    use crate::audio_data::PetalSonicAudioData;
    use crate::config::PetalSonicWorldDesc;
    use crate::domain::{
        DirectPath, Emitter, EmitterDesc, EmitterSpatialState, EnvironmentSend, ExtentSample,
        ExtentSampleId, OcclusionProfile, PlayOptions, SourceExtent,
    };
    use crate::playback::prepare_test_voice;
    use crate::realtime_latest::Publisher;
    use crate::spatial::LateReverbParameters;
    use ringbuf::traits::Consumer;
    use std::sync::atomic::{AtomicBool, AtomicU64};

    fn release_baseline(key: &str) -> u64 {
        include_str!("../perf/balanced_near_capacity.baseline")
            .lines()
            .filter_map(|line| line.split_once('='))
            .find_map(|(candidate, value)| {
                (candidate.trim() == key).then(|| {
                    value
                        .trim()
                        .parse()
                        .unwrap_or_else(|_| panic!("invalid {key} in release baseline"))
                })
            })
            .unwrap_or_else(|| panic!("missing {key} in release baseline"))
    }

    struct Harness {
        quantum: RenderQuantum,
        spatial_frames: Publisher<SpatialFrame>,
        acoustic_responses: Publisher<AcousticResponse>,
        commands: Sender<PlaybackCommand>,
        events: crossbeam_channel::Receiver<PetalSonicEvent>,
        timing: crossbeam_channel::Receiver<RenderTimingEvent>,
        _voice_retirement_owner: crossbeam_channel::Receiver<VoiceRetirement>,
    }

    fn harness(block_size: usize, max_voices: usize) -> Harness {
        harness_with_buses(block_size, max_voices, vec![BusParams::default()])
    }

    fn harness_with_buses(block_size: usize, max_voices: usize, buses: Vec<BusParams>) -> Harness {
        harness_with_buses_and_acoustic_input(
            block_size,
            max_voices,
            buses,
            AcousticVoiceInput::isolated(max_voices.max(1)),
        )
    }

    fn harness_with_buses_and_acoustic_input(
        block_size: usize,
        max_voices: usize,
        buses: Vec<BusParams>,
        acoustic_voice_input: AcousticVoiceInput,
    ) -> Harness {
        let desc = PetalSonicWorldDesc {
            block_size,
            max_voices,
            event_queue_capacity: max_voices.saturating_mul(4).max(8),
            timing_queue_capacity: max_voices.max(8),
            ..PetalSonicWorldDesc::default()
        };
        let (commands, regular) = crossbeam_channel::bounded(max_voices.max(1));
        let (_lifecycle, lifecycle) = crossbeam_channel::bounded(max_voices.max(1));
        let (retirement_sender, _) = crossbeam_channel::bounded(max_voices.max(1));
        let (spatial_frames, spatial_frame_consumer) = RealtimeLatest::bounded(1);
        let (acoustic_responses, acoustic_response_consumer) = RealtimeLatest::bounded(2);
        let (voice_retirement_sender, voice_retirement_owner) =
            crossbeam_channel::bounded(max_voices.max(1));
        let (event_sender, events) = crossbeam_channel::bounded(desc.event_queue_capacity);
        let (voice_telemetry_sender, _voice_telemetry) =
            crossbeam_channel::bounded(desc.event_queue_capacity);
        let (timing_sender, timing) = crossbeam_channel::bounded(desc.timing_queue_capacity);
        let startup = PreparedRender::new(
            desc,
            Arc::new(AtomicUsize::new(0)),
            retirement_sender,
            spatial_frame_consumer,
            acoustic_response_consumer,
            acoustic_voice_input,
            Arc::new(AtomicU64::new(0)),
            Arc::new(AtomicBool::new(true)),
            regular,
            lifecycle,
            event_sender,
            voice_telemetry_sender,
            timing_sender,
            Arc::new(RuntimeCounters::default()),
        );
        Harness {
            quantum: RenderQuantum::new(startup, buses, voice_retirement_sender).unwrap(),
            spatial_frames,
            acoustic_responses,
            commands,
            events,
            timing,
            _voice_retirement_owner: voice_retirement_owner,
        }
    }

    #[test]
    fn render_spatial_publication_updates_playback_without_waking_acoustics_at_capacity() {
        const EMITTERS: usize = 4_096;
        const VOICES: usize = 2_048;
        const BLOCK_SIZE: usize = 8;
        let old_pose = crate::math::Pose::from_position(-crate::math::Vec3::Y);
        let emitter = |index: usize| Emitter {
            world_id: 19,
            index: index as u32,
            generation: 7,
        };
        let acoustic_voice_input = AcousticVoiceInput::isolated(VOICES);
        let mut harness = harness_with_buses_and_acoustic_input(
            BLOCK_SIZE,
            VOICES,
            vec![BusParams::default()],
            acoustic_voice_input.clone(),
        );
        let source = Arc::new(PetalSonicAudioData::new(
            vec![0.25; BLOCK_SIZE * 2],
            48_000,
            1,
            Duration::from_secs_f64((BLOCK_SIZE * 2) as f64 / 48_000.0),
        ));
        for voice_index in 0..VOICES {
            let options = if voice_index == 0 {
                PlayOptions::looping().detached()
            } else {
                PlayOptions::looping()
            };
            harness
                .commands
                .try_send(PlaybackCommand::Play(prepare_test_voice(
                    VoiceId::from(voice_index as u64 + 1),
                    emitter(voice_index * 2),
                    source.clone(),
                    EmitterDesc::spatial(old_pose),
                    options,
                    None,
                    0,
                    BLOCK_SIZE,
                )))
                .unwrap();
        }
        harness.quantum.render();
        let wake_before = acoustic_voice_input.wake_generation_for_test();
        assert_eq!(wake_before, 1, "Voice activation must publish one baseline");

        let frame = Arc::new(
            SpatialFrame::new(
                71,
                4.0,
                crate::math::Pose::identity(),
                (0..EMITTERS)
                    .rev()
                    .map(|index| {
                        EmitterSpatialState::new(
                            emitter(index),
                            crate::math::Pose::from_position(crate::math::Vec3::new(
                                index as f32,
                                2.0,
                                0.0,
                            )),
                        )
                    })
                    .collect(),
            )
            .unwrap(),
        );
        harness
            .spatial_frames
            .prepare_latest(frame)
            .unwrap()
            .commit();

        let attached_voice = VoiceId::from(VOICES as u64);
        let expected_attached_pose = crate::math::Pose::from_position(crate::math::Vec3::new(
            ((VOICES - 1) * 2) as f32,
            2.0,
            0.0,
        ));
        let memory_activity =
            crate::test_support::realtime_memory_activity(|| harness.quantum.render());

        assert_eq!(memory_activity, 0, "render spatial consumption allocated");
        assert_eq!(
            harness.quantum.active_playback[&attached_voice]
                .render_state
                .spatial_pose(),
            Some(expected_attached_pose),
            "playback pose must still follow the render-side SpatialFrame"
        );
        assert_eq!(
            harness.quantum.active_playback[&VoiceId::from(1)]
                .render_state
                .spatial_pose(),
            Some(old_pose),
            "detached playback keeps its independently captured pose"
        );
        assert_eq!(
            acoustic_voice_input.wake_generation_for_test(),
            wake_before,
            "render-only publication must not wake the acoustic worker"
        );
    }

    fn empty_acoustic_response(revision: u64) -> Arc<AcousticResponse> {
        Arc::new(AcousticResponse {
            spatial_revision: revision,
            geometry_version: 0,
            direct: Vec::new().into(),
            late_reverb: LateReverbParameters::SILENT,
            published_at: Instant::now(),
            solve_time_us: 0,
        })
    }

    #[test]
    fn completion_reclaim_association_is_linear_for_4096_voices() {
        const VOICES: usize = 4_096;
        let mut harness = harness(1, VOICES);
        let clip = Arc::new(PetalSonicAudioData::new(
            vec![0.25],
            48_000,
            1,
            Duration::from_secs_f64(1.0 / 48_000.0),
        ));
        for voice in 0..VOICES {
            let voice_id = VoiceId::from(voice as u64 + 1);
            let emitter = Emitter {
                world_id: 1,
                index: voice as u32,
                generation: 1,
            };
            let (_, playback, acoustic) = prepare_test_voice(
                voice_id,
                emitter,
                clip.clone(),
                EmitterDesc::non_spatial(),
                PlayOptions::once(),
                None,
                0,
                1,
            )
            .start();
            assert!(acoustic.is_none());
            assert!(
                harness
                    .quantum
                    .active_playback
                    .insert(voice_id, playback)
                    .is_none()
            );
        }
        let _consumer = harness.quantum.connect_output(48_000).unwrap();

        let mut memory_activity = 0;
        let steps = crate::test_support::render_completion_step_activity(|| {
            memory_activity = crate::test_support::realtime_memory_activity(|| {
                harness.quantum.mix_output_block(SpatialRenderContext {
                    render_block_index: 0,
                    spatial_revision: 0,
                });
                harness.quantum.mix_output_block(SpatialRenderContext {
                    render_block_index: 1,
                    spatial_revision: 0,
                });
            });
        });

        assert_eq!(harness.quantum.completed_playbacks.len(), VOICES);
        assert_eq!(memory_activity, 0);
        assert!(
            steps <= VOICES * 2,
            "two 4096-Voice render blocks performed {steps} completion association steps"
        );
    }

    #[test]
    fn acoustic_replacements_progress_without_spatial_or_scene_publication() {
        let mut harness = harness(16, 1);
        let emitter = Emitter {
            world_id: 1,
            index: 0,
            generation: 1,
        };

        for revision in 1..=6 {
            let voice_id = VoiceId::from(revision);
            harness.quantum.acoustic_voices.activate(
                AcousticVoice {
                    voice_id,
                    emitter,
                    emitter_world_pose: crate::math::Pose::identity(),
                    acoustic_priority: 1.0,
                    audibility: 1.0,
                    detached: false,
                    direct_path: DirectPath::default(),
                    environment_send: EnvironmentSend::default(),
                    source_extent: SourceExtent::Point,
                    occlusion_profile: OcclusionProfile::PointExact,
                    routing_generation: 0,
                    _retirement_witness: None,
                },
                None,
            );
            harness
                .acoustic_responses
                .publish_latest(empty_acoustic_response(revision))
                .unwrap();

            harness.quantum.render();

            assert_eq!(
                harness.quantum.processor.acoustic_response_generation(),
                Some((revision, 0))
            );
            harness.quantum.acoustic_voices.retire(voice_id);
        }
    }

    fn play_command(
        voice_id: VoiceId,
        emitter: Emitter,
        clip: Arc<PetalSonicAudioData>,
        block_size: usize,
    ) -> PlaybackCommand {
        PlaybackCommand::Play(prepare_test_voice(
            voice_id,
            emitter,
            clip,
            EmitterDesc::non_spatial(),
            PlayOptions::looping(),
            None,
            0,
            block_size,
        ))
    }

    fn bus_play_command(
        voice_id: VoiceId,
        emitter: Emitter,
        sample: f32,
        bus_index: usize,
        block_size: usize,
    ) -> PlaybackCommand {
        let clip = Arc::new(PetalSonicAudioData::new(
            vec![sample; block_size * 16],
            48_000,
            1,
            Duration::from_secs_f64((block_size * 16) as f64 / 48_000.0),
        ));
        PlaybackCommand::Play(prepare_test_voice(
            voice_id,
            emitter,
            clip,
            EmitterDesc::non_spatial(),
            PlayOptions::looping(),
            None,
            bus_index,
            block_size,
        ))
    }

    fn retirement_emitter(index: u32) -> Emitter {
        Emitter {
            world_id: 1,
            index,
            generation: 1,
        }
    }

    #[test]
    fn one_quantum_applies_commands_advances_one_cursor_and_writes_stereo() {
        let block_size = 64;
        let mut harness = harness(block_size, 4);
        let emitter = Emitter {
            world_id: 1,
            index: 0,
            generation: 1,
        };
        let clip = Arc::new(PetalSonicAudioData::new(
            vec![0.25; block_size * 8],
            48_000,
            1,
            Duration::from_secs_f64((block_size * 8) as f64 / 48_000.0),
        ));
        harness
            .quantum
            .active_voice_count
            .store(1, Ordering::Release);
        harness
            .commands
            .try_send(play_command(VoiceId::from(1), emitter, clip, block_size))
            .unwrap();
        let mut consumer = harness.quantum.connect_output(48_000).unwrap();

        harness.quantum.render();

        let frame = consumer.try_pop().expect("quantum produced stereo output");
        assert!(frame.left > 0.0 && frame.right > 0.0);
        assert_eq!(
            harness.quantum.active_playback[&VoiceId::from(1)]
                .info
                .current_frame,
            block_size * 2
        );
    }

    #[test]
    fn named_bus_pause_freezes_only_its_voice_through_the_render_owner() {
        let block_size = 4;
        let mut harness = harness_with_buses(
            block_size,
            2,
            vec![
                BusParams::default(),
                BusParams {
                    paused: true,
                    ..BusParams::default()
                },
                BusParams::default(),
            ],
        );
        harness
            .quantum
            .active_voice_count
            .store(2, Ordering::Release);
        for (voice, bus, sample) in [(61, 1, 1.0), (62, 2, 0.5)] {
            harness
                .commands
                .try_send(bus_play_command(
                    VoiceId::from(voice),
                    retirement_emitter(voice as u32),
                    sample,
                    bus,
                    block_size,
                ))
                .unwrap();
        }
        let mut consumer = harness.quantum.connect_output(48_000).unwrap();

        harness.quantum.render();

        assert!(consumer.try_pop().is_some());
        assert_eq!(
            harness.quantum.active_playback[&VoiceId::from(61)]
                .info
                .current_frame,
            0
        );
        assert_eq!(
            harness.quantum.active_playback[&VoiceId::from(62)]
                .info
                .current_frame,
            block_size * 2
        );
    }

    #[test]
    fn master_and_named_bus_rates_compose_through_the_render_owner() {
        let block_size = 4;
        let mut harness = harness_with_buses(
            block_size,
            2,
            vec![
                BusParams {
                    playback_rate: 0.5,
                    ..BusParams::default()
                },
                BusParams {
                    playback_rate: 2.0,
                    ..BusParams::default()
                },
                BusParams::default(),
            ],
        );
        harness
            .quantum
            .active_voice_count
            .store(2, Ordering::Release);
        for (voice, bus) in [(63, 1), (64, 2)] {
            harness
                .commands
                .try_send(bus_play_command(
                    VoiceId::from(voice),
                    retirement_emitter(voice as u32),
                    0.25,
                    bus,
                    block_size,
                ))
                .unwrap();
        }
        let _consumer = harness.quantum.connect_output(48_000).unwrap();

        harness.quantum.render();

        assert_eq!(
            harness.quantum.active_playback[&VoiceId::from(63)]
                .info
                .current_frame,
            block_size * 2
        );
        assert_eq!(
            harness.quantum.active_playback[&VoiceId::from(64)]
                .info
                .current_frame,
            block_size
        );
    }

    #[test]
    fn output_recovery_advances_the_same_voice_timeline() {
        let block_size = 64;
        let mut harness = harness(block_size, 2);
        let emitter = Emitter {
            world_id: 1,
            index: 0,
            generation: 1,
        };
        let clip = Arc::new(PetalSonicAudioData::new(
            (0..256).map(|sample| sample as f32 / 256.0).collect(),
            48_000,
            1,
            Duration::from_secs_f64(256.0 / 48_000.0),
        ));
        harness
            .quantum
            .active_voice_count
            .store(1, Ordering::Release);
        harness
            .commands
            .try_send(play_command(VoiceId::from(7), emitter, clip, block_size))
            .unwrap();

        harness
            .quantum
            .advance_without_output(Duration::from_millis(2));
        assert_eq!(
            harness.quantum.active_playback[&VoiceId::from(7)]
                .info
                .current_frame,
            96
        );
        let mut consumer = harness.quantum.connect_output(48_000).unwrap();
        harness.quantum.render();
        assert!(consumer.try_pop().is_some());
        assert_eq!(
            harness.quantum.active_playback[&VoiceId::from(7)]
                .info
                .current_frame,
            224
        );
    }

    #[test]
    fn spatial_frame_moves_only_attached_voice_and_preserves_captured_extent() {
        let mut harness = harness(32, 2);
        let emitter = Emitter {
            world_id: 1,
            index: 4,
            generation: 2,
        };
        let old_pose = crate::math::Pose::from_position(crate::math::Vec3::X);
        let new_pose = crate::math::Pose::from_position(crate::math::Vec3::Y);
        let clip = Arc::new(PetalSonicAudioData::new(
            vec![0.25; 64],
            48_000,
            1,
            Duration::from_secs_f64(64.0 / 48_000.0),
        ));
        let captured_extent = SourceExtent::weighted_samples(vec![
            ExtentSample::new(ExtentSampleId(17), crate::math::Vec3::X, 1.0).unwrap(),
        ])
        .unwrap();
        for (id, detached) in [(1, false), (2, true)] {
            let mut options = PlayOptions::looping();
            if detached {
                options = options.detached();
            }
            let voice = prepare_test_voice(
                VoiceId::from(id),
                emitter,
                clip.clone(),
                EmitterDesc::spatial(old_pose).with_extent(captured_extent.clone()),
                options,
                None,
                0,
                32,
            )
            .start()
            .1;
            harness
                .quantum
                .active_playback
                .insert(VoiceId::from(id), voice);
        }
        let frame = SpatialFrame::new(
            1,
            0.0,
            crate::math::Pose::default(),
            vec![EmitterSpatialState::new(emitter, new_pose).with_extent(SourceExtent::Point)],
        )
        .unwrap();

        RenderQuantum::apply_spatial_frame_to_voices(&frame, &mut harness.quantum.active_playback);

        assert_eq!(
            harness.quantum.active_playback[&VoiceId::from(1)]
                .render_state
                .spatial_pose(),
            Some(new_pose)
        );
        assert_eq!(
            harness.quantum.active_playback[&VoiceId::from(2)]
                .render_state
                .spatial_pose(),
            Some(old_pose)
        );
        assert_eq!(
            harness.quantum.active_playback[&VoiceId::from(1)].source_extent,
            captured_extent
        );
    }

    #[test]
    fn spatial_frame_updates_2048_voices_with_bounded_comparisons_and_no_realtime_allocation() {
        const EMITTERS: usize = 4_096;
        const VOICES: usize = 2_048;
        const MAX_BINARY_COMPARISONS: usize = 13;
        let block_size = 8;
        let old_pose = crate::math::Pose::from_position(-crate::math::Vec3::Y);
        let source = Arc::new(PetalSonicAudioData::new(
            vec![0.25; block_size * 2],
            48_000,
            1,
            Duration::from_secs_f64((block_size * 2) as f64 / 48_000.0),
        ));
        let emitter = |index: usize| Emitter {
            world_id: 9,
            index: index as u32,
            generation: 3,
        };
        let mut voices = HashMap::with_capacity(VOICES);
        for voice_index in 0..VOICES {
            let instance = prepare_test_voice(
                VoiceId::from(voice_index as u64 + 1),
                emitter(voice_index * 2),
                source.clone(),
                EmitterDesc::spatial(old_pose),
                PlayOptions::looping(),
                None,
                0,
                block_size,
            )
            .start()
            .1;
            voices.insert(VoiceId::from(voice_index as u64 + 1), instance);
        }
        let frame = SpatialFrame::new(
            41,
            3.0,
            crate::math::Pose::identity(),
            (0..EMITTERS)
                .rev()
                .map(|index| {
                    EmitterSpatialState::new(
                        emitter(index),
                        crate::math::Pose::from_position(crate::math::Vec3::new(
                            index as f32,
                            2.0,
                            0.0,
                        )),
                    )
                })
                .collect(),
        )
        .unwrap();
        let mut comparisons = 0;

        let memory_activity = crate::test_support::realtime_memory_activity(|| {
            comparisons = crate::test_support::spatial_frame_comparison_activity(|| {
                RenderQuantum::apply_spatial_frame_to_voices(&frame, &mut voices);
            });
        });

        assert_eq!(memory_activity, 0);
        assert!(comparisons >= VOICES);
        assert!(comparisons <= VOICES * MAX_BINARY_COMPARISONS);
        let last = &voices[&VoiceId::from(VOICES as u64)];
        assert_eq!(
            last.render_state.spatial_pose().unwrap().position,
            crate::math::Vec3::new(((VOICES - 1) * 2) as f32, 2.0, 0.0)
        );
    }

    #[test]
    fn warmed_quantum_is_allocation_free() {
        const VOICES: usize = 32;
        let block_size = 64;
        let mut harness = harness(block_size, VOICES);
        let source = Arc::new(PetalSonicAudioData::new(
            vec![0.25 / VOICES as f32; block_size * 16],
            48_000,
            1,
            Duration::from_secs_f64((block_size * 16) as f64 / 48_000.0),
        ));
        harness
            .quantum
            .active_voice_count
            .store(VOICES, Ordering::Release);
        for voice in 0..VOICES {
            harness
                .commands
                .try_send(play_command(
                    VoiceId::from(voice as u64 + 1),
                    Emitter {
                        world_id: 1,
                        index: voice as u32,
                        generation: 1,
                    },
                    source.clone(),
                    block_size,
                ))
                .unwrap();
        }
        let mut consumer = harness.quantum.connect_output(44_100).unwrap();
        harness.quantum.render();
        while consumer.try_pop().is_some() {}
        while harness.timing.try_recv().is_ok() {}

        let activity = crate::test_support::realtime_memory_activity(|| {
            harness.quantum.render();
        });

        assert_eq!(activity, 0, "steady render quantum allocated or freed");
        assert!(consumer.try_pop().is_some());
        assert!(harness.events.try_recv().is_err());
    }

    #[test]
    fn latency_profiles_select_only_bounded_refill_schedules() {
        let responsive = RenderSchedule::for_profile(LatencyProfile::Responsive);
        let balanced = RenderSchedule::for_profile(LatencyProfile::Balanced);
        let robust = RenderSchedule::for_profile(LatencyProfile::Robust);

        assert!(responsive.ring_blocks < balanced.ring_blocks);
        assert!(balanced.ring_blocks < robust.ring_blocks);
        for schedule in [responsive, balanced, robust] {
            assert!(schedule.low_water_blocks < schedule.high_water_blocks);
            assert!(schedule.high_water_blocks <= schedule.ring_blocks);
            assert!(schedule.catch_up_chunk_blocks <= schedule.high_water_blocks);
        }
    }

    #[test]
    #[ignore = "release-mode near-capacity render performance gate"]
    fn warmed_near_capacity_quantum_meets_release_budget() {
        const VOICES: usize = 32;
        const SAMPLES: usize = 1_024;
        let block_size = 64;
        let mut harness = harness(block_size, VOICES);
        let source = Arc::new(PetalSonicAudioData::new(
            vec![0.25 / VOICES as f32; block_size * 16],
            48_000,
            1,
            Duration::from_secs_f64((block_size * 16) as f64 / 48_000.0),
        ));
        harness
            .quantum
            .active_voice_count
            .store(VOICES, Ordering::Release);
        for voice in 0..VOICES {
            harness
                .commands
                .try_send(play_command(
                    VoiceId::from(voice as u64 + 1),
                    Emitter {
                        world_id: 1,
                        index: voice as u32,
                        generation: 1,
                    },
                    source.clone(),
                    block_size,
                ))
                .unwrap();
        }
        let mut consumer = harness.quantum.connect_output(44_100).unwrap();
        harness.quantum.render();

        let mut elapsed_us = [0u64; SAMPLES];
        for elapsed in &mut elapsed_us {
            while consumer.try_pop().is_some() {}
            while harness.timing.try_recv().is_ok() {}
            let start = Instant::now();
            harness.quantum.render();
            *elapsed = start.elapsed().as_micros() as u64;
        }
        elapsed_us.sort_unstable();
        let p99 = elapsed_us[elapsed_us.len() * 99 / 100];
        let device_period_us = block_size as u64 * 1_000_000 / 48_000;
        if !cfg!(debug_assertions) {
            assert_eq!(release_baseline("voices"), VOICES as u64);
            assert_eq!(release_baseline("world_sample_rate"), 48_000);
            assert_eq!(release_baseline("device_sample_rate"), 44_100);
            assert_eq!(release_baseline("block_size"), block_size as u64);
            assert!(
                p99 * 100 < device_period_us * 80,
                "full-quantum p99 {p99}us lacks 20% device-period margin"
            );
            let limit = release_baseline("p99_us")
                .saturating_mul(100 + release_baseline("max_p99_regression_percent"))
                .div_ceil(100);
            assert!(
                p99 <= limit,
                "full-quantum p99 regressed: current={p99}us limit={limit}us"
            );
        }
    }
}
