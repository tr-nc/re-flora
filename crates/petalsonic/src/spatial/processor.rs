use super::late_reverb::{LateReverbParameters, ThreeBandFdn};
use super::native_ambisonics::{
    DEFAULT_NATIVE_AMBISONICS_ORDER, NativeAmbisonicsBinauralDecoder,
    NativeAmbisonicsBinauralState, NativeAmbisonicsEncoder, native_ambisonics_channel_count,
};
use super::native_hrtf::{
    NativeHrtfRenderMetrics, NativeHrtfRenderer, NativeHrtfSourceState, NativeHrtfTable,
};
use crate::acoustic_propagation::{
    AcousticResponse, DirectLobeTarget, MAX_EARLY_REFLECTION_TAPS, VoiceAcousticResponse,
};
use crate::domain::VoiceId;
use crate::domain::{
    ACOUSTIC_START_MAX_DELAY, DirectGeometry, DirectPlacement, Emitter, EnvironmentOrigin,
    MAX_DIRECT_LOBES, OcclusionProfile, PlayCommandId, SourceExtent,
};
use crate::error::{PetalSonicError, Result};
use crate::events::{
    LateReverbTelemetry, VoiceEnergyTelemetry, VoiceFirstRenderTelemetry, VoiceTelemetryEvent,
};
use crate::gain;
use crate::math::{Pose, Vec3};
use crate::playback::PlaybackInstance;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

const DEFAULT_NATIVE_HRTF_BYTES: &[u8] = include_bytes!("../../asset/hrtf/hrtf_b_nh172.petalhrtf");
const DIRECT_LOW_CROSSOVER_HZ: f32 = 400.0;
const DIRECT_HIGH_CROSSOVER_HZ: f32 = 4_000.0;
const DIRECT_GAIN_SMOOTHING_SECONDS: f32 = 0.05;
const EARLY_REFLECTION_MAX_DELAY_SECONDS: f32 = 0.25;
const EARLY_REFLECTION_SMOOTHING_SECONDS: f32 = 0.05;
const EARLY_REFLECTION_SLOT_RELEASE_GAIN: f32 = 1.0e-3;
// Keep one active set plus one fading set so a priority change can crossfade without allocating.
const EARLY_REFLECTION_SOURCE_STATE_CAPACITY: usize = 16;
const MAX_LOBE_DECORRELATION_DELAY: usize = 17;

#[derive(Debug, Clone)]
struct NativeDirectSourceState {
    low_state: f32,
    low_mid_state: f32,
    current_gain: [f32; 3],
}

#[derive(Clone, Copy)]
struct ThreeBandCoefficients {
    low: f32,
    low_mid: f32,
    smoothing: f32,
}

impl NativeDirectSourceState {
    fn new() -> Self {
        Self {
            low_state: 0.0,
            low_mid_state: 0.0,
            current_gain: [1.0; 3],
        }
    }

    fn new_silent() -> Self {
        Self {
            low_state: 0.0,
            low_mid_state: 0.0,
            current_gain: [0.0; 3],
        }
    }
}

#[derive(Debug, Clone)]
struct LobeDecorrelator {
    delay_samples: usize,
    coefficient: f32,
    input_delay: [f32; MAX_LOBE_DECORRELATION_DELAY],
    output_delay: [f32; MAX_LOBE_DECORRELATION_DELAY],
    index: usize,
}

impl LobeDecorrelator {
    fn new(lobe_id: u8) -> Self {
        let (delay_samples, coefficient) = match lobe_id {
            0 => (0, 0.0),
            1 => (5, 0.37),
            2 => (11, -0.51),
            _ => (17, 0.63),
        };
        Self {
            delay_samples,
            coefficient,
            input_delay: [0.0; MAX_LOBE_DECORRELATION_DELAY],
            output_delay: [0.0; MAX_LOBE_DECORRELATION_DELAY],
            index: 0,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        if self.delay_samples == 0 {
            return input;
        }
        let delayed_input = self.input_delay[self.index];
        let delayed_output = self.output_delay[self.index];
        let output = self.coefficient * input + delayed_input - self.coefficient * delayed_output;
        self.input_delay[self.index] = input;
        self.output_delay[self.index] = output;
        self.index = (self.index + 1) % self.delay_samples;
        output
    }
}

#[derive(Clone, Copy, Debug)]
struct LocalDirectLobeTarget {
    direction: Vec3,
    gain: [f32; 3],
}

#[derive(Debug, Clone)]
struct NativeDirectLobeSlotState {
    current_direction: Vec3,
    filter: NativeDirectSourceState,
    decorrelator: LobeDecorrelator,
}

impl NativeDirectLobeSlotState {
    fn new(lobe_id: u8) -> Self {
        Self {
            current_direction: Vec3::Z,
            filter: NativeDirectSourceState::new_silent(),
            decorrelator: LobeDecorrelator::new(lobe_id),
        }
    }
}

#[derive(Debug, Clone)]
struct NativeDirectLobeSourceState {
    slots: [NativeDirectLobeSlotState; MAX_DIRECT_LOBES],
    held_targets: [Option<LocalDirectLobeTarget>; MAX_DIRECT_LOBES],
    targets_initialized: bool,
}

impl NativeDirectLobeSourceState {
    fn new() -> Self {
        Self {
            slots: std::array::from_fn(|index| NativeDirectLobeSlotState::new(index as u8)),
            held_targets: [None; MAX_DIRECT_LOBES],
            targets_initialized: false,
        }
    }
}

#[derive(Debug, Clone)]
struct NativeEarlyReflectionTapState {
    path_id: Option<u16>,
    arrival_direction: Vec3,
    current_delay_samples: f32,
    current_gain: [f32; 3],
    low_state: f32,
    low_mid_state: f32,
    hrtf_state: Option<NativeHrtfSourceState>,
}

impl NativeEarlyReflectionTapState {
    fn new(hrtf_state: Option<NativeHrtfSourceState>) -> Self {
        Self {
            path_id: None,
            arrival_direction: Vec3::Z,
            current_delay_samples: 1.0,
            current_gain: [0.0; 3],
            low_state: 0.0,
            low_mid_state: 0.0,
            hrtf_state,
        }
    }

    fn is_released(&self) -> bool {
        self.current_gain
            .iter()
            .all(|gain| gain.abs() <= EARLY_REFLECTION_SLOT_RELEASE_GAIN)
    }
}

#[derive(Debug, Clone)]
struct NativeEarlyReflectionSourceState {
    voice_id: Option<VoiceId>,
    silenced: bool,
    draining: bool,
    delay_line: Vec<f32>,
    write_index: usize,
    taps: Vec<NativeEarlyReflectionTapState>,
}

impl NativeEarlyReflectionSourceState {
    fn new(
        voice_id: Option<VoiceId>,
        sample_rate: u32,
        renderer: &NativeHrtfRenderer,
        use_ambisonics: bool,
    ) -> Self {
        let delay_samples =
            (sample_rate as f32 * EARLY_REFLECTION_MAX_DELAY_SECONDS).ceil() as usize + 2;
        let taps = (0..MAX_EARLY_REFLECTION_TAPS)
            .map(|_| {
                NativeEarlyReflectionTapState::new(
                    (!use_ambisonics).then(|| renderer.create_source_state()),
                )
            })
            .collect();
        Self {
            voice_id,
            silenced: true,
            draining: false,
            delay_line: vec![0.0; delay_samples],
            write_index: 0,
            taps,
        }
    }

    fn reset_for_voice(&mut self, voice_id: Option<VoiceId>) {
        self.voice_id = voice_id;
        self.silenced = true;
        self.draining = false;
        self.delay_line.fill(0.0);
        self.write_index = 0;
        for tap in &mut self.taps {
            tap.path_id = None;
            tap.arrival_direction = Vec3::Z;
            tap.current_delay_samples = 1.0;
            tap.current_gain = [0.0; 3];
            tap.low_state = 0.0;
            tap.low_mid_state = 0.0;
            if let Some(hrtf_state) = &mut tap.hrtf_state {
                hrtf_state.reset();
            }
        }
    }

    fn is_released(&self) -> bool {
        self.taps.iter().all(|tap| tap.is_released())
    }
}

/// Backend allocations removed from active rendering and transferred to the
/// non-render supervisor for destruction.
pub(crate) struct RetiredSpatialSource {
    _native_hrtf: Option<NativeHrtfSourceState>,
    _native_direct: Option<NativeDirectSourceState>,
    _native_direct_lobes: Option<NativeDirectLobeSourceState>,
    _native_environment: Option<NativeDirectSourceState>,
}

pub(crate) struct SpatialProcessorConfig {
    pub sample_rate: u32,
    pub frame_size: usize,
    pub max_voices: usize,
    pub distance_scaler: f32,
    pub native_hrtf_path: Option<String>,
    pub hrtf_gain: f32,
    pub use_ambisonics: bool,
    pub environmental_acoustics_enabled: Arc<AtomicBool>,
}

pub(crate) enum AcousticResponseReplacement {
    Accepted(Option<Arc<AcousticResponse>>),
    Rejected(Arc<AcousticResponse>),
}

/// PetalSonic's native HRTF, Ambisonics, and geometry-acoustics renderer.
pub struct SpatialProcessor {
    // Native HRTF/Ambisonics renderer and delay state
    native_hrtf_renderer: NativeHrtfRenderer,
    native_hrtf_source_states: HashMap<VoiceId, NativeHrtfSourceState>,
    native_ambisonics_encoder: NativeAmbisonicsEncoder,
    native_ambisonics_decoder: Option<NativeAmbisonicsBinauralDecoder>,
    native_ambisonics_state: Option<NativeAmbisonicsBinauralState>,
    native_direct_source_states: HashMap<VoiceId, NativeDirectSourceState>,
    native_direct_lobe_source_states: HashMap<VoiceId, NativeDirectLobeSourceState>,
    native_environment_source_states: HashMap<VoiceId, NativeDirectSourceState>,
    native_early_reflection_source_states: HashMap<VoiceId, NativeEarlyReflectionSourceState>,
    free_native_early_reflection_source_states: Vec<NativeEarlyReflectionSourceState>,
    draining_early_reflection_ids: Vec<VoiceId>,
    voice_energy_accumulators: HashMap<VoiceId, VoiceEnergyAccumulator>,
    pending_voice_energy_summaries: Vec<VoiceEnergyAccumulator>,
    acoustic_response: Option<Arc<AcousticResponse>>,

    // Configuration
    frame_size: usize,
    sample_rate: u32,
    distance_scaler: f32,
    use_ambisonics: bool,
    environmental_acoustics_enabled: Arc<AtomicBool>,
    environmental_acoustics_active: bool,
    direct_low_coefficient: f32,
    direct_low_mid_coefficient: f32,
    direct_gain_smoothing_coefficient: f32,
    early_reflection_smoothing_coefficient: f32,
    late_reverb: ThreeBandFdn,
    /// HRTF gain as linear multiplier.
    hrtf_gain_linear: f32,

    // Cached buffers to avoid allocations
    cached_input_buf: Vec<f32>,             // Input mono samples
    cached_direct_buf: Vec<f32>,            // After DirectEffect
    cached_direct_lobe_buf: Vec<f32>,       // One reusable decorrelated direct lobe
    cached_environment_send_buf: Vec<f32>,  // Voice block routed to the shared environment
    cached_summed_encoded_buf: Vec<f32>,    // Accumulated native Ambisonics field
    cached_binaural_processed: Vec<f32>,    // Final binaural output (interleaved stereo)
    cached_early_reflection_bufs: Vec<f32>, // One mono block per bounded reflection tap
    cached_late_reverb_input: Vec<f32>,     // Shared listener-centric mono send

    // Listener state
    listener_position: Vec3,
    listener_up: Vec3,
    listener_front: Vec3,
    listener_right: Vec3,
    direction_field_active: bool,
}

/// Detailed timing metrics captured for a single spatial processing pass.
#[derive(Debug, Default, Clone, Copy)]
pub struct SpatialProcessingMetrics {
    /// Number of spatial sources considered by this processing pass.
    pub spatial_source_count: usize,
    /// Time spent in the acoustic query/simulation stage, when configured.
    pub physics_simulation_time_us: u64,
    /// Time spent applying direct-path processing.
    pub direct_processing_time_us: u64,
    /// Time spent encoding all spatial sources into the ambisonics field.
    pub ambisonics_encoding_time_us: u64,
    /// Time spent decoding ambisonics data back to listener channels.
    pub ambisonics_decoding_time_us: u64,
    /// Time spent rendering HRTF/binaural output.
    pub hrtf_rendering_time_us: u64,
    /// Time spent rendering the shared three-band FDN late-reverb bus.
    pub late_reverb_time_us: u64,
    /// Time spent rendering bounded early-reflection taps.
    pub early_reflection_time_us: u64,
    /// Time spent selecting native HRTF directions.
    pub native_hrtf_direction_lookup_time_us: u64,
    /// Time spent in native HRTF FIR convolution.
    pub native_hrtf_convolution_time_us: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SpatialRenderContext {
    pub render_block_index: u64,
    pub spatial_revision: u64,
}

#[derive(Debug, Default, Clone, Copy)]
struct SourceProcessingMetrics {
    direct_processing_time_us: u64,
    early_reflection_time_us: u64,
    ambisonics_encoding_time_us: u64,
    hrtf_rendering_time_us: u64,
    native_hrtf_direction_lookup_time_us: u64,
    native_hrtf_convolution_time_us: u64,
}

#[derive(Clone, Copy, Debug)]
struct VoiceEnergyAccumulator {
    play_command_id: PlayCommandId,
    emitter: Emitter,
    source_energy: f64,
    direct_energy: f64,
    environment_send_energy: f64,
    early_reflection_energy: f64,
}

impl VoiceEnergyAccumulator {
    fn new(play_command_id: PlayCommandId, emitter: Emitter) -> Self {
        Self {
            play_command_id,
            emitter,
            source_energy: 0.0,
            direct_energy: 0.0,
            environment_send_energy: 0.0,
            early_reflection_energy: 0.0,
        }
    }

