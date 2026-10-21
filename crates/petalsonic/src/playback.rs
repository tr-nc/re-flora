//! Playback control and state management.
//!
//! This module provides types and functionality for controlling audio playback:
//! - [`LoopMode`]: Control how audio loops (once, infinite)
//! - [`PlayState`]: Current playback state (playing, paused, stopped)
//! - [`PlaybackInfo`]: Detailed playback position and timing information
//! - [`PlaybackInstance`]: Active playback instance with state management
//! - [`PlaybackCommand`]: Commands for controlling playback (internal)
//!
//! Most users will interact with playback through [`PetalSonicWorld`](crate::PetalSonicWorld)
//! methods like `play()`, `pause()`, and `stop()`, rather than using these types directly.

use crate::acoustic_propagation::AcousticVoice;
use crate::audio_data::PetalSonicAudioData;
use crate::domain::{
    BusParams, DirectPath, Emitter, EmitterDesc, EnvironmentOrigin, EnvironmentSend,
    OcclusionProfile, PlayCommandId, PlayOptions, PlaybackTag, ResidentClip, SourceExtent, VoiceId,
};
use std::fmt;
use std::sync::Arc;

/// Loop mode for audio playback
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoopMode {
    /// Play once and stop
    /// Emits SourceCompleted event when finished
    #[default]
    Once,
    /// Loop infinitely
    Infinite,
}

/// Playback-owned pose and gain state consumed by the mixer and spatial renderer.
///
/// Domain values are captured once at Voice admission or Emitter update. Callers outside this
/// module can query behavior, but cannot assemble a second representation of the same state.
#[derive(Debug, Clone)]
pub(crate) enum VoiceRenderState {
    NonSpatial {
        volume_db: f32,
    },
    Spatial {
        pose: crate::math::Pose,
        volume_db: f32,
    },
}

impl VoiceRenderState {
    fn capture(emitter_desc: &EmitterDesc, voice_gain_db: f32) -> Self {
        let volume_db = emitter_desc.gain_db() + voice_gain_db;
        match emitter_desc.pose() {
            Some(pose) => Self::Spatial { pose, volume_db },
            None => Self::NonSpatial { volume_db },
        }
    }

    pub(crate) fn is_spatial(&self) -> bool {
        matches!(self, Self::Spatial { .. })
    }

    pub(crate) fn spatial_pose(&self) -> Option<crate::math::Pose> {
        match self {
            Self::Spatial { pose, .. } => Some(*pose),
            Self::NonSpatial { .. } => None,
        }
    }

    pub(crate) fn set_spatial_pose(&mut self, next_pose: crate::math::Pose) -> bool {
        match self {
            Self::Spatial { pose, .. } => {
                *pose = next_pose;
                true
            }
            Self::NonSpatial { .. } => false,
        }
    }

    pub(crate) fn volume_db(&self) -> f32 {
        match self {
            Self::Spatial { volume_db, .. } | Self::NonSpatial { volume_db } => *volume_db,
        }
    }

    pub(crate) fn volume_linear(&self) -> f32 {
        crate::gain::db_to_linear(self.volume_db())
    }
}

/// One validated low-frequency Emitter update captured for the render owner.
pub(crate) struct EmitterUpdate {
    emitter: Emitter,
    render_state: VoiceRenderState,
    bus_index: usize,
}

impl EmitterUpdate {
    pub(crate) fn capture(emitter: Emitter, desc: &EmitterDesc, bus_index: usize) -> Self {
        Self {
            emitter,
            render_state: VoiceRenderState::capture(desc, 0.0),
            bus_index,
        }
    }

    pub(crate) fn into_parts(self) -> (Emitter, VoiceRenderState, usize) {
        (self.emitter, self.render_state, self.bus_index)
    }
}

/// Represents the current playback state of an audio source.
///
/// Used to track whether an audio source is currently playing, paused, or stopped.
#[derive(Debug, Clone)]
pub enum PlayState {
    /// Audio is currently playing
    Playing,
    /// Audio is paused (retains playback position)
    Paused,
    /// Audio is stopped (playback position may be reset)
    Stopped,
}

/// Information about the current playback state of an audio source
#[derive(Debug, Clone)]
pub struct PlaybackInfo {
    /// Current playback position in frames
    pub current_frame: usize,
    /// Total number of frames in the audio
    pub total_frames: usize,
    /// Current playback time in seconds
    pub current_time: f64,
    /// Current playback state
    pub play_state: PlayState,
}

