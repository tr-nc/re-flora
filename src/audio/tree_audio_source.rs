use crate::audio::SpatialSoundManager;
use crate::wind::{Wind, WindResponseCurve};
use anyhow::Result;
use glam::Vec3;
use uuid::Uuid;

const WIND_VOLUME_SWING_DB: f32 = 20.0;
const VOLUME_EPSILON: f32 = 0.01;

/// Represents a single looping tree ambience source that can react to wind.
#[allow(dead_code)]
pub struct TreeAudioSource {
    pub uuid: Uuid,
    pub tree_id: u32,
    pub position: Vec3,
    pub cluster_size: u32,
    base_volume_db: f32,
    current_volume_db: f32,
    wind_response_curve: WindResponseCurve,
}

impl TreeAudioSource {
    pub fn new(
        uuid: Uuid,
        tree_id: u32,
        position: Vec3,
        cluster_size: u32,
        base_volume_db: f32,
        wind_response_curve: WindResponseCurve,
    ) -> Self {
        Self {
            uuid,
            tree_id,
            position,
            cluster_size,
            base_volume_db,
            current_volume_db: base_volume_db,
            wind_response_curve,
        }
    }

    /// Sample the wind at the source position and update the playing source volume.
    ///
    /// We only care about the magnitude of the planar wind vector for audio intensity.
    pub fn update(
        &mut self,
        wind: &Wind,
        time_seconds: f32,
        spatial_sound_manager: &SpatialSoundManager,
    ) -> Result<()> {
        let normalized =
            wind.sample_response(self.position, time_seconds, self.wind_response_curve);

        let target_volume_db = self.base_volume_db + normalized * WIND_VOLUME_SWING_DB;
        if (target_volume_db - self.current_volume_db).abs() <= VOLUME_EPSILON {
            return Ok(());
        }

        // debug
        spatial_sound_manager.update_source_volume(self.uuid, target_volume_db)?;
        self.current_volume_db = target_volume_db;
        Ok(())
    }
}
