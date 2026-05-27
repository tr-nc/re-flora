use crate::audio::SpatialSoundManager;
use crate::wind::{Wind, WindResponseCurve, WindSource};
use anyhow::Result;
use glam::Vec3;
use uuid::Uuid;

const TREE_SILENT_VOLUME_DB: f32 = -80.0;
const VOLUME_EPSILON: f32 = 0.01;
const TREE_AUDIO_FULL_WIND_STRENGTH: f32 = 8.0;
const TREE_AUDIO_RESPONSE_FLOOR: f32 = 0.02;

/// Represents a single looping tree ambience source that can react to wind.
#[allow(dead_code)]
pub struct TreeAudioSource {
    pub uuid: Uuid,
    pub tree_id: u32,
    pub position: Vec3,
    pub cluster_size: u32,
    wind_volume_db: f32,
    current_response: f32,
    current_volume_db: f32,
    wind_response_curve: WindResponseCurve,
}

impl TreeAudioSource {
    pub fn new(
        uuid: Uuid,
        tree_id: u32,
        position: Vec3,
        cluster_size: u32,
        wind_volume_db: f32,
        wind_response_curve: WindResponseCurve,
    ) -> Self {
        Self {
            uuid,
            tree_id,
            position,
            cluster_size,
            wind_volume_db,
            current_response: 0.0,
            current_volume_db: TREE_SILENT_VOLUME_DB,
            wind_response_curve,
        }
    }

    /// Keep the GUI response curve setter wired for now, but the debug audio path below
    /// intentionally bypasses the response curve and uses linear sampled wind strength.
    pub fn set_wind_response_curve(&mut self, wind_response_curve: WindResponseCurve) {
        self.wind_response_curve = wind_response_curve;
    }

    pub fn set_wind_volume_db(
        &mut self,
        wind_volume_db: f32,
        spatial_sound_manager: &SpatialSoundManager,
    ) -> Result<()> {
        if (wind_volume_db - self.wind_volume_db).abs() <= VOLUME_EPSILON {
            return Ok(());
        }

        self.wind_volume_db = wind_volume_db;
        self.apply_response_volume(self.current_response, spatial_sound_manager)
    }

    pub fn current_response(&self) -> f32 {
        self.current_response
    }

    pub fn current_volume_db(&self) -> f32 {
        self.current_volume_db
    }

    pub fn wind_volume_db(&self) -> f32 {
        self.wind_volume_db
    }

    pub fn update(
        &mut self,
        wind: &Wind,
        time_seconds: f32,
        wind_sources: &[WindSource],
        spatial_sound_manager: &SpatialSoundManager,
    ) -> Result<()> {
        let normalized = Self::linear_sampled_wind_response(
            wind.sample_sources(self.position, time_seconds, wind_sources).length(),
            wind_sources,
        );
        self.apply_response_volume(normalized, spatial_sound_manager)
    }

    fn linear_sampled_wind_response(sampled_strength: f32, wind_sources: &[WindSource]) -> f32 {
        let has_active_wind = wind_sources
            .iter()
            .any(|source| source.strength.max(0.0) > f32::EPSILON);
        if !has_active_wind {
            return 0.0;
        }

        let linear = (sampled_strength.max(0.0) / TREE_AUDIO_FULL_WIND_STRENGTH).clamp(0.0, 1.0);
        TREE_AUDIO_RESPONSE_FLOOR + linear * (1.0 - TREE_AUDIO_RESPONSE_FLOOR)
    }

    fn apply_response_volume(
        &mut self,
        response: f32,
        spatial_sound_manager: &SpatialSoundManager,
    ) -> Result<()> {
        let response = response.clamp(0.0, 1.0);
        let target_volume_db = if response <= f32::EPSILON {
            TREE_SILENT_VOLUME_DB
        } else {
            // `SourceConfig` takes dB, but for tuning we want the sampled wind
            // response to scale perceived source amplitude linearly. Convert the
            // linear response into a dB offset instead of linearly interpolating
            // dB from silence, which made normal wind responses nearly inaudible.
            self.wind_volume_db + 20.0 * response.log10()
        };

        self.current_response = response;
        if (target_volume_db - self.current_volume_db).abs() <= VOLUME_EPSILON {
            return Ok(());
        }

        spatial_sound_manager.update_source_volume(self.uuid, target_volume_db)?;
        self.current_volume_db = target_volume_db;
        Ok(())
    }
}
