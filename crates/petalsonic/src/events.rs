//! Event types for PetalSonic

use crate::config::{LatencyProfile, SpatialQuality};
use crate::domain::{Emitter, ExtentSampleId, PlayCommandId, PlaybackControl, PlaybackTag};
use crate::math::Pose;
use crate::math::Vec3;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

/// Observable state of the world-owned output runtime.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeState {
    Running = 0,
    Recovering = 1,
    Failed = 2,
    Closing = 3,
    Closed = 4,
}

impl RuntimeState {
    pub(crate) fn load(state: &std::sync::atomic::AtomicU8) -> Self {
        Self::from_storage(state.load(Ordering::Acquire))
    }

    pub(crate) fn from_storage(value: u8) -> Self {
        match value {
            value if value == Self::Running as u8 => Self::Running,
            value if value == Self::Recovering as u8 => Self::Recovering,
            value if value == Self::Failed as u8 => Self::Failed,
            value if value == Self::Closing as u8 => Self::Closing,
            _ => Self::Closed,
        }
    }
}

/// Pull-based health snapshot. Device names are diagnostic, not stable identifiers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeStatus {
    pub state: RuntimeState,
    pub recovery_attempts: u64,
    pub active_output_device: Option<String>,
}

/// Cumulative, allocation-free runtime health counters captured at one instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeDiagnostics {
    pub frames_processed: usize,
    pub underrun_count: usize,
    pub active_emitters: usize,
    pub active_voices: usize,
    pub control_queue_depth: usize,
    pub control_queue_high_water: usize,
    pub lifecycle_queue_depth: usize,
    pub lifecycle_queue_high_water: usize,
    pub event_queue_depth: usize,
    pub event_queue_high_water: usize,
    pub timing_queue_depth: usize,
    pub timing_queue_high_water: usize,
    pub rejected_commands: u64,
    pub dropped_events: u64,
    pub dropped_timing_events: u64,
    pub render_iterations: u64,
    pub render_time_p50_us: u64,
    pub render_time_p95_us: u64,
    pub render_time_p99_us: u64,
    pub render_time_max_us: u64,
    pub acoustic_solve_count: u64,
    pub acoustic_superseded_solve_count: u64,
    pub acoustic_published_response_count: u64,
    pub acoustic_response_spatial_revision: u64,
    pub acoustic_response_geometry_version: u64,
    pub acoustic_last_solve_time_us: u64,
    pub acoustic_solve_time_p50_us: u64,
    pub acoustic_solve_time_p95_us: u64,
    pub acoustic_solve_time_p99_us: u64,
    pub acoustic_solve_time_max_us: u64,
    pub acoustic_response_age_ms: u64,
    pub acoustic_direct_ray_count: u64,
    pub acoustic_sample_cache_hit_count: u64,
    pub acoustic_processed_extent_count: u64,
    pub acoustic_lobe_count: u64,
    pub acoustic_retained_response_count: u64,
    pub acoustic_deferred_response_count: u64,
    pub acoustic_render_rejected_response_count: u64,
    pub device_generation: u64,
    pub recovery_attempts: u64,
    pub output_sample_rate: u32,
    pub output_channels: u16,
    pub spatial_quality: SpatialQuality,
    pub latency_profile: LatencyProfile,
}

pub(crate) struct RuntimeCounters {
    pub(crate) control_queue_high_water: AtomicUsize,
    pub(crate) lifecycle_queue_high_water: AtomicUsize,
    pub(crate) event_queue_high_water: AtomicUsize,
    pub(crate) voice_telemetry_queue_high_water: AtomicUsize,
    pub(crate) timing_queue_high_water: AtomicUsize,
    pub(crate) rejected_commands: AtomicU64,
    pub(crate) dropped_events: AtomicU64,
    pub(crate) dropped_voice_telemetry: AtomicU64,
    pub(crate) dropped_timing_events: AtomicU64,
    pub(crate) acoustic_render_rejected_responses: AtomicU64,
    pub(crate) device_generation: AtomicU64,
    pub(crate) output_sample_rate: AtomicUsize,
    pub(crate) output_channels: AtomicUsize,
    #[cfg(test)]
    pub(crate) rendered_spatial_revision: AtomicU64,
    render_time_max_us: AtomicU64,
    render_histogram: [AtomicU64; 64],
}

