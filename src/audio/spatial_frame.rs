use super::{LocalPlayerFootstepAudio, SpatialSoundManager};
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

    pub(crate) fn advance(&mut self, facts: SpatialFrameFacts<'_>) {
        self.spatial_sound_manager.observe_listener(facts.listener);

        self.local_player_footsteps
            .set_volume_gain_db(facts.footstep_volume_gain_db);

        // Complete or deadline-expire old Voices before reserving emitters for this frame. Any
        // resulting structural removal participates in the publication below.
        self.local_player_footsteps.maintain(facts.sim_time_seconds);
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
    }
}