impl PlaybackInfo {
    pub fn new(total_frames: usize) -> Self {
        Self {
            current_frame: 0,
            total_frames,
            current_time: 0.0,
            play_state: PlayState::Stopped,
        }
    }

    pub fn update_position(&mut self, current_frame: usize, sample_rate: u32) {
        self.current_frame = current_frame.min(self.total_frames);
        self.current_time = self.current_frame as f64 / sample_rate as f64;
    }

    pub fn is_finished(&self) -> bool {
        self.current_frame >= self.total_frames
    }
}

/// One Voice whose caller-facing validation, identity, and immutable routing were accepted.
///
/// The World captures this value from a validated Emitter snapshot. The runtime can then prepare
/// render-owned storage without reconstructing or independently interpreting the accepted facts.
pub(crate) struct AcceptedVoice {
    sample_envelope: Option<crate::SampleEnvelopeControl>,
    voice_id: VoiceId,
    emitter: Emitter,
    audio_data: Arc<PetalSonicAudioData>,
    render_state: VoiceRenderState,
    loop_mode: LoopMode,
    bus_index: usize,
    playback_rate: f32,
    detached: bool,
    completion_tag: Option<PlaybackTag>,
    direct_path: DirectPath,
    environment_send: EnvironmentSend,
    play_command_id: Option<PlayCommandId>,
    acoustic_start: bool,
    source_extent: SourceExtent,
    occlusion_profile: OcclusionProfile,
}

impl AcceptedVoice {
    pub(crate) fn capture(
        voice_id: VoiceId,
        emitter: Emitter,
        clip: &ResidentClip,
        emitter_desc: &EmitterDesc,
        options: PlayOptions,
        completion_tag: Option<PlaybackTag>,
        bus_index: usize,
    ) -> Self {
        Self {
            voice_id,
            emitter,
            audio_data: clip.data.clone(),
            sample_envelope: clip.sample_envelope.clone(),
            render_state: VoiceRenderState::capture(emitter_desc, options.gain_db),
            loop_mode: options.loop_mode,
            bus_index,
            playback_rate: options.playback_rate(),
            detached: options.detached,
            completion_tag,
            direct_path: options.direct_path(),
            environment_send: options.environment_send(),
            play_command_id: options.play_command_id(),
            acoustic_start: options.acoustic_start(),
            source_extent: emitter_desc.extent().clone(),
            occlusion_profile: options.occlusion_profile(emitter_desc.occlusion_profile()),
        }
    }

    /// Performs every allocation required to hand this Voice to the render thread.
    pub(crate) fn prepare(self, block_size: usize) -> PreparedVoice {
        PreparedVoice {
            accepted: self,
            mono_scratch: vec![0.0; block_size],
            #[cfg(test)]
            acoustic_retirement_witness: None,
        }
    }
}

/// A fully allocated Voice ready for one allocation-free render-quantum admission.
pub(crate) struct PreparedVoice {
    accepted: AcceptedVoice,
    mono_scratch: Vec<f32>,
    #[cfg(test)]
    acoustic_retirement_witness:
        Option<crate::acoustic_propagation::AcousticVoiceRetirementWitness>,
}

impl PreparedVoice {
    /// Starts the single playback cursor and derives its matching acoustics route from the same
    /// captured facts. No caller can accidentally prepare the two consumers differently.
    pub(crate) fn start(self) -> (VoiceId, PlaybackInstance, Option<AcousticVoice>) {
        let Self {
            accepted,
            mono_scratch,
            #[cfg(test)]
            acoustic_retirement_witness,
        } = self;
        let AcceptedVoice {
            sample_envelope,
            voice_id,
            emitter,
            audio_data,
            render_state,
            loop_mode,
            bus_index,
            playback_rate,
            detached,
            completion_tag,
            direct_path,
            environment_send,
            play_command_id,
            acoustic_start,
            source_extent,
            occlusion_profile,
        } = accepted;
        let acoustic_voice = render_state.spatial_pose().map(|pose| AcousticVoice {
            voice_id,
            emitter,
            emitter_world_pose: pose,
            acoustic_priority: 1.0,
            audibility: render_state.volume_linear(),
            detached,
            direct_path,
            environment_send,
            source_extent: source_extent.clone(),
            occlusion_profile,
            routing_generation: 0,
            #[cfg(test)]
            _retirement_witness: acoustic_retirement_witness,
        });
        let total_frames = audio_data.total_frames();
        let sample_rate = audio_data.sample_rate();
        let mut playback = PlaybackInstance {
            sample_envelope: sample_envelope.map(crate::SampleEnvelopeRenderer::new),
            emitter,
            detached,
            completion_tag,
            direct_path,
            environment_send,
            source_extent,
            occlusion_profile,
            play_command_id,
            acoustic_start_wait_frames: acoustic_start.then_some(0),
            first_render_pending: play_command_id.is_some(),
            environment_response_pending: play_command_id.is_some()
                && !matches!(environment_send.origin(), EnvironmentOrigin::Disabled),
            audio_data,
            info: PlaybackInfo::new(total_frames),
            render_state,
            loop_mode,
            bus_index,
            reached_end_this_iteration: false,
            sample_rate,
            cursor: 0.0,
            voice_rate: playback_rate,
            mix_gain_linear: 1.0,
            mix_rate: playback_rate,
            mono_scratch,
            fade_out_remaining_frames: 0,
            fade_out_total_frames: 0,
            retired: false,
            reclaim_issued: false,
        };
        playback.play_from_beginning();
        (voice_id, playback, acoustic_voice)
    }
}

