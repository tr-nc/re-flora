use crate::audio::{CanopyAcousticSampleProvenance, CanopyAudioSourceKey, SpatialSoundManager};
use crate::wind::{Wind, WindResponseCurve, WindSource};
use anyhow::Result;
use glam::Vec3;
use uuid::Uuid;

const TREE_SILENT_VOLUME_DB: f32 = -80.0;
const VOLUME_EPSILON: f32 = 0.01;
const TREE_AUDIO_FULL_WIND_STRENGTH: f32 = 8.0;
const TREE_AUDIO_DECAY_RATE_MIN: f32 = 0.25;
const TREE_AUDIO_DECAY_RATE_MAX: f32 = 8.0;

/// One PetalSonic point-emitter adapter owned by a weighted canopy sample generation.
pub struct TreeAudioSource {
    pub uuid: Uuid,
    pub key: CanopyAudioSourceKey,
    pub position_tree_voxels: Vec3,
    pub position: Vec3,
    pub sample_weight: f32,
    pub clearance_voxels: f32,
    pub content_seed: u64,
    pub phase: f32,
    pub provenance: CanopyAcousticSampleProvenance,
    wind_volume_db: f32,
    lifecycle_power: f32,
    target_response: f32,
    current_response: f32,
    current_volume_db: f32,
    last_update_time_seconds: Option<f32>,
    wind_response_curve: WindResponseCurve,
    base_wind: f32,
}

impl TreeAudioSource {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        uuid: Uuid,
        key: CanopyAudioSourceKey,
        position_tree_voxels: Vec3,
        position: Vec3,
        sample_weight: f32,
        clearance_voxels: f32,
        content_seed: u64,
        phase: f32,
        provenance: CanopyAcousticSampleProvenance,
        wind_volume_db: f32,
        wind_response_curve: WindResponseCurve,
        base_wind: f32,
    ) -> Self {
        Self {
            uuid,
            key,
            position_tree_voxels,
            position,
            sample_weight: sample_weight.clamp(0.0, 1.0),
            clearance_voxels,
            content_seed,
            phase: phase.clamp(0.0, 1.0),
            provenance,
            wind_volume_db,
            lifecycle_power: 0.0,
            target_response: 0.0,
            current_response: 0.0,
            current_volume_db: TREE_SILENT_VOLUME_DB,
            last_update_time_seconds: None,
            wind_response_curve,
            base_wind,
        }
    }

    pub fn set_wind_response_curve(&mut self, wind_response_curve: WindResponseCurve) {
        self.wind_response_curve = wind_response_curve;
    }

    pub fn set_base_wind(&mut self, base_wind: f32) {
        self.base_wind = base_wind.clamp(0.0, 1.0);
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

    pub fn set_lifecycle_power(
        &mut self,
        lifecycle_power: f32,
        spatial_sound_manager: &SpatialSoundManager,
    ) -> Result<()> {
        let lifecycle_power = lifecycle_power.clamp(0.0, 1.0);
        if (lifecycle_power - self.lifecycle_power).abs() <= f32::EPSILON {
            return Ok(());
        }
        self.lifecycle_power = lifecycle_power;
        self.apply_response_volume(self.current_response, spatial_sound_manager)
    }

    pub fn lifecycle_power(&self) -> f32 {
        self.lifecycle_power
    }

    pub fn target_response(&self) -> f32 {
        self.target_response
    }

    pub fn current_response(&self) -> f32 {
        self.current_response
    }

    pub fn current_volume_db(&self) -> f32 {
        self.current_volume_db
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
        let content_active = response > f32::EPSILON || self.base_wind > f32::EPSILON;
        let target_volume_db = if content_active {
            Self::volume_db_for_power(
                self.wind_volume_db,
                self.sample_weight * self.lifecycle_power,
            )
        } else {
            TREE_SILENT_VOLUME_DB
        };

        self.current_response = response;
        if (target_volume_db - self.current_volume_db).abs() <= VOLUME_EPSILON {
            return Ok(());
        }

        spatial_sound_manager.update_source_volume(self.uuid, target_volume_db)?;
        self.current_volume_db = target_volume_db;
        Ok(())
    }

    fn volume_db_for_power(base_volume_db: f32, power: f32) -> f32 {
        if power <= f32::EPSILON {
            return TREE_SILENT_VOLUME_DB;
        }
        (base_volume_db + 10.0 * power.log10()).max(TREE_SILENT_VOLUME_DB)
    }
}

#[cfg(test)]
mod tests {
    use super::TreeAudioSource;

    #[test]
    fn weighted_sample_gain_preserves_normalized_power() {
        let base = -10.0;
        let quarter = TreeAudioSource::volume_db_for_power(base, 0.25);

        assert!((quarter - (-16.0206)).abs() < 1.0e-3);
        let reconstructed_power = 4.0 * 10.0_f32.powf((quarter - base) / 10.0);
        assert!((reconstructed_power - 1.0).abs() < 1.0e-5);
        assert_eq!(TreeAudioSource::volume_db_for_power(base, 0.0), -80.0);
    }
}
