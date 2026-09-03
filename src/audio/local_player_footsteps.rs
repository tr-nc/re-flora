use super::spatial_sound_manager::{
    AcousticPipelineSnapshot, SpatialFramePublication, TransientSpatialEmitter,
};
use super::{LocalFootstepTelemetryObservations, SpatialSoundManager};
use crate::gameplay::camera::{FootstepEvent, FootstepKind, Gait};
use anyhow::Result;
use petalsonic::{
    AcousticRouteOutcome, AcousticSolveStatus, AcousticVoiceConclusionTelemetry, DirectGeometry,
    DirectPath, DirectPropagation, EnvironmentResponse, EnvironmentSend, LateReverbTelemetry,
    PetalSonicEvent, PlayCommandId, PlayOptions, PlaybackControl, PlaybackTag, Pose,
    Vec3 as PetalVec3, VoiceEnergyTelemetry, VoiceTelemetryEvent,
};

const LOCAL_FOOTSTEP_DIRECT_WORLD_Y: f32 = -0.08;
const LOCAL_FOOTSTEP_DIRECT_RADIUS_TOLERANCE: f32 = 1.0e-5;
const LOCAL_FOOTSTEP_ENVIRONMENT_GAIN_DB: f32 = -12.0;
const MAX_ACTIVE_LOCAL_FOOTSTEPS: usize = 6;
const COMPLETION_DEADLINE_GRACE_SECONDS: f64 = 1.0;
const TELEMETRY_CONCLUSION_GRACE_SECONDS: f64 = 1.0;
const LOCAL_FOOTSTEP_CORRELATION_NAMESPACE: u64 = 1 << 63;

pub(crate) fn local_footstep_correlation_id(event_seq: u64) -> u64 {
    LOCAL_FOOTSTEP_CORRELATION_NAMESPACE
        .checked_add(event_seq)
        .expect("local footstep telemetry correlation ID overflowed")
}