#[cfg(test)]
impl PreparedVoice {
    pub(crate) fn with_acoustic_retirement_witness(
        mut self,
        witness: crate::acoustic_propagation::AcousticVoiceRetirementWitness,
    ) -> Self {
        self.acoustic_retirement_witness = Some(witness);
        self
    }
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_test_voice(
    voice_id: VoiceId,
    emitter: Emitter,
    audio_data: Arc<PetalSonicAudioData>,
    emitter_desc: EmitterDesc,
    options: PlayOptions,
    completion_tag: Option<PlaybackTag>,
    bus_index: usize,
    block_size: usize,
) -> PreparedVoice {
    AcceptedVoice::capture(
        voice_id,
        emitter,
        &ResidentClip::from_audio_data(audio_data),
        &emitter_desc,
        options,
        completion_tag,
        bus_index,
    )
    .prepare(block_size)
}

/// Active playback instance
#[derive(Debug)]
pub struct PlaybackInstance {
    sample_envelope: Option<crate::SampleEnvelopeRenderer>,
    /// Logical emitter that initiated this voice.
    pub emitter: Emitter,
    /// Detached voices survive emitter destruction and stop following emitter updates.
    pub detached: bool,
    /// Present only for explicitly controlled playback.
    pub completion_tag: Option<PlaybackTag>,
    /// Immutable direct-path semantics captured when this Voice was created.
    pub direct_path: DirectPath,
    /// Immutable environment routing captured when this Voice was created.
    pub environment_send: EnvironmentSend,
    /// Immutable local source domain captured when this Voice was created.
    pub source_extent: SourceExtent,
    /// Immutable geometry-response policy, orthogonal to extent and route placement.
    pub occlusion_profile: OcclusionProfile,
    play_command_id: Option<PlayCommandId>,
    pub(crate) acoustic_start_wait_frames: Option<usize>,
    first_render_pending: bool,
    environment_response_pending: bool,
    /// Immutable resident PCM shared by playback voices.
    pub audio_data: Arc<PetalSonicAudioData>,
    /// Current playback information
    pub info: PlaybackInfo,
    /// Source configuration (spatial/non-spatial)
    pub(crate) render_state: VoiceRenderState,
    /// Loop mode for this playback
    pub loop_mode: LoopMode,
    /// Fixed bus route selected when the Voice is created. Zero is Master.
    pub bus_index: usize,
    /// Flag to track if we've reached the end this iteration (for event emission)
    pub(crate) reached_end_this_iteration: bool,
    sample_rate: u32,
    cursor: f64,
    voice_rate: f32,
    mix_gain_linear: f32,
    mix_rate: f32,
    mono_scratch: Vec<f32>,
    fade_out_remaining_frames: usize,
    fade_out_total_frames: usize,
    retired: bool,
    reclaim_issued: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PlaybackReclaim {
    pub emitter: Emitter,
    pub completion_tag: Option<PlaybackTag>,
}

impl PlaybackInstance {
    /// Resume playing from current position
    pub fn resume(&mut self) {
        self.info.play_state = PlayState::Playing;
    }

    /// Reset playback cursor to the beginning
    pub fn reset(&mut self) {
        self.info.current_frame = 0;
        self.info.current_time = 0.0;
        self.cursor = 0.0;
        self.reached_end_this_iteration = false;
    }

    /// Play from the beginning (reset + resume)
    pub fn play_from_beginning(&mut self) {
        self.reset();
        self.retired = false;
        self.reclaim_issued = false;
        self.fade_out_remaining_frames = 0;
        self.fade_out_total_frames = 0;
        self.resume();
    }

