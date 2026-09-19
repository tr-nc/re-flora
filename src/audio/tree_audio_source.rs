use crate::audio::{
    CanopyAcousticDescriptor, CanopyAudioGenerationKey, CanopyAudioSourceKey, SpatialSoundManager,
};
use crate::wind_field::WindFieldFrame;
use crate::wind_response::WindResponseCurve;
use anyhow::Result;
use uuid::Uuid;

const TREE_SILENT_VOLUME_DB: f32 = -80.0;
const VOLUME_EPSILON: f32 = 0.01;
const TREE_AUDIO_FULL_WIND_STRENGTH: f32 = 8.0;
const TREE_AUDIO_DECAY_RATE_MIN: f32 = 0.25;
const TREE_AUDIO_DECAY_RATE_MAX: f32 = 8.0;

/// One immutable canopy generation realized as one PetalSonic Emitter and looping Voice.
pub struct CanopyAudioVoice {
    pub uuid: Uuid,
    pub key: CanopyAudioGenerationKey,
    pub descriptor: CanopyAcousticDescriptor,
    pub phase: f32,
    wind_volume_db: f32,
    lifecycle_power: f32,
    target_response: f32,
    current_response: f32,
    current_volume_db: f32,
    last_update_time_seconds: Option<f32>,
    wind_response_curve: WindResponseCurve,
}

