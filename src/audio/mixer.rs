//! Saved game mix, independent of per-event gain and spatial attenuation.
use petalsonic::BusParams;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AudioCategory {
    Leaves,
    TreeCicadas,
    GroundCicadas,
    Footsteps,
    Terrain,
    Interface,
}

impl AudioCategory {
    pub const ALL: [Self; 6] = [
        Self::Leaves,
        Self::TreeCicadas,
        Self::GroundCicadas,
        Self::Footsteps,
        Self::Terrain,
        Self::Interface,
    ];
    pub fn bus_name(self) -> &'static str {
        match self {
            Self::Leaves => "leaves",
            Self::TreeCicadas => "tree_cicadas",
            Self::GroundCicadas => "ground_cicadas",
            Self::Footsteps => "footsteps",
            Self::Terrain => "terrain",
            Self::Interface => "interface",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MixChannel {
    pub enabled: bool,
    pub scale: f32,
}
impl Default for MixChannel {
    fn default() -> Self {
        Self {
            enabled: true,
            scale: 1.0,
        }
    }
}
impl MixChannel {
    pub const MAX_SCALE: f32 = 128.0;

    pub fn params(self) -> BusParams {
        let scale = if self.scale.is_finite() {
            self.scale.clamp(0.0, Self::MAX_SCALE)
        } else {
            1.0
        };
        BusParams {
            muted: !self.enabled || scale == 0.0,
            gain_db: if scale > 0.0 {
                20.0 * scale.log10()
            } else {
                0.0
            },
            ..BusParams::default()
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(from = "StoredAudioMixSettings")]
pub struct AudioMixSettings {
    pub master: MixChannel,
    pub leaves: MixChannel,
    pub tree_cicadas: MixChannel,
    pub ground_cicadas: MixChannel,
    pub footsteps: MixChannel,
    pub terrain: MixChannel,
    pub interface: MixChannel,
}
impl AudioMixSettings {
    pub fn channel(self, category: AudioCategory) -> MixChannel {
        match category {
            AudioCategory::Leaves => self.leaves,
            AudioCategory::TreeCicadas => self.tree_cicadas,
            AudioCategory::GroundCicadas => self.ground_cicadas,
            AudioCategory::Footsteps => self.footsteps,
            AudioCategory::Terrain => self.terrain,
            AudioCategory::Interface => self.interface,
        }
    }
}

// Read the previous combined channel without retaining a hidden master gain.
// Explicit new fields win; a missing new field inherits the legacy value.
#[derive(Default, Deserialize)]
#[serde(default)]
struct StoredAudioMixSettings {
    master: MixChannel,
    leaves: MixChannel,
    cicadas: Option<MixChannel>,
    tree_cicadas: Option<MixChannel>,
    ground_cicadas: Option<MixChannel>,
    footsteps: MixChannel,
    terrain: MixChannel,
    interface: MixChannel,
}

impl From<StoredAudioMixSettings> for AudioMixSettings {
    fn from(stored: StoredAudioMixSettings) -> Self {
        let legacy = stored.cicadas.unwrap_or_default();
        Self {
            master: stored.master,
            leaves: stored.leaves,
            tree_cicadas: stored.tree_cicadas.unwrap_or(legacy),
            ground_cicadas: stored.ground_cicadas.unwrap_or(legacy),
            footsteps: stored.footsteps,
            terrain: stored.terrain,
            interface: stored.interface,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_cicada_mix_migrates_without_changing_either_sound() {
        let mix: AudioMixSettings =
            toml::from_str("[cicadas]\nenabled = false\nscale = 37.0\n").unwrap();
        assert_eq!(
            mix.tree_cicadas,
            MixChannel {
                enabled: false,
                scale: 37.0
            }
        );
        assert_eq!(mix.ground_cicadas, mix.tree_cicadas);
        let saved = toml::to_string(&mix).unwrap();
        assert!(!saved.contains("[cicadas]"));
        assert_eq!(toml::from_str::<AudioMixSettings>(&saved).unwrap(), mix);
    }

    #[test]
    fn split_cicada_mix_is_independent_and_round_trips() {
        let mut mix: AudioMixSettings =
            toml::from_str("[cicadas]\nscale = 37.0\n[tree_cicadas]\nscale = 2.0\n").unwrap();
        assert_eq!(mix.tree_cicadas.scale, 2.0);
        assert_eq!(mix.ground_cicadas.scale, 37.0);
        mix.ground_cicadas = MixChannel {
            enabled: false,
            scale: 0.5,
        };
        assert!(!mix.channel(AudioCategory::TreeCicadas).params().muted);
        assert!(mix.channel(AudioCategory::GroundCicadas).params().muted);
        let loaded: AudioMixSettings = toml::from_str(&toml::to_string(&mix).unwrap()).unwrap();
        assert_eq!(loaded, mix);
        assert_ne!(
            AudioCategory::TreeCicadas.bus_name(),
            AudioCategory::GroundCicadas.bus_name()
        );
        assert_eq!(
            toml::from_str::<AudioMixSettings>("").unwrap(),
            AudioMixSettings::default()
        );
    }
    #[test]
    fn tenfold_master_boost_is_twenty_db_without_old_eightfold_ceiling() {
        let gain = |scale| {
            MixChannel {
                scale,
                ..Default::default()
            }
            .params()
            .gain_db
        };
        assert!((gain(80.0) - gain(8.0) - 20.0).abs() < 0.001);
        assert!((gain(0.1) - gain(0.01) - 20.0).abs() < 0.001);
    }
    #[test]
    fn mix_defaults_are_neutral_and_each_channel_is_independent() {
        let mut settings = AudioMixSettings::default();
        settings.tree_cicadas = MixChannel {
            enabled: false,
            scale: 4.0,
        };
        for category in AudioCategory::ALL {
            let p = settings.channel(category).params();
            assert_eq!(p.muted, category == AudioCategory::TreeCicadas);
            assert_eq!(
                p.gain_db,
                if category == AudioCategory::TreeCicadas {
                    20.0 * 4.0_f32.log10()
                } else {
                    0.0
                }
            );
        }
        assert!(!settings.master.params().muted);
    }
    #[test]
    fn zero_is_real_mute_and_invalid_scale_is_safe() {
        assert!(
            MixChannel {
                scale: 0.0,
                ..Default::default()
            }
            .params()
            .muted
        );
        assert_eq!(
            MixChannel {
                scale: f32::NAN,
                ..Default::default()
            }
            .params()
            .gain_db,
            0.0
        );
        assert!(
            (MixChannel {
                scale: 0.5,
                ..Default::default()
            }
            .params()
            .gain_db
                + 6.0206)
                .abs()
                < 0.001
        );
    }
}
