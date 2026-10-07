use crate::acoustics::AcousticSceneSnapshot;
use crate::domain::BusDesc;

/// Effect-oriented spatial quality selected once for the world lifetime.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SpatialQuality {
    /// Prioritizes low processing cost and direct binaural feedback.
    LowLatency,
    /// Balances spatial detail, source count, and render cost.
    #[default]
    Balanced,
    /// Enables the most detailed fixed processing plan available.
    HighQuality,
}

/// Constrained render-ahead policy. Callers do not specify raw device periods.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LatencyProfile {
    Responsive,
    #[default]
    Balanced,
    Robust,
}

/// Output-device selection policy.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum OutputDevicePolicy {
    /// Follow the operating system's current default output and recover across changes.
    #[default]
    FollowSystemDefault,
    /// Stay attached to the matching named device and reconnect when it returns.
    PinnedNameContains(String),
}

/// Hard upper bounds for each asynchronous direct-acoustics solve.
///
/// Spatial quality may choose lower limits, but never exceeds these values. One direct or
/// environment route through one extent sample consumes one direct ray.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnvironmentalAcousticsBudget {
    /// Maximum Point or extended sources selected for one solve.
    pub max_processed_extents: usize,
    /// Maximum direct/environment visibility rays across the selected extents.
    pub max_direct_rays: usize,
}

impl Default for EnvironmentalAcousticsBudget {
    fn default() -> Self {
        Self {
            max_processed_extents: 64,
            max_direct_rays: 1_024,
        }
    }
}

/// Configuration descriptor for a PetalSonic world
#[derive(Clone)]
pub struct PetalSonicWorldDesc {
    /// Sample rate for the world processing (may differ from device sample rate)
    pub sample_rate: u32,
    /// Block size in world sample rate (number of frames to generate per audio processing chunk).
    ///
    /// This is the fixed number of frames generated at the world's sample rate, which are then
    /// resampled to the device's sample rate (producing variable output based on the ratio).
    pub block_size: usize,
    /// Maximum number of long-lived emitters.
    pub max_emitters: usize,
    /// Maximum number of simultaneous playback voices.
    pub max_voices: usize,
    /// Maximum number of declared buses, excluding the implicit Master bus.
    pub max_buses: usize,
    /// Stable buses created with the world. Every bus feeds Master directly.
    pub buses: Vec<BusDesc>,
    /// Capacity of the bounded regular control queue.
    pub control_queue_capacity: usize,
    /// Reserved capacity for stop and destruction commands.
    ///
    /// This queue is independent from regular control traffic so overload cannot
    /// consume the capacity needed to retire voices and emitters.
    pub lifecycle_queue_capacity: usize,
    /// Capacity of each bounded pull-event queue (lifecycle, Voice, and acoustics telemetry).
    pub event_queue_capacity: usize,
    /// Capacity of the bounded timing/diagnostics queue.
    pub timing_queue_capacity: usize,
    pub output_device: OutputDevicePolicy,
    pub spatial_quality: SpatialQuality,
    pub latency_profile: LatencyProfile,
    /// Optional custom `.petalhrtf` path. The embedded native HRTF is used when absent.
    pub native_hrtf_path: Option<String>,
    /// HRTF gain compensation in decibels (default: 0.0 dB = no change)
    ///
    /// Different HRTF datasets can have different overall gain levels.
    /// Use this to compensate for volume differences between HRTFs.
    ///
    /// # Examples
    /// - `0.0`: No change (unity gain)
    /// - `6.0`: Approximately double the perceived loudness (+6 dB)
    /// - `-6.0`: Approximately half the perceived loudness (-6 dB)
    /// - `3.0`: Modest increase (~40% louder)
    /// - `-20.0`: Very quiet (1/10th perceived loudness)
    pub hrtf_gain: f32,
    /// Distance scale factor to convert world units to meters for spatialization.
    ///
    /// This factor controls how your application's coordinate system maps to real-world meters
    /// when running the native spatial simulation.
    ///
    /// - `1.0`: 1 world unit = 1 meter
    /// - `10.0`: 1 world unit = 10 meters (larger-scale worlds)
    pub distance_scaler: f32,
    /// Enables geometry-driven occlusion, reflections, and reverberation.
    ///
    /// Native HRTF spatialization, distance attenuation, air absorption, and playback stay
    /// active when this is disabled. The setting can also be changed while the world is running.
    pub environmental_acoustics_enabled: bool,
    /// Normalized geometry-driven acoustics quality in the inclusive range `0.0..=1.0`.
    ///
    /// The default `0.5` preserves the balanced native propagation plan. The value controls a
    /// bounded internal ray budget and can also be changed while the world is running without
    /// rebuilding the output runtime.
    pub environmental_acoustics_quality: f32,
    /// Hard caps applied after the normalized quality plan.
    pub environmental_acoustics_budget: EnvironmentalAcousticsBudget,
    /// Optional immutable acoustic scene available when the runtime starts.
    pub acoustic_scene: Option<AcousticSceneSnapshot>,
}

impl Default for PetalSonicWorldDesc {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            block_size: 1024,
            max_emitters: 2048,
            max_voices: 4096,
            max_buses: 8,
            buses: ["Gameplay", "Music", "UI", "Voice"]
                .into_iter()
                .map(BusDesc::new)
                .collect(),
            control_queue_capacity: 4096,
            lifecycle_queue_capacity: 256,
            event_queue_capacity: 1024,
            timing_queue_capacity: 512,
            output_device: OutputDevicePolicy::default(),
            spatial_quality: SpatialQuality::default(),
            latency_profile: LatencyProfile::default(),
            native_hrtf_path: None,
            hrtf_gain: 0.0,
            distance_scaler: 10.0,
            environmental_acoustics_enabled: true,
            environmental_acoustics_quality: 0.5,
            environmental_acoustics_budget: EnvironmentalAcousticsBudget::default(),
            acoustic_scene: None,
        }
    }
}

impl std::fmt::Debug for PetalSonicWorldDesc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PetalSonicWorldDesc")
            .field("sample_rate", &self.sample_rate)
            .field("block_size", &self.block_size)
            .field("max_emitters", &self.max_emitters)
            .field("max_voices", &self.max_voices)
            .field("max_buses", &self.max_buses)
            .field("buses", &self.buses)
            .field("control_queue_capacity", &self.control_queue_capacity)
            .field("lifecycle_queue_capacity", &self.lifecycle_queue_capacity)
            .field("event_queue_capacity", &self.event_queue_capacity)
            .field("timing_queue_capacity", &self.timing_queue_capacity)
            .field("output_device", &self.output_device)
            .field("spatial_quality", &self.spatial_quality)
            .field("latency_profile", &self.latency_profile)
            .field("native_hrtf_path", &self.native_hrtf_path)
            .field("hrtf_gain", &self.hrtf_gain)
            .field("distance_scaler", &self.distance_scaler)
            .field(
                "environmental_acoustics_enabled",
                &self.environmental_acoustics_enabled,
            )
            .field(
                "environmental_acoustics_quality",
                &self.environmental_acoustics_quality,
            )
            .field(
                "environmental_acoustics_budget",
                &self.environmental_acoustics_budget,
            )
            .field("acoustic_scene", &self.acoustic_scene)
            .finish()
    }
}