    /// Retire an audible voice using a short, bounded de-click ramp.
    pub(crate) fn begin_fade_out(&mut self) {
        // Explicit stop/destroy does not report a natural-completion event.
        self.completion_tag = None;
        if !matches!(self.info.play_state, PlayState::Playing) {
            self.info.play_state = PlayState::Stopped;
            self.retired = true;
            return;
        }
        let fade_frames = (self.sample_rate as usize / 200).max(1);
        self.fade_out_total_frames = fade_frames;
        self.fade_out_remaining_frames = fade_frames;
    }

    /// Pending spatial audio has never been audible: cancel without a fade or completion.
    pub(crate) fn cancel_acoustic_start(&mut self) {
        self.completion_tag = None;
        self.reached_end_this_iteration = false;
        self.info.play_state = PlayState::Stopped;
        self.retired = true;
    }

    pub(crate) fn fading_out(&self) -> bool {
        self.fade_out_remaining_frames != 0
    }

    /// Pause this instance
    pub fn pause(&mut self) {
        self.info.play_state = PlayState::Paused;
    }

    /// Seek to a specific progress position (0.0 = start, 1.0 = end)
    ///
    pub fn seek(&mut self, progress: f32) {
        let progress_clamped = progress.clamp(0.0, 1.0);
        let total_frames = self.audio_data.total_frames();
        let target_frame = (total_frames as f32 * progress_clamped) as usize;

        self.info.current_frame = target_frame.min(total_frames);
        self.cursor = self.info.current_frame as f64;
        self.info
            .update_position(self.info.current_frame, self.audio_data.sample_rate());

        self.reached_end_this_iteration = false;
    }

    pub(crate) fn set_mix_parameters(&mut self, bus: BusParams) {
        self.mix_gain_linear = if bus.muted {
            0.0
        } else {
            crate::gain::db_to_linear(bus.gain_db)
        };
        self.mix_rate = self.voice_rate * bus.playback_rate;
    }

    pub(crate) fn set_playback_rate(&mut self, playback_rate: f32) {
        self.voice_rate = playback_rate;
    }

    /// Returns true only for acoustic-start timeout (including the muted shortcut).
    pub(crate) fn advance_silently(&mut self, output_frames: usize) -> bool {
        if output_frames == 0 || !matches!(self.info.play_state, PlayState::Playing) {
            return false;
        }
        // Muted spatial Voices bypass DSP, not the pending-start cursor contract.
        if let Some(waited) = self.acoustic_start_wait_frames {
            if self.fading_out() {
                self.cancel_acoustic_start();
            } else {
                let next = waited.saturating_add(output_frames);
                let limit = (crate::domain::ACOUSTIC_START_MAX_DELAY.as_secs_f64()
                    * self.sample_rate as f64)
                    .ceil() as usize;
                if next >= limit {
                    self.cancel_acoustic_start();
                    return true;
                } else {
                    self.acoustic_start_wait_frames = Some(next);
                }
            }
            return false;
        }
        let total_frames = self.audio_data.total_frames();
        if total_frames == 0 {
            self.reached_end_this_iteration = true;
            self.info.play_state = PlayState::Stopped;
            return false;
        }

        if self.fade_out_remaining_frames > 0 {
            self.fade_out_remaining_frames =
                self.fade_out_remaining_frames.saturating_sub(output_frames);
            if self.fade_out_remaining_frames == 0 {
                self.info.play_state = PlayState::Stopped;
                self.retired = true;
            }
        }

        self.cursor += output_frames as f64 * self.mix_rate as f64;
        match self.loop_mode {
            LoopMode::Infinite => {
                if self.cursor >= total_frames as f64 {
                    self.cursor %= total_frames as f64;
                    self.reached_end_this_iteration = true;
                }
                self.info
                    .update_position(self.cursor.floor() as usize, self.sample_rate);
            }
            LoopMode::Once => {
                if self.cursor >= total_frames as f64 {
                    self.cursor = total_frames as f64;
                    self.info.update_position(total_frames, self.sample_rate);
                    self.info.play_state = PlayState::Stopped;
                    self.reached_end_this_iteration = true;
                } else {
                    self.info
                        .update_position(self.cursor.floor() as usize, self.sample_rate);
                }
            }
        }
        false
    }