impl Default for RuntimeCounters {
    fn default() -> Self {
        Self {
            control_queue_high_water: AtomicUsize::new(0),
            lifecycle_queue_high_water: AtomicUsize::new(0),
            event_queue_high_water: AtomicUsize::new(0),
            voice_telemetry_queue_high_water: AtomicUsize::new(0),
            timing_queue_high_water: AtomicUsize::new(0),
            rejected_commands: AtomicU64::new(0),
            dropped_events: AtomicU64::new(0),
            dropped_voice_telemetry: AtomicU64::new(0),
            dropped_timing_events: AtomicU64::new(0),
            acoustic_render_rejected_responses: AtomicU64::new(0),
            device_generation: AtomicU64::new(0),
            output_sample_rate: AtomicUsize::new(0),
            output_channels: AtomicUsize::new(0),
            #[cfg(test)]
            rendered_spatial_revision: AtomicU64::new(0),
            render_time_max_us: AtomicU64::new(0),
            render_histogram: std::array::from_fn(|_| AtomicU64::new(0)),
        }
    }
}

impl RuntimeCounters {
    pub(crate) fn observe_high_water(counter: &AtomicUsize, depth: usize) {
        let mut current = counter.load(Ordering::Relaxed);
        while depth > current {
            match counter.compare_exchange_weak(
                current,
                depth,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(observed) => current = observed,
            }
        }
    }

    pub(crate) fn record_render_time(&self, elapsed_us: u64) {
        self.render_time_max_us
            .fetch_max(elapsed_us, Ordering::Relaxed);
        let bucket = if elapsed_us == 0 {
            0
        } else {
            (u64::BITS - (elapsed_us - 1).leading_zeros()) as usize + 1
        }
        .min(self.render_histogram.len() - 1);
        self.render_histogram[bucket].fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn render_summary(&self) -> (u64, u64, u64, u64, u64) {
        let total = self
            .render_histogram
            .iter()
            .map(|bucket| bucket.load(Ordering::Relaxed))
            .sum();
        (
            total,
            self.percentile(total, 50),
            self.percentile(total, 95),
            self.percentile(total, 99),
            self.render_time_max_us.load(Ordering::Relaxed),
        )
    }

    fn percentile(&self, total: u64, percentile: u64) -> u64 {
        if total == 0 {
            return 0;
        }
        let target = total.saturating_mul(percentile).div_ceil(100);
        let mut cumulative = 0;
        for (index, bucket) in self.render_histogram.iter().enumerate() {
            cumulative += bucket.load(Ordering::Relaxed);
            if cumulative >= target {
                return match index {
                    0 => 0,
                    63 => u64::MAX,
                    _ => 1u64 << (index - 1),
                };
            }
        }
        self.render_time_max_us.load(Ordering::Relaxed)
    }
}

/// Timing information for a single render iteration
/// Used for performance profiling and stress testing
#[derive(Debug, Clone, Copy)]
pub struct RenderTimingEvent {
    /// Time spent mixing audio sources (microseconds)
    pub mixing_time_us: u64,
    /// Time spent on spatial processing (microseconds)
    pub spatial_time_us: u64,
    /// Time spent mixing non-spatial (direct) sources (microseconds)
    pub direct_mixing_time_us: u64,
    /// Number of spatial sources processed.
    pub spatial_source_count: usize,
    /// Time spent running the spatial physics simulation step (microseconds)
    pub spatial_simulation_time_us: u64,
    /// Time spent applying direct-path processing (microseconds)
    pub direct_processing_time_us: u64,
    /// Time spent encoding sources into the ambisonics field (microseconds)
    pub ambisonics_encoding_time_us: u64,
    /// Time spent decoding ambisonics back to listener channels (microseconds)
    pub ambisonics_decoding_time_us: u64,
    /// Time spent rendering HRTF/binaural output (microseconds)
    pub hrtf_rendering_time_us: u64,
    /// Time spent rendering the shared late-reverb bus (microseconds)
    pub late_reverb_time_us: u64,
    /// Time spent filtering, delaying, and spatializing early reflections (microseconds)
    pub early_reflection_time_us: u64,
    /// Time spent selecting native HRTF directions (microseconds)
    pub native_hrtf_direction_lookup_time_us: u64,
    /// Time spent in native HRTF FIR convolution (microseconds)
    pub native_hrtf_convolution_time_us: u64,
    /// Time spent on resampling (microseconds)
    pub resampling_time_us: u64,
    /// Total time for the entire render iteration (microseconds)
    pub total_time_us: u64,
}

/// The asynchronous environment response observed by a render block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnvironmentResponse {
    /// Complete spatial input generation used by the solver.
    pub spatial_revision: u64,
    /// Immutable acoustic geometry generation used by the solver.
    pub geometry_version: u64,
    /// Elapsed time between publication by the solver and observation by the render block.
    pub age: Duration,
}

