use super::{CanopyAcousticObservation, LocalPlayerFootstepAudio, SpatialSoundManager};
use crate::gameplay::{CameraPose, FootstepEvent};

/// Domain facts observed by spatial audio at the end of one application frame.
pub(crate) struct SpatialFrameFacts<'a> {
    pub(crate) sim_time_seconds: f64,
    pub(crate) listener: CameraPose,
    pub(crate) local_footsteps: &'a [FootstepEvent],
    pub(crate) footstep_volume_gain_db: f32,
}

/// Owns Re: Flora's per-frame spatial-audio transaction.
///
/// Listener and existing-emitter poses may be coalesced by the audio cadence. Registering or
/// retiring an emitter marks the shared spatial generation dirty, so this transaction publishes
/// that structural change before starting any Voice that depends on it. A failed publication
/// terminates every prepared footstep instead of allowing playback against an older generation.
/// It also owns the single per-frame telemetry drain: local-footstep observations are applied
/// before completion/deadline retirement, while typed canopy observations are returned to the
/// canopy domain owner. PetalSonic lifecycle events remain on their independent stream.
pub(crate) struct SpatialFrame {
    spatial_sound_manager: SpatialSoundManager,
    local_player_footsteps: LocalPlayerFootstepAudio,
}

impl SpatialFrame {
    pub(crate) fn new(spatial_sound_manager: SpatialSoundManager) -> Self {
        Self {
            local_player_footsteps: LocalPlayerFootstepAudio::new(spatial_sound_manager.clone()),
            spatial_sound_manager,
        }
    }

    pub(crate) fn advance(
        &mut self,
        facts: SpatialFrameFacts<'_>,
    ) -> Vec<CanopyAcousticObservation> {
        let telemetry = self.spatial_sound_manager.drain_audio_telemetry();
        self.spatial_sound_manager.observe_listener(facts.listener);

        self.local_player_footsteps
            .set_volume_gain_db(facts.footstep_volume_gain_db);

        // Complete or deadline-expire old Voices before reserving emitters for this frame. Any
        // resulting structural removal participates in the publication below.
        self.local_player_footsteps
            .maintain(facts.sim_time_seconds, telemetry.local_footsteps);
        let prepared = self
            .local_player_footsteps
            .prepare(facts.local_footsteps, facts.sim_time_seconds);

        match self
            .spatial_sound_manager
            .publish_spatial_frame(facts.sim_time_seconds)
        {
            Ok(publication) => self
                .local_player_footsteps
                .play_after_publication(prepared, publication),
            Err(err) => {
                log::warn!("Failed to publish spatial audio frame: {err}");
                self.local_player_footsteps
                    .abort_prepared(prepared, "spatial_frame_publish_failed");
            }
        }
        telemetry.canopy
    }
}

#[cfg(test)]
mod tests {
    use super::super::local_player_footsteps::{
        for_each_local_footstep_frame_action, LocalFootstepFrameAction,
    };
    use super::super::{AudioTelemetryRouter, CanopyAcousticObservation, CanopyAudioGenerationKey};
    use petalsonic::{
        AcousticExtentTelemetry, AcousticOcclusionState, AcousticRouteOutcome,
        AcousticRouteTelemetry, AcousticSolveStatus, AcousticTelemetryEvent,
        AcousticVoiceConclusionTelemetry, Emitter, EmitterDesc, EnvironmentResponse,
        OutputDevicePolicy, PetalSonicEvent, PetalSonicWorld, PetalSonicWorldDesc, PlayCommandId,
        PlayOptions, PlaybackTag, ResidentClip, VoiceTelemetryEvent,
    };
    use std::{cell::Cell, time::Duration};