impl CanopyAudioVoice {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        uuid: Uuid,
        key: CanopyAudioGenerationKey,
        descriptor: CanopyAcousticDescriptor,
        phase: f32,
        wind_volume_db: f32,
        wind_response_curve: WindResponseCurve,
    ) -> Self {
        Self {
            uuid,
            key,
            descriptor,
            phase: phase.clamp(0.0, 1.0),
            wind_volume_db,
            lifecycle_power: 0.0,
            target_response: 0.0,
            current_response: 0.0,
            current_volume_db: TREE_SILENT_VOLUME_DB,
            last_update_time_seconds: None,
            wind_response_curve,
        }
    }

    pub fn sample_key(
        &self,
        sample_id: crate::audio::CanopyAcousticSampleId,
    ) -> CanopyAudioSourceKey {
        CanopyAudioSourceKey::new(self.key.tree_id(), self.key.generation(), sample_id)
    }

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
        wind: &WindFieldFrame,
        time_seconds: f32,
        wind_audio_attack_decay: f32,
        wind_audio_release_decay: f32,
        spatial_sound_manager: &SpatialSoundManager,
    ) -> Result<()> {
        let target_response =
            Self::sampled_response(&self.descriptor, wind, self.wind_response_curve);
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

    fn sampled_response(
        descriptor: &CanopyAcousticDescriptor,
        wind: &WindFieldFrame,
        curve: WindResponseCurve,
    ) -> f32 {
        descriptor
            .samples()
            .iter()
            .map(|sample| {
                let position = descriptor.sample_world_position(sample);
                let strength =
                    Self::linear_sampled_wind_response(wind.sample_world(position).length());
                // A local gust drives the leaves it reaches, not a threshold on the
                // whole-canopy mean. Keep one weighted emitter and the same wind field.
                sample.weight()
                    * if strength <= 0.0 {
                        0.0
                    } else {
                        curve.factor(strength)
                    }
            })
            .sum::<f32>()
            .clamp(0.0, 1.0)
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
        let target_volume_db =
            Self::response_volume_db(self.wind_volume_db, self.lifecycle_power, response);

        self.current_response = response;
        if (target_volume_db - self.current_volume_db).abs() <= VOLUME_EPSILON {
            return Ok(());
        }

        spatial_sound_manager.update_source_volume(self.uuid, target_volume_db)?;
        self.current_volume_db = target_volume_db;
        Ok(())
    }

    fn response_volume_db(base: f32, power: f32, response: f32) -> f32 {
        // The resident clip is a full-wind reference. Wind response is amplitude,
        // not a binary playback gate; lifecycle fades remain independent power.
        let response = response.clamp(0.0, 1.0);
        Self::volume_db_for_power(base, power * response * response)
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
    use super::CanopyAudioVoice;

    #[test]
    fn fastest_wind_audio_response_still_has_a_125ms_time_constant() {
        use crate::{
            audio::{CanopyAcousticDescriptor, CanopyAudioGenerationKey},
            wind_response::WindResponseCurve,
        };
        let mut voice = CanopyAudioVoice::new(
            uuid::Uuid::nil(),
            CanopyAudioGenerationKey::new(1, 1),
            CanopyAcousticDescriptor::build(1, glam::Vec3::ZERO, 1, &[], &[]),
            0.0,
            0.0,
            WindResponseCurve {
                min_strength: 0.0,
                max_strength: 1.0,
                power: 1.0,
            },
        );
        voice.last_update_time_seconds = Some(0.0);
        let at_125ms = voice.inertial_response(1.0, 0.125, 1.0, 1.0);
        let at_375ms = voice.inertial_response(1.0, 0.375, 1.0, 1.0);
        assert!((at_125ms - 0.63212055).abs() < 1e-6);
        assert!((at_375ms - 0.95021296).abs() < 1e-6);
        voice.current_response = 1.0;
        let release_125ms = voice.inertial_response(0.0, 0.125, 1.0, 1.0);
        assert!((release_125ms - 0.36787945).abs() < 1e-6);
        println!("fast attack: 125ms={at_125ms:.6}, 375ms={at_375ms:.6}; release 125ms={release_125ms:.6}");
    }

    #[test]
    fn manual_gust_replay_reports_local_and_canopy_threshold_onset() {
        use crate::{
            audio::CanopyAcousticDescriptor,
            tree_gen::LeafPlacement,
            wind_field::{GustSettings, WindField},
            wind_response::WindResponseCurve,
        };
        use glam::{Vec2, Vec3};
        let descriptor = CanopyAcousticDescriptor::build(
            1,
            Vec3::new(0.98, 0.7, 1.13),
            1,
            &[-64., 64.].map(|x| LeafPlacement {
                position: Vec3::new(x, 0., 0.),
                anchor: Vec3::ZERO,
            }),
            &[],
        );
        let mut field = WindField::default();
        field.background_enabled = false;
        field.advance(0.0);
        // Recorded 9-item gesture; other gust settings use the startup defaults.
        assert!(field.release_with_settings(
            Vec3::new(0.82689226, 0.42759848, 0.7031249),
            Vec2::new(0.6090692, 0.79311705),
            GustSettings {
                speed: 108.042145,
                ..Default::default()
            }
        ));
        let curve = WindResponseCurve {
            min_strength: 0.135,
            max_strength: 1.0,
            power: 1.0,
        };
        let mut first_local = None;
        let mut first_old_mean = None;
        let mut first_response = None;
        for frame in 1..=480 {
            let time = frame as f32 / 60.0;
            field.advance(time);
            let wind = field.frame();
            let mut mean = 0.0;
            for sample in descriptor.samples() {
                let local = CanopyAudioVoice::linear_sampled_wind_response(
                    wind.sample_world(descriptor.sample_world_position(sample))
                        .length(),
                );
                mean += sample.weight() * local;
                if curve.factor(local) > 0.0 {
                    first_local.get_or_insert(time);
                }
            }
            if curve.factor(mean) > 0.0 {
                first_old_mean.get_or_insert(time);
            }
            if CanopyAudioVoice::sampled_response(&descriptor, &wind, curve) > 0.0 {
                first_response.get_or_insert(time);
            }
        }
        assert!(first_local.is_some());
        assert_eq!(
            first_response, first_local,
            "a locally driven canopy sample must respond in that same update"
        );
        println!("manual gust onset seconds: local={first_local:?} old_mean={first_old_mean:?} response={first_response:?}; synthetic two-leaf canopy, recorded gesture and default gust shape");
    }

    #[test]
    fn quiet_canopy_does_not_play_the_full_wind_loop_at_full_gain() {
        let strong = CanopyAudioVoice::response_volume_db(0.0, 1.0, 1.0);
        let weak = CanopyAudioVoice::response_volume_db(0.0, 1.0, 0.01);
        assert!(weak < strong - 30.0, "weak={weak} strong={strong}");
        assert_eq!(CanopyAudioVoice::response_volume_db(0.0, 1.0, 0.0), -80.0);
    }

    #[test]
    fn canopy_response_samples_the_shared_field_at_its_weighted_leaf_positions() {
        use crate::{
            audio::CanopyAcousticDescriptor, tree_gen::LeafPlacement, wind_field::WindFieldFrame,
        };
        use glam::{Vec2, Vec3};
        let descriptor = CanopyAcousticDescriptor::build(
            1,
            Vec3::new(0.75, 0.5, 1.),
            11,
            &[-32., 32.].map(|x| LeafPlacement {
                position: Vec3::new(x, 4., 0.),
                anchor: Vec3::ZERO,
            }),
            &[],
        );
        assert_eq!(descriptor.samples().len(), 2);
        let curve = crate::wind_response::WindResponseCurve {
            min_strength: 0.0,
            max_strength: 1.0,
            power: 1.0,
        };
        assert_eq!(
            CanopyAudioVoice::sampled_response(&descriptor, &WindFieldFrame::default(), curve),
            0.
        );
        let mut wind = WindFieldFrame::uniform(Vec2::ZERO);
        for (i, pair) in wind.cells.iter_mut().enumerate() {
            pair[0] = ((i * 2) % 32) as f32 / 31. * 8.;
            pair[2] = ((i * 2 + 1) % 32) as f32 / 31. * 8.;
        }
        assert!(
            (CanopyAudioVoice::sampled_response(&descriptor, &wind, curve) - 0.375).abs() < 1e-6
        );
        // A gust reaches one side of the canopy before the other. A locally
        // wind-driven leaf must not wait for the whole-canopy mean to cross Min.
        assert!(
            CanopyAudioVoice::sampled_response(
                &descriptor,
                &wind,
                crate::wind_response::WindResponseCurve {
                    min_strength: 0.4,
                    ..curve
                }
            ) > 0.0
        );
        assert_eq!(
            CanopyAudioVoice::sampled_response(
                &descriptor,
                &WindFieldFrame::uniform(Vec2::X * 8.),
                curve
            ),
            1.
        );
        let wind = WindFieldFrame::uniform(Vec2::X * 2.);
        assert_eq!(
            CanopyAudioVoice::sampled_response(
                &descriptor,
                &wind,
                crate::wind_response::WindResponseCurve {
                    min_strength: 0.3,
                    ..curve
                }
            ),
            0.
        );
        assert_eq!(
            CanopyAudioVoice::sampled_response(
                &descriptor,
                &wind,
                crate::wind_response::WindResponseCurve {
                    max_strength: 0.25,
                    ..curve
                }
            ),
            1.
        );
    }

    #[test]
    fn generation_gain_uses_lifecycle_power_without_sample_count_gain() {
        let base = -10.0;
        let quarter = CanopyAudioVoice::volume_db_for_power(base, 0.25);

        assert!((quarter - (-16.0206)).abs() < 1.0e-3);
        assert_eq!(CanopyAudioVoice::volume_db_for_power(base, 1.0), base);
        assert_eq!(CanopyAudioVoice::volume_db_for_power(base, 0.0), -80.0);
    }
}