/// Opt-in telemetry for the first render block that advances one Voice's PCM cursor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoiceFirstRenderTelemetry {
    /// Caller-supplied correlation value from [`crate::PlayOptions`].
    pub play_command_id: PlayCommandId,
    /// Reusable emitter whose immutable playback Voice produced this event.
    pub emitter: Emitter,
    /// Monotonic world render-block index, starting at zero for each output session.
    pub render_block_index: u64,
    /// Latest complete spatial frame observed at the render-quantum boundary.
    pub spatial_revision: u64,
    /// Direct placement in listener-local coordinates, or `None` when direct is disabled.
    pub direct_local_pose: Option<Pose>,
    /// World-space origin captured for this environment send, or `None` when disabled.
    pub acoustic_origin: Option<Pose>,
    /// Environment response already available for this Voice on its first render block.
    pub environment_response: Option<EnvironmentResponse>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PetalSonicEvent {
    /// A controlled one-shot reached its natural end.
    PlaybackCompleted {
        emitter: Emitter,
        control: PlaybackControl,
        tag: PlaybackTag,
    },
    /// The output runtime entered a different lifecycle state.
    RuntimeStateChanged(RuntimeState),
}

/// Opt-in per-Voice spatial telemetry, kept separate from lifecycle events for source
/// compatibility and independent bounded consumption.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VoiceTelemetryEvent {
    /// An opted-in spatial Voice advanced its PCM cursor for the first time.
    FirstRendered(VoiceFirstRenderTelemetry),
    /// The opted-in Voice first received an asynchronous environment response.
    EnvironmentResponse {
        play_command_id: PlayCommandId,
        response: EnvironmentResponse,
    },
    /// Final processing-stage energy totals for one opted-in Voice, emitted after its bounded
    /// per-Voice early-reflection tail has drained.
    EnergySummary(VoiceEnergyTelemetry),
    /// Initial transmission was unavailable beyond the bounded acoustic-start wait.
    /// The opted-in Voice was reclaimed without advancing PCM or reporting completion.
    AcousticStartTimedOut {
        play_command_id: PlayCommandId,
        emitter: Emitter,
    },
}

/// Sum-of-squares energy accumulated at the named stages of one immutable PCM Voice.
///
/// These values are diagnostic processing-stage energy, not loudness or independently mixable
/// audio. Direct and early-reflection energy sum the bounded distributed route contributions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoiceEnergyTelemetry {
    pub play_command_id: PlayCommandId,
    pub emitter: Emitter,
    pub source_energy: f64,
    pub direct_energy: f64,
    pub environment_send_energy: f64,
    pub early_reflection_energy: f64,
    /// Global shared late-reverb state observed when this Voice's summary was emitted.
    pub late_reverb: LateReverbTelemetry,
}

/// Cumulative global late-reverb diagnostics captured without blocking the render thread.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LateReverbTelemetry {
    pub pre_delay_seconds: f32,
    pub rt60_seconds: [f32; 3],
    pub wet_gain: f32,
    pub cumulative_input_energy: f64,
    pub cumulative_output_energy: f64,
}

/// Current pressure on the independently bounded per-Voice telemetry queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoiceTelemetryDiagnostics {
    /// Events currently waiting for the caller.
    pub queue_depth: usize,
    /// Maximum observed queue depth since World creation.
    pub queue_high_water: usize,
    /// Events rejected because the queue was full or disconnected.
    pub dropped_events: u64,
}

/// Publication state for one Voice in an asynchronous direct-acoustics solve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcousticSolveStatus {
    Solved,
    Retained,
    Deferred,
}

/// Explicit worker conclusion for one direct or environment route in a solve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcousticRouteOutcome {
    /// The route was admitted to this solve and its response was applied.
    Applied,
    /// The route was active but its Voice was outside the bounded solve budget.
    ExcludedByBudget,
    /// The immutable Voice routing disabled this worker route.
    Disabled,
}

/// Stable Schmitt-classified state of one direct or environment route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcousticOcclusionState {
    Visible,
    Occluded,
}

/// Per-route measurements captured entirely on the acoustics worker.
#[derive(Debug, Clone, PartialEq)]
pub struct AcousticRouteTelemetry {
    /// Number of observations in `samples`; active routes contain the complete captured extent,
    /// inactive routes contain 0. The producer chooses the extent's sample budget.
    pub sample_count: usize,
    /// Stable-ID-ordered observations that produced `raw_gain`, `hit_count`, and `visible_fraction`.
    pub samples: Vec<AcousticSampleObservation>,
    pub ray_count: usize,
    pub cache_hit_count: usize,
    pub hit_count: usize,
    pub visible_fraction: f32,
    pub raw_gain: [f32; 3],
    pub filtered_gain: [f32; 3],
    pub classified_state: AcousticOcclusionState,
    pub dwell_seconds: f32,
}

