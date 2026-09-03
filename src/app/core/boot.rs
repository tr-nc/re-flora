use super::App;
use crate::window::{select_monitor_by_score, WindowMode, WindowState, WindowStateDesc};
use re_flora_vkn::{VulkanContext, VulkanContextDesc};
use winit::event_loop::ActiveEventLoop;

impl App {
    pub(super) fn create_window_state(
        event_loop: &ActiveEventLoop,
        options: &crate::DisplayPlan,
    ) -> WindowState {
        const WINDOW_TITLE_DEBUG: &str = "Re: Flora - debug build";
        const WINDOW_TITLE_RELEASE: &str = "Re: Flora - release build";
        let using_mode = if cfg!(debug_assertions) {
            WINDOW_TITLE_DEBUG
        } else {
            WINDOW_TITLE_RELEASE
        };
        let fullscreen_monitor = if options.windowed {
            None
        } else {
            select_monitor_by_score(event_loop, options.monitor_score)
        };
        let hidden_fullscreen_monitor_size = if options.hidden && !options.windowed {
            fullscreen_monitor.as_ref().map(|monitor| {
                let size = monitor.size();
                let scale_factor = monitor.scale_factor() as f32;
                (
                    size.width as f32 / scale_factor,
                    size.height as f32 / scale_factor,
                )
            })
        } else {
            None
        };

        let window_mode = if options.hidden || options.windowed {
            WindowMode::Windowed(false)
        } else {
            WindowMode::BorderlessFullscreen
        };
        let mut window_descriptor = WindowStateDesc {
            title: using_mode.to_owned(),
            window_mode,
            fullscreen_monitor,
            cursor_locked: false,
            cursor_visible: true,
            visible: !options.hidden,
            ..Default::default()
        };
        if let Some((width, height)) = hidden_fullscreen_monitor_size {
            window_descriptor.width = width;
            window_descriptor.height = height;
        } else if options.hidden && !options.windowed {
            log::warn!(
                "No scored monitor available for --hidden; falling back to default windowed extent"
            );
        }
        if options.hidden {
            log::info!(
                "Running with a hidden native window at {:.0}x{:.0} logical pixels; Vulkan surface/swapchain path is unchanged",
                window_descriptor.width,
                window_descriptor.height,
            );
        }
        let window_state = WindowState::new(event_loop, &window_descriptor);
        if options.hidden {
            let extent = window_state.window_extent();
            log::info!(
                "Hidden window render extent is {}x{} physical pixels",
                extent.width,
                extent.height,
            );
        }
        window_state
    }

    pub(super) fn create_vulkan_context(window_state: &WindowState) -> VulkanContext {
        VulkanContext::new(
            &window_state.window(),
            VulkanContextDesc {
                name: "Re: Flora".into(),
            },
        )
    }
}
