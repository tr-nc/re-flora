//! Resolve one immutable tree from a recipe and an individual key. No ambient RNG,
//! position-dependent color or live settings are consulted after this seam.
use super::TreeDesc;
use glam::Vec3;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TreeAppearance {
    pub hue_degrees: f32,
    pub saturation: f32,
    pub brightness: f32,
}

impl Default for TreeAppearance {
    fn default() -> Self {
        Self {
            hue_degrees: 0.,
            saturation: 1.,
            brightness: 1.,
        }
    }
}

impl TreeAppearance {
    pub fn valid(self) -> bool {
        (-120. ..=60.).contains(&self.hue_degrees)
            && (0.25..=2.).contains(&self.saturation)
            && (0.25..=2.).contains(&self.brightness)
    }

    /// Offset the shared palette in sRGB HSV, retaining its height gradient.
    /// Identity is exact, including for legacy snapshots with no appearance field.
    pub fn apply_linear(self, color: Vec3) -> Vec3 {
        if self == Self::default() {
            return color;
        }
        let srgb = color.map(|v| {
            if v <= 0.0031308 {
                v * 12.92
            } else {
                1.055 * v.powf(1. / 2.4) - 0.055
            }
        });
        let max = srgb.max_element();
        let min = srgb.min_element();
        let delta = max - min;
        let hue = if delta < 1e-6 {
            0.
        } else if max == srgb.x {
            ((srgb.y - srgb.z) / delta).rem_euclid(6.)
        } else if max == srgb.y {
            (srgb.z - srgb.x) / delta + 2.
        } else {
            (srgb.x - srgb.y) / delta + 4.
        };
        let hue = (hue + self.hue_degrees / 60.).rem_euclid(6.);
        let saturation = if max > 0. {
            (delta / max * self.saturation).clamp(0., 1.)
        } else {
            0.
        };
        let value = (max * self.brightness).clamp(0., 1.);
        let c = value * saturation;
        let x = c * (1. - (hue.rem_euclid(2.) - 1.).abs());
        let rgb = match hue as u32 {
            0 => Vec3::new(c, x, 0.),
            1 => Vec3::new(x, c, 0.),
            2 => Vec3::new(0., c, x),
            3 => Vec3::new(0., x, c),
            4 => Vec3::new(x, 0., c),
            _ => Vec3::new(c, 0., x),
        } + Vec3::splat(value - c);
        rgb.map(|v| {
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TreeVariation {
    pub enabled: bool,
    pub shape: bool,
    /// Relative maximum deviation from the recipe's size, not statistical variance.
    pub size_spread: f32,
    pub mean: TreeAppearance,
    pub spread: TreeAppearance,
}

impl TreeVariation {
    pub fn resolve(self, mut recipe: TreeDesc, individual: u64) -> TreeDesc {
        if self.enabled && self.shape {
            recipe.branching.seed = individual;
        }
        let sample = |mean, spread, lo, hi, domain| {
            let mean = bounded(mean, lo, hi, (lo + hi) * 0.5);
            // Symmetric support keeps the configured mean even at domain edges;
            // clipping samples instead would pile probability up at the limits.
            let spread = bounded(spread, 0., hi - lo, 0.)
                .min(mean - lo)
                .min(hi - mean);
            mean + if self.enabled {
                spread * triangular(individual, domain)
            } else {
                0.
            }
        };
        if self.enabled {
            recipe.size *= 1. + bounded(self.size_spread, 0., 0.5, 0.) * triangular(individual, 1);
        }
        recipe.appearance = TreeAppearance {
            hue_degrees: sample(
                self.mean.hue_degrees,
                self.spread.hue_degrees,
                -120.,
                60.,
                2,
            ),
            saturation: sample(self.mean.saturation, self.spread.saturation, 0.25, 2., 3),
            brightness: sample(self.mean.brightness, self.spread.brightness, 0.25, 2., 4),
        };
        recipe
    }
}

fn bounded(value: f32, lo: f32, hi: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(lo, hi)
    } else {
        fallback
    }
}

fn mix(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

// Independent stable channels: adjusting color cannot reroll shape or size.
fn triangular(individual: u64, domain: u64) -> f32 {
    let unit = |salt| (mix(individual ^ salt) >> 40) as f32 / 16_777_216.;
    let domain = domain.wrapping_mul(0x9e3779b97f4a7c15);
    unit(domain) + unit(domain ^ 0xd1b54a32d192ed03) - 1.
}

#[cfg(test)]
mod tests {
    use super::*;
    fn settings() -> TreeVariation {
        TreeVariation {
            enabled: true,
            shape: true,
            size_spread: 0.2,
            mean: TreeAppearance::default(),
            spread: TreeAppearance {
                hue_degrees: 30.,
                saturation: 0.2,
                brightness: 0.2,
            },
        }
    }
    #[test]
    fn resolved_individual_is_stable_independent_and_owned_through_growth_and_restore() {
        let recipe = TreeDesc::default();
        let settings = settings();
        let first = settings.resolve(recipe.clone(), 907);
        assert_eq!(first, settings.resolve(recipe.clone(), 907));
        assert_ne!(first, settings.resolve(recipe.clone(), 908));
        let mut changed = settings;
        changed.mean.hue_degrees = -40.;
        let next = changed.resolve(recipe, 907);
        assert_eq!(first.size, next.size);
        assert_eq!(first.branching, next.branching);
        assert_ne!(first.appearance, next.appearance);
        let restored: TreeDesc =
            serde_json::from_str(&serde_json::to_string(&first).unwrap()).unwrap();
        assert_eq!(restored, first);
        for age in [0.1, 0.5, 1.] {
            assert_eq!(restored.at_age(age).appearance, first.appearance);
        }
    }
    #[test]
    fn disabled_uses_means_and_fixed_shape_without_random_consumption() {
        let mut settings = settings();
        settings.enabled = false;
        settings.mean.hue_degrees = -35.;
        let recipe = TreeDesc::default();
        let first = settings.resolve(recipe.clone(), 1);
        assert_eq!(first, settings.resolve(recipe.clone(), u64::MAX));
        assert_eq!(first.branching.seed, recipe.branching.seed);
        assert_eq!(first.size, recipe.size);
        assert_eq!(first.appearance, settings.mean);
    }
    #[test]
    fn deterministic_distribution_has_bounded_support_and_expected_mean() {
        let settings = settings();
        let recipe = TreeDesc::default();
        let mut sum = 0.;
        let mut squares = 0.;
        for key in 0..4096 {
            let desc = settings.resolve(recipe.clone(), key);
            let deviation = desc.size / recipe.size - 1.;
            assert!(deviation.abs() <= 0.2);
            assert!(desc.appearance.valid());
            assert!(desc.appearance.hue_degrees.abs() <= 30.);
            sum += deviation;
            squares += deviation * deviation;
        }
        assert!((sum / 4096.).abs() < 0.004);
        assert!((squares / 4096. - 0.2 * 0.2 / 6.).abs() < 0.0007);
    }
    #[test]
    fn shape_lock_and_zero_spreads_are_independent_of_master_randomization() {
        let mut config = settings();
        config.shape = false;
        config.size_spread = 0.;
        config.spread = TreeAppearance {
            hue_degrees: 0.,
            saturation: 0.,
            brightness: 0.,
        };
        for species in super::super::TreeSpecies::PLANTABLE {
            let recipe = species.default_desc();
            assert_eq!(config.resolve(recipe.clone(), 1), recipe);
            assert_eq!(config.resolve(recipe.clone(), u64::MAX), recipe);
        }
    }

    #[test]
    fn edge_means_and_invalid_spreads_remain_finite_bounded_and_centered() {
        let mut config = settings();
        config.mean = TreeAppearance {
            hue_degrees: -120.,
            saturation: 0.25,
            brightness: 2.,
        };
        config.spread = TreeAppearance {
            hue_degrees: 90.,
            saturation: f32::NAN,
            brightness: f32::INFINITY,
        };
        config.size_spread = f32::NAN;
        for key in 0..16 {
            let result = config.resolve(TreeDesc::default(), key);
            assert_eq!(result.appearance, config.mean);
            assert!(result.appearance.valid());
            assert_eq!(result.size, TreeDesc::default().size);
        }
    }

    #[test]
    fn old_description_is_neutral_and_hue_maps_green_to_yellow_and_red() {
        let mut json = serde_json::to_value(TreeDesc::default()).unwrap();
        json.as_object_mut().unwrap().remove("appearance");
        let old: TreeDesc = serde_json::from_value(json).unwrap();
        assert_eq!(old.appearance, TreeAppearance::default());
        let color = Vec3::new(0.17, 0.64, 0.08);
        assert_eq!(old.appearance.apply_linear(color), color);
        for (shift, expected) in [(-60., Vec3::new(1., 1., 0.)), (-120., Vec3::X)] {
            let style = TreeAppearance {
                hue_degrees: shift,
                ..Default::default()
            };
            assert!(style.apply_linear(Vec3::Y).distance(expected) < 1e-5);
        }
    }
}
