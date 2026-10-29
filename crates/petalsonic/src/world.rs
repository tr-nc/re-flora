use crate::acoustics::AcousticSceneSnapshot;
use crate::domain::{
    Bus, BusParams, Emitter, EmitterDesc, PlayOptions, PlaybackControl, PlaybackTag, ResidentClip,
    SpatialFrame, VoiceId, validate_extent_pose, validate_pose,
};
use crate::error::{PetalSonicError, Result};
use crate::events::{
    AcousticTelemetryDiagnostics, AcousticTelemetryEvent, PetalSonicEvent, RenderTimingEvent,
    RuntimeDiagnostics, RuntimeState, RuntimeStatus, VoiceTelemetryDiagnostics,
    VoiceTelemetryEvent,
};
#[cfg(test)]
use crate::math::Pose;
use crate::playback::{AcceptedVoice, EmitterUpdate};
use crate::runtime::{AudioRuntime, RuntimeIntent};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

const MIN_PLAYBACK_RATE: f32 = 0.01;
const MAX_PLAYBACK_RATE: f32 = 4.0;
static NEXT_WORLD_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
struct EmitterState {
    clip: ResidentClip,
    desc: EmitterDesc,
}

#[derive(Clone, Copy)]
struct ControlledVoiceState {
    emitter: Emitter,
    detached: bool,
}

struct EmitterSlot {
    generation: u32,
    state: Option<EmitterState>,
}

struct EmitterRegistry {
    world_id: u64,
    slots: Vec<EmitterSlot>,
    free: Vec<u32>,
    len: usize,
    limit: usize,
}

/// A complete spatial frame checked against the currently locked Emitter registry.
///
/// Construction performs no mutation. Keeping the registry lock until `commit` means the
/// checked handles and completeness proof cannot become stale between runtime publication and
/// the caller-facing Emitter state update.
struct ValidatedSpatialUpdate {
    frame: Arc<SpatialFrame>,
}

impl ValidatedSpatialUpdate {
    fn publication(&self) -> Arc<SpatialFrame> {
        self.frame.clone()
    }
}

impl EmitterRegistry {
    fn new(limit: usize, world_id: u64) -> Self {
        Self {
            world_id,
            slots: Vec::with_capacity(limit),
            free: Vec::with_capacity(limit),
            len: 0,
            limit,
        }
    }

    fn insert(&mut self, state: EmitterState) -> Result<Emitter> {
        if self.len >= self.limit {
            return Err(PetalSonicError::CapacityExceeded {
                resource: "emitter",
                limit: self.limit,
            });
        }

        self.len += 1;
        if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            slot.state = Some(state);
            return Ok(Emitter {
                world_id: self.world_id,
                index,
                generation: slot.generation,
            });
        }

        let index = self.slots.len() as u32;
        self.slots.push(EmitterSlot {
            generation: 1,
            state: Some(state),
        });
        Ok(Emitter {
            world_id: self.world_id,
            index,
            generation: 1,
        })
    }

    fn get(&self, emitter: Emitter) -> Result<&EmitterState> {
        if emitter.world_id != self.world_id {
            return Err(PetalSonicError::StaleEmitter);
        }
        let slot = self
            .slots
            .get(emitter.index as usize)
            .ok_or(PetalSonicError::StaleEmitter)?;
        if slot.generation != emitter.generation {
            return Err(PetalSonicError::StaleEmitter);
        }
        slot.state.as_ref().ok_or(PetalSonicError::StaleEmitter)
    }

    fn get_mut(&mut self, emitter: Emitter) -> Result<&mut EmitterState> {
        if emitter.world_id != self.world_id {
            return Err(PetalSonicError::StaleEmitter);
        }
        let slot = self
            .slots
            .get_mut(emitter.index as usize)
            .ok_or(PetalSonicError::StaleEmitter)?;
        if slot.generation != emitter.generation {
            return Err(PetalSonicError::StaleEmitter);
        }
        slot.state.as_mut().ok_or(PetalSonicError::StaleEmitter)
    }

    fn remove(&mut self, emitter: Emitter) -> Result<EmitterState> {
        if emitter.world_id != self.world_id {
            return Err(PetalSonicError::StaleEmitter);
        }
        let slot = self
            .slots
            .get_mut(emitter.index as usize)
            .ok_or(PetalSonicError::StaleEmitter)?;
        if slot.generation != emitter.generation {
            return Err(PetalSonicError::StaleEmitter);
        }
        let state = slot.state.take().ok_or(PetalSonicError::StaleEmitter)?;
        slot.generation = slot.generation.wrapping_add(1).max(1);
        self.free.push(emitter.index);
        self.len -= 1;
        Ok(state)
    }

    fn prepare_spatial_update(&self, frame: Arc<SpatialFrame>) -> Result<ValidatedSpatialUpdate> {
        let expected = self
            .slots
            .iter()
            .filter_map(|slot| slot.state.as_ref())
            .filter(|state| state.desc.is_spatial())
            .count();
        if frame.emitters().len() != expected {
            return Err(PetalSonicError::InvalidConfiguration {
                field: "spatial_frame",
                reason: format!(
                    "expected {expected} spatial emitters, received {}",
                    frame.emitters().len()
                ),
            });
        }

        for spatial in frame.emitters() {
            let state = self.get(spatial.emitter)?;
            state.desc.extent().validate_update(spatial.extent())?;
            if !state.desc.is_spatial() {
                return Err(PetalSonicError::InvalidConfiguration {
                    field: "spatial_frame",
                    reason: format!("{} is not spatial", spatial.emitter),
                });
            }
        }

        Ok(ValidatedSpatialUpdate { frame })
    }

    fn commit_spatial_update(&mut self, update: ValidatedSpatialUpdate) {
        for spatial in update.frame.emitters() {
            let desc = &mut self
                .get_mut(spatial.emitter)
                .expect("validated spatial Emitter changed while its registry lock was held")
                .desc;
            desc.set_pose(spatial.pose);
            desc.set_extent(spatial.extent().clone());
        }
    }
}

/// Main facade for audio resources, emitters, playback, events, and runtime state.
///
/// Creating a world starts its private render runtime. Callers submit bounded,
/// non-blocking intent and never drive audio progress themselves.
pub struct PetalSonicWorld {
    world_id: u64,
    desc: crate::config::PetalSonicWorldDesc,
    emitters: Mutex<EmitterRegistry>,
    next_voice_id: AtomicU64,
    controlled_voices: Mutex<HashMap<VoiceId, ControlledVoiceState>>,
    runtime: AudioRuntime,
}

impl PetalSonicWorld {
    pub fn new(config: crate::config::PetalSonicWorldDesc) -> Result<Self> {
        Self::validate_config(&config)?;
        let runtime = AudioRuntime::start(&config)?;
        Ok(Self::from_runtime(config, runtime))
    }

    #[cfg(test)]
    fn new_with_output(
        config: crate::config::PetalSonicWorldDesc,
        output: impl FnOnce() -> Result<Box<dyn crate::platform::output::OutputPlatform>>
        + Send
        + 'static,
    ) -> Result<Self> {
        Self::validate_config(&config)?;
        let runtime = AudioRuntime::start_with_output(&config, output)?;
        Ok(Self::from_runtime(config, runtime))
    }

    fn from_runtime(config: crate::config::PetalSonicWorldDesc, runtime: AudioRuntime) -> Self {
        let world_id = NEXT_WORLD_ID.fetch_add(1, Ordering::Relaxed);
        Self {
            world_id,
            emitters: Mutex::new(EmitterRegistry::new(config.max_emitters, world_id)),
            controlled_voices: Mutex::new(HashMap::with_capacity(config.max_voices)),
            desc: config,
            next_voice_id: AtomicU64::new(0),
            runtime,
        }
    }

    fn validate_config(config: &crate::config::PetalSonicWorldDesc) -> Result<()> {
        for (field, value) in [
            ("sample_rate", config.sample_rate as usize),
            ("block_size", config.block_size),
            ("max_emitters", config.max_emitters),
            ("max_voices", config.max_voices),
            ("control_queue_capacity", config.control_queue_capacity),
            ("lifecycle_queue_capacity", config.lifecycle_queue_capacity),
            ("event_queue_capacity", config.event_queue_capacity),
            ("timing_queue_capacity", config.timing_queue_capacity),
        ] {
            if value == 0 {
                return Err(PetalSonicError::InvalidConfiguration {
                    field,
                    reason: "must be greater than zero".into(),
                });
            }
        }
        if !config.hrtf_gain.is_finite() {
            return Err(PetalSonicError::InvalidConfiguration {
                field: "hrtf_gain",
                reason: "must be finite".into(),
            });
        }
        if !config.distance_scaler.is_finite() || config.distance_scaler <= 0.0 {
            return Err(PetalSonicError::InvalidConfiguration {
                field: "distance_scaler",
                reason: "must be finite and greater than zero".into(),
            });
        }
        if !config.environmental_acoustics_quality.is_finite()
            || !(0.0..=1.0).contains(&config.environmental_acoustics_quality)
        {
            return Err(PetalSonicError::InvalidConfiguration {
                field: "environmental_acoustics_quality",
                reason: "must be finite and in the inclusive range 0.0..=1.0".into(),
            });
        }
        for (field, value) in [
            (
                "environmental_acoustics_budget.max_processed_extents",
                config.environmental_acoustics_budget.max_processed_extents,
            ),
            (
                "environmental_acoustics_budget.max_direct_rays",
                config.environmental_acoustics_budget.max_direct_rays,
            ),
        ] {
            if value == 0 {
                return Err(PetalSonicError::InvalidConfiguration {
                    field,
                    reason: "must be greater than zero".into(),
                });
            }
        }
        if config.buses.len() > config.max_buses {
            return Err(PetalSonicError::CapacityExceeded {
                resource: "bus",
                limit: config.max_buses,
            });
        }
        let mut names = HashSet::with_capacity(config.buses.len());
        for bus in &config.buses {
            let name = bus.name().trim();
            if name.is_empty() {
                return Err(PetalSonicError::InvalidConfiguration {
                    field: "buses",
                    reason: "bus names must not be empty".into(),
                });
            }
            if name.eq_ignore_ascii_case("Master") {
                return Err(PetalSonicError::InvalidConfiguration {
                    field: "buses",
                    reason: "Master is implicit and must not be declared".into(),
                });
            }
            if !names.insert(name.to_ascii_lowercase()) {
                return Err(PetalSonicError::InvalidConfiguration {
                    field: "buses",
                    reason: format!("duplicate bus name {name:?}"),
                });
            }
            Self::validate_bus_params(bus.params())?;
        }
        Ok(())
    }

