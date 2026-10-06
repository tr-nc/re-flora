//! Saved global Bayer preferences normalized at the renderer boundary.
//! The post-processing shader owns the only quantizer and fixed pattern.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrderedDitherSettings {
    enabled: bool,
    levels: u32,
    strength: f32,
}

impl Default for OrderedDitherSettings {
    fn default() -> Self {
        Self::from_controls(false, 8, 1.0)
    }
}

impl OrderedDitherSettings {
    pub fn from_controls(enabled: bool, levels: u32, strength: f32) -> Self {
        Self {
            enabled,
            levels: levels.clamp(2, 32),
            strength: if strength.is_finite() {
                strength.clamp(0.0, 1.0)
            } else {
                0.0
            },
        }
    }

    pub(super) fn enabled(self) -> u32 {
        u32::from(self.enabled)
    }

    pub(super) fn levels(self) -> u32 {
        self.levels
    }

    pub(super) fn strength(self) -> f32 {
        self.strength
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_preserves_the_original_scene() {
        let settings = OrderedDitherSettings::default();
        assert_eq!(settings.enabled(), 0);
        assert_eq!(settings.levels(), 8);
        assert_eq!(settings.strength(), 1.0);
    }

    #[test]
    fn normalization_preserves_the_global_preference() {
        for strength in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.0, 0.0] {
            let settings = OrderedDitherSettings::from_controls(true, 0, strength);
            assert_eq!(settings.enabled(), 1);
            assert_eq!(settings.levels(), 2);
            assert_eq!(settings.strength(), 0.0);
        }
        let settings = OrderedDitherSettings::from_controls(true, 99, 2.0);
        assert_eq!(settings.enabled(), 1);
        assert_eq!(settings.levels(), 32);
        assert_eq!(settings.strength(), 1.0);
    }

    #[test]
    fn only_post_processing_owns_ordered_dither() {
        for shader in [
            include_str!("../../shader/slang/composition.slang"),
            include_str!("../../shader/slang/composition_scene.slang"),
            include_str!("../../shader/slang/glass_resolve.slang"),
            include_str!("../../shader/slang/tracer.slang"),
            include_str!("../../shader/slang/tracer_types.slang"),
        ] {
            assert!(!shader.contains("ordered_dither"));
            assert!(!shader.contains("orderedDither"));
            assert!(!shader.contains("ordered_pixel_extent"));
        }
        let post = include_str!("../../shader/slang/post_processing.slang");
        assert_eq!(post.matches("orderedDitherColor(").count(), 1);
        let filter = include_str!("../../shader/slang/ordered_dither.slang");
        assert!(!filter.contains("HALFTONE"));
        assert!(!filter.contains("ORDERED_"));
        assert!(!filter.contains("pattern =="));
    }
}