    fn next_source_frame(&mut self) -> Option<usize> {
        let total_frames = self.audio_data.total_frames();

        if total_frames == 0 {
            self.reached_end_this_iteration = true;
            self.info.play_state = PlayState::Stopped;
            return None;
        }

        if self.cursor >= total_frames as f64 {
            match self.loop_mode {
                LoopMode::Infinite => {
                    self.cursor %= total_frames as f64;
                    self.reached_end_this_iteration = true;
                }
                LoopMode::Once => {
                    self.info.update_position(total_frames, self.sample_rate);
                    self.info.play_state = PlayState::Stopped;
                    self.reached_end_this_iteration = true;
                    return None;
                }
            }
        }

        let source_frame = self.cursor.floor() as usize;
        self.cursor += self.mix_rate as f64;
        if self.cursor >= total_frames as f64 {
            match self.loop_mode {
                LoopMode::Infinite => {
                    self.cursor %= total_frames as f64;
                    self.reached_end_this_iteration = true;
                }
                LoopMode::Once => {
                    self.reached_end_this_iteration = true;
                    self.info.play_state = PlayState::Stopped;
                }
            }
        }

        let next_frame = if matches!(self.loop_mode, LoopMode::Once)
            && !matches!(self.info.play_state, PlayState::Playing)
        {
            total_frames
        } else {
            self.cursor.floor() as usize
        };
        self.info.update_position(next_frame, self.sample_rate);
        Some(source_frame)
    }

    /// Fill a mono buffer for this instance and apply `volume`.
    pub fn fill_mono_buffer(&mut self, buffer: &mut [f32], volume: f32) -> usize {
        buffer.fill(0.0);

        if !matches!(self.info.play_state, PlayState::Playing) {
            return 0;
        }

        let channels = self.audio_data.channels().max(1) as usize;
        let total_frames = self.audio_data.total_frames();
        if total_frames == 0 {
            self.reached_end_this_iteration = true;
            self.info.play_state = PlayState::Stopped;
            return 0;
        }

        let mut frames_filled = 0;
        if let Some(envelope) = &mut self.sample_envelope {
            envelope.begin_block();
        }

        for out_sample in buffer.iter_mut() {
            let Some(source_frame) = self.next_source_frame() else {
                break;
            };

            let base_idx = source_frame * channels;
            let mut mono = 0.0;
            for channel in 0..channels {
                mono += self
                    .audio_data
                    .samples()
                    .get(base_idx + channel)
                    .copied()
                    .unwrap_or(0.0);
            }
            let fade_gain = if self.fade_out_remaining_frames > 0 {
                let gain = self.fade_out_remaining_frames as f32
                    / self.fade_out_total_frames.max(1) as f32;
                self.fade_out_remaining_frames -= 1;
                if self.fade_out_remaining_frames == 0 {
                    self.info.play_state = PlayState::Stopped;
                    self.retired = true;
                }
                gain
            } else {
                1.0
            };
            let envelope_gain = self
                .sample_envelope
                .as_mut()
                .map_or(1.0, |e| e.next_gain(self.sample_rate));
            *out_sample = (mono / channels as f32)
                * volume
                * self.mix_gain_linear
                * fade_gain
                * envelope_gain;
            frames_filled += 1;
            if self.retired {
                break;
            }
        }

        frames_filled
    }

    /// Fill audio buffer for this instance.
    /// Returns the number of frames actually filled.
    pub fn fill_buffer(&mut self, buffer: &mut [f32], channels: u16) -> usize {
        let channels_usize = channels as usize;
        if channels_usize == 0 {
            return 0;
        }
        let frame_count = buffer.len() / channels_usize;
        if frame_count == 0 {
            return 0;
        }

        let mut scratch = std::mem::take(&mut self.mono_scratch);
        if scratch.len() < frame_count {
            scratch.resize(frame_count, 0.0);
        }

        let volume = self.render_state.volume_linear();
        let frames_filled = self.fill_mono_buffer(&mut scratch[..frame_count], volume);

        for (frame_idx, sample) in scratch.iter().copied().take(frames_filled).enumerate() {
            for channel in 0..channels_usize {
                let buffer_idx = frame_idx * channels_usize + channel;
                if buffer_idx < buffer.len() {
                    buffer[buffer_idx] += sample;
                }
            }
        }

        self.mono_scratch = scratch;
        frames_filled
    }

    /// Check if this instance reached the end of playback this iteration
    /// Returns the loop mode when this voice crossed its end boundary.
    pub fn check_and_clear_end_flag(&mut self) -> Option<LoopMode> {
        if self.reached_end_this_iteration {
            self.reached_end_this_iteration = false;
            Some(self.loop_mode)
        } else {
            None
        }
    }