    pub fn config(&self) -> &crate::config::PetalSonicWorldDesc {
        &self.desc
    }

    pub fn create_emitter(&self, clip: ResidentClip, desc: EmitterDesc) -> Result<Emitter> {
        self.runtime.ensure_open()?;
        Self::validate_emitter_desc(&desc)?;
        self.validate_optional_bus(desc.bus())?;
        let clip = clip.resampled_to(self.desc.sample_rate)?;
        self.emitters
            .lock()
            .map_err(|_| PetalSonicError::Engine("Emitter registry is poisoned".into()))?
            .insert(EmitterState { clip, desc })
    }

    pub fn update_emitter(&self, emitter: Emitter, desc: EmitterDesc) -> Result<()> {
        Self::validate_emitter_desc(&desc)?;
        self.validate_optional_bus(desc.bus())?;
        let bus_index = self.resolve_bus(desc.bus())?;
        let mut emitters = self
            .emitters
            .lock()
            .map_err(|_| PetalSonicError::Engine("Emitter registry is poisoned".into()))?;
        let state = emitters.get_mut(emitter)?;
        if state.desc.is_spatial() != desc.is_spatial() {
            return Err(PetalSonicError::InvalidConfiguration {
                field: "emitter_desc",
                reason: "spatial placement cannot change after emitter creation".into(),
            });
        }
        if state.desc.extent() != desc.extent() {
            return Err(PetalSonicError::InvalidConfiguration {
                field: "emitter_desc.extent",
                reason: "extent changes must be published in a complete SpatialFrame".into(),
            });
        }
        self.runtime
            .try_submit(RuntimeIntent::UpdateEmitter(EmitterUpdate::capture(
                emitter, &desc, bus_index,
            )))?;
        state.desc = desc;
        Ok(())
    }

    /// Publishes the latest complete listener + spatial-emitter transform set.
    ///
    /// An unconsumed older frame is replaced on the caller thread. The render thread
    /// observes only complete frame generations and never accumulates stale movement.
    pub fn publish_spatial_frame(&self, frame: SpatialFrame) -> Result<()> {
        self.runtime.ensure_open()?;
        let mut emitters = self
            .emitters
            .try_lock()
            .map_err(|_| PetalSonicError::QueuePressure)?;
        let (current_revision, current_sim_time) = self.runtime.spatial_cursor();
        if frame.revision() <= current_revision {
            return Err(PetalSonicError::InvalidConfiguration {
                field: "spatial_frame.revision",
                reason: format!(
                    "must increase monotonically beyond the current revision {current_revision}"
                ),
            });
        }
        if frame.sim_time_seconds() < current_sim_time {
            return Err(PetalSonicError::InvalidConfiguration {
                field: "spatial_frame.sim_time_seconds",
                reason: format!(
                    "must be finite and monotonic beyond the current time {current_sim_time}"
                ),
            });
        }
        let frame = Arc::new(frame);
        let update = emitters.prepare_spatial_update(frame)?;
        self.runtime.publish_spatial_frame(update.publication())?;
        emitters.commit_spatial_update(update);
        Ok(())
    }

    /// Publishes a newer immutable acoustic-scene version by swapping a shared handle.
    /// Geometry and unchanged BVH chunks remain owned and shared by the snapshot backend.
    pub fn publish_acoustic_scene(&self, snapshot: AcousticSceneSnapshot) -> Result<()> {
        self.runtime.ensure_open()?;
        let current = self.runtime.acoustic_scene_version();
        if snapshot.version() <= current {
            return Err(PetalSonicError::InvalidConfiguration {
                field: "acoustic_scene.version",
                reason: format!("must increase monotonically beyond the current version {current}"),
            });
        }
        let snapshot = Arc::new(snapshot);
        self.runtime.publish_acoustic_scene(snapshot)
    }

    /// Enables or disables all geometry-driven environmental effects at the next render block.
    ///
    /// This latest-value control does not rebuild the output runtime. Native HRTF
    /// spatialization, distance attenuation, air absorption, and playback remain active.
    pub fn set_environmental_acoustics_enabled(&self, enabled: bool) -> Result<()> {
        self.runtime.set_environmental_acoustics_enabled(enabled)
    }

    pub fn environmental_acoustics_enabled(&self) -> bool {
        self.runtime.environmental_acoustics_enabled()
    }

    /// Changes the bounded geometry-driven acoustics quality at the next propagation solve.
    ///
    /// This latest-value control does not rebuild the output runtime or interrupt playback.
    pub fn set_environmental_acoustics_quality(&self, quality: f32) -> Result<()> {
        self.runtime.ensure_open()?;
        if !quality.is_finite() || !(0.0..=1.0).contains(&quality) {
            return Err(PetalSonicError::InvalidConfiguration {
                field: "environmental_acoustics_quality",
                reason: "must be finite and in the inclusive range 0.0..=1.0".into(),
            });
        }
        self.runtime.set_environmental_acoustics_quality(quality)
    }

    pub fn environmental_acoustics_quality(&self) -> f32 {
        self.runtime.environmental_acoustics_quality()
    }

    pub fn destroy_emitter(&self, emitter: Emitter) -> Result<()> {
        let mut emitters = self
            .emitters
            .lock()
            .map_err(|_| PetalSonicError::Engine("Emitter registry is poisoned".into()))?;
        emitters.get(emitter)?;
        self.runtime
            .try_submit(RuntimeIntent::DestroyEmitter(emitter))?;
        emitters.remove(emitter)?;
        if let Ok(mut controlled) = self.controlled_voices.lock() {
            controlled.retain(|_, voice| voice.emitter != emitter || voice.detached);
        }
        Ok(())
    }

    pub fn play(&self, emitter: Emitter, options: PlayOptions) -> Result<()> {
        self.submit_play(emitter, options, None).map(|_| ())
    }

    pub fn play_controlled(
        &self,
        emitter: Emitter,
        options: PlayOptions,
        tag: PlaybackTag,
    ) -> Result<PlaybackControl> {
        let voice_id = self.submit_play(emitter, options, Some(tag))?;
        Ok(PlaybackControl {
            world_id: self.world_id,
            voice_id,
        })
    }

    fn submit_play(
        &self,
        emitter: Emitter,
        options: PlayOptions,
        completion_tag: Option<PlaybackTag>,
    ) -> Result<VoiceId> {
        self.drain_retired_controls();
        self.ensure_open()?;
        let state = self.emitter_state(emitter)?;
        Self::validate_playback_rate(options.playback_rate())?;
        Self::validate_spatial_routing(state.desc.is_spatial(), options)?;
        let total_gain_db = state.desc.gain_db() + options.gain_db;
        if !options.gain_db.is_finite() || !total_gain_db.is_finite() {
            return Err(PetalSonicError::InvalidConfiguration {
                field: "play_options.gain_db",
                reason: "Voice and combined emitter gain must be finite".into(),
            });
        }
        match options.direct_path().placement() {
            crate::domain::DirectPlacement::ListenerRelative(pose) => validate_extent_pose(
                "play_options.direct_path.listener_relative",
                pose,
                state.desc.extent(),
            )?,
            crate::domain::DirectPlacement::ListenerPositionRelative(pose) => validate_extent_pose(
                "play_options.direct_path.listener_position_relative",
                pose,
                state.desc.extent(),
            )?,
            _ => {}
        }
        if let crate::domain::EnvironmentOrigin::World(pose) = options.environment_send().origin() {
            validate_extent_pose(
                "play_options.environment_send.origin",
                pose,
                state.desc.extent(),
            )?;
        }
        let bus_index = self.resolve_bus(options.bus().or(state.desc.bus()))?;
        self.runtime.reserve_voice()?;
        let voice_id = VoiceId::from(self.next_voice_id.fetch_add(1, Ordering::Relaxed));
        if completion_tag.is_some() {
            let mut controlled = match self.controlled_voices.lock() {
                Ok(controlled) => controlled,
                Err(_) => {
                    self.runtime.release_reserved_voice();
                    return Err(PetalSonicError::Engine(
                        "Playback registry is poisoned".into(),
                    ));
                }
            };
            controlled.insert(
                voice_id,
                ControlledVoiceState {
                    emitter,
                    detached: options.detached,
                },
            );
        }
        let intent = RuntimeIntent::Play(AcceptedVoice::capture(
            voice_id,
            emitter,
            &state.clip,
            &state.desc,
            options,
            completion_tag,
            bus_index,
        ));
        if let Err(error) = self.runtime.try_submit(intent) {
            self.runtime.release_reserved_voice();
            if completion_tag.is_some()
                && let Ok(mut controlled) = self.controlled_voices.lock()
            {
                controlled.remove(&voice_id);
            }
            return Err(error);
        }
        Ok(voice_id)
    }