    fn telemetry_world() -> (PetalSonicWorld, Emitter, Emitter) {
        let world = PetalSonicWorld::new(PetalSonicWorldDesc {
            block_size: 64,
            output_device: OutputDevicePolicy::PinnedNameContains(
                "re-flora-spatial-frame-acceptance-device-that-does-not-exist".to_owned(),
            ),
            ..PetalSonicWorldDesc::default()
        })
        .unwrap();
        let clip = ResidentClip::from_mono_pcm(vec![0.0; 16], 48_000).unwrap();
        let canopy = world
            .create_emitter(clip.clone(), EmitterDesc::non_spatial())
            .unwrap();
        let footstep = world
            .create_emitter(clip, EmitterDesc::non_spatial())
            .unwrap();
        (world, canopy, footstep)
    }

    fn route_telemetry() -> AcousticRouteTelemetry {
        AcousticRouteTelemetry {
            sample_count: 0,
            samples: Vec::new(),
            ray_count: 0,
            cache_hit_count: 0,
            hit_count: 0,
            visible_fraction: 0.0,
            raw_gain: [0.0; 3],
            filtered_gain: [0.0; 3],
            classified_state: AcousticOcclusionState::Visible,
            dwell_seconds: 0.0,
        }
    }

    fn canopy_extent(emitter: Emitter) -> AcousticExtentTelemetry {
        AcousticExtentTelemetry {
            voice_id: 41,
            emitter,
            spatial_revision: 12,
            geometry_version: 9,
            response_spatial_revision: 12,
            response_geometry_version: 9,
            extent_sample_count: 0,
            direct: route_telemetry(),
            environment: route_telemetry(),
            lobes: Vec::new(),
            solve_status: AcousticSolveStatus::Solved,
            cache_age_seconds: 0.0,
            budget_member: true,
        }
    }

    fn footstep_conclusion(emitter: Emitter) -> AcousticVoiceConclusionTelemetry {
        AcousticVoiceConclusionTelemetry {
            voice_id: 42,
            emitter,
            spatial_revision: 12,
            geometry_version: 9,
            candidate_rank: Some(0),
            candidate_limit: 8,
            direct: AcousticRouteOutcome::Applied,
            environment: AcousticRouteOutcome::Applied,
            environment_transmission_gain: [0.9, 0.8, 0.7],
            early_tap_count: 3,
            solve_status: Some(AcousticSolveStatus::Solved),
        }
    }

    #[derive(Debug, PartialEq, Eq)]
    enum ObservedStep {
        Acoustic,
        Voice,
        PlaybackCompleted,
        CompletionDeadlineRetirement,
        TelemetryDeadlineRetirement,
    }