    pub(crate) fn take_reclaim(&mut self) -> Option<PlaybackReclaim> {
        #[cfg(test)]
        crate::test_support::record_render_completion_step();
        let reclaimable =
            self.retired || matches!(self.loop_mode, LoopMode::Once) && self.info.is_finished();
        if self.reclaim_issued || !reclaimable {
            return None;
        }
        self.reclaim_issued = true;
        Some(PlaybackReclaim {
            emitter: self.emitter,
            completion_tag: self.completion_tag,
        })
    }

    pub(crate) fn take_first_render_command_id(&mut self) -> Option<PlayCommandId> {
        if !self.first_render_pending {
            return None;
        }
        self.first_render_pending = false;
        self.play_command_id
    }

    pub(crate) fn telemetry_command_id(&self) -> Option<PlayCommandId> {
        self.play_command_id
    }

    pub(crate) fn pending_environment_response_id(&self) -> Option<PlayCommandId> {
        self.environment_response_pending
            .then_some(self.play_command_id)
            .flatten()
    }

    pub(crate) fn mark_environment_response_reported(&mut self) {
        self.environment_response_pending = false;
    }
}

/// Commands that can be sent to the audio engine for playback control.
///
/// These commands are used internally to communicate between the main thread
/// and the audio processing thread. Most users will interact with playback
/// through [`PetalSonicWorld`](crate::PetalSonicWorld) methods instead.
///
/// # Variants
///
/// - `Play`: Start playing an audio source with specified configuration and loop mode
/// - `Pause`: Pause a playing audio source
/// - `Stop`: Stop an audio source and reset its position
/// - `StopAll`: Stop all currently playing audio sources
/// - `UpdateConfig`: Update the spatial configuration of a playing source
/// - `Seek`: Seek to a specific position in the audio (0.0 = start, 1.0 = end)
// Keeping Play inline lets the render thread move its owned buffers into a Voice without
// allocating or freeing an extra command box at a render-quantum boundary.
#[allow(clippy::large_enum_variant)]
pub enum PlaybackCommand {
    Play(PreparedVoice),
    PauseVoice(VoiceId),
    StopVoice(VoiceId),
    SeekVoice(VoiceId, f32),
    ResumeVoice(VoiceId),
    SetVoiceRate(VoiceId, f32),
    PauseEmitter(Emitter),
    ResumeEmitter(Emitter),
    StopEmitter(Emitter),
    SeekEmitter(Emitter, f32),
    DestroyEmitter(Emitter),
    StopAll,
    UpdateEmitter(EmitterUpdate),
    UpdateBus(usize, BusParams),
}

impl fmt::Debug for PlaybackCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Play(_) => f.write_str("Play(PreparedVoice)"),
            Self::PauseVoice(voice_id) => f.debug_tuple("PauseVoice").field(voice_id).finish(),
            Self::StopVoice(voice_id) => f.debug_tuple("StopVoice").field(voice_id).finish(),
            Self::SeekVoice(voice_id, progress) => f
                .debug_tuple("SeekVoice")
                .field(voice_id)
                .field(progress)
                .finish(),
            Self::ResumeVoice(voice_id) => f.debug_tuple("ResumeVoice").field(voice_id).finish(),
            Self::SetVoiceRate(voice_id, rate) => f
                .debug_tuple("SetVoiceRate")
                .field(voice_id)
                .field(rate)
                .finish(),
            Self::PauseEmitter(emitter) => f.debug_tuple("PauseEmitter").field(emitter).finish(),
            Self::ResumeEmitter(emitter) => f.debug_tuple("ResumeEmitter").field(emitter).finish(),
            Self::StopEmitter(emitter) => f.debug_tuple("StopEmitter").field(emitter).finish(),
            Self::SeekEmitter(emitter, progress) => f
                .debug_tuple("SeekEmitter")
                .field(emitter)
                .field(progress)
                .finish(),
            Self::DestroyEmitter(emitter) => {
                f.debug_tuple("DestroyEmitter").field(emitter).finish()
            }
            Self::StopAll => f.write_str("StopAll"),
            Self::UpdateEmitter(update) => f
                .debug_tuple("UpdateEmitter")
                .field(&update.emitter)
                .field(&update.bus_index)
                .finish(),
            Self::UpdateBus(index, params) => f
                .debug_tuple("UpdateBus")
                .field(index)
                .field(params)
                .finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        EmitterDesc, ExtentSample, ExtentSampleId, PlayCommandId, PlayOptions, ResidentClip,
    };
    use crate::math::{Pose, Vec3};
    use std::time::Duration;

