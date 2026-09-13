//! Saved game mix, independent of per-event gain and spatial attenuation.
use petalsonic::BusParams;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AudioCategory {
    Leaves,
    Cicadas,
    Footsteps,
    Terrain,
    Interface,
}

impl AudioCategory {
    pub const ALL: [Self; 5] = [
        Self::Leaves,
        Self::Cicadas,
        Self::Footsteps,
        Self::Terrain,
        Self::Interface,
    ];
    pub fn bus_name(self) -> &'static str {
        match self {
            Self::Leaves => "leaves",
            Self::Cicadas => "cicadas",
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
    pub fn params(self) -> BusParams {
        let scale = if self.scale.is_finite() {
            self.scale.clamp(0.0, 8.0)
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
#[serde(default)]
pub struct AudioMixSettings {
    pub master: MixChannel,
    pub leaves: MixChannel,
    pub cicadas: MixChannel,
    pub footsteps: MixChannel,
    pub terrain: MixChannel,
    pub interface: MixChannel,
}
impl AudioMixSettings {
    pub fn channel(self, category: AudioCategory) -> MixChannel {
        match category {
            AudioCategory::Leaves => self.leaves,
            AudioCategory::Cicadas => self.cicadas,
            AudioCategory::Footsteps => self.footsteps,
            AudioCategory::Terrain => self.terrain,
            AudioCategory::Interface => self.interface,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mix_defaults_are_neutral_and_each_channel_is_independent() {
        let mut settings = AudioMixSettings::default();
        settings.cicadas = MixChannel {
            enabled: false,
            scale: 4.0,
        };
        for category in AudioCategory::ALL {
            let p = settings.channel(category).params();
            assert_eq!(p.muted, category == AudioCategory::Cicadas);
            assert_eq!(
                p.gain_db,
                if category == AudioCategory::Cicadas {
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
