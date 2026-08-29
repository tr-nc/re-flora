use crate::audio::CanopyAudioGenerationKey;
use petalsonic::{
    AcousticDiscardReason, AcousticExtentTelemetry, AcousticTelemetryEvent,
    AcousticVoiceConclusionTelemetry, Emitter, PlayCommandId, VoiceTelemetryEvent,
};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AudioTelemetryOwner {
    Canopy(CanopyAudioGenerationKey),
    LocalFootstep {
        event_seq: u64,
        play_command_id: PlayCommandId,
    },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub(crate) enum AudioTelemetryOwnershipError {
    #[error("audio telemetry emitter {emitter} is already owned")]
    EmitterAlreadyOwned { emitter: Emitter },
    #[error("audio telemetry PlayCommandId {play_command_id:?} is already owned")]
    PlayCommandAlreadyOwned { play_command_id: PlayCommandId },
    #[error("audio telemetry emitter {emitter} has no owner")]
    UnknownEmitter { emitter: Emitter },
    #[error("audio telemetry emitter {emitter} is not canopy-owned")]
    NotCanopyOwned { emitter: Emitter },
}

#[derive(Debug)]
pub(crate) enum CanopyAcousticObservation {
    ExtentResponse {
        generation: CanopyAudioGenerationKey,
        response: Box<AcousticExtentTelemetry>,
    },
    SolveDiscarded {
        spatial_revision: u64,
        geometry_version: u64,
    },
}

#[derive(Debug)]
pub(crate) struct LocalFootstepVoiceObservation {
    pub(crate) event_seq: u64,
    pub(crate) event: VoiceTelemetryEvent,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LocalFootstepAcousticObservation {
    pub(crate) event_seq: u64,
    pub(crate) conclusion: AcousticVoiceConclusionTelemetry,
}

#[derive(Debug, Default)]
pub(crate) struct LocalFootstepTelemetryObservations {
    pub(crate) acoustic: Vec<LocalFootstepAcousticObservation>,
    pub(crate) voice: Vec<LocalFootstepVoiceObservation>,
}

#[derive(Debug, Default)]
pub(crate) struct AudioTelemetryObservations {
    pub(crate) canopy: Vec<CanopyAcousticObservation>,
    pub(crate) local_footsteps: LocalFootstepTelemetryObservations,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct AudioTelemetryRoutingDiagnostics {
    pub(crate) unowned_voice_events: u64,
    pub(crate) mismatched_voice_emitters: u64,
    pub(crate) unowned_acoustic_events: u64,
}

/// Owns Re: Flora's closed-set routing of PetalSonic telemetry to domain observations.
///
/// PetalSonic lifecycle events remain on their independent stream. This router only consumes the
/// independently bounded Voice and acoustic telemetry streams, once per application frame, and
/// returns frame-scoped observations without retaining a secondary event inbox.
#[derive(Default)]
pub(crate) struct AudioTelemetryRouter {
    owners: HashMap<Emitter, AudioTelemetryOwner>,
    footstep_commands: HashMap<PlayCommandId, (Emitter, u64)>,
    diagnostics: AudioTelemetryRoutingDiagnostics,
}

impl AudioTelemetryRouter {
    /// Drains each PetalSonic telemetry queue exactly once, then performs closed-set routing.
    ///
    /// Lifecycle events deliberately are not accepted here; their independent stream is owned by
    /// the local-footstep frame observation cycle.
    pub(crate) fn drain_once(
        &mut self,
        drain_voice: impl FnOnce() -> Vec<VoiceTelemetryEvent>,
        drain_acoustic: impl FnOnce() -> Vec<AcousticTelemetryEvent>,
    ) -> AudioTelemetryObservations {
        self.route(drain_voice(), drain_acoustic())
    }

    pub(crate) fn claim_canopy(
        &mut self,
        emitter: Emitter,
        generation: CanopyAudioGenerationKey,
    ) -> Result<(), AudioTelemetryOwnershipError> {
        self.claim(emitter, AudioTelemetryOwner::Canopy(generation))
    }

    pub(crate) fn claim_local_footstep(
        &mut self,
        emitter: Emitter,
        event_seq: u64,
        play_command_id: PlayCommandId,
    ) -> Result<(), AudioTelemetryOwnershipError> {
        if self.footstep_commands.contains_key(&play_command_id) {
            return Err(AudioTelemetryOwnershipError::PlayCommandAlreadyOwned { play_command_id });
        }
        self.claim(
            emitter,
            AudioTelemetryOwner::LocalFootstep {
                event_seq,
                play_command_id,
            },
        )?;
        self.footstep_commands
            .insert(play_command_id, (emitter, event_seq));
        Ok(())
    }

    pub(crate) fn claim_canopy_replacement(
        &mut self,
        old_emitter: Emitter,
        new_emitter: Emitter,
    ) -> Result<(), AudioTelemetryOwnershipError> {
        let Some(AudioTelemetryOwner::Canopy(generation)) = self.owners.get(&old_emitter).copied()
        else {
            return Err(if self.owners.contains_key(&old_emitter) {
                AudioTelemetryOwnershipError::NotCanopyOwned {
                    emitter: old_emitter,
                }
            } else {
                AudioTelemetryOwnershipError::UnknownEmitter {
                    emitter: old_emitter,
                }
            });
        };
        self.claim_canopy(new_emitter, generation)
    }

    pub(crate) fn release(&mut self, emitter: Emitter) {
        if let Some(AudioTelemetryOwner::LocalFootstep {
            play_command_id, ..
        }) = self.owners.remove(&emitter)
        {
            self.footstep_commands.remove(&play_command_id);
        }
    }

    fn route(
        &mut self,
        voice_events: Vec<VoiceTelemetryEvent>,
        acoustic_events: Vec<AcousticTelemetryEvent>,
    ) -> AudioTelemetryObservations {
        let mut observations = AudioTelemetryObservations::default();
        for event in voice_events {
            self.route_voice(event, &mut observations.local_footsteps.voice);
        }
        for event in acoustic_events {
            self.route_acoustic(event, &mut observations);
        }
        observations
    }

    fn claim(
        &mut self,
        emitter: Emitter,
        owner: AudioTelemetryOwner,
    ) -> Result<(), AudioTelemetryOwnershipError> {
        if self.owners.contains_key(&emitter) {
            return Err(AudioTelemetryOwnershipError::EmitterAlreadyOwned { emitter });
        }
        self.owners.insert(emitter, owner);
        Ok(())
    }

    fn route_voice(
        &mut self,
        event: VoiceTelemetryEvent,
        observations: &mut Vec<LocalFootstepVoiceObservation>,
    ) {
        let (play_command_id, observed_emitter) = match &event {
            VoiceTelemetryEvent::FirstRendered(telemetry) => {
                (telemetry.play_command_id, Some(telemetry.emitter))
            }
            VoiceTelemetryEvent::EnvironmentResponse {
                play_command_id, ..
            } => (*play_command_id, None),
            VoiceTelemetryEvent::EnergySummary(telemetry) => {
                (telemetry.play_command_id, Some(telemetry.emitter))
            }
            _ => return,
        };
        let Some(&(owned_emitter, event_seq)) = self.footstep_commands.get(&play_command_id) else {
            self.diagnostics.unowned_voice_events =
                self.diagnostics.unowned_voice_events.saturating_add(1);
            log::debug!(
                "[AUDIO][TELEMETRY_ROUTER] play_command_id={play_command_id:?} reason=unowned_voice_telemetry"
            );
            return;
        };
        if observed_emitter.is_some_and(|emitter| emitter != owned_emitter) {
            self.diagnostics.mismatched_voice_emitters =
                self.diagnostics.mismatched_voice_emitters.saturating_add(1);
            log::error!(
                "[AUDIO][TELEMETRY_ROUTER] play_command_id={play_command_id:?} reason=voice_emitter_mismatch expected={owned_emitter} observed={observed_emitter:?}"
            );
            return;
        }
        observations.push(LocalFootstepVoiceObservation { event_seq, event });
    }

    fn route_acoustic(
        &mut self,
        event: AcousticTelemetryEvent,
        observations: &mut AudioTelemetryObservations,
    ) {
        match event {
            AcousticTelemetryEvent::ExtentResponse(response) => {
                match self.owners.get(&response.emitter).copied() {
                    Some(AudioTelemetryOwner::Canopy(generation)) => {
                        observations
                            .canopy
                            .push(CanopyAcousticObservation::ExtentResponse {
                                generation,
                                response,
                            });
                    }
                    Some(AudioTelemetryOwner::LocalFootstep { .. }) => {}
                    None => {
                        self.diagnostics.unowned_acoustic_events =
                            self.diagnostics.unowned_acoustic_events.saturating_add(1);
                        log::debug!(
                            "[AUDIO][TELEMETRY_ROUTER] emitter={} reason=unowned_extent_response",
                            response.emitter,
                        );
                    }
                }
            }
            AcousticTelemetryEvent::VoiceConclusion(conclusion) => {
                match self.owners.get(&conclusion.emitter).copied() {
                    Some(AudioTelemetryOwner::LocalFootstep { event_seq, .. }) => observations
                        .local_footsteps
                        .acoustic
                        .push(LocalFootstepAcousticObservation {
                            event_seq,
                            conclusion,
                        }),
                    Some(AudioTelemetryOwner::Canopy(_)) => {}
                    None => {
                        self.diagnostics.unowned_acoustic_events =
                            self.diagnostics.unowned_acoustic_events.saturating_add(1);
                        log::debug!(
                            "[AUDIO][TELEMETRY_ROUTER] emitter={} voice_id={} reason=unowned_voice_conclusion",
                            conclusion.emitter,
                            conclusion.voice_id,
                        );
                    }
                }
            }
            AcousticTelemetryEvent::SolveDiscarded {
                spatial_revision,
                geometry_version,
                reason: AcousticDiscardReason::Superseded,
            } => observations
                .canopy
                .push(CanopyAcousticObservation::SolveDiscarded {
                    spatial_revision,
                    geometry_version,
                }),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AudioTelemetryRouter, CanopyAcousticObservation, LocalFootstepAcousticObservation,
    };
    use crate::audio::CanopyAudioGenerationKey;
    use petalsonic::{
        AcousticExtentTelemetry, AcousticOcclusionState, AcousticRouteOutcome,
        AcousticRouteTelemetry, AcousticSolveStatus, AcousticTelemetryEvent,
        AcousticVoiceConclusionTelemetry, Emitter, EmitterDesc, EnvironmentResponse,
        OutputDevicePolicy, PetalSonicWorld, PetalSonicWorldDesc, PlayCommandId, ResidentClip,
        VoiceTelemetryEvent,
    };
    use std::time::Duration;

    fn emitters() -> (PetalSonicWorld, Emitter, Emitter, Emitter) {
        let world = PetalSonicWorld::new(PetalSonicWorldDesc {
            block_size: 64,
            output_device: OutputDevicePolicy::PinnedNameContains(
                "re-flora-audio-telemetry-router-test-device-that-does-not-exist".to_owned(),
            ),
            ..PetalSonicWorldDesc::default()
        })
        .unwrap();
        let clip = ResidentClip::from_mono_pcm(vec![0.0; 16], 48_000).unwrap();
        let create = || {
            world
                .create_emitter(clip.clone(), EmitterDesc::non_spatial())
                .unwrap()
        };
        let first = create();
        let second = create();
        let third = create();
        (world, first, second, third)
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
            voice_id: 17,
            emitter,
            spatial_revision: 5,
            geometry_version: 7,
            response_spatial_revision: 5,
            response_geometry_version: 7,
            extent_sample_count: 0,
            direct: route(),
            environment: route(),
            lobes: Vec::new(),
            solve_status: AcousticSolveStatus::Solved,
            cache_age_seconds: 0.0,
            budget_member: true,
        }
    }

    fn conclusion(emitter: Emitter) -> AcousticVoiceConclusionTelemetry {
        AcousticVoiceConclusionTelemetry {
            voice_id: 18,
            emitter,
            spatial_revision: 6,
            geometry_version: 7,
            candidate_rank: Some(1),
            candidate_limit: 8,
            direct: AcousticRouteOutcome::Disabled,
            environment: AcousticRouteOutcome::Applied,
            environment_transmission_gain: [0.8, 0.7, 0.6],
            early_tap_count: 2,
            solve_status: Some(AcousticSolveStatus::Solved),
        }
    }

    #[test]
    fn one_route_call_produces_stable_domain_observations_without_cross_drain() {
        let (world, canopy_emitter, footstep_emitter, _) = emitters();
        let generation = CanopyAudioGenerationKey::new(4, 9);
        let command = PlayCommandId((1 << 63) + 23);
        let mut router = AudioTelemetryRouter::default();
        router.claim_canopy(canopy_emitter, generation).unwrap();
        router
            .claim_local_footstep(footstep_emitter, 23, command)
            .unwrap();

        let response = EnvironmentResponse {
            spatial_revision: 5,
            geometry_version: 7,
            age: Duration::from_millis(3),
        };
        let footstep_conclusion = conclusion(footstep_emitter);
        let observations = router.route(
            vec![VoiceTelemetryEvent::EnvironmentResponse {
                play_command_id: command,
                response,
            }],
            vec![
                AcousticTelemetryEvent::ExtentResponse(Box::new(extent(canopy_emitter))),
                AcousticTelemetryEvent::VoiceConclusion(footstep_conclusion),
            ],
        );

        assert_eq!(observations.canopy.len(), 1);
        match &observations.canopy[0] {
            CanopyAcousticObservation::ExtentResponse {
                generation: observed_generation,
                response,
            } => {
                assert_eq!(*observed_generation, generation);
                assert_eq!(response.emitter, canopy_emitter);
                assert_eq!(response.voice_id, 17);
            }
            other => panic!("expected canopy extent response, got {other:?}"),
        }
        assert_eq!(observations.local_footsteps.voice.len(), 1);
        assert_eq!(observations.local_footsteps.voice[0].event_seq, 23);
        assert_eq!(
            observations.local_footsteps.voice[0].event,
            VoiceTelemetryEvent::EnvironmentResponse {
                play_command_id: command,
                response,
            }
        );
        assert_eq!(
            observations.local_footsteps.acoustic,
            vec![LocalFootstepAcousticObservation {
                event_seq: 23,
                conclusion: footstep_conclusion,
            }]
        );
        world.close().unwrap();
    }

    #[test]
    fn ownership_replacement_and_release_control_late_attribution() {
        let (world, old_canopy, new_canopy, footstep) = emitters();
        let generation = CanopyAudioGenerationKey::new(7, 11);
        let command = PlayCommandId((1 << 63) + 31);
        let mut router = AudioTelemetryRouter::default();
        router.claim_canopy(old_canopy, generation).unwrap();
        router
            .claim_canopy_replacement(old_canopy, new_canopy)
            .unwrap();
        router.release(old_canopy);
        router.claim_local_footstep(footstep, 31, command).unwrap();

        let while_owned = router.route(
            Vec::new(),
            vec![AcousticTelemetryEvent::VoiceConclusion(conclusion(
                footstep,
            ))],
        );
        assert_eq!(while_owned.local_footsteps.acoustic.len(), 1);

        router.release(footstep);
        let after_release = router.route(
            Vec::new(),
            vec![
                AcousticTelemetryEvent::ExtentResponse(Box::new(extent(new_canopy))),
                AcousticTelemetryEvent::VoiceConclusion(conclusion(footstep)),
            ],
        );
        assert!(matches!(
            after_release.canopy.as_slice(),
            [CanopyAcousticObservation::ExtentResponse {
                generation: observed,
                ..
            }] if *observed == generation
        ));
        assert!(after_release.local_footsteps.acoustic.is_empty());
        assert_eq!(router.diagnostics.unowned_acoustic_events, 1);
        world.close().unwrap();
    }
}