    #[test]
    fn one_frame_drains_telemetry_once_and_preserves_domain_and_lifecycle_ownership() {
        let (world, canopy_emitter, footstep_emitter) = telemetry_world();
        let generation = CanopyAudioGenerationKey::new(5, 13);
        let event_seq = 29;
        let command = PlayCommandId((1 << 63) + event_seq);
        let tag = PlaybackTag(command.0);
        let control = world
            .play_controlled(footstep_emitter, PlayOptions::once(), tag)
            .unwrap();

        let mut router = AudioTelemetryRouter::default();
        router.claim_canopy(canopy_emitter, generation).unwrap();
        router
            .claim_local_footstep(footstep_emitter, event_seq, command)
            .unwrap();

        let voice_queue = vec![VoiceTelemetryEvent::EnvironmentResponse {
            play_command_id: command,
            response: EnvironmentResponse {
                spatial_revision: 12,
                geometry_version: 9,
                age: Duration::from_millis(2),
            },
        }];
        let canopy_response = Box::new(canopy_extent(canopy_emitter));
        let canopy_response_address = (&*canopy_response) as *const AcousticExtentTelemetry;
        let footstep_acoustic = footstep_conclusion(footstep_emitter);
        let acoustic_queue = vec![
            AcousticTelemetryEvent::ExtentResponse(canopy_response),
            AcousticTelemetryEvent::VoiceConclusion(footstep_acoustic),
        ];
        let lifecycle_queue = vec![PetalSonicEvent::PlaybackCompleted {
            emitter: footstep_emitter,
            control,
            tag,
        }];

        let voice_drains = Cell::new(0);
        let acoustic_drains = Cell::new(0);
        let observations = router.drain_once(
            || {
                voice_drains.set(voice_drains.get() + 1);
                voice_queue
            },
            || {
                acoustic_drains.set(acoustic_drains.get() + 1);
                acoustic_queue
            },
        );
        assert_eq!(voice_drains.get(), 1);
        assert_eq!(acoustic_drains.get(), 1);

        // The router cannot consume PetalSonic's independent lifecycle stream.
        assert_eq!(lifecycle_queue.len(), 1);

        let mut canopy_owner_observations = observations.canopy;
        assert_eq!(canopy_owner_observations.len(), 1);
        match canopy_owner_observations.pop().unwrap() {
            CanopyAcousticObservation::ExtentResponse {
                generation: observed_generation,
                response,
            } => {
                assert_eq!(observed_generation, generation);
                assert_eq!(response.emitter, canopy_emitter);
                assert_eq!(response.voice_id, 41);
                assert_eq!(response.spatial_revision, 12);
                assert_eq!(response.geometry_version, 9);
                assert_eq!(
                    (&*response) as *const AcousticExtentTelemetry,
                    canopy_response_address,
                    "the complete response allocation reaches the canopy owner",
                );
            }
            other => panic!("expected canopy extent response, got {other:?}"),
        }

        let lifecycle_drains = Cell::new(0);
        let mut applied = Vec::new();
        for_each_local_footstep_frame_action(
            observations.local_footsteps,
            || {
                lifecycle_drains.set(lifecycle_drains.get() + 1);
                lifecycle_queue
            },
            17.0,
            |action| match action {
                LocalFootstepFrameAction::Acoustic(observation) => {
                    assert_eq!(observation.event_seq, event_seq);
                    assert_eq!(observation.conclusion, footstep_acoustic);
                    applied.push(ObservedStep::Acoustic);
                }
                LocalFootstepFrameAction::Voice(observation) => {
                    assert_eq!(observation.event_seq, event_seq);
                    assert!(matches!(
                        observation.event,
                        VoiceTelemetryEvent::EnvironmentResponse { play_command_id, .. }
                            if play_command_id == command
                    ));
                    applied.push(ObservedStep::Voice);
                }
                LocalFootstepFrameAction::Lifecycle(PetalSonicEvent::PlaybackCompleted {
                    emitter,
                    control: observed_control,
                    tag: observed_tag,
                }) => {
                    assert_eq!(emitter, footstep_emitter);
                    assert_eq!(observed_control, control);
                    assert_eq!(observed_tag, tag);
                    applied.push(ObservedStep::PlaybackCompleted);
                }
                LocalFootstepFrameAction::Lifecycle(other) => {
                    panic!("unexpected lifecycle event: {other:?}");
                }
                LocalFootstepFrameAction::RetireCompletionDeadlines { sim_time_seconds } => {
                    assert_eq!(sim_time_seconds, 17.0);
                    applied.push(ObservedStep::CompletionDeadlineRetirement);
                }
                LocalFootstepFrameAction::FinalizeTelemetryDeadlines { sim_time_seconds } => {
                    assert_eq!(sim_time_seconds, 17.0);
                    applied.push(ObservedStep::TelemetryDeadlineRetirement);
                }
            },
        );
        assert_eq!(lifecycle_drains.get(), 1);
        assert_eq!(
            applied,
            vec![
                ObservedStep::Acoustic,
                ObservedStep::Voice,
                ObservedStep::PlaybackCompleted,
                ObservedStep::CompletionDeadlineRetirement,
                ObservedStep::TelemetryDeadlineRetirement,
            ]
        );
        world.close().unwrap();
    }
}
