//! Owns the coarse display grid independently of the scene's sampling density.
//! The baseline uses one sample per pixel; the A/B mode renders exactly twice
//! each grid dimension for a 2x2 box resolve. UI and the swapchain stay native.
use re_flora_vkn::Extent2D;

pub(crate) const SCALE: f32 = 0.125;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SceneResolution {
    pub pixel_extent: Extent2D,
    pub render_extent: Extent2D,
    pub samples_per_axis: u32,
}

impl SceneResolution {
    pub fn new(screen: Extent2D, scale: f32, supersampling: bool) -> Self {
        let pixel_extent = render_extent(screen, scale);
        let samples_per_axis = if supersampling { 2 } else { 1 };
        // Round the display grid first. Scaling the window directly by twice
        // `scale` would change the grid/aspect on odd window dimensions.
        let render_extent = Extent2D::new(
            pixel_extent.width * samples_per_axis,
            pixel_extent.height * samples_per_axis,
        );
        log::info!(
            "[SCENE_PIXELS] screen={}x{} scene={}x{} scale={} pixels={}x{} samples_per_axis={} filter={} upscale=nearest ui=native",
            screen.width,
            screen.height,
            render_extent.width,
            render_extent.height,
            scale,
            pixel_extent.width,
            pixel_extent.height,
            samples_per_axis,
            if supersampling { "box2x2" } else { "point" },
        );
        Self {
            pixel_extent,
            render_extent,
            samples_per_axis,
        }
    }

    pub fn supersampling_enabled(self) -> bool {
        self.samples_per_axis == 2
    }
}

fn render_extent(screen: Extent2D, scale: f32) -> Extent2D {
    Extent2D::new(
        ((screen.width as f32 * scale) as u32).max(1),
        ((screen.height as f32 * scale) as u32).max(1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_scene_has_one_sixteenth_the_previous_pixel_count() {
        let screen = Extent2D::new(2560, 1440);
        let previous = render_extent(screen, 0.5);
        let current = render_extent(screen, SCALE);
        assert_eq!(current, Extent2D::new(320, 180));
        assert_eq!(
            previous.width * previous.height,
            16 * current.width * current.height
        );
    }

    #[test]
    fn supersampling_preserves_the_display_grid_and_projection_aspect() {
        for screen in [
            Extent2D::new(2560, 1440),
            Extent2D::new(1023, 767),
            Extent2D::new(1, 1),
            Extent2D::new(0, 0),
        ] {
            let a = SceneResolution::new(screen, SCALE, false);
            let b = SceneResolution::new(screen, SCALE, true);
            assert_eq!(a.pixel_extent, b.pixel_extent);
            assert_eq!(a.render_extent, a.pixel_extent);
            assert_eq!(b.render_extent.width, a.render_extent.width * 2);
            assert_eq!(b.render_extent.height, a.render_extent.height * 2);
            assert_eq!(
                a.render_extent.get_aspect_ratio(),
                b.render_extent.get_aspect_ratio()
            );
            assert!(!a.supersampling_enabled());
            assert!(b.supersampling_enabled());
        }
        let odd = SceneResolution::new(Extent2D::new(1023, 767), SCALE, true);
        assert_eq!(odd.pixel_extent, Extent2D::new(127, 95));
        assert_eq!(odd.render_extent, Extent2D::new(254, 190));
    }

    #[test]
    fn tiny_and_odd_windows_keep_valid_render_targets() {
        assert_eq!(
            render_extent(Extent2D::new(1, 1), SCALE),
            Extent2D::new(1, 1)
        );
        assert_eq!(
            render_extent(Extent2D::new(0, 0), SCALE),
            Extent2D::new(1, 1)
        );
        assert_eq!(
            render_extent(Extent2D::new(1023, 767), SCALE),
            Extent2D::new(127, 95)
        );
    }
}