    pub fn pause_emitter(&self, emitter: Emitter) -> Result<()> {
        self.emitter_state(emitter)?;
        self.runtime
            .try_submit(RuntimeIntent::PauseEmitter(emitter))
    }

    pub fn resume_emitter(&self, emitter: Emitter) -> Result<()> {
        self.emitter_state(emitter)?;
        self.runtime
            .try_submit(RuntimeIntent::ResumeEmitter(emitter))
    }

    pub fn stop_emitter(&self, emitter: Emitter) -> Result<()> {
        self.emitter_state(emitter)?;
        self.runtime
            .try_submit(RuntimeIntent::StopEmitter(emitter))?;
        if let Ok(mut controlled) = self.controlled_voices.lock() {
            controlled.retain(|_, voice| voice.emitter != emitter);
        }
        Ok(())
    }

    pub fn seek_emitter(&self, emitter: Emitter, progress: f32) -> Result<()> {
        self.emitter_state(emitter)?;
        self.runtime
            .try_submit(RuntimeIntent::SeekEmitter(emitter, progress))
    }

    pub fn pause_playback(&self, control: PlaybackControl) -> Result<()> {
        self.ensure_controlled(control)?;
        self.runtime
            .try_submit(RuntimeIntent::PauseVoice(control.voice_id))
    }

    pub fn resume_playback(&self, control: PlaybackControl) -> Result<()> {
        self.ensure_controlled(control)?;
        self.runtime
            .try_submit(RuntimeIntent::ResumeVoice(control.voice_id))
    }

    pub fn set_playback_rate(&self, control: PlaybackControl, playback_rate: f32) -> Result<()> {
        self.ensure_controlled(control)?;
        Self::validate_playback_rate(playback_rate)?;
        self.runtime
            .try_submit(RuntimeIntent::SetVoiceRate(control.voice_id, playback_rate))
    }

    pub fn stop_playback(&self, control: PlaybackControl) -> Result<()> {
        self.ensure_controlled(control)?;
        self.runtime
            .try_submit(RuntimeIntent::StopVoice(control.voice_id))?;
        self.controlled_voices
            .lock()
            .map_err(|_| PetalSonicError::Engine("Playback registry is poisoned".into()))?
            .remove(&control.voice_id);
        Ok(())
    }

    pub fn seek_playback(&self, control: PlaybackControl, progress: f32) -> Result<()> {
        self.ensure_controlled(control)?;
        self.runtime
            .try_submit(RuntimeIntent::SeekVoice(control.voice_id, progress))
    }

    pub fn stop_all(&self) -> Result<()> {
        self.runtime.try_submit(RuntimeIntent::StopAll)?;
        self.controlled_voices
            .lock()
            .map_err(|_| PetalSonicError::Engine("Playback registry is poisoned".into()))?
            .clear();
        Ok(())
    }

    /// Returns the implicit Master bus.
    pub fn master_bus(&self) -> Bus {
        Bus {
            world_id: self.world_id,
            index: 0,
        }
    }

    /// Resolves a declared bus by name. Matching is case-insensitive.
    pub fn bus(&self, name: &str) -> Option<Bus> {
        if name.eq_ignore_ascii_case("Master") {
            return Some(self.master_bus());
        }
        self.desc
            .buses
            .iter()
            .position(|bus| bus.name().eq_ignore_ascii_case(name))
            .and_then(|index| u16::try_from(index + 1).ok())
            .map(|index| Bus {
                world_id: self.world_id,
                index,
            })
    }

    pub fn set_bus_params(&self, bus: Bus, params: BusParams) -> Result<()> {
        let index = self.resolve_bus(Some(bus))?;
        Self::validate_bus_params(params)?;
        self.runtime.set_bus_params(index, params)
    }

    pub fn bus_params(&self, bus: Bus) -> Result<BusParams> {
        let index = self.resolve_bus(Some(bus))?;
        self.runtime.bus_params(index)
    }

    fn validate_optional_bus(&self, bus: Option<Bus>) -> Result<()> {
        self.resolve_bus(bus).map(|_| ())
    }

    fn resolve_bus(&self, bus: Option<Bus>) -> Result<usize> {
        let Some(bus) = bus else {
            return Ok(0);
        };
        let index = bus.index as usize;
        if bus.world_id != self.world_id || index > self.desc.buses.len() {
            return Err(PetalSonicError::StaleBus);
        }
        Ok(index)
    }

    fn validate_bus_params(params: BusParams) -> Result<()> {
        if !params.gain_db.is_finite() {
            return Err(PetalSonicError::InvalidConfiguration {
                field: "bus.gain_db",
                reason: "must be finite".into(),
            });
        }
        Self::validate_playback_rate(params.playback_rate)
    }

    fn validate_playback_rate(playback_rate: f32) -> Result<()> {
        if playback_rate.is_finite()
            && (MIN_PLAYBACK_RATE..=MAX_PLAYBACK_RATE).contains(&playback_rate)
        {
            Ok(())
        } else {
            Err(PetalSonicError::InvalidConfiguration {
                field: "playback_rate",
                reason: format!("must be between {MIN_PLAYBACK_RATE} and {MAX_PLAYBACK_RATE}"),
            })
        }
    }

    fn validate_spatial_routing(is_spatial: bool, options: PlayOptions) -> Result<()> {
        if options.acoustic_start() && !matches!(options.loop_mode, crate::playback::LoopMode::Once)
        {
            return Err(PetalSonicError::InvalidConfiguration {
                field: "play_options.acoustic_start",
                reason: "acoustic start requires finite playback".into(),
            });
        }
        if !is_spatial
            && (options.has_spatial_routing_override()
                || options.play_command_id().is_some()
                || options.acoustic_start())
        {
            return Err(PetalSonicError::InvalidConfiguration {
                field: "play_options.spatial_routing",
                reason: "DirectPath, EnvironmentSend, acoustic start, and spatial render telemetry require a spatial emitter"
                    .into(),
            });
        }

        let direct_path = options.direct_path();
        match direct_path.placement() {
            crate::domain::DirectPlacement::ListenerRelative(pose) => {
                validate_pose("play_options.direct_path.listener_relative", pose)?
            }
            crate::domain::DirectPlacement::ListenerPositionRelative(pose) => {
                validate_pose("play_options.direct_path.listener_position_relative", pose)?
            }
            _ => {}
        }
        if matches!(
            direct_path.placement(),
            crate::domain::DirectPlacement::Disabled
        ) && !matches!(
            direct_path.geometry(),
            crate::domain::DirectGeometry::BypassTransmission
        ) {
            return Err(PetalSonicError::InvalidConfiguration {
                field: "play_options.direct_path.geometry",
                reason: "a disabled DirectPath must bypass transmission".into(),
            });
        }

        let environment_send = options.environment_send();
        if !environment_send.gain_db().is_finite() {
            return Err(PetalSonicError::InvalidConfiguration {
                field: "play_options.environment_send.gain_db",
                reason: "must be finite".into(),
            });
        }
        match environment_send.origin() {
            crate::domain::EnvironmentOrigin::World(pose) => {
                validate_pose("play_options.environment_send.origin", pose)?;
            }
            crate::domain::EnvironmentOrigin::Disabled
                if environment_send.gain_db().to_bits() != 0.0f32.to_bits() =>
            {
                return Err(PetalSonicError::InvalidConfiguration {
                    field: "play_options.environment_send.gain_db",
                    reason: "a disabled EnvironmentSend must use 0 dB".into(),
                });
            }
            _ => {}
        }
        Ok(())
    }

    fn validate_emitter_desc(desc: &EmitterDesc) -> Result<()> {
        if !desc.gain_db().is_finite() {
            return Err(PetalSonicError::InvalidConfiguration {
                field: "emitter_desc.gain_db",
                reason: "must be finite".into(),
            });
        }
        match desc.pose() {
            Some(pose) => {
                validate_pose("emitter_desc.pose", pose)?;
                validate_extent_pose("emitter_desc.extent", pose, desc.extent())
            }
            None if !matches!(desc.extent(), crate::domain::SourceExtent::Point)
                || !matches!(
                    desc.occlusion_profile(),
                    crate::domain::OcclusionProfile::PointExact
                ) =>
            {
                Err(PetalSonicError::InvalidConfiguration {
                    field: "emitter_desc.extent",
                    reason: "source extent and occlusion profile require a spatial emitter".into(),
                })
            }
            None => Ok(()),
        }
    }

    fn emitter_state(&self, emitter: Emitter) -> Result<EmitterState> {
        self.emitters
            .lock()
            .map_err(|_| PetalSonicError::Engine("Emitter registry is poisoned".into()))?
            .get(emitter)
            .cloned()
    }

