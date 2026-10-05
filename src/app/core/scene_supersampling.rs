//! Scene-only sampling changes at the frame boundary, using the saved Debug flag.
use super::App;

impl App {
    pub(super) fn sync_scene_supersampling(&mut self) {
        let enabled = self
            .debug_settings
            .adjustables
            .scene_supersampling_enabled
            .value;
        if enabled == self.tracer.scene_supersampling_enabled() {
            return;
        }
        // No acquired frame exists here. Complete all scene reads before changing
        // the shared sampling uniform and publishing replacement attachments.
        // The swapchain, its presentation semaphores and native UI are untouched.
        self.frame_manager.wait_for_all_submissions();
        self.tracer.set_scene_supersampling(
            enabled,
            self.contree_builder.get_resources(),
            self.scene_accel_builder.get_resources(),
            self.plain_builder.get_resources(),
        );
        log::info!(
            "[SCENE_SUPERSAMPLING] enabled={} scene_samples={} frame_extent_generation={} saved_control=true",
            enabled,
            if enabled { 4 } else { 1 },
            self.tracer.frame_extent_generation().serial(),
        );
    }

    /// Opt-in native lifecycle fixture. Drives the real saved field without Save,
    /// exercises A/B/A and window resize, and exits after all submitted phases.
    pub(super) fn advance_scene_supersampling_review(&mut self) -> bool {
        let Some(review) = self.scene_supersampling_review.as_mut() else {
            return false;
        };
        if self.loading_state.is_some() {
            return false;
        }
        let frame = review.frame;
        if frame == 36 {
            log::info!("[SCENE_SUPERSAMPLING_REVIEW] phase=complete frames=36 saved=false");
            return true;
        }
        review.frame += 1;
        let enabled = (frame / 6) % 2 == 1;
        self.debug_settings
            .adjustables
            .scene_supersampling_enabled
            .value = enabled;
        let resize = match frame {
            18 => Some((1023, 767)),
            30 => Some((1280, 720)),
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
            log::info!("[SCENE_SUPERSAMPLING_REVIEW] frame={frame} enabled={enabled} saved=false");
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
                < sync.find("set_scene_supersampling(").unwrap()
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
}
