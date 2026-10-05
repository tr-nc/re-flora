//! Integer screen-pixel grouping and independently requested supersampling.
//! The final grid is derived from the physical window. Partial edge blocks are
//! cropped, never stretched. Sampling is capped at the native-equivalent grid.
use re_flora_vkn::Extent2D;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PixelRatio {
    Native,
    Four,
    Sixteen,
    SixtyFour,
}

impl PixelRatio {
    pub fn stride(self) -> u32 {
        match self {
            Self::Native => 1,
            Self::Four => 2,
            Self::Sixteen => 4,
            Self::SixtyFour => 8,
        }
    }

    fn from_choice(choice: u32) -> Self {
        match choice {
            0 => Self::Native,
            1 => Self::Four,
            2 => Self::Sixteen,
            _ => Self::SixtyFour,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Settings {
    ratio: PixelRatio,
    requested_samples_per_axis: u32,
    resolve_mode: u32,
}

impl Settings {
    /// Normalize the saved controls once, without making the renderer GUI-aware.
    pub fn from_controls(ratio: u32, enabled: bool, quality: u32) -> Self {
        Self {
            ratio: PixelRatio::from_choice(ratio),
            resolve_mode: 0,
            requested_samples_per_axis: if !enabled {
                1
            } else if quality == 1 {
                4
            } else {
                2
            },
        }
    }

    pub fn with_resolve_mode(mut self, choice: u32) -> Self {
        self.resolve_mode = u32::from(choice == 1);
        self
    }

    pub fn resolve_mode(self) -> u32 {
        if self.samples_per_axis() > 1 {
            self.resolve_mode
        } else {
            0
        }
    }

    pub fn pixel_stride(self) -> u32 {
        self.ratio.stride()
    }

    pub fn requested_samples(self) -> u32 {
        self.requested_samples_per_axis * self.requested_samples_per_axis
    }

    pub fn samples_per_axis(self) -> u32 {
        // No enormous beyond-native allocation when native output and 16x AA
        // are selected together. Keep the requested preference for coarser grids.
        self.requested_samples_per_axis.min(self.ratio.stride())
    }

    pub fn requires_resource_change(self, previous: Self) -> bool {
        self.ratio != previous.ratio || self.samples_per_axis() != previous.samples_per_axis()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SceneResolution {
    pub settings: Settings,
    pub pixel_extent: Extent2D,
    pub render_extent: Extent2D,
    pub samples_per_axis: u32,
}

impl SceneResolution {
    pub fn new(screen: Extent2D, settings: Settings) -> Self {
        let stride = settings.ratio.stride();
        // Ceil division keeps the last, possibly partial, screen block visible.
        let pixel_extent = Extent2D::new(
            screen.width.div_ceil(stride).max(1),
            screen.height.div_ceil(stride).max(1),
        );
        let samples_per_axis = settings.samples_per_axis();
        let render_extent = Extent2D::new(
            pixel_extent.width * samples_per_axis,
            pixel_extent.height * samples_per_axis,
        );
        log::info!(
            "[SCENE_PIXELS] screen={}x{} scene={}x{} ratio={}:1 pixel_stride={} pixels={}x{} requested_samples={} samples_per_axis={} filter={} upscale=nearest ui=native",
            screen.width, screen.height, render_extent.width, render_extent.height,
            stride * stride, stride, pixel_extent.width, pixel_extent.height,
            settings.requested_samples_per_axis * settings.requested_samples_per_axis,
            samples_per_axis,
            if samples_per_axis == 1 { "point".to_owned() } else if settings.resolve_mode() == 1 { "contrast".to_owned() } else { format!("box{samples_per_axis}x{samples_per_axis}") },
        );
        Self {
            settings,
            pixel_extent,
            render_extent,
            samples_per_axis,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_count_screen_pixels_not_linear_dimensions() {
        for (choice, width, height) in [
            (0, 2560, 1440),
            (1, 1280, 720),
            (2, 640, 360),
            (3, 320, 180),
        ] {
            let plan = SceneResolution::new(
                Extent2D::new(2560, 1440),
                Settings::from_controls(choice, false, 0),
            );
            assert_eq!(plan.pixel_extent, Extent2D::new(width, height));
            assert_eq!(plan.render_extent, plan.pixel_extent);
        }
    }

    #[test]
    fn supersampling_preserves_the_display_grid_and_projection_aspect() {
        for screen in [
            Extent2D::new(2560, 1440),
            Extent2D::new(1023, 767),
            Extent2D::new(1, 1),
        ] {
            for ratio in 0..4 {
                let a = SceneResolution::new(screen, Settings::from_controls(ratio, false, 0));
                for quality in 0..2 {
                    let b =
                        SceneResolution::new(screen, Settings::from_controls(ratio, true, quality));
                    assert_eq!(a.pixel_extent, b.pixel_extent);
                    assert_eq!(
                        b.render_extent.width,
                        a.pixel_extent.width * b.samples_per_axis
                    );
                    assert_eq!(
                        b.render_extent.height,
                        a.pixel_extent.height * b.samples_per_axis
                    );
                    assert_eq!(
                        a.render_extent.get_aspect_ratio(),
                        b.render_extent.get_aspect_ratio()
                    );
                    // Padding is at most the remainder of one displayed block.
                    assert!(b.render_extent.width <= screen.width + b.settings.ratio.stride() - 1);
                    assert!(
                        b.render_extent.height <= screen.height + b.settings.ratio.stride() - 1
                    );
                }
            }
        }
    }

    #[test]
    fn partial_edge_blocks_are_kept_without_rounding_the_sampling_grid_twice() {
        let plan = SceneResolution::new(
            Extent2D::new(1023, 767),
            Settings::from_controls(3, true, 0),
        );
        assert_eq!(plan.pixel_extent, Extent2D::new(128, 96));
        assert_eq!(plan.render_extent, Extent2D::new(256, 192));
        let tiny = SceneResolution::new(Extent2D::new(1, 1), Settings::from_controls(3, false, 0));
        assert_eq!(tiny.pixel_extent, Extent2D::new(1, 1));
    }

    #[test]
    fn requested_quality_is_capped_without_losing_the_preference() {
        let native = Settings::from_controls(0, true, 1);
        let four = Settings::from_controls(1, true, 1);
        let sixteen = Settings::from_controls(2, true, 1);
        assert_eq!(native.samples_per_axis(), 1);
        assert_eq!(four.samples_per_axis(), 2);
        assert_eq!(sixteen.samples_per_axis(), 4);
        assert_eq!(native.requested_samples_per_axis, 4);
        assert_eq!(native.with_resolve_mode(1).resolve_mode(), 0);
        assert_eq!(sixteen.with_resolve_mode(1).resolve_mode(), 1);
        assert!(!sixteen
            .with_resolve_mode(1)
            .requires_resource_change(sixteen));
        assert!(!native.requires_resource_change(Settings::from_controls(0, false, 0)));
        assert!(sixteen.requires_resource_change(Settings::from_controls(2, true, 0)));
        assert_eq!(
            Settings::from_controls(u32::MAX, true, u32::MAX),
            Settings::from_controls(3, true, 0)
        );
    }
}