    fn telemetry(self, late_reverb: LateReverbTelemetry) -> VoiceEnergyTelemetry {
        VoiceEnergyTelemetry {
            play_command_id: self.play_command_id,
            emitter: self.emitter,
            source_energy: self.source_energy,
            direct_energy: self.direct_energy,
            environment_send_energy: self.environment_send_energy,
            early_reflection_energy: self.early_reflection_energy,
            late_reverb,
        }
    }
}

impl SourceProcessingMetrics {
    fn add_native_hrtf_metrics(&mut self, metrics: NativeHrtfRenderMetrics) {
        self.native_hrtf_direction_lookup_time_us += metrics.direction_lookup_time_us;
        self.native_hrtf_convolution_time_us += metrics.convolution_time_us;
    }
}

impl SpatialProcessor {
    /// Create a new spatial processor from the fixed world backend plan.
    pub fn new(config: SpatialProcessorConfig) -> Result<Self> {
        let SpatialProcessorConfig {
            sample_rate,
            frame_size,
            max_voices,
            distance_scaler,
            native_hrtf_path,
            hrtf_gain,
            use_ambisonics,
            environmental_acoustics_enabled,
        } = config;
        let table = load_native_hrtf_table(sample_rate, native_hrtf_path.as_deref())?;
        let native_hrtf_renderer = NativeHrtfRenderer::with_frame_size(table.clone(), frame_size)?;
        // The shared decoder is also the bounded direction-distribution renderer for extended
        // sources, even when ordinary point Voices use direct native HRTF.
        let decoder = NativeAmbisonicsBinauralDecoder::with_frame_size(
            table.clone(),
            DEFAULT_NATIVE_AMBISONICS_ORDER,
            frame_size,
        )?;
        let native_ambisonics_state = Some(decoder.create_state());
        let native_ambisonics_decoder = Some(decoder);

        let native_ambisonics_encoder =
            NativeAmbisonicsEncoder::new(DEFAULT_NATIVE_AMBISONICS_ORDER)?;

        // Pre-allocate buffers
        let cached_input_buf = vec![0.0; frame_size];
        let cached_direct_buf = vec![0.0; frame_size];
        let cached_direct_lobe_buf = vec![0.0; frame_size];
        let cached_environment_send_buf = vec![0.0; frame_size];
        let ambisonics_channel_count =
            native_ambisonics_channel_count(DEFAULT_NATIVE_AMBISONICS_ORDER)?;
        let cached_summed_encoded_buf = vec![0.0; frame_size * ambisonics_channel_count];
        let cached_binaural_processed = vec![0.0; frame_size * 2];
        let cached_early_reflection_bufs = vec![0.0; frame_size * MAX_EARLY_REFLECTION_TAPS];
        let cached_late_reverb_input = vec![0.0; frame_size];
        let mut late_reverb = ThreeBandFdn::new(sample_rate);
        late_reverb.set_parameters(LateReverbParameters::SILENT);

        // Pre-compute HRTF gain in linear space for efficient application.
        let hrtf_gain_linear = gain::db_to_linear(hrtf_gain);

        log::info!(
            "PetalSonic spatial processor: hrtf_backend=Native, acoustics_backend=NativeAsync, use_ambisonics={}, ambisonics_backend=Native, environmental_acoustics_enabled={}",
            use_ambisonics,
            environmental_acoustics_enabled.load(Ordering::Acquire)
        );

        let environmental_acoustics_active =
            environmental_acoustics_enabled.load(Ordering::Acquire);
        let early_reflection_pool_size = EARLY_REFLECTION_SOURCE_STATE_CAPACITY.min(max_voices);
        let free_native_early_reflection_source_states = (0..early_reflection_pool_size)
            .map(|_| {
                NativeEarlyReflectionSourceState::new(
                    None,
                    sample_rate,
                    &native_hrtf_renderer,
                    use_ambisonics,
                )
            })
            .collect();
        Ok(Self {
            native_hrtf_renderer,
            native_hrtf_source_states: HashMap::with_capacity(max_voices),
            native_ambisonics_encoder,
            native_ambisonics_decoder,
            native_ambisonics_state,
            native_direct_source_states: HashMap::with_capacity(max_voices),
            native_direct_lobe_source_states: HashMap::with_capacity(max_voices),
            native_environment_source_states: HashMap::with_capacity(max_voices),
            native_early_reflection_source_states: HashMap::with_capacity(
                early_reflection_pool_size,
            ),
            free_native_early_reflection_source_states,
            draining_early_reflection_ids: Vec::with_capacity(early_reflection_pool_size),
            voice_energy_accumulators: HashMap::with_capacity(
                max_voices.saturating_add(early_reflection_pool_size),
            ),
            pending_voice_energy_summaries: Vec::with_capacity(
                max_voices.saturating_add(early_reflection_pool_size),
            ),
            acoustic_response: None,
            frame_size,
            sample_rate,
            distance_scaler,
            use_ambisonics,
            environmental_acoustics_enabled,
            environmental_acoustics_active,
            direct_low_coefficient: one_pole_coefficient(DIRECT_LOW_CROSSOVER_HZ, sample_rate),
            direct_low_mid_coefficient: one_pole_coefficient(DIRECT_HIGH_CROSSOVER_HZ, sample_rate),
            direct_gain_smoothing_coefficient: one_pole_coefficient(
                1.0 / (std::f32::consts::TAU * DIRECT_GAIN_SMOOTHING_SECONDS),
                sample_rate,
            ),
            early_reflection_smoothing_coefficient: one_pole_coefficient(
                1.0 / (std::f32::consts::TAU * EARLY_REFLECTION_SMOOTHING_SECONDS),
                sample_rate,
            ),
            late_reverb,
            hrtf_gain_linear,
            cached_input_buf,
            cached_direct_buf,
            cached_direct_lobe_buf,
            cached_environment_send_buf,
            cached_summed_encoded_buf,
            cached_binaural_processed,
            cached_early_reflection_bufs,
            cached_late_reverb_input,
            listener_position: Vec3::ZERO,
            listener_up: Vec3::new(0.0, 1.0, 0.0),
            listener_front: Vec3::new(0.0, 0.0, -1.0),
            listener_right: Vec3::new(1.0, 0.0, 0.0),
            direction_field_active: false,
        })
    }