    fn ensure_controlled(&self, control: PlaybackControl) -> Result<()> {
        self.drain_retired_controls();
        if control.world_id == self.world_id
            && self
                .controlled_voices
                .lock()
                .map_err(|_| PetalSonicError::Engine("Playback registry is poisoned".into()))?
                .contains_key(&control.voice_id)
        {
            Ok(())
        } else {
            Err(PetalSonicError::StalePlayback)
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.desc.sample_rate
    }

    pub fn is_running(&self) -> bool {
        self.runtime.runtime_status().state == RuntimeState::Running
    }

    pub fn runtime_status(&self) -> RuntimeStatus {
        self.runtime.runtime_status()
    }

    pub fn diagnostics(&self) -> RuntimeDiagnostics {
        let active_emitters = self
            .emitters
            .lock()
            .map(|emitters| emitters.len)
            .unwrap_or_default();
        self.runtime.diagnostics(active_emitters, &self.desc)
    }

    pub fn active_voice_count(&self) -> usize {
        self.runtime.active_voice_count()
    }

    pub fn frames_processed(&self) -> usize {
        self.runtime.frames_processed()
    }

    pub fn underrun_count(&self) -> usize {
        self.runtime.underrun_count()
    }

    pub fn drain_events(&self) -> Vec<PetalSonicEvent> {
        let events = self.runtime.drain_events();
        self.drain_retired_controls();
        if let Ok(mut controlled) = self.controlled_voices.lock() {
            for event in &events {
                if let PetalSonicEvent::PlaybackCompleted { control, .. } = event {
                    controlled.remove(&control.voice_id);
                }
            }
        }
        events
    }

    /// Drains opt-in per-Voice render telemetry without consuming lifecycle events.
    pub fn drain_voice_telemetry(&self) -> Vec<VoiceTelemetryEvent> {
        self.runtime.drain_voice_telemetry()
    }

    /// Reports pressure on the independently bounded Voice telemetry queue.
    pub fn voice_telemetry_diagnostics(&self) -> VoiceTelemetryDiagnostics {
        self.runtime.voice_telemetry_diagnostics()
    }

    /// Drains worker-side source-extent telemetry without consuming Voice or lifecycle events.
    pub fn drain_acoustic_telemetry(&self) -> Vec<AcousticTelemetryEvent> {
        self.runtime.drain_acoustic_telemetry()
    }

    /// Reports pressure on the independently bounded acoustics telemetry queue.
    pub fn acoustic_telemetry_diagnostics(&self) -> AcousticTelemetryDiagnostics {
        self.runtime.acoustic_telemetry_diagnostics()
    }

    fn drain_retired_controls(&self) {
        if let Ok(mut controlled) = self.controlled_voices.lock() {
            for voice_id in self.runtime.drain_retired_voice_ids() {
                controlled.remove(&voice_id);
            }
        }
    }

    pub fn drain_timing_events(&self) -> Vec<RenderTimingEvent> {
        self.runtime.drain_timing_events()
    }

    pub fn close(&self) -> Result<()> {
        self.runtime.close()
    }

    fn ensure_open(&self) -> Result<()> {
        self.runtime.ensure_open()
    }
}

impl Drop for PetalSonicWorld {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn acoustic_start_rejects_nonspatial_and_looping_contracts() {
        use crate::domain::PlayOptions;
        assert!(
            super::PetalSonicWorld::validate_spatial_routing(
                false,
                PlayOptions::once().with_acoustic_start()
            )
            .is_err()
        );
        assert!(
            super::PetalSonicWorld::validate_spatial_routing(
                true,
                PlayOptions::looping().with_acoustic_start()
            )
            .is_err()
        );
    }

    use super::*;
    use crate::acoustics::{AcousticHit, AcousticRay, AcousticRayQuerySnapshot};
    use crate::audio_data::PetalSonicAudioData;
    use crate::domain::{
        DirectGeometry, DirectPath, EnvironmentSend, ExtentSample, ExtentSampleId, PlayCommandId,
        SourceExtent,
    };
    use crate::events::{AcousticTelemetryEvent, VoiceTelemetryEvent};
    use crate::platform::output::fake::{
        FakeDevice as PlatformFakeDevice, FakeOutputPlatform, FakeSampleFormat,
    };
    use std::time::Instant;

    struct OpenAcoustics;

    impl AcousticRayQuerySnapshot for OpenAcoustics {
        fn trace_any_hit_batch(
            &self,
            rays: &[AcousticRay],
            _min_distances: &[f32],
            _max_distances: &[f32],
            hits: &mut [bool],
        ) {
            assert_eq!(rays.len(), hits.len());
            hits.fill(false);
        }

        fn trace_closest_hit_batch(
            &self,
            rays: &[AcousticRay],
            _min_distances: &[f32],
            _max_distances: &[f32],
            hits: &mut [Option<AcousticHit>],
        ) {
            assert_eq!(rays.len(), hits.len());
            hits.fill(None);
        }
    }

    struct BlockingShutdownAcoustics {
        first_call: std::sync::atomic::AtomicBool,
        entered: Mutex<bool>,
        entered_changed: std::sync::Condvar,
        released: Mutex<bool>,
        released_changed: std::sync::Condvar,
    }

    impl BlockingShutdownAcoustics {
        fn new() -> Self {
            Self {
                first_call: std::sync::atomic::AtomicBool::new(true),
                entered: Mutex::new(false),
                entered_changed: std::sync::Condvar::new(),
                released: Mutex::new(false),
                released_changed: std::sync::Condvar::new(),
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
                "acoustics worker never entered the controlled solve"
            );
        }

        fn release(&self) {
            *self.released.lock().unwrap() = true;
            self.released_changed.notify_all();
        }
    }

    impl AcousticRayQuerySnapshot for BlockingShutdownAcoustics {
        fn trace_any_hit_batch(
            &self,
            rays: &[AcousticRay],
            _min_distances: &[f32],
            _max_distances: &[f32],
            hits: &mut [bool],
        ) {
            assert_eq!(rays.len(), hits.len());
            hits.fill(false);
        }