/// One bounded physical observation used in a direct or environment route solve.
///
/// A valid closest hit reports `hit == true` and the sanitized low/mid/high linear-amplitude
/// transmission copied from its [`crate::AcousticMaterial`]. A miss reports `hit == false` and
/// unity transmission; there is no material to name or infer. Cache reuse republishes this exact
/// observation even though it executes no new ray.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AcousticSampleObservation {
    /// Caller-stable weighted-sample identity, or [`ExtentSampleId::POINT`] for a Point extent.
    pub sample_id: ExtentSampleId,
    /// Normalized source-power fraction. Active route observations sum to one.
    pub normalized_power_weight: f32,
    /// World-space sample position used for this immutable route response.
    pub world_position: Vec3,
    /// Whether the route ray produced a valid closest material hit.
    pub hit: bool,
    /// Sanitized low/mid/high material transmission, as linear amplitude in `0..=1`.
    pub transmission: [f32; 3],
}

/// One bounded renderer lobe derived deterministically from stable extent samples.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AcousticLobeTelemetry {
    pub lobe_id: u8,
    pub direction: Vec3,
    pub gain: [f32; 3],
    pub power: f32,
}

/// Worker-side extended-source measurements for one immutable Voice route.
#[derive(Debug, Clone, PartialEq)]
pub struct AcousticExtentTelemetry {
    pub voice_id: u64,
    pub emitter: Emitter,
    pub spatial_revision: u64,
    pub geometry_version: u64,
    /// Spatial revision that actually produced the response (older while retained).
    pub response_spatial_revision: u64,
    /// Geometry version that actually produced the response (older while retained).
    pub response_geometry_version: u64,
    pub extent_sample_count: usize,
    pub direct: AcousticRouteTelemetry,
    pub environment: AcousticRouteTelemetry,
    pub lobes: Vec<AcousticLobeTelemetry>,
    pub solve_status: AcousticSolveStatus,
    pub cache_age_seconds: f32,
    pub budget_member: bool,
}

/// Bounded QoS and response summary for every immutable Voice route captured by one solve.
///
/// This is a separate event from [`AcousticExtentTelemetry`] so extending opt-in diagnostics does
/// not change the construction contract of the existing public telemetry record.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AcousticVoiceConclusionTelemetry {
    pub voice_id: u64,
    pub emitter: Emitter,
    pub spatial_revision: u64,
    pub geometry_version: u64,
    /// One-based deterministic candidate rank, or `None` when neither route was a valid candidate.
    pub candidate_rank: Option<usize>,
    /// Maximum number of Voice candidates admitted by the source-count budget.
    pub candidate_limit: usize,
    pub direct: AcousticRouteOutcome,
    pub environment: AcousticRouteOutcome,
    /// Filtered low/mid/high transmission applied to the environment-send route.
    pub environment_transmission_gain: [f32; 3],
    /// Number of bounded early-reflection taps attached to the published Voice response.
    pub early_tap_count: usize,
    /// Existing retained/deferred semantics for an active candidate, or `None` if fully disabled.
    pub solve_status: Option<AcousticSolveStatus>,
}

/// Why a completed worker result was deliberately not published.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcousticDiscardReason {
    Superseded,
}

/// Independently bounded acoustics-worker telemetry; never mixed with lifecycle events.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub enum AcousticTelemetryEvent {
    ExtentResponse(Box<AcousticExtentTelemetry>),
    /// Explicit admission and route conclusion for one captured Voice.
    VoiceConclusion(AcousticVoiceConclusionTelemetry),
    SolveDiscarded {
        spatial_revision: u64,
        geometry_version: u64,
        reason: AcousticDiscardReason,
    },
}

/// Current pressure on the independently bounded acoustics telemetry queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcousticTelemetryDiagnostics {
    pub queue_depth: usize,
    pub queue_high_water: usize,
    pub dropped_events: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_histogram_reports_bounded_cumulative_percentiles() {
        let counters = RuntimeCounters::default();
        for elapsed in [1, 2, 3, 4, 100] {
            counters.record_render_time(elapsed);
        }

        let (count, p50, p95, p99, max) = counters.render_summary();
        assert_eq!(count, 5);
        assert_eq!(p50, 4);
        assert_eq!(p95, 128);
        assert_eq!(p99, 128);
        assert_eq!(max, 100);
    }
}
