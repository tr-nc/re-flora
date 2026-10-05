//! Scene-only pixel grouping/sampling at the frame boundary, from saved controls.
use super::App;
use crate::tracer::scene_resolution::Settings;

pub(super) fn from_gui(gui: &crate::app::GuiAdjustables) -> Settings {
    Settings::from_controls(
        gui.scene_pixel_ratio.value,
        gui.scene_supersampling_enabled.value,
        gui.scene_supersampling_quality.value,
    )
}

impl App {
    pub(super) fn sync_scene_supersampling(&mut self) {
        let settings = from_gui(&self.debug_settings.adjustables);
        let previous = self.tracer.scene_pixel_settings();
        if settings == previous {
            return;
        }
        // No acquired frame exists here. Complete scene reads before changing
        // the sampling uniform/attachments, without touching the native swapchain.
        // An inactive/capped quality preference alone needs no resource rebuild.
        if settings.requires_resource_change(previous) {
            self.frame_manager.wait_for_all_submissions();
        }
        self.tracer.set_scene_pixel_settings(
            settings,
            self.contree_builder.get_resources(),
            self.scene_accel_builder.get_resources(),
            self.plain_builder.get_resources(),
        );
        let axis = settings.samples_per_axis();
        log::info!(
            "[SCENE_SUPERSAMPLING] enabled={} scene_samples={} frame_extent_generation={} requested_samples={} ratio={}:1 saved_control=true",
            axis > 1, axis * axis, self.tracer.frame_extent_generation().serial(),
            settings.requested_samples(), settings.pixel_stride() * settings.pixel_stride(),
        );
    }

    /// Opt-in native lifecycle fixture. Drives only the real saved controls in
    /// memory; exercises every ratio, both qualities, capping, and partial blocks.
    pub(super) fn advance_scene_supersampling_review(&mut self) -> bool {
        let Some(review) = self.scene_supersampling_review.as_mut() else {
            return false;
        };
        if self.loading_state.is_some() {
            return false;
        }
        let frame = review.frame;
        if frame == 54 {
            log::info!("[SCENE_SUPERSAMPLING_REVIEW] phase=complete frames=54 saved=false");
            return true;
        }
        review.frame += 1;
        let (ratio, enabled, quality) = match frame / 6 {
            0 => (3, false, 0),
            1 => (3, true, 0),
            2 => (3, true, 1),
            3 => (2, true, 1),
            4 => (1, true, 1),
            5 => (0, true, 1),
            6 => (3, true, 0),
            7 => (3, false, 0),
            _ => (3, true, 0),
        };
        let gui = &mut self.debug_settings.adjustables;
        gui.scene_pixel_ratio.value = ratio;
        gui.scene_supersampling_enabled.value = enabled;
        gui.scene_supersampling_quality.value = quality;
        if frame == 36 {
            gui.depth_outline_enabled.value = true;
        }
        let resize = match frame {
            18 => Some((1023, 767)),
            36 => Some((9, 8)),
            48 => Some((1280, 720)),
            _ => None,
        };
        if let Some((width, height)) = resize {
            let accepted = self
                .window_state
                .window()
                .request_inner_size(winit::dpi::PhysicalSize::new(width, height));
            if let Some(size) = accepted {
                self.queue_frame_extent(re_flora_vkn::Extent2D::new(size.width, size.height));
            }
            log::info!("[SCENE_SUPERSAMPLING_REVIEW] frame={frame} resize={width}x{height} accepted={accepted:?} saved=false");
        }
        if frame.is_multiple_of(6) {
            log::info!("[SCENE_SUPERSAMPLING_REVIEW] frame={frame} ratio_choice={ratio} enabled={enabled} quality_choice={quality} saved=false");
        }
        false
    }
}

pub(super) struct SceneSupersamplingReview {
    frame: u32,
}

impl SceneSupersamplingReview {
    pub(super) fn from_environment() -> Option<Self> {
        std::env::var_os("RE_FLORA_SCENE_SUPERSAMPLING_REVIEW").map(|_| Self { frame: 0 })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn scene_sampling_transition_waits_before_publishing_without_swapchain_resize() {
        let source = include_str!("scene_supersampling.rs");
        let sync = source
            .split("/// Opt-in native lifecycle fixture")
            .next()
            .unwrap();
        assert!(
            sync.find("wait_for_all_submissions()").unwrap()
                < sync.find("set_scene_pixel_settings(").unwrap()
        );
        assert!(!sync.contains("swapchain.on_resize"));
        assert!(!sync.contains("wait_idle()"));
        let app = include_str!("mod.rs");
        let redraw = app.split("WindowEvent::RedrawRequested =>").nth(1).unwrap();
        assert!(
            redraw.find("self.sync_scene_supersampling()").unwrap()
                < redraw.find("self.frame_manager.begin_frame(").unwrap()
        );
    }

    #[test]
    fn pixel_controls_resolve_without_mutating_the_saved_preferences() {
        let mut gui = crate::app::gui_config::DebugSettings::load().adjustables;
        gui.scene_pixel_ratio.value = 0;
        gui.scene_supersampling_enabled.value = true;
        gui.scene_supersampling_quality.value = 1;
        let settings = super::from_gui(&gui);
        assert_eq!(settings.pixel_stride(), 1);
        assert_eq!(settings.samples_per_axis(), 1);
        assert_eq!(settings.requested_samples(), 16);
        assert!(gui.scene_supersampling_enabled.value);
        assert_eq!(gui.scene_supersampling_quality.value, 1);
    }
}
