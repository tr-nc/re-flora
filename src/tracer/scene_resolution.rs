//! This branch stylizes the entire scene through one low-resolution target.
//! The previous normal scene used half the native width/height. One eighth of
//! native is one quarter of that previous width/height: one sixteenth as many
//! scene pixels. UI and the swapchain remain at their native dimensions.
use re_flora_vkn::Extent2D;

pub(crate) const SCALE: f32 = 0.125;

pub(super) fn render_extent(screen: Extent2D, scale: f32) -> Extent2D {
    let extent = Extent2D::new(
        ((screen.width as f32 * scale) as u32).max(1),
        ((screen.height as f32 * scale) as u32).max(1),
    );
    log::info!(
        "[SCENE_PIXELS] screen={}x{} scene={}x{} scale={} upscale=nearest ui=native",
        screen.width,
        screen.height,
        extent.width,
        extent.height,
        scale,
    );
    extent
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