    #[test]
    fn accepted_voice_prepares_one_playback_cursor_and_matching_acoustic_route() {
        let emitter = Emitter {
            world_id: 7,
            index: 3,
            generation: 2,
        };
        let audio = Arc::new(PetalSonicAudioData::new(
            vec![0.25; 256],
            48_000,
            1,
            Duration::from_secs_f64(256.0 / 48_000.0),
        ));
        let clip = ResidentClip::from_audio_data(audio.clone());
        let extent = SourceExtent::weighted_samples(vec![
            ExtentSample::new(ExtentSampleId(11), Vec3::new(-1.0, 0.0, 0.0), 1.0).unwrap(),
            ExtentSample::new(ExtentSampleId(12), Vec3::new(1.0, 0.0, 0.0), 3.0).unwrap(),
        ])
        .unwrap();
        let emitter_pose = Pose::from_position(Vec3::new(8.0, 1.0, -2.0));
        let direct_pose = Pose::from_position(Vec3::new(0.25, 0.0, -0.5));
        let acoustic_origin = Pose::from_position(Vec3::new(4.0, 2.0, 9.0));
        let desc = EmitterDesc::spatial(emitter_pose)
            .with_gain_db(-3.0)
            .with_extent(extent.clone());
        let options = PlayOptions::looping()
            .with_gain_db(-2.0)
            .with_playback_rate(0.5)
            .detached()
            .with_direct_path(DirectPath::listener_relative(direct_pose))
            .with_environment_send(EnvironmentSend::from_world_pose(acoustic_origin))
            .with_play_command_id(PlayCommandId(91));

        let prepared = AcceptedVoice::capture(
            VoiceId::from(23),
            emitter,
            &clip,
            &desc,
            options,
            Some(PlaybackTag(5)),
            4,
        )
        .prepare(64);
        let (voice_id, instance, acoustic_voice) = prepared.start();

        assert_eq!(voice_id, VoiceId::from(23));
        assert_eq!(instance.emitter, emitter);
        assert!(Arc::ptr_eq(&instance.audio_data, &audio));
        assert_eq!(instance.render_state.spatial_pose(), Some(emitter_pose));
        assert_eq!(instance.render_state.volume_db(), -5.0);
        assert!(matches!(instance.loop_mode, LoopMode::Infinite));
        assert!(instance.detached);
        assert_eq!(instance.completion_tag, Some(PlaybackTag(5)));
        assert_eq!(instance.bus_index, 4);
        assert_eq!(
            instance.direct_path,
            DirectPath::listener_relative(direct_pose)
        );
        assert_eq!(
            instance.environment_send,
            EnvironmentSend::from_world_pose(acoustic_origin)
        );
        assert_eq!(instance.source_extent, extent);
        assert_eq!(instance.play_command_id, Some(PlayCommandId(91)));
        assert_eq!(instance.mono_scratch.len(), 64);
        assert!(matches!(instance.info.play_state, PlayState::Playing));

        let acoustic_voice = acoustic_voice.expect("spatial Voice must have an acoustic route");
        assert_eq!(acoustic_voice.voice_id, VoiceId::from(23));
        assert_eq!(acoustic_voice.emitter, emitter);
        assert_eq!(acoustic_voice.emitter_world_pose, emitter_pose);
        assert!(acoustic_voice.detached);
        assert_eq!(acoustic_voice.direct_path, instance.direct_path);
        assert_eq!(acoustic_voice.environment_send, instance.environment_send);
        assert_eq!(acoustic_voice.source_extent, instance.source_extent);
        assert_eq!(acoustic_voice.occlusion_profile, instance.occlusion_profile);
    }

    #[test]
    fn resident_clip_fills_stereo_and_seeks() {
        let audio = Arc::new(PetalSonicAudioData::new(
            vec![0.0, 1.0, 2.0, 3.0],
            48_000,
            1,
            Duration::from_secs_f64(4.0 / 48_000.0),
        ));
        let mut instance = prepare_test_voice(
            VoiceId::from(1),
            Emitter {
                world_id: 1,
                index: 0,
                generation: 1,
            },
            audio,
            EmitterDesc::non_spatial(),
            PlayOptions::looping(),
            None,
            0,
            4,
        )
        .start()
        .1;

        let mut stereo = [0.0; 8];
        let frames = instance.fill_buffer(&mut stereo, 2);
        assert_eq!(frames, 4);
        assert_eq!(stereo, [0.0, 0.0, 1.0, 1.0, 2.0, 2.0, 3.0, 3.0]);
        assert_eq!(instance.info.current_frame, 0);

        instance.seek(0.5);
        assert_eq!(instance.info.current_frame, 2);

        let mut mono = [0.0; 2];
        let frames = instance.fill_mono_buffer(&mut mono, 0.5);
        assert_eq!(frames, 2);
        assert_eq!(mono, [1.0, 1.5]);
    }

