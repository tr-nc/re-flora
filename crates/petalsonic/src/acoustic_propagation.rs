use crate::acoustics::{AcousticMaterial, AcousticRay, AcousticSceneSnapshot};
use crate::config::EnvironmentalAcousticsBudget;
use crate::domain::VoiceId;
use crate::domain::{
    DirectGeometry, DirectPath, DirectPlacement, Emitter, EnvironmentOrigin, EnvironmentSend,
    MAX_DIRECT_LOBES, MAX_EXTENT_SAMPLES, OcclusionProfile, SourceExtent, SpatialFrame,
};
use crate::events::{
    AcousticDiscardReason, AcousticExtentTelemetry, AcousticLobeTelemetry, AcousticOcclusionState,
    AcousticRouteOutcome, AcousticRouteTelemetry, AcousticSampleObservation, AcousticSolveStatus,
    AcousticTelemetryEvent, AcousticVoiceConclusionTelemetry,
    EnvironmentResponse as EnvironmentResponseTelemetry,
};
use crate::math::{Pose, Vec3};
use crate::realtime_latest::{Publisher, RealtimeConsumer, RealtimeLatest};
use crate::runtime_services::{
    ChildCancellation, ChildStartup, RuntimeChildFailure, RuntimeChildResult, RuntimeServices,
};
use crate::spatial::LateReverbParameters;
use crossbeam_channel::{Receiver, Sender};
use std::cmp::Ordering as CmpOrdering;
use std::collections::{HashMap, HashSet};
#[cfg(test)]
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, TryLockError};
use std::time::{Duration, Instant};

const SPEED_OF_SOUND_METERS_PER_SECOND: f32 = 343.0;
const SOLVE_INTERVAL: Duration = Duration::from_millis(33);
pub(crate) const MAX_EARLY_REFLECTION_TAPS: usize = 2;
const EARLY_REFLECTION_MAX_DELAY_SECONDS: f32 = 0.25;
const EARLY_REFLECTION_GAIN: f32 = 0.6;
const MAX_TRACE_DISTANCE_METERS: f32 = 120.0;
const RAY_EPSILON_METERS: f32 = 0.05;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AcousticSolvePlan {
    max_direct_sources: usize,
    max_direct_rays: usize,
    max_early_reflection_sources: usize,
    early_reflection_taps: usize,
    early_reflection_ray_count: usize,
    late_ray_count: usize,
    late_bounce_count: usize,
}

impl AcousticSolvePlan {
    fn for_quality(quality: f32) -> Self {
        let quality = quality.clamp(0.0, 1.0);
        Self {
            max_direct_sources: quality_count(32, 32, 64, quality),
            max_direct_rays: quality_count(128, 256, 1_024, quality),
            // Keep the existing bounded crossfade pool; higher quality improves path sampling
            // without imposing more persistent render-thread state on lower settings.
            max_early_reflection_sources: quality_count(4, 8, 8, quality),
            early_reflection_taps: if quality < 0.25 { 1 } else { 2 },
            early_reflection_ray_count: quality_count(32, 64, 256, quality),
            late_ray_count: quality_count(128, 256, 1_024, quality),
            late_bounce_count: quality_count(4, 8, 12, quality),
        }
    }

    fn bounded_by(mut self, budget: EnvironmentalAcousticsBudget) -> Self {
        self.max_direct_sources = self.max_direct_sources.min(budget.max_processed_extents);
        self.max_direct_rays = self.max_direct_rays.min(budget.max_direct_rays);
        self
    }
}

fn quality_count(low: usize, balanced: usize, high: usize, quality: f32) -> usize {
    let (start, end, progress) = if quality <= 0.5 {
        (low, balanced, quality * 2.0)
    } else {
        (balanced, high, (quality - 0.5) * 2.0)
    };
    (start as f32 + (end as f32 - start as f32) * progress).round() as usize
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct EarlyReflectionTap {
    pub path_id: u16,
    pub arrival_direction: Vec3,
    pub delay_seconds: f32,
    pub gain: [f32; 3],
}

#[derive(Clone, Debug)]
pub(crate) struct DirectAcousticResponse {
    pub voice_id: VoiceId,
    pub(crate) routing_generation: u64,
    pub gain: [f32; 3],
    pub environment_gain: [f32; 3],
    pub direct_lobes: Vec<DirectLobeTarget>,
    pub(crate) environment_representatives: Vec<EnvironmentRepresentative>,
    pub early_reflections: Vec<EarlyReflectionTap>,
    pub solve_status: DirectSolveStatus,
    pub cache_age_seconds: f32,
}

impl DirectAcousticResponse {
    fn route_key(&self) -> VoiceRouteKey {
        VoiceRouteKey {
            voice_id: self.voice_id,
            routing_generation: self.routing_generation,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DirectSolveStatus {
    Solved,
    Retained,
    Deferred,
}

/// Immutable VoiceId-sorted render lookup table for one acoustic publication.
///
/// Construction owns ordering and uniqueness; rendering receives slice-like read access but
/// cannot mutate the index out from under binary lookup.
#[derive(Debug)]
pub(crate) struct AcousticVoiceResponses(Vec<DirectAcousticResponse>);

impl From<Vec<DirectAcousticResponse>> for AcousticVoiceResponses {
    fn from(mut responses: Vec<DirectAcousticResponse>) -> Self {
        responses.sort_by(|left, right| compare_voice_ids(left.voice_id, right.voice_id));
        assert!(
            responses.windows(2).all(|pair| {
                compare_voice_ids(pair[0].voice_id, pair[1].voice_id) == CmpOrdering::Less
            }),
            "one acoustic render publication cannot contain duplicate Voice identities"
        );
        Self(responses)
    }
}

impl FromIterator<DirectAcousticResponse> for AcousticVoiceResponses {
    fn from_iter<T: IntoIterator<Item = DirectAcousticResponse>>(iter: T) -> Self {
        iter.into_iter().collect::<Vec<_>>().into()
    }
}

impl std::ops::Deref for AcousticVoiceResponses {
    type Target = [DirectAcousticResponse];

    fn deref(&self) -> &Self::Target {
        self.0.as_slice()
    }
}

#[derive(Debug)]
pub(crate) struct AcousticResponse {
    pub spatial_revision: u64,
    pub geometry_version: u64,
    pub direct: AcousticVoiceResponses,
    pub late_reverb: LateReverbParameters,
    pub published_at: Instant,
    pub solve_time_us: u64,
}

/// One Voice's complete view of an immutable acoustic publication.
///
/// The renderer resolves this envelope once per Voice and quantum, then shares its behavior
/// across direct, environment, reflection, and telemetry processing without another search.
#[derive(Clone, Copy)]
pub(crate) struct VoiceAcousticResponse<'a> {
    frame: &'a AcousticResponse,
    voice: &'a DirectAcousticResponse,
}

impl<'a> VoiceAcousticResponse<'a> {
    #[cfg(test)]
    pub(crate) fn direct_gain(self) -> [f32; 3] {
        self.voice.gain
    }

    pub(crate) fn direct_gain_target(self) -> Option<[f32; 3]> {
        (self.voice.solve_status != DirectSolveStatus::Deferred).then_some(self.voice.gain)
    }

    #[cfg(test)]
    pub(crate) fn environment_gain(self) -> [f32; 3] {
        self.voice.environment_gain
    }

    pub(crate) fn environment_gain_target(self) -> Option<[f32; 3]> {
        (self.voice.solve_status != DirectSolveStatus::Deferred)
            .then_some(self.voice.environment_gain)
    }

    pub(crate) fn telemetry(self) -> EnvironmentResponseTelemetry {
        EnvironmentResponseTelemetry {
            spatial_revision: self.frame.spatial_revision,
            geometry_version: self.frame.geometry_version,
            age: self.frame.published_at.elapsed(),
        }
    }

    pub(crate) fn direct_lobes(self) -> Option<&'a [DirectLobeTarget]> {
        (self.voice.solve_status != DirectSolveStatus::Deferred)
            .then_some(self.voice.direct_lobes.as_slice())
    }

    pub(crate) fn early_reflections(self) -> &'a [EarlyReflectionTap] {
        self.voice.early_reflections.as_slice()
    }
}

impl AcousticResponse {
    pub(crate) fn voice_response(&self, voice_id: VoiceId) -> Option<VoiceAcousticResponse<'_>> {
        #[cfg(test)]
        crate::test_support::record_acoustic_response_lookup();
        self.direct
            .binary_search_by(|response| compare_voice_ids(response.voice_id, voice_id))
            .ok()
            .map(|index| &self.direct[index])
            .map(|voice| VoiceAcousticResponse { frame: self, voice })
    }
}

fn compare_voice_ids(left: VoiceId, right: VoiceId) -> CmpOrdering {
    #[cfg(test)]
    crate::test_support::record_acoustic_publication_comparison();
    left.value().cmp(&right.value())
}

fn compare_voice_routes(left: VoiceRouteKey, right: VoiceRouteKey) -> CmpOrdering {
    #[cfg(test)]
    crate::test_support::record_acoustic_publication_comparison();
    left.voice_id
        .value()
        .cmp(&right.voice_id.value())
        .then_with(|| left.routing_generation.cmp(&right.routing_generation))
}

/// Immutable routing and the latest compatible emitter state for one active Voice.
#[derive(Debug)]
pub(crate) struct AcousticVoice {
    pub voice_id: VoiceId,
    pub emitter: Emitter,
    pub emitter_world_pose: Pose,
    pub acoustic_priority: f32,
    pub audibility: f32,
    pub detached: bool,
    pub direct_path: DirectPath,
    pub environment_send: EnvironmentSend,
    pub source_extent: SourceExtent,
    pub occlusion_profile: OcclusionProfile,
    pub(crate) routing_generation: u64,
    #[cfg(test)]
    pub(crate) _retirement_witness: Option<AcousticVoiceRetirementWitness>,
}

#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct AcousticVoiceRetirementObservation {
    dropped_identity: AtomicU64,
    drop_count: AtomicUsize,
    realtime_drop_count: AtomicUsize,
}

#[cfg(test)]
impl AcousticVoiceRetirementObservation {
    pub(crate) fn observed(&self) -> (u64, usize, usize) {
        (
            self.dropped_identity.load(Ordering::Acquire),
            self.drop_count.load(Ordering::Acquire),
            self.realtime_drop_count.load(Ordering::Acquire),
        )
    }
}

#[cfg(test)]
#[derive(Debug)]
pub(crate) struct AcousticVoiceRetirementWitness {
    identity: u64,
    observation: Arc<AcousticVoiceRetirementObservation>,
}

#[cfg(test)]
impl AcousticVoiceRetirementWitness {
    pub(crate) fn new(identity: u64, observation: Arc<AcousticVoiceRetirementObservation>) -> Self {
        Self {
            identity,
            observation,
        }
    }
}

#[cfg(test)]
impl Drop for AcousticVoiceRetirementWitness {
    fn drop(&mut self) {
        self.observation
            .dropped_identity
            .store(self.identity, Ordering::Release);
        self.observation.drop_count.fetch_add(1, Ordering::AcqRel);
        if crate::test_support::realtime_probe_is_active() {
            self.observation
                .realtime_drop_count
                .fetch_add(1, Ordering::AcqRel);
        }
    }
}

impl AcousticVoice {
    fn snapshot(&self, spatial: Option<&SpatialFrame>) -> AcousticVoiceSnapshot {
        let mut snapshot = AcousticVoiceSnapshot {
            voice_id: self.voice_id,
            emitter: self.emitter,
            emitter_world_pose: self.emitter_world_pose,
            acoustic_priority: self.acoustic_priority,
            audibility: self.audibility,
            detached: self.detached,
            direct_path: self.direct_path,
            environment_send: self.environment_send,
            source_extent: self.source_extent.clone(),
            occlusion_profile: self.occlusion_profile,
            routing_generation: self.routing_generation,
        };
        if !snapshot.detached
            && let Some(emitter) = spatial.and_then(|frame| frame.emitter_state(snapshot.emitter))
        {
            snapshot.emitter_world_pose = emitter.pose;
            snapshot.acoustic_priority = emitter.acoustic_priority();
        }
        snapshot
    }
}

/// A worker-owned immutable copy of the routing facts. It deliberately excludes the linear
/// active-Voice retirement owner, so solver snapshots cannot duplicate lifecycle ownership.
#[derive(Clone, Debug)]
struct AcousticVoiceSnapshot {
    voice_id: VoiceId,
    emitter: Emitter,
    emitter_world_pose: Pose,
    acoustic_priority: f32,
    audibility: f32,
    detached: bool,
    direct_path: DirectPath,
    environment_send: EnvironmentSend,
    source_extent: SourceExtent,
    occlusion_profile: OcclusionProfile,
    routing_generation: u64,
}

impl AcousticVoiceSnapshot {
    fn route_key(&self) -> VoiceRouteKey {
        VoiceRouteKey {
            voice_id: self.voice_id,
            routing_generation: self.routing_generation,
        }
    }

