use crate::audio::{SpatialSoundManager, TreeRustleControl, TreeRustleParams};
use crate::wind::{Wind, WindResponseCurve, WindSource};
use anyhow::Result;
use glam::Vec3;
use std::sync::Arc;
use uuid::Uuid;

const TREE_SILENT_VOLUME_DB: f32 = -80.0;
const VOLUME_EPSILON: f32 = 0.01;
const TREE_AUDIO_FULL_WIND_STRENGTH: f32 = 8.0;
const TREE_AUDIO_DECAY_RATE_MIN: f32 = 0.25;
const TREE_AUDIO_DECAY_RATE_MAX: f32 = 8.0;

/// Represents a single looping tree ambience source that can react to wind.
#[allow(dead_code)]
pub struct TreeAudioSource {
    pub uuid: Uuid,
    pub tree_id: u32,
    pub position: Vec3,
    pub cluster_size: u32,
    wind_volume_db: f32,
    target_response: f32,
    current_response: f32,
    current_volume_db: f32,
    last_update_time_seconds: Option<f32>,
    wind_response_curve: WindResponseCurve,
    rustle_control: Arc<TreeRustleControl>,
}

impl TreeAudioSource {
    pub fn new(
        uuid: Uuid,
        tree_id: u32,
        position: Vec3,
        cluster_size: u32,
        wind_volume_db: f32,
        wind_response_curve: WindResponseCurve,
        rustle_control: Arc<TreeRustleControl>,
    ) -> Self {
        Self {
            uuid,
            tree_id,
            position,
            cluster_size,
            wind_volume_db,
            target_response: 0.0,
            current_response: 0.0,
            current_volume_db: TREE_SILENT_VOLUME_DB,
            last_update_time_seconds: None,
            wind_response_curve,
            rustle_control,
        }
    }

    /// Keep the GUI response curve setter wired for now, but the debug audio path below
    /// intentionally bypasses the response curve and uses linear sampled wind strength.
    pub fn set_wind_response_curve(&mut self, wind_response_curve: WindResponseCurve) {
        self.wind_response_curve = wind_response_curve;
    }

    pub fn set_rustle_params(&self, params: TreeRustleParams) {
        self.rustle_control.set_params(params);
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

    pub fn update(
        &mut self,
        wind: &Wind,
        time_seconds: f32,
        wind_sources: &[WindSource],
        wind_audio_attack_decay: f32,
        wind_audio_release_decay: f32,
        spatial_sound_manager: &SpatialSoundManager,
    ) -> Result<()> {
        let target_response = Self::linear_sampled_wind_response(
            wind.sample_sources(self.position, time_seconds, wind_sources)
                .length(),
        );
        let response = self.inertial_response(
            target_response,
            time_seconds,
            wind_audio_attack_decay,
            wind_audio_release_decay,
        );
        self.target_response = target_response;
        self.last_update_time_seconds = Some(time_seconds);
        self.rustle_control.set_wind_response(response);
        self.apply_response_volume(response, spatial_sound_manager)
    }

    fn linear_sampled_wind_response(sampled_strength: f32) -> f32 {
        (sampled_strength.max(0.0) / TREE_AUDIO_FULL_WIND_STRENGTH).clamp(0.0, 1.0)
    }

    fn inertial_response(
        &self,
        target_response: f32,
        time_seconds: f32,
        wind_audio_attack_decay: f32,
        wind_audio_release_decay: f32,
    ) -> f32 {
        let target_response = target_response.clamp(0.0, 1.0);
        let Some(last_update_time_seconds) = self.last_update_time_seconds else {
            return target_response;
        };
        let delta_time = (time_seconds - last_update_time_seconds).max(0.0);
        if delta_time <= f32::EPSILON {
            return self.current_response;
        }

        let decay_control = if target_response >= self.current_response {
            wind_audio_attack_decay
        } else {
            wind_audio_release_decay
        }
        .clamp(0.0, 1.0);
        let blend_rate = TREE_AUDIO_DECAY_RATE_MIN
            + (TREE_AUDIO_DECAY_RATE_MAX - TREE_AUDIO_DECAY_RATE_MIN) * decay_control;
        let alpha = 1.0 - (-blend_rate * delta_time).exp();
        self.current_response + (target_response - self.current_response) * alpha
    }

    fn apply_response_volume(
        &mut self,
        response: f32,
        spatial_sound_manager: &SpatialSoundManager,
    ) -> Result<()> {
        let response = response.clamp(0.0, 1.0);
        let base_wind = self.rustle_control.params().base_wind;
        let target_volume_db = if response <= f32::EPSILON && base_wind <= f32::EPSILON {
            TREE_SILENT_VOLUME_DB
        } else {
            // Runtime rustle now uses wind response inside the procedural model.
            // Keep SourceConfig volume as a master/cluster gain instead of
            // double-applying wind amplitude through both volume and synthesis.
            self.wind_volume_db
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