    /// Update listener pose
    pub fn set_listener_pose(&mut self, pose: Pose) -> Result<()> {
        // Extract position and orientation from pose
        self.listener_position = pose.position;

        // Use the helper methods from Pose
        self.listener_front = pose.forward();
        self.listener_up = pose.up();
        self.listener_right = pose.right();

        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn replace_acoustic_response(
        &mut self,
        response: Arc<AcousticResponse>,
    ) -> AcousticResponseReplacement {
        self.replace_acoustic_response_for_scene(response, 0)
    }

    pub(crate) fn replace_acoustic_response_for_scene(
        &mut self,
        response: Arc<AcousticResponse>,
        required_geometry_version: u64,
    ) -> AcousticResponseReplacement {
        if required_geometry_version != 0 && response.geometry_version != required_geometry_version
        {
            return AcousticResponseReplacement::Rejected(response);
        }
        if self.acoustic_response.as_ref().is_some_and(|current| {
            response.geometry_version < current.geometry_version
                || response.geometry_version == current.geometry_version
                    && response.spatial_revision < current.spatial_revision
        }) {
            return AcousticResponseReplacement::Rejected(response);
        }
        self.late_reverb.set_parameters(response.late_reverb);
        AcousticResponseReplacement::Accepted(self.acoustic_response.replace(response))
    }

    #[cfg(test)]
    pub(crate) fn acoustic_response_generation(&self) -> Option<(u64, u64)> {
        self.acoustic_response
            .as_ref()
            .map(|response| (response.spatial_revision, response.geometry_version))
    }

    pub(crate) fn retire_voice(&mut self, voice_id: VoiceId) -> Option<RetiredSpatialSource> {
        let native_hrtf = self.native_hrtf_source_states.remove(&voice_id);
        let native_direct = self.native_direct_source_states.remove(&voice_id);
        let native_direct_lobes = self.native_direct_lobe_source_states.remove(&voice_id);
        let native_environment = self.native_environment_source_states.remove(&voice_id);
        let release_early_state = self
            .native_early_reflection_source_states
            .get(&voice_id)
            .is_some_and(NativeEarlyReflectionSourceState::is_released);
        let mut early_tail_draining = false;
        if release_early_state {
            if let Some(mut state) = self.native_early_reflection_source_states.remove(&voice_id) {
                state.reset_for_voice(None);
                self.free_native_early_reflection_source_states.push(state);
            }
        } else if let Some(state) = self
            .native_early_reflection_source_states
            .get_mut(&voice_id)
        {
            state.draining = true;
            early_tail_draining = true;
        }
        if !early_tail_draining {
            self.finish_voice_energy(voice_id);
        }
        (native_hrtf.is_some()
            || native_direct.is_some()
            || native_direct_lobes.is_some()
            || native_environment.is_some())
        .then_some(RetiredSpatialSource {
            _native_hrtf: native_hrtf,
            _native_direct: native_direct,
            _native_direct_lobes: native_direct_lobes,
            _native_environment: native_environment,
        })
    }

    fn ensure_native_hrtf_state_for_voice(&mut self, voice_id: VoiceId) -> Result<()> {
        if self.use_ambisonics || self.native_hrtf_source_states.contains_key(&voice_id) {
            return Ok(());
        }

        self.native_hrtf_source_states
            .insert(voice_id, self.native_hrtf_renderer.create_source_state());
        Ok(())
    }

    fn ensure_native_direct_state_for_voice(&mut self, voice_id: VoiceId) {
        self.native_direct_source_states
            .entry(voice_id)
            .or_insert_with(NativeDirectSourceState::new);
    }

    fn ensure_native_direct_lobe_state_for_voice(&mut self, voice_id: VoiceId) {
        self.native_direct_lobe_source_states
            .entry(voice_id)
            .or_insert_with(NativeDirectLobeSourceState::new);
    }

    fn ensure_native_environment_state_for_voice(&mut self, voice_id: VoiceId) {
        self.native_environment_source_states
            .entry(voice_id)
            .or_insert_with(NativeDirectSourceState::new);
    }

    fn ensure_native_early_reflection_state_for_voice(
        &mut self,
        voice_id: VoiceId,
        voice_acoustics: Option<VoiceAcousticResponse<'_>>,
    ) {
        if self
            .native_early_reflection_source_states
            .contains_key(&voice_id)
            || !self.environmental_acoustics_active
            || voice_acoustics.is_none_or(|response| response.early_reflections().is_empty())
        {
            return;
        }
        if let Some(mut state) = self.free_native_early_reflection_source_states.pop() {
            state.reset_for_voice(Some(voice_id));
            self.native_early_reflection_source_states
                .insert(voice_id, state);
            return;
        }

        let reusable_voice_id = self.native_early_reflection_source_states.iter().find_map(
            |(candidate_voice_id, state)| state.is_released().then_some(*candidate_voice_id),
        );
        if let Some(reusable_voice_id) = reusable_voice_id
            && let Some(mut state) = self
                .native_early_reflection_source_states
                .remove(&reusable_voice_id)
        {
            state.reset_for_voice(Some(voice_id));
            self.native_early_reflection_source_states
                .insert(voice_id, state);
        }
    }

    pub(crate) fn silence_voice_state(&mut self, voice_id: VoiceId) {
        if let Some(state) = self
            .native_early_reflection_source_states
            .get_mut(&voice_id)
            && !state.silenced
        {
            state.reset_for_voice(state.voice_id);
        }
    }

    /// Process all spatial sources and return bounded timing metrics.
    pub fn process_spatial_sources_with_metrics(
        &mut self,
        spatial_voice_ids: &[VoiceId],
        instances: &mut HashMap<VoiceId, PlaybackInstance>,
        output_buffer: &mut [f32],
        render_context: SpatialRenderContext,
        events: &mut Vec<VoiceTelemetryEvent>,
    ) -> Result<SpatialProcessingMetrics> {
        self.capture_environmental_acoustics_state();

        let mut metrics = SpatialProcessingMetrics {
            spatial_source_count: spatial_voice_ids.len(),
            ..SpatialProcessingMetrics::default()
        };

        // Clear accumulation buffers before entering the per-Voice quantum path.
        self.cached_summed_encoded_buf.fill(0.0);
        self.cached_binaural_processed.fill(0.0);
        self.cached_late_reverb_input.fill(0.0);
        self.direction_field_active = false;

        // Resolve one immutable acoustic envelope at each active Voice's quantum entry. The same
        // envelope drives backend preparation and every direct/environment/reflection consumer.
        let acoustic_response = self.acoustic_response.clone();
        for voice_id in spatial_voice_ids {
            let Some(instance) = instances.get_mut(voice_id) else {
                continue;
            };
            if !instance.render_state.is_spatial() {
                continue;
            }
            let voice_acoustics = acoustic_response
                .as_deref()
                .and_then(|response| response.voice_response(*voice_id));

            if let Some(play_command_id) = instance.telemetry_command_id() {
                self.voice_energy_accumulators
                    .entry(*voice_id)
                    .or_insert_with(|| {
                        VoiceEnergyAccumulator::new(play_command_id, instance.emitter)
                    });
            }

            if !matches!(instance.direct_path.placement(), DirectPlacement::Disabled) {
                match instance.source_extent {
                    SourceExtent::Point => {
                        self.ensure_native_hrtf_state_for_voice(*voice_id)?;
                        self.ensure_native_direct_state_for_voice(*voice_id);
                    }
                    SourceExtent::WeightedSamples(_) => {
                        self.ensure_native_direct_lobe_state_for_voice(*voice_id);
                    }
                }
            }
            if !matches!(
                instance.environment_send.origin(),
                EnvironmentOrigin::Disabled
            ) {
                self.ensure_native_environment_state_for_voice(*voice_id);
                self.ensure_native_early_reflection_state_for_voice(*voice_id, voice_acoustics);
            }

            let source_metrics = self.process_single_source(
                *voice_id,
                voice_acoustics,
                instance,
                render_context,
                events,
            )?;
            metrics.direct_processing_time_us += source_metrics.direct_processing_time_us;
            metrics.early_reflection_time_us += source_metrics.early_reflection_time_us;
            metrics.ambisonics_encoding_time_us += source_metrics.ambisonics_encoding_time_us;
            metrics.hrtf_rendering_time_us += source_metrics.hrtf_rendering_time_us;
            metrics.native_hrtf_direction_lookup_time_us +=
                source_metrics.native_hrtf_direction_lookup_time_us;
            metrics.native_hrtf_convolution_time_us +=
                source_metrics.native_hrtf_convolution_time_us;
        }

        let drain_started = Instant::now();
        let draining_metrics = self.process_draining_early_reflections()?;
        metrics.early_reflection_time_us += drain_started.elapsed().as_micros() as u64;
        metrics.hrtf_rendering_time_us += draining_metrics.convolution_time_us;
        metrics.native_hrtf_direction_lookup_time_us += draining_metrics.direction_lookup_time_us;
        metrics.native_hrtf_convolution_time_us += draining_metrics.convolution_time_us;

        if self.use_ambisonics || self.direction_field_active {
            let decoding_start = Instant::now();
            let native_metrics = self.apply_native_ambisonics_decode_effect()?;
            metrics.native_hrtf_direction_lookup_time_us += native_metrics.direction_lookup_time_us;
            metrics.native_hrtf_convolution_time_us += native_metrics.convolution_time_us;
            let decode_elapsed = decoding_start.elapsed().as_micros() as u64;
            metrics.ambisonics_decoding_time_us = decode_elapsed;
            metrics.hrtf_rendering_time_us += decode_elapsed;

            if self.hrtf_gain_linear != 1.0 {
                self.apply_hrtf_gain();
            }
        } else if self.hrtf_gain_linear != 1.0 {
            self.apply_hrtf_gain();
        }

        let late_reverb_start = Instant::now();
        self.late_reverb.process_block(
            &self.cached_late_reverb_input,
            &mut self.cached_binaural_processed,
            self.environmental_acoustics_active,
        );
        metrics.late_reverb_time_us = late_reverb_start.elapsed().as_micros() as u64;
        self.emit_pending_voice_energy(events);

        // Add to output buffer (don't overwrite - allow mixing with non-spatial sources)
        let frames_to_copy = (output_buffer.len() / 2).min(self.frame_size);
        for i in 0..frames_to_copy {
            output_buffer[i * 2] += self.cached_binaural_processed[i * 2];
            output_buffer[i * 2 + 1] += self.cached_binaural_processed[i * 2 + 1];
        }

        Ok(metrics)
    }

    pub(crate) fn has_environment_tail(&self) -> bool {
        self.late_reverb.needs_processing()
            || self
                .native_early_reflection_source_states
                .values()
                .any(|state| state.draining)
    }

    pub(crate) fn has_pending_voice_telemetry(&self) -> bool {
        !self.pending_voice_energy_summaries.is_empty()
    }

    fn process_draining_early_reflections(&mut self) -> Result<NativeHrtfRenderMetrics> {
        self.draining_early_reflection_ids.clear();
        self.draining_early_reflection_ids.extend(
            self.native_early_reflection_source_states
                .iter()
                .filter_map(|(voice_id, state)| state.draining.then_some(*voice_id)),
        );
        self.cached_input_buf.fill(0.0);
        let mut metrics = NativeHrtfRenderMetrics::default();
        let acoustic_response = self.acoustic_response.clone();
        for index in 0..self.draining_early_reflection_ids.len() {
            let voice_id = self.draining_early_reflection_ids[index];
            let voice_acoustics = acoustic_response
                .as_deref()
                .and_then(|response| response.voice_response(voice_id));
            let source_metrics =
                self.apply_native_early_reflections(voice_id, voice_acoustics, 0.0, false)?;
            metrics.direction_lookup_time_us += source_metrics.direction_lookup_time_us;
            metrics.convolution_time_us += source_metrics.convolution_time_us;
        }
        for index in 0..self.draining_early_reflection_ids.len() {
            let voice_id = self.draining_early_reflection_ids[index];
            let released = self
                .native_early_reflection_source_states
                .get(&voice_id)
                .is_some_and(NativeEarlyReflectionSourceState::is_released);
            if released
                && let Some(mut state) =
                    self.native_early_reflection_source_states.remove(&voice_id)
            {
                state.reset_for_voice(None);
                self.free_native_early_reflection_source_states.push(state);
                self.finish_voice_energy(voice_id);
            }
        }
        Ok(metrics)
    }

    fn finish_voice_energy(&mut self, voice_id: VoiceId) {
        let Some(energy) = self.voice_energy_accumulators.remove(&voice_id) else {
            return;
        };
        debug_assert!(
            self.pending_voice_energy_summaries.len()
                < self.pending_voice_energy_summaries.capacity()
        );
        if self.pending_voice_energy_summaries.len()
            < self.pending_voice_energy_summaries.capacity()
        {
            self.pending_voice_energy_summaries.push(energy);
        }
    }

    fn emit_pending_voice_energy(&mut self, events: &mut Vec<VoiceTelemetryEvent>) {
        let (parameters, cumulative_input_energy, cumulative_output_energy) =
            self.late_reverb.telemetry();
        let late_reverb = LateReverbTelemetry {
            pre_delay_seconds: parameters.pre_delay_seconds,
            rt60_seconds: parameters.rt60_seconds,
            wet_gain: parameters.wet_gain,
            cumulative_input_energy,
            cumulative_output_energy,
        };
        events.extend(
            self.pending_voice_energy_summaries
                .drain(..)
                .map(|energy| VoiceTelemetryEvent::EnergySummary(energy.telemetry(late_reverb))),
        );
    }

    // Called before decoding the first PCM block. A missing/deferred solve is not
    // permission to render unoccluded, nor to silently consume a finite attack.
    fn prepare_acoustic_start(
        &mut self,
        voice_id: VoiceId,
        response: Option<VoiceAcousticResponse<'_>>,
        instance: &mut PlaybackInstance,
        events: &mut Vec<VoiceTelemetryEvent>,
    ) -> bool {
        let Some(waited) = instance.acoustic_start_wait_frames else {
            return true;
        };
        if !matches!(
            instance.info.play_state,
            crate::playback::PlayState::Playing
        ) {
            return false;
        }
        if instance.fading_out() {
            instance.cancel_acoustic_start();
            return false;
        }
        let need_direct = self.environmental_acoustics_active
            && !matches!(instance.direct_path.placement(), DirectPlacement::Disabled)
            && matches!(
                instance.direct_path.geometry(),
                DirectGeometry::SimulatedTransmission
            );
        let need_environment = self.environmental_acoustics_active
            && !matches!(
                instance.environment_send.origin(),
                EnvironmentOrigin::Disabled
            );
        let distributed = matches!(instance.source_extent, SourceExtent::WeightedSamples(_));
        let direct_ready = !need_direct
            || if distributed {
                response
                    .and_then(VoiceAcousticResponse::direct_lobes)
                    .is_some()
            } else {
                response
                    .and_then(VoiceAcousticResponse::direct_gain_target)
                    .is_some()
            };
        let environment_ready = !need_environment
            || response
                .and_then(VoiceAcousticResponse::environment_gain_target)
                .is_some();
        if !direct_ready || !environment_ready {
            let next = waited.saturating_add(self.frame_size);
            let limit =
                (ACOUSTIC_START_MAX_DELAY.as_secs_f64() * self.sample_rate as f64).ceil() as usize;
            if next >= limit {
                instance.cancel_acoustic_start();
                if let Some(play_command_id) = instance.telemetry_command_id() {
                    events.push(VoiceTelemetryEvent::AcousticStartTimedOut {
                        play_command_id,
                        emitter: instance.emitter,
                    });
                }
            } else {
                instance.acoustic_start_wait_frames = Some(next);
            }
            return false;
        }
        // Initialize to the FIRST valid target, not unity followed by a 50-ms
        // down-ramp (point) or zero followed by a swallowed attack (lobes).
        if distributed {
            let targets = if need_direct {
                self.listener_local_lobe_targets(
                    response
                        .and_then(VoiceAcousticResponse::direct_lobes)
                        .unwrap(),
                )
            } else {
                self.unoccluded_lobe_targets(instance)
            };
            if let Some(state) = self.native_direct_lobe_source_states.get_mut(&voice_id) {
                state.held_targets = targets;
                state.targets_initialized = true;
                for (slot, target) in state.slots.iter_mut().zip(targets) {
                    slot.filter.current_gain = target.map_or([0.; 3], |t| t.gain);
                    if let Some(target) = target {
                        slot.current_direction = target.direction;
                    }
                }
            }
        } else if let Some(state) = self.native_direct_source_states.get_mut(&voice_id) {
            state.current_gain = if need_direct {
                response
                    .and_then(VoiceAcousticResponse::direct_gain_target)
                    .unwrap()
            } else {
                [1.; 3]
            };
        }
        if let Some(state) = self.native_environment_source_states.get_mut(&voice_id) {
            state.current_gain = if need_environment {
                response
                    .and_then(VoiceAcousticResponse::environment_gain_target)
                    .unwrap()
            } else {
                [1.; 3]
            };
        }
        instance.acoustic_start_wait_frames = None;
        true
    }

    /// Process a single spatial source
    fn process_single_source(
        &mut self,
        voice_id: VoiceId,
        voice_acoustics: Option<VoiceAcousticResponse<'_>>,
        instance: &mut PlaybackInstance,
        render_context: SpatialRenderContext,
        events: &mut Vec<VoiceTelemetryEvent>,
    ) -> Result<SourceProcessingMetrics> {
        // Get spatial configuration (position + per-source volume)
        let emitter_position = match instance.render_state.spatial_pose() {
            Some(pose) => pose.position,
            None => return Ok(SourceProcessingMetrics::default()), // Not a spatial source, skip
        };

        if !self.prepare_acoustic_start(voice_id, voice_acoustics, instance, events) {
            return Ok(SourceProcessingMetrics::default());
        }

        // Convert dB volume from config to linear gain once per block.
        let volume = instance.render_state.volume_linear();

        // Fill input buffer with audio samples
        let frames_filled = self.fill_input_buffer(instance, volume);
        if let Some(energy) = self.voice_energy_accumulators.get_mut(&voice_id) {
            energy.source_energy += block_energy(&self.cached_input_buf);
        }

        let mut metrics = SourceProcessingMetrics::default();

        // Direct and environment paths derive from this one cursor block. Neither path asks the
        // Voice to decode or advance a second time.
        let direct_start = Instant::now();
        let direct_local_position =
            self.resolve_direct_local_position(instance.direct_path.placement(), emitter_position);
        let distributed_direct = matches!(instance.source_extent, SourceExtent::WeightedSamples(_));
        self.cached_direct_buf.fill(0.0);
        let mut direct_energy = 0.0;
        if let Some(direct_local_position) = direct_local_position {
            if distributed_direct {
                direct_energy = self.apply_native_distributed_direct_effect(
                    voice_id,
                    voice_acoustics,
                    instance,
                    direct_local_position,
                    instance.direct_path.geometry(),
                )?;
            } else {
                self.apply_native_direct_effect(
                    voice_id,
                    voice_acoustics,
                    direct_local_position,
                    instance.direct_path.geometry(),
                )?;
                direct_energy = block_energy(&self.cached_direct_buf);
            }
        }
        if let Some(energy) = self.voice_energy_accumulators.get_mut(&voice_id) {
            energy.direct_energy += direct_energy;
        }

        let environment_local_position = match instance.environment_send.origin() {
            EnvironmentOrigin::FollowEmitter => {
                Some(self.world_to_listener_position(emitter_position))
            }
            EnvironmentOrigin::World(origin) => {
                Some(self.world_to_listener_position(origin.position))
            }
            EnvironmentOrigin::Disabled => None,
        };
        let environment_send_gain = gain::db_to_linear(instance.environment_send.gain_db());
        self.cached_environment_send_buf.fill(0.0);
        if let Some(environment_local_position) = environment_local_position {
            let compatible_shared_path = !distributed_direct
                && matches!(instance.direct_path.placement(), DirectPlacement::World)
                && matches!(
                    instance.direct_path.geometry(),
                    DirectGeometry::SimulatedTransmission
                )
                && matches!(
                    instance.environment_send.origin(),
                    EnvironmentOrigin::FollowEmitter
                );
            if compatible_shared_path {
                for (send, direct) in self
                    .cached_environment_send_buf
                    .iter_mut()
                    .zip(&self.cached_direct_buf)
                {
                    *send = *direct * environment_send_gain;
                }
            } else {
                self.apply_native_environment_send_effect(
                    voice_id,
                    voice_acoustics,
                    environment_local_position,
                    environment_send_gain,
                )?;
            }
            for (sum, send) in self
                .cached_late_reverb_input
                .iter_mut()
                .zip(&self.cached_environment_send_buf)
            {
                *sum += *send;
            }
        }
        if let Some(energy) = self.voice_energy_accumulators.get_mut(&voice_id) {
            energy.environment_send_energy += block_energy(&self.cached_environment_send_buf);
        }
        if frames_filled > 0 {
            self.observe_voice_render(
                voice_acoustics,
                instance,
                emitter_position,
                direct_local_position,
                render_context,
                events,
            );
        }
        metrics.direct_processing_time_us = direct_start.elapsed().as_micros() as u64;

        if let Some(direct_local_position) = direct_local_position
            && !distributed_direct
        {
            if self.use_ambisonics {
                let encoding_start = Instant::now();
                self.apply_native_ambisonics_encode_effect(direct_local_position)?;
                metrics.ambisonics_encoding_time_us = encoding_start.elapsed().as_micros() as u64;
            } else {
                let render_start = Instant::now();
                let native_metrics =
                    self.apply_native_hrtf_effect(voice_id, direct_local_position)?;
                metrics.add_native_hrtf_metrics(native_metrics);
                metrics.hrtf_rendering_time_us = render_start.elapsed().as_micros() as u64;
            }
        }

        let reflection_start = Instant::now();
        let native_metrics = self.apply_native_early_reflections(
            voice_id,
            voice_acoustics,
            environment_send_gain,
            environment_local_position.is_some(),
        )?;
        metrics.add_native_hrtf_metrics(native_metrics);
        metrics.early_reflection_time_us = reflection_start.elapsed().as_micros() as u64;
        metrics.hrtf_rendering_time_us += native_metrics.convolution_time_us;

        Ok(metrics)
    }

    /// Fill input buffer from playback instance
    fn fill_input_buffer(&mut self, instance: &mut PlaybackInstance, volume: f32) -> usize {
        self.cached_input_buf.fill(0.0);
        instance.fill_mono_buffer(&mut self.cached_input_buf[..self.frame_size], volume)
    }

    fn observe_voice_render(
        &self,
        voice_acoustics: Option<VoiceAcousticResponse<'_>>,
        instance: &mut PlaybackInstance,
        emitter_position: Vec3,
        direct_local_position: Option<Vec3>,
        render_context: SpatialRenderContext,
        events: &mut Vec<VoiceTelemetryEvent>,
    ) {
        let environment_response = voice_acoustics.map(VoiceAcousticResponse::telemetry);
        if let Some(play_command_id) = instance.take_first_render_command_id() {
            let direct_local_pose = match instance.direct_path.placement() {
                DirectPlacement::ListenerRelative(local_pose) => Some(local_pose),
                DirectPlacement::World | DirectPlacement::ListenerPositionRelative(_) => {
                    direct_local_position.map(Pose::from_position)
                }
                DirectPlacement::Disabled => None,
            };
            let acoustic_origin = match instance.environment_send.origin() {
                EnvironmentOrigin::FollowEmitter => Some(Pose::from_position(emitter_position)),
                EnvironmentOrigin::World(origin) => Some(origin),
                EnvironmentOrigin::Disabled => None,
            };
            events.push(VoiceTelemetryEvent::FirstRendered(
                VoiceFirstRenderTelemetry {
                    play_command_id,
                    emitter: instance.emitter,
                    render_block_index: render_context.render_block_index,
                    spatial_revision: render_context.spatial_revision,
                    direct_local_pose,
                    acoustic_origin,
                    environment_response,
                },
            ));
            if environment_response.is_some() {
                instance.mark_environment_response_reported();
            }
        }
        if let (Some(play_command_id), Some(response)) = (
            instance.pending_environment_response_id(),
            environment_response,
        ) {
            events.push(VoiceTelemetryEvent::EnvironmentResponse {
                play_command_id,
                response,
            });
            instance.mark_environment_response_reported();
        }
    }

    /// Apply PetalSonic's native direct path to the input buffer.
    fn apply_native_distributed_direct_effect(
        &mut self,
        voice_id: VoiceId,
        voice_acoustics: Option<VoiceAcousticResponse<'_>>,
        instance: &PlaybackInstance,
        direct_local_position: Vec3,
        geometry: DirectGeometry,
    ) -> Result<f64> {
        let use_solved_targets = self.environmental_acoustics_active
            && matches!(geometry, DirectGeometry::SimulatedTransmission);
        let solved_targets = use_solved_targets
            .then(|| voice_acoustics.and_then(VoiceAcousticResponse::direct_lobes))
            .flatten();
        let has_held_targets = self
            .native_direct_lobe_source_states
            .get(&voice_id)
            .is_some_and(|state| state.targets_initialized);
        let next_targets = if let Some(targets) = solved_targets {
            Some(self.listener_local_lobe_targets(targets))
        } else if use_solved_targets && has_held_targets {
            None
        } else {
            Some(self.unoccluded_lobe_targets(instance))
        };

        let distance_meters = direct_local_position.length() * self.distance_scaler;
        let distance_gain =
            native_distance_attenuation(distance_meters) * native_air_absorption(distance_meters);
        let direction_alpha =
            1.0 - (-(self.frame_size as f32 / self.sample_rate.max(1) as f32) / 0.1).exp();
        let coefficients = ThreeBandCoefficients {
            low: self.direct_low_coefficient,
            low_mid: self.direct_low_mid_coefficient,
            smoothing: self.direct_gain_smoothing_coefficient,
        };
        let state = self
            .native_direct_lobe_source_states
            .get_mut(&voice_id)
            .ok_or_else(|| {
                PetalSonicError::SpatialAudio(format!(
                    "No native direct-lobe state found for source {voice_id}"
                ))
            })?;
        if let Some(next_targets) = next_targets {
            state.held_targets = next_targets;
            state.targets_initialized = true;
        }

        let mut field_active = false;
        let mut direct_energy = 0.0;
        for (slot_index, slot) in state.slots.iter_mut().enumerate() {
            let target = state.held_targets[slot_index];
            let target_gain = target.map_or([0.0; 3], |target| target.gain);
            if let Some(target) = target {
                slot.current_direction = normalized_direction(
                    slot.current_direction
                        .lerp(target.direction, direction_alpha),
                );
            }
            apply_three_band_gain(
                &self.cached_input_buf,
                &mut self.cached_direct_lobe_buf,
                &mut slot.filter,
                coefficients,
                target_gain,
                distance_gain,
            );
            for sample in &mut self.cached_direct_lobe_buf {
                *sample = slot.decorrelator.process(*sample);
            }
            direct_energy += block_energy(&self.cached_direct_lobe_buf);
            if target.is_some()
                || slot
                    .filter
                    .current_gain
                    .iter()
                    .any(|gain| gain.abs() > 1.0e-5)
            {
                self.native_ambisonics_encoder.encode_source_accumulate(
                    slot.current_direction,
                    &self.cached_direct_lobe_buf,
                    &mut self.cached_summed_encoded_buf,
                )?;
                field_active = true;
            }
        }
        self.direction_field_active |= field_active;
        Ok(direct_energy)
    }

    fn listener_local_lobe_targets(
        &self,
        targets: &[DirectLobeTarget],
    ) -> [Option<LocalDirectLobeTarget>; MAX_DIRECT_LOBES] {
        let mut local = [None; MAX_DIRECT_LOBES];
        for target in targets {
            let index = usize::from(target.lobe_id);
            if index >= MAX_DIRECT_LOBES {
                continue;
            }
            local[index] = Some(LocalDirectLobeTarget {
                direction: listener_local_direction(
                    target.direction,
                    self.listener_right,
                    self.listener_up,
                    self.listener_front,
                ),
                gain: target.gain,
            });
        }
        local
    }

    fn unoccluded_lobe_targets(
        &self,
        instance: &PlaybackInstance,
    ) -> [Option<LocalDirectLobeTarget>; MAX_DIRECT_LOBES] {
        let mut power = [0.0_f32; MAX_DIRECT_LOBES];
        let mut direction_sum = [Vec3::ZERO; MAX_DIRECT_LOBES];
        let mut fallback = [None; MAX_DIRECT_LOBES];
        let SourceExtent::WeightedSamples(weighted) = &instance.source_extent else {
            return [None; MAX_DIRECT_LOBES];
        };
        let lobe_count = render_lobe_count(instance.occlusion_profile);
        for sample in weighted.samples() {
            let lobe = sample.id().0 as usize % lobe_count;
            let local_position = match instance.direct_path.placement() {
                DirectPlacement::World => instance
                    .render_state
                    .spatial_pose()
                    .map(|pose| {
                        self.world_to_listener_position(
                            pose.position + pose.rotation * sample.local_position(),
                        )
                    })
                    .unwrap_or(Vec3::Z),
                DirectPlacement::ListenerRelative(pose) => {
                    pose.position + pose.rotation * sample.local_position()
                }
                DirectPlacement::ListenerPositionRelative(pose) => self
                    .world_vector_to_listener_position(
                        pose.position + pose.rotation * sample.local_position(),
                    ),
                DirectPlacement::Disabled => continue,
            };
            let direction = normalized_direction(local_position);
            power[lobe] += sample.power_weight();
            direction_sum[lobe] += direction * sample.power_weight();
            fallback[lobe].get_or_insert(direction);
        }
        std::array::from_fn(|lobe| {
            fallback[lobe].map(|fallback_direction| LocalDirectLobeTarget {
                direction: if direction_sum[lobe].length_squared() > f32::EPSILON {
                    direction_sum[lobe].normalize()
                } else {
                    fallback_direction
                },
                gain: [power[lobe].sqrt(); 3],
            })
        })
    }

    fn apply_native_direct_effect(
        &mut self,
        voice_id: VoiceId,
        voice_acoustics: Option<VoiceAcousticResponse<'_>>,
        direct_local_position: Vec3,
        geometry: DirectGeometry,
    ) -> Result<()> {
        let distance_world = direct_local_position.length();
        let distance_meters = distance_world * self.distance_scaler;
        let distance_attenuation = native_distance_attenuation(distance_meters);
        let air_absorption = native_air_absorption(distance_meters);
        let distance_gain = distance_attenuation * air_absorption;
        let state = self
            .native_direct_source_states
            .get_mut(&voice_id)
            .ok_or_else(|| {
                PetalSonicError::SpatialAudio(format!(
                    "No native direct state found for source {}",
                    voice_id
                ))
            })?;
        let target_gain = if self.environmental_acoustics_active
            && matches!(geometry, DirectGeometry::SimulatedTransmission)
        {
            voice_acoustics
                .and_then(VoiceAcousticResponse::direct_gain_target)
                .unwrap_or(state.current_gain)
        } else {
            [1.0; 3]
        };

        apply_three_band_gain(
            &self.cached_input_buf,
            &mut self.cached_direct_buf,
            state,
            ThreeBandCoefficients {
                low: self.direct_low_coefficient,
                low_mid: self.direct_low_mid_coefficient,
                smoothing: self.direct_gain_smoothing_coefficient,
            },
            target_gain,
            distance_gain,
        );
        Ok(())
    }

    fn apply_native_environment_send_effect(
        &mut self,
        voice_id: VoiceId,
        voice_acoustics: Option<VoiceAcousticResponse<'_>>,
        environment_local_position: Vec3,
        send_gain: f32,
    ) -> Result<()> {
        let distance_meters = environment_local_position.length() * self.distance_scaler;
        let distance_gain = native_distance_attenuation(distance_meters)
            * native_air_absorption(distance_meters)
            * send_gain;
        let state = self
            .native_environment_source_states
            .get_mut(&voice_id)
            .ok_or_else(|| {
                PetalSonicError::SpatialAudio(format!(
                    "No native environment-send state found for source {}",
                    voice_id
                ))
            })?;
        let target_gain = if self.environmental_acoustics_active {
            voice_acoustics
                .and_then(VoiceAcousticResponse::environment_gain_target)
                .unwrap_or(state.current_gain)
        } else {
            [1.0; 3]
        };
        apply_three_band_gain(
            &self.cached_input_buf,
            &mut self.cached_environment_send_buf,
            state,
            ThreeBandCoefficients {
                low: self.direct_low_coefficient,
                low_mid: self.direct_low_mid_coefficient,
                smoothing: self.direct_gain_smoothing_coefficient,
            },
            target_gain,
            distance_gain,
        );
        Ok(())
    }

    fn world_to_listener_position(&self, world_position: Vec3) -> Vec3 {
        self.world_vector_to_listener_position(world_position - self.listener_position)
    }

    fn world_vector_to_listener_position(&self, world_vector: Vec3) -> Vec3 {
        Vec3::new(
            world_vector.dot(self.listener_right),
            world_vector.dot(self.listener_up),
            world_vector.dot(self.listener_front),
        )
    }

    fn resolve_direct_local_position(
        &self,
        placement: DirectPlacement,
        emitter_world_position: Vec3,
    ) -> Option<Vec3> {
        match placement {
            DirectPlacement::World => Some(self.world_to_listener_position(emitter_world_position)),
            DirectPlacement::ListenerRelative(local_pose) => Some(local_pose.position),
            DirectPlacement::ListenerPositionRelative(world_offset_pose) => {
                Some(self.world_vector_to_listener_position(world_offset_pose.position))
            }
            DirectPlacement::Disabled => None,
        }
    }

    fn apply_native_early_reflections(
        &mut self,
        voice_id: VoiceId,
        voice_acoustics: Option<VoiceAcousticResponse<'_>>,
        send_gain: f32,
        send_enabled: bool,
    ) -> Result<NativeHrtfRenderMetrics> {
        let mut targets = [None; MAX_EARLY_REFLECTION_TAPS];
        if send_enabled
            && self.environmental_acoustics_active
            && let Some(response) = voice_acoustics
        {
            for (target, tap) in targets.iter_mut().zip(response.early_reflections()) {
                *target = Some(*tap);
            }
        }
        let Some(state) = self
            .native_early_reflection_source_states
            .get_mut(&voice_id)
        else {
            return Ok(NativeHrtfRenderMetrics::default());
        };
        state.silenced = false;

        let mut slot_targets = [None; MAX_EARLY_REFLECTION_TAPS];
        let mut target_claimed = [false; MAX_EARLY_REFLECTION_TAPS];
        for (slot_index, slot) in state.taps.iter().enumerate() {
            let Some(path_id) = slot.path_id else {
                continue;
            };
            if let Some((target_index, target)) =
                targets.iter().enumerate().find_map(|(index, target)| {
                    target
                        .filter(|target| target.path_id == path_id && !target_claimed[index])
                        .map(|target| (index, target))
                })
            {
                slot_targets[slot_index] = Some(target);
                target_claimed[target_index] = true;
            }
        }
        for (slot_index, slot) in state.taps.iter_mut().enumerate() {
            if slot_targets[slot_index].is_some() || !slot.is_released() {
                continue;
            }
            slot.path_id = None;
            if let Some((target_index, target)) =
                targets.iter().enumerate().find_map(|(index, target)| {
                    target
                        .filter(|_| !target_claimed[index])
                        .map(|target| (index, target))
                })
            {
                slot.path_id = Some(target.path_id);
                slot.arrival_direction = target.arrival_direction;
                slot.current_delay_samples = reflection_delay_samples(
                    target.delay_seconds,
                    self.sample_rate,
                    state.delay_line.len(),
                );
                slot_targets[slot_index] = Some(target);
                target_claimed[target_index] = true;
            }
        }

        self.cached_early_reflection_bufs.fill(0.0);
        let mut write_index = state.write_index;
        for (sample_index, input) in self.cached_input_buf.iter().copied().enumerate() {
            state.delay_line[write_index] = input * send_gain;
            for (slot_index, slot) in state.taps.iter_mut().enumerate() {
                let target = slot_targets[slot_index];
                let target_delay = target
                    .map(|target| {
                        reflection_delay_samples(
                            target.delay_seconds,
                            self.sample_rate,
                            state.delay_line.len(),
                        )
                    })
                    .unwrap_or(slot.current_delay_samples);
                slot.current_delay_samples += self.early_reflection_smoothing_coefficient
                    * (target_delay - slot.current_delay_samples);
                let target_gain = target.map(|target| target.gain).unwrap_or([0.0; 3]);
                for (current, target) in slot.current_gain.iter_mut().zip(target_gain) {
                    *current += self.early_reflection_smoothing_coefficient * (target - *current);
                }

                let delayed = read_fractional_delay(
                    &state.delay_line,
                    write_index,
                    slot.current_delay_samples,
                );
                slot.low_state += self.direct_low_coefficient * (delayed - slot.low_state);
                slot.low_mid_state +=
                    self.direct_low_mid_coefficient * (delayed - slot.low_mid_state);
                let bands = [
                    slot.low_state,
                    slot.low_mid_state - slot.low_state,
                    delayed - slot.low_mid_state,
                ];
                self.cached_early_reflection_bufs[slot_index * self.frame_size + sample_index] =
                    bands[0] * slot.current_gain[0]
                        + bands[1] * slot.current_gain[1]
                        + bands[2] * slot.current_gain[2];
                if let Some(target) = target {
                    slot.arrival_direction = target.arrival_direction;
                }
            }
            write_index = (write_index + 1) % state.delay_line.len();
        }
        state.write_index = write_index;
        for (slot, target) in state.taps.iter_mut().zip(slot_targets) {
            if target.is_none() && slot.is_released() {
                slot.path_id = None;
            }
        }

        let listener_right = self.listener_right;
        let listener_up = self.listener_up;
        let listener_front = self.listener_front;
        let mut metrics = NativeHrtfRenderMetrics::default();
        for (slot_index, slot) in state.taps.iter_mut().enumerate() {
            if slot.path_id.is_none() && slot.is_released() {
                continue;
            }
            let direction = listener_local_direction(
                slot.arrival_direction,
                listener_right,
                listener_up,
                listener_front,
            );
            let reflection = &self.cached_early_reflection_bufs
                [slot_index * self.frame_size..(slot_index + 1) * self.frame_size];
            if self.use_ambisonics {
                self.native_ambisonics_encoder.encode_source_accumulate(
                    direction,
                    reflection,
                    &mut self.cached_summed_encoded_buf,
                )?;
            } else if let Some(hrtf_state) = &mut slot.hrtf_state {
                let tap_metrics = self.native_hrtf_renderer.render_source_with_metrics(
                    hrtf_state,
                    direction,
                    reflection,
                    &mut self.cached_binaural_processed,
                )?;
                metrics.direction_lookup_time_us += tap_metrics.direction_lookup_time_us;
                metrics.convolution_time_us += tap_metrics.convolution_time_us;
            }
        }
        if let Some(energy) = self.voice_energy_accumulators.get_mut(&voice_id) {
            energy.early_reflection_energy += block_energy(&self.cached_early_reflection_bufs);
        }
        Ok(metrics)
    }

    fn apply_native_hrtf_effect(
        &mut self,
        voice_id: VoiceId,
        direct_local_position: Vec3,
    ) -> Result<NativeHrtfRenderMetrics> {
        let direction = normalized_direction(direct_local_position);
        let state = self
            .native_hrtf_source_states
            .get_mut(&voice_id)
            .ok_or_else(|| {
                PetalSonicError::SpatialAudio(format!(
                    "No native HRTF state found for source {}",
                    voice_id
                ))
            })?;

        self.native_hrtf_renderer.render_source_with_metrics(
            state,
            direction,
            &self.cached_direct_buf,
            &mut self.cached_binaural_processed,
        )
    }

    fn apply_native_ambisonics_encode_effect(&mut self, direct_local_position: Vec3) -> Result<()> {
        let direction = normalized_direction(direct_local_position);
        self.native_ambisonics_encoder.encode_source_accumulate(
            direction,
            &self.cached_direct_buf,
            &mut self.cached_summed_encoded_buf,
        )
    }

    fn apply_native_ambisonics_decode_effect(&mut self) -> Result<NativeHrtfRenderMetrics> {
        let decoder = self.native_ambisonics_decoder.as_ref().ok_or_else(|| {
            PetalSonicError::SpatialAudio(
                "native Ambisonics decoder is not initialized".to_string(),
            )
        })?;
        let state = self.native_ambisonics_state.as_mut().ok_or_else(|| {
            PetalSonicError::SpatialAudio(
                "native Ambisonics decoder state is not initialized".to_string(),
            )
        })?;

        decoder.decode(
            state,
            &self.cached_summed_encoded_buf,
            &mut self.cached_binaural_processed,
        )
    }

    fn apply_hrtf_gain(&mut self) {
        for sample in self.cached_binaural_processed.iter_mut() {
            *sample *= self.hrtf_gain_linear;
        }
    }

    fn capture_environmental_acoustics_state(&mut self) {
        self.environmental_acoustics_active =
            self.environmental_acoustics_enabled.load(Ordering::Acquire);
    }
}

fn apply_three_band_gain(
    input: &[f32],
    output: &mut [f32],
    state: &mut NativeDirectSourceState,
    coefficients: ThreeBandCoefficients,
    target_gain: [f32; 3],
    broadband_gain: f32,
) {
    output.fill(0.0);
    for (output, input) in output.iter_mut().zip(input) {
        state.low_state += coefficients.low * (*input - state.low_state);
        state.low_mid_state += coefficients.low_mid * (*input - state.low_mid_state);
        let bands = [
            state.low_state,
            state.low_mid_state - state.low_state,
            *input - state.low_mid_state,
        ];
        for (current, target) in state.current_gain.iter_mut().zip(target_gain) {
            *current += coefficients.smoothing * (target - *current);
        }
        *output = (bands[0] * state.current_gain[0]
            + bands[1] * state.current_gain[1]
            + bands[2] * state.current_gain[2])
            * broadband_gain;
    }
}

fn normalized_direction(position: Vec3) -> Vec3 {
    if position.is_finite() && position.length_squared() > f32::EPSILON {
        position.normalize()
    } else {
        Vec3::Z
    }
}

fn block_energy(samples: &[f32]) -> f64 {
    samples
        .iter()
        .map(|sample| f64::from(*sample) * f64::from(*sample))
        .sum()
}

fn render_lobe_count(profile: OcclusionProfile) -> usize {
    match profile {
        OcclusionProfile::PointExact => MAX_DIRECT_LOBES,
        OcclusionProfile::AmbientDistributed(profile) => usize::from(profile.lobe_count()),
    }
}

fn load_native_hrtf_table(
    sample_rate: u32,
    native_hrtf_path: Option<&str>,
) -> Result<Arc<NativeHrtfTable>> {
    let table = if let Some(path) = native_hrtf_path {
        NativeHrtfTable::from_petalhrtf_file(path)?
    } else {
        NativeHrtfTable::from_petalhrtf_bytes(DEFAULT_NATIVE_HRTF_BYTES)?
    };
    if table.sample_rate() != sample_rate {
        return Err(PetalSonicError::Configuration(format!(
            "native HRTF sample rate {} does not match world sample rate {}",
            table.sample_rate(),
            sample_rate
        )));
    }

    Ok(Arc::new(table))
}

fn one_pole_coefficient(cutoff_hz: f32, sample_rate: u32) -> f32 {
    1.0 - (-std::f32::consts::TAU * cutoff_hz / sample_rate.max(1) as f32).exp()
}

fn reflection_delay_samples(delay_seconds: f32, sample_rate: u32, delay_line_len: usize) -> f32 {
    (delay_seconds * sample_rate as f32).clamp(1.0, delay_line_len.saturating_sub(2).max(1) as f32)
}

fn read_fractional_delay(delay_line: &[f32], write_index: usize, delay_samples: f32) -> f32 {
    let read_position =
        (write_index as f32 - delay_samples).rem_euclid(delay_line.len().max(1) as f32);
    let first = read_position.floor() as usize % delay_line.len();
    let second = (first + 1) % delay_line.len();
    let fraction = read_position - read_position.floor();
    delay_line[first] * (1.0 - fraction) + delay_line[second] * fraction
}

fn listener_local_direction(
    world_direction: Vec3,
    listener_right: Vec3,
    listener_up: Vec3,
    listener_front: Vec3,
) -> Vec3 {
    let direction =
        if world_direction.is_finite() && world_direction.length_squared() > f32::EPSILON {
            world_direction.normalize()
        } else {
            Vec3::Z
        };
    Vec3::new(
        direction.dot(listener_right),
        direction.dot(listener_up),
        direction.dot(listener_front),
    )
}

fn native_distance_attenuation(distance_meters: f32) -> f32 {
    if !distance_meters.is_finite() {
        return 0.0;
    }

    1.0 / distance_meters.max(1.0)
}

fn native_air_absorption(distance_meters: f32) -> f32 {
    if !distance_meters.is_finite() {
        return 0.0;
    }

    // Conservative broadband approximation used until native spectral filtering lands.
    // Roughly -1.7 dB over 100 m, enough to remove harshness without large loudness shifts.
    (-0.0002 * distance_meters.max(0.0)).exp().clamp(0.2, 1.0)
}

#[cfg(test)]
#[path = "acoustic_start_tests.rs"]
mod acoustic_start_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acoustic_propagation::{DirectAcousticResponse, EarlyReflectionTap};
    use crate::audio_data::PetalSonicAudioData;
    use crate::domain::{
        DirectPath, Emitter, EmitterDesc, EnvironmentSend, PlayCommandId, PlayOptions,
    };
    use crate::math::Quat;
    use crate::playback::prepare_test_voice;
    use std::time::Duration;