    #[cfg(test)]
    fn active_for_test(&self) -> AcousticVoice {
        AcousticVoice {
            voice_id: self.voice_id,
            emitter: self.emitter,
            emitter_world_pose: self.emitter_world_pose,
            acoustic_priority: self.acoustic_priority,
            audibility: self.audibility,
            detached: self.detached,
            direct_path: self.direct_path,
            environment_send: self.environment_send,
            source_extent: self.source_extent.clone(),
            occlusion_profile: self.occlusion_profile,
            routing_generation: self.routing_generation,
            _retirement_witness: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct VoiceRouteKey {
    voice_id: VoiceId,
    routing_generation: u64,
}

#[derive(Clone)]
struct SolveInput {
    wake_generation: u64,
    spatial: Arc<SpatialFrame>,
    scene: Arc<AcousticSceneSnapshot>,
    voices: Vec<AcousticVoiceSnapshot>,
    environmental_acoustics_quality: f32,
}

struct InputState {
    wake_generation: u64,
    spatial: Option<Arc<SpatialFrame>>,
    scene: Option<Arc<AcousticSceneSnapshot>>,
    voices: Vec<AcousticVoiceSnapshot>,
    environmental_acoustics_quality: f32,
}

impl InputState {
    fn new(environmental_acoustics_quality: f32, max_voices: usize) -> Self {
        Self {
            wake_generation: 0,
            spatial: None,
            scene: None,
            voices: Vec::with_capacity(max_voices),
            environmental_acoustics_quality,
        }
    }

    fn capture(&self) -> Option<SolveInput> {
        Some(SolveInput {
            wake_generation: self.wake_generation,
            spatial: self.spatial.clone()?,
            scene: self.scene.clone()?,
            voices: self.voices.clone(),
            environmental_acoustics_quality: self.environmental_acoustics_quality,
        })
    }

    fn advance_wake_generation(&mut self) {
        self.wake_generation = self.wake_generation.wrapping_add(1).max(1);
    }
}

struct SharedInput {
    state: Mutex<InputState>,
    changed: Condvar,
}

impl SharedInput {
    fn new(environmental_acoustics_quality: f32, max_voices: usize) -> Self {
        let input = Self {
            state: Mutex::new(InputState::new(environmental_acoustics_quality, max_voices)),
            changed: Condvar::new(),
        };

        // pthread-backed Rust targets initialize these synchronization primitives lazily with
        // heap allocations. Force that one-time work onto the control thread so the first render
        // publication has the same allocation-free contract on every supported platform.
        drop(
            input
                .state
                .lock()
                .expect("new acoustic input state cannot be poisoned"),
        );
        input.changed.notify_one();
        input
    }
}

/// Bounded render-runtime port for active Voice acoustics state.
#[derive(Clone)]
pub(crate) struct AcousticVoiceInput {
    input: Arc<SharedInput>,
}

/// Render-owned, bounded lifetime registry for every acoustic Voice.
///
/// Identity lookup and slot reuse are constant-time. Complete [`AcousticVoice`] owners never
/// cross the worker mutex; only immutable snapshots are published from this registry.
pub(crate) struct AcousticVoiceRegistry {
    voices: Vec<Option<AcousticVoice>>,
    indices: HashMap<VoiceId, usize>,
    free_slots: Vec<usize>,
    next_routing_generation: u64,
    dirty: bool,
}

impl AcousticVoiceRegistry {
    pub(crate) fn new(max_voices: usize) -> Self {
        Self {
            voices: Vec::with_capacity(max_voices),
            indices: HashMap::with_capacity(max_voices),
            free_slots: Vec::with_capacity(max_voices),
            next_routing_generation: 0,
            dirty: false,
        }
    }

    pub(crate) fn activate(&mut self, mut voice: AcousticVoice, spatial: Option<&SpatialFrame>) {
        #[cfg(test)]
        crate::test_support::record_acoustic_input_comparison();
        assert!(
            !self.indices.contains_key(&voice.voice_id),
            "one acoustic Voice identity cannot be activated twice"
        );
        if let Some(emitter) = (!voice.detached)
            .then(|| spatial.and_then(|frame| frame.emitter_state(voice.emitter)))
            .flatten()
        {
            voice.emitter_world_pose = emitter.pose;
            voice.acoustic_priority = emitter.acoustic_priority();
        }
        self.next_routing_generation = self.next_routing_generation.wrapping_add(1).max(1);
        voice.routing_generation = self.next_routing_generation;

        let index = if let Some(index) = self.free_slots.pop() {
            index
        } else {
            assert!(
                self.voices.len() < self.voices.capacity(),
                "acoustic Voice registry exceeded the configured world capacity"
            );
            let index = self.voices.len();
            self.voices.push(None);
            index
        };
        assert!(self.indices.insert(voice.voice_id, index).is_none());
        self.voices[index] = Some(voice);
        self.dirty = true;
    }

    pub(crate) fn retire(&mut self, voice_id: VoiceId) -> Option<AcousticVoice> {
        #[cfg(test)]
        crate::test_support::record_acoustic_input_comparison();
        let index = self.indices.remove(&voice_id)?;
        let retired = self.voices[index].take();
        self.free_slots.push(index);
        self.dirty = true;
        retired
    }

    pub(crate) fn update_emitter_audibility(&mut self, emitter: Emitter, audibility: f32) {
        let mut changed = false;
        for voice in self.voices.iter_mut().filter_map(Option::as_mut) {
            if voice.emitter == emitter && !voice.detached {
                voice.audibility = audibility.max(0.0);
                changed = true;
            }
        }
        self.dirty |= changed;
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.indices.len()
    }
}

impl AcousticVoiceInput {
    #[cfg(test)]
    pub(crate) fn isolated(max_voices: usize) -> Self {
        Self {
            input: Arc::new(SharedInput::new(0.5, max_voices)),
        }
    }

    #[cfg(test)]
    pub(crate) fn wake_generation_for_test(&self) -> u64 {
        self.input.state.lock().unwrap().wake_generation
    }

    #[cfg(test)]
    pub(crate) fn voice_spatial_for_test(&self, voice_id: VoiceId) -> Option<(Pose, f32, bool)> {
        self.input
            .state
            .lock()
            .unwrap()
            .voices
            .iter()
            .find(|voice| voice.voice_id == voice_id)
            .map(|voice| {
                (
                    voice.emitter_world_pose,
                    voice.acoustic_priority,
                    voice.detached,
                )
            })
    }

    /// Attempts one complete render-to-worker synchronization without ever waiting.
    ///
    /// A busy worker leaves the registry dirty so the next render quantum retries. Poison is
    /// recovered while holding the acquired guard because lifecycle owners remain render-local.
    pub(crate) fn try_publish(&self, registry: &mut AcousticVoiceRegistry) -> bool {
        if !registry.dirty {
            return true;
        }
        let mut state = match self.input.state.try_lock() {
            Ok(state) => state,
            Err(TryLockError::WouldBlock) => return false,
            Err(TryLockError::Poisoned(error)) => {
                self.input.state.clear_poison();
                error.into_inner()
            }
        };
        let InputState {
            spatial, voices, ..
        } = &mut *state;
        voices.clear();
        voices.extend(
            registry
                .voices
                .iter()
                .filter_map(Option::as_ref)
                .map(|voice| voice.snapshot(spatial.as_deref())),
        );
        state.advance_wake_generation();
        registry.dirty = false;
        drop(state);
        self.input.changed.notify_one();
        true
    }
}

pub(crate) struct AcousticPropagationCounters {
    solve_count: AtomicU64,
    superseded_solve_count: AtomicU64,
    published_response_count: AtomicU64,
    latest_spatial_revision: AtomicU64,
    latest_geometry_version: AtomicU64,
    last_solve_time_us: AtomicU64,
    solve_time_max_us: AtomicU64,
    solve_time_histogram: [AtomicU64; 64],
    last_publication: Mutex<Option<Instant>>,
    telemetry_queue_high_water: std::sync::atomic::AtomicUsize,
    dropped_telemetry_events: AtomicU64,
    direct_ray_count: AtomicU64,
    cache_hit_count: AtomicU64,
    processed_extent_count: AtomicU64,
    lobe_count: AtomicU64,
    retained_response_count: AtomicU64,
    deferred_response_count: AtomicU64,
}

impl Default for AcousticPropagationCounters {
    fn default() -> Self {
        Self {
            solve_count: AtomicU64::new(0),
            superseded_solve_count: AtomicU64::new(0),
            published_response_count: AtomicU64::new(0),
            latest_spatial_revision: AtomicU64::new(0),
            latest_geometry_version: AtomicU64::new(0),
            last_solve_time_us: AtomicU64::new(0),
            solve_time_max_us: AtomicU64::new(0),
            solve_time_histogram: std::array::from_fn(|_| AtomicU64::new(0)),
            last_publication: Mutex::new(None),
            telemetry_queue_high_water: std::sync::atomic::AtomicUsize::new(0),
            dropped_telemetry_events: AtomicU64::new(0),
            direct_ray_count: AtomicU64::new(0),
            cache_hit_count: AtomicU64::new(0),
            processed_extent_count: AtomicU64::new(0),
            lobe_count: AtomicU64::new(0),
            retained_response_count: AtomicU64::new(0),
            deferred_response_count: AtomicU64::new(0),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct AcousticPropagationDiagnostics {
    pub solve_count: u64,
    pub superseded_solve_count: u64,
    pub published_response_count: u64,
    pub latest_spatial_revision: u64,
    pub latest_geometry_version: u64,
    pub last_solve_time_us: u64,
    pub solve_time_p50_us: u64,
    pub solve_time_p95_us: u64,
    pub solve_time_p99_us: u64,
    pub solve_time_max_us: u64,
    pub response_age_ms: u64,
    pub direct_ray_count: u64,
    pub cache_hit_count: u64,
    pub processed_extent_count: u64,
    pub lobe_count: u64,
    pub retained_response_count: u64,
    pub deferred_response_count: u64,
}

impl AcousticPropagationCounters {
    fn record_solve(&self, elapsed_us: u64) {
        self.solve_count.fetch_add(1, Ordering::Relaxed);
        self.last_solve_time_us.store(elapsed_us, Ordering::Relaxed);
        self.solve_time_max_us
            .fetch_max(elapsed_us, Ordering::Relaxed);
        self.solve_time_histogram[histogram_bucket(elapsed_us)].fetch_add(1, Ordering::Relaxed);
    }

    fn snapshot(&self) -> AcousticPropagationDiagnostics {
        let total = self
            .solve_time_histogram
            .iter()
            .map(|bucket| bucket.load(Ordering::Relaxed))
            .sum();
        let response_age_ms = self
            .last_publication
            .lock()
            .ok()
            .and_then(|published| *published)
            .map(|published| published.elapsed().as_millis() as u64)
            .unwrap_or(0);
        AcousticPropagationDiagnostics {
            solve_count: self.solve_count.load(Ordering::Relaxed),
            superseded_solve_count: self.superseded_solve_count.load(Ordering::Relaxed),
            published_response_count: self.published_response_count.load(Ordering::Relaxed),
            latest_spatial_revision: self.latest_spatial_revision.load(Ordering::Acquire),
            latest_geometry_version: self.latest_geometry_version.load(Ordering::Acquire),
            last_solve_time_us: self.last_solve_time_us.load(Ordering::Relaxed),
            solve_time_p50_us: histogram_percentile(&self.solve_time_histogram, total, 50),
            solve_time_p95_us: histogram_percentile(&self.solve_time_histogram, total, 95),
            solve_time_p99_us: histogram_percentile(&self.solve_time_histogram, total, 99),
            solve_time_max_us: self.solve_time_max_us.load(Ordering::Relaxed),
            response_age_ms,
            direct_ray_count: self.direct_ray_count.load(Ordering::Relaxed),
            cache_hit_count: self.cache_hit_count.load(Ordering::Relaxed),
            processed_extent_count: self.processed_extent_count.load(Ordering::Relaxed),
            lobe_count: self.lobe_count.load(Ordering::Relaxed),
            retained_response_count: self.retained_response_count.load(Ordering::Relaxed),
            deferred_response_count: self.deferred_response_count.load(Ordering::Relaxed),
        }
    }
}

pub(crate) struct AcousticPropagation {
    input: Arc<SharedInput>,
    response_publication: Publisher<AcousticResponse>,
    response_consumer: Mutex<Option<RealtimeConsumer<AcousticResponse>>>,
    counters: Arc<AcousticPropagationCounters>,
    enabled: Arc<AtomicBool>,
    quality_bits: AtomicU32,
    telemetry_receiver: Receiver<AcousticTelemetryEvent>,
    #[cfg(test)]
    worker_fault: Arc<std::sync::atomic::AtomicU8>,
}

pub(crate) struct AcousticWorker {
    context: PropagationWorkerContext,
    cancellation: ChildCancellation,
}

impl AcousticWorker {
    pub(crate) fn start(self, services: &mut RuntimeServices) -> crate::error::Result<()> {
        let cancellation = self.cancellation.clone();
        services.start_acoustics(
            "petalsonic-acoustics",
            cancellation,
            move |startup, cancellation| self.run(startup, cancellation),
        )
    }

    fn run(self, startup: ChildStartup, cancellation: ChildCancellation) -> RuntimeChildResult {
        startup.ready()?;
        propagation_loop(self.context, &cancellation)
    }
}

impl AcousticPropagation {
    pub(crate) fn prepare(
        distance_scaler: f32,
        enabled: Arc<AtomicBool>,
        environmental_acoustics_quality: f32,
        environmental_acoustics_budget: EnvironmentalAcousticsBudget,
        max_voices: usize,
        telemetry_capacity: usize,
    ) -> (Self, AcousticWorker) {
        let input = Arc::new(SharedInput::new(
            environmental_acoustics_quality,
            max_voices,
        ));
        let (response_publication, response_consumer) = RealtimeLatest::bounded(2);
        let counters = Arc::new(AcousticPropagationCounters::default());
        let (telemetry_sender, telemetry_receiver) = crossbeam_channel::bounded(telemetry_capacity);
        #[cfg(test)]
        let worker_fault = Arc::new(std::sync::atomic::AtomicU8::new(0));
        let cancellation = {
            let input = input.clone();
            ChildCancellation::new(move || input.changed.notify_one())
        };
        let worker = AcousticWorker {
            context: PropagationWorkerContext {
                input: input.clone(),
                response_publication: response_publication.clone(),
                counters: counters.clone(),
                enabled: enabled.clone(),
                distance_scaler,
                environmental_acoustics_budget,
                telemetry_sender,
                #[cfg(test)]
                worker_fault: worker_fault.clone(),
            },
            cancellation,
        };
        (
            Self {
                input,
                response_publication,
                response_consumer: Mutex::new(Some(response_consumer)),
                counters,
                enabled,
                quality_bits: AtomicU32::new(environmental_acoustics_quality.to_bits()),
                telemetry_receiver,
                #[cfg(test)]
                worker_fault,
            },
            worker,
        )
    }

    pub(crate) fn publish_spatial_frame(
        &self,
        frame: Arc<SpatialFrame>,
    ) -> std::result::Result<(), Arc<SpatialFrame>> {
        let Ok(mut state) = self.input.state.lock() else {
            return Err(frame);
        };
        state.advance_wake_generation();
        for voice in &mut state.voices {
            if voice.detached {
                continue;
            }
            if let Some(emitter) = frame.emitter_state(voice.emitter) {
                voice.emitter_world_pose = emitter.pose;
                voice.acoustic_priority = emitter.acoustic_priority();
            }
        }
        state.spatial = Some(frame);
        drop(state);
        self.input.changed.notify_one();
        Ok(())
    }

    pub(crate) fn publish_scene(
        &self,
        scene: Arc<AcousticSceneSnapshot>,
    ) -> std::result::Result<(), Arc<AcousticSceneSnapshot>> {
        let Ok(mut state) = self.input.state.lock() else {
            return Err(scene);
        };
        state.advance_wake_generation();
        state.scene = Some(scene);
        drop(state);
        self.input.changed.notify_one();
        Ok(())
    }

    pub(crate) fn take_response_consumer(&self) -> Option<RealtimeConsumer<AcousticResponse>> {
        self.response_consumer.lock().ok()?.take()
    }

    pub(crate) fn voice_input(&self) -> AcousticVoiceInput {
        AcousticVoiceInput {
            input: self.input.clone(),
        }
    }

    #[cfg(test)]
    pub(crate) fn wake_generation_for_test(&self) -> u64 {
        self.input.state.lock().unwrap().wake_generation
    }

    #[cfg(test)]
    pub(crate) fn voice_spatial_for_test(&self, voice_id: VoiceId) -> Option<(Pose, f32, bool)> {
        self.voice_input().voice_spatial_for_test(voice_id)
    }

    pub(crate) fn diagnostics(&self) -> AcousticPropagationDiagnostics {
        self.counters.snapshot()
    }

    pub(crate) fn telemetry_receiver(&self) -> Receiver<AcousticTelemetryEvent> {
        self.telemetry_receiver.clone()
    }

    pub(crate) fn telemetry_pressure(&self) -> (usize, u64) {
        (
            self.counters
                .telemetry_queue_high_water
                .load(Ordering::Relaxed),
            self.counters
                .dropped_telemetry_events
                .load(Ordering::Relaxed),
        )
    }

    pub(crate) fn set_enabled(&self, enabled: bool) {
        if self.enabled.load(Ordering::Acquire) == enabled {
            return;
        }
        // Change the predicate while holding the waiter's mutex so this notification cannot land
        // between the worker's predicate check and Condvar::wait.
        let state = self.input.state.lock().ok();
        self.enabled.store(enabled, Ordering::Release);
        drop(state);
        self.input.changed.notify_one();
    }

    pub(crate) fn set_quality(&self, quality: f32) {
        let quality_bits = quality.to_bits();
        if self.quality_bits.load(Ordering::Acquire) == quality_bits {
            return;
        }
        let Ok(mut state) = self.input.state.lock() else {
            return;
        };
        if state.environmental_acoustics_quality.to_bits() == quality_bits {
            self.quality_bits.store(quality_bits, Ordering::Release);
            return;
        }
        state.environmental_acoustics_quality = quality;
        state.advance_wake_generation();
        self.quality_bits.store(quality_bits, Ordering::Release);
        drop(state);
        self.input.changed.notify_one();
    }

    pub(crate) fn quality(&self) -> f32 {
        f32::from_bits(self.quality_bits.load(Ordering::Acquire))
    }

    #[cfg(test)]
    pub(crate) fn fail_worker_for_test(&self) {
        self.worker_fault.store(1, Ordering::Release);
        self.input.changed.notify_one();
    }

    #[cfg(test)]
    pub(crate) fn panic_worker_for_test(&self) {
        self.worker_fault.store(2, Ordering::Release);
        self.input.changed.notify_one();
    }

    pub(crate) fn close_publication(&self) {
        self.response_publication.close();
    }
}

struct PropagationWorkerContext {
    input: Arc<SharedInput>,
    response_publication: Publisher<AcousticResponse>,
    counters: Arc<AcousticPropagationCounters>,
    enabled: Arc<AtomicBool>,
    distance_scaler: f32,
    environmental_acoustics_budget: EnvironmentalAcousticsBudget,
    telemetry_sender: Sender<AcousticTelemetryEvent>,
    #[cfg(test)]
    worker_fault: Arc<std::sync::atomic::AtomicU8>,
}

struct AcousticPublisher {
    input: Arc<SharedInput>,
    response_publication: Publisher<AcousticResponse>,
    counters: Arc<AcousticPropagationCounters>,
    telemetry_sender: Sender<AcousticTelemetryEvent>,
}

impl AcousticPublisher {
    fn commit(
        &self,
        captured_wake_generation: u64,
        completed: CompletedAcousticFrame,
    ) -> std::result::Result<bool, RuntimeChildFailure> {
        let completed_solve_time_us = completed.solve_time_us;
        self.counters.record_solve(completed_solve_time_us);
        let publication_guard = self.input.state.lock().map_err(|_| {
            RuntimeChildFailure::new("acoustics publication input state is poisoned")
        })?;
        let newer_input_pending = publication_guard.wake_generation != captured_wake_generation;
        let discarded_spatial_revision = completed.spatial_revision;
        let discarded_geometry_version = completed.geometry_version;
        let Some(completed) = completed.retain_compatible(&publication_guard) else {
            drop(publication_guard);
            self.counters
                .superseded_solve_count
                .fetch_add(1, Ordering::Relaxed);
            try_send_acoustic_telemetry(
                &self.telemetry_sender,
                &self.counters,
                AcousticTelemetryEvent::SolveDiscarded {
                    spatial_revision: discarded_spatial_revision,
                    geometry_version: discarded_geometry_version,
                    reason: AcousticDiscardReason::Superseded,
                },
            );
            return Ok(newer_input_pending);
        };

        let published_at = Instant::now();
        let output = completed.into_solve_output(published_at);
        debug_assert_eq!(output.response.solve_time_us, completed_solve_time_us);
        let response_spatial_revision = output.response.spatial_revision;
        let response_geometry_version = output.response.geometry_version;
        self.response_publication
            .publish_latest(Arc::new(output.response))
            .map_err(|_| RuntimeChildFailure::new("acoustics response consumer disconnected"))?;
        drop(publication_guard);

        self.counters
            .latest_spatial_revision
            .store(response_spatial_revision, Ordering::Release);
        self.counters
            .latest_geometry_version
            .store(response_geometry_version, Ordering::Release);
        self.counters
            .published_response_count
            .fetch_add(1, Ordering::Relaxed);
        if let Ok(mut publication) = self.counters.last_publication.lock() {
            *publication = Some(published_at);
        }
        for telemetry in output.telemetry {
            self.counters.direct_ray_count.fetch_add(
                (telemetry.direct.ray_count + telemetry.environment.ray_count) as u64,
                Ordering::Relaxed,
            );
            self.counters.cache_hit_count.fetch_add(
                (telemetry.direct.cache_hit_count + telemetry.environment.cache_hit_count) as u64,
                Ordering::Relaxed,
            );
            self.counters
                .processed_extent_count
                .fetch_add(u64::from(telemetry.budget_member), Ordering::Relaxed);
            self.counters
                .lobe_count
                .fetch_add(telemetry.lobes.len() as u64, Ordering::Relaxed);
            match telemetry.solve_status {
                AcousticSolveStatus::Solved => {}
                AcousticSolveStatus::Retained => {
                    self.counters
                        .retained_response_count
                        .fetch_add(1, Ordering::Relaxed);
                }
                AcousticSolveStatus::Deferred => {
                    self.counters
                        .deferred_response_count
                        .fetch_add(1, Ordering::Relaxed);
                }
            }
            try_send_acoustic_telemetry(
                &self.telemetry_sender,
                &self.counters,
                AcousticTelemetryEvent::ExtentResponse(Box::new(telemetry)),
            );
        }
        for conclusion in output.conclusions {
            try_send_acoustic_telemetry(
                &self.telemetry_sender,
                &self.counters,
                AcousticTelemetryEvent::VoiceConclusion(conclusion.telemetry),
            );
        }
        Ok(newer_input_pending)
    }
}

fn propagation_loop(
    context: PropagationWorkerContext,
    cancellation: &ChildCancellation,
) -> RuntimeChildResult {
    let PropagationWorkerContext {
        input,
        response_publication,
        counters,
        enabled,
        distance_scaler,
        environmental_acoustics_budget,
        telemetry_sender,
        #[cfg(test)]
        worker_fault,
    } = context;
    let publisher = AcousticPublisher {
        input: input.clone(),
        response_publication,
        counters,
        telemetry_sender,
    };
    let mut consumed_wake_generation = 0;
    let mut next_solve = Instant::now();
    let mut solver = AcousticSolver::new(0);
    while !cancellation.is_requested() {
        let captured = {
            let mut state = input
                .state
                .lock()
                .map_err(|_| RuntimeChildFailure::new("acoustics input state is poisoned"))?;
            loop {
                #[cfg(test)]
                match worker_fault.swap(0, Ordering::AcqRel) {
                    1 => {
                        return Err(RuntimeChildFailure::new(
                            "injected acoustics worker failure",
                        ));
                    }
                    2 => panic!("injected acoustics worker panic"),
                    _ => {}
                }
                if cancellation.is_requested() {
                    return Ok(());
                }
                if !enabled.load(Ordering::Acquire) {
                    state = input.changed.wait(state).map_err(|_| {
                        RuntimeChildFailure::new("acoustics input wait state is poisoned")
                    })?;
                    continue;
                }
                let captured = (state.wake_generation != consumed_wake_generation)
                    .then(|| state.capture())
                    .flatten();
                let now = Instant::now();
                if captured.is_some() && now >= next_solve {
                    break captured;
                }
                if captured.is_none() {
                    state = input.changed.wait(state).map_err(|_| {
                        RuntimeChildFailure::new("acoustics input wait state is poisoned")
                    })?;
                } else {
                    let wait = next_solve.saturating_duration_since(now);
                    let (next_state, _) =
                        input.changed.wait_timeout(state, wait).map_err(|_| {
                            RuntimeChildFailure::new("acoustics timed input wait state is poisoned")
                        })?;
                    state = next_state;
                }
            }
        };
        let Some(captured) = captured else {
            continue;
        };
        consumed_wake_generation = captured.wake_generation;
        next_solve = Instant::now() + SOLVE_INTERVAL;

        let started = Instant::now();
        let plan = AcousticSolvePlan::for_quality(captured.environmental_acoustics_quality)
            .bounded_by(environmental_acoustics_budget);
        let completed = solver.solve_completed(&captured, distance_scaler, plan);
        let elapsed_us = started.elapsed().as_micros() as u64;
        let newer_input_pending = publisher.commit(
            captured.wake_generation,
            completed.with_solve_time(elapsed_us),
        )?;
        if newer_input_pending {
            next_solve = Instant::now();
        }
    }
    Ok(())
}

/// Filters one completed solve against the current scene and per-Voice routing lifetimes.
///
/// Ordinary pose revisions are deliberately absent from this compatibility seam. They make the
/// completed response spatially older, not unsafe. A scene change invalidates the whole solve;
/// Voice retirement or routing replacement invalidates only that Voice's result.
#[cfg(test)]
fn retain_compatible_completed_results(
    current: &InputState,
    completed: CompletedAcousticFrame,
) -> Option<AcousticSolveOutput> {
    completed
        .retain_compatible(current)
        .map(|completed| completed.into_solve_output(Instant::now()))
}

fn try_send_acoustic_telemetry(
    sender: &Sender<AcousticTelemetryEvent>,
    counters: &AcousticPropagationCounters,
    event: AcousticTelemetryEvent,
) {
    if sender.try_send(event).is_err() {
        counters
            .dropped_telemetry_events
            .fetch_add(1, Ordering::Relaxed);
        return;
    }
    let depth = sender.len();
    let mut current = counters.telemetry_queue_high_water.load(Ordering::Relaxed);
    while depth > current {
        match counters.telemetry_queue_high_water.compare_exchange_weak(
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

#[cfg(test)]
fn solve_response(input: &SolveInput, distance_scaler: f32) -> AcousticResponse {
    let plan = AcousticSolvePlan::for_quality(input.environmental_acoustics_quality);
    AcousticSolver::new(input.voices.len()).solve_with_plan(input, distance_scaler, plan)
}

struct AcousticSolveOutput {
    response: AcousticResponse,
    telemetry: Vec<AcousticExtentTelemetry>,
    conclusions: Vec<CompletedVoiceConclusion>,
}

/// Publication payload whose Voice-owned outputs cannot be filtered independently.
struct CompletedAcousticFrame {
    spatial_revision: u64,
    geometry_version: u64,
    late_reverb: LateReverbParameters,
    solve_time_us: u64,
    voices: Vec<CompletedVoice>,
}

/// One route generation's response and both telemetry views.
struct CompletedVoice {
    route: VoiceRouteKey,
    response: Option<DirectAcousticResponse>,
    telemetry: Option<AcousticExtentTelemetry>,
    conclusion: AcousticVoiceConclusionTelemetry,
}

struct VoiceAdmission {
    route: VoiceRouteKey,
    candidate_rank: usize,
    selected: bool,
}

struct SolvedVoiceOutput {
    route: VoiceRouteKey,
    response: DirectAcousticResponse,
    telemetry: AcousticExtentTelemetry,
}

impl CompletedAcousticFrame {
    /// Seals one solve into route-sorted, indivisible Voice envelopes.
    ///
    /// Candidate admission, response data, extent telemetry, and conclusion telemetry cross this
    /// interface together. Sorting and association stay behind the seam; consumers never have to
    /// correlate parallel arrays.
    fn finish_solve(
        input: &SolveInput,
        candidates: &[RankedVoice],
        selected: &[RankedVoice],
        mut responses: Vec<DirectAcousticResponse>,
        mut telemetry: Vec<AcousticExtentTelemetry>,
        late_reverb: LateReverbParameters,
        plan: AcousticSolvePlan,
    ) -> Self {
        let selected_routes = selected
            .iter()
            .map(|candidate| candidate.voice.route_key())
            .collect::<HashSet<_>>();
        let mut admissions = candidates
            .iter()
            .map(|candidate| VoiceAdmission {
                route: candidate.voice.route_key(),
                candidate_rank: candidate.candidate_rank,
                selected: selected_routes.contains(&candidate.voice.route_key()),
            })
            .collect::<Vec<_>>();
        admissions.sort_by(|left, right| compare_voice_routes(left.route, right.route));
        assert!(
            admissions.windows(2).all(|pair| {
                compare_voice_routes(pair[0].route, pair[1].route) == CmpOrdering::Less
            }),
            "one Voice route must have exactly one admission record"
        );

        responses.sort_by(|left, right| compare_voice_routes(left.route_key(), right.route_key()));
        telemetry.sort_by(|left, right| {
            compare_voice_ids(VoiceId::from(left.voice_id), VoiceId::from(right.voice_id))
        });
        assert_eq!(
            responses.len(),
            telemetry.len(),
            "one solved response must have exactly one extent telemetry record"
        );
        let mut outputs = responses
            .into_iter()
            .zip(telemetry)
            .map(|(response, telemetry)| {
                assert_eq!(
                    response.voice_id.value(),
                    telemetry.voice_id,
                    "response and extent telemetry must belong to the same Voice"
                );
                SolvedVoiceOutput {
                    route: response.route_key(),
                    response,
                    telemetry,
                }
            })
            .collect::<Vec<_>>();
        outputs.sort_by(|left, right| compare_voice_routes(left.route, right.route));
        assert!(
            outputs.windows(2).all(|pair| {
                compare_voice_routes(pair[0].route, pair[1].route) == CmpOrdering::Less
            }),
            "one Voice route must have exactly one solved output"
        );

        let mut active_voices = input.voices.iter().collect::<Vec<_>>();
        active_voices
            .sort_by(|left, right| compare_voice_routes(left.route_key(), right.route_key()));
        assert!(
            active_voices.windows(2).all(|pair| {
                compare_voice_ids(pair[0].voice_id, pair[1].voice_id) == CmpOrdering::Less
            }),
            "one acoustic publication cannot contain duplicate Voice identities"
        );

        let mut admissions = admissions.into_iter().peekable();
        let mut outputs = outputs.into_iter().peekable();
        let mut voices = Vec::with_capacity(active_voices.len());
        for voice in active_voices {
            let route = voice.route_key();
            let admission = match admissions.peek() {
                Some(candidate)
                    if compare_voice_routes(candidate.route, route) == CmpOrdering::Equal =>
                {
                    admissions.next()
                }
                Some(candidate)
                    if compare_voice_routes(candidate.route, route) == CmpOrdering::Less =>
                {
                    panic!("candidate admission does not belong to an active Voice")
                }
                _ => None,
            };
            let output = match outputs.peek() {
                Some(solved) if compare_voice_routes(solved.route, route) == CmpOrdering::Equal => {
                    outputs.next()
                }
                Some(solved) if compare_voice_routes(solved.route, route) == CmpOrdering::Less => {
                    panic!("solved output does not belong to an active Voice")
                }
                _ => None,
            };
            assert_eq!(
                admission.is_some(),
                output.is_some(),
                "every candidate must produce one complete response envelope"
            );

            let direct_enabled =
                matches!(
                    voice.direct_path.geometry(),
                    DirectGeometry::SimulatedTransmission
                ) && !matches!(voice.direct_path.placement(), DirectPlacement::Disabled);
            let environment_enabled =
                !matches!(voice.environment_send.origin(), EnvironmentOrigin::Disabled);
            let selected = admission
                .as_ref()
                .is_some_and(|candidate| candidate.selected);
            let active_outcome = if selected {
                AcousticRouteOutcome::Applied
            } else {
                AcousticRouteOutcome::ExcludedByBudget
            };
            let response = output.as_ref().map(|solved| &solved.response);
            let extent = output.as_ref().map(|solved| &solved.telemetry);
            let conclusion = AcousticVoiceConclusionTelemetry {
                voice_id: voice.voice_id.value(),
                emitter: voice.emitter,
                spatial_revision: input.spatial.revision(),
                geometry_version: input.scene.version(),
                candidate_rank: admission.as_ref().map(|candidate| candidate.candidate_rank),
                candidate_limit: plan.max_direct_sources,
                direct: if direct_enabled {
                    active_outcome
                } else {
                    AcousticRouteOutcome::Disabled
                },
                environment: if environment_enabled {
                    active_outcome
                } else {
                    AcousticRouteOutcome::Disabled
                },
                environment_transmission_gain: response
                    .map_or([1.0; 3], |response| response.environment_gain),
                early_tap_count: response.map_or(0, |response| response.early_reflections.len()),
                solve_status: extent.map(|event| event.solve_status),
            };
            let (response, telemetry) = output.map_or((None, None), |solved| {
                (Some(solved.response), Some(solved.telemetry))
            });
            voices.push(CompletedVoice {
                route,
                response,
                telemetry,
                conclusion,
            });
        }
        assert!(admissions.next().is_none(), "orphaned candidate admission");
        assert!(outputs.next().is_none(), "orphaned solved output");

        Self {
            spatial_revision: input.spatial.revision(),
            geometry_version: input.scene.version(),
            late_reverb,
            solve_time_us: 0,
            voices,
        }
    }

    fn with_solve_time(mut self, solve_time_us: u64) -> Self {
        self.solve_time_us = solve_time_us;
        self
    }

    /// Applies the ADR 0003 compatibility barrier as one consuming publication transition.
    ///
    /// Scene replacement rejects the whole frame. Ordinary pose revisions are deliberately
    /// absent, while retirement or rerouting removes the complete grouped Voice envelope. Both
    /// route sets are sorted once and then compared with a linear merge-join.
    fn retain_compatible(mut self, current: &InputState) -> Option<Self> {
        if current.scene.as_ref().map(|scene| scene.version()) != Some(self.geometry_version) {
            return None;
        }
        let mut current_routes = current
            .voices
            .iter()
            .map(AcousticVoiceSnapshot::route_key)
            .collect::<Vec<_>>();
        current_routes.sort_by(|left, right| compare_voice_routes(*left, *right));
        assert!(
            current_routes.windows(2).all(|pair| {
                compare_voice_ids(pair[0].voice_id, pair[1].voice_id) == CmpOrdering::Less
            }),
            "active acoustic Voice identities must be unique"
        );

        let mut current_index = 0;
        self.voices.retain(|completed| {
            loop {
                let Some(current_route) = current_routes.get(current_index).copied() else {
                    return false;
                };
                match compare_voice_routes(current_route, completed.route) {
                    CmpOrdering::Less => current_index += 1,
                    CmpOrdering::Equal => {
                        current_index += 1;
                        return true;
                    }
                    CmpOrdering::Greater => return false,
                }
            }
        });
        Some(self)
    }

    fn into_solve_output(self, published_at: Instant) -> AcousticSolveOutput {
        let mut direct = Vec::with_capacity(self.voices.len());
        let mut telemetry = Vec::with_capacity(self.voices.len());
        let mut conclusions = Vec::with_capacity(self.voices.len());
        for voice in self.voices {
            if let Some(response) = voice.response {
                direct.push(response);
            }
            if let Some(event) = voice.telemetry {
                telemetry.push(event);
            }
            conclusions.push(CompletedVoiceConclusion {
                telemetry: voice.conclusion,
            });
        }
        AcousticSolveOutput {
            response: AcousticResponse {
                spatial_revision: self.spatial_revision,
                geometry_version: self.geometry_version,
                direct: direct.into(),
                late_reverb: self.late_reverb,
                published_at,
                solve_time_us: self.solve_time_us,
            },
            telemetry,
            conclusions,
        }
    }
}

struct CompletedVoiceConclusion {
    telemetry: AcousticVoiceConclusionTelemetry,
}

#[derive(Clone, Debug)]
struct CachedDirectResponse {
    response: DirectAcousticResponse,
    telemetry: AcousticExtentTelemetry,
    solved_at_sim_time_seconds: f64,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum RouteKind {
    Direct,
    Environment,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct SampleCacheKey {
    voice_id: VoiceId,
    routing_generation: u64,
    emitter: Emitter,
    spatial_revision: u64,
    geometry_version: u64,
    sample_id: crate::domain::ExtentSampleId,
    route: RouteKind,
}

struct AcousticSolver {
    previous_budget_membership: HashSet<VoiceRouteKey>,
    last_solved_response: HashMap<VoiceRouteKey, CachedDirectResponse>,
    temporal_occlusion: HashMap<(VoiceRouteKey, RouteKind), TemporalOcclusionState>,
    sample_cache: HashMap<SampleCacheKey, ExtentSampleResponse>,
}

impl AcousticSolver {
    fn new(max_voices: usize) -> Self {
        Self {
            previous_budget_membership: HashSet::with_capacity(max_voices),
            last_solved_response: HashMap::with_capacity(max_voices),
            temporal_occlusion: HashMap::with_capacity(max_voices.saturating_mul(2)),
            sample_cache: HashMap::with_capacity(
                max_voices
                    .saturating_mul(MAX_EXTENT_SAMPLES)
                    .saturating_mul(2),
            ),
        }
    }

    #[cfg(test)]
    fn solve_with_plan(
        &mut self,
        input: &SolveInput,
        distance_scaler: f32,
        plan: AcousticSolvePlan,
    ) -> AcousticResponse {
        self.solve_with_telemetry(input, distance_scaler, plan)
            .response
    }

    #[cfg(test)]
    fn solve_with_telemetry(
        &mut self,
        input: &SolveInput,
        distance_scaler: f32,
        plan: AcousticSolvePlan,
    ) -> AcousticSolveOutput {
        self.solve_completed(input, distance_scaler, plan)
            .into_solve_output(Instant::now())
    }

    fn solve_completed(
        &mut self,
        input: &SolveInput,
        distance_scaler: f32,
        plan: AcousticSolvePlan,
    ) -> CompletedAcousticFrame {
        let candidates = ranked_voices(input, &self.previous_budget_membership);
        let (selected, skipped) = select_budgeted_candidates(&candidates, plan);
        let (mut direct, mut telemetry) = solve_direct(
            input,
            &selected,
            distance_scaler,
            &mut self.temporal_occlusion,
            &mut self.sample_cache,
        );
        solve_early_reflections(input, &selected, distance_scaler, plan, &mut direct);

        let sim_time = input.spatial.sim_time_seconds();
        for (response, telemetry) in direct.iter().zip(&telemetry) {
            self.last_solved_response.insert(
                VoiceRouteKey {
                    voice_id: response.voice_id,
                    routing_generation: response.routing_generation,
                },
                CachedDirectResponse {
                    response: response.clone(),
                    telemetry: telemetry.clone(),
                    solved_at_sim_time_seconds: sim_time,
                },
            );
        }
        for candidate in skipped {
            let max_age = max_response_age_seconds(candidate.voice.occlusion_profile);
            let retained = self
                .last_solved_response
                .get(&candidate.voice.route_key())
                .and_then(|cached| {
                    let age = (sim_time - cached.solved_at_sim_time_seconds).max(0.0) as f32;
                    (age <= max_age).then(|| {
                        let mut response = cached.response.clone();
                        response.solve_status = DirectSolveStatus::Retained;
                        response.cache_age_seconds = age;
                        let mut telemetry = cached.telemetry.clone();
                        telemetry.spatial_revision = input.spatial.revision();
                        telemetry.geometry_version = input.scene.version();
                        telemetry.solve_status = AcousticSolveStatus::Retained;
                        telemetry.cache_age_seconds = age;
                        telemetry.budget_member = false;
                        telemetry.direct.ray_count = 0;
                        telemetry.direct.cache_hit_count = 0;
                        telemetry.environment.ray_count = 0;
                        telemetry.environment.cache_hit_count = 0;
                        (response, telemetry)
                    })
                });
            let (response, event) = retained.unwrap_or_else(|| {
                (
                    deferred_response(&candidate.voice),
                    deferred_telemetry(input, &candidate.voice),
                )
            });
            direct.push(response);
            telemetry.push(event);
        }
        self.previous_budget_membership.clear();
        self.previous_budget_membership
            .extend(selected.iter().map(|candidate| candidate.voice.route_key()));
        let active_voice_routes = input
            .voices
            .iter()
            .map(AcousticVoiceSnapshot::route_key)
            .collect::<HashSet<_>>();
        self.last_solved_response
            .retain(|route, _| active_voice_routes.contains(route));
        self.temporal_occlusion
            .retain(|(route, _), _| active_voice_routes.contains(route));
        self.sample_cache.retain(|key, _| {
            active_voice_routes.contains(&VoiceRouteKey {
                voice_id: key.voice_id,
                routing_generation: key.routing_generation,
            }) && key.spatial_revision == input.spatial.revision()
                && key.geometry_version == input.scene.version()
        });

        CompletedAcousticFrame::finish_solve(
            input,
            &candidates,
            &selected,
            direct,
            telemetry,
            solve_late_reverb(input, distance_scaler, plan),
            plan,
        )
    }
}

fn deferred_response(voice: &AcousticVoiceSnapshot) -> DirectAcousticResponse {
    DirectAcousticResponse {
        voice_id: voice.voice_id,
        routing_generation: voice.routing_generation,
        gain: [1.0; 3],
        environment_gain: [1.0; 3],
        direct_lobes: Vec::new(),
        environment_representatives: Vec::new(),
        early_reflections: Vec::new(),
        solve_status: DirectSolveStatus::Deferred,
        cache_age_seconds: 0.0,
    }
}

fn inactive_route_telemetry() -> AcousticRouteTelemetry {
    AcousticRouteTelemetry {
        sample_count: 0,
        samples: Vec::new(),
        ray_count: 0,
        cache_hit_count: 0,
        hit_count: 0,
        visible_fraction: 1.0,
        raw_gain: [1.0; 3],
        filtered_gain: [1.0; 3],
        classified_state: AcousticOcclusionState::Visible,
        dwell_seconds: 0.0,
    }
}

fn deferred_telemetry(
    input: &SolveInput,
    voice: &AcousticVoiceSnapshot,
) -> AcousticExtentTelemetry {
    AcousticExtentTelemetry {
        voice_id: voice.voice_id.value(),
        emitter: voice.emitter,
        spatial_revision: input.spatial.revision(),
        geometry_version: input.scene.version(),
        response_spatial_revision: input.spatial.revision(),
        response_geometry_version: input.scene.version(),
        extent_sample_count: voice.source_extent.sample_count(),
        direct: inactive_route_telemetry(),
        environment: inactive_route_telemetry(),
        lobes: Vec::new(),
        solve_status: AcousticSolveStatus::Deferred,
        cache_age_seconds: 0.0,
        budget_member: false,
    }
}

fn max_response_age_seconds(profile: OcclusionProfile) -> f32 {
    match profile {
        OcclusionProfile::PointExact => 0.25,
        OcclusionProfile::AmbientDistributed(profile) => profile.max_response_age_seconds(),
    }
}

#[derive(Clone, Debug)]
struct RankedVoice {
    voice: AcousticVoiceSnapshot,
    direct_pose: Option<Pose>,
    environment_pose: Option<Pose>,
    candidate_rank: usize,
}

fn ranked_voices(
    input: &SolveInput,
    previous_budget_membership: &HashSet<VoiceRouteKey>,
) -> Vec<RankedVoice> {
    let listener_pose = input.spatial.listener();
    let listener = listener_pose.position;
    let mut candidates: Vec<(f32, RankedVoice)> = input
        .voices
        .iter()
        .filter_map(|voice| {
            let direct_pose = match voice.direct_path.placement() {
                DirectPlacement::World
                    if matches!(
                        voice.direct_path.geometry(),
                        DirectGeometry::SimulatedTransmission
                    ) =>
                {
                    Some(voice.emitter_world_pose)
                }
                DirectPlacement::ListenerRelative(local_pose)
                    if matches!(
                        voice.direct_path.geometry(),
                        DirectGeometry::SimulatedTransmission
                    ) =>
                {
                    Some(listener_to_world_pose(listener_pose, local_pose))
                }
                DirectPlacement::ListenerPositionRelative(world_offset_pose)
                    if matches!(
                        voice.direct_path.geometry(),
                        DirectGeometry::SimulatedTransmission
                    ) =>
                {
                    Some(listener_position_relative_to_world_pose(
                        listener_pose,
                        world_offset_pose,
                    ))
                }
                _ => None,
            };
            let environment_pose = match voice.environment_send.origin() {
                EnvironmentOrigin::FollowEmitter => Some(voice.emitter_world_pose),
                EnvironmentOrigin::World(origin) => Some(origin),
                EnvironmentOrigin::Disabled => None,
            };
            if direct_pose
                .is_some_and(|pose| !extent_transform_is_finite(&voice.source_extent, pose))
                || environment_pose
                    .is_some_and(|pose| !extent_transform_is_finite(&voice.source_extent, pose))
            {
                return None;
            }
            let origin = environment_pose.or(direct_pose)?.position;
            let distance = origin.distance(listener);
            let priority = voice.acoustic_priority;
            let membership_bias = if previous_budget_membership.contains(&voice.route_key()) {
                1.1
            } else {
                1.0
            };
            (distance.is_finite() && priority.is_finite() && priority > 0.0).then_some((
                priority * voice.audibility.max(0.0) * membership_bias / (1.0 + distance),
                RankedVoice {
                    voice: voice.clone(),
                    direct_pose,
                    environment_pose,
                    candidate_rank: 0,
                },
            ))
        })
        .collect();
    candidates.sort_by(|left, right| {
        right
            .0
            .partial_cmp(&left.0)
            .unwrap_or(CmpOrdering::Equal)
            .then_with(|| {
                left.1
                    .voice
                    .voice_id
                    .value()
                    .cmp(&right.1.voice.voice_id.value())
            })
    });
    candidates
        .into_iter()
        .enumerate()
        .map(|(index, (_, mut voice))| {
            voice.candidate_rank = index + 1;
            voice
        })
        .collect()
}

fn extent_transform_is_finite(extent: &SourceExtent, pose: Pose) -> bool {
    extent.weighted().is_none_or(|weighted| {
        weighted
            .samples()
            .iter()
            .all(|sample| (pose.position + pose.rotation * sample.local_position()).is_finite())
    })
}

fn select_budgeted_candidates(
    candidates: &[RankedVoice],
    plan: AcousticSolvePlan,
) -> (Vec<RankedVoice>, Vec<RankedVoice>) {
    let mut selected = Vec::with_capacity(plan.max_direct_sources.min(candidates.len()));
    let mut skipped = Vec::new();
    let mut rays = 0usize;
    for candidate in candidates {
        let route_count = usize::from(candidate.direct_pose.is_some())
            + usize::from(candidate.environment_pose.is_some());
        let cost = candidate
            .voice
            .source_extent
            .sample_count()
            .saturating_mul(route_count);
        if selected.len() < plan.max_direct_sources
            && rays.saturating_add(cost) <= plan.max_direct_rays
        {
            rays += cost;
            selected.push(candidate.clone());
        } else {
            skipped.push(candidate.clone());
        }
    }
    (selected, skipped)
}

fn solve_direct(
    input: &SolveInput,
    candidates: &[RankedVoice],
    distance_scaler: f32,
    temporal_occlusion: &mut HashMap<(VoiceRouteKey, RouteKind), TemporalOcclusionState>,
    sample_cache: &mut HashMap<SampleCacheKey, ExtentSampleResponse>,
) -> (Vec<DirectAcousticResponse>, Vec<AcousticExtentTelemetry>) {
    let listener = input.spatial.listener().position;
    let ray_epsilon_world = RAY_EPSILON_METERS / distance_scaler.max(0.001);

    candidates
        .iter()
        .map(|candidate| {
            let direct_samples = candidate.direct_pose.map(|pose| {
                trace_extent_transmission(
                    input,
                    RouteTraceContext {
                        listener,
                        ray_epsilon_world,
                        voice_id: candidate.voice.voice_id,
                        routing_generation: candidate.voice.routing_generation,
                        emitter: candidate.voice.emitter,
                        route: RouteKind::Direct,
                    },
                    resolve_extent_samples(&candidate.voice.source_extent, pose),
                    sample_cache,
                )
            });
            let environment_samples = candidate.environment_pose.map(|pose| {
                trace_extent_transmission(
                    input,
                    RouteTraceContext {
                        listener,
                        ray_epsilon_world,
                        voice_id: candidate.voice.voice_id,
                        routing_generation: candidate.voice.routing_generation,
                        emitter: candidate.voice.emitter,
                        route: RouteKind::Environment,
                    },
                    resolve_extent_samples(&candidate.voice.source_extent, pose),
                    sample_cache,
                )
            });
            let direct_aggregate = direct_samples
                .as_ref()
                .map(|trace| trace.samples.as_slice())
                .map(aggregate_extent_energy);
            let environment_aggregate = environment_samples
                .as_ref()
                .map(|trace| trace.samples.as_slice())
                .map(aggregate_extent_energy);
            let raw_direct = direct_aggregate.map_or([1.0; 3], |aggregate| aggregate.gain);
            let raw_environment =
                environment_aggregate.map_or([1.0; 3], |aggregate| aggregate.gain);
            let direct_update = filter_profile_gain(
                candidate.voice.route_key(),
                RouteKind::Direct,
                raw_direct,
                direct_aggregate.map_or(1.0, |aggregate| aggregate.visible_fraction),
                input.spatial.sim_time_seconds(),
                candidate.voice.occlusion_profile,
                temporal_occlusion,
            );
            let environment_update = filter_profile_gain(
                candidate.voice.route_key(),
                RouteKind::Environment,
                raw_environment,
                environment_aggregate.map_or(1.0, |aggregate| aggregate.visible_fraction),
                input.spatial.sim_time_seconds(),
                candidate.voice.occlusion_profile,
                temporal_occlusion,
            );
            let gain = direct_update.filtered_gain;
            let environment_gain = environment_update.filtered_gain;
            let direct_lobes =
                direct_samples.as_ref().map_or_else(Vec::new, |trace| {
                    match candidate.voice.source_extent {
                        SourceExtent::Point => Vec::new(),
                        SourceExtent::WeightedSamples(_) => aggregate_directional_lobes(
                            &trace.samples,
                            listener,
                            profile_lobe_count(candidate.voice.occlusion_profile),
                            gain,
                        ),
                    }
                });
            let environment_representatives =
                environment_samples.as_ref().map_or_else(Vec::new, |trace| {
                    select_environment_representatives(&trace.samples, 2)
                });
            let direct_route = route_telemetry(
                direct_samples.as_ref(),
                direct_aggregate,
                raw_direct,
                direct_update,
            );
            let environment_route = route_telemetry(
                environment_samples.as_ref(),
                environment_aggregate,
                raw_environment,
                environment_update,
            );
            let lobe_telemetry = direct_lobes
                .iter()
                .map(|lobe| AcousticLobeTelemetry {
                    lobe_id: lobe.lobe_id,
                    direction: lobe.direction,
                    gain: lobe.gain,
                    power: lobe.power,
                })
                .collect();
            (
                DirectAcousticResponse {
                    voice_id: candidate.voice.voice_id,
                    routing_generation: candidate.voice.routing_generation,
                    gain,
                    environment_gain,
                    direct_lobes,
                    environment_representatives,
                    early_reflections: Vec::with_capacity(MAX_EARLY_REFLECTION_TAPS),
                    solve_status: DirectSolveStatus::Solved,
                    cache_age_seconds: 0.0,
                },
                AcousticExtentTelemetry {
                    voice_id: candidate.voice.voice_id.value(),
                    emitter: candidate.voice.emitter,
                    spatial_revision: input.spatial.revision(),
                    geometry_version: input.scene.version(),
                    response_spatial_revision: input.spatial.revision(),
                    response_geometry_version: input.scene.version(),
                    extent_sample_count: candidate.voice.source_extent.sample_count(),
                    direct: direct_route,
                    environment: environment_route,
                    lobes: lobe_telemetry,
                    solve_status: AcousticSolveStatus::Solved,
                    cache_age_seconds: 0.0,
                    budget_member: true,
                },
            )
        })
        .unzip()
}

fn route_telemetry(
    trace: Option<&ExtentTrace>,
    aggregate: Option<ExtentEnergyAggregate>,
    raw_gain: [f32; 3],
    update: TemporalOcclusionUpdate,
) -> AcousticRouteTelemetry {
    let Some(trace) = trace else {
        return inactive_route_telemetry();
    };
    let aggregate = aggregate.expect("a traced extent always has an energy aggregate");
    debug_assert!(!trace.samples.is_empty());
    AcousticRouteTelemetry {
        sample_count: trace.samples.len(),
        samples: trace
            .samples
            .iter()
            .map(|sample| AcousticSampleObservation {
                sample_id: sample.sample_id,
                normalized_power_weight: sample.power_weight,
                world_position: sample.world_position,
                hit: sample.hit,
                transmission: sample.transmission,
            })
            .collect(),
        ray_count: trace.ray_count,
        cache_hit_count: trace.cache_hit_count,
        hit_count: aggregate.hit_count,
        visible_fraction: aggregate.visible_fraction,
        raw_gain,
        filtered_gain: update.filtered_gain,
        classified_state: match update.classification {
            OcclusionClassification::Visible => AcousticOcclusionState::Visible,
            OcclusionClassification::Occluded => AcousticOcclusionState::Occluded,
        },
        dwell_seconds: update.dwell_seconds,
    }
}

#[derive(Clone, Copy, Debug)]
struct ResolvedExtentSample {
    sample_id: crate::domain::ExtentSampleId,
    power_weight: f32,
    world_position: Vec3,
}

struct ExtentTrace {
    samples: Vec<ExtentSampleResponse>,
    ray_count: usize,
    cache_hit_count: usize,
}

#[derive(Clone, Copy)]
struct RouteTraceContext {
    listener: Vec3,
    ray_epsilon_world: f32,
    voice_id: VoiceId,
    routing_generation: u64,
    emitter: Emitter,
    route: RouteKind,
}

fn resolve_extent_samples(extent: &SourceExtent, pose: Pose) -> Vec<ResolvedExtentSample> {
    match extent {
        SourceExtent::Point => vec![ResolvedExtentSample {
            sample_id: crate::domain::ExtentSampleId::POINT,
            power_weight: 1.0,
            world_position: pose.position,
        }],
        SourceExtent::WeightedSamples(weighted) => weighted
            .samples()
            .iter()
            .map(|sample| ResolvedExtentSample {
                sample_id: sample.id(),
                power_weight: sample.power_weight(),
                world_position: pose.position + pose.rotation * sample.local_position(),
            })
            .collect(),
    }
}

fn trace_extent_transmission(
    input: &SolveInput,
    context: RouteTraceContext,
    samples: Vec<ResolvedExtentSample>,
    sample_cache: &mut HashMap<SampleCacheKey, ExtentSampleResponse>,
) -> ExtentTrace {
    let mut responses = vec![None; samples.len()];
    let mut miss_indices = Vec::with_capacity(samples.len());
    let mut miss_keys = Vec::with_capacity(samples.len());
    let mut rays = Vec::with_capacity(samples.len());
    let mut min_distances = Vec::with_capacity(samples.len());
    let mut max_distances = Vec::with_capacity(samples.len());
    for (index, sample) in samples.iter().enumerate() {
        let key = SampleCacheKey {
            voice_id: context.voice_id,
            routing_generation: context.routing_generation,
            emitter: context.emitter,
            spatial_revision: input.spatial.revision(),
            geometry_version: input.scene.version(),
            sample_id: sample.sample_id,
            route: context.route,
        };
        if let Some(cached) = sample_cache.get(&key).copied() {
            responses[index] = Some(cached);
            continue;
        }
        let delta = sample.world_position - context.listener;
        let distance = delta.length();
        miss_indices.push(index);
        miss_keys.push(key);
        rays.push(AcousticRay {
            origin: context.listener,
            direction: normalize_or(delta, Vec3::Z),
        });
        let max_distance = (distance - context.ray_epsilon_world).max(0.0);
        min_distances.push(context.ray_epsilon_world.min(max_distance));
        max_distances.push(max_distance);
    }
    if !rays.is_empty() {
        let mut hits = vec![None; rays.len()];
        input.scene.query().trace_closest_hit_batch(
            &rays,
            &min_distances,
            &max_distances,
            &mut hits,
        );
        for ((((index, key), hit), min_distance), max_distance) in miss_indices
            .into_iter()
            .zip(miss_keys)
            .zip(hits)
            .zip(min_distances)
            .zip(max_distances)
        {
            let sample = samples[index];
            let hit =
                hit.filter(|hit| valid_hit_distance(hit.distance, min_distance, max_distance));
            let response = ExtentSampleResponse {
                sample_id: sample.sample_id,
                power_weight: sample.power_weight,
                world_position: sample.world_position,
                transmission: hit
                    .map(|hit| {
                        hit.material
                            .transmission
                            .map(|gain| sanitize_unit(gain, 1.0))
                    })
                    .unwrap_or([1.0; 3]),
                hit: hit.is_some(),
            };
            sample_cache.insert(key, response);
            responses[index] = Some(response);
        }
    }
    ExtentTrace {
        samples: responses
            .into_iter()
            .map(|response| response.expect("every extent sample must resolve or hit the cache"))
            .collect(),
        ray_count: rays.len(),
        cache_hit_count: samples.len().saturating_sub(rays.len()),
    }
}

fn apply_profile_floor(gain: [f32; 3], profile: OcclusionProfile) -> [f32; 3] {
    match profile {
        OcclusionProfile::PointExact => gain,
        OcclusionProfile::AmbientDistributed(profile) => {
            let floor = profile.gain_floor();
            std::array::from_fn(|band| gain[band].max(floor[band]))
        }
    }
}

fn filter_profile_gain(
    voice_route: VoiceRouteKey,
    route: RouteKind,
    raw_gain: [f32; 3],
    visible_fraction: f32,
    sim_time_seconds: f64,
    profile: OcclusionProfile,
    temporal_occlusion: &mut HashMap<(VoiceRouteKey, RouteKind), TemporalOcclusionState>,
) -> TemporalOcclusionUpdate {
    match profile {
        OcclusionProfile::PointExact => TemporalOcclusionUpdate {
            filtered_gain: raw_gain,
            classification: if visible_fraction < 1.0 - f32::EPSILON {
                OcclusionClassification::Occluded
            } else {
                OcclusionClassification::Visible
            },
            dwell_seconds: 0.0,
        },
        OcclusionProfile::AmbientDistributed(profile) => {
            let bounded =
                apply_profile_floor(raw_gain, OcclusionProfile::AmbientDistributed(profile));
            temporal_occlusion
                .entry((voice_route, route))
                .or_default()
                .update(bounded, visible_fraction, sim_time_seconds, profile)
        }
    }
}

fn profile_lobe_count(profile: OcclusionProfile) -> u8 {
    match profile {
        OcclusionProfile::PointExact => MAX_DIRECT_LOBES as u8,
        OcclusionProfile::AmbientDistributed(profile) => profile.lobe_count(),
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum OcclusionClassification {
    #[default]
    Visible,
    Occluded,
}

#[derive(Clone, Copy, Debug)]
struct TemporalOcclusionUpdate {
    filtered_gain: [f32; 3],
    classification: OcclusionClassification,
    dwell_seconds: f32,
}

#[derive(Clone, Copy, Debug)]
struct TemporalOcclusionState {
    initialized: bool,
    filtered_gain: [f32; 3],
    classification: OcclusionClassification,
    classification_since: f64,
    pending_classification: Option<OcclusionClassification>,
    pending_since: f64,
    last_time: f64,
}

impl Default for TemporalOcclusionState {
    fn default() -> Self {
        Self {
            initialized: false,
            filtered_gain: [1.0; 3],
            classification: OcclusionClassification::Visible,
            classification_since: 0.0,
            pending_classification: None,
            pending_since: 0.0,
            last_time: 0.0,
        }
    }
}

impl TemporalOcclusionState {
    fn update(
        &mut self,
        raw_gain: [f32; 3],
        visible_fraction: f32,
        sim_time_seconds: f64,
        profile: crate::domain::DistributedOcclusionProfile,
    ) -> TemporalOcclusionUpdate {
        let sim_time_seconds = if sim_time_seconds.is_finite() {
            sim_time_seconds.max(self.last_time)
        } else {
            self.last_time
        };
        if !self.initialized {
            self.initialized = true;
            self.filtered_gain = raw_gain;
            self.classification = if visible_fraction <= profile.classification().0 {
                OcclusionClassification::Occluded
            } else {
                OcclusionClassification::Visible
            };
            self.classification_since = sim_time_seconds;
            self.last_time = sim_time_seconds;
            return self.snapshot(sim_time_seconds);
        }

        let elapsed = (sim_time_seconds - self.last_time).max(0.0) as f32;
        let (attack_seconds, release_seconds) = profile.response_times_seconds();
        for (filtered, raw) in self.filtered_gain.iter_mut().zip(raw_gain) {
            let response_seconds = if raw < *filtered {
                attack_seconds
            } else {
                release_seconds
            };
            let alpha = 1.0 - (-elapsed / response_seconds).exp();
            *filtered += alpha * (raw - *filtered);
        }
        self.last_time = sim_time_seconds;

        let (enter_threshold, exit_threshold, minimum_dwell_seconds) = profile.classification();
        let requested = match self.classification {
            OcclusionClassification::Visible if visible_fraction < enter_threshold => {
                Some(OcclusionClassification::Occluded)
            }
            OcclusionClassification::Occluded if visible_fraction > exit_threshold => {
                Some(OcclusionClassification::Visible)
            }
            _ => None,
        };
        match requested {
            Some(requested) if self.pending_classification == Some(requested) => {
                if sim_time_seconds - self.pending_since + 1.0e-6
                    >= f64::from(minimum_dwell_seconds)
                {
                    self.classification = requested;
                    self.classification_since = sim_time_seconds;
                    self.pending_classification = None;
                }
            }
            Some(requested) => {
                self.pending_classification = Some(requested);
                self.pending_since = sim_time_seconds;
            }
            None => self.pending_classification = None,
        }
        self.snapshot(sim_time_seconds)
    }

    fn snapshot(self, sim_time_seconds: f64) -> TemporalOcclusionUpdate {
        TemporalOcclusionUpdate {
            filtered_gain: self.filtered_gain,
            classification: self.classification,
            dwell_seconds: (sim_time_seconds - self.classification_since).max(0.0) as f32,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct ExtentSampleResponse {
    sample_id: crate::domain::ExtentSampleId,
    power_weight: f32,
    world_position: Vec3,
    transmission: [f32; 3],
    hit: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ExtentEnergyAggregate {
    gain: [f32; 3],
    hit_count: usize,
    visible_fraction: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DirectLobeTarget {
    pub lobe_id: u8,
    pub direction: Vec3,
    pub gain: [f32; 3],
    pub power: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct EnvironmentRepresentative {
    sample_id: crate::domain::ExtentSampleId,
    world_position: Vec3,
    power: f32,
}

fn aggregate_extent_energy(samples: &[ExtentSampleResponse]) -> ExtentEnergyAggregate {
    let mut ordered = samples.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|sample| sample.sample_id);
    let mut energy = [0.0_f64; 3];
    let mut visible_power = 0.0_f64;
    let mut hit_count = 0;
    for sample in ordered {
        let weight = f64::from(sample.power_weight);
        if sample.hit {
            hit_count += 1;
        } else {
            visible_power += weight;
        }
        for (band_energy, transmission) in energy.iter_mut().zip(sample.transmission) {
            let transmission = f64::from(sanitize_unit(transmission, 1.0));
            *band_energy += weight * transmission * transmission;
        }
    }
    ExtentEnergyAggregate {
        gain: energy.map(|energy| energy.max(0.0).sqrt() as f32),
        hit_count,
        visible_fraction: visible_power.clamp(0.0, 1.0) as f32,
    }
}

fn aggregate_directional_lobes(
    samples: &[ExtentSampleResponse],
    listener: Vec3,
    requested_lobes: u8,
    target_gain: [f32; 3],
) -> Vec<DirectLobeTarget> {
    let lobe_count = usize::from(requested_lobes).clamp(1, MAX_DIRECT_LOBES);
    let mut ordered = samples.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|sample| sample.sample_id);
    let mut band_energy = [[0.0_f64; 3]; MAX_DIRECT_LOBES];
    let mut power_basis = [0.0_f64; MAX_DIRECT_LOBES];
    let mut fallback_power = [0.0_f64; MAX_DIRECT_LOBES];
    let mut direction_sum = [Vec3::ZERO; MAX_DIRECT_LOBES];
    let mut fallback: [Option<(f32, crate::domain::ExtentSampleId, Vec3)>; MAX_DIRECT_LOBES] =
        [None; MAX_DIRECT_LOBES];

    for sample in ordered {
        let lobe = sample.sample_id.0 as usize % lobe_count;
        fallback_power[lobe] += f64::from(sample.power_weight);
        let direction = normalize_or(sample.world_position - listener, Vec3::Z);
        let mut broadband_energy = 0.0_f64;
        for (energy, transmission) in band_energy[lobe].iter_mut().zip(sample.transmission) {
            let transmission = f64::from(sanitize_unit(transmission, 1.0));
            let sample_energy = f64::from(sample.power_weight) * transmission * transmission;
            *energy += sample_energy;
            broadband_energy += sample_energy / 3.0;
        }
        let directional_weight = broadband_energy.max(f64::from(sample.power_weight) * 1.0e-6);
        power_basis[lobe] += broadband_energy;
        direction_sum[lobe] += direction * directional_weight as f32;
        let candidate = (directional_weight as f32, sample.sample_id, direction);
        if fallback[lobe].is_none_or(|current| {
            candidate.0 > current.0 || (candidate.0 == current.0 && candidate.1 < current.1)
        }) {
            fallback[lobe] = Some(candidate);
        }
    }

    let measured_total_power = power_basis.iter().sum::<f64>();
    let total_fallback_power = fallback_power.iter().sum::<f64>();
    let total_band_energy = std::array::from_fn::<_, 3, _>(|band| {
        band_energy
            .iter()
            .map(|lobe_energy| lobe_energy[band])
            .sum::<f64>()
    });
    let active_lobes = (0..lobe_count)
        .filter(|index| fallback[*index].is_some())
        .count()
        .max(1);
    let mut lobes = Vec::with_capacity(active_lobes);
    for lobe in 0..lobe_count {
        let Some((_, _, fallback_direction)) = fallback[lobe] else {
            continue;
        };
        let power = if measured_total_power > f64::EPSILON {
            (power_basis[lobe] / measured_total_power) as f32
        } else if total_fallback_power > f64::EPSILON {
            (fallback_power[lobe] / total_fallback_power) as f32
        } else {
            1.0 / active_lobes as f32
        };
        lobes.push(DirectLobeTarget {
            lobe_id: lobe as u8,
            direction: normalize_or(direction_sum[lobe], fallback_direction),
            gain: std::array::from_fn(|band| {
                let energy_fraction = if total_band_energy[band] > f64::EPSILON {
                    band_energy[lobe][band] / total_band_energy[band]
                } else {
                    f64::from(power)
                };
                energy_fraction.max(0.0).sqrt() as f32 * target_gain[band]
            }),
            power,
        });
    }
    lobes
}

fn select_environment_representatives(
    samples: &[ExtentSampleResponse],
    limit: usize,
) -> Vec<EnvironmentRepresentative> {
    let mut representatives = samples
        .iter()
        .map(|sample| {
            let transmission_energy = sample
                .transmission
                .iter()
                .map(|transmission| transmission * transmission)
                .sum::<f32>()
                / 3.0;
            EnvironmentRepresentative {
                sample_id: sample.sample_id,
                world_position: sample.world_position,
                power: sample.power_weight * transmission_energy,
            }
        })
        .collect::<Vec<_>>();
    representatives.sort_by(|left, right| {
        right
            .power
            .total_cmp(&left.power)
            .then_with(|| left.sample_id.cmp(&right.sample_id))
    });
    representatives.truncate(limit.min(representatives.len()));
    let selected_power = representatives
        .iter()
        .map(|representative| representative.power)
        .sum::<f32>();
    if selected_power > f32::EPSILON {
        for representative in &mut representatives {
            representative.power /= selected_power;
        }
    } else if !representatives.is_empty() {
        let equal_power = 1.0 / representatives.len() as f32;
        for representative in &mut representatives {
            representative.power = equal_power;
        }
    }
    representatives.sort_by_key(|representative| representative.sample_id);
    representatives
}

fn solve_early_reflections(
    input: &SolveInput,
    candidates: &[RankedVoice],
    distance_scaler: f32,
    plan: AcousticSolvePlan,
    responses: &mut [DirectAcousticResponse],
) {
    let listener = input.spatial.listener().position;
    let ray_epsilon_world = RAY_EPSILON_METERS / distance_scaler.max(0.001);
    let max_distance_world = MAX_TRACE_DISTANCE_METERS / distance_scaler.max(0.001);
    let probe_rays: Vec<_> = (0..plan.early_reflection_ray_count)
        .map(|index| AcousticRay {
            origin: listener,
            direction: fibonacci_direction(index, plan.early_reflection_ray_count),
        })
        .collect();
    let min_distances = vec![ray_epsilon_world; plan.early_reflection_ray_count];
    let max_distances = vec![max_distance_world; plan.early_reflection_ray_count];
    let mut surface_hits = vec![None; plan.early_reflection_ray_count];
    input.scene.query().trace_closest_hit_batch(
        &probe_rays,
        &min_distances,
        &max_distances,
        &mut surface_hits,
    );

    assert_eq!(
        candidates.len(),
        responses.len(),
        "each selected acoustic candidate must own one response"
    );
    for (candidate, response) in candidates
        .iter()
        .zip(responses.iter_mut())
        .take(plan.max_early_reflection_sources)
    {
        assert_eq!(
            compare_voice_ids(response.voice_id, candidate.voice.voice_id),
            CmpOrdering::Equal,
            "selected candidate and response order diverged"
        );
        let representatives = response.environment_representatives.clone();
        let aggregate_environment_gain = response.environment_gain;
        if representatives.is_empty() {
            continue;
        }
        let mut visibility_rays = Vec::with_capacity(plan.early_reflection_ray_count);
        let mut visibility_min = Vec::with_capacity(plan.early_reflection_ray_count);
        let mut visibility_max = Vec::with_capacity(plan.early_reflection_ray_count);
        let mut candidates_for_visibility = Vec::with_capacity(plan.early_reflection_ray_count);

        for (path_id, (probe_ray, hit)) in probe_rays.iter().zip(&surface_hits).enumerate() {
            let Some(hit) = hit.filter(|hit| {
                valid_hit_distance(hit.distance, ray_epsilon_world, max_distance_world)
            }) else {
                continue;
            };
            let representative = representatives[path_id % representatives.len()];
            let source_position = representative.world_position;
            let direct_distance_world = source_position.distance(listener);
            if !direct_distance_world.is_finite() {
                continue;
            }
            let hit_position = listener + probe_ray.direction * hit.distance;
            let hit_to_source = source_position - hit_position;
            let hit_to_source_distance = hit_to_source.length();
            if !hit_to_source_distance.is_finite()
                || hit_to_source_distance <= ray_epsilon_world * 2.0
            {
                continue;
            }
            let to_source = hit_to_source / hit_to_source_distance;
            let incoming = -to_source;
            let mut normal = normalize_or(hit.normal, -incoming);
            if normal.dot(incoming) > 0.0 {
                normal = -normal;
            }
            let to_listener = -probe_ray.direction;
            let reflected =
                normalize_or(incoming - 2.0 * incoming.dot(normal) * normal, to_listener);
            let specular_alignment = reflected.dot(to_listener).max(0.0).powi(8);
            let cosine_in = normal.dot(-incoming).max(0.0);
            let cosine_out = normal.dot(to_listener).max(0.0);
            let scattering = sanitize_unit(
                hit.material.scattering,
                AcousticMaterial::default().scattering,
            );
            let path_weight = (specular_alignment * (1.0 - scattering)
                + cosine_in * cosine_out * scattering * 0.25)
                .clamp(0.0, 1.0);
            if path_weight <= 1.0e-4 {
                continue;
            }
            let total_distance_world = hit.distance + hit_to_source_distance;
            let excess_distance_meters =
                (total_distance_world - direct_distance_world).max(0.0) * distance_scaler;
            let delay_seconds = excess_distance_meters / SPEED_OF_SOUND_METERS_PER_SECOND;
            if !delay_seconds.is_finite()
                || !(f32::EPSILON..=EARLY_REFLECTION_MAX_DELAY_SECONDS).contains(&delay_seconds)
            {
                continue;
            }
            let total_distance_meters = total_distance_world * distance_scaler;
            let propagation_gain = EARLY_REFLECTION_GAIN
                * path_weight
                * representative.power.sqrt()
                * propagation_air_absorption(total_distance_meters)
                * propagation_distance_attenuation(total_distance_meters);
            let gain = std::array::from_fn(|band| {
                propagation_gain
                    * aggregate_environment_gain[band]
                    * (1.0
                        - sanitize_unit(
                            hit.material.absorption[band],
                            AcousticMaterial::default().absorption[band],
                        ))
            });
            if gain.iter().all(|gain| *gain <= 1.0e-5) {
                continue;
            }

            visibility_rays.push(AcousticRay {
                origin: hit_position + to_source * ray_epsilon_world,
                direction: to_source,
            });
            visibility_min.push(0.0);
            visibility_max.push((hit_to_source_distance - ray_epsilon_world * 2.0).max(0.0));
            candidates_for_visibility.push(EarlyReflectionTap {
                path_id: ((path_id as u64 ^ representative.sample_id.0.rotate_left(17))
                    & u64::from(u16::MAX)) as u16,
                arrival_direction: probe_ray.direction,
                delay_seconds,
                gain,
            });
        }

        if visibility_rays.is_empty() {
            continue;
        }
        let mut blocked = vec![false; visibility_rays.len()];
        input.scene.query().trace_any_hit_batch(
            &visibility_rays,
            &visibility_min,
            &visibility_max,
            &mut blocked,
        );
        let mut taps: Vec<_> = candidates_for_visibility
            .into_iter()
            .zip(blocked)
            .filter_map(|(tap, blocked)| (!blocked).then_some(tap))
            .collect();
        taps.sort_by(|left, right| {
            reflection_strength(right)
                .partial_cmp(&reflection_strength(left))
                .unwrap_or(CmpOrdering::Equal)
        });
        taps.truncate(plan.early_reflection_taps);
        taps.sort_by_key(|tap| tap.path_id);
        response.early_reflections = taps;
    }
}

fn listener_to_world_pose(listener: Pose, local: Pose) -> Pose {
    Pose::new(
        listener.position
            + listener.right() * local.position.x
            + listener.up() * local.position.y
            + listener.forward() * local.position.z,
        listener.rotation * local.rotation,
    )
}

fn listener_position_relative_to_world_pose(listener: Pose, world_offset: Pose) -> Pose {
    Pose::new(
        listener.position + world_offset.position,
        world_offset.rotation,
    )
}

fn reflection_strength(tap: &EarlyReflectionTap) -> f32 {
    tap.gain.iter().sum()
}

fn propagation_distance_attenuation(distance_meters: f32) -> f32 {
    1.0 / distance_meters.max(1.0)
}

fn propagation_air_absorption(distance_meters: f32) -> f32 {
    (-0.0002 * distance_meters.max(0.0)).exp().clamp(0.2, 1.0)
}

fn solve_late_reverb(
    input: &SolveInput,
    distance_scaler: f32,
    plan: AcousticSolvePlan,
) -> LateReverbParameters {
    let listener = input.spatial.listener().position;
    let max_distance_world = MAX_TRACE_DISTANCE_METERS / distance_scaler.max(0.001);
    let ray_epsilon_world = RAY_EPSILON_METERS / distance_scaler.max(0.001);
    let mut rays: Vec<AcousticRay> = (0..plan.late_ray_count)
        .map(|index| AcousticRay {
            origin: listener,
            direction: fibonacci_direction(index, plan.late_ray_count),
        })
        .collect();
    let min_distances = vec![ray_epsilon_world; plan.late_ray_count];
    let max_distances = vec![max_distance_world; plan.late_ray_count];
    let mut hits = vec![None; plan.late_ray_count];
    let mut active = vec![true; plan.late_ray_count];
    let mut energy = vec![[1.0f32; 3]; plan.late_ray_count];
    let mut hit_segments = 0usize;
    let mut first_bounce_hits = 0usize;
    let mut minimum_hit_distance_meters = f32::INFINITY;
    let mut segment_time_sum = 0.0f32;
    let mut log_reflectivity_sum = [0.0f32; 3];
    let mut reflected_energy_sum = [0.0f32; 3];

    for bounce in 0..plan.late_bounce_count {
        hits.fill(None);
        input.scene.query().trace_closest_hit_batch(
            &rays,
            &min_distances,
            &max_distances,
            &mut hits,
        );
        for index in 0..plan.late_ray_count {
            if !active[index] {
                continue;
            }
            let Some(hit) = hits[index].filter(|hit| {
                valid_hit_distance(hit.distance, ray_epsilon_world, max_distance_world)
            }) else {
                active[index] = false;
                continue;
            };
            if bounce == 0 {
                first_bounce_hits += 1;
                minimum_hit_distance_meters =
                    minimum_hit_distance_meters.min(hit.distance.max(0.0) * distance_scaler);
            }
            hit_segments += 1;
            segment_time_sum +=
                hit.distance.max(0.0) * distance_scaler / SPEED_OF_SOUND_METERS_PER_SECOND;
            for band in 0..3 {
                let absorption = sanitize_unit(
                    hit.material.absorption[band],
                    AcousticMaterial::default().absorption[band],
                );
                let reflectivity = (1.0 - absorption).clamp(0.01, 0.999);
                log_reflectivity_sum[band] += reflectivity.ln();
                energy[index][band] *= reflectivity;
                reflected_energy_sum[band] += energy[index][band];
            }
            if energy[index].iter().all(|value| *value < 1.0e-4) {
                active[index] = false;
                continue;
            }

            let normal = normalize_or(hit.normal, -rays[index].direction);
            let specular = rays[index].direction - 2.0 * rays[index].direction.dot(normal) * normal;
            let diffuse = deterministic_hemisphere(index, bounce, normal);
            let scattering = sanitize_unit(
                hit.material.scattering,
                AcousticMaterial::default().scattering,
            );
            let direction = normalize_or(specular.lerp(diffuse, scattering), specular);
            rays[index] = AcousticRay {
                origin: rays[index].origin
                    + rays[index].direction * hit.distance
                    + normal * ray_epsilon_world,
                direction,
            };
        }
    }

    if hit_segments == 0 || first_bounce_hits == 0 {
        return LateReverbParameters::SILENT;
    }
    let mean_segment_time = segment_time_sum / hit_segments as f32;
    let rt60_seconds = std::array::from_fn(|band| {
        let mean_log_reflectivity = log_reflectivity_sum[band] / hit_segments as f32;
        if mean_log_reflectivity >= -f32::EPSILON {
            20.0
        } else {
            (-6.0 * mean_segment_time / (mean_log_reflectivity / std::f32::consts::LN_10))
                .clamp(0.05, 20.0)
        }
    });
    let enclosure = first_bounce_hits as f32 / plan.late_ray_count as f32;
    let reflected_energy =
        reflected_energy_sum.iter().sum::<f32>() / (hit_segments.max(1) * 3) as f32;
    LateReverbParameters {
        pre_delay_seconds: (minimum_hit_distance_meters * 2.0 / SPEED_OF_SOUND_METERS_PER_SECOND)
            .clamp(0.0, 0.25),
        rt60_seconds,
        wet_gain: (enclosure * reflected_energy * 0.35).clamp(0.0, 0.35),
    }
}

fn fibonacci_direction(index: usize, count: usize) -> Vec3 {
    let y = 1.0 - 2.0 * (index as f32 + 0.5) / count as f32;
    let radius = (1.0 - y * y).max(0.0).sqrt();
    let angle = std::f32::consts::TAU * index as f32 * 0.618_034;
    Vec3::new(radius * angle.cos(), y, radius * angle.sin())
}

fn deterministic_hemisphere(index: usize, bounce: usize, normal: Vec3) -> Vec3 {
    let seed = (index as u32)
        .wrapping_mul(0x9e37_79b9)
        .wrapping_add((bounce as u32).wrapping_mul(0x85eb_ca6b));
    let u = hash_unit(seed);
    let v = hash_unit(seed ^ 0xc2b2_ae35);
    let radius = u.sqrt();
    let angle = std::f32::consts::TAU * v;
    let local = Vec3::new(radius * angle.cos(), (1.0 - u).sqrt(), radius * angle.sin());
    let tangent = normalize_or(
        if normal.y.abs() < 0.9 {
            normal.cross(Vec3::Y)
        } else {
            normal.cross(Vec3::X)
        },
        Vec3::X,
    );
    let bitangent = normal.cross(tangent);
    normalize_or(
        tangent * local.x + normal * local.y + bitangent * local.z,
        normal,
    )
}

fn hash_unit(mut value: u32) -> f32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846c_a68b);
    value ^= value >> 16;
    value as f32 / u32::MAX as f32
}

fn normalize_or(value: Vec3, fallback: Vec3) -> Vec3 {
    if value.is_finite() && value.length_squared() > f32::EPSILON {
        value.normalize()
    } else {
        fallback
    }
}

fn valid_hit_distance(distance: f32, min_distance: f32, max_distance: f32) -> bool {
    distance.is_finite()
        && distance >= min_distance
        && distance <= max_distance
        && max_distance > 0.0
}

fn sanitize_unit(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        fallback
    }
}

fn histogram_bucket(value: u64) -> usize {
    if value == 0 {
        0
    } else {
        ((u64::BITS - (value - 1).leading_zeros()) as usize + 1).min(63)
    }
}

fn histogram_percentile(histogram: &[AtomicU64; 64], total: u64, percentile: u64) -> u64 {
    if total == 0 {
        return 0;
    }
    let target = total.saturating_mul(percentile).div_ceil(100);
    let mut cumulative = 0;
    for (index, bucket) in histogram.iter().enumerate() {
        cumulative += bucket.load(Ordering::Relaxed);
        if cumulative >= target {
            return match index {
                0 => 0,
                63 => u64::MAX,
                _ => 1u64 << (index - 1),
            };
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acoustics::{AcousticHit, AcousticMaterial, AcousticRayQuerySnapshot};
    use crate::domain::EmitterSpatialState;
    use crate::math::Pose;
    use std::sync::atomic::{AtomicU8, AtomicUsize};

    fn direct_response(voice_id: u64, routing_generation: u64) -> DirectAcousticResponse {
        DirectAcousticResponse {
            voice_id: VoiceId::from(voice_id),
            routing_generation,
            gain: [voice_id as f32; 3],
            environment_gain: [1.0; 3],
            direct_lobes: Vec::new(),
            environment_representatives: Vec::new(),
            early_reflections: Vec::new(),
            solve_status: DirectSolveStatus::Solved,
            cache_age_seconds: 0.0,
        }
    }

    #[test]
    fn render_publication_lookup_is_bounded_for_4096_voices() {
        const VOICES: usize = 4_096;
        const MAX_BINARY_COMPARISONS_PER_LOOKUP: usize = 13;
        let response = AcousticResponse {
            spatial_revision: 3,
            geometry_version: 5,
            direct: (0..VOICES)
                .map(|voice| direct_response(voice as u64, voice as u64 + 1))
                .collect::<Vec<_>>()
                .into(),
            late_reverb: LateReverbParameters::SILENT,
            published_at: Instant::now(),
            solve_time_us: 0,
        };

        let comparisons = crate::test_support::acoustic_publication_comparison_activity(|| {
            for voice in (0..VOICES).rev() {
                assert_eq!(
                    response
                        .voice_response(VoiceId::from(voice as u64))
                        .unwrap()
                        .direct_gain(),
                    [voice as f32; 3]
                );
            }
            assert!(
                response
                    .voice_response(VoiceId::from(VOICES as u64))
                    .is_none()
            );
        });

        assert!(
            comparisons <= (VOICES + 1) * MAX_BINARY_COMPARISONS_PER_LOOKUP,
            "4096-Voice render publication performed {comparisons} comparisons"
        );
    }

    #[test]
    fn render_publication_normalizes_unordered_voice_responses() {
        let response = AcousticResponse {
            spatial_revision: 3,
            geometry_version: 5,
            direct: [4, 1, 3, 2]
                .into_iter()
                .map(|voice| direct_response(voice, voice + 10))
                .collect::<Vec<_>>()
                .into(),
            late_reverb: LateReverbParameters::SILENT,
            published_at: Instant::now(),
            solve_time_us: 0,
        };

        for voice in 1..=4 {
            assert_eq!(
                response
                    .voice_response(VoiceId::from(voice))
                    .unwrap()
                    .direct_gain(),
                [voice as f32; 3]
            );
        }
    }

    #[test]
    #[should_panic(
        expected = "one acoustic render publication cannot contain duplicate Voice identities"
    )]
    fn render_publication_rejects_duplicate_voice_identity() {
        let _: AcousticVoiceResponses = vec![direct_response(7, 1), direct_response(7, 2)].into();
    }

    #[test]
    #[should_panic(expected = "one acoustic publication cannot contain duplicate Voice identities")]
    fn completed_publication_rejects_duplicate_active_voice_identity() {
        let mut workload = input(Arc::new(NoGeometry));
        let mut duplicate = workload.voices[0].clone();
        duplicate.routing_generation += 1;
        workload.voices.push(duplicate);
        let plan = AcousticSolvePlan {
            max_direct_sources: 0,
            max_direct_rays: 0,
            max_early_reflection_sources: 0,
            early_reflection_taps: 0,
            early_reflection_ray_count: 0,
            late_ray_count: 0,
            late_bounce_count: 0,
        };

        let _ = AcousticSolver::new(2).solve_completed(&workload, 1.0, plan);
    }

    #[test]
    fn completed_voice_publication_association_is_bounded_for_4096_voices() {
        const VOICES: usize = 4_096;
        const MAX_LOGARITHMIC_COMPARISONS_PER_VOICE: usize = 40;
        let mut workload = input(Arc::new(NoGeometry));
        let template = workload.voices[0].clone();
        workload.voices = (0..VOICES)
            .map(|voice| {
                let mut voice_input = template.clone();
                voice_input.voice_id = VoiceId::from(voice as u64 + 1);
                voice_input.routing_generation = voice as u64 + 11;
                voice_input.emitter.index = voice as u32;
                voice_input
            })
            .collect();
        let plan = AcousticSolvePlan {
            max_direct_sources: 0,
            max_direct_rays: 0,
            max_early_reflection_sources: 0,
            early_reflection_taps: 0,
            early_reflection_ray_count: 0,
            late_ray_count: 0,
            late_bounce_count: 0,
        };
        let mut output = None;

        let comparisons = crate::test_support::acoustic_publication_comparison_activity(|| {
            output = Some(AcousticSolver::new(VOICES).solve_with_telemetry(&workload, 1.0, plan));
        });
        let output = output.unwrap();

        assert_eq!(output.response.direct.len(), VOICES);
        assert_eq!(output.telemetry.len(), VOICES);
        assert_eq!(output.conclusions.len(), VOICES);
        for voice in [1, VOICES as u64 / 2, VOICES as u64] {
            let response = output
                .response
                .voice_response(VoiceId::from(voice))
                .unwrap();
            assert_eq!(response.direct_gain_target(), None);
            let telemetry = &output.telemetry[voice as usize - 1];
            assert_eq!(telemetry.voice_id, voice);
            assert_eq!(telemetry.solve_status, AcousticSolveStatus::Deferred);
            let conclusion = &output.conclusions[voice as usize - 1].telemetry;
            assert_eq!(conclusion.voice_id, voice);
            assert_eq!(conclusion.solve_status, Some(AcousticSolveStatus::Deferred));
        }
        assert!(
            comparisons <= VOICES * MAX_LOGARITHMIC_COMPARISONS_PER_VOICE,
            "4096-Voice completion association performed {comparisons} comparisons"
        );
    }

    #[test]
    fn early_reflection_response_association_is_bounded_for_4096_voices() {
        const VOICES: usize = 4_096;
        const MAX_SOLVE_COMPARISONS_PER_VOICE: usize = 64;
        let mut workload = input(Arc::new(NoGeometry));
        let template = workload.voices[0].clone();
        workload.voices = (0..VOICES)
            .map(|voice| {
                let mut voice_input = template.clone();
                voice_input.voice_id = VoiceId::from(voice as u64 + 1);
                voice_input.routing_generation = voice as u64 + 11;
                voice_input.emitter.index = voice as u32;
                voice_input
            })
            .collect();
        let plan = AcousticSolvePlan {
            max_direct_sources: VOICES,
            max_direct_rays: VOICES * 2,
            max_early_reflection_sources: VOICES,
            early_reflection_taps: 0,
            early_reflection_ray_count: 0,
            late_ray_count: 0,
            late_bounce_count: 0,
        };
        let mut completed = None;

        let comparisons = crate::test_support::acoustic_publication_comparison_activity(|| {
            completed = Some(AcousticSolver::new(VOICES).solve_completed(&workload, 1.0, plan));
        });

        assert_eq!(completed.unwrap().voices.len(), VOICES);
        assert!(
            comparisons <= VOICES * MAX_SOLVE_COMPARISONS_PER_VOICE,
            "4096-Voice early-reflection association performed {comparisons} comparisons"
        );
    }

    #[test]
    fn compatibility_retain_is_bounded_and_keeps_voice_envelopes_grouped() {
        const VOICES: usize = 4_096;
        const REROUTED: usize = 16;
        const RETIRED: usize = 16;
        const MAX_LOGARITHMIC_COMPARISONS_PER_VOICE: usize = 32;
        let mut captured = input(Arc::new(NoGeometry));
        let template = captured.voices[0].clone();
        captured.voices = (0..VOICES)
            .map(|voice| {
                let mut voice_input = template.clone();
                voice_input.voice_id = VoiceId::from(voice as u64 + 1);
                voice_input.routing_generation = voice as u64 + 11;
                voice_input.emitter.index = voice as u32;
                voice_input
            })
            .collect();
        let plan = AcousticSolvePlan {
            max_direct_sources: 0,
            max_direct_rays: 0,
            max_early_reflection_sources: 0,
            early_reflection_taps: 0,
            early_reflection_ray_count: 0,
            late_ray_count: 0,
            late_bounce_count: 0,
        };
        let completed = AcousticSolver::new(VOICES).solve_completed(&captured, 1.0, plan);
        let mut current = InputState::new(0.5, VOICES);
        current.scene = Some(captured.scene.clone());
        current.spatial = Some(Arc::new(
            SpatialFrame::new(
                captured.spatial.revision() + 99,
                captured.spatial.sim_time_seconds() + 1.0,
                Pose::from_position(Vec3::X),
                Vec::new(),
            )
            .unwrap(),
        ));
        current.voices = captured.voices.to_vec();
        for voice in current
            .voices
            .iter_mut()
            .filter(|voice| voice.voice_id.value() <= REROUTED as u64)
        {
            voice.routing_generation += 10_000;
        }
        current
            .voices
            .retain(|voice| voice.voice_id.value() <= (VOICES - RETIRED) as u64);
        current.voices.reverse();
        let mut retained = None;

        let comparisons = crate::test_support::acoustic_publication_comparison_activity(|| {
            retained = completed.retain_compatible(&current);
        });
        let retained = retained.unwrap().into_solve_output(Instant::now());
        let expected = VOICES - REROUTED - RETIRED;

        assert_eq!(retained.response.direct.len(), expected);
        assert_eq!(retained.telemetry.len(), expected);
        assert_eq!(retained.conclusions.len(), expected);
        assert_eq!(
            retained.response.spatial_revision,
            captured.spatial.revision()
        );
        assert!(
            retained
                .response
                .voice_response(VoiceId::from(REROUTED as u64))
                .is_none()
        );
        assert_eq!(
            retained.response.direct.first().unwrap().voice_id,
            VoiceId::from(REROUTED as u64 + 1)
        );
        assert_eq!(
            retained.response.direct.last().unwrap().voice_id,
            VoiceId::from((VOICES - RETIRED) as u64)
        );
        for index in [0, expected / 2, expected - 1] {
            assert_eq!(
                retained.response.direct[index].voice_id.value(),
                retained.telemetry[index].voice_id
            );
            assert_eq!(
                retained.response.direct[index].voice_id.value(),
                retained.conclusions[index].telemetry.voice_id
            );
        }
        assert!(
            comparisons <= VOICES * MAX_LOGARITHMIC_COMPARISONS_PER_VOICE,
            "4096-Voice compatibility retain performed {comparisons} comparisons"
        );
    }

    fn start_propagation(
        enabled: bool,
        quality: f32,
        max_voices: usize,
        telemetry_capacity: usize,
    ) -> (AcousticPropagation, RuntimeServices) {
        let state = Arc::new(AtomicU8::new(crate::events::RuntimeState::Recovering as u8));
        let mut services =
            RuntimeServices::new(crate::runtime_health::RuntimeFailurePublisher::new(state));
        let (propagation, worker) = AcousticPropagation::prepare(
            1.0,
            Arc::new(AtomicBool::new(enabled)),
            quality,
            EnvironmentalAcousticsBudget::default(),
            max_voices,
            telemetry_capacity,
        );
        worker.start(&mut services).unwrap();
        (propagation, services)
    }

    #[test]
    fn listener_position_relative_acoustic_pose_follows_translation_in_world_axes() {
        let listener = Pose::new(
            Vec3::new(10.0, 20.0, 30.0),
            crate::math::Quat::from_rotation_x(-0.8),
        );
        let world_offset = Pose::new(
            Vec3::new(0.0, -0.08, 0.0),
            crate::math::Quat::from_rotation_y(0.4),
        );

        let resolved = listener_position_relative_to_world_pose(listener, world_offset);

        assert_eq!(resolved.position, Vec3::new(10.0, 19.92, 30.0));
        assert_eq!(resolved.rotation, world_offset.rotation);
    }

    struct NoGeometry;

    impl AcousticRayQuerySnapshot for NoGeometry {
        fn trace_any_hit_batch(
            &self,
            _rays: &[AcousticRay],
            _min_distances: &[f32],
            _max_distances: &[f32],
            hits: &mut [bool],
        ) {
            hits.fill(false);
        }

        fn trace_closest_hit_batch(
            &self,
            _rays: &[AcousticRay],
            _min_distances: &[f32],
            _max_distances: &[f32],
            hits: &mut [Option<AcousticHit>],
        ) {
            hits.fill(None);
        }
    }

    struct BlockingOpenGeometry {
        first_call: AtomicBool,
        entered: Mutex<bool>,
        entered_changed: Condvar,
        released: Mutex<bool>,
        released_changed: Condvar,
    }

    impl BlockingOpenGeometry {
        fn new() -> Self {
            Self {
                first_call: AtomicBool::new(true),
                entered: Mutex::new(false),
                entered_changed: Condvar::new(),
                released: Mutex::new(false),
                released_changed: Condvar::new(),
            }
        }

        fn wait_until_entered(&self) {
            let entered = self.entered.lock().unwrap();
            let (_entered, timeout) = self
                .entered_changed
                .wait_timeout_while(entered, Duration::from_secs(1), |entered| !*entered)
                .unwrap();
            assert!(
                !timeout.timed_out(),
                "worker never entered the controlled solve"
            );
        }

        fn release(&self) {
            *self.released.lock().unwrap() = true;
            self.released_changed.notify_all();
        }
    }

    impl AcousticRayQuerySnapshot for BlockingOpenGeometry {
        fn trace_any_hit_batch(
            &self,
            _rays: &[AcousticRay],
            _min_distances: &[f32],
            _max_distances: &[f32],
            hits: &mut [bool],
        ) {
            hits.fill(false);
        }

        fn trace_closest_hit_batch(
            &self,
            _rays: &[AcousticRay],
            _min_distances: &[f32],
            _max_distances: &[f32],
            hits: &mut [Option<AcousticHit>],
        ) {
            if self.first_call.swap(false, Ordering::AcqRel) {
                *self.entered.lock().unwrap() = true;
                self.entered_changed.notify_all();
                let released = self.released.lock().unwrap();
                drop(
                    self.released_changed
                        .wait_while(released, |released| !*released)
                        .unwrap(),
                );
            }
            hits.fill(None);
        }
    }

    #[derive(Default)]
    struct CountingOpenGeometry {
        traced_closest_rays: AtomicUsize,
    }

    impl AcousticRayQuerySnapshot for CountingOpenGeometry {
        fn trace_any_hit_batch(
            &self,
            _rays: &[AcousticRay],
            _min_distances: &[f32],
            _max_distances: &[f32],
            hits: &mut [bool],
        ) {
            hits.fill(false);
        }

        fn trace_closest_hit_batch(
            &self,
            rays: &[AcousticRay],
            _min_distances: &[f32],
            _max_distances: &[f32],
            hits: &mut [Option<AcousticHit>],
        ) {
            self.traced_closest_rays
                .fetch_add(rays.len(), Ordering::Relaxed);
            hits.fill(None);
        }
    }

    struct UnitRoom;

    impl AcousticRayQuerySnapshot for UnitRoom {
        fn trace_any_hit_batch(
            &self,
            rays: &[AcousticRay],
            _min_distances: &[f32],
            _max_distances: &[f32],
            hits: &mut [bool],
        ) {
            hits[..rays.len()].fill(true);
        }

        fn trace_closest_hit_batch(
            &self,
            rays: &[AcousticRay],
            _min_distances: &[f32],
            max_distances: &[f32],
            hits: &mut [Option<AcousticHit>],
        ) {
            for ((ray, max_distance), hit) in rays
                .iter()
                .zip(max_distances.iter().copied())
                .zip(hits.iter_mut())
            {
                let distance = 1.0f32.min(max_distance);
                *hit = (distance > 0.0).then_some(AcousticHit {
                    distance,
                    normal: -ray.direction,
                    material: AcousticMaterial {
                        absorption: [0.2, 0.3, 0.5],
                        scattering: 0.2,
                        transmission: [0.1, 0.05, 0.02],
                    },
                });
            }
        }
    }

    struct InvalidMaterialRoom;

    impl AcousticRayQuerySnapshot for InvalidMaterialRoom {
        fn trace_any_hit_batch(
            &self,
            rays: &[AcousticRay],
            _min_distances: &[f32],
            _max_distances: &[f32],
            hits: &mut [bool],
        ) {
            hits[..rays.len()].fill(true);
        }

        fn trace_closest_hit_batch(
            &self,
            rays: &[AcousticRay],
            min_distances: &[f32],
            max_distances: &[f32],
            hits: &mut [Option<AcousticHit>],
        ) {
            for (((ray, min_distance), max_distance), hit) in rays
                .iter()
                .zip(min_distances.iter().copied())
                .zip(max_distances.iter().copied())
                .zip(hits.iter_mut())
            {
                *hit = Some(AcousticHit {
                    distance: (min_distance + max_distance) * 0.5,
                    normal: ray.direction * f32::NAN,
                    material: AcousticMaterial {
                        absorption: [f32::NAN, -1.0, 2.0],
                        scattering: f32::NAN,
                        transmission: [f32::NAN, -1.0, 2.0],
                    },
                });
            }
        }
    }

    struct DirectionalTransmission;

    impl AcousticRayQuerySnapshot for DirectionalTransmission {
        fn trace_any_hit_batch(
            &self,
            _rays: &[AcousticRay],
            _min_distances: &[f32],
            _max_distances: &[f32],
            hits: &mut [bool],
        ) {
            hits.fill(false);
        }

        fn trace_closest_hit_batch(
            &self,
            rays: &[AcousticRay],
            min_distances: &[f32],
            max_distances: &[f32],
            hits: &mut [Option<AcousticHit>],
        ) {
            for (((ray, min_distance), max_distance), hit) in rays
                .iter()
                .zip(min_distances.iter().copied())
                .zip(max_distances.iter().copied())
                .zip(hits.iter_mut())
            {
                let distance = (min_distance + max_distance) * 0.5;
                let transmission = if ray.direction.x >= 0.0 { 0.2 } else { 0.8 };
                *hit = valid_hit_distance(distance, min_distance, max_distance).then_some(
                    AcousticHit {
                        distance,
                        normal: -ray.direction,
                        material: AcousticMaterial {
                            transmission: [transmission; 3],
                            ..AcousticMaterial::default()
                        },
                    },
                );
            }
        }
    }

    struct NegativeXWood;

    impl AcousticRayQuerySnapshot for NegativeXWood {
        fn trace_any_hit_batch(
            &self,
            _rays: &[AcousticRay],
            _min_distances: &[f32],
            _max_distances: &[f32],
            hits: &mut [bool],
        ) {
            hits.fill(false);
        }

        fn trace_closest_hit_batch(
            &self,
            rays: &[AcousticRay],
            min_distances: &[f32],
            max_distances: &[f32],
            hits: &mut [Option<AcousticHit>],
        ) {
            for (((ray, min_distance), max_distance), hit) in rays
                .iter()
                .zip(min_distances.iter().copied())
                .zip(max_distances.iter().copied())
                .zip(hits.iter_mut())
            {
                *hit = (ray.direction.x < 0.0).then_some(AcousticHit {
                    distance: (min_distance + max_distance) * 0.5,
                    normal: -ray.direction,
                    material: AcousticMaterial {
                        transmission: [0.1; 3],
                        ..AcousticMaterial::default()
                    },
                });
            }
        }
    }

    struct ReflectiveFloor {
        blocks_visibility: bool,
    }

    impl AcousticRayQuerySnapshot for ReflectiveFloor {
        fn trace_any_hit_batch(
            &self,
            rays: &[AcousticRay],
            _min_distances: &[f32],
            _max_distances: &[f32],
            hits: &mut [bool],
        ) {
            hits[..rays.len()].fill(self.blocks_visibility);
        }

        fn trace_closest_hit_batch(
            &self,
            rays: &[AcousticRay],
            min_distances: &[f32],
            max_distances: &[f32],
            hits: &mut [Option<AcousticHit>],
        ) {
            for (((ray, min_distance), max_distance), hit) in rays
                .iter()
                .zip(min_distances.iter().copied())
                .zip(max_distances.iter().copied())
                .zip(hits.iter_mut())
            {
                let distance = (-1.0 - ray.origin.y) / ray.direction.y;
                *hit = (ray.direction.y < -1.0e-4
                    && valid_hit_distance(distance, min_distance, max_distance))
                .then_some(AcousticHit {
                    distance,
                    normal: Vec3::Y,
                    material: AcousticMaterial {
                        absorption: [0.1, 0.3, 0.6],
                        scattering: 0.8,
                        transmission: [0.05; 3],
                    },
                });
            }
        }
    }

    fn input(query: Arc<dyn AcousticRayQuerySnapshot>) -> SolveInput {
        let emitter = Emitter {
            world_id: 1,
            index: 0,
            generation: 1,
        };
        SolveInput {
            wake_generation: 1,
            spatial: Arc::new(
                SpatialFrame::new(
                    11,
                    1.5,
                    Pose::from_position(Vec3::ZERO),
                    vec![EmitterSpatialState::new(
                        emitter,
                        Pose::from_position(Vec3::Z),
                    )],
                )
                .unwrap(),
            ),
            scene: Arc::new(AcousticSceneSnapshot::new(17, query)),
            voices: vec![AcousticVoiceSnapshot {
                voice_id: VoiceId::from(1),
                routing_generation: 1,
                emitter,
                emitter_world_pose: Pose::from_position(Vec3::Z),
                acoustic_priority: 1.0,
                audibility: 1.0,
                detached: false,
                direct_path: DirectPath::default(),
                environment_send: EnvironmentSend::default(),
                source_extent: SourceExtent::Point,
                occlusion_profile: OcclusionProfile::PointExact,
            }],
            environmental_acoustics_quality: 0.5,
        }
    }

    fn eight_sample_extent() -> SourceExtent {
        use crate::domain::{ExtentSample, ExtentSampleId};

        SourceExtent::weighted_samples(
            [-1.75_f32, -1.25, -0.75, -0.25, 0.25, 0.75, 1.25, 1.75]
                .into_iter()
                .enumerate()
                .map(|(id, x)| {
                    ExtentSample::new(ExtentSampleId(id as u64), Vec3::new(x, 0.0, 2.0), 1.0)
                        .unwrap()
                })
                .collect(),
        )
        .unwrap()
    }

    fn fixture_gain_at_listener(listener_x: f32, revision: u64) -> f32 {
        let mut fixture = input(Arc::new(NegativeXWood));
        fixture.spatial = Arc::new(
            SpatialFrame::new(
                revision,
                revision as f64 * 0.1,
                Pose::from_position(Vec3::new(listener_x, 0.0, 0.0)),
                vec![EmitterSpatialState::new(
                    fixture.voices[0].emitter,
                    Pose::identity(),
                )],
            )
            .unwrap(),
        );
        fixture.voices[0].emitter_world_pose = Pose::identity();
        fixture.voices[0].source_extent = eight_sample_extent();
        fixture.voices[0].environment_send = EnvironmentSend::disabled();
        let plan = AcousticSolvePlan {
            max_early_reflection_sources: 0,
            early_reflection_taps: 0,
            early_reflection_ray_count: 0,
            late_ray_count: 0,
            late_bounce_count: 0,
            ..AcousticSolvePlan::for_quality(0.5)
        };
        AcousticSolver::new(1)
            .solve_with_plan(&fixture, 1.0, plan)
            .direct[0]
            .gain[0]
    }

    #[test]
    fn fixed_extended_source_trajectory_and_budget_fixture_is_deterministic() {
        let listener_path = [-2.0_f32, -1.0, -0.5, 0.0, 0.5, 1.0, 2.0];
        let forward = listener_path
            .into_iter()
            .enumerate()
            .map(|(index, x)| fixture_gain_at_listener(x, index as u64 + 1))
            .collect::<Vec<_>>();
        let reverse = listener_path
            .into_iter()
            .rev()
            .enumerate()
            .map(|(index, x)| fixture_gain_at_listener(x, index as u64 + 20))
            .collect::<Vec<_>>();
        for (forward_gain, reverse_gain) in forward.iter().zip(reverse.iter().rev()) {
            assert!((forward_gain - reverse_gain).abs() < 1.0e-6);
        }
        assert!(forward.windows(2).all(|pair| pair[1] <= pair[0] + 1.0e-6));

        let static_left = fixture_gain_at_listener(-1.0e-4, 40);
        let static_right = fixture_gain_at_listener(1.0e-4, 41);
        assert!((static_left - static_right).abs() < 1.0e-6);
        let coarse_half_gain = fixture_gain_at_listener(0.0, 42);
        assert!((coarse_half_gain - 0.710_633_5).abs() < 1.0e-6);

        let mut budget_input = input(Arc::new(NoGeometry));
        budget_input.voices[0].source_extent = eight_sample_extent();
        budget_input.voices[0].environment_send = EnvironmentSend::disabled();
        budget_input.voices = (0..10)
            .map(|index| {
                let mut voice = budget_input.voices[0].clone();
                voice.voice_id = VoiceId::from(index as u64);
                voice.emitter = Emitter {
                    world_id: 1,
                    index,
                    generation: 1,
                };
                voice
            })
            .collect();
        let output = AcousticSolver::new(10).solve_with_telemetry(
            &budget_input,
            1.0,
            AcousticSolvePlan {
                max_direct_sources: 8,
                max_direct_rays: 64,
                max_early_reflection_sources: 0,
                early_reflection_taps: 0,
                early_reflection_ray_count: 0,
                late_ray_count: 0,
                late_bounce_count: 0,
            },
        );
        let solved = output
            .telemetry
            .iter()
            .filter(|event| event.solve_status == AcousticSolveStatus::Solved)
            .count();
        let deferred = output
            .telemetry
            .iter()
            .filter(|event| event.solve_status == AcousticSolveStatus::Deferred)
            .count();
        assert_eq!((solved, deferred), (8, 2));

        println!(
            "PETALSONIC_EXTENDED_FIXTURE_METRICS {{\"forward_gain\":{forward:?},\"reverse_gain\":{reverse:?},\"static_span\":{},\"coarse_half_gain\":{coarse_half_gain},\"budget_solved\":{solved},\"budget_deferred\":{deferred}}}",
            (static_left - static_right).abs(),
        );
    }

    #[test]
    fn publication_filters_retired_and_rerouted_voices_without_dropping_weighted_extent() {
        let mut captured = input(Arc::new(NoGeometry));
        captured.voices[0].source_extent = eight_sample_extent();
        for (voice_id, routing_generation) in [(2, 2), (3, 3)] {
            let mut voice = captured.voices[0].clone();
            voice.voice_id = VoiceId::from(voice_id);
            voice.routing_generation = routing_generation;
            captured.voices.push(voice);
        }
        let completed = AcousticSolver::new(3).solve_completed(
            &captured,
            1.0,
            AcousticSolvePlan::for_quality(0.5),
        );
        let mut current = InputState::new(0.5, 3);
        current.scene = Some(captured.scene.clone());
        current.spatial = Some(Arc::new(
            SpatialFrame::new(
                captured.spatial.revision() + 12,
                captured.spatial.sim_time_seconds() + 0.05,
                Pose::from_position(Vec3::X),
                Vec::new(),
            )
            .unwrap(),
        ));
        current.voices = captured.voices.to_vec();
        current
            .voices
            .retain(|voice| voice.voice_id != VoiceId::from(3));
        let rerouted = current
            .voices
            .iter_mut()
            .find(|voice| voice.voice_id == VoiceId::from(2))
            .unwrap();
        rerouted.routing_generation = 20;
        rerouted.environment_send = EnvironmentSend::from_world_pose(Pose::from_position(-Vec3::Z));

        let filtered = retain_compatible_completed_results(&current, completed).unwrap();

        assert_eq!(
            filtered.response.spatial_revision,
            captured.spatial.revision()
        );
        assert_eq!(filtered.response.direct.len(), 1);
        assert_eq!(filtered.response.direct[0].voice_id, VoiceId::from(1));
        assert_eq!(filtered.telemetry.len(), 1);
        assert_eq!(filtered.telemetry[0].voice_id, 1);
        assert_eq!(
            filtered.telemetry[0].spatial_revision,
            captured.spatial.revision()
        );
        assert_eq!(
            filtered.telemetry[0].response_spatial_revision,
            captured.spatial.revision()
        );
        assert_eq!(filtered.conclusions.len(), 1);
        assert_eq!(filtered.conclusions[0].telemetry.voice_id, 1);
        assert_eq!(
            filtered.conclusions[0].telemetry.spatial_revision,
            captured.spatial.revision()
        );
        assert_eq!(
            filtered.telemetry[0].extent_sample_count,
            MAX_EXTENT_SAMPLES
        );
        assert_eq!(
            filtered.telemetry[0].direct.samples.len(),
            MAX_EXTENT_SAMPLES
        );
        assert_eq!(
            filtered.telemetry[0].environment.samples.len(),
            MAX_EXTENT_SAMPLES
        );
    }

    #[test]
    fn scene_change_rejects_completed_publication() {
        let captured = input(Arc::new(NoGeometry));
        let completed = AcousticSolver::new(1).solve_completed(
            &captured,
            1.0,
            AcousticSolvePlan::for_quality(0.5),
        );
        let mut current = InputState::new(0.5, 1);
        current.voices = captured.voices.to_vec();
        current.spatial = Some(captured.spatial.clone());
        current.scene = Some(Arc::new(AcousticSceneSnapshot::new(
            captured.scene.version() + 1,
            Arc::new(NoGeometry),
        )));

        assert!(retain_compatible_completed_results(&current, completed).is_none());
    }

    #[test]
    #[ignore = "release-mode extended-source worker performance probe"]
    fn extended_source_worker_release_budget() {
        use std::hint::black_box;

        const EXTENTS: usize = 8;
        const SAMPLES: usize = 8;
        const ITERATIONS: usize = 512;
        let query = Arc::new(CountingOpenGeometry::default());
        let mut workload = input(query.clone());
        workload.voices = (0..EXTENTS)
            .map(|index| {
                let mut voice = workload.voices[0].clone();
                voice.voice_id = VoiceId::from(index as u64);
                voice.emitter = Emitter {
                    world_id: 1,
                    index: index as u32,
                    generation: 1,
                };
                voice.emitter_world_pose =
                    Pose::from_position(Vec3::new(index as f32 * 0.5 - 1.75, 0.0, 4.0));
                voice.source_extent = eight_sample_extent();
                voice.occlusion_profile = OcclusionProfile::AmbientDistributed(
                    crate::domain::DistributedOcclusionProfile::default(),
                );
                voice
            })
            .collect();
        let plan = AcousticSolvePlan {
            max_direct_sources: EXTENTS,
            max_direct_rays: EXTENTS * SAMPLES * 2,
            max_early_reflection_sources: 0,
            early_reflection_taps: 0,
            early_reflection_ray_count: 0,
            late_ray_count: 0,
            late_bounce_count: 0,
        };
        let mut solver = AcousticSolver::new(EXTENTS);
        let mut elapsed_us = Vec::with_capacity(ITERATIONS);
        let mut observed_rays = 0usize;
        for iteration in 0..ITERATIONS {
            workload.spatial = Arc::new(
                SpatialFrame::new(
                    iteration as u64 + 100,
                    iteration as f64 / 60.0,
                    Pose::from_position(Vec3::new((iteration as f32 * 0.01).sin(), 0.0, 0.0)),
                    Vec::new(),
                )
                .unwrap(),
            );
            let started = Instant::now();
            let output = solver.solve_with_telemetry(black_box(&workload), 1.0, plan);
            elapsed_us.push(started.elapsed().as_micros() as u64);
            observed_rays = output
                .telemetry
                .iter()
                .map(|event| event.direct.ray_count + event.environment.ray_count)
                .sum();
            black_box(output);
        }
        elapsed_us.sort_unstable();
        assert_eq!(observed_rays, EXTENTS * SAMPLES * 2);
        assert_eq!(
            query.traced_closest_rays.load(Ordering::Relaxed),
            ITERATIONS * observed_rays,
        );
        println!(
            "PETALSONIC_EXTENDED_WORKER_METRICS {{\"extents\":{EXTENTS},\"samples_per_extent\":{SAMPLES},\"rays_per_solve\":{observed_rays},\"iterations\":{ITERATIONS},\"p50_us\":{},\"p95_us\":{},\"p99_us\":{},\"max_us\":{}}}",
            elapsed_us[ITERATIONS / 2],
            elapsed_us[ITERATIONS * 95 / 100],
            elapsed_us[ITERATIONS * 99 / 100],
            elapsed_us[ITERATIONS - 1],
        );
    }

    #[test]
    fn quality_plans_preserve_the_existing_default_and_scale_monotonically() {
        assert_eq!(
            AcousticSolvePlan::for_quality(0.0),
            AcousticSolvePlan {
                max_direct_sources: 32,
                max_direct_rays: 128,
                max_early_reflection_sources: 4,
                early_reflection_taps: 1,
                early_reflection_ray_count: 32,
                late_ray_count: 128,
                late_bounce_count: 4,
            }
        );
        assert_eq!(
            AcousticSolvePlan::for_quality(0.5),
            AcousticSolvePlan {
                max_direct_sources: 32,
                max_direct_rays: 256,
                max_early_reflection_sources: 8,
                early_reflection_taps: 2,
                early_reflection_ray_count: 64,
                late_ray_count: 256,
                late_bounce_count: 8,
            }
        );
        assert_eq!(
            AcousticSolvePlan::for_quality(1.0),
            AcousticSolvePlan {
                max_direct_sources: 64,
                max_direct_rays: 1_024,
                max_early_reflection_sources: 8,
                early_reflection_taps: 2,
                early_reflection_ray_count: 256,
                late_ray_count: 1_024,
                late_bounce_count: 12,
            }
        );
        let bounded =
            AcousticSolvePlan::for_quality(1.0).bounded_by(EnvironmentalAcousticsBudget {
                max_processed_extents: 7,
                max_direct_rays: 80,
            });
        assert_eq!(bounded.max_direct_sources, 7);
        assert_eq!(bounded.max_direct_rays, 80);
        assert_eq!(bounded.max_early_reflection_sources, 8);

        let mut previous = AcousticSolvePlan::for_quality(0.0);
        for step in 1..=100 {
            let plan = AcousticSolvePlan::for_quality(step as f32 / 100.0);
            assert!(plan.max_direct_sources >= previous.max_direct_sources);
            assert!(plan.max_early_reflection_sources >= previous.max_early_reflection_sources);
            assert!(plan.early_reflection_taps >= previous.early_reflection_taps);
            assert!(plan.early_reflection_ray_count >= previous.early_reflection_ray_count);
            assert!(plan.late_ray_count >= previous.late_ray_count);
            assert!(plan.late_bounce_count >= previous.late_bounce_count);
            previous = plan;
        }
    }

    #[test]
    fn acoustic_telemetry_queue_is_bounded_and_counts_pressure() {
        let (sender, receiver) = crossbeam_channel::bounded(1);
        let counters = AcousticPropagationCounters::default();
        let event = AcousticTelemetryEvent::SolveDiscarded {
            spatial_revision: 1,
            geometry_version: 2,
            reason: AcousticDiscardReason::Superseded,
        };

        try_send_acoustic_telemetry(&sender, &counters, event.clone());
        try_send_acoustic_telemetry(&sender, &counters, event);

        assert_eq!(receiver.len(), 1);
        assert_eq!(
            counters.telemetry_queue_high_water.load(Ordering::Relaxed),
            1
        );
        assert_eq!(counters.dropped_telemetry_events.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn max_sample_telemetry_queue_drops_whole_events_under_pressure() {
        let mut input = input(Arc::new(NoGeometry));
        input.voices[0].source_extent = eight_sample_extent();
        let mut solver = AcousticSolver::new(1);
        let output = solver.solve_with_telemetry(
            &input,
            1.0,
            AcousticSolvePlan {
                max_direct_sources: 1,
                max_direct_rays: MAX_EXTENT_SAMPLES * 2,
                max_early_reflection_sources: 0,
                early_reflection_taps: 0,
                early_reflection_ray_count: 0,
                late_ray_count: 0,
                late_bounce_count: 0,
            },
        );
        assert_eq!(output.telemetry[0].direct.samples.len(), MAX_EXTENT_SAMPLES);
        assert_eq!(
            output.telemetry[0].environment.samples.len(),
            MAX_EXTENT_SAMPLES
        );
        let event = AcousticTelemetryEvent::ExtentResponse(Box::new(output.telemetry[0].clone()));
        let (sender, receiver) = crossbeam_channel::bounded(1);
        let counters = AcousticPropagationCounters::default();

        try_send_acoustic_telemetry(&sender, &counters, event.clone());
        try_send_acoustic_telemetry(&sender, &counters, event);

        assert_eq!(receiver.len(), 1);
        assert_eq!(
            counters.telemetry_queue_high_water.load(Ordering::Relaxed),
            1
        );
        assert_eq!(counters.dropped_telemetry_events.load(Ordering::Relaxed), 1);
        let received = receiver.try_recv().unwrap();
        assert!(matches!(
            received,
            AcousticTelemetryEvent::ExtentResponse(response)
                if response.direct.samples.len() == MAX_EXTENT_SAMPLES
                    && response.environment.samples.len() == MAX_EXTENT_SAMPLES
        ));
    }

    #[test]
    fn distributed_transmission_aggregates_normalized_power_not_binary_hits() {
        let mut samples = (0..8)
            .map(|id| ExtentSampleResponse {
                sample_id: crate::domain::ExtentSampleId(id),
                power_weight: 0.125,
                world_position: Vec3::new(id as f32 - 3.5, 0.0, 4.0),
                transmission: if id == 0 { [0.1; 3] } else { [1.0; 3] },
                hit: id == 0,
            })
            .collect::<Vec<_>>();

        let first = aggregate_extent_energy(&samples);
        samples.reverse();
        let reversed = aggregate_extent_energy(&samples);

        assert_eq!(
            first, reversed,
            "sample traversal order changed the response"
        );
        assert_eq!(first.hit_count, 1);
        assert!((first.visible_fraction - 0.875).abs() < 1.0e-6);
        for gain in first.gain {
            assert!((gain - 0.936_082_3).abs() < 1.0e-6);
            assert!((20.0 * gain.log10() - -0.573_72).abs() < 0.001);
        }
    }

    #[test]
    fn half_occlusion_changes_only_the_occluded_sample_energy() {
        let samples = [
            ExtentSampleResponse {
                sample_id: crate::domain::ExtentSampleId(1),
                power_weight: 0.5,
                world_position: Vec3::X,
                transmission: [1.0; 3],
                hit: false,
            },
            ExtentSampleResponse {
                sample_id: crate::domain::ExtentSampleId(2),
                power_weight: 0.5,
                world_position: -Vec3::X,
                transmission: [0.0; 3],
                hit: true,
            },
        ];

        let aggregate = aggregate_extent_energy(&samples);
        assert_eq!(aggregate.gain, [std::f32::consts::FRAC_1_SQRT_2; 3]);
        assert_eq!(aggregate.visible_fraction, 0.5);
    }

    #[test]
    fn directional_lobes_are_stable_order_independent_and_power_normalized() {
        let mut samples = [
            (11, Vec3::X, 1.0),
            (12, Vec3::Y, 0.8),
            (13, Vec3::Z, 0.6),
            (14, -Vec3::X, 0.4),
        ]
        .map(|(id, world_position, transmission)| ExtentSampleResponse {
            sample_id: crate::domain::ExtentSampleId(id),
            power_weight: 0.25,
            world_position,
            transmission: [transmission; 3],
            hit: transmission < 1.0,
        });

        let first = aggregate_directional_lobes(&samples, Vec3::ZERO, 4, [1.0; 3]);
        samples.reverse();
        let reversed = aggregate_directional_lobes(&samples, Vec3::ZERO, 4, [1.0; 3]);

        assert_eq!(first, reversed);
        assert_eq!(first.len(), 4);
        assert!((first.iter().map(|lobe| lobe.power).sum::<f32>() - 1.0).abs() < 1.0e-6);
        assert!(first.iter().all(|lobe| lobe.direction.is_normalized()));
        assert_eq!(
            first.iter().map(|lobe| lobe.lobe_id).collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
    }

    #[test]
    fn distributed_profile_applies_attack_release_schmitt_thresholds_and_dwell() {
        let profile = crate::domain::DistributedOcclusionProfile::default()
            .with_gain_floor([0.1; 3])
            .unwrap()
            .with_response_times(0.2, 0.1)
            .unwrap()
            .with_classification(0.25, 0.55, 0.1)
            .unwrap();
        let mut state = TemporalOcclusionState::default();

        let initial = state.update([1.0; 3], 1.0, 0.0, profile);
        assert_eq!(initial.classification, OcclusionClassification::Visible);
        assert_eq!(initial.filtered_gain, [1.0; 3]);

        let entering = state.update([0.5; 3], 0.1, 0.05, profile);
        assert_eq!(entering.classification, OcclusionClassification::Visible);
        assert!((entering.filtered_gain[0] - 0.889_400_4).abs() < 1.0e-6);

        let entered = state.update([0.5; 3], 0.1, 0.15, profile);
        assert_eq!(entered.classification, OcclusionClassification::Occluded);
        assert!((entered.dwell_seconds - 0.0).abs() < 1.0e-6);

        let exiting = state.update([1.0; 3], 0.9, 0.2, profile);
        assert_eq!(exiting.classification, OcclusionClassification::Occluded);
        assert!(exiting.filtered_gain[0] > entered.filtered_gain[0]);

        let exited = state.update([1.0; 3], 0.9, 0.3, profile);
        assert_eq!(exited.classification, OcclusionClassification::Visible);
    }

    #[test]
    fn response_pairs_complete_spatial_and_geometry_generations() {
        let response = solve_response(&input(Arc::new(UnitRoom)), 1.0);
        assert_eq!(response.spatial_revision, 11);
        assert_eq!(response.geometry_version, 17);
        assert_eq!(response.direct.len(), 1);
        assert_eq!(response.direct[0].gain, [0.1, 0.05, 0.02]);
    }

    #[test]
    fn weighted_extent_solve_uses_all_stable_samples_without_extra_voices() {
        use crate::domain::{DistributedOcclusionProfile, ExtentSample, ExtentSampleId};

        let mut input = input(Arc::new(NegativeXWood));
        let samples = (0..8)
            .map(|id| {
                let x = if id == 0 { -1.0 } else { 1.0 + id as f32 * 0.1 };
                ExtentSample::new(ExtentSampleId(id), Vec3::new(x, id as f32 * 0.1, 2.0), 1.0)
                    .unwrap()
            })
            .collect();
        input.voices[0].emitter_world_pose = Pose::identity();
        input.voices[0].source_extent = SourceExtent::weighted_samples(samples).unwrap();
        input.voices[0].occlusion_profile = OcclusionProfile::AmbientDistributed(
            DistributedOcclusionProfile::default()
                .with_lobe_count(4)
                .unwrap(),
        );

        let response = solve_response(&input, 1.0);
        assert_eq!(
            response.direct.len(),
            1,
            "one Voice must remain one cursor/response"
        );
        assert!((response.direct[0].gain[0] - 0.936_082_3).abs() < 1.0e-6);
        assert_eq!(response.direct[0].direct_lobes.len(), 4);
        for band in 0..3 {
            let lobe_energy = response.direct[0]
                .direct_lobes
                .iter()
                .map(|lobe| lobe.gain[band] * lobe.gain[band])
                .sum::<f32>();
            assert!((lobe_energy - response.direct[0].gain[band].powi(2)).abs() < 1.0e-6);
        }
    }

    #[test]
    fn caller_sized_extents_are_solved_cached_and_budgeted_whole() {
        use crate::domain::{ExtentSample, ExtentSampleId};
        for count in [16, 64, 257] {
            let mut input = input(Arc::new(NoGeometry));
            input.voices[0].source_extent = SourceExtent::weighted_samples_with_limit(
                (0..count)
                    .map(|i| {
                        ExtentSample::new(ExtentSampleId(i as u64), Vec3::X * i as f32 * 0.01, 1.0)
                            .unwrap()
                    })
                    .collect(),
                count,
            )
            .unwrap();
            let plan = AcousticSolvePlan {
                max_direct_sources: 1,
                max_direct_rays: count * 2,
                max_early_reflection_sources: 0,
                early_reflection_taps: 0,
                early_reflection_ray_count: 0,
                late_ray_count: 0,
                late_bounce_count: 0,
            };
            let mut solver = AcousticSolver::new(1);
            let output = solver.solve_with_telemetry(&input, 1.0, plan);
            assert_eq!(output.response.direct.len(), 1);
            let event = &output.telemetry[0];
            assert_eq!(event.extent_sample_count, count);
            for route in [&event.direct, &event.environment] {
                assert_eq!(route.samples.len(), count);
                assert_eq!(
                    route.samples.last().unwrap().sample_id,
                    ExtentSampleId((count - 1) as u64)
                );
                assert_eq!(route.ray_count, count);
            }
            let cached = solver.solve_with_telemetry(&input, 1.0, plan);
            assert_eq!(cached.telemetry[0].direct.cache_hit_count, count);
            assert_eq!(cached.telemetry[0].environment.cache_hit_count, count);
            let mut fresh_solver = AcousticSolver::new(1);
            let deferred = fresh_solver.solve_with_telemetry(
                &input,
                1.0,
                AcousticSolvePlan {
                    max_direct_rays: count * 2 - 1,
                    ..plan
                },
            );
            assert!(!deferred.telemetry[0].budget_member);
            assert_eq!(
                deferred.telemetry[0].solve_status,
                AcousticSolveStatus::Deferred
            );
            assert!(deferred.telemetry[0].direct.samples.is_empty());
            assert!(deferred.telemetry[0].environment.samples.is_empty());
        }
    }

    #[test]
    fn extent_telemetry_reports_rays_hits_cache_filter_and_lobes() {
        use crate::domain::{DistributedOcclusionProfile, ExtentSample, ExtentSampleId};

        let mut input = input(Arc::new(NegativeXWood));
        input.voices[0].emitter_world_pose = Pose::identity();
        input.voices[0].source_extent = SourceExtent::weighted_samples(
            (0..8)
                .map(|id| {
                    ExtentSample::new(
                        ExtentSampleId(id),
                        Vec3::new(if id == 0 { -1.0 } else { 1.0 }, id as f32 * 0.1, 2.0),
                        1.0,
                    )
                    .unwrap()
                })
                .collect(),
        )
        .unwrap();
        input.voices[0].occlusion_profile = OcclusionProfile::AmbientDistributed(
            DistributedOcclusionProfile::default()
                .with_lobe_count(4)
                .unwrap(),
        );
        let plan = AcousticSolvePlan {
            max_early_reflection_sources: 0,
            late_ray_count: 0,
            ..AcousticSolvePlan::for_quality(0.5)
        };
        let mut solver = AcousticSolver::new(1);

        let first = solver.solve_with_telemetry(&input, 1.0, plan);
        let event = &first.telemetry[0];
        assert_eq!(event.extent_sample_count, 8);
        assert_eq!(event.direct.sample_count, 8);
        assert_eq!(event.direct.ray_count, 8);
        assert_eq!(event.direct.cache_hit_count, 0);
        assert_eq!(event.direct.hit_count, 1);
        assert!((event.direct.visible_fraction - 0.875).abs() < 1.0e-6);
        assert!((event.direct.raw_gain[0] - 0.936_082_3).abs() < 1.0e-6);
        assert!((20.0 * event.direct.raw_gain[0].log10() - -0.573_72).abs() < 0.001);
        assert_eq!(event.direct.samples.len(), 8);
        assert_eq!(event.direct.samples[0].sample_id, ExtentSampleId(0));
        assert_eq!(event.direct.samples[0].normalized_power_weight, 0.125);
        assert_eq!(
            event.direct.samples[0].world_position,
            Vec3::new(-1.0, 0.0, 2.0)
        );
        assert!(event.direct.samples[0].hit);
        assert_eq!(event.direct.samples[0].transmission, [0.1; 3]);
        assert!(!event.direct.samples[1].hit);
        assert_eq!(event.direct.samples[1].transmission, [1.0; 3]);
        let reconstructed_energy = std::array::from_fn::<_, 3, _>(|band| {
            event
                .direct
                .samples
                .iter()
                .map(|sample| sample.normalized_power_weight * sample.transmission[band].powi(2))
                .sum::<f32>()
        });
        for (reconstructed, reported) in reconstructed_energy
            .map(f32::sqrt)
            .into_iter()
            .zip(event.direct.raw_gain)
        {
            assert!((reconstructed - reported).abs() < 1.0e-6);
        }
        assert_eq!(
            event
                .direct
                .samples
                .iter()
                .filter(|sample| sample.hit)
                .count(),
            event.direct.hit_count
        );
        assert_eq!(
            event
                .direct
                .samples
                .iter()
                .filter(|sample| !sample.hit)
                .map(|sample| sample.normalized_power_weight)
                .sum::<f32>(),
            event.direct.visible_fraction
        );
        assert_eq!(event.direct.raw_gain, event.direct.filtered_gain);
        assert_eq!(event.lobes.len(), 4);
        assert_eq!(event.solve_status, AcousticSolveStatus::Solved);
        assert!(event.budget_member);
        assert_eq!(
            first.response.direct[0].environment_representatives.len(),
            2,
            "early reflections must have bounded distributed representatives",
        );
        assert!(
            first.response.direct[0]
                .environment_representatives
                .iter()
                .all(|representative| representative.world_position != Vec3::ZERO)
        );

        let cached = solver.solve_with_telemetry(&input, 1.0, plan);
        assert_eq!(cached.telemetry[0].direct.ray_count, 0);
        assert_eq!(cached.telemetry[0].direct.cache_hit_count, 8);
        assert_eq!(cached.telemetry[0].direct.samples, event.direct.samples);
        assert_eq!(cached.telemetry[0].environment.ray_count, 0);
        assert_eq!(cached.telemetry[0].environment.cache_hit_count, 8);
        assert_eq!(
            cached.telemetry[0].environment.samples,
            event.environment.samples
        );
    }

    #[test]
    fn public_sample_telemetry_reconstructs_half_wood_occlusion() {
        let mut input = input(Arc::new(NegativeXWood));
        input.voices[0].emitter_world_pose = Pose::identity();
        input.voices[0].source_extent = eight_sample_extent();
        input.voices[0].environment_send = EnvironmentSend::disabled();
        let output = AcousticSolver::new(1).solve_with_telemetry(
            &input,
            1.0,
            AcousticSolvePlan {
                max_early_reflection_sources: 0,
                late_ray_count: 0,
                ..AcousticSolvePlan::for_quality(0.5)
            },
        );
        let direct = &output.telemetry[0].direct;

        assert_eq!(direct.samples.len(), 8);
        assert_eq!(direct.hit_count, 4);
        assert_eq!(direct.visible_fraction, 0.5);
        assert!((direct.raw_gain[0] - 0.710_633_5).abs() < 1.0e-6);
        assert!((20.0 * direct.raw_gain[0].log10() - -2.967_1).abs() < 0.001);
        let reconstructed_gain = std::array::from_fn::<_, 3, _>(|band| {
            direct
                .samples
                .iter()
                .map(|sample| sample.normalized_power_weight * sample.transmission[band].powi(2))
                .sum::<f32>()
                .sqrt()
        });
        assert_eq!(reconstructed_gain, direct.raw_gain);
    }

    #[test]
    fn point_sample_telemetry_keeps_direct_and_environment_routes_independent() {
        use crate::domain::ExtentSampleId;

        let mut input = input(Arc::new(DirectionalTransmission));
        input.voices[0].emitter_world_pose = Pose::from_position(Vec3::X);
        input.voices[0].environment_send =
            EnvironmentSend::from_world_pose(Pose::from_position(-Vec3::X));
        let mut solver = AcousticSolver::new(1);
        let output = solver.solve_with_telemetry(
            &input,
            1.0,
            AcousticSolvePlan {
                max_early_reflection_sources: 0,
                late_ray_count: 0,
                ..AcousticSolvePlan::for_quality(0.5)
            },
        );
        let event = &output.telemetry[0];

        assert_eq!(event.direct.samples.len(), 1);
        assert_eq!(event.environment.samples.len(), 1);
        assert_eq!(event.direct.samples[0].sample_id, ExtentSampleId::POINT);
        assert_eq!(
            event.environment.samples[0].sample_id,
            ExtentSampleId::POINT
        );
        assert_eq!(event.direct.samples[0].normalized_power_weight, 1.0);
        assert_eq!(event.environment.samples[0].normalized_power_weight, 1.0);
        assert_eq!(event.direct.samples[0].world_position, Vec3::X);
        assert_eq!(event.environment.samples[0].world_position, -Vec3::X);
        assert!(event.direct.samples[0].hit);
        assert!(event.environment.samples[0].hit);
        assert_eq!(event.direct.samples[0].transmission, [0.2; 3]);
        assert_eq!(event.environment.samples[0].transmission, [0.8; 3]);

        let cached = solver.solve_with_telemetry(
            &input,
            1.0,
            AcousticSolvePlan {
                max_early_reflection_sources: 0,
                late_ray_count: 0,
                ..AcousticSolvePlan::for_quality(0.5)
            },
        );
        assert_eq!(cached.telemetry[0].direct.ray_count, 0);
        assert_eq!(cached.telemetry[0].direct.cache_hit_count, 1);
        assert_eq!(cached.telemetry[0].direct.samples[0].transmission, [0.2; 3]);
        assert_eq!(cached.telemetry[0].environment.ray_count, 0);
        assert_eq!(cached.telemetry[0].environment.cache_hit_count, 1);
        assert_eq!(
            cached.telemetry[0].environment.samples[0].transmission,
            [0.8; 3]
        );
        let conclusion = &cached.conclusions[0].telemetry;
        assert_eq!(conclusion.candidate_rank, Some(1));
        assert_eq!(
            conclusion.candidate_limit,
            AcousticSolvePlan::for_quality(0.5).max_direct_sources
        );
        assert_eq!(conclusion.direct, AcousticRouteOutcome::Applied);
        assert_eq!(conclusion.environment, AcousticRouteOutcome::Applied);
        assert_eq!(conclusion.environment_transmission_gain, [0.8; 3]);
        assert_eq!(conclusion.early_tap_count, 0);
        assert_eq!(conclusion.solve_status, Some(AcousticSolveStatus::Solved));
    }

    #[test]
    fn disabled_routes_receive_an_explicit_non_candidate_conclusion() {
        let mut input = input(Arc::new(NoGeometry));
        input.voices[0].direct_path = DirectPath::disabled();
        input.voices[0].environment_send = EnvironmentSend::disabled();

        let output = AcousticSolver::new(1).solve_with_telemetry(
            &input,
            1.0,
            AcousticSolvePlan::for_quality(0.5),
        );

        assert!(output.response.direct.is_empty());
        assert!(output.telemetry.is_empty());
        let conclusion = &output.conclusions[0].telemetry;
        assert_eq!(conclusion.candidate_rank, None);
        assert_eq!(conclusion.direct, AcousticRouteOutcome::Disabled);
        assert_eq!(conclusion.environment, AcousticRouteOutcome::Disabled);
        assert_eq!(conclusion.solve_status, None);
    }

    #[test]
    fn budget_skip_retains_bounded_history_then_defers_without_unity_fallback() {
        let mut input = input(Arc::new(DirectionalTransmission));
        let emitter_b = Emitter {
            world_id: 1,
            index: 1,
            generation: 1,
        };
        input.voices[0].emitter_world_pose = Pose::from_position(Vec3::X);
        input.voices[0].acoustic_priority = 2.0;
        let mut voice_b = input.voices[0].clone();
        voice_b.voice_id = VoiceId::from(2);
        voice_b.emitter = emitter_b;
        voice_b.emitter_world_pose = Pose::from_position(-Vec3::X);
        voice_b.acoustic_priority = 1.0;
        input.voices.push(voice_b);
        input.spatial = Arc::new(
            SpatialFrame::new(
                20,
                0.0,
                Pose::identity(),
                vec![
                    EmitterSpatialState::new(input.voices[0].emitter, Pose::from_position(Vec3::X)),
                    EmitterSpatialState::new(emitter_b, Pose::from_position(-Vec3::X)),
                ],
            )
            .unwrap(),
        );
        let plan = AcousticSolvePlan {
            max_direct_sources: 1,
            max_direct_rays: 4,
            ..AcousticSolvePlan::for_quality(0.5)
        };
        let mut solver = AcousticSolver::new(2);

        let first = solver.solve_with_telemetry(&input, 1.0, plan);
        let voice_a = first
            .response
            .direct
            .iter()
            .find(|response| response.voice_id == VoiceId::from(1))
            .unwrap();
        assert_eq!(voice_a.solve_status, DirectSolveStatus::Solved);
        assert_eq!(voice_a.gain, [0.2; 3]);
        let solved_telemetry = first
            .telemetry
            .iter()
            .find(|event| event.voice_id == 1)
            .unwrap()
            .clone();
        assert_eq!(solved_telemetry.direct.samples.len(), 1);

        input.voices[0].acoustic_priority = 1.0;
        input.voices[1].acoustic_priority = 3.0;
        input.spatial = Arc::new(
            SpatialFrame::new(
                21,
                0.1,
                Pose::identity(),
                vec![
                    EmitterSpatialState::new(input.voices[0].emitter, Pose::from_position(Vec3::X)),
                    EmitterSpatialState::new(emitter_b, Pose::from_position(-Vec3::X)),
                ],
            )
            .unwrap(),
        );
        let retained = solver.solve_with_telemetry(&input, 1.0, plan);
        let voice_a = retained
            .response
            .direct
            .iter()
            .find(|response| response.voice_id == VoiceId::from(1))
            .unwrap();
        assert_eq!(voice_a.solve_status, DirectSolveStatus::Retained);
        assert_eq!(voice_a.gain, [0.2; 3]);
        let retained_telemetry = retained
            .telemetry
            .iter()
            .find(|event| event.voice_id == 1)
            .unwrap();
        assert_eq!(
            retained_telemetry.solve_status,
            AcousticSolveStatus::Retained
        );
        assert_eq!(retained_telemetry.response_spatial_revision, 20);
        assert_eq!(retained_telemetry.spatial_revision, 21);
        assert_eq!(retained_telemetry.direct.ray_count, 0);
        assert_eq!(retained_telemetry.direct.cache_hit_count, 0);
        assert_eq!(
            retained_telemetry.direct.samples,
            solved_telemetry.direct.samples
        );
        let retained_conclusion = retained
            .conclusions
            .iter()
            .find(|event| event.telemetry.voice_id == 1)
            .unwrap();
        assert_eq!(retained_conclusion.telemetry.candidate_rank, Some(2));
        assert_eq!(retained_conclusion.telemetry.candidate_limit, 1);
        assert_eq!(
            retained_conclusion.telemetry.direct,
            AcousticRouteOutcome::ExcludedByBudget
        );
        assert_eq!(
            retained_conclusion.telemetry.environment,
            AcousticRouteOutcome::ExcludedByBudget
        );
        assert_eq!(
            retained_conclusion.telemetry.solve_status,
            Some(AcousticSolveStatus::Retained)
        );

        input.spatial = Arc::new(
            SpatialFrame::new(
                22,
                0.4,
                Pose::identity(),
                vec![
                    EmitterSpatialState::new(input.voices[0].emitter, Pose::from_position(Vec3::X)),
                    EmitterSpatialState::new(emitter_b, Pose::from_position(-Vec3::X)),
                ],
            )
            .unwrap(),
        );
        let deferred = solver.solve_with_telemetry(&input, 1.0, plan);
        let voice_a = deferred
            .response
            .direct
            .iter()
            .find(|response| response.voice_id == VoiceId::from(1))
            .unwrap();
        assert_eq!(voice_a.solve_status, DirectSolveStatus::Deferred);
        assert_eq!(
            deferred
                .response
                .voice_response(VoiceId::from(1))
                .and_then(VoiceAcousticResponse::direct_gain_target),
            None
        );
        let deferred_telemetry = deferred
            .telemetry
            .iter()
            .find(|event| event.voice_id == 1)
            .unwrap();
        assert_eq!(
            deferred_telemetry.solve_status,
            AcousticSolveStatus::Deferred
        );
        assert!(deferred_telemetry.direct.samples.is_empty());
        assert!(deferred_telemetry.environment.samples.is_empty());
    }

    #[test]
    fn sample_cache_key_includes_voice_emitter_revisions_geometry_and_stable_sample_id() {
        let query = Arc::new(CountingOpenGeometry::default());
        let mut input = input(query.clone());
        let plan = AcousticSolvePlan {
            max_direct_sources: 1,
            max_direct_rays: 2,
            max_early_reflection_sources: 0,
            early_reflection_taps: 0,
            early_reflection_ray_count: 0,
            late_ray_count: 0,
            late_bounce_count: 0,
        };
        let mut solver = AcousticSolver::new(1);

        solver.solve_with_plan(&input, 1.0, plan);
        assert_eq!(query.traced_closest_rays.load(Ordering::Relaxed), 2);
        solver.solve_with_plan(&input, 1.0, plan);
        assert_eq!(
            query.traced_closest_rays.load(Ordering::Relaxed),
            2,
            "identical immutable solve input should reuse per-route sample results"
        );

        input.spatial = Arc::new(
            SpatialFrame::new(
                12,
                1.6,
                Pose::identity(),
                vec![EmitterSpatialState::new(
                    input.voices[0].emitter,
                    Pose::from_position(Vec3::Z),
                )],
            )
            .unwrap(),
        );
        solver.solve_with_plan(&input, 1.0, plan);
        assert_eq!(query.traced_closest_rays.load(Ordering::Relaxed), 4);

        input.scene = Arc::new(AcousticSceneSnapshot::new(18, query.clone()));
        solver.solve_with_plan(&input, 1.0, plan);
        assert_eq!(query.traced_closest_rays.load(Ordering::Relaxed), 6);
    }

    #[test]
    fn budget_ranking_prefers_audible_priority_and_resists_small_rank_churn() {
        let mut input = input(Arc::new(NoGeometry));
        let mut quiet = input.voices[0].clone();
        quiet.voice_id = VoiceId::from(2);
        quiet.audibility = 0.1;
        quiet.acoustic_priority = 1.0;
        input.voices[0].audibility = 1.0;
        input.voices[0].acoustic_priority = 1.0;
        input.voices.push(quiet);

        let first = ranked_voices(&input, &HashSet::new());
        assert_eq!(first[0].voice.voice_id, VoiceId::from(1));

        let previous = [input.voices[0].route_key()].into_iter().collect();
        input.voices[1].audibility = 0.95;
        let stable = ranked_voices(&input, &previous);
        assert_eq!(stable[0].voice.voice_id, VoiceId::from(1));
    }

    #[test]
    fn emitter_audibility_updates_attached_acoustic_voices_only() {
        let input_port = AcousticVoiceInput::isolated(2);
        let mut registry = AcousticVoiceRegistry::new(2);
        let snapshot = input(Arc::new(NoGeometry)).voices.remove(0);
        let attached = snapshot.active_for_test();
        let mut detached = snapshot.active_for_test();
        detached.voice_id = VoiceId::from(2);
        detached.detached = true;
        let emitter = attached.emitter;
        registry.activate(attached, None);
        registry.activate(detached, None);

        registry.update_emitter_audibility(emitter, 0.25);
        assert!(input_port.try_publish(&mut registry));

        let state = input_port.input.state.lock().unwrap();
        assert_eq!(state.voices[0].audibility, 0.25);
        assert_eq!(state.voices[1].audibility, 1.0);
    }

    #[test]
    fn active_acoustic_voice_edits_are_linear_for_4096_voices() {
        const VOICES: usize = 4_096;
        let input_port = AcousticVoiceInput::isolated(VOICES);
        let mut registry = AcousticVoiceRegistry::new(VOICES);
        let mut retired = Vec::with_capacity(VOICES);
        let emitter = |index: usize| Emitter {
            world_id: 1,
            index: index as u32,
            generation: 1,
        };
        let mut comparisons = 0;
        let memory_activity = crate::test_support::realtime_memory_activity(|| {
            comparisons = crate::test_support::acoustic_input_comparison_activity(|| {
                for voice in 0..VOICES {
                    registry.activate(
                        AcousticVoice {
                            voice_id: VoiceId::from(voice as u64 + 1),
                            emitter: emitter(voice),
                            emitter_world_pose: Pose::identity(),
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
                }
                assert!(input_port.try_publish(&mut registry));
                for voice in 0..VOICES {
                    retired.push(
                        registry
                            .retire(VoiceId::from(voice as u64 + 1))
                            .expect("activated Voice remains owned by the registry"),
                    );
                }
                assert!(input_port.try_publish(&mut registry));
            });
        });

        assert_eq!(registry.len(), 0);
        assert_eq!(
            memory_activity, 0,
            "bounded registry or snapshot sync allocated"
        );
        assert!(
            comparisons <= VOICES * 4,
            "4096 activate/retire edits performed {comparisons} identity comparisons"
        );
    }

    #[test]
    fn render_voice_input_never_waits_for_the_worker_mutex() {
        let input_port = AcousticVoiceInput::isolated(1);
        let mut registry = AcousticVoiceRegistry::new(1);
        registry.activate(
            AcousticVoice {
                voice_id: VoiceId::from(1),
                emitter: Emitter {
                    world_id: 1,
                    index: 0,
                    generation: 1,
                },
                emitter_world_pose: Pose::identity(),
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
        let guard = input_port.input.state.lock().unwrap();
        assert!(!input_port.try_publish(&mut registry));
        assert_eq!(registry.len(), 1, "busy publication lost the Voice owner");
        drop(guard);
        assert!(input_port.try_publish(&mut registry));
        assert_eq!(input_port.input.state.lock().unwrap().voices.len(), 1);
    }

    #[test]
    fn poisoned_worker_input_recovers_without_losing_render_owned_voices() {
        let input_port = AcousticVoiceInput::isolated(1);
        let poison_input = input_port.clone();
        assert!(
            std::thread::spawn(move || {
                let _guard = poison_input.input.state.lock().unwrap();
                panic!("poison worker input for recovery proof");
            })
            .join()
            .is_err()
        );
        assert!(input_port.input.state.is_poisoned());

        let mut registry = AcousticVoiceRegistry::new(1);
        registry.activate(
            input(Arc::new(NoGeometry))
                .voices
                .remove(0)
                .active_for_test(),
            None,
        );

        assert!(input_port.try_publish(&mut registry));
        assert!(!input_port.input.state.is_poisoned());
        assert_eq!(registry.len(), 1);
        assert_eq!(input_port.input.state.lock().unwrap().voices.len(), 1);
    }

    #[test]
    fn activation_and_spatial_publication_share_bounded_emitter_lookup() {
        const EMITTERS: usize = 4_096;
        const VOICES: usize = 2_048;
        const MAX_BINARY_COMPARISONS: usize = 13;
        let emitter = |index: usize| Emitter {
            world_id: 1,
            index: index as u32,
            generation: 1,
        };
        let frame = Arc::new(
            SpatialFrame::new(
                23,
                2.0,
                Pose::identity(),
                (0..EMITTERS)
                    .rev()
                    .map(|index| {
                        EmitterSpatialState::new(
                            emitter(index),
                            Pose::from_position(Vec3::new(index as f32, 1.0, 0.0)),
                        )
                        .with_acoustic_priority(index as f32 + 1.0)
                    })
                    .collect(),
            )
            .unwrap(),
        );
        let input_port = AcousticVoiceInput::isolated(1);
        let mut registry = AcousticVoiceRegistry::new(1);
        input_port.input.state.lock().unwrap().spatial = Some(frame.clone());
        let mut activated = input(Arc::new(NoGeometry))
            .voices
            .remove(0)
            .active_for_test();
        activated.emitter = emitter(EMITTERS - 1);
        activated.emitter_world_pose = Pose::from_position(-Vec3::Y);
        let activation_comparisons = crate::test_support::spatial_frame_comparison_activity(|| {
            registry.activate(activated, None);
            assert!(input_port.try_publish(&mut registry));
        });
        assert!(activation_comparisons > 0);
        assert!(activation_comparisons <= MAX_BINARY_COMPARISONS);
        let state = input_port.input.state.lock().unwrap();
        assert_eq!(
            state.voices[0].emitter_world_pose.position,
            Vec3::new((EMITTERS - 1) as f32, 1.0, 0.0)
        );
        assert_eq!(state.voices[0].acoustic_priority, EMITTERS as f32);
        drop(state);

        let (propagation, _worker) = AcousticPropagation::prepare(
            1.0,
            Arc::new(AtomicBool::new(true)),
            0.5,
            EnvironmentalAcousticsBudget::default(),
            VOICES,
            1,
        );
        let voice_input = propagation.voice_input();
        let mut registry = AcousticVoiceRegistry::new(VOICES);
        let template = input(Arc::new(NoGeometry)).voices.remove(0);
        for voice_index in 0..VOICES {
            let mut voice = template.clone().active_for_test();
            voice.voice_id = VoiceId::from(voice_index as u64 + 1);
            voice.emitter = emitter(voice_index * 2);
            voice.emitter_world_pose = Pose::from_position(-Vec3::Y);
            voice.detached = voice_index == 0;
            registry.activate(voice, None);
        }
        assert!(voice_input.try_publish(&mut registry));

        let publication_comparisons =
            crate::test_support::spatial_frame_comparison_activity(|| {
                propagation.publish_spatial_frame(frame).unwrap();
            });

        assert!(publication_comparisons >= VOICES - 1);
        assert!(publication_comparisons <= (VOICES - 1) * MAX_BINARY_COMPARISONS);
        let state = voice_input.input.state.lock().unwrap();
        assert_eq!(state.voices.len(), VOICES);
        assert_eq!(state.voices[0].emitter_world_pose.position, -Vec3::Y);
        assert_eq!(
            state.voices[VOICES - 1].emitter_world_pose.position,
            Vec3::new(((VOICES - 1) * 2) as f32, 1.0, 0.0)
        );
        assert_eq!(state.voices[VOICES - 1].source_extent, SourceExtent::Point);
    }

    #[test]
    fn overlapping_voices_keep_independent_fixed_acoustic_origins() {
        let mut input = input(Arc::new(DirectionalTransmission));
        let emitter = input.voices[0].emitter;
        input.voices = vec![
            AcousticVoiceSnapshot {
                voice_id: VoiceId::from(41),
                emitter,
                emitter_world_pose: Pose::from_position(Vec3::Z),
                acoustic_priority: 1.0,
                audibility: 1.0,
                detached: false,
                direct_path: DirectPath::listener_relative(Pose::from_position(-Vec3::Y))
                    .with_geometry(DirectGeometry::BypassTransmission),
                environment_send: EnvironmentSend::from_world_pose(Pose::from_position(Vec3::X)),
                source_extent: SourceExtent::Point,
                occlusion_profile: OcclusionProfile::PointExact,
                routing_generation: 1,
            },
            AcousticVoiceSnapshot {
                voice_id: VoiceId::from(42),
                emitter,
                emitter_world_pose: Pose::from_position(Vec3::Z),
                acoustic_priority: 1.0,
                audibility: 1.0,
                detached: false,
                direct_path: DirectPath::listener_relative(Pose::from_position(-Vec3::Y))
                    .with_geometry(DirectGeometry::BypassTransmission),
                environment_send: EnvironmentSend::from_world_pose(Pose::from_position(-Vec3::X)),
                source_extent: SourceExtent::Point,
                occlusion_profile: OcclusionProfile::PointExact,
                routing_generation: 2,
            },
        ];

        let response = solve_response(&input, 1.0);
        assert_eq!(response.direct.len(), 2);
        let voice_a = response.voice_response(VoiceId::from(41)).unwrap();
        let voice_b = response.voice_response(VoiceId::from(42)).unwrap();
        assert_eq!(voice_a.environment_gain(), [0.2; 3]);
        assert_eq!(voice_b.environment_gain(), [0.8; 3]);
        assert_eq!(voice_a.direct_gain(), [1.0; 3]);
        assert_eq!(voice_b.direct_gain(), [1.0; 3]);
    }

    #[test]
    fn open_geometry_has_no_invented_late_reverb() {
        let response = solve_response(&input(Arc::new(NoGeometry)), 1.0);
        assert_eq!(response.late_reverb, LateReverbParameters::SILENT);
    }

    #[test]
    fn enclosed_geometry_produces_bounded_frequency_dependent_reverb() {
        let response = solve_response(&input(Arc::new(UnitRoom)), 1.0);
        assert!(response.late_reverb.wet_gain > 0.0);
        assert!(response.late_reverb.wet_gain <= 0.35);
        assert!(
            response
                .late_reverb
                .rt60_seconds
                .iter()
                .all(|rt60| (0.05..=20.0).contains(rt60))
        );
        assert!(response.late_reverb.rt60_seconds[0] > response.late_reverb.rt60_seconds[2]);
    }

    #[test]
    fn invalid_backend_values_are_sanitized_before_reaching_dsp() {
        let response = solve_response(&input(Arc::new(InvalidMaterialRoom)), 1.0);
        assert_eq!(response.direct[0].gain, [1.0, 0.0, 1.0]);
        assert!(response.late_reverb.wet_gain.is_finite());
        assert!(
            response
                .late_reverb
                .rt60_seconds
                .iter()
                .all(|value| value.is_finite())
        );
    }

    #[test]
    fn visible_first_bounce_paths_produce_bounded_frequency_dependent_taps() {
        let response = solve_response(
            &input(Arc::new(ReflectiveFloor {
                blocks_visibility: false,
            })),
            1.0,
        );
        let taps = &response.direct[0].early_reflections;
        assert!(!taps.is_empty());
        assert!(taps.len() <= MAX_EARLY_REFLECTION_TAPS);
        for tap in taps {
            assert!(tap.arrival_direction.is_finite());
            assert!((0.0..=EARLY_REFLECTION_MAX_DELAY_SECONDS).contains(&tap.delay_seconds));
            assert!(tap.gain[0] > tap.gain[2]);
            assert!(tap.gain.iter().all(|gain| gain.is_finite() && *gain >= 0.0));
        }
    }

    #[test]
    fn blocked_second_segments_reject_early_reflection_candidates() {
        let response = solve_response(
            &input(Arc::new(ReflectiveFloor {
                blocks_visibility: true,
            })),
            1.0,
        );
        assert!(response.direct[0].early_reflections.is_empty());
    }

    #[test]
    fn rapid_pose_revisions_publish_compatible_response_before_short_voice_deadline() {
        let query = Arc::new(BlockingOpenGeometry::new());
        let (propagation, children) = start_propagation(true, 0.0, 1, 16);
        let emitter = Emitter {
            world_id: 1,
            index: 0,
            generation: 1,
        };
        let voice_input = propagation.voice_input();
        let mut voice_registry = AcousticVoiceRegistry::new(1);
        voice_registry.activate(
            AcousticVoice {
                voice_id: VoiceId::from(1),
                emitter,
                emitter_world_pose: Pose::from_position(Vec3::Z),
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
        assert!(voice_input.try_publish(&mut voice_registry));
        propagation
            .publish_scene(Arc::new(AcousticSceneSnapshot::new(7, query.clone())))
            .unwrap();
        propagation
            .publish_spatial_frame(Arc::new(
                SpatialFrame::new(
                    1,
                    0.0,
                    Pose::identity(),
                    vec![EmitterSpatialState::new(
                        emitter,
                        Pose::from_position(Vec3::Z),
                    )],
                )
                .unwrap(),
            ))
            .unwrap();
        query.wait_until_entered();

        const FRAME_HZ: u64 = 240;
        const SOLVE_TIME_MILLIS: u64 = 50;
        const ONE_SHOT_MILLIS: u64 = 600;
        let frames_during_solve = SOLVE_TIME_MILLIS * FRAME_HZ / 1_000;
        for revision in 2..=frames_during_solve + 1 {
            propagation
                .publish_spatial_frame(Arc::new(
                    SpatialFrame::new(
                        revision,
                        revision as f64 / FRAME_HZ as f64,
                        Pose::identity(),
                        vec![EmitterSpatialState::new(
                            emitter,
                            Pose::from_position(Vec3::new(0.0, 0.0, revision as f32)),
                        )],
                    )
                    .unwrap(),
                ))
                .unwrap();
        }
        assert!(propagation.response_publication.latest().is_none());
        query.release();

        let deadline = Instant::now() + Duration::from_millis(ONE_SHOT_MILLIS);
        loop {
            let revision = propagation
                .response_publication
                .latest()
                .as_ref()
                .map(|response| response.spatial_revision);
            if revision == Some(frames_during_solve + 1) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "latest revision was not published within the one-shot lifetime"
            );
            std::thread::sleep(Duration::from_millis(5));
        }

        assert_eq!(propagation.diagnostics().superseded_solve_count, 0);
        let events = propagation
            .telemetry_receiver()
            .try_iter()
            .collect::<Vec<_>>();
        assert!(!events.iter().any(|event| {
            matches!(
                event,
                AcousticTelemetryEvent::SolveDiscarded {
                    spatial_revision: 1,
                    geometry_version: 7,
                    reason: AcousticDiscardReason::Superseded,
                }
            )
        }));
        assert!(events.iter().any(|event| {
            matches!(
                event,
                AcousticTelemetryEvent::ExtentResponse(response)
                    if response.response_spatial_revision == 1
                        && response.direct.samples.len() == 1
                        && response.environment.samples.len() == 1
            )
        }));
        assert!(events.iter().any(|event| {
            matches!(
                event,
                AcousticTelemetryEvent::VoiceConclusion(conclusion)
                    if conclusion.spatial_revision == 1
                        && conclusion.candidate_rank == Some(1)
                        && conclusion.environment == AcousticRouteOutcome::Applied
            )
        }));
        children.close().unwrap();
        propagation.close_publication();
    }

    #[test]
    fn worker_keeps_only_latest_complete_input_and_closes_cleanly() {
        let (propagation, children) = start_propagation(true, 0.5, 8, 8);
        propagation
            .publish_scene(Arc::new(AcousticSceneSnapshot::new(17, Arc::new(UnitRoom))))
            .unwrap();
        let emitter = Emitter {
            world_id: 1,
            index: 0,
            generation: 1,
        };

        for revision in [11, 12] {
            propagation
                .publish_spatial_frame(Arc::new(
                    SpatialFrame::new(
                        revision,
                        revision as f64 * 0.01,
                        Pose::from_position(Vec3::ZERO),
                        vec![EmitterSpatialState::new(
                            emitter,
                            Pose::from_position(Vec3::Z),
                        )],
                    )
                    .unwrap(),
                ))
                .unwrap();
        }

        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            let latest_revision = propagation
                .response_publication
                .latest()
                .as_ref()
                .map(|response| response.spatial_revision);
            if latest_revision == Some(12) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "worker did not publish latest input"
            );
            std::thread::sleep(Duration::from_millis(5));
        }

        let diagnostics = propagation.diagnostics();
        assert!(diagnostics.solve_count >= 1);
        assert!(diagnostics.published_response_count >= 1);
        assert_eq!(diagnostics.latest_spatial_revision, 12);
        assert_eq!(diagnostics.latest_geometry_version, 17);
        children.close().unwrap();
        propagation.close_publication();
    }

    #[test]
    fn consumer_disconnect_while_running_remains_a_terminal_acoustics_failure() {
        let runtime_state = Arc::new(AtomicU8::new(crate::events::RuntimeState::Running as u8));
        let mut services = RuntimeServices::new(
            crate::runtime_health::RuntimeFailurePublisher::new(runtime_state.clone()),
        );
        let (propagation, worker) = AcousticPropagation::prepare(
            1.0,
            Arc::new(AtomicBool::new(true)),
            0.5,
            EnvironmentalAcousticsBudget::default(),
            1,
            8,
        );
        worker.start(&mut services).unwrap();
        drop(
            propagation
                .take_response_consumer()
                .expect("the runtime consumer starts connected"),
        );
        let emitter = Emitter {
            world_id: 1,
            index: 0,
            generation: 1,
        };
        let voice_input = propagation.voice_input();
        let mut voice_registry = AcousticVoiceRegistry::new(1);
        voice_registry.activate(
            AcousticVoice {
                voice_id: VoiceId::from(1),
                emitter,
                emitter_world_pose: Pose::from_position(Vec3::Z),
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
        assert!(voice_input.try_publish(&mut voice_registry));
        propagation
            .publish_scene(Arc::new(AcousticSceneSnapshot::new(
                23,
                Arc::new(NoGeometry),
            )))
            .unwrap();
        propagation
            .publish_spatial_frame(Arc::new(
                SpatialFrame::new(
                    1,
                    0.0,
                    Pose::identity(),
                    vec![EmitterSpatialState::new(
                        emitter,
                        Pose::from_position(Vec3::Z),
                    )],
                )
                .unwrap(),
            ))
            .unwrap();

        let deadline = Instant::now() + Duration::from_secs(1);
        while crate::events::RuntimeState::load(&runtime_state)
            != crate::events::RuntimeState::Failed
        {
            assert!(
                Instant::now() < deadline,
                "consumer loss did not become a terminal runtime failure"
            );
            std::thread::park_timeout(Duration::from_millis(1));
        }
        let close_error = services
            .close()
            .expect_err("unexpected consumer loss must remain visible during close");
        assert!(
            close_error
                .to_string()
                .contains("acoustics: acoustics response consumer disconnected"),
            "close lost the response publication failure: {close_error}"
        );
        propagation.close_publication();
    }

    #[test]
    fn disabled_worker_waits_for_reenable_before_solving() {
        let (propagation, children) = start_propagation(false, 0.5, 8, 8);
        propagation
            .publish_scene(Arc::new(AcousticSceneSnapshot::new(3, Arc::new(UnitRoom))))
            .unwrap();
        propagation
            .publish_spatial_frame(Arc::new(
                SpatialFrame::new(4, 0.04, Pose::from_position(Vec3::ZERO), Vec::new()).unwrap(),
            ))
            .unwrap();

        std::thread::sleep(Duration::from_millis(75));
        assert_eq!(propagation.diagnostics().solve_count, 0);

        propagation.set_enabled(true);
        let deadline = Instant::now() + Duration::from_secs(1);
        while propagation.diagnostics().solve_count == 0 {
            assert!(
                Instant::now() < deadline,
                "worker did not resume after reenable"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        children.close().unwrap();
        propagation.close_publication();
    }

    #[test]
    fn quality_change_wakes_the_existing_worker_without_new_scene_input() {
        let (propagation, children) = start_propagation(true, 0.5, 8, 8);
        propagation
            .publish_scene(Arc::new(AcousticSceneSnapshot::new(3, Arc::new(UnitRoom))))
            .unwrap();
        propagation
            .publish_spatial_frame(Arc::new(
                SpatialFrame::new(4, 0.04, Pose::from_position(Vec3::ZERO), Vec::new()).unwrap(),
            ))
            .unwrap();

        let deadline = Instant::now() + Duration::from_secs(1);
        while propagation.diagnostics().solve_count == 0 {
            assert!(
                Instant::now() < deadline,
                "worker did not solve initial input"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let initial_solve_count = propagation.diagnostics().solve_count;
        propagation.set_quality(1.0);
        assert_eq!(propagation.quality(), 1.0);
        while propagation.diagnostics().solve_count == initial_solve_count {
            assert!(
                Instant::now() < deadline,
                "quality change did not wake the propagation worker"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let settled_solve_count = propagation.diagnostics().solve_count;
        propagation.set_quality(1.0);
        std::thread::sleep(Duration::from_millis(75));
        assert_eq!(propagation.diagnostics().solve_count, settled_solve_count);
        children.close().unwrap();
        propagation.close_publication();
    }
}