    #[test]
    fn playback_rate_changes_source_progress_and_muted_time_still_advances() {
        let audio = Arc::new(PetalSonicAudioData::new(
            vec![0.0, 1.0, 2.0, 3.0],
            48_000,
            1,
            Duration::from_secs_f64(4.0 / 48_000.0),
        ));
        let mut instance = prepare_test_voice(
            VoiceId::from(1),
            Emitter {
                world_id: 1,
                index: 0,
                generation: 1,
            },
            audio,
            EmitterDesc::non_spatial(),
            PlayOptions::once().with_playback_rate(2.0),
            None,
            0,
            4,
        )
        .start()
        .1;
        instance.set_mix_parameters(BusParams::default());

        let mut output = [0.0; 4];
        assert_eq!(instance.fill_mono_buffer(&mut output, 1.0), 2);
        assert_eq!(output, [0.0, 2.0, 0.0, 0.0]);
        assert!(instance.info.is_finished());

        instance.reset();
        instance.resume();
        instance.set_playback_rate(1.0);
        instance.set_mix_parameters(BusParams {
            muted: true,
            ..BusParams::default()
        });
        instance.advance_silently(3);
        assert_eq!(instance.info.current_frame, 3);
    }

    #[test]
    fn explicit_stop_uses_bounded_declick_ramp_before_reclaim() {
        let audio = Arc::new(PetalSonicAudioData::new(
            vec![1.0; 1_000],
            48_000,
            1,
            Duration::from_secs_f64(1_000.0 / 48_000.0),
        ));
        let mut instance = prepare_test_voice(
            VoiceId::from(1),
            Emitter {
                world_id: 1,
                index: 0,
                generation: 1,
            },
            audio,
            EmitterDesc::non_spatial(),
            PlayOptions::looping(),
            Some(PlaybackTag(9)),
            0,
            256,
        )
        .start()
        .1;
        instance.begin_fade_out();

        let mut output = [0.0; 240];
        assert_eq!(instance.fill_mono_buffer(&mut output, 1.0), 240);
        assert!(output[0] > output[120]);
        assert!(output[120] > output[239]);
        let reclaim = instance
            .take_reclaim()
            .expect("completed fade must issue one reclaim");
        assert_eq!(reclaim.emitter, instance.emitter);
        assert_eq!(reclaim.completion_tag, None);
        assert_eq!(instance.take_reclaim(), None);
        assert_eq!(instance.completion_tag, None);
    }
}

#[cfg(test)]
mod envelope_integration_tests {
    use super::*;
    use crate::{SampleEnvelopeControl, test_support::realtime_memory_activity};
    #[test]
    fn resident_voice_uses_envelope_and_reclaims_without_realtime_allocation() {
        let control = SampleEnvelopeControl::default();
        let clip = ResidentClip::from_mono_pcm(vec![0.1; 1024], 48000)
            .unwrap()
            .with_sample_envelope(control.clone());
        let emitter = Emitter {
            world_id: 1,
            index: 0,
            generation: 1,
        };
        let mut voice = AcceptedVoice::capture(
            VoiceId::from(1),
            emitter,
            &clip,
            &EmitterDesc::non_spatial(),
            PlayOptions::looping(),
            None,
            0,
        )
        .prepare(256)
        .start()
        .1;
        let mut pcm = [0.0; 256];
        control.anchor(0.0, 20.0, 1.0);
        let allocations = realtime_memory_activity(|| {
            voice.fill_mono_buffer(&mut pcm, 1.0);
        });
        assert_eq!(allocations, 0);
        assert!(pcm[255] > 0.07 && pcm[255] < 0.073);
        control.anchor(0.0, 0.0, 0.0);
        voice.fill_mono_buffer(&mut pcm, 1.0);
        assert_eq!(pcm[255], 0.0);
        drop(clip);
        assert!(control.has_render_owners());
        voice.begin_fade_out();
        voice.fill_mono_buffer(&mut pcm, 1.0);
        assert!(voice.retired);
        drop(voice);
        assert!(!control.has_render_owners());
    }
}