    fn empty_acoustic_response(
        spatial_revision: u64,
        geometry_version: u64,
    ) -> Arc<AcousticResponse> {
        Arc::new(AcousticResponse {
            spatial_revision,
            geometry_version,
            direct: Vec::new().into(),
            late_reverb: LateReverbParameters::SILENT,
            published_at: Instant::now(),
            solve_time_us: 0,
        })
    }

    fn lookup_contract_processor(max_voices: usize) -> SpatialProcessor {
        SpatialProcessor::new(SpatialProcessorConfig {
            sample_rate: 48_000,
            frame_size: 8,
            max_voices,
            distance_scaler: 1.0,
            native_hrtf_path: None,
            hrtf_gain: 0.0,
            use_ambisonics: true,
            environmental_acoustics_enabled: Arc::new(AtomicBool::new(true)),
        })
        .unwrap()
    }

    fn lookup_contract_voice(voice_id: VoiceId) -> PlaybackInstance {
        let audio = Arc::new(PetalSonicAudioData::new(
            vec![0.25; 64],
            48_000,
            1,
            Duration::from_secs_f64(64.0 / 48_000.0),
        ));
        let mut voice = prepare_test_voice(
            voice_id,
            Emitter {
                world_id: 1,
                index: 0,
                generation: 1,
            },
            audio,
            EmitterDesc::spatial(Pose::from_position(Vec3::Z)),
            PlayOptions::looping().with_environment_send(EnvironmentSend::follow_emitter()),
            None,
            0,
            8,
        )
        .start()
        .1;
        voice.set_mix_parameters(crate::domain::BusParams::default());
        voice
    }

