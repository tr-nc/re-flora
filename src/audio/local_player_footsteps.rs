use super::spatial_sound_manager::{SpatialFramePublication, TransientSpatialEmitter};
use super::SpatialSoundManager;
use crate::gameplay::camera::{FootstepEvent, FootstepKind, Gait};
use anyhow::Result;
use petalsonic::{
    DirectGeometry, DirectPath, DirectPropagation, EnvironmentSend, PetalSonicEvent, PlayCommandId,
    PlayOptions, PlaybackControl, PlaybackTag, Pose, Vec3 as PetalVec3, VoiceTelemetryEvent,
};

const LOCAL_FOOTSTEP_DIRECT_Y: f32 = -0.08;
const LOCAL_FOOTSTEP_ENVIRONMENT_GAIN_DB: f32 = -12.0;
const MAX_ACTIVE_LOCAL_FOOTSTEPS: usize = 6;
const COMPLETION_DEADLINE_GRACE_SECONDS: f64 = 1.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LocalPlayerFootstepRoutingMode {
    #[default]
    SplitSpatial,
    Legacy2d,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct FootstepRouting {
    direct_listener_local: glam::Vec3,
    environment_world: glam::Vec3,
}

impl FootstepRouting {
    fn for_event(event: &FootstepEvent) -> Self {
        Self {
            direct_listener_local: glam::Vec3::new(0.0, LOCAL_FOOTSTEP_DIRECT_Y, 0.0),
            environment_world: event.contact_world,
        }
    }

    fn direct_pose(self) -> Pose {
        Pose::from_position(Self::petal_position(self.direct_listener_local))
    }

    fn environment_pose(self) -> Pose {
        Pose::from_position(Self::petal_position(self.environment_world))
    }

    fn petal_position(position: glam::Vec3) -> PetalVec3 {
        PetalVec3::new(position.x, position.y, position.z)
    }

    fn play_options(self, event_seq: u64) -> PlayOptions {
        PlayOptions::once()
            .with_direct_path(
                DirectPath::listener_relative(self.direct_pose())
                    .with_geometry(DirectGeometry::BypassTransmission)
                    .with_propagation(DirectPropagation::Immediate),
            )
            .with_environment_send(
                EnvironmentSend::from_world_pose(self.environment_pose())
                    .with_gain_db(LOCAL_FOOTSTEP_ENVIRONMENT_GAIN_DB),
            )
            .with_play_command_id(PlayCommandId(event_seq))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ManagedFootstepVoice<EmitterHandle, ControlHandle> {
    event_seq: u64,
    emitter: EmitterHandle,
    control: ControlHandle,
    routing: FootstepRouting,
    published_revision: u64,
    completion_deadline_seconds: f64,
}

impl<EmitterHandle, ControlHandle> ManagedFootstepVoice<EmitterHandle, ControlHandle> {
    fn new(
        event_seq: u64,
        emitter: EmitterHandle,
        control: ControlHandle,
        routing: FootstepRouting,
        published_revision: u64,
        completion_deadline_seconds: f64,
    ) -> Self {
        Self {
            event_seq,
            emitter,
            control,
            routing,
            published_revision,
            completion_deadline_seconds,
        }
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
            direct_ok: direct_local_pose == Some(voice.routing.direct_pose()),
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

/// Converts semantic local-player footstep events into one PetalSonic Voice per event.
///
/// The default route splits each Voice between an immediate listener-relative direct path and an
/// environment send fixed at the captured world contact. Legacy 2D playback remains an explicit
/// comparison and recovery route; it does not own gameplay cadence or contact generation.
pub struct LocalPlayerFootstepAudio {
    spatial_sound_manager: SpatialSoundManager,
    clip_bank: FootstepClipBank,
    volume_gain_db: f32,
    routing_mode: LocalPlayerFootstepRoutingMode,
    active: ActiveFootstepRegistry<TransientSpatialEmitter, PlaybackControl>,
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

    pub(crate) fn maintain(&mut self, sim_time_seconds: f64) {
        self.retry_retiring_emitters();
        for event in self.spatial_sound_manager.drain_voice_telemetry() {
            self.handle_voice_telemetry(event);
        }
        for event in self.spatial_sound_manager.drain_events() {
            self.handle_petalsonic_event(event);
        }
        for voice in self.active.take_expired(sim_time_seconds) {
            log::warn!(
                "[AUDIO][LOCAL_FOOTSTEP] route=split_spatial event_seq={} reason=completion_deadline_expired deadline={:.6} sim_time={:.6}",
                voice.event_seq,
                voice.completion_deadline_seconds,
                sim_time_seconds,
            );
            self.retire_voice(voice, true, "completion_deadline_expired");
        }
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
        prepared: PreparedLocalFootsteps,
        publication: SpatialFramePublication,
    ) {
        for prepared_voice in prepared.spatial {
            let event = prepared_voice.event;
            let options = prepared_voice.routing.play_options(event.event_seq);
            match self.spatial_sound_manager.play_controlled_transient(
                prepared_voice.emitter,
                options,
                PlaybackTag(event.event_seq),
            ) {
                Ok(control) => {
                    self.active.activate(ManagedFootstepVoice::new(
                        event.event_seq,
                        prepared_voice.emitter,
                        control,
                        prepared_voice.routing,
                        publication.revision(),
                        prepared_voice.completion_deadline_seconds,
                    ));
                    log::debug!(
                        "[AUDIO][LOCAL_FOOTSTEP] route=split_spatial order=publish_before_play event_seq={} kind={:?} side={:?} surface={:?} contact={:?} direct_listener_local={:?} environment_gain_db={:.1} speed_mps={:.3} event_sim_time={:.6} spatial_revision={} active={}",
                        event.event_seq,
                        event.kind,
                        event.side,
                        event.surface,
                        event.contact_world,
                        prepared_voice.routing.direct_listener_local,
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
        if !self.reserve_voice_slot(prepared.spatial.len()) {
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
            &path,
            gain_db,
            routing.environment_world,
        ) {
            Ok(emitter) => prepared.spatial.push(PreparedSpatialFootstep {
                event,
                emitter,
                routing,
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

    fn reserve_voice_slot(&mut self, pending: usize) -> bool {
        while self.active.len() + self.retiring.len() + pending >= MAX_ACTIVE_LOCAL_FOOTSTEPS {
            let Some(oldest) = self.active.take_oldest() else {
                return false;
            };
            log::warn!(
                "[AUDIO][LOCAL_FOOTSTEP] route=split_spatial event_seq={} reason=voice_cap_steal cap={}",
                oldest.event_seq,
                MAX_ACTIVE_LOCAL_FOOTSTEPS,
            );
            self.retire_voice(oldest, true, "voice_cap_steal");
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

    fn handle_petalsonic_event(&mut self, event: PetalSonicEvent) {
        match event {
            PetalSonicEvent::PlaybackCompleted {
                emitter,
                control,
                tag,
            } => {
                let event_seq = tag.0;
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
                self.retire_voice(voice, false, "completed");
            }
            other => log::debug!("PetalSonic event: {other:?}"),
        }
    }

    fn handle_voice_telemetry(&self, event: VoiceTelemetryEvent) {
        match event {
            VoiceTelemetryEvent::FirstRendered(telemetry) => {
                let event_seq = telemetry.play_command_id.0;
                let Some(voice) = self.active.get(event_seq) else {
                    log::debug!(
                        "[AUDIO][LOCAL_FOOTSTEP] event_seq={event_seq} reason=unmatched_first_render"
                    );
                    return;
                };
                let contract = FirstRenderContractCheck::for_voice(
                    voice,
                    telemetry.spatial_revision,
                    telemetry.direct_local_pose,
                    telemetry.acoustic_origin,
                );
                if contract.satisfied() {
                    log::debug!(
                        "[AUDIO][LOCAL_FOOTSTEP] route=split_spatial event_seq={event_seq} state=first_render render_block={} published_revision={} audible_revision={} direct_listener_local={:?} environment_world={:?}",
                        telemetry.render_block_index,
                        voice.published_revision,
                        telemetry.spatial_revision,
                        voice.routing.direct_listener_local,
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
            } => log::debug!(
                "[AUDIO][LOCAL_FOOTSTEP] route=split_spatial event_seq={} state=environment_response spatial_revision={} geometry_version={} age_ms={:.3}",
                play_command_id.0,
                response.spatial_revision,
                response.geometry_version,
                response.age.as_secs_f64() * 1000.0,
            ),
            other => log::debug!("PetalSonic voice telemetry: {other:?}"),
        }
    }

    fn retire_voice(
        &mut self,
        voice: ManagedFootstepVoice<TransientSpatialEmitter, PlaybackControl>,
        stop_first: bool,
        reason: &'static str,
    ) {
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
        self.retire_prepared_emitter(voice.event_seq, voice.emitter, reason);
    }

    fn retire_prepared_emitter(
        &mut self,
        event_seq: u64,
        emitter: TransientSpatialEmitter,
        reason: &'static str,
    ) {
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
        ActiveFootstepRegistry, FirstRenderContractCheck, FootstepClipBank, FootstepRouting,
        LocalPlayerFootstepRoutingMode, ManagedFootstepVoice, MAX_ACTIVE_LOCAL_FOOTSTEPS,
    };
    use crate::gameplay::camera::{
        FootstepEvent, FootstepKind, FootstepSide, FootstepSurface, Gait,
    };
    use glam::Vec3;

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
            deadline,
        )
    }

    #[test]
    fn direct_anchor_is_listener_local_and_independent_of_world_motion() {
        let first = FootstepRouting::for_event(&event(1, Vec3::new(10.0, 2.0, -4.0), 0.55));
        let after_stop_and_reverse =
            FootstepRouting::for_event(&event(2, Vec3::new(-20.0, 8.0, 30.0), -0.55));

        assert_eq!(first.direct_listener_local, Vec3::new(0.0, -0.08, 0.0));
        assert_eq!(
            after_stop_and_reverse.direct_listener_local,
            first.direct_listener_local
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
            Some(voice.routing.direct_pose()),
            Some(voice.routing.environment_pose()),
        );
        assert!(exact.satisfied());

        let stale = FirstRenderContractCheck::for_voice(
            &voice,
            voice.published_revision - 1,
            Some(voice.routing.direct_pose()),
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
            Some(voice.routing.direct_pose()),
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
        ] {
            footstep.surface = surface;
            assert_eq!(bank.path_for(&footstep), fallback);
            assert_eq!(footstep.surface, surface);
        }
    }
}