fn local_footstep_event_seq(correlation_id: u64) -> Option<u64> {
    correlation_id.checked_sub(LOCAL_FOOTSTEP_CORRELATION_NAMESPACE)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LocalPlayerFootstepRoutingMode {
    #[default]
    SplitSpatial,
    Legacy2d,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct FootstepRouting {
    direct_world_offset: glam::Vec3,
    environment_world: glam::Vec3,
}

impl FootstepRouting {
    fn for_event(event: &FootstepEvent) -> Self {
        Self {
            direct_world_offset: glam::Vec3::new(0.0, LOCAL_FOOTSTEP_DIRECT_WORLD_Y, 0.0),
            environment_world: event.contact_world,
        }
    }

    fn direct_world_offset_pose(self) -> Pose {
        Pose::from_position(Self::petal_position(self.direct_world_offset))
    }

    fn environment_pose(self) -> Pose {
        Pose::from_position(Self::petal_position(self.environment_world))
    }

    fn petal_position(position: glam::Vec3) -> PetalVec3 {
        PetalVec3::new(position.x, position.y, position.z)
    }

    fn direct_path(self) -> DirectPath {
        DirectPath::listener_position_relative(self.direct_world_offset_pose())
            .with_geometry(DirectGeometry::BypassTransmission)
            .with_propagation(DirectPropagation::Immediate)
    }

    fn matches_resolved_direct_pose(self, direct_local_pose: Option<Pose>) -> bool {
        direct_local_pose.is_some_and(|pose| {
            pose.position.is_finite()
                && (pose.position.length() - self.direct_world_offset.length()).abs()
                    <= LOCAL_FOOTSTEP_DIRECT_RADIUS_TOLERANCE
        })
    }

    fn play_options(self, correlation_id: u64) -> PlayOptions {
        PlayOptions::once()
            .with_direct_path(self.direct_path())
            .with_environment_send(
                EnvironmentSend::from_world_pose(self.environment_pose())
                    .with_gain_db(LOCAL_FOOTSTEP_ENVIRONMENT_GAIN_DB),
            )
            .with_play_command_id(PlayCommandId(correlation_id))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ManagedFootstepVoice<EmitterHandle, ControlHandle> {
    event_seq: u64,
    emitter: EmitterHandle,
    control: ControlHandle,
    routing: FootstepRouting,
    published_revision: u64,
    emitter_gain_db: f32,
    clip_duration_seconds: f64,
    wet_observation: FootstepWetObservation,
    acoustics_at_play: AcousticPipelineSnapshot,
    completion_deadline_seconds: f64,
}

impl<EmitterHandle, ControlHandle> ManagedFootstepVoice<EmitterHandle, ControlHandle> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        event_seq: u64,
        emitter: EmitterHandle,
        control: ControlHandle,
        routing: FootstepRouting,
        published_revision: u64,
        emitter_gain_db: f32,
        clip_duration_seconds: f64,
        acoustics_at_play: AcousticPipelineSnapshot,
        completion_deadline_seconds: f64,
    ) -> Self {
        Self {
            event_seq,
            emitter,
            control,
            routing,
            published_revision,
            emitter_gain_db,
            clip_duration_seconds,
            wet_observation: FootstepWetObservation::default(),
            acoustics_at_play,
            completion_deadline_seconds,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum EnvironmentResponseTiming {
    #[default]
    Unobserved,
    ReadyAtFirstRender,
    ArrivedDuringPlayback,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct FootstepAcousticConclusion {
    voice_id: u64,
    spatial_revision: u64,
    geometry_version: u64,
    candidate_rank: Option<usize>,
    candidate_limit: usize,
    direct: AcousticRouteOutcome,
    environment: AcousticRouteOutcome,
    environment_transmission_gain: [f32; 3],
    early_tap_count: usize,
    solve_status: Option<AcousticSolveStatus>,
}

impl From<AcousticVoiceConclusionTelemetry> for FootstepAcousticConclusion {
    fn from(telemetry: AcousticVoiceConclusionTelemetry) -> Self {
        Self {
            voice_id: telemetry.voice_id,
            spatial_revision: telemetry.spatial_revision,
            geometry_version: telemetry.geometry_version,
            candidate_rank: telemetry.candidate_rank,
            candidate_limit: telemetry.candidate_limit,
            direct: telemetry.direct,
            environment: telemetry.environment,
            environment_transmission_gain: telemetry.environment_transmission_gain,
            early_tap_count: telemetry.early_tap_count,
            solve_status: telemetry.solve_status,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct FootstepEnergySummary {
    source_energy: f64,
    direct_energy: f64,
    environment_send_energy: f64,
    early_reflection_energy: f64,
    global_late_reverb: LateReverbTelemetry,
}

impl From<VoiceEnergyTelemetry> for FootstepEnergySummary {
    fn from(telemetry: VoiceEnergyTelemetry) -> Self {
        Self {
            source_energy: telemetry.source_energy,
            direct_energy: telemetry.direct_energy,
            environment_send_energy: telemetry.environment_send_energy,
            early_reflection_energy: telemetry.early_reflection_energy,
            global_late_reverb: telemetry.late_reverb,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct FootstepWetObservation {
    first_render_block: Option<u64>,
    response_timing: EnvironmentResponseTiming,
    response: Option<EnvironmentResponse>,
    acoustic_conclusion: Option<FootstepAcousticConclusion>,
    energy_summary: Option<FootstepEnergySummary>,
}

impl FootstepWetObservation {
    fn record_first_render(
        &mut self,
        render_block_index: u64,
        response: Option<EnvironmentResponse>,
    ) {
        self.first_render_block.get_or_insert(render_block_index);
        if self.response_timing == EnvironmentResponseTiming::Unobserved {
            if let Some(response) = response {
                self.response_timing = EnvironmentResponseTiming::ReadyAtFirstRender;
                self.response = Some(response);
            }
        }
    }

    fn record_environment_response(&mut self, response: EnvironmentResponse) {
        if self.response_timing == EnvironmentResponseTiming::Unobserved {
            self.response_timing = EnvironmentResponseTiming::ArrivedDuringPlayback;
            self.response = Some(response);
        }
    }

    fn record_acoustic_conclusion(&mut self, conclusion: FootstepAcousticConclusion) {
        self.acoustic_conclusion = Some(conclusion);
    }

    fn record_energy_summary(&mut self, summary: FootstepEnergySummary) {
        self.energy_summary = Some(summary);
    }

    fn ready_for_final_summary(self, acoustics_enabled_at_play: bool) -> bool {
        self.energy_summary.is_some()
            && (!acoustics_enabled_at_play || self.acoustic_conclusion.is_some())
    }

    fn outcome(
        self,
        acoustics_enabled_at_play: bool,
        telemetry_was_dropped: bool,
    ) -> FootstepWetOutcome {
        if telemetry_was_dropped {
            return FootstepWetOutcome::TelemetryIncomplete;
        }
        if self.energy_summary.is_none() {
            return FootstepWetOutcome::TelemetryNotReceivedBeforeDeadline;
        }
        match self
            .acoustic_conclusion
            .map(|conclusion| conclusion.environment)
        {
            Some(AcousticRouteOutcome::ExcludedByBudget) => {
                FootstepWetOutcome::EnvironmentExcludedByBudget
            }
            Some(AcousticRouteOutcome::Disabled) => FootstepWetOutcome::AcousticsDisabled,
            Some(AcousticRouteOutcome::Applied) => match self.response_timing {
                EnvironmentResponseTiming::ReadyAtFirstRender => {
                    FootstepWetOutcome::ResponseReadyAtFirstRender
                }
                EnvironmentResponseTiming::ArrivedDuringPlayback => {
                    FootstepWetOutcome::ResponseArrivedDuringPlayback
                }
                EnvironmentResponseTiming::Unobserved => {
                    FootstepWetOutcome::TelemetryNotReceivedBeforeDeadline
                }
            },
            None if !acoustics_enabled_at_play => FootstepWetOutcome::AcousticsDisabled,
            None => FootstepWetOutcome::TelemetryNotReceivedBeforeDeadline,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FootstepWetOutcome {
    AcousticsDisabled,
    EnvironmentExcludedByBudget,
    TelemetryIncomplete,
    TelemetryNotReceivedBeforeDeadline,
    ResponseReadyAtFirstRender,
    ResponseArrivedDuringPlayback,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TelemetryObservationStatus {
    Observed,
    NotExpectedAcousticsDisabled,
    NotReceivedBeforeDeadline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FootstepTelemetryAttribution {
    Accepted,
    UnknownVoice,
    WrongEmitter,
}

fn correlated_telemetry_attribution<EmitterHandle, ControlHandle>(
    voice: Option<&ManagedFootstepVoice<EmitterHandle, ControlHandle>>,
    emitter_matches: impl FnOnce(&EmitterHandle) -> bool,
) -> FootstepTelemetryAttribution {
    match voice {
        Some(voice) if emitter_matches(&voice.emitter) => FootstepTelemetryAttribution::Accepted,
        Some(_) => FootstepTelemetryAttribution::WrongEmitter,
        None => FootstepTelemetryAttribution::UnknownVoice,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FirstRenderContractCheck {
    revision_ok: bool,
    direct_ok: bool,
    environment_ok: bool,
}

impl FirstRenderContractCheck {
    fn for_voice<EmitterHandle, ControlHandle>(
        voice: &ManagedFootstepVoice<EmitterHandle, ControlHandle>,
        spatial_revision: u64,
        direct_local_pose: Option<Pose>,
        acoustic_origin: Option<Pose>,
    ) -> Self {
        Self {
            revision_ok: spatial_revision >= voice.published_revision,
            direct_ok: voice
                .routing
                .matches_resolved_direct_pose(direct_local_pose),
            environment_ok: acoustic_origin == Some(voice.routing.environment_pose()),
        }
    }

    fn satisfied(self) -> bool {
        self.revision_ok && self.direct_ok && self.environment_ok
    }
}

struct ActiveFootstepRegistry<EmitterHandle, ControlHandle> {
    capacity: usize,
    voices: Vec<ManagedFootstepVoice<EmitterHandle, ControlHandle>>,
}

impl<EmitterHandle, ControlHandle> ActiveFootstepRegistry<EmitterHandle, ControlHandle> {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            voices: Vec::with_capacity(capacity),
        }
    }

    fn activate(&mut self, voice: ManagedFootstepVoice<EmitterHandle, ControlHandle>) {
        assert!(self.voices.len() < self.capacity);
        self.voices.push(voice);
    }

    fn complete(
        &mut self,
        event_seq: u64,
    ) -> Option<ManagedFootstepVoice<EmitterHandle, ControlHandle>> {
        let index = self
            .voices
            .iter()
            .position(|voice| voice.event_seq == event_seq)?;
        Some(self.voices.remove(index))
    }

    fn take_oldest(&mut self) -> Option<ManagedFootstepVoice<EmitterHandle, ControlHandle>> {
        (!self.voices.is_empty()).then(|| self.voices.remove(0))
    }

    fn take_expired(
        &mut self,
        sim_time_seconds: f64,
    ) -> Vec<ManagedFootstepVoice<EmitterHandle, ControlHandle>> {
        let mut expired = Vec::new();
        let mut index = 0;
        while index < self.voices.len() {
            if self.voices[index].completion_deadline_seconds <= sim_time_seconds {
                expired.push(self.voices.remove(index));
            } else {
                index += 1;
            }
        }
        expired
    }

    fn get(&self, event_seq: u64) -> Option<&ManagedFootstepVoice<EmitterHandle, ControlHandle>> {
        self.voices
            .iter()
            .find(|voice| voice.event_seq == event_seq)
    }

    fn get_mut(
        &mut self,
        event_seq: u64,
    ) -> Option<&mut ManagedFootstepVoice<EmitterHandle, ControlHandle>> {
        self.voices
            .iter_mut()
            .find(|voice| voice.event_seq == event_seq)
    }

    fn len(&self) -> usize {
        self.voices.len()
    }

    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.voices.is_empty()
    }
}

struct FootstepClipBank {
    walk_paths: Vec<String>,
    jump_paths: Vec<String>,
    land_paths: Vec<String>,
    run_paths: Vec<String>,
}

impl FootstepClipBank {
    fn new() -> Self {
        Self {
            jump_paths: Self::clip_paths("jump", 10),
            land_paths: Self::clip_paths("land", 10),
            walk_paths: Self::clip_paths("walk", 25),
            run_paths: Self::clip_paths("run", 25),
        }
    }

    fn clip_paths(sample_name: &str, sample_count: usize) -> Vec<String> {
        let prefix =
            "assets/sfx/Footsteps SFX - Undergrowth & Leaves/TomWinandySFX - FS_UndergrowthLeaves_";
        (1..=sample_count)
            .map(|index| format!("{prefix}{sample_name}_{index:02}.wav"))
            .collect()
    }

    fn deterministic_path(paths: &[String], event_seq: u64) -> &str {
        &paths[(event_seq % paths.len() as u64) as usize]
    }

    fn path_for(&self, event: &FootstepEvent) -> &str {
        // The installed bank contains only undergrowth/leaves recordings. Surface remains part of
        // the semantic event, but every surface intentionally falls back to this existing bank.
        let paths = match event.kind {
            FootstepKind::Jump => &self.jump_paths,
            FootstepKind::Land => &self.land_paths,
            FootstepKind::Stride(Gait::Walk) => &self.walk_paths,
            FootstepKind::Stride(Gait::Run) => &self.run_paths,
        };
        Self::deterministic_path(paths, event.event_seq)
    }
}

struct PreparedSpatialFootstep {
    event: FootstepEvent,
    emitter: TransientSpatialEmitter,
    routing: FootstepRouting,
    emitter_gain_db: f32,
    clip_duration_seconds: f64,
    completion_deadline_seconds: f64,
}

pub(crate) struct PreparedLocalFootsteps {
    spatial: Vec<PreparedSpatialFootstep>,
    legacy_2d: Vec<FootstepEvent>,
}

impl PreparedLocalFootsteps {
    fn empty() -> Self {
        Self {
            spatial: Vec::new(),
            legacy_2d: Vec::new(),
        }
    }
}

#[derive(Clone, Copy)]
struct RetiringEmitter {
    event_seq: u64,
    emitter: TransientSpatialEmitter,
}

struct ConcludingFootstepVoice<EmitterHandle, ControlHandle> {
    voice: ManagedFootstepVoice<EmitterHandle, ControlHandle>,
    reason: &'static str,
    telemetry_deadline_seconds: f64,
}

/// Converts semantic local-player footstep events into one PetalSonic Voice per event.
///
/// The default route splits each Voice between an immediate listener-position-relative direct path
/// and an environment send fixed at the captured world contact. Legacy 2D playback remains an
/// explicit comparison and recovery route; it does not own gameplay cadence or contact generation.
pub struct LocalPlayerFootstepAudio {
    spatial_sound_manager: SpatialSoundManager,
    clip_bank: FootstepClipBank,
    volume_gain_db: f32,
    routing_mode: LocalPlayerFootstepRoutingMode,
    active: ActiveFootstepRegistry<TransientSpatialEmitter, PlaybackControl>,
    concluding: Vec<ConcludingFootstepVoice<TransientSpatialEmitter, PlaybackControl>>,
    retiring: Vec<RetiringEmitter>,
}

impl LocalPlayerFootstepAudio {
    pub fn new(spatial_sound_manager: SpatialSoundManager) -> Self {
        Self {
            spatial_sound_manager,
            clip_bank: FootstepClipBank::new(),
            volume_gain_db: -40.0,
            routing_mode: LocalPlayerFootstepRoutingMode::default(),
            active: ActiveFootstepRegistry::new(MAX_ACTIVE_LOCAL_FOOTSTEPS),
            concluding: Vec::with_capacity(MAX_ACTIVE_LOCAL_FOOTSTEPS),
            retiring: Vec::with_capacity(MAX_ACTIVE_LOCAL_FOOTSTEPS),
        }
    }

    pub fn set_volume_gain_db(&mut self, volume_gain_db: f32) {
        self.volume_gain_db = volume_gain_db;
    }

    /// Explicit recovery/comparison switch. Split spatial routing remains the production default.
    #[allow(dead_code)]
    pub(crate) fn set_legacy_2d_comparison(&mut self, enabled: bool) {
        self.routing_mode = if enabled {
            LocalPlayerFootstepRoutingMode::Legacy2d
        } else {
            LocalPlayerFootstepRoutingMode::SplitSpatial
        };
    }

    pub(crate) fn maintain(
        &mut self,
        sim_time_seconds: f64,
        telemetry: LocalFootstepTelemetryObservations,
    ) {
        self.retry_retiring_emitters();
        for observation in telemetry.acoustic {
            self.handle_acoustic_conclusion(observation.event_seq, observation.conclusion);
        }
        for observation in telemetry.voice {
            self.handle_voice_telemetry(observation.event_seq, observation.event);
        }
        for event in self.spatial_sound_manager.drain_events() {
            self.handle_petalsonic_event(event, sim_time_seconds);
        }
        for voice in self.active.take_expired(sim_time_seconds) {
            log::warn!(
                "[AUDIO][LOCAL_FOOTSTEP] route=split_spatial event_seq={} reason=completion_deadline_expired deadline={:.6} sim_time={:.6}",
                voice.event_seq,
                voice.completion_deadline_seconds,
                sim_time_seconds,
            );
            self.begin_voice_retirement(
                voice,
                true,
                "completion_deadline_expired",
                sim_time_seconds,
            );
        }
        self.finalize_concluding_voices(sim_time_seconds);
    }

    #[cfg(test)]
    pub(super) fn test_active_voice_identity(
        &self,
        event_seq: u64,
    ) -> (petalsonic::Emitter, PlaybackControl, f64) {
        let voice = self
            .active
            .get(event_seq)
            .expect("test footstep Voice must be active");
        (
            voice.emitter.test_emitter(),
            voice.control,
            voice.completion_deadline_seconds,
        )
    }

    #[cfg(test)]
    pub(super) fn test_voice_counts(&self) -> (usize, usize) {
        (self.active.len(), self.concluding.len())
    }

    pub(crate) fn prepare(
        &mut self,
        events: &[FootstepEvent],
        sim_time_seconds: f64,
    ) -> PreparedLocalFootsteps {
        let mut prepared = PreparedLocalFootsteps::empty();
        match self.routing_mode {
            LocalPlayerFootstepRoutingMode::SplitSpatial => {
                for event in events {
                    self.prepare_spatial_event(*event, sim_time_seconds, &mut prepared);
                }
            }
            LocalPlayerFootstepRoutingMode::Legacy2d => {
                prepared.legacy_2d.extend_from_slice(events);
            }
        }
        prepared
    }

    pub(crate) fn play_after_publication(
        &mut self,
        mut prepared: PreparedLocalFootsteps,
        publication: SpatialFramePublication,
    ) {
        if !prepared.spatial.is_empty() && !publication.published_now() {
            log::error!(
                "[AUDIO][LOCAL_FOOTSTEP] reason=structural_spatial_frame_was_coalesced prepared={} spatial_revision={}",
                prepared.spatial.len(),
                publication.revision(),
            );
            for prepared_voice in prepared.spatial.drain(..) {
                self.retire_prepared_emitter(
                    prepared_voice.event.event_seq,
                    prepared_voice.emitter,
                    "structural_spatial_frame_was_coalesced",
                );
            }
        }

        for prepared_voice in prepared.spatial {
            let event = prepared_voice.event;
            let correlation_id = local_footstep_correlation_id(event.event_seq);
            let options = prepared_voice.routing.play_options(correlation_id);
            let acoustics_at_play = self.spatial_sound_manager.acoustic_pipeline_snapshot();
            match self.spatial_sound_manager.play_controlled_transient(
                prepared_voice.emitter,
                options,
                PlaybackTag(correlation_id),
            ) {
                Ok(control) => {
                    self.active.activate(ManagedFootstepVoice::new(
                        event.event_seq,
                        prepared_voice.emitter,
                        control,
                        prepared_voice.routing,
                        publication.revision(),
                        prepared_voice.emitter_gain_db,
                        prepared_voice.clip_duration_seconds,
                        acoustics_at_play,
                        prepared_voice.completion_deadline_seconds,
                    ));
                    log::debug!(
                        "[AUDIO][LOCAL_FOOTSTEP] route=split_spatial order=publish_before_play event_seq={} kind={:?} side={:?} surface={:?} contact={:?} direct_world_offset={:?} environment_gain_db={:.1} speed_mps={:.3} event_sim_time={:.6} spatial_revision={} active={}",
                        event.event_seq,
                        event.kind,
                        event.side,
                        event.surface,
                        event.contact_world,
                        prepared_voice.routing.direct_world_offset,
                        LOCAL_FOOTSTEP_ENVIRONMENT_GAIN_DB,
                        event.speed_mps,
                        event.sim_time_seconds,
                        publication.revision(),
                        self.active.len(),
                    );
                }
                Err(err) => {
                    log::error!(
                        "[AUDIO][LOCAL_FOOTSTEP] route=split_spatial event_seq={} reason=play_failed spatial_revision={} error={err:#}",
                        event.event_seq,
                        publication.revision(),
                    );
                    self.retire_prepared_emitter(
                        event.event_seq,
                        prepared_voice.emitter,
                        "play_failed",
                    );
                }
            }
        }

        for event in prepared.legacy_2d {
            if let Err(err) = self.play_legacy_2d(&event) {
                log::error!(
                    "[AUDIO][LOCAL_FOOTSTEP] route=legacy_2d event_seq={} kind={:?} side={:?} surface={:?} contact={:?} sim_time={:.6} error={err:#}",
                    event.event_seq,
                    event.kind,
                    event.side,
                    event.surface,
                    event.contact_world,
                    event.sim_time_seconds,
                );
            } else {
                log::debug!(
                    "[AUDIO][LOCAL_FOOTSTEP] route=legacy_2d order=publish_before_play event_seq={} spatial_revision={}",
                    event.event_seq,
                    publication.revision(),
                );
            }
        }
    }

    pub(crate) fn abort_prepared(
        &mut self,
        prepared: PreparedLocalFootsteps,
        reason: &'static str,
    ) {
        let dropped = prepared.spatial.len() + prepared.legacy_2d.len();
        for prepared_voice in prepared.spatial {
            self.retire_prepared_emitter(
                prepared_voice.event.event_seq,
                prepared_voice.emitter,
                reason,
            );
        }
        if dropped > 0 {
            log::warn!("[AUDIO][LOCAL_FOOTSTEP] dropped={dropped} reason={reason}");
        }
    }

    fn prepare_spatial_event(
        &mut self,
        event: FootstepEvent,
        sim_time_seconds: f64,
        prepared: &mut PreparedLocalFootsteps,
    ) {
        let path = self.clip_bank.path_for(&event).to_owned();
        let duration_seconds = match self
            .spatial_sound_manager
            .transient_clip_duration_seconds(&path)
        {
            Ok(duration_seconds) => duration_seconds,
            Err(err) => {
                log::error!(
                    "[AUDIO][LOCAL_FOOTSTEP] route=split_spatial event_seq={} reason=clip_unavailable path={path:?} error={err:#}",
                    event.event_seq,
                );
                return;
            }
        };
        if !self.reserve_voice_slot(prepared.spatial.len(), sim_time_seconds) {
            log::warn!(
                "[AUDIO][LOCAL_FOOTSTEP] route=split_spatial event_seq={} reason=voice_cap_cleanup_pending cap={}",
                event.event_seq,
                MAX_ACTIVE_LOCAL_FOOTSTEPS,
            );
            return;
        }
        let routing = FootstepRouting::for_event(&event);
        let gain_db = self.event_gain_db(&event) + self.volume_gain_db;
        match self.spatial_sound_manager.create_transient_spatial_emitter(
            event.event_seq,
            &path,
            gain_db,
            routing.environment_world,
        ) {
            Ok(emitter) => prepared.spatial.push(PreparedSpatialFootstep {
                event,
                emitter,
                routing,
                emitter_gain_db: gain_db,
                clip_duration_seconds: duration_seconds,
                completion_deadline_seconds: sim_time_seconds
                    + duration_seconds
                    + COMPLETION_DEADLINE_GRACE_SECONDS,
            }),
            Err(err) => log::error!(
                "[AUDIO][LOCAL_FOOTSTEP] route=split_spatial event_seq={} reason=emitter_create_failed path={path:?} error={err:#}",
                event.event_seq,
            ),
        }
    }

    fn reserve_voice_slot(&mut self, pending: usize, sim_time_seconds: f64) -> bool {
        while self.active.len() + self.retiring.len() + pending >= MAX_ACTIVE_LOCAL_FOOTSTEPS {
            let Some(oldest) = self.active.take_oldest() else {
                return false;
            };
            log::warn!(
                "[AUDIO][LOCAL_FOOTSTEP] route=split_spatial event_seq={} reason=voice_cap_steal cap={}",
                oldest.event_seq,
                MAX_ACTIVE_LOCAL_FOOTSTEPS,
            );
            self.begin_voice_retirement(oldest, true, "voice_cap_steal", sim_time_seconds);
        }
        true
    }

    fn event_gain_db(&self, event: &FootstepEvent) -> f32 {
        let (minimum_db, maximum_db) = match event.kind {
            FootstepKind::Jump | FootstepKind::Land => (-6.0, 6.0),
            FootstepKind::Stride(_) => (-4.0, 0.0),
        };
        let speed_ratio = (event.speed_mps / 3.0).clamp(0.0, 1.0);
        minimum_db + (maximum_db - minimum_db) * speed_ratio
    }

    fn play_legacy_2d(&self, event: &FootstepEvent) -> Result<()> {
        self.spatial_sound_manager.add_non_spatial_source(
            self.clip_bank.path_for(event),
            self.event_gain_db(event) + self.volume_gain_db,
        )
    }

    fn voice_by_event_seq_mut(
        &mut self,
        event_seq: u64,
    ) -> Option<&mut ManagedFootstepVoice<TransientSpatialEmitter, PlaybackControl>> {
        if let Some(index) = self
            .active
            .voices
            .iter()
            .position(|voice| voice.event_seq == event_seq)
        {
            return self.active.voices.get_mut(index);
        }
        self.concluding
            .iter_mut()
            .find(|concluding| concluding.voice.event_seq == event_seq)
            .map(|concluding| &mut concluding.voice)
    }

    fn voice_by_event_seq(
        &self,
        event_seq: u64,
    ) -> Option<&ManagedFootstepVoice<TransientSpatialEmitter, PlaybackControl>> {
        self.active.get(event_seq).or_else(|| {
            self.concluding
                .iter()
                .find(|concluding| concluding.voice.event_seq == event_seq)
                .map(|concluding| &concluding.voice)
        })
    }

    fn handle_acoustic_conclusion(
        &mut self,
        event_seq: u64,
        conclusion: AcousticVoiceConclusionTelemetry,
    ) {
        let Some(voice) = self.voice_by_event_seq_mut(event_seq) else {
            log::debug!(
                "[AUDIO][LOCAL_FOOTSTEP] event_seq={event_seq} voice_id={} reason=unmatched_acoustic_conclusion",
                conclusion.voice_id,
            );
            return;
        };
        if !voice.emitter.matches(conclusion.emitter) {
            log::error!(
                "[AUDIO][LOCAL_FOOTSTEP] event_seq={event_seq} voice_id={} reason=acoustic_conclusion_emitter_mismatch",
                conclusion.voice_id,
            );
            return;
        }
        voice
            .wet_observation
            .record_acoustic_conclusion(conclusion.into());
        log::debug!(
            "[AUDIO][LOCAL_FOOTSTEP] event_seq={} state=acoustic_conclusion voice_id={} candidate_rank={:?} candidate_limit={} direct={:?} environment={:?} solve_status={:?}",
            voice.event_seq,
            conclusion.voice_id,
            conclusion.candidate_rank,
            conclusion.candidate_limit,
            conclusion.direct,
            conclusion.environment,
            conclusion.solve_status,
        );
    }

    fn handle_petalsonic_event(&mut self, event: PetalSonicEvent, sim_time_seconds: f64) {
        match event {
            PetalSonicEvent::PlaybackCompleted {
                emitter,
                control,
                tag,
            } => {
                let Some(event_seq) = local_footstep_event_seq(tag.0) else {
                    log::debug!(
                        "[AUDIO][LOCAL_FOOTSTEP] correlation_id={} reason=unowned_completion",
                        tag.0,
                    );
                    return;
                };
                let Some(expected) = self.active.get(event_seq) else {
                    log::debug!(
                        "[AUDIO][LOCAL_FOOTSTEP] event_seq={event_seq} reason=unmatched_completion"
                    );
                    return;
                };
                if expected.control != control || !expected.emitter.matches(emitter) {
                    log::error!(
                        "[AUDIO][LOCAL_FOOTSTEP] event_seq={event_seq} reason=completion_handle_mismatch"
                    );
                    return;
                }
                let voice = self
                    .active
                    .complete(event_seq)
                    .expect("matched footstep completion must remain active");
                log::debug!(
                    "[AUDIO][LOCAL_FOOTSTEP] route=split_spatial event_seq={event_seq} state=completed active={}",
                    self.active.len(),
                );
                self.begin_voice_retirement(voice, false, "completed", sim_time_seconds);
            }
            other => log::debug!("PetalSonic event: {other:?}"),
        }
    }

    fn handle_voice_telemetry(&mut self, event_seq: u64, event: VoiceTelemetryEvent) {
        match event {
            VoiceTelemetryEvent::FirstRendered(telemetry) => {
                debug_assert_eq!(
                    local_footstep_event_seq(telemetry.play_command_id.0),
                    Some(event_seq)
                );
                match correlated_telemetry_attribution(self.active.get(event_seq), |emitter| {
                    emitter.matches(telemetry.emitter)
                }) {
                    FootstepTelemetryAttribution::Accepted => {}
                    FootstepTelemetryAttribution::UnknownVoice => {
                        log::debug!(
                            "[AUDIO][LOCAL_FOOTSTEP] event_seq={event_seq} reason=unmatched_first_render"
                        );
                        return;
                    }
                    FootstepTelemetryAttribution::WrongEmitter => {
                        log::error!(
                            "[AUDIO][LOCAL_FOOTSTEP] event_seq={event_seq} reason=first_render_emitter_mismatch"
                        );
                        return;
                    }
                }
                let voice = self
                    .active
                    .get_mut(event_seq)
                    .expect("accepted first-render telemetry must remain active");
                let contract = FirstRenderContractCheck::for_voice(
                    voice,
                    telemetry.spatial_revision,
                    telemetry.direct_local_pose,
                    telemetry.acoustic_origin,
                );
                voice.wet_observation.record_first_render(
                    telemetry.render_block_index,
                    telemetry.environment_response,
                );
                if contract.satisfied() {
                    log::debug!(
                        "[AUDIO][LOCAL_FOOTSTEP] route=split_spatial event_seq={event_seq} state=first_render render_block={} published_revision={} audible_revision={} direct_world_offset={:?} direct_local_pose={:?} environment_world={:?}",
                        telemetry.render_block_index,
                        voice.published_revision,
                        telemetry.spatial_revision,
                        voice.routing.direct_world_offset,
                        telemetry.direct_local_pose,
                        voice.routing.environment_world,
                    );
                } else {
                    log::error!(
                        "[AUDIO][LOCAL_FOOTSTEP] route=split_spatial event_seq={event_seq} reason=first_render_contract_violation published_revision={} audible_revision={} revision_ok={} direct_ok={} environment_ok={} direct_local_pose={:?} acoustic_origin={:?}",
                        voice.published_revision,
                        telemetry.spatial_revision,
                        contract.revision_ok,
                        contract.direct_ok,
                        contract.environment_ok,
                        telemetry.direct_local_pose,
                        telemetry.acoustic_origin,
                    );
                }
            }
            VoiceTelemetryEvent::EnvironmentResponse {
                play_command_id,
                response,
            } => {
                debug_assert_eq!(local_footstep_event_seq(play_command_id.0), Some(event_seq));
                let Some(voice) = self.voice_by_event_seq_mut(event_seq) else {
                    log::debug!(
                        "[AUDIO][LOCAL_FOOTSTEP] event_seq={event_seq} reason=unmatched_environment_response"
                    );
                    return;
                };
                voice.wet_observation.record_environment_response(response);
                log::debug!(
                    "[AUDIO][LOCAL_FOOTSTEP] route=split_spatial event_seq={} state=environment_response spatial_revision={} geometry_version={} age_ms={:.3}",
                    event_seq,
                    response.spatial_revision,
                    response.geometry_version,
                    response.age.as_secs_f64() * 1000.0,
                );
            }
            VoiceTelemetryEvent::EnergySummary(telemetry) => {
                debug_assert_eq!(
                    local_footstep_event_seq(telemetry.play_command_id.0),
                    Some(event_seq)
                );
                match correlated_telemetry_attribution(
                    self.voice_by_event_seq(event_seq),
                    |emitter| emitter.matches(telemetry.emitter),
                ) {
                    FootstepTelemetryAttribution::Accepted => {}
                    FootstepTelemetryAttribution::UnknownVoice => {
                        log::debug!(
                            "[AUDIO][LOCAL_FOOTSTEP] event_seq={event_seq} reason=unmatched_energy_summary"
                        );
                        return;
                    }
                    FootstepTelemetryAttribution::WrongEmitter => {
                        log::error!(
                            "[AUDIO][LOCAL_FOOTSTEP] event_seq={event_seq} reason=energy_summary_emitter_mismatch"
                        );
                        return;
                    }
                }
                let voice = self
                    .voice_by_event_seq_mut(event_seq)
                    .expect("accepted energy telemetry must retain its footstep voice");
                voice
                    .wet_observation
                    .record_energy_summary(telemetry.into());
                log::debug!(
                    "[AUDIO][LOCAL_FOOTSTEP] event_seq={event_seq} state=energy_summary source_energy={:.6} direct_energy={:.6} environment_send_energy={:.6} early_reflection_energy={:.6}",
                    telemetry.source_energy,
                    telemetry.direct_energy,
                    telemetry.environment_send_energy,
                    telemetry.early_reflection_energy,
                );
            }
            other => log::debug!("PetalSonic voice telemetry: {other:?}"),
        }
    }

    fn begin_voice_retirement(
        &mut self,
        voice: ManagedFootstepVoice<TransientSpatialEmitter, PlaybackControl>,
        stop_first: bool,
        reason: &'static str,
        sim_time_seconds: f64,
    ) {
        #[cfg(test)]
        self.spatial_sound_manager.observe_test_footstep_retirement(
            voice.event_seq,
            reason,
            voice.wet_observation.response.is_some(),
            voice.wet_observation.acoustic_conclusion.is_some(),
        );
        if stop_first {
            if let Err(err) = self
                .spatial_sound_manager
                .stop_controlled_transient(voice.control)
            {
                log::warn!(
                    "[AUDIO][LOCAL_FOOTSTEP] event_seq={} reason={reason} stop_failed={err:#}",
                    voice.event_seq,
                );
            }
        }
        if let Err(err) = self
            .spatial_sound_manager
            .destroy_transient_spatial_emitter(voice.emitter)
        {
            log::warn!(
                "[AUDIO][LOCAL_FOOTSTEP] event_seq={} reason={reason} destroy_deferred={err:#}",
                voice.event_seq,
            );
            self.retiring.push(RetiringEmitter {
                event_seq: voice.event_seq,
                emitter: voice.emitter,
            });
        }
        self.concluding.push(ConcludingFootstepVoice {
            voice,
            reason,
            telemetry_deadline_seconds: sim_time_seconds + TELEMETRY_CONCLUSION_GRACE_SECONDS,
        });
    }

    fn finalize_concluding_voices(&mut self, sim_time_seconds: f64) {
        let concluding = std::mem::take(&mut self.concluding);
        for concluding_voice in concluding {
            let deadline_expired = concluding_voice.telemetry_deadline_seconds <= sim_time_seconds;
            let telemetry_complete = concluding_voice
                .voice
                .wet_observation
                .ready_for_final_summary(concluding_voice.voice.acoustics_at_play.enabled);
            if telemetry_complete || deadline_expired {
                self.log_wet_path_summary(
                    &concluding_voice.voice,
                    concluding_voice.reason,
                    deadline_expired,
                );
                self.spatial_sound_manager
                    .release_transient_spatial_telemetry_ownership(concluding_voice.voice.emitter);
            } else {
                self.concluding.push(concluding_voice);
            }
        }
    }

    fn log_wet_path_summary(
        &self,
        voice: &ManagedFootstepVoice<TransientSpatialEmitter, PlaybackControl>,
        reason: &'static str,
        telemetry_deadline_expired: bool,
    ) {
        let acoustics_at_retirement = self.spatial_sound_manager.acoustic_pipeline_snapshot();
        let activity = acoustics_at_retirement.activity_since(voice.acoustics_at_play);
        let telemetry_was_dropped =
            activity.dropped_voice_telemetry > 0 || activity.dropped_acoustic_telemetry > 0;
        let outcome = voice
            .wet_observation
            .outcome(voice.acoustics_at_play.enabled, telemetry_was_dropped);
        let environment_send_gain_linear = 10.0_f32.powf(LOCAL_FOOTSTEP_ENVIRONMENT_GAIN_DB / 20.0);
        let environment_pre_acoustics_gain_db =
            voice.emitter_gain_db + LOCAL_FOOTSTEP_ENVIRONMENT_GAIN_DB;
        let response_revision = voice
            .wet_observation
            .response
            .map(|response| response.spatial_revision);
        let response_geometry = voice
            .wet_observation
            .response
            .map(|response| response.geometry_version);
        let response_age_ms = voice
            .wet_observation
            .response
            .map(|response| response.age.as_secs_f64() * 1000.0);
        let conclusion = voice.wet_observation.acoustic_conclusion;
        let energy = voice.wet_observation.energy_summary;
        let qos_status = if conclusion.is_some() {
            TelemetryObservationStatus::Observed
        } else if !voice.acoustics_at_play.enabled {
            TelemetryObservationStatus::NotExpectedAcousticsDisabled
        } else {
            debug_assert!(telemetry_deadline_expired);
            TelemetryObservationStatus::NotReceivedBeforeDeadline
        };
        let energy_status = if energy.is_some() {
            TelemetryObservationStatus::Observed
        } else {
            debug_assert!(telemetry_deadline_expired);
            TelemetryObservationStatus::NotReceivedBeforeDeadline
        };
        let message = format!(
            "[AUDIO][LOCAL_FOOTSTEP_WET] event_seq={} reason={} outcome={:?} first_render_block={:?} clip_duration_ms={:.3} dry_path=listener_position_relative_immediate direct_pre_dsp_gain_db={:.1} environment_send=world_contact send_gain_db={:.1} send_gain_linear={:.4} environment_pre_acoustics_gain_db={:.1} environment_response_spatial_revision={:?} environment_response_geometry_version={:?} environment_response_age_ms={:?} qos_status={:?} qos_voice_id={:?} qos_spatial_revision={:?} qos_geometry_version={:?} candidate_rank={:?} candidate_limit={:?} direct_outcome={:?} environment_outcome={:?} environment_transmission_gain={:?} solve_status={:?} early_reflections_status={:?} early_tap_count={:?} energy_status={:?} source_energy={:?} direct_energy={:?} environment_send_energy={:?} early_reflection_energy={:?} global_late_reverb_status={:?} global_late_pre_delay_seconds={:?} global_late_rt60_seconds={:?} global_late_wet_gain={:?} global_late_cumulative_input_energy={:?} global_late_cumulative_output_energy={:?} acoustics_enabled_at_play={} acoustics_enabled_at_retirement={} solves_during_voice={} superseded_during_voice={} responses_published_during_voice={} dropped_voice_telemetry_during_voice={} dropped_acoustic_telemetry_during_voice={} latest_response_spatial_revision={} latest_response_geometry_version={} latest_response_age_ms={}",
            voice.event_seq,
            reason,
            outcome,
            voice.wet_observation.first_render_block,
            voice.clip_duration_seconds * 1000.0,
            voice.emitter_gain_db,
            LOCAL_FOOTSTEP_ENVIRONMENT_GAIN_DB,
            environment_send_gain_linear,
            environment_pre_acoustics_gain_db,
            response_revision,
            response_geometry,
            response_age_ms,
            qos_status,
            conclusion.map(|observation| observation.voice_id),
            conclusion.map(|observation| observation.spatial_revision),
            conclusion.map(|observation| observation.geometry_version),
            conclusion.and_then(|observation| observation.candidate_rank),
            conclusion.map(|observation| observation.candidate_limit),
            conclusion.map(|observation| observation.direct),
            conclusion.map(|observation| observation.environment),
            conclusion.map(|observation| observation.environment_transmission_gain),
            conclusion.and_then(|observation| observation.solve_status),
            qos_status,
            conclusion.map(|observation| observation.early_tap_count),
            energy_status,
            energy.map(|observation| observation.source_energy),
            energy.map(|observation| observation.direct_energy),
            energy.map(|observation| observation.environment_send_energy),
            energy.map(|observation| observation.early_reflection_energy),
            energy_status,
            energy.map(|observation| observation.global_late_reverb.pre_delay_seconds),
            energy.map(|observation| observation.global_late_reverb.rt60_seconds),
            energy.map(|observation| observation.global_late_reverb.wet_gain),
            energy.map(|observation| {
                observation.global_late_reverb.cumulative_input_energy
            }),
            energy.map(|observation| {
                observation.global_late_reverb.cumulative_output_energy
            }),
            voice.acoustics_at_play.enabled,
            acoustics_at_retirement.enabled,
            activity.solves,
            activity.superseded,
            activity.published,
            activity.dropped_voice_telemetry,
            activity.dropped_acoustic_telemetry,
            acoustics_at_retirement.response_spatial_revision,
            acoustics_at_retirement.response_geometry_version,
            acoustics_at_retirement.response_age_ms,
        );
        match outcome {
            FootstepWetOutcome::TelemetryIncomplete
            | FootstepWetOutcome::TelemetryNotReceivedBeforeDeadline
            | FootstepWetOutcome::EnvironmentExcludedByBudget => log::warn!("{message}"),
            FootstepWetOutcome::AcousticsDisabled
            | FootstepWetOutcome::ResponseReadyAtFirstRender
            | FootstepWetOutcome::ResponseArrivedDuringPlayback => log::info!("{message}"),
        }
    }

    fn retire_prepared_emitter(
        &mut self,
        event_seq: u64,
        emitter: TransientSpatialEmitter,
        reason: &'static str,
    ) {
        self.spatial_sound_manager
            .release_transient_spatial_telemetry_ownership(emitter);
        if let Err(err) = self
            .spatial_sound_manager
            .destroy_transient_spatial_emitter(emitter)
        {
            log::warn!(
                "[AUDIO][LOCAL_FOOTSTEP] event_seq={event_seq} reason={reason} destroy_deferred={err:#}"
            );
            self.retiring.push(RetiringEmitter { event_seq, emitter });
        }
    }

    fn retry_retiring_emitters(&mut self) {
        let retiring = std::mem::take(&mut self.retiring);
        for retired in retiring {
            if let Err(err) = self
                .spatial_sound_manager
                .destroy_transient_spatial_emitter(retired.emitter)
            {
                log::warn!(
                    "[AUDIO][LOCAL_FOOTSTEP] event_seq={} reason=destroy_retry_failed error={err:#}",
                    retired.event_seq,
                );
                self.retiring.push(retired);
            } else {
                log::debug!(
                    "[AUDIO][LOCAL_FOOTSTEP] event_seq={} state=emitter_retired",
                    retired.event_seq,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        correlated_telemetry_attribution, local_footstep_correlation_id, local_footstep_event_seq,
        ActiveFootstepRegistry, ConcludingFootstepVoice, EnvironmentResponseTiming,
        FirstRenderContractCheck, FootstepAcousticConclusion, FootstepClipBank,
        FootstepEnergySummary, FootstepRouting, FootstepTelemetryAttribution,
        FootstepWetObservation, FootstepWetOutcome, LocalPlayerFootstepRoutingMode,
        ManagedFootstepVoice, LOCAL_FOOTSTEP_CORRELATION_NAMESPACE, MAX_ACTIVE_LOCAL_FOOTSTEPS,
    };
    use crate::audio::spatial_sound_manager::AcousticPipelineSnapshot;
    use crate::gameplay::camera::{
        FootstepEvent, FootstepKind, FootstepSide, FootstepSurface, Gait,
    };
    use glam::Vec3;
    use petalsonic::{
        AcousticRouteOutcome, AcousticSolveStatus, DirectPlacement, EnvironmentResponse,
        LateReverbTelemetry, Pose, Vec3 as PetalVec3,
    };
    use std::time::Duration;

    fn event(event_seq: u64, contact_world: Vec3, speed_mps: f32) -> FootstepEvent {
        FootstepEvent {
            event_seq,
            kind: FootstepKind::Stride(Gait::Run),
            side: FootstepSide::Left,
            contact_world,
            surface: FootstepSurface::Unknown,
            speed_mps,
            sim_time_seconds: event_seq as f64,
        }
    }

    fn voice(
        event_seq: u64,
        emitter: u64,
        control: u64,
        deadline: f64,
    ) -> ManagedFootstepVoice<u64, u64> {
        let footstep = event(event_seq, Vec3::new(event_seq as f32, 0.0, 0.0), 1.0);
        ManagedFootstepVoice::new(
            event_seq,
            emitter,
            control,
            FootstepRouting::for_event(&footstep),
            event_seq + 10,
            -8.0,
            0.5,
            AcousticPipelineSnapshot {
                enabled: true,
                solve_count: 10,
                superseded_solve_count: 8,
                published_response_count: 2,
                response_spatial_revision: 9,
                response_geometry_version: 7,
                response_age_ms: 12,
                dropped_voice_telemetry_count: 0,
                dropped_acoustic_telemetry_count: 0,
            },
            deadline,
        )
    }

    fn environment_response(spatial_revision: u64) -> EnvironmentResponse {
        EnvironmentResponse {
            spatial_revision,
            geometry_version: 7,
            age: Duration::from_millis(12),
        }
    }

    fn acoustic_conclusion(environment: AcousticRouteOutcome) -> FootstepAcousticConclusion {
        FootstepAcousticConclusion {
            voice_id: 71,
            spatial_revision: 81,
            geometry_version: 7,
            candidate_rank: Some(2),
            candidate_limit: 8,
            direct: AcousticRouteOutcome::Disabled,
            environment,
            environment_transmission_gain: [0.8, 0.7, 0.6],
            early_tap_count: usize::from(environment == AcousticRouteOutcome::Applied),
            solve_status: (environment != AcousticRouteOutcome::Disabled)
                .then_some(AcousticSolveStatus::Solved),
        }
    }

    fn energy_summary() -> FootstepEnergySummary {
        FootstepEnergySummary {
            source_energy: 4.0,
            direct_energy: 2.0,
            environment_send_energy: 1.0,
            early_reflection_energy: 0.25,
            global_late_reverb: LateReverbTelemetry {
                pre_delay_seconds: 0.02,
                rt60_seconds: [0.7, 0.9, 1.1],
                wet_gain: 0.4,
                cumulative_input_energy: 3.0,
                cumulative_output_energy: 1.5,
            },
        }
    }

    #[test]
    fn wet_path_reports_telemetry_deadline_when_conclusion_is_missing() {
        let mut observation = FootstepWetObservation::default();
        observation.record_first_render(41, None);

        assert_eq!(
            observation.outcome(true, false),
            FootstepWetOutcome::TelemetryNotReceivedBeforeDeadline
        );
        assert_eq!(
            observation.response_timing,
            EnvironmentResponseTiming::Unobserved
        );
        assert_eq!(observation.first_render_block, Some(41));
    }

    #[test]
    fn wet_path_distinguishes_response_ready_at_onset_from_later_arrival() {
        let response = environment_response(81);
        let mut ready_at_onset = FootstepWetObservation::default();
        ready_at_onset.record_first_render(42, Some(response));
        ready_at_onset
            .record_acoustic_conclusion(acoustic_conclusion(AcousticRouteOutcome::Applied));
        ready_at_onset.record_energy_summary(energy_summary());
        assert_eq!(
            ready_at_onset.outcome(true, false),
            FootstepWetOutcome::ResponseReadyAtFirstRender
        );
        assert_eq!(ready_at_onset.response, Some(response));

        let mut arrived_later = FootstepWetObservation::default();
        arrived_later.record_first_render(43, None);
        arrived_later.record_environment_response(response);
        arrived_later
            .record_acoustic_conclusion(acoustic_conclusion(AcousticRouteOutcome::Applied));
        arrived_later.record_energy_summary(energy_summary());
        assert_eq!(
            arrived_later.outcome(true, false),
            FootstepWetOutcome::ResponseArrivedDuringPlayback
        );
        assert_eq!(arrived_later.response, Some(response));
    }

    #[test]
    fn wet_path_does_not_report_starvation_when_acoustics_remain_disabled() {
        let mut observation = FootstepWetObservation::default();
        observation.record_first_render(44, None);
        observation.record_acoustic_conclusion(acoustic_conclusion(AcousticRouteOutcome::Disabled));
        observation.record_energy_summary(energy_summary());

        assert_eq!(
            observation.outcome(false, false),
            FootstepWetOutcome::AcousticsDisabled
        );
    }

    #[test]
    fn wet_path_reports_environment_budget_exclusion_explicitly() {
        let mut observation = FootstepWetObservation::default();
        observation.record_first_render(44, None);
        observation.record_acoustic_conclusion(acoustic_conclusion(
            AcousticRouteOutcome::ExcludedByBudget,
        ));
        observation.record_energy_summary(energy_summary());

        assert_eq!(
            observation.outcome(true, false),
            FootstepWetOutcome::EnvironmentExcludedByBudget
        );
    }

    #[test]
    fn wet_path_marks_missing_response_telemetry_as_incomplete_when_events_were_dropped() {
        let mut observation = FootstepWetObservation::default();
        observation.record_first_render(45, Some(environment_response(82)));
        observation.record_acoustic_conclusion(acoustic_conclusion(AcousticRouteOutcome::Applied));
        observation.record_energy_summary(energy_summary());

        assert_eq!(
            observation.outcome(true, true),
            FootstepWetOutcome::TelemetryIncomplete
        );
    }

    #[test]
    fn non_footstep_telemetry_is_ignored_by_the_namespaced_correlation() {
        for event_seq in [0, 1, 42, LOCAL_FOOTSTEP_CORRELATION_NAMESPACE - 1] {
            let correlation_id = local_footstep_correlation_id(event_seq);
            assert!(correlation_id >= LOCAL_FOOTSTEP_CORRELATION_NAMESPACE);
            assert_eq!(local_footstep_event_seq(correlation_id), Some(event_seq));
        }
        assert_eq!(
            local_footstep_event_seq(LOCAL_FOOTSTEP_CORRELATION_NAMESPACE - 1),
            None
        );
    }

    #[test]
    fn correlated_telemetry_rejects_the_wrong_emitter() {
        let voice = voice(5, 105, 205, 2.0);
        assert_eq!(
            correlated_telemetry_attribution(Some(&voice), |emitter| *emitter == 999),
            FootstepTelemetryAttribution::WrongEmitter
        );
        assert_eq!(
            correlated_telemetry_attribution(Some(&voice), |emitter| *emitter == 105),
            FootstepTelemetryAttribution::Accepted
        );
        assert_eq!(
            correlated_telemetry_attribution::<u64, u64>(None, |_| true),
            FootstepTelemetryAttribution::UnknownVoice
        );
    }

    #[test]
    fn completed_voice_waits_for_energy_summary_after_its_tail() {
        let mut concluding = ConcludingFootstepVoice {
            voice: voice(5, 105, 205, 2.0),
            reason: "completed",
            telemetry_deadline_seconds: 3.0,
        };
        concluding
            .voice
            .wet_observation
            .record_acoustic_conclusion(acoustic_conclusion(AcousticRouteOutcome::Applied));

        assert!(!concluding
            .voice
            .wet_observation
            .ready_for_final_summary(true));
        concluding
            .voice
            .wet_observation
            .record_energy_summary(energy_summary());
        assert!(concluding
            .voice
            .wet_observation
            .ready_for_final_summary(true));
    }

    #[test]
    fn direct_anchor_follows_listener_position_while_remaining_world_gravity_aligned() {
        let first = FootstepRouting::for_event(&event(1, Vec3::new(10.0, 2.0, -4.0), 0.55));
        let after_stop_and_reverse =
            FootstepRouting::for_event(&event(2, Vec3::new(-20.0, 8.0, 30.0), -0.55));

        assert_eq!(first.direct_world_offset, Vec3::new(0.0, -0.08, 0.0));
        assert_eq!(
            after_stop_and_reverse.direct_world_offset,
            first.direct_world_offset
        );
        assert_eq!(
            first.direct_path().placement(),
            DirectPlacement::ListenerPositionRelative(first.direct_world_offset_pose())
        );
        assert_eq!(first.environment_world, Vec3::new(10.0, 2.0, -4.0));
        assert_eq!(
            after_stop_and_reverse.environment_world,
            Vec3::new(-20.0, 8.0, 30.0)
        );
    }

    #[test]
    fn split_spatial_is_the_default_and_legacy_2d_is_only_an_explicit_route() {
        assert_eq!(
            LocalPlayerFootstepRoutingMode::default(),
            LocalPlayerFootstepRoutingMode::SplitSpatial
        );
        assert_ne!(
            LocalPlayerFootstepRoutingMode::Legacy2d,
            LocalPlayerFootstepRoutingMode::SplitSpatial
        );
    }

    #[test]
    fn first_audible_block_requires_published_revision_and_both_captured_origins() {
        let voice = voice(5, 105, 205, 2.0);
        let exact = FirstRenderContractCheck::for_voice(
            &voice,
            voice.published_revision,
            Some(voice.routing.direct_world_offset_pose()),
            Some(voice.routing.environment_pose()),
        );
        assert!(exact.satisfied());

        let looking_down = FirstRenderContractCheck::for_voice(
            &voice,
            voice.published_revision,
            Some(Pose::from_position(PetalVec3::new(0.0, -0.0014, 0.07998))),
            Some(voice.routing.environment_pose()),
        );
        assert!(looking_down.satisfied());

        let stale = FirstRenderContractCheck::for_voice(
            &voice,
            voice.published_revision - 1,
            Some(voice.routing.direct_world_offset_pose()),
            Some(voice.routing.environment_pose()),
        );
        assert!(!stale.revision_ok);

        let wrong_direct = FirstRenderContractCheck::for_voice(
            &voice,
            voice.published_revision,
            Some(FootstepRouting::for_event(&event(99, Vec3::ZERO, 0.0)).environment_pose()),
            Some(voice.routing.environment_pose()),
        );
        assert!(!wrong_direct.direct_ok);

        let relocated_environment = FirstRenderContractCheck::for_voice(
            &voice,
            voice.published_revision,
            Some(voice.routing.direct_world_offset_pose()),
            Some(FootstepRouting::for_event(&event(99, Vec3::ZERO, 0.0)).environment_pose()),
        );
        assert!(!relocated_environment.environment_ok);
    }

    #[test]
    fn overlapping_voices_complete_independently_and_return_to_baseline() {
        let mut registry = ActiveFootstepRegistry::new(MAX_ACTIVE_LOCAL_FOOTSTEPS);
        registry.activate(voice(1, 101, 201, 1.8));
        registry.activate(voice(2, 102, 202, 2.8));

        let first = registry.complete(1).expect("first completion must match");
        assert_eq!(first.emitter, 101);
        assert_eq!(registry.len(), 1);
        assert_eq!(registry.get(2).unwrap().emitter, 102);

        let second = registry.complete(2).expect("second completion must match");
        assert_eq!(second.emitter, 102);
        assert!(registry.is_empty());
    }

    #[test]
    fn cap_steals_the_oldest_voice_without_retargeting_newer_overlap() {
        let mut registry = ActiveFootstepRegistry::new(2);
        registry.activate(voice(10, 110, 210, 3.0));
        registry.activate(voice(11, 111, 211, 4.0));

        let oldest = registry.take_oldest().expect("cap must select one voice");
        assert_eq!(oldest.event_seq, 10);
        assert_eq!(registry.get(11).unwrap().emitter, 111);
    }

    #[test]
    fn missed_completion_expires_deterministically_after_its_deadline() {
        let mut registry = ActiveFootstepRegistry::new(MAX_ACTIVE_LOCAL_FOOTSTEPS);
        registry.activate(voice(1, 101, 201, 1.8));
        registry.activate(voice(2, 102, 202, 2.8));

        let expired = registry.take_expired(2.0);
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].event_seq, 1);
        assert_eq!(registry.get(2).unwrap().emitter, 102);
    }

    #[test]
    fn every_surface_uses_the_existing_bank_fallback_without_changing_semantics() {
        let bank = FootstepClipBank::new();
        let mut footstep = event(7, Vec3::ZERO, 1.0);
        let fallback = bank.path_for(&footstep).to_owned();

        for surface in [
            FootstepSurface::Unknown,
            FootstepSurface::Dirt,
            FootstepSurface::Sand,
            FootstepSurface::Stone,
            FootstepSurface::Wood,
            FootstepSurface::Stucco,
            FootstepSurface::Glass,
        ] {
            footstep.surface = surface;
            assert_eq!(bank.path_for(&footstep), fallback);
            assert_eq!(footstep.surface, surface);
        }
    }
}
