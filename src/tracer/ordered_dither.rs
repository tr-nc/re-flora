//! Saved A/B preferences normalized at the renderer boundary. The shader owns
//! the patterns/quantizer; this module owns only settings and uniform encoding.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrderedDitherSettings {
    flags: u32,
    pattern: u32,
    levels: u32,
    strength: f32,
}

impl Default for OrderedDitherSettings {
    fn default() -> Self {
        Self::from_controls([false; 5], 0, 8, 1.0)
    }
}

impl OrderedDitherSettings {
    /// Switch order: global scene, god rays, lens flare, sky background, terrain ambient.
    pub fn from_controls(switches: [bool; 5], pattern: u32, levels: u32, strength: f32) -> Self {
        let flags = switches
            .into_iter()
            .enumerate()
            .fold(0, |flags, (i, enabled)| flags | (u32::from(enabled) << i));
        Self {
            flags,
            pattern: u32::from(pattern == 1),
            levels: levels.clamp(2, 32),
            strength: if strength.is_finite() {
                strength.clamp(0.0, 1.0)
            } else {
                0.0
            },
        }
    }

    pub(super) fn options(self, samples_per_axis: u32) -> [u32; 4] {
        [
            self.flags,
            self.pattern,
            self.levels,
            samples_per_axis.max(1),
        ]
    }

    pub(super) fn strength(self) -> f32 {
        self.strength
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_preserves_every_original_path() {
        let settings = OrderedDitherSettings::default();
        assert_eq!(settings.options(1), [0, 0, 8, 1]);
        assert_eq!(settings.strength(), 1.0);
    }

    #[test]
    fn normalization_handles_corrupt_inputs_without_losing_switch_preferences() {
        for strength in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.0, 0.0] {
            let settings = OrderedDitherSettings::from_controls([true; 5], 99, 0, strength);
            assert_eq!(settings.options(0), [31, 0, 2, 1]);
            assert_eq!(settings.strength(), 0.0);
        }
        let settings = OrderedDitherSettings::from_controls([true; 5], 1, 99, 2.0);
        assert_eq!(settings.options(4), [31, 1, 32, 4]);
        assert_eq!(settings.strength(), 1.0);
    }

    #[test]
    fn every_effect_has_an_independent_bit() {
        for i in 0..5 {
            let mut switches = [false; 5];
            switches[i] = true;
            assert_eq!(
                OrderedDitherSettings::from_controls(switches, 0, 8, 1.0).options(2)[0],
                1 << i
            );
        }
    }
}