    fn lookup_contract_response(
        voice_id: VoiceId,
        early_reflections: Vec<EarlyReflectionTap>,
    ) -> Arc<AcousticResponse> {
        Arc::new(AcousticResponse {
            spatial_revision: 1,
            geometry_version: 1,
            direct: vec![DirectAcousticResponse {
                voice_id,
                routing_generation: 0,
                gain: [1.0; 3],
                environment_gain: [1.0; 3],
                direct_lobes: Vec::new(),
                environment_representatives: Vec::new(),
                solve_status: crate::acoustic_propagation::DirectSolveStatus::Solved,
                cache_age_seconds: 0.0,
                early_reflections,
            }]
            .into(),
            late_reverb: LateReverbParameters::SILENT,
            published_at: Instant::now(),
            solve_time_us: 1,
        })
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct LookupContractActivity {
        lookups: usize,
        memory_operations: usize,
    }

    fn render_lookup_contract_quantum(
        processor: &mut SpatialProcessor,
        voice_id: VoiceId,
        voices: &mut HashMap<VoiceId, PlaybackInstance>,
    ) -> LookupContractActivity {
        let mut output = [0.0; 16];
        let mut events = Vec::new();
        let mut lookups = 0;
        let memory_operations = crate::test_support::realtime_memory_activity(|| {
            lookups = crate::test_support::acoustic_response_lookup_activity(|| {
                processor
                    .process_spatial_sources_with_metrics(
                        &[voice_id],
                        voices,
                        &mut output,
                        SpatialRenderContext::default(),
                        &mut events,
                    )
                    .unwrap();
            });
        });
        LookupContractActivity {
            lookups,
            memory_operations,
        }
    }

    fn assert_lookup_contract(label: &str, activity: LookupContractActivity) {
        assert_eq!(
            activity.lookups, 1,
            "{label} must resolve exactly one acoustic envelope"
        );
        assert_eq!(
            activity.memory_operations, 0,
            "{label} allocated, reallocated, or freed on the realtime path"
        );
    }

    #[test]
    fn each_active_voice_resolves_one_acoustic_envelope_per_quantum() {
        let missing_voice_id = VoiceId::from(11);
        let mut missing_processor = lookup_contract_processor(1);
        missing_processor.replace_acoustic_response(empty_acoustic_response(1, 1));
        let mut missing_voices =
            HashMap::from([(missing_voice_id, lookup_contract_voice(missing_voice_id))]);
        let missing_entry = render_lookup_contract_quantum(
            &mut missing_processor,
            missing_voice_id,
            &mut missing_voices,
        );
        assert!(
            missing_processor
                .native_early_reflection_source_states
                .is_empty(),
            "a missing response entry must not mint early-reflection state"
        );
        assert_eq!(missing_voices[&missing_voice_id].info.current_frame, 8);

        let no_tap_voice_id = VoiceId::from(12);
        let mut no_tap_processor = lookup_contract_processor(1);
        no_tap_processor
            .replace_acoustic_response(lookup_contract_response(no_tap_voice_id, Vec::new()));
        let mut no_tap_voices =
            HashMap::from([(no_tap_voice_id, lookup_contract_voice(no_tap_voice_id))]);
        let no_tap = render_lookup_contract_quantum(
            &mut no_tap_processor,
            no_tap_voice_id,
            &mut no_tap_voices,
        );
        assert!(
            no_tap_processor
                .native_early_reflection_source_states
                .is_empty(),
            "a response without taps must not mint early-reflection state"
        );
        assert_eq!(no_tap_voices[&no_tap_voice_id].info.current_frame, 8);

        let new_state_voice_id = VoiceId::from(13);
        let mut new_state_processor = lookup_contract_processor(1);
        new_state_processor.replace_acoustic_response(lookup_contract_response(
            new_state_voice_id,
            vec![EarlyReflectionTap {
                path_id: 13,
                arrival_direction: Vec3::Z,
                delay_seconds: 0.01,
                gain: [0.1; 3],
            }],
        ));
        let mut new_state_voices = HashMap::from([(
            new_state_voice_id,
            lookup_contract_voice(new_state_voice_id),
        )]);
        let new_state = render_lookup_contract_quantum(
            &mut new_state_processor,
            new_state_voice_id,
            &mut new_state_voices,
        );
        assert!(
            new_state_processor
                .native_early_reflection_source_states
                .contains_key(&new_state_voice_id),
            "the cold path must mint the Voice's preallocated early-reflection state"
        );
        assert!(
            new_state_processor
                .free_native_early_reflection_source_states
                .is_empty(),
            "the cold path must consume its preallocated state"
        );
        assert_eq!(new_state_voices[&new_state_voice_id].info.current_frame, 8);

        let retired_voice_id = VoiceId::from(14);
        let reused_voice_id = VoiceId::from(15);
        let mut reuse_processor = lookup_contract_processor(1);
        reuse_processor.replace_acoustic_response(lookup_contract_response(
            retired_voice_id,
            vec![EarlyReflectionTap {
                path_id: 14,
                arrival_direction: Vec3::Z,
                delay_seconds: 0.01,
                gain: [0.1; 3],
            }],
        ));
        let mut retired_voices =
            HashMap::from([(retired_voice_id, lookup_contract_voice(retired_voice_id))]);
        render_lookup_contract_quantum(&mut reuse_processor, retired_voice_id, &mut retired_voices);
        reuse_processor.silence_voice_state(retired_voice_id);
        assert!(
            reuse_processor
                .free_native_early_reflection_source_states
                .is_empty()
        );
        assert!(
            reuse_processor.native_early_reflection_source_states[&retired_voice_id].is_released(),
            "the fixture must expose an in-map released state for transfer"
        );
        reuse_processor.replace_acoustic_response(lookup_contract_response(
            reused_voice_id,
            vec![EarlyReflectionTap {
                path_id: 15,
                arrival_direction: Vec3::Z,
                delay_seconds: 0.01,
                gain: [0.1; 3],
            }],
        ));
        let mut reused_voices =
            HashMap::from([(reused_voice_id, lookup_contract_voice(reused_voice_id))]);
        let pool_reuse = render_lookup_contract_quantum(
            &mut reuse_processor,
            reused_voice_id,
            &mut reused_voices,
        );
        assert!(
            !reuse_processor
                .native_early_reflection_source_states
                .contains_key(&retired_voice_id)
                && reuse_processor
                    .native_early_reflection_source_states
                    .contains_key(&reused_voice_id),
            "the production path must transfer the released state to the new Voice"
        );
        assert_eq!(
            reuse_processor.native_early_reflection_source_states.len(),
            1,
            "pool transfer must not mint parallel state"
        );
        assert_eq!(reused_voices[&reused_voice_id].info.current_frame, 8);

        assert_lookup_contract("missing response entry", missing_entry);
        assert_lookup_contract("response without early-reflection taps", no_tap);
        assert_lookup_contract("cold early-reflection state mint", new_state);
        assert_lookup_contract("released early-reflection pool transfer", pool_reuse);
    }

    #[test]
    fn render_accepts_lagging_pose_but_rejects_response_rollbacks_and_wrong_scene() {
        let mut processor = SpatialProcessor::new(SpatialProcessorConfig {
            sample_rate: 48_000,
            frame_size: 8,
            max_voices: 1,
            distance_scaler: 1.0,
            native_hrtf_path: None,
            hrtf_gain: 0.0,
            use_ambisonics: false,
            environmental_acoustics_enabled: Arc::new(AtomicBool::new(true)),
        })
        .unwrap();

        assert!(matches!(
            processor.replace_acoustic_response(empty_acoustic_response(10, 20)),
            AcousticResponseReplacement::Accepted(None)
        ));
        assert!(matches!(
            processor.replace_acoustic_response(empty_acoustic_response(9, 20)),
            AcousticResponseReplacement::Rejected(_)
        ));
        assert!(matches!(
            processor.replace_acoustic_response(empty_acoustic_response(11, 19)),
            AcousticResponseReplacement::Rejected(_)
        ));
        assert!(matches!(
            processor.replace_acoustic_response_for_scene(empty_acoustic_response(10, 20), 20),
            AcousticResponseReplacement::Accepted(_)
        ));
        assert!(matches!(
            processor.replace_acoustic_response_for_scene(empty_acoustic_response(11, 20), 21),
            AcousticResponseReplacement::Rejected(_)
        ));

        let current = processor.acoustic_response.as_ref().unwrap();
        assert_eq!(current.spatial_revision, 10);
        assert_eq!(current.geometry_version, 20);
    }

    #[test]
    fn native_distance_attenuation_is_clamped_near_listener() {
        assert_eq!(native_distance_attenuation(0.0), 1.0);
        assert_eq!(native_distance_attenuation(0.5), 1.0);
        assert_eq!(native_distance_attenuation(1.0), 1.0);
        assert!((native_distance_attenuation(4.0) - 0.25).abs() < 1e-6);
    }

    #[test]
    fn bounded_lobe_decorrelation_preserves_normalized_impulse_power() {
        let weights = [0.4_f32, 0.3, 0.2, 0.1];
        let mut total_energy = 0.0_f32;
        for (lobe_id, power) in weights.into_iter().enumerate() {
            let mut decorrelator = LobeDecorrelator::new(lobe_id as u8);
            let mut lobe_energy = 0.0;
            for sample_index in 0..512 {
                let input = if sample_index == 0 { power.sqrt() } else { 0.0 };
                let output = decorrelator.process(input);
                lobe_energy += output * output;
            }
            total_energy += lobe_energy;
        }

        assert!((total_energy - 1.0).abs() < 1.0e-4);
    }

    #[test]
    fn distributed_direct_field_advances_one_cursor_once_and_stays_bounded() {
        use crate::domain::{
            DistributedOcclusionProfile, ExtentSample, ExtentSampleId, OcclusionProfile,
            SourceExtent,
        };

        let voice_id = VoiceId::from(55);
        let emitter = Emitter {
            world_id: 1,
            index: 0,
            generation: 1,
        };
        let source_extent = SourceExtent::weighted_samples(vec![
            ExtentSample::new(ExtentSampleId(1), Vec3::X, 0.5).unwrap(),
            ExtentSample::new(ExtentSampleId(2), Vec3::Z, 0.5).unwrap(),
        ])
        .unwrap();
        let profile = OcclusionProfile::AmbientDistributed(
            DistributedOcclusionProfile::default()
                .with_lobe_count(2)
                .unwrap(),
        );
        let audio = Arc::new(PetalSonicAudioData::new(
            vec![0.25; 64],
            48_000,
            1,
            Duration::from_secs_f64(64.0 / 48_000.0),
        ));
        let mut voice = prepare_test_voice(
            voice_id,
            emitter,
            audio,
            EmitterDesc::spatial(Pose::from_position(Vec3::Z))
                .with_extent(source_extent)
                .with_occlusion_profile(profile),
            PlayOptions::looping().with_environment_send(EnvironmentSend::disabled()),
            None,
            0,
            8,
        )
        .start()
        .1;
        voice.set_mix_parameters(crate::domain::BusParams::default());
        let mut voices = [(voice_id, voice)].into_iter().collect();
        let mut processor = SpatialProcessor::new(SpatialProcessorConfig {
            sample_rate: 48_000,
            frame_size: 8,
            max_voices: 1,
            distance_scaler: 1.0,
            native_hrtf_path: None,
            hrtf_gain: 0.0,
            use_ambisonics: false,
            environmental_acoustics_enabled: Arc::new(AtomicBool::new(true)),
        })
        .unwrap();
        processor.replace_acoustic_response(Arc::new(AcousticResponse {
            spatial_revision: 1,
            geometry_version: 1,
            direct: vec![DirectAcousticResponse {
                voice_id,
                routing_generation: 0,
                gain: [1.0; 3],
                environment_gain: [1.0; 3],
                direct_lobes: vec![
                    crate::acoustic_propagation::DirectLobeTarget {
                        lobe_id: 0,
                        direction: Vec3::X,
                        gain: [std::f32::consts::FRAC_1_SQRT_2; 3],
                        power: 0.5,
                    },
                    crate::acoustic_propagation::DirectLobeTarget {
                        lobe_id: 1,
                        direction: Vec3::Z,
                        gain: [std::f32::consts::FRAC_1_SQRT_2; 3],
                        power: 0.5,
                    },
                ],
                environment_representatives: Vec::new(),
                early_reflections: Vec::new(),
                solve_status: crate::acoustic_propagation::DirectSolveStatus::Solved,
                cache_age_seconds: 0.0,
            }]
            .into(),
            late_reverb: LateReverbParameters::SILENT,
            published_at: Instant::now(),
            solve_time_us: 1,
        }));
        let mut output = [0.0; 16];
        let mut events = Vec::new();

        processor
            .process_spatial_sources_with_metrics(
                &[voice_id],
                &mut voices,
                &mut output,
                SpatialRenderContext::default(),
                &mut events,
            )
            .unwrap();

        let memory_activity = crate::test_support::realtime_memory_activity(|| {
            processor
                .process_spatial_sources_with_metrics(
                    &[voice_id],
                    &mut voices,
                    &mut output,
                    SpatialRenderContext::default(),
                    &mut events,
                )
                .unwrap();
        });

        assert_eq!(
            memory_activity, 0,
            "steady distributed rendering allocated or freed"
        );
        assert_eq!(voices[&voice_id].info.current_frame, 16);
        assert_eq!(processor.native_direct_lobe_source_states.len(), 1);
        assert!(output.iter().all(|sample| sample.is_finite()));
        assert!(output.iter().any(|sample| sample.abs() > 0.0));
    }

    #[test]
    fn native_air_absorption_stays_bounded() {
        assert_eq!(native_air_absorption(f32::NAN), 0.0);
        assert_eq!(native_air_absorption(-10.0), 1.0);
        assert!((0.2..=1.0).contains(&native_air_absorption(10_000.0)));
    }

    #[test]
    fn listener_relative_direct_pose_is_invariant_under_world_motion() {
        let mut processor = SpatialProcessor::new(SpatialProcessorConfig {
            sample_rate: 48_000,
            frame_size: 8,
            max_voices: 1,
            distance_scaler: 1.0,
            native_hrtf_path: None,
            hrtf_gain: 0.0,
            use_ambisonics: false,
            environmental_acoustics_enabled: Arc::new(AtomicBool::new(false)),
        })
        .unwrap();
        let local_pose = Pose::from_position(Vec3::new(0.02, -0.08, 0.0));
        for listener in [
            Pose::identity(),
            Pose::new(Vec3::new(40.0, -3.0, 12.0), Quat::from_rotation_y(1.7)),
            Pose::new(Vec3::new(-8.0, 6.0, 2.0), Quat::from_rotation_x(-0.4)),
        ] {
            processor.set_listener_pose(listener).unwrap();
            assert_eq!(
                processor.resolve_direct_local_position(
                    DirectPlacement::ListenerRelative(local_pose),
                    Vec3::new(999.0, 999.0, 999.0),
                ),
                Some(local_pose.position)
            );
        }
    }

    #[test]
    fn listener_position_relative_direct_keeps_world_gravity_offset_under_pitch() {
        let mut processor = SpatialProcessor::new(SpatialProcessorConfig {
            sample_rate: 48_000,
            frame_size: 8,
            max_voices: 1,
            distance_scaler: 1.0,
            native_hrtf_path: None,
            hrtf_gain: 0.0,
            use_ambisonics: false,
            environmental_acoustics_enabled: Arc::new(AtomicBool::new(false)),
        })
        .unwrap();
        let world_offset = Pose::from_position(Vec3::new(0.0, -0.08, 0.0));

        processor
            .set_listener_pose(Pose::new(Vec3::new(40.0, -3.0, 12.0), Quat::IDENTITY))
            .unwrap();
        assert_eq!(
            processor.resolve_direct_local_position(
                DirectPlacement::ListenerPositionRelative(world_offset),
                Vec3::new(999.0, 999.0, 999.0),
            ),
            Some(world_offset.position),
        );

        processor
            .set_listener_pose(Pose::new(
                Vec3::new(-8.0, 6.0, 2.0),
                Quat::from_rotation_x(-89.0_f32.to_radians()),
            ))
            .unwrap();
        let looking_down = processor
            .resolve_direct_local_position(
                DirectPlacement::ListenerPositionRelative(world_offset),
                Vec3::new(-999.0, -999.0, -999.0),
            )
            .unwrap();
        assert!(
            looking_down.z > 0.079,
            "world-down footstep did not move in front: {looking_down:?}"
        );
        assert!(
            looking_down.y.abs() < 0.002,
            "world-down footstep stayed below: {looking_down:?}"
        );
    }

    #[test]
    fn split_routing_advances_one_cursor_and_never_adds_a_second_direct_copy() {
        fn render_once(environment_send: EnvironmentSend) -> ([f32; 16], usize) {
            let voice_id = VoiceId::from(3);
            let audio = Arc::new(PetalSonicAudioData::new(
                (1..=32).map(|sample| sample as f32 / 32.0).collect(),
                48_000,
                1,
                Duration::from_secs_f64(32.0 / 48_000.0),
            ));
            let mut voice = prepare_test_voice(
                voice_id,
                Emitter {
                    world_id: 1,
                    index: 0,
                    generation: 1,
                },
                audio,
                EmitterDesc::spatial(Pose::from_position(Vec3::Z)),
                PlayOptions::once()
                    .with_direct_path(
                        DirectPath::listener_relative(Pose::from_position(Vec3::X))
                            .with_geometry(DirectGeometry::BypassTransmission),
                    )
                    .with_environment_send(environment_send),
                None,
                0,
                8,
            )
            .start()
            .1;
            voice.set_mix_parameters(crate::domain::BusParams::default());
            let mut voices = HashMap::from([(voice_id, voice)]);
            let mut processor = SpatialProcessor::new(SpatialProcessorConfig {
                sample_rate: 48_000,
                frame_size: 8,
                max_voices: 1,
                distance_scaler: 1.0,
                native_hrtf_path: None,
                hrtf_gain: 0.0,
                use_ambisonics: false,
                environmental_acoustics_enabled: Arc::new(AtomicBool::new(false)),
            })
            .unwrap();
            let mut output = [0.0; 16];
            processor
                .process_spatial_sources_with_metrics(
                    &[voice_id],
                    &mut voices,
                    &mut output,
                    SpatialRenderContext::default(),
                    &mut Vec::new(),
                )
                .unwrap();
            (output, voices[&voice_id].info.current_frame)
        }

        let (split_output, split_cursor) = render_once(EnvironmentSend::from_world_pose(
            Pose::from_position(Vec3::X),
        ));
        let (direct_only_output, direct_only_cursor) = render_once(EnvironmentSend::disabled());

        assert_eq!(split_cursor, 8);
        assert_eq!(direct_only_cursor, 8);
        assert_eq!(split_output, direct_only_output);
        assert!(split_output.iter().any(|sample| sample.abs() > 0.0));
    }

    #[test]
    fn opted_in_voice_telemetry_correlates_first_render_and_late_environment_response() {
        let voice_id = VoiceId::from(9);
        let emitter = Emitter {
            world_id: 1,
            index: 4,
            generation: 2,
        };
        let play_command_id = PlayCommandId(71);
        let direct_local_pose = Pose::from_position(Vec3::new(0.02, -0.08, 0.0));
        let acoustic_origin = Pose::from_position(Vec3::new(14.0, 0.0, -3.0));
        let audio = Arc::new(PetalSonicAudioData::new(
            vec![0.5; 32],
            48_000,
            1,
            Duration::from_secs_f64(32.0 / 48_000.0),
        ));
        let mut voice = prepare_test_voice(
            voice_id,
            emitter,
            audio,
            EmitterDesc::spatial(Pose::from_position(Vec3::new(99.0, 2.0, 8.0))),
            PlayOptions::once()
                .with_direct_path(
                    DirectPath::listener_relative(direct_local_pose)
                        .with_geometry(DirectGeometry::BypassTransmission),
                )
                .with_environment_send(EnvironmentSend::from_world_pose(acoustic_origin))
                .with_play_command_id(play_command_id),
            None,
            0,
            8,
        )
        .start()
        .1;
        voice.set_mix_parameters(crate::domain::BusParams::default());
        let mut voices = HashMap::from([(voice_id, voice)]);
        let mut processor = SpatialProcessor::new(SpatialProcessorConfig {
            sample_rate: 48_000,
            frame_size: 8,
            max_voices: 1,
            distance_scaler: 1.0,
            native_hrtf_path: None,
            hrtf_gain: 0.0,
            use_ambisonics: false,
            environmental_acoustics_enabled: Arc::new(AtomicBool::new(true)),
        })
        .unwrap();
        let mut output = [0.0; 16];
        let mut events = Vec::with_capacity(2);

        processor
            .process_spatial_sources_with_metrics(
                &[voice_id],
                &mut voices,
                &mut output,
                SpatialRenderContext {
                    render_block_index: 17,
                    spatial_revision: 23,
                },
                &mut events,
            )
            .unwrap();
        assert_eq!(events.len(), 1);
        let VoiceTelemetryEvent::FirstRendered(first) = events[0] else {
            panic!("expected first-render telemetry");
        };
        assert_eq!(first.play_command_id, play_command_id);
        assert_eq!(first.emitter, emitter);
        assert_eq!(first.render_block_index, 17);
        assert_eq!(first.spatial_revision, 23);
        assert_eq!(first.direct_local_pose, Some(direct_local_pose));
        assert_eq!(first.acoustic_origin, Some(acoustic_origin));
        assert_eq!(first.environment_response, None);

        events.clear();
        processor.replace_acoustic_response(Arc::new(AcousticResponse {
            spatial_revision: 24,
            geometry_version: 8,
            direct: vec![DirectAcousticResponse {
                voice_id,
                routing_generation: 0,
                gain: [1.0; 3],
                environment_gain: [1.0; 3],
                direct_lobes: Vec::new(),
                environment_representatives: Vec::new(),
                solve_status: crate::acoustic_propagation::DirectSolveStatus::Solved,
                cache_age_seconds: 0.0,
                early_reflections: Vec::new(),
            }]
            .into(),
            late_reverb: LateReverbParameters {
                pre_delay_seconds: 0.02,
                rt60_seconds: [0.7, 0.9, 1.1],
                wet_gain: 0.4,
            },
            published_at: Instant::now() - Duration::from_millis(7),
            solve_time_us: 1,
        }));
        processor
            .process_spatial_sources_with_metrics(
                &[voice_id],
                &mut voices,
                &mut output,
                SpatialRenderContext {
                    render_block_index: 18,
                    spatial_revision: 24,
                },
                &mut events,
            )
            .unwrap();
        assert_eq!(events.len(), 1);
        let VoiceTelemetryEvent::EnvironmentResponse {
            play_command_id: response_id,
            response,
        } = events[0]
        else {
            panic!("expected environment-response telemetry");
        };
        assert_eq!(response_id, play_command_id);
        assert_eq!(response.spatial_revision, 24);
        assert_eq!(response.geometry_version, 8);
        assert!(response.age >= Duration::from_millis(7));

        events.clear();
        processor
            .process_spatial_sources_with_metrics(
                &[voice_id],
                &mut voices,
                &mut output,
                SpatialRenderContext {
                    render_block_index: 19,
                    spatial_revision: 24,
                },
                &mut events,
            )
            .unwrap();
        assert!(events.is_empty(), "voice telemetry must be one-shot");

        let _ = processor.retire_voice(voice_id);
        assert!(processor.has_pending_voice_telemetry());
        events.clear();
        processor
            .process_spatial_sources_with_metrics(
                &[],
                &mut voices,
                &mut output,
                SpatialRenderContext {
                    render_block_index: 20,
                    spatial_revision: 24,
                },
                &mut events,
            )
            .unwrap();
        assert_eq!(events.len(), 1);
        let VoiceTelemetryEvent::EnergySummary(energy) = events[0] else {
            panic!("expected final Voice energy telemetry");
        };
        assert_eq!(energy.play_command_id, play_command_id);
        assert_eq!(energy.emitter, emitter);
        assert!(energy.source_energy > 0.0);
        assert!(energy.direct_energy > 0.0);
        assert!(energy.environment_send_energy > 0.0);
        assert_eq!(energy.early_reflection_energy, 0.0);
        assert_eq!(energy.late_reverb.pre_delay_seconds, 0.02);
        assert_eq!(energy.late_reverb.rt60_seconds, [0.7, 0.9, 1.1]);
        assert_eq!(energy.late_reverb.wet_gain, 0.4);
        assert!(energy.late_reverb.cumulative_input_energy > 0.0);
        assert!(energy.late_reverb.cumulative_output_energy >= 0.0);
        assert!(!processor.has_pending_voice_telemetry());
    }

    #[test]
    fn environmental_acoustics_toggle_bypasses_geometry_but_keeps_native_direct_path() {
        let enabled = Arc::new(AtomicBool::new(true));
        let voice_id = VoiceId::from(1);
        let mut processor = SpatialProcessor::new(SpatialProcessorConfig {
            sample_rate: 48_000,
            frame_size: 8,
            max_voices: 1,
            distance_scaler: 1.0,
            native_hrtf_path: None,
            hrtf_gain: 0.0,
            use_ambisonics: false,
            environmental_acoustics_enabled: enabled.clone(),
        })
        .unwrap();
        processor.ensure_native_direct_state_for_voice(voice_id);
        processor.replace_acoustic_response(Arc::new(AcousticResponse {
            spatial_revision: 1,
            geometry_version: 1,
            direct: vec![DirectAcousticResponse {
                voice_id,
                routing_generation: 0,
                gain: [0.0; 3],
                environment_gain: [0.0; 3],
                direct_lobes: Vec::new(),
                environment_representatives: Vec::new(),
                solve_status: crate::acoustic_propagation::DirectSolveStatus::Solved,
                cache_age_seconds: 0.0,
                early_reflections: Vec::new(),
            }]
            .into(),
            late_reverb: LateReverbParameters::SILENT,
            published_at: Instant::now(),
            solve_time_us: 1,
        }));
        let acoustic_response = processor.acoustic_response.clone().unwrap();
        let voice_acoustics = acoustic_response.voice_response(voice_id);

        for _ in 0..1_200 {
            processor.cached_input_buf.fill(1.0);
            processor
                .apply_native_direct_effect(
                    voice_id,
                    voice_acoustics,
                    Vec3::Z,
                    DirectGeometry::SimulatedTransmission,
                )
                .unwrap();
        }
        assert!(
            processor
                .cached_direct_buf
                .iter()
                .all(|sample| *sample < 0.05)
        );

        enabled.store(false, Ordering::Release);
        processor.capture_environmental_acoustics_state();
        for _ in 0..1_200 {
            processor.cached_input_buf.fill(1.0);
            processor
                .apply_native_direct_effect(
                    voice_id,
                    voice_acoustics,
                    Vec3::Z,
                    DirectGeometry::SimulatedTransmission,
                )
                .unwrap();
        }
        assert!(
            processor
                .cached_direct_buf
                .iter()
                .all(|sample| *sample > 0.95)
        );
    }

    #[test]
    fn early_reflection_delay_and_runtime_fade_remain_finite() {
        let enabled = Arc::new(AtomicBool::new(true));
        let voice_id = VoiceId::from(1);
        let mut processor = SpatialProcessor::new(SpatialProcessorConfig {
            sample_rate: 48_000,
            frame_size: 8,
            max_voices: 1,
            distance_scaler: 1.0,
            native_hrtf_path: None,
            hrtf_gain: 0.0,
            use_ambisonics: false,
            environmental_acoustics_enabled: enabled.clone(),
        })
        .unwrap();
        processor.replace_acoustic_response(Arc::new(AcousticResponse {
            spatial_revision: 1,
            geometry_version: 1,
            direct: vec![DirectAcousticResponse {
                voice_id,
                routing_generation: 0,
                gain: [1.0; 3],
                environment_gain: [1.0; 3],
                direct_lobes: Vec::new(),
                environment_representatives: Vec::new(),
                solve_status: crate::acoustic_propagation::DirectSolveStatus::Solved,
                cache_age_seconds: 0.0,
                early_reflections: vec![EarlyReflectionTap {
                    path_id: 7,
                    arrival_direction: Vec3::Z,
                    delay_seconds: 4.0 / 48_000.0,
                    gain: [0.5, 0.25, 0.1],
                }],
            }]
            .into(),
            late_reverb: LateReverbParameters::SILENT,
            published_at: Instant::now(),
            solve_time_us: 1,
        }));
        let acoustic_response = processor.acoustic_response.clone().unwrap();
        let voice_acoustics = acoustic_response.voice_response(voice_id);
        processor.ensure_native_early_reflection_state_for_voice(voice_id, voice_acoustics);

        let mut reflected_energy = 0.0;
        for block in 0..8 {
            processor.cached_input_buf.fill(0.0);
            if block == 0 {
                processor.cached_input_buf[0] = 1.0;
            }
            processor.cached_binaural_processed.fill(0.0);
            processor
                .apply_native_early_reflections(voice_id, voice_acoustics, 1.0, true)
                .unwrap();
            reflected_energy += processor
                .cached_binaural_processed
                .iter()
                .map(|sample| sample * sample)
                .sum::<f32>();
            assert!(
                processor
                    .cached_binaural_processed
                    .iter()
                    .all(|sample| sample.is_finite())
            );
        }
        assert!(reflected_energy > 0.0);

        enabled.store(false, Ordering::Release);
        processor.capture_environmental_acoustics_state();
        for _ in 0..1_200 {
            processor.cached_input_buf.fill(0.0);
            processor.cached_binaural_processed.fill(0.0);
            processor
                .apply_native_early_reflections(voice_id, voice_acoustics, 1.0, true)
                .unwrap();
        }
        let state = &processor.native_early_reflection_source_states[&voice_id];
        assert!(state.taps.iter().all(|tap| tap.path_id.is_none()));
    }

    #[test]
    fn completed_voice_drains_bounded_early_and_late_environment_tail() {
        let voice_id = VoiceId::from(7);
        let mut processor = SpatialProcessor::new(SpatialProcessorConfig {
            sample_rate: 48_000,
            frame_size: 1_024,
            max_voices: 1,
            distance_scaler: 1.0,
            native_hrtf_path: None,
            hrtf_gain: 0.0,
            use_ambisonics: false,
            environmental_acoustics_enabled: Arc::new(AtomicBool::new(true)),
        })
        .unwrap();
        processor.replace_acoustic_response(Arc::new(AcousticResponse {
            spatial_revision: 1,
            geometry_version: 1,
            direct: vec![DirectAcousticResponse {
                voice_id,
                routing_generation: 0,
                gain: [1.0; 3],
                environment_gain: [1.0; 3],
                direct_lobes: Vec::new(),
                environment_representatives: Vec::new(),
                solve_status: crate::acoustic_propagation::DirectSolveStatus::Solved,
                cache_age_seconds: 0.0,
                early_reflections: vec![EarlyReflectionTap {
                    path_id: 4,
                    arrival_direction: Vec3::Z,
                    delay_seconds: 0.03,
                    gain: [0.8; 3],
                }],
            }]
            .into(),
            late_reverb: LateReverbParameters {
                pre_delay_seconds: 0.0,
                rt60_seconds: [0.2; 3],
                wet_gain: 0.5,
            },
            published_at: Instant::now(),
            solve_time_us: 1,
        }));
        let acoustic_response = processor.acoustic_response.clone().unwrap();
        let voice_acoustics = acoustic_response.voice_response(voice_id);
        processor.ensure_native_early_reflection_state_for_voice(voice_id, voice_acoustics);
        let play_command_id = PlayCommandId(88);
        processor.voice_energy_accumulators.insert(
            voice_id,
            VoiceEnergyAccumulator::new(
                play_command_id,
                Emitter {
                    world_id: 1,
                    index: 7,
                    generation: 1,
                },
            ),
        );
        processor.cached_input_buf.fill(0.0);
        processor.cached_input_buf[0] = 1.0;
        processor.cached_binaural_processed.fill(0.0);
        processor
            .apply_native_early_reflections(voice_id, voice_acoustics, 1.0, true)
            .unwrap();
        processor.cached_late_reverb_input.fill(0.0);
        processor.cached_late_reverb_input[0] = 1.0;
        processor.late_reverb.process_block(
            &processor.cached_late_reverb_input,
            &mut processor.cached_binaural_processed,
            true,
        );

        let _ = processor.retire_voice(voice_id);
        assert!(processor.has_environment_tail());
        assert!(processor.native_early_reflection_source_states[&voice_id].draining);

        let mut early_tail_energy = 0.0;
        let mut late_tail_energy_after_early_release = 0.0;
        for _ in 0..256 {
            processor.cached_binaural_processed.fill(0.0);
            processor.process_draining_early_reflections().unwrap();
            let early_active = processor
                .native_early_reflection_source_states
                .contains_key(&voice_id);
            let early_and_late_energy = processor
                .cached_binaural_processed
                .iter()
                .map(|sample| sample * sample)
                .sum::<f32>();
            if early_active {
                early_tail_energy += early_and_late_energy;
            }
            processor.cached_late_reverb_input.fill(0.0);
            processor.late_reverb.process_block(
                &processor.cached_late_reverb_input,
                &mut processor.cached_binaural_processed,
                true,
            );
            if !early_active {
                late_tail_energy_after_early_release += processor
                    .cached_binaural_processed
                    .iter()
                    .map(|sample| sample * sample)
                    .sum::<f32>();
            }
            if !processor.has_environment_tail() {
                break;
            }
        }
        assert!(
            early_tail_energy > 0.0,
            "delayed early response was truncated"
        );
        assert!(
            late_tail_energy_after_early_release > 0.0,
            "shared late response stopped with the Voice"
        );
        assert!(
            !processor
                .native_early_reflection_source_states
                .contains_key(&voice_id),
            "early tail did not finish in bound"
        );
        assert!(
            !processor.has_environment_tail(),
            "late tail did not finish in bound"
        );
        let mut events = Vec::with_capacity(1);
        processor.emit_pending_voice_energy(&mut events);
        let VoiceTelemetryEvent::EnergySummary(energy) = events[0] else {
            panic!("expected energy summary after early tail release");
        };
        assert_eq!(energy.play_command_id, play_command_id);
        assert!(energy.early_reflection_energy > 0.0);
        assert!(energy.late_reverb.cumulative_input_energy > 0.0);
        assert!(energy.late_reverb.cumulative_output_energy > 0.0);
    }

    #[test]
    fn early_reflection_source_state_pool_stays_bounded_across_priority_changes() {
        let enabled = Arc::new(AtomicBool::new(true));
        let mut processor = SpatialProcessor::new(SpatialProcessorConfig {
            sample_rate: 48_000,
            frame_size: 8,
            max_voices: 64,
            distance_scaler: 1.0,
            native_hrtf_path: None,
            hrtf_gain: 0.0,
            use_ambisonics: true,
            environmental_acoustics_enabled: enabled,
        })
        .unwrap();

        for generation in 0..3 {
            let first_index = generation * EARLY_REFLECTION_SOURCE_STATE_CAPACITY;
            let direct = (first_index..first_index + EARLY_REFLECTION_SOURCE_STATE_CAPACITY)
                .map(|index| DirectAcousticResponse {
                    voice_id: VoiceId::from(index as u64),
                    routing_generation: 0,
                    gain: [1.0; 3],
                    environment_gain: [1.0; 3],
                    direct_lobes: Vec::new(),
                    environment_representatives: Vec::new(),
                    solve_status: crate::acoustic_propagation::DirectSolveStatus::Solved,
                    cache_age_seconds: 0.0,
                    early_reflections: vec![EarlyReflectionTap {
                        path_id: index as u16,
                        arrival_direction: Vec3::Z,
                        delay_seconds: 0.01,
                        gain: [0.1; 3],
                    }],
                })
                .collect();
            processor.replace_acoustic_response(Arc::new(AcousticResponse {
                spatial_revision: generation as u64 + 1,
                geometry_version: 1,
                direct,
                late_reverb: LateReverbParameters::SILENT,
                published_at: Instant::now(),
                solve_time_us: 1,
            }));
            let acoustic_response = processor.acoustic_response.clone().unwrap();
            for index in first_index..first_index + EARLY_REFLECTION_SOURCE_STATE_CAPACITY {
                let voice_id = VoiceId::from(index as u64);
                let voice_acoustics = acoustic_response.voice_response(voice_id);
                processor.ensure_native_early_reflection_state_for_voice(voice_id, voice_acoustics);
            }
            assert!(
                processor.native_early_reflection_source_states.len()
                    <= EARLY_REFLECTION_SOURCE_STATE_CAPACITY
            );
        }
    }

    #[test]
    #[ignore = "release-mode performance probe"]
    fn active_early_reflections_release_budget() {
        use std::hint::black_box;

        const SAMPLE_RATE: u32 = 48_000;
        const FRAMES: usize = 1_024;
        const BLOCKS: usize = 1_000;
        let enabled = Arc::new(AtomicBool::new(true));
        let mut processor = SpatialProcessor::new(SpatialProcessorConfig {
            sample_rate: SAMPLE_RATE,
            frame_size: FRAMES,
            max_voices: 8,
            distance_scaler: 1.0,
            native_hrtf_path: None,
            hrtf_gain: 0.0,
            use_ambisonics: false,
            environmental_acoustics_enabled: enabled,
        })
        .unwrap();
        let sources: Vec<_> = (0..8)
            .map(|index| {
                (
                    VoiceId::from(index as u64),
                    Emitter {
                        world_id: 1,
                        index,
                        generation: 1,
                    },
                )
            })
            .collect();
        let direct = sources
            .iter()
            .map(|(voice_id, _)| DirectAcousticResponse {
                voice_id: *voice_id,
                routing_generation: 0,
                gain: [1.0; 3],
                environment_gain: [1.0; 3],
                direct_lobes: Vec::new(),
                environment_representatives: Vec::new(),
                solve_status: crate::acoustic_propagation::DirectSolveStatus::Solved,
                cache_age_seconds: 0.0,
                early_reflections: (0..MAX_EARLY_REFLECTION_TAPS)
                    .map(|path_id| EarlyReflectionTap {
                        path_id: path_id as u16,
                        arrival_direction: if path_id == 0 { Vec3::X } else { Vec3::Z },
                        delay_seconds: 0.01 + path_id as f32 * 0.005,
                        gain: [0.15, 0.1, 0.05],
                    })
                    .collect(),
            })
            .collect();
        processor.replace_acoustic_response(Arc::new(AcousticResponse {
            spatial_revision: 1,
            geometry_version: 1,
            direct,
            late_reverb: LateReverbParameters::SILENT,
            published_at: Instant::now(),
            solve_time_us: 1,
        }));
        let acoustic_response = processor.acoustic_response.clone().unwrap();
        for (voice_id, _) in &sources {
            let voice_acoustics = acoustic_response.voice_response(*voice_id);
            processor.ensure_native_early_reflection_state_for_voice(*voice_id, voice_acoustics);
        }
        processor.cached_input_buf.fill(0.1);
        for _ in 0..32 {
            processor.cached_binaural_processed.fill(0.0);
            for (voice_id, _) in &sources {
                let voice_acoustics = acoustic_response.voice_response(*voice_id);
                processor
                    .apply_native_early_reflections(*voice_id, voice_acoustics, 1.0, true)
                    .unwrap();
            }
        }

        let started = Instant::now();
        for _ in 0..BLOCKS {
            processor.cached_binaural_processed.fill(0.0);
            for (voice_id, _) in black_box(&sources) {
                let voice_acoustics = acoustic_response.voice_response(*voice_id);
                processor
                    .apply_native_early_reflections(*voice_id, voice_acoustics, 1.0, true)
                    .unwrap();
            }
        }
        let elapsed = started.elapsed();
        let audio_seconds = FRAMES as f64 * BLOCKS as f64 / SAMPLE_RATE as f64;
        println!(
            "active early reflections: sources={} taps_per_source={} blocks={BLOCKS} frames={FRAMES} elapsed_ms={:.3} us_per_block={:.3} realtime_cpu_percent={:.3}",
            sources.len(),
            MAX_EARLY_REFLECTION_TAPS,
            elapsed.as_secs_f64() * 1_000.0,
            elapsed.as_secs_f64() * 1_000_000.0 / BLOCKS as f64,
            elapsed.as_secs_f64() / audio_seconds * 100.0,
        );
        assert!(
            processor
                .cached_binaural_processed
                .iter()
                .all(|sample| sample.is_finite())
        );
    }

    #[test]
    #[ignore = "release-mode performance probe"]
    fn split_direct_environment_routing_release_budget() {
        use std::hint::black_box;

        const SAMPLE_RATE: u32 = 48_000;
        const FRAMES: usize = 1_024;
        const VOICES: usize = 8;
        const BLOCKS: usize = 1_000;
        let voice_ids: Vec<_> = (1..=VOICES)
            .map(|voice| VoiceId::from(voice as u64))
            .collect();
        let audio = Arc::new(PetalSonicAudioData::new(
            vec![0.1 / VOICES as f32; FRAMES],
            SAMPLE_RATE,
            1,
            Duration::from_secs_f64(FRAMES as f64 / SAMPLE_RATE as f64),
        ));
        let mut voices = voice_ids
            .iter()
            .enumerate()
            .map(|(index, voice_id)| {
                let mut voice = prepare_test_voice(
                    *voice_id,
                    Emitter {
                        world_id: 1,
                        index: index as u32,
                        generation: 1,
                    },
                    audio.clone(),
                    EmitterDesc::spatial(Pose::from_position(Vec3::Z)),
                    PlayOptions::looping()
                        .with_direct_path(
                            DirectPath::listener_relative(Pose::from_position(Vec3::new(
                                index as f32 * 0.02 - 0.07,
                                -0.08,
                                0.2,
                            )))
                            .with_geometry(DirectGeometry::BypassTransmission),
                        )
                        .with_environment_send(EnvironmentSend::from_world_pose(
                            Pose::from_position(Vec3::new(index as f32 - 3.5, 0.0, 4.0)),
                        )),
                    None,
                    0,
                    FRAMES,
                )
                .start()
                .1;
                voice.set_mix_parameters(crate::domain::BusParams::default());
                (*voice_id, voice)
            })
            .collect::<HashMap<_, _>>();
        let mut processor = SpatialProcessor::new(SpatialProcessorConfig {
            sample_rate: SAMPLE_RATE,
            frame_size: FRAMES,
            max_voices: VOICES,
            distance_scaler: 1.0,
            native_hrtf_path: None,
            hrtf_gain: 0.0,
            use_ambisonics: false,
            environmental_acoustics_enabled: Arc::new(AtomicBool::new(true)),
        })
        .unwrap();
        processor.replace_acoustic_response(Arc::new(AcousticResponse {
            spatial_revision: 1,
            geometry_version: 1,
            direct: voice_ids
                .iter()
                .map(|voice_id| DirectAcousticResponse {
                    voice_id: *voice_id,
                    routing_generation: 0,
                    gain: [1.0; 3],
                    environment_gain: [1.0; 3],
                    direct_lobes: Vec::new(),
                    environment_representatives: Vec::new(),
                    solve_status: crate::acoustic_propagation::DirectSolveStatus::Solved,
                    cache_age_seconds: 0.0,
                    early_reflections: vec![EarlyReflectionTap {
                        path_id: 1,
                        arrival_direction: Vec3::Z,
                        delay_seconds: 0.01,
                        gain: [0.1, 0.08, 0.05],
                    }],
                })
                .collect(),
            late_reverb: LateReverbParameters {
                pre_delay_seconds: 0.02,
                rt60_seconds: [0.8, 1.2, 0.9],
                wet_gain: 0.2,
            },
            published_at: Instant::now(),
            solve_time_us: 1,
        }));
        let mut output = vec![0.0; FRAMES * 2];
        let mut events = Vec::with_capacity(VOICES * 2);
        for _ in 0..32 {
            output.fill(0.0);
            processor
                .process_spatial_sources_with_metrics(
                    &voice_ids,
                    &mut voices,
                    &mut output,
                    SpatialRenderContext::default(),
                    &mut events,
                )
                .unwrap();
        }

        let started = Instant::now();
        for _ in 0..BLOCKS {
            output.fill(0.0);
            processor
                .process_spatial_sources_with_metrics(
                    black_box(&voice_ids),
                    black_box(&mut voices),
                    black_box(&mut output),
                    SpatialRenderContext::default(),
                    black_box(&mut events),
                )
                .unwrap();
        }
        let elapsed = started.elapsed();
        let audio_seconds = FRAMES as f64 * BLOCKS as f64 / SAMPLE_RATE as f64;
        println!(
            "split direct/environment routing: voices={VOICES} blocks={BLOCKS} frames={FRAMES} elapsed_ms={:.3} us_per_block={:.3} realtime_cpu_percent={:.3}",
            elapsed.as_secs_f64() * 1_000.0,
            elapsed.as_secs_f64() * 1_000_000.0 / BLOCKS as f64,
            elapsed.as_secs_f64() / audio_seconds * 100.0,
        );
        assert!(output.iter().all(|sample| sample.is_finite()));
    }

    #[test]
    #[ignore = "release-mode extended-source renderer performance probe"]
    fn extended_source_direction_field_release_budget() {
        measure_extended_source_direction_field(8);
    }

    #[test]
    #[ignore = "release-mode extended source performance acceptance"]
    fn sixteen_sample_direction_field_release_budget() {
        measure_extended_source_direction_field(16);
    }

    fn measure_extended_source_direction_field(sample_count: usize) {
        use crate::domain::{
            DistributedOcclusionProfile, ExtentSample, ExtentSampleId, SourceExtent,
        };
        use std::hint::black_box;

        const SAMPLE_RATE: u32 = 48_000;
        const FRAMES: usize = 1_024;
        const VOICES: usize = 8;
        const BLOCKS: usize = 1_000;
        let voice_ids = (1..=VOICES)
            .map(|voice| VoiceId::from(voice as u64))
            .collect::<Vec<_>>();
        let extent = SourceExtent::weighted_samples_with_limit(
            (0..sample_count)
                .map(|id| {
                    let angle = id as f32 * std::f32::consts::TAU / sample_count as f32;
                    ExtentSample::new(
                        ExtentSampleId(id as u64),
                        Vec3::new(angle.cos(), angle.sin() * 0.5, angle.sin()),
                        1.0,
                    )
                    .unwrap()
                })
                .collect(),
            sample_count,
        )
        .unwrap();
        let profile = OcclusionProfile::AmbientDistributed(
            DistributedOcclusionProfile::default()
                .with_lobe_count(3)
                .unwrap(),
        );
        let audio = Arc::new(PetalSonicAudioData::new(
            vec![0.1 / VOICES as f32; FRAMES],
            SAMPLE_RATE,
            1,
            Duration::from_secs_f64(FRAMES as f64 / SAMPLE_RATE as f64),
        ));
        let mut voices = voice_ids
            .iter()
            .enumerate()
            .map(|(index, voice_id)| {
                let mut voice = prepare_test_voice(
                    *voice_id,
                    Emitter {
                        world_id: 1,
                        index: index as u32,
                        generation: 1,
                    },
                    audio.clone(),
                    EmitterDesc::spatial(Pose::from_position(Vec3::new(
                        index as f32 - 3.5,
                        0.0,
                        4.0,
                    )))
                    .with_extent(extent.clone())
                    .with_occlusion_profile(profile),
                    PlayOptions::looping(),
                    None,
                    0,
                    FRAMES,
                )
                .start()
                .1;
                voice.set_mix_parameters(crate::domain::BusParams::default());
                (*voice_id, voice)
            })
            .collect::<HashMap<_, _>>();
        let lobe_gain = (1.0_f32 / 3.0).sqrt();
        let mut processor = SpatialProcessor::new(SpatialProcessorConfig {
            sample_rate: SAMPLE_RATE,
            frame_size: FRAMES,
            max_voices: VOICES,
            distance_scaler: 1.0,
            native_hrtf_path: None,
            hrtf_gain: 0.0,
            use_ambisonics: false,
            environmental_acoustics_enabled: Arc::new(AtomicBool::new(true)),
        })
        .unwrap();
        processor.replace_acoustic_response(Arc::new(AcousticResponse {
            spatial_revision: 1,
            geometry_version: 1,
            direct: voice_ids
                .iter()
                .map(|voice_id| DirectAcousticResponse {
                    voice_id: *voice_id,
                    routing_generation: 0,
                    gain: [1.0; 3],
                    environment_gain: [0.8, 0.7, 0.6],
                    direct_lobes: vec![
                        DirectLobeTarget {
                            lobe_id: 0,
                            direction: Vec3::X,
                            gain: [lobe_gain; 3],
                            power: 1.0 / 3.0,
                        },
                        DirectLobeTarget {
                            lobe_id: 1,
                            direction: Vec3::Z,
                            gain: [lobe_gain; 3],
                            power: 1.0 / 3.0,
                        },
                        DirectLobeTarget {
                            lobe_id: 2,
                            direction: -Vec3::X,
                            gain: [lobe_gain; 3],
                            power: 1.0 / 3.0,
                        },
                    ],
                    environment_representatives: Vec::new(),
                    solve_status: crate::acoustic_propagation::DirectSolveStatus::Solved,
                    cache_age_seconds: 0.0,
                    early_reflections: Vec::new(),
                })
                .collect(),
            late_reverb: LateReverbParameters {
                pre_delay_seconds: 0.02,
                rt60_seconds: [0.8, 1.2, 0.9],
                wet_gain: 0.2,
            },
            published_at: Instant::now(),
            solve_time_us: 1,
        }));
        let mut output = vec![0.0; FRAMES * 2];
        let mut events = Vec::with_capacity(VOICES * 2);
        for _ in 0..32 {
            output.fill(0.0);
            processor
                .process_spatial_sources_with_metrics(
                    &voice_ids,
                    &mut voices,
                    &mut output,
                    SpatialRenderContext::default(),
                    &mut events,
                )
                .unwrap();
        }

        let mut elapsed_us = Vec::with_capacity(BLOCKS);
        for _ in 0..BLOCKS {
            output.fill(0.0);
            let started = Instant::now();
            processor
                .process_spatial_sources_with_metrics(
                    black_box(&voice_ids),
                    black_box(&mut voices),
                    black_box(&mut output),
                    SpatialRenderContext::default(),
                    black_box(&mut events),
                )
                .unwrap();
            elapsed_us.push(started.elapsed().as_micros() as u64);
        }
        elapsed_us.sort_unstable();
        let audio_seconds = FRAMES as f64 * BLOCKS as f64 / SAMPLE_RATE as f64;
        let elapsed_seconds = elapsed_us.iter().sum::<u64>() as f64 / 1_000_000.0;
        println!(
            "PETALSONIC_EXTENDED_RENDER_METRICS {{\"voices\":{VOICES},\"samples_per_extent\":{sample_count},\"lobes_per_voice\":3,\"blocks\":{BLOCKS},\"frames\":{FRAMES},\"p50_us\":{},\"p95_us\":{},\"p99_us\":{},\"max_us\":{},\"realtime_cpu_percent\":{}}}",
            elapsed_us[BLOCKS / 2],
            elapsed_us[BLOCKS * 95 / 100],
            elapsed_us[BLOCKS * 99 / 100],
            elapsed_us[BLOCKS - 1],
            elapsed_seconds / audio_seconds * 100.0,
        );
        assert!(output.iter().all(|sample| sample.is_finite()));
    }
}