        fn trace_closest_hit_batch(
            &self,
            rays: &[AcousticRay],
            _min_distances: &[f32],
            _max_distances: &[f32],
            hits: &mut [Option<AcousticHit>],
        ) {
            assert_eq!(rays.len(), hits.len());
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

    #[test]
    fn spatial_routing_rejects_invalid_or_inapplicable_policies() {
        let local_nan = PlayOptions::once().with_direct_path(DirectPath::listener_relative(
            Pose::from_position(crate::math::Vec3::splat(f32::NAN)),
        ));
        assert!(PetalSonicWorld::validate_spatial_routing(true, local_nan).is_err());

        let position_relative_nan =
            PlayOptions::once().with_direct_path(DirectPath::listener_position_relative(
                Pose::from_position(crate::math::Vec3::splat(f32::NAN)),
            ));
        assert!(PetalSonicWorld::validate_spatial_routing(true, position_relative_nan).is_err());

        let disabled_with_geometry = PlayOptions::once().with_direct_path(
            DirectPath::disabled().with_geometry(DirectGeometry::SimulatedTransmission),
        );
        assert!(PetalSonicWorld::validate_spatial_routing(true, disabled_with_geometry).is_err());

        let non_spatial_override = PlayOptions::once()
            .with_environment_send(EnvironmentSend::from_world_pose(Pose::identity()));
        assert!(PetalSonicWorld::validate_spatial_routing(false, non_spatial_override).is_err());

        let non_spatial_telemetry = PlayOptions::once().with_play_command_id(PlayCommandId(1));
        assert!(PetalSonicWorld::validate_spatial_routing(false, non_spatial_telemetry).is_err());

        let invalid_gain = PlayOptions::once()
            .with_environment_send(EnvironmentSend::follow_emitter().with_gain_db(f32::NAN));
        assert!(PetalSonicWorld::validate_spatial_routing(true, invalid_gain).is_err());
    }
    use std::time::Duration;

    fn clip() -> ResidentClip {
        ResidentClip::from_audio_data(Arc::new(PetalSonicAudioData::new(
            vec![0.0; 16],
            48_000,
            1,
            Duration::from_secs_f64(16.0 / 48_000.0),
        )))
    }

    fn wait_for_async_observation(mut predicate: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !predicate() {
            assert!(Instant::now() < deadline, "World observation timed out");
            std::thread::park_timeout(Duration::from_millis(1));
        }
    }

    #[test]
    fn world_keeps_emitter_and_voice_identity_across_fake_output_recovery() {
        let a = PlatformFakeDevice::stereo("A", 48_000);
        let mut b = PlatformFakeDevice::stereo("B", 96_000);
        b.state.physical_channels = 6;
        b.sample_format = FakeSampleFormat::U16;
        let (adapter, handle) = FakeOutputPlatform::scripted(vec![a, b], Some(0));
        let desc = crate::config::PetalSonicWorldDesc {
            environmental_acoustics_enabled: false,
            ..Default::default()
        };
        let world = PetalSonicWorld::new_with_output(desc, move || Ok(Box::new(adapter))).unwrap();
        let emitter = world
            .create_emitter(clip(), EmitterDesc::non_spatial())
            .unwrap();
        let control = world
            .play_controlled(emitter, PlayOptions::looping(), PlaybackTag(77))
            .unwrap();

        wait_for_async_observation(|| {
            world.runtime_status().active_output_device.as_deref() == Some("A")
        });
        assert_eq!(world.active_voice_count(), 1);
        assert_eq!(world.diagnostics().output_sample_rate, 48_000);
        assert_eq!(world.diagnostics().output_channels, 2);

        handle.set_selected(None);
        handle.fail_stream();
        wait_for_async_observation(|| world.runtime_status().state == RuntimeState::Recovering);

        let one_shot = world
            .play_controlled(emitter, PlayOptions::once(), PlaybackTag(78))
            .unwrap();
        let mut completion = None;
        wait_for_async_observation(|| {
            completion = world.drain_events().into_iter().find(|event| {
                matches!(
                    event,
                    PetalSonicEvent::PlaybackCompleted {
                        tag: PlaybackTag(78),
                        ..
                    }
                )
            });
            completion.is_some()
        });
        assert_eq!(
            completion,
            Some(PetalSonicEvent::PlaybackCompleted {
                emitter,
                control: one_shot,
                tag: PlaybackTag(78),
            })
        );

        handle.set_selected(Some(1));
        wait_for_async_observation(|| {
            world.runtime_status().active_output_device.as_deref() == Some("B")
        });

        assert_eq!(world.runtime_status().state, RuntimeState::Running);
        assert_eq!(world.active_voice_count(), 1);
        world.pause_playback(control).unwrap();
        world.resume_playback(control).unwrap();
        world
            .update_emitter(emitter, EmitterDesc::non_spatial())
            .unwrap();
        assert_eq!(world.diagnostics().output_sample_rate, 96_000);
        assert_eq!(world.diagnostics().output_channels, 6);
        assert!(world.diagnostics().device_generation >= 2);
        assert!(world.runtime_status().recovery_attempts >= 3);
        world.close().unwrap();
    }

    #[test]
    fn world_observes_permanent_output_failure() {
        let mut device = PlatformFakeDevice::stereo("unsupported", 48_000);
        device.sample_format = FakeSampleFormat::Unsupported;
        let (adapter, _) = FakeOutputPlatform::scripted(vec![device], Some(0));
        let desc = crate::config::PetalSonicWorldDesc {
            environmental_acoustics_enabled: false,
            ..Default::default()
        };
        let world = PetalSonicWorld::new_with_output(desc, move || Ok(Box::new(adapter))).unwrap();

        wait_for_async_observation(|| world.runtime_status().state == RuntimeState::Failed);
        let emitter = world
            .create_emitter(clip(), EmitterDesc::non_spatial())
            .unwrap_err();
        assert!(matches!(emitter, PetalSonicError::RuntimeFailed));
        world.close().unwrap();
    }

    #[test]
    fn world_surfaces_enabled_acoustics_worker_failure_in_status_and_close() {
        let device = PlatformFakeDevice::stereo("A", 48_000);
        let (adapter, _) = FakeOutputPlatform::scripted(vec![device], Some(0));
        let desc = crate::config::PetalSonicWorldDesc {
            environmental_acoustics_enabled: true,
            ..Default::default()
        };
        let world = PetalSonicWorld::new_with_output(desc, move || Ok(Box::new(adapter))).unwrap();

        world.runtime.fail_acoustic_worker_for_test();
        wait_for_async_observation(|| world.runtime_status().state == RuntimeState::Failed);
        let first_close_error = world
            .close()
            .expect_err("an abnormal acoustics exit must remain visible during close");
        let repeated_close_error = world
            .close()
            .expect_err("the terminal child failure must remain visible on close reentry");
        assert!(
            first_close_error.to_string().contains("acoustics"),
            "close reported an unrelated failure: {first_close_error}"
        );
        assert_eq!(
            repeated_close_error.to_string(),
            first_close_error.to_string(),
            "close reentry must observe the original terminal outcome"
        );
        assert_eq!(world.runtime_status().state, RuntimeState::Closed);
    }

    #[test]
    fn concurrent_close_observers_share_one_terminal_child_failure() {
        let device = PlatformFakeDevice::stereo("A", 48_000);
        let (adapter, _) = FakeOutputPlatform::scripted(vec![device], Some(0));
        let desc = crate::config::PetalSonicWorldDesc {
            environmental_acoustics_enabled: true,
            ..Default::default()
        };
        let world = PetalSonicWorld::new_with_output(desc, move || Ok(Box::new(adapter))).unwrap();
        world.runtime.fail_acoustic_worker_for_test();
        wait_for_async_observation(|| world.runtime_status().state == RuntimeState::Failed);

        let start = std::sync::Barrier::new(3);
        let (left, right) = std::thread::scope(|scope| {
            let left = scope.spawn(|| {
                start.wait();
                world.close()
            });
            let right = scope.spawn(|| {
                start.wait();
                world.close()
            });
            start.wait();
            (left.join().unwrap(), right.join().unwrap())
        });
        let left = left.expect_err("every close observer must see the terminal child failure");
        let right = right.expect_err("every close observer must see the terminal child failure");

        assert_eq!(left.to_string(), right.to_string());
        assert!(left.to_string().contains("acoustics"));
        assert_eq!(world.runtime_status().state, RuntimeState::Closed);
    }

    #[test]
    fn normal_render_and_runtime_shutdown_does_not_report_a_child_failure() {
        let device = PlatformFakeDevice::stereo("A", 48_000);
        let (adapter, _) = FakeOutputPlatform::scripted(vec![device], Some(0));
        let desc = crate::config::PetalSonicWorldDesc {
            environmental_acoustics_enabled: true,
            ..Default::default()
        };
        let world = PetalSonicWorld::new_with_output(desc, move || Ok(Box::new(adapter))).unwrap();

        world.close().unwrap();
        world.close().unwrap();
        assert_eq!(world.runtime_status().state, RuntimeState::Closed);
    }

    #[test]
    fn shutdown_keeps_the_acoustic_consumer_alive_until_the_producer_is_quiesced() {
        let acoustics = Arc::new(BlockingShutdownAcoustics::new());
        let device = PlatformFakeDevice::stereo("ordered-shutdown", 48_000);
        let (adapter, output) = FakeOutputPlatform::scripted(vec![device], Some(0));
        let world = PetalSonicWorld::new_with_output(
            crate::config::PetalSonicWorldDesc {
                acoustic_scene: Some(AcousticSceneSnapshot::new(1, acoustics.clone())),
                environmental_acoustics_enabled: true,
                ..Default::default()
            },
            move || Ok(Box::new(adapter)),
        )
        .unwrap();
        let emitter = world
            .create_emitter(clip(), EmitterDesc::spatial(Pose::identity()))
            .unwrap();
        world.play(emitter, PlayOptions::looping()).unwrap();
        world
            .publish_spatial_frame(
                SpatialFrame::new(
                    1,
                    0.0,
                    Pose::identity(),
                    vec![crate::domain::EmitterSpatialState::new(
                        emitter,
                        Pose::identity(),
                    )],
                )
                .unwrap(),
            )
            .unwrap();
        acoustics.wait_until_entered();

        std::thread::scope(|scope| {
            let close = scope.spawn(|| world.close());
            wait_for_async_observation(|| world.runtime_status().state == RuntimeState::Closing);

            let observation_deadline = Instant::now() + Duration::from_millis(100);
            while Instant::now() < observation_deadline && !output.actions().contains(&"shutdown") {
                std::thread::park_timeout(Duration::from_millis(1));
            }
            let output_stopped_before_acoustics = output.actions().contains(&"shutdown");
            acoustics.release();

            let close_result = close.join().unwrap();
            assert!(
                !output_stopped_before_acoustics,
                "output consumer stopped while its acoustic producer was still solving"
            );
            close_result.expect("ordered runtime shutdown must not report consumer disconnection");
        });
        assert_eq!(world.runtime_status().state, RuntimeState::Closed);
        assert_eq!(
            output
                .actions()
                .iter()
                .filter(|action| **action == "shutdown")
                .count(),
            1
        );
    }

    #[test]
    fn world_surfaces_render_child_panic_in_status_and_close() {
        let device = PlatformFakeDevice::stereo("A", 48_000);
        let (adapter, _) = FakeOutputPlatform::scripted(vec![device], Some(0));
        let desc = crate::config::PetalSonicWorldDesc {
            environmental_acoustics_enabled: false,
            ..Default::default()
        };
        let world = PetalSonicWorld::new_with_output(desc, move || Ok(Box::new(adapter))).unwrap();
        wait_for_async_observation(|| world.runtime_status().state == RuntimeState::Running);

        world.runtime.panic_render_worker_for_test();
        wait_for_async_observation(|| world.runtime_status().state == RuntimeState::Failed);
        let close_error = world
            .close()
            .expect_err("a render child panic must remain visible during close");
        assert!(
            close_error.to_string().contains("render"),
            "close did not attribute the render failure: {close_error}"
        );
        assert_eq!(world.runtime_status().state, RuntimeState::Closed);
    }

    #[test]
    fn world_classifies_acoustics_child_panic_during_close() {
        let device = PlatformFakeDevice::stereo("A", 48_000);
        let (adapter, _) = FakeOutputPlatform::scripted(vec![device], Some(0));
        let desc = crate::config::PetalSonicWorldDesc {
            environmental_acoustics_enabled: true,
            ..Default::default()
        };
        let world = PetalSonicWorld::new_with_output(desc, move || Ok(Box::new(adapter))).unwrap();

        world.runtime.panic_acoustic_worker_for_test();
        wait_for_async_observation(|| world.runtime_status().state == RuntimeState::Failed);
        let close_error = world
            .close()
            .expect_err("an acoustics child panic must remain visible during close");
        assert!(
            close_error.to_string().contains("acoustics: panicked"),
            "close did not classify the child panic: {close_error}"
        );
        assert_eq!(world.runtime_status().state, RuntimeState::Closed);
    }

    #[test]
    fn bus_declarations_are_bounded_unique_and_exclude_master() {
        let mut desc = crate::config::PetalSonicWorldDesc {
            max_buses: 1,
            buses: vec![crate::domain::BusDesc::new("Gameplay")],
            ..Default::default()
        };
        assert!(PetalSonicWorld::validate_config(&desc).is_ok());

        desc.buses.push(crate::domain::BusDesc::new("Music"));
        assert!(matches!(
            PetalSonicWorld::validate_config(&desc),
            Err(PetalSonicError::CapacityExceeded {
                resource: "bus",
                limit: 1
            })
        ));

        desc.max_buses = 2;
        desc.buses = vec![
            crate::domain::BusDesc::new("Gameplay"),
            crate::domain::BusDesc::new("gameplay"),
        ];
        assert!(PetalSonicWorld::validate_config(&desc).is_err());

        desc.buses = vec![crate::domain::BusDesc::new("Master")];
        assert!(PetalSonicWorld::validate_config(&desc).is_err());
    }

    #[test]
    fn static_configuration_and_spatial_backend_fail_before_world_is_returned() {
        let desc = crate::config::PetalSonicWorldDesc {
            distance_scaler: 0.0,
            ..Default::default()
        };
        assert!(matches!(
            PetalSonicWorld::new(desc),
            Err(PetalSonicError::InvalidConfiguration {
                field: "distance_scaler",
                ..
            })
        ));

        for environmental_acoustics_quality in [-0.01, 1.01, f32::NAN] {
            let desc = crate::config::PetalSonicWorldDesc {
                environmental_acoustics_quality,
                ..Default::default()
            };
            assert!(matches!(
                PetalSonicWorld::validate_config(&desc),
                Err(PetalSonicError::InvalidConfiguration {
                    field: "environmental_acoustics_quality",
                    ..
                })
            ));
        }

        for environmental_acoustics_budget in [
            crate::config::EnvironmentalAcousticsBudget {
                max_processed_extents: 0,
                ..Default::default()
            },
            crate::config::EnvironmentalAcousticsBudget {
                max_direct_rays: 0,
                ..Default::default()
            },
        ] {
            let desc = crate::config::PetalSonicWorldDesc {
                environmental_acoustics_budget,
                ..Default::default()
            };
            assert!(PetalSonicWorld::validate_config(&desc).is_err());
        }

        let desc = crate::config::PetalSonicWorldDesc {
            spatial_quality: crate::config::SpatialQuality::LowLatency,
            native_hrtf_path: Some("/petalsonic/definitely-missing.petalhrtf".into()),
            output_device: crate::config::OutputDevicePolicy::PinnedNameContains(
                "petalsonic-test-device-that-does-not-exist".into(),
            ),
            ..Default::default()
        };
        assert!(matches!(
            PetalSonicWorld::new(desc),
            Err(PetalSonicError::BackendUnavailable {
                backend: "spatial renderer",
                ..
            })
        ));
    }

    #[test]
    fn near_capacity_runtime_remains_bounded_under_snapshot_and_bus_pressure() {
        const EMITTERS: usize = 24;
        const VOICES: usize = 48;
        let desc = crate::config::PetalSonicWorldDesc {
            max_emitters: EMITTERS,
            max_voices: VOICES,
            max_buses: 1,
            buses: vec![crate::domain::BusDesc::new("Gameplay")],
            control_queue_capacity: 128,
            lifecycle_queue_capacity: 64,
            output_device: crate::config::OutputDevicePolicy::PinnedNameContains(
                "petalsonic-test-device-that-does-not-exist".into(),
            ),
            ..Default::default()
        };
        let world = PetalSonicWorld::new(desc).unwrap();
        let mut emitters = Vec::with_capacity(EMITTERS);
        for index in 0..EMITTERS {
            let pose = Pose::from_position(crate::math::Vec3::new(index as f32, 0.0, 0.0));
            emitters.push(
                world
                    .create_emitter(clip(), EmitterDesc::spatial(pose))
                    .unwrap(),
            );
        }
        assert!(matches!(
            world.create_emitter(clip(), EmitterDesc::non_spatial()),
            Err(PetalSonicError::CapacityExceeded {
                resource: "emitter",
                limit: EMITTERS
            })
        ));

        for emitter in &emitters {
            world.play(*emitter, PlayOptions::looping()).unwrap();
            world.play(*emitter, PlayOptions::looping()).unwrap();
        }
        assert_eq!(world.active_voice_count(), VOICES);
        assert!(matches!(
            world.play(emitters[0], PlayOptions::looping()),
            Err(PetalSonicError::CapacityExceeded {
                resource: "voice",
                limit: VOICES
            })
        ));

        let gameplay = world.bus("Gameplay").unwrap();
        let mut queue_pressure_observed = false;
        for generation in 0usize..1_024 {
            let states = emitters
                .iter()
                .enumerate()
                .map(|(index, emitter)| {
                    crate::domain::EmitterSpatialState::new(
                        *emitter,
                        Pose::from_position(crate::math::Vec3::new(
                            index as f32,
                            generation as f32 * 0.001,
                            0.0,
                        )),
                    )
                })
                .collect();
            world
                .publish_spatial_frame(
                    SpatialFrame::new(
                        generation as u64 + 1,
                        generation as f64 * 0.001,
                        Pose::default(),
                        states,
                    )
                    .unwrap(),
                )
                .unwrap();

            let params = BusParams {
                gain_db: if generation.is_multiple_of(2) {
                    -3.0
                } else {
                    0.0
                },
                ..Default::default()
            };
            match world.set_bus_params(gameplay, params) {
                Ok(()) => {}
                Err(PetalSonicError::QueuePressure) => queue_pressure_observed = true,
                Err(error) => panic!("unexpected bus update failure: {error}"),
            }
        }

        let diagnostics = world.diagnostics();
        assert_eq!(diagnostics.active_emitters, EMITTERS);
        assert_eq!(diagnostics.active_voices, VOICES);
        assert!(diagnostics.control_queue_depth <= 128);
        assert!(diagnostics.control_queue_high_water <= 128);
        assert_eq!(queue_pressure_observed, diagnostics.rejected_commands > 0);

        world.stop_all().unwrap();
        world.close().unwrap();
        assert_eq!(world.runtime_status().state, RuntimeState::Closed);
    }

    #[test]
    fn bounded_event_pressure_is_observable() {
        let desc = crate::config::PetalSonicWorldDesc {
            max_emitters: 1,
            max_voices: 16,
            control_queue_capacity: 32,
            event_queue_capacity: 1,
            output_device: crate::config::OutputDevicePolicy::PinnedNameContains(
                "petalsonic-test-device-that-does-not-exist".into(),
            ),
            ..Default::default()
        };
        let world = PetalSonicWorld::new(desc).unwrap();
        let emitter = world
            .create_emitter(clip(), EmitterDesc::non_spatial())
            .unwrap();
        for tag in 0..16 {
            world
                .play_controlled(emitter, PlayOptions::once(), PlaybackTag(tag))
                .unwrap();
        }

        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline && world.diagnostics().dropped_events == 0 {
            std::thread::yield_now();
        }

        let diagnostics = world.diagnostics();
        assert_eq!(diagnostics.event_queue_high_water, 1);
        assert!(diagnostics.event_queue_depth <= 1);
        assert!(diagnostics.dropped_events > 0);
        world.close().unwrap();
    }

    #[test]
    fn spatial_publication_accepts_latest_complete_frames_without_control_queue_growth() {
        let desc = crate::config::PetalSonicWorldDesc {
            output_device: crate::config::OutputDevicePolicy::PinnedNameContains(
                "petalsonic-test-device-that-does-not-exist".into(),
            ),
            ..Default::default()
        };
        let world = PetalSonicWorld::new(desc).unwrap();
        let emitter = world
            .create_emitter(clip(), EmitterDesc::spatial(Pose::default()))
            .unwrap();
        let first_listener = Pose::from_position(crate::math::Vec3::new(1.0, 0.0, 0.0));
        let second_listener = Pose::from_position(crate::math::Vec3::new(2.0, 0.0, 0.0));
        let first_emitter = Pose::from_position(crate::math::Vec3::new(10.0, 0.0, 0.0));
        let second_emitter = Pose::from_position(crate::math::Vec3::new(20.0, 0.0, 0.0));

        world
            .publish_spatial_frame(
                SpatialFrame::new(
                    1,
                    0.0,
                    first_listener,
                    vec![crate::domain::EmitterSpatialState::new(
                        emitter,
                        first_emitter,
                    )],
                )
                .unwrap(),
            )
            .unwrap();
        world
            .publish_spatial_frame(
                SpatialFrame::new(
                    2,
                    0.1,
                    second_listener,
                    vec![crate::domain::EmitterSpatialState::new(
                        emitter,
                        second_emitter,
                    )],
                )
                .unwrap(),
            )
            .unwrap();

        assert!(matches!(
            world.publish_spatial_frame(
                SpatialFrame::new(
                    1,
                    0.2,
                    second_listener,
                    vec![crate::domain::EmitterSpatialState::new(
                        emitter,
                        second_emitter,
                    )],
                )
                .unwrap()
            ),
            Err(PetalSonicError::InvalidConfiguration {
                field: "spatial_frame.revision",
                ..
            })
        ));
        assert_eq!(world.diagnostics().control_queue_depth, 0);
        world.close().unwrap();
    }

    #[test]
    fn world_spatial_publication_updates_acoustics_once_before_render_observes_the_frame() {
        let (adapter, _) = FakeOutputPlatform::scripted(
            vec![PlatformFakeDevice::stereo(
                "spatial-production-gate",
                48_000,
            )],
            Some(0),
        );
        let world = PetalSonicWorld::new_with_output(
            crate::config::PetalSonicWorldDesc {
                max_emitters: 1,
                max_voices: 2,
                environmental_acoustics_enabled: false,
                ..Default::default()
            },
            move || Ok(Box::new(adapter)),
        )
        .unwrap();
        let old_pose = Pose::from_position(-crate::math::Vec3::Y);
        let new_pose = Pose::from_position(crate::math::Vec3::new(4.0, 2.0, 1.0));
        let emitter = world
            .create_emitter(clip(), EmitterDesc::spatial(old_pose))
            .unwrap();
        let attached = world
            .play_controlled(emitter, PlayOptions::looping(), PlaybackTag(81))
            .unwrap();
        let detached = world
            .play_controlled(emitter, PlayOptions::looping().detached(), PlaybackTag(82))
            .unwrap();

        wait_for_async_observation(|| {
            world
                .runtime
                .acoustic_voice_spatial_for_test(attached.voice_id)
                .is_some()
                && world
                    .runtime
                    .acoustic_voice_spatial_for_test(detached.voice_id)
                    .is_some()
        });
        let wake_before = world.runtime.acoustic_wake_generation_for_test();

        world
            .publish_spatial_frame(
                SpatialFrame::new(
                    1,
                    0.1,
                    Pose::identity(),
                    vec![
                        crate::domain::EmitterSpatialState::new(emitter, new_pose)
                            .with_acoustic_priority(7.25),
                    ],
                )
                .unwrap(),
            )
            .unwrap();
        wait_for_async_observation(|| world.runtime.rendered_spatial_revision_for_test() == 1);

        assert_eq!(
            world
                .runtime
                .acoustic_voice_spatial_for_test(attached.voice_id),
            Some((new_pose, 7.25, false))
        );
        assert_eq!(
            world
                .runtime
                .acoustic_voice_spatial_for_test(detached.voice_id),
            Some((old_pose, 1.0, true)),
            "a detached acoustic origin belongs to its Voice lifetime"
        );
        let expected_wake = wake_before + 1;
        assert_eq!(
            world.runtime.acoustic_wake_generation_for_test(),
            expected_wake,
            "one World publication must wake its acoustic worker exactly once"
        );
        let stability_deadline = Instant::now() + Duration::from_millis(20);
        while Instant::now() < stability_deadline {
            assert_eq!(
                world.runtime.acoustic_wake_generation_for_test(),
                expected_wake,
                "render consumption produced a delayed second acoustic wake"
            );
            std::thread::park_timeout(Duration::from_millis(1));
        }

        world.close().unwrap();
    }

    #[test]
    fn failed_runtime_spatial_publication_preserves_the_pose_and_extent_for_the_next_voice() {
        let (adapter, _) = FakeOutputPlatform::scripted(
            vec![PlatformFakeDevice::stereo("spatial-transaction", 48_000)],
            Some(0),
        );
        let desc = crate::config::PetalSonicWorldDesc {
            acoustic_scene: Some(AcousticSceneSnapshot::new(1, Arc::new(OpenAcoustics))),
            ..Default::default()
        };
        let world = PetalSonicWorld::new_with_output(desc, move || Ok(Box::new(adapter))).unwrap();
        let old_pose = Pose::from_position(crate::math::Vec3::new(10.0, 2.0, -3.0));
        let new_pose = Pose::from_position(crate::math::Vec3::new(-20.0, 4.0, 7.0));
        let old_extent = SourceExtent::weighted_samples(vec![
            ExtentSample::new(
                ExtentSampleId(41),
                crate::math::Vec3::new(-1.0, 0.0, 0.0),
                1.0,
            )
            .unwrap(),
            ExtentSample::new(
                ExtentSampleId(42),
                crate::math::Vec3::new(1.0, 0.0, 0.0),
                1.0,
            )
            .unwrap(),
        ])
        .unwrap();
        let new_extent = SourceExtent::weighted_samples(vec![
            ExtentSample::new(
                ExtentSampleId(91),
                crate::math::Vec3::new(0.0, 1.0, 0.0),
                1.0,
            )
            .unwrap(),
        ])
        .unwrap();
        let emitter = world
            .create_emitter(
                clip(),
                EmitterDesc::spatial(old_pose).with_extent(old_extent.clone()),
            )
            .unwrap();
        world
            .publish_spatial_frame(
                SpatialFrame::new(
                    1,
                    1.0,
                    Pose::identity(),
                    vec![
                        crate::domain::EmitterSpatialState::new(emitter, old_pose)
                            .with_extent(old_extent),
                    ],
                )
                .unwrap(),
            )
            .unwrap();

        let failed = world.runtime.with_spatial_publication_blocked(|| {
            world.publish_spatial_frame(
                SpatialFrame::new(
                    2,
                    2.0,
                    Pose::identity(),
                    vec![
                        crate::domain::EmitterSpatialState::new(emitter, new_pose)
                            .with_extent(new_extent.clone()),
                    ],
                )
                .unwrap(),
            )
        });
        assert!(matches!(failed, Err(PetalSonicError::QueuePressure)));

        world
            .play(
                emitter,
                PlayOptions::looping().with_play_command_id(PlayCommandId(700)),
            )
            .unwrap();
        let mut first_render = None;
        wait_for_async_observation(|| {
            first_render =
                world
                    .drain_voice_telemetry()
                    .into_iter()
                    .find_map(|event| match event {
                        VoiceTelemetryEvent::FirstRendered(event)
                            if event.play_command_id == PlayCommandId(700) =>
                        {
                            Some(event)
                        }
                        _ => None,
                    });
            first_render.is_some()
        });
        let first_render = first_render.unwrap();
        assert_eq!(first_render.spatial_revision, 1);
        assert_eq!(first_render.acoustic_origin, Some(old_pose));

        let mut extent_response = None;
        wait_for_async_observation(|| {
            extent_response = world
                .drain_acoustic_telemetry()
                .into_iter()
                .find_map(|event| match event {
                    AcousticTelemetryEvent::ExtentResponse(response)
                        if response.emitter == emitter =>
                    {
                        Some(response)
                    }
                    _ => None,
                });
            extent_response.is_some()
        });
        let extent_response = extent_response.unwrap();
        assert_eq!(extent_response.spatial_revision, 1);
        assert_eq!(extent_response.extent_sample_count, 2);
        assert_eq!(
            extent_response
                .direct
                .samples
                .iter()
                .map(|sample| sample.sample_id)
                .collect::<Vec<_>>(),
            vec![ExtentSampleId(41), ExtentSampleId(42)]
        );
        assert_eq!(
            extent_response
                .direct
                .samples
                .iter()
                .map(|sample| sample.world_position)
                .collect::<Vec<_>>(),
            vec![
                old_pose.position + crate::math::Vec3::new(-1.0, 0.0, 0.0),
                old_pose.position + crate::math::Vec3::new(1.0, 0.0, 0.0),
            ]
        );

        world
            .publish_spatial_frame(
                SpatialFrame::new(
                    2,
                    2.0,
                    Pose::identity(),
                    vec![
                        crate::domain::EmitterSpatialState::new(emitter, new_pose)
                            .with_extent(new_extent),
                    ],
                )
                .unwrap(),
            )
            .expect("the failed revision and time must remain available for retry");
        world.close().unwrap();
    }

    #[test]
    fn world_play_delivers_captured_routes_and_playback_rate_to_render_behavior() {
        let (adapter, handle) = FakeOutputPlatform::scripted(
            vec![PlatformFakeDevice::stereo("voice-admission", 48_000)],
            Some(0),
        );
        let world = PetalSonicWorld::new_with_output(
            crate::config::PetalSonicWorldDesc {
                block_size: 64,
                ..Default::default()
            },
            move || Ok(Box::new(adapter)),
        )
        .unwrap();
        wait_for_async_observation(|| world.runtime_status().state == RuntimeState::Running);
        let emitter_pose = Pose::from_position(crate::math::Vec3::new(6.0, 1.0, -4.0));
        let emitter = world
            .create_emitter(
                ResidentClip::from_audio_data(Arc::new(PetalSonicAudioData::new(
                    vec![0.25; 4_096],
                    48_000,
                    1,
                    Duration::from_secs_f64(4_096.0 / 48_000.0),
                ))),
                EmitterDesc::spatial(emitter_pose),
            )
            .unwrap();
        world
            .publish_spatial_frame(
                SpatialFrame::new(
                    1,
                    0.0,
                    Pose::identity(),
                    vec![crate::domain::EmitterSpatialState::new(
                        emitter,
                        emitter_pose,
                    )],
                )
                .unwrap(),
            )
            .unwrap();
        let direct_pose = Pose::from_position(crate::math::Vec3::new(0.2, -0.1, 0.5));
        let acoustic_origin = Pose::from_position(crate::math::Vec3::new(9.0, 3.0, -7.0));
        world
            .play(
                emitter,
                PlayOptions::once()
                    .with_playback_rate(2.0)
                    .with_direct_path(DirectPath::listener_relative(direct_pose))
                    .with_environment_send(EnvironmentSend::from_world_pose(acoustic_origin))
                    .with_play_command_id(PlayCommandId(801)),
            )
            .unwrap();
        let slow = world
            .play_controlled(
                emitter,
                PlayOptions::once()
                    .with_playback_rate(0.5)
                    .with_play_command_id(PlayCommandId(802)),
                PlaybackTag(802),
            )
            .unwrap();

        let mut first_render = None;
        wait_for_async_observation(|| {
            handle.advance(64);
            first_render =
                world
                    .drain_voice_telemetry()
                    .into_iter()
                    .find_map(|event| match event {
                        VoiceTelemetryEvent::FirstRendered(event)
                            if event.play_command_id == PlayCommandId(801) =>
                        {
                            Some(event)
                        }
                        _ => None,
                    });
            first_render.is_some()
        });
        let first_render = first_render.unwrap();
        assert_eq!(first_render.emitter, emitter);
        assert_eq!(first_render.direct_local_pose, Some(direct_pose));
        assert_eq!(first_render.acoustic_origin, Some(acoustic_origin));

        let mut observed_completions = Vec::new();
        wait_for_async_observation(|| {
            handle.advance(64);
            observed_completions.extend(world.drain_events());
            world.active_voice_count() == 1
        });
        assert!(
            !observed_completions.iter().any(|event| {
                matches!(
                    event,
                    PetalSonicEvent::PlaybackCompleted {
                        control,
                        tag: PlaybackTag(802),
                        ..
                    } if *control == slow
                )
            }),
            "the 0.5x Voice completed no later than the 2.0x Voice"
        );
        wait_for_async_observation(|| {
            handle.advance(64);
            observed_completions.extend(world.drain_events());
            observed_completions.iter().any(|event| {
                matches!(
                    event,
                    PetalSonicEvent::PlaybackCompleted {
                        control,
                        tag: PlaybackTag(802),
                        ..
                    } if *control == slow
                )
            })
        });
        world.close().unwrap();
    }

    #[test]
    fn spatial_publication_rejects_torn_or_non_monotonic_solver_inputs() {
        let desc = crate::config::PetalSonicWorldDesc {
            output_device: crate::config::OutputDevicePolicy::PinnedNameContains(
                "petalsonic-test-device-that-does-not-exist".into(),
            ),
            ..Default::default()
        };
        let world = PetalSonicWorld::new(desc).unwrap();
        let emitter = world
            .create_emitter(clip(), EmitterDesc::spatial(Pose::default()))
            .unwrap();
        let frame = |revision, sim_time_seconds, priority| {
            SpatialFrame::new(
                revision,
                sim_time_seconds,
                Pose::default(),
                vec![
                    crate::domain::EmitterSpatialState::new(emitter, Pose::default())
                        .with_acoustic_priority(priority),
                ],
            )
        };

        world
            .publish_spatial_frame(frame(1, 1.0, 1.0).unwrap())
            .unwrap();
        assert!(matches!(
            world.publish_spatial_frame(frame(1, 2.0, 1.0).unwrap()),
            Err(PetalSonicError::InvalidConfiguration {
                field: "spatial_frame.revision",
                ..
            })
        ));
        assert!(matches!(
            world.publish_spatial_frame(frame(2, 0.5, 1.0).unwrap()),
            Err(PetalSonicError::InvalidConfiguration {
                field: "spatial_frame.sim_time_seconds",
                ..
            })
        ));
        assert!(matches!(
            frame(2, 1.0, f32::NAN),
            Err(PetalSonicError::InvalidConfiguration {
                field: "spatial_frame.emitters.acoustic_priority",
                ..
            })
        ));
        world
            .publish_spatial_frame(frame(2, 1.0, 0.0).unwrap())
            .unwrap();
        world.close().unwrap();
    }

    #[test]
    fn world_can_be_recreated_after_close() {
        let desc = crate::config::PetalSonicWorldDesc {
            output_device: crate::config::OutputDevicePolicy::PinnedNameContains(
                "petalsonic-test-device-that-does-not-exist".into(),
            ),
            ..Default::default()
        };

        let first = PetalSonicWorld::new(desc.clone()).unwrap();
        first.close().unwrap();
        drop(first);

        let second = PetalSonicWorld::new(desc).unwrap();
        second.close().unwrap();
    }

    #[test]
    fn detached_control_survives_emitter_destruction() {
        let desc = crate::config::PetalSonicWorldDesc {
            output_device: crate::config::OutputDevicePolicy::PinnedNameContains(
                "petalsonic-test-device-that-does-not-exist".into(),
            ),
            ..Default::default()
        };
        let world = PetalSonicWorld::new(desc).unwrap();
        let emitter = world
            .create_emitter(clip(), EmitterDesc::non_spatial())
            .unwrap();
        let control = world
            .play_controlled(emitter, PlayOptions::looping().detached(), PlaybackTag(7))
            .unwrap();

        world.destroy_emitter(emitter).unwrap();
        assert!(matches!(
            world.play(emitter, PlayOptions::once()),
            Err(PetalSonicError::StaleEmitter)
        ));
        world.pause_playback(control).unwrap();
        world.stop_playback(control).unwrap();
        world.close().unwrap();
    }

    #[test]
    fn bounded_event_pressure_then_detached_shutdown() {
        bounded_event_pressure_is_observable();
        detached_control_survives_emitter_destruction();
    }

    #[test]
    fn emitter_registry_rejects_stale_generation_after_slot_reuse() {
        let mut registry = EmitterRegistry::new(1, 1);
        let first = registry
            .insert(EmitterState {
                clip: clip(),
                desc: EmitterDesc::default(),
            })
            .unwrap();
        registry.remove(first).unwrap();
        let second = registry
            .insert(EmitterState {
                clip: clip(),
                desc: EmitterDesc::default(),
            })
            .unwrap();

        assert_eq!(first.index, second.index);
        assert_ne!(first.generation, second.generation);
        assert!(matches!(
            registry.get(first),
            Err(PetalSonicError::StaleEmitter)
        ));
    }

    #[test]
    fn emitter_registry_enforces_capacity_and_recovers_after_remove() {
        let mut registry = EmitterRegistry::new(1, 1);
        let emitter = registry
            .insert(EmitterState {
                clip: clip(),
                desc: EmitterDesc::default(),
            })
            .unwrap();
        assert!(matches!(
            registry.insert(EmitterState {
                clip: clip(),
                desc: EmitterDesc::default(),
            }),
            Err(PetalSonicError::CapacityExceeded {
                resource: "emitter",
                limit: 1
            })
        ));
        registry.remove(emitter).unwrap();
        registry
            .insert(EmitterState {
                clip: clip(),
                desc: EmitterDesc::default(),
            })
            .unwrap();
    }

    #[test]
    fn validated_spatial_frame_needs_no_second_membership_allocation() {
        const EMITTERS: usize = 2_048;
        let mut registry = EmitterRegistry::new(EMITTERS, 1);
        let resident_clip = clip();
        let mut spatial_emitters = Vec::with_capacity(EMITTERS);
        for index in 0..EMITTERS {
            let pose = Pose::from_position(crate::math::Vec3::new(index as f32, 0.0, 0.0));
            let emitter = registry
                .insert(EmitterState {
                    clip: resident_clip.clone(),
                    desc: EmitterDesc::spatial(pose),
                })
                .unwrap();
            spatial_emitters.push(crate::domain::EmitterSpatialState::new(emitter, pose));
        }
        let frame =
            Arc::new(SpatialFrame::new(1, 0.0, Pose::identity(), spatial_emitters).unwrap());
        let mut update = None;

        let memory_activity = crate::test_support::realtime_memory_activity(|| {
            update = Some(registry.prepare_spatial_update(frame.clone()).unwrap());
        });

        assert_eq!(memory_activity, 0);
        assert_eq!(update.unwrap().frame.emitters().len(), EMITTERS);
    }

    #[test]
    fn spatial_frame_must_be_complete_before_any_pose_is_updated() {
        let mut registry = EmitterRegistry::new(2, 1);
        let first = registry
            .insert(EmitterState {
                clip: clip(),
                desc: EmitterDesc::spatial(Pose::default()),
            })
            .unwrap();
        let second = registry
            .insert(EmitterState {
                clip: clip(),
                desc: EmitterDesc::spatial(Pose::default()),
            })
            .unwrap();
        let moved = Pose::from_position(crate::math::Vec3::new(1.0, 2.0, 3.0));

        let incomplete = SpatialFrame::new(
            1,
            0.0,
            Pose::default(),
            vec![crate::domain::EmitterSpatialState::new(first, moved)],
        )
        .unwrap();
        assert!(matches!(
            registry.prepare_spatial_update(Arc::new(incomplete)),
            Err(PetalSonicError::InvalidConfiguration {
                field: "spatial_frame",
                ..
            })
        ));
        assert_eq!(
            registry.get(first).unwrap().desc.pose(),
            Some(Pose::default())
        );

        let complete = SpatialFrame::new(
            2,
            0.1,
            Pose::default(),
            vec![
                crate::domain::EmitterSpatialState::new(first, moved),
                crate::domain::EmitterSpatialState::new(second, Pose::default()),
            ],
        )
        .unwrap();
        let update = registry.prepare_spatial_update(Arc::new(complete)).unwrap();
        registry.commit_spatial_update(update);
        assert_eq!(registry.get(first).unwrap().desc.pose(), Some(moved));
    }

    #[test]
    fn emitter_and_spatial_frame_poses_require_finite_nonzero_rotations() {
        use crate::math::{Quat, Vec3};

        assert!(
            PetalSonicWorld::validate_emitter_desc(&EmitterDesc::spatial(Pose::from_position(
                Vec3::splat(f32::NAN)
            )))
            .is_err()
        );
        assert!(matches!(
            SpatialFrame::new(
                1,
                0.0,
                Pose::from_rotation(Quat::from_xyzw(0.0, 0.0, 0.0, 0.0)),
                Vec::new(),
            ),
            Err(PetalSonicError::InvalidConfiguration {
                field: "spatial_frame.listener",
                ..
            })
        ));
    }

    #[test]
    fn non_spatial_emitters_reject_source_extent_semantics() {
        use crate::domain::{ExtentSample, ExtentSampleId, SourceExtent};

        let extent = SourceExtent::weighted_samples(vec![
            ExtentSample::new(ExtentSampleId(1), crate::math::Vec3::ZERO, 1.0).unwrap(),
        ])
        .unwrap();
        assert!(
            PetalSonicWorld::validate_emitter_desc(&EmitterDesc::non_spatial().with_extent(extent))
                .is_err()
        );
    }
}
