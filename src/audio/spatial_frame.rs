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
    use super::{SpatialFrame, SpatialFrameFacts};
    use crate::audio::spatial_sound_manager::{FootstepRetirementProbe, SpatialAudioQueueProbe};
    use crate::audio::{CanopyAcousticObservation, CanopyAudioGenerationKey, SpatialSoundManager};
    use crate::gameplay::{
        CameraPose, FootstepEvent, FootstepKind, FootstepSide, FootstepSurface, Gait,
    };
    use glam::Vec3;
    use petalsonic::{
        AcousticExtentTelemetry, AcousticHit, AcousticOcclusionState, AcousticRay,
        AcousticRayQuerySnapshot, AcousticRouteOutcome, AcousticRouteTelemetry,
        AcousticSceneSnapshot, AcousticSolveStatus, AcousticTelemetryEvent,
        AcousticVoiceConclusionTelemetry, Emitter, EnvironmentResponse,
        EnvironmentalAcousticsBudget, OcclusionProfile, PetalSonicEvent, PlayCommandId,
        PlaybackTag, ResidentClip, SourceExtent, VoiceTelemetryEvent,
    };
    use std::{
        panic::{catch_unwind, AssertUnwindSafe},
        sync::Arc,
        time::Duration,
    };

    struct NoAcousticHits;

    impl AcousticRayQuerySnapshot for NoAcousticHits {
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

    fn test_manager() -> SpatialSoundManager {
        SpatialSoundManager::new(
            64,
            AcousticSceneSnapshot::new(1, Arc::new(NoAcousticHits)),
            Some("re-flora-spatial-frame-acceptance-device-that-does-not-exist".to_owned()),
            EnvironmentalAcousticsBudget::default(),
        )
        .unwrap()
    }

    fn camera() -> CameraPose {
        CameraPose {
            position: Vec3::ZERO,
            yaw_deg: 0.0,
            pitch_deg: 0.0,
            fov_deg: 60.0,
        }
    }

    fn footstep(event_seq: u64, x: f32) -> FootstepEvent {
        FootstepEvent {
            event_seq,
            kind: FootstepKind::Stride(Gait::Run),
            side: FootstepSide::Left,
            contact_world: Vec3::new(x, 0.0, 0.0),
            surface: FootstepSurface::Unknown,
            speed_mps: 2.0,
            sim_time_seconds: 0.0,
        }
    }

    fn route() -> AcousticRouteTelemetry {
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

    fn extent(emitter: Emitter) -> AcousticExtentTelemetry {
        AcousticExtentTelemetry {
            voice_id: 71,
            emitter,
            spatial_revision: 12,
            geometry_version: 9,
            response_spatial_revision: 12,
            response_geometry_version: 9,
            extent_sample_count: 0,
            direct: route(),
            environment: route(),
            lobes: Vec::new(),
            solve_status: AcousticSolveStatus::Solved,
            cache_age_seconds: 0.0,
            budget_member: true,
        }
    }

    fn conclusion(emitter: Emitter, voice_id: u64) -> AcousticVoiceConclusionTelemetry {
        AcousticVoiceConclusionTelemetry {
            voice_id,
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

    #[test]
    fn advance_drains_one_frame_once_before_real_completion_and_deadline_retirement() {
        let manager = test_manager();
        let mut frame = SpatialFrame::new(manager.clone());
        let completed_event_seq = 101;
        let deadline_event_seq = 102;
        let initial_footsteps = [
            footstep(completed_event_seq, 1.0),
            footstep(deadline_event_seq, 2.0),
        ];

        assert!(frame
            .advance(SpatialFrameFacts {
                sim_time_seconds: 0.0,
                listener: camera(),
                local_footsteps: &initial_footsteps,
                footstep_volume_gain_db: 0.0,
            })
            .is_empty());

        let (completed_emitter, completed_control, completed_deadline) = frame
            .local_player_footsteps
            .test_active_voice_identity(completed_event_seq);
        let (deadline_emitter, _, deadline) = frame
            .local_player_footsteps
            .test_active_voice_identity(deadline_event_seq);

        let generation = CanopyAudioGenerationKey::new(7, 19);
        let canopy_source = manager
            .add_canopy_looping_clip_with_extent_at_phase(
                generation,
                ResidentClip::from_mono_pcm(vec![0.0; 16], 48_000).unwrap(),
                -20.0,
                Vec3::new(0.0, 1.0, 0.0),
                0.0,
                SourceExtent::Point,
                OcclusionProfile::default(),
            )
            .unwrap();
        let canopy_emitter = manager.test_source_emitter(canopy_source);
        let canopy_response = Box::new(extent(canopy_emitter));
        let canopy_response_address = (&*canopy_response) as *const AcousticExtentTelemetry;

        let response = |play_command_id| VoiceTelemetryEvent::EnvironmentResponse {
            play_command_id,
            response: EnvironmentResponse {
                spatial_revision: 12,
                geometry_version: 9,
                age: Duration::from_millis(2),
            },
        };
        let completed_command = PlayCommandId((1 << 63) + completed_event_seq);
        let deadline_command = PlayCommandId((1 << 63) + deadline_event_seq);
        let probe = Arc::new(SpatialAudioQueueProbe::new(
            vec![response(completed_command), response(deadline_command)],
            vec![
                AcousticTelemetryEvent::ExtentResponse(canopy_response),
                AcousticTelemetryEvent::VoiceConclusion(conclusion(completed_emitter, 72)),
                AcousticTelemetryEvent::VoiceConclusion(conclusion(deadline_emitter, 73)),
            ],
            vec![PetalSonicEvent::PlaybackCompleted {
                emitter: completed_emitter,
                control: completed_control,
                tag: PlaybackTag(completed_command.0),
            }],
        ));
        manager.install_audio_queue_probe(probe.clone());

        let retirement_time = completed_deadline.max(deadline) + 0.001;
        let mut canopy_owner_observations = frame.advance(SpatialFrameFacts {
            sim_time_seconds: retirement_time,
            listener: camera(),
            local_footsteps: &[],
            footstep_volume_gain_db: 0.0,
        });

        probe.assert_all_queues_drained_once();
        assert_eq!(frame.local_player_footsteps.test_voice_counts(), (0, 2));
        assert_eq!(
            probe.retirements(),
            vec![
                FootstepRetirementProbe {
                    event_seq: completed_event_seq,
                    reason: "completed",
                    environment_response_observed: true,
                    acoustic_conclusion_observed: true,
                },
                FootstepRetirementProbe {
                    event_seq: deadline_event_seq,
                    reason: "completion_deadline_expired",
                    environment_response_observed: true,
                    acoustic_conclusion_observed: true,
                },
            ]
        );

        assert_eq!(canopy_owner_observations.len(), 1);
        match canopy_owner_observations.pop().unwrap() {
            CanopyAcousticObservation::ExtentResponse {
                generation: observed_generation,
                response,
            } => {
                assert_eq!(observed_generation, generation);
                assert_eq!(response.emitter, canopy_emitter);
                assert_eq!(response.voice_id, 71);
                assert_eq!(response.spatial_revision, 12);
                assert_eq!(response.geometry_version, 9);
                assert_eq!(
                    (&*response) as *const AcousticExtentTelemetry,
                    canopy_response_address,
                );
            }
            other => panic!("expected canopy extent response, got {other:?}"),
        }

        assert!(
            catch_unwind(AssertUnwindSafe(|| manager.drain_audio_telemetry())).is_err(),
            "a second telemetry drain in the same probed frame must fail",
        );
    }
}
