use crate::MonitorScorePreference;
use std::sync::Arc;
use verdarium_vkn::Extent2D;
use winit::{
    dpi::{LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize},
    event_loop::ActiveEventLoop,
    monitor::MonitorHandle,
    window::{Fullscreen, Window},
};

#[cfg(target_os = "linux")]
use winit::platform::wayland::WindowExtWayland;

#[cfg(not(target_os = "macos"))]
use winit::window::CursorGrabMode;

/// Native macOS cursor grab using CoreGraphics + AppKit.
/// winit's CursorGrabMode::Confined is unsupported on macOS, and set_cursor_visible
/// can crash with SIGBUS. These native APIs are reliable.
#[cfg(target_os = "macos")]
mod macos_cursor {
    use core_graphics::display::CGAssociateMouseAndMouseCursorPosition;
    use objc2_app_kit::NSCursor;

    pub fn grab_and_hide() {
        unsafe {
            CGAssociateMouseAndMouseCursorPosition(0);
            NSCursor::hide();
        }
    }

    pub fn release_and_show() {
        unsafe {
            CGAssociateMouseAndMouseCursorPosition(1);
            NSCursor::unhide();
        }
    }
}

#[cfg(not(target_os = "macos"))]
#[allow(dead_code)]
mod macos_cursor {
    pub fn grab_and_hide() {}
    pub fn release_and_show() {}
}

fn monitor_score(size: PhysicalSize<u32>) -> u64 {
    size.width as u64 * size.height as u64
}

pub fn select_monitor_by_score(
    event_loop: &ActiveEventLoop,
    preference: MonitorScorePreference,
) -> Option<MonitorHandle> {
    struct ScoredMonitor {
        index: usize,
        handle: MonitorHandle,
        size: PhysicalSize<u32>,
        name: String,
        score: u64,
    }

    let mut monitors: Vec<ScoredMonitor> = event_loop
        .available_monitors()
        .enumerate()
        .map(|(index, handle)| {
            let size = handle.size();
            let name = handle.name().unwrap_or_else(|| "<unknown>".to_owned());
            let score = monitor_score(size);
            ScoredMonitor {
                index,
                handle,
                size,
                name,
                score,
            }
        })
        .collect();

    match preference {
        MonitorScorePreference::Highest => monitors.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| b.size.width.cmp(&a.size.width))
                .then_with(|| b.size.height.cmp(&a.size.height))
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.index.cmp(&b.index))
        }),
        MonitorScorePreference::Lowest => monitors.sort_by(|a, b| {
            a.score
                .cmp(&b.score)
                .then_with(|| a.size.width.cmp(&b.size.width))
                .then_with(|| a.size.height.cmp(&b.size.height))
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.index.cmp(&b.index))
        }),
    }

    let selected = monitors.into_iter().next()?;
    log::info!(
        "Selected borderless fullscreen monitor by {:?} score: '{}' {}x{} physical pixels (score {})",
        preference,
        selected.name,
        selected.size.width,
        selected.size.height,
        selected.score,
    );
    Some(selected.handle)
}

/// Defines the way a window
/// is displayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowMode {
    #[allow(dead_code)]
    Windowed(bool),
    #[allow(dead_code)]
    BorderlessFullscreen,
}

/// Describes the information needed for creating a window.
#[derive(Debug, Clone)]
pub struct WindowStateDesc {
    /// The requested logical width of the window's client area.
    /// May vary from the physical width due to different pixel density on different monitors.
    pub width: f32,

    /// The requested logical height of the window's client area.
    /// May vary from the physical height due to different pixel density on different monitors.
    pub height: f32,

    /// The position on the screen that the window will be centered at.
    /// If set to `None`, some platform-specific position will be chosen.
    pub position: Option<[f32; 2]>,

    /// Sets the title that displays on the window top bar, on the system task bar and other OS specific places.
    pub title: String,

    /// Sets whether the window is resizable.
    pub resizable: bool,

    /// Sets whether the window should have borders and bars.
    pub decorations: bool,

    /// Sets whether the cursor is visible when the window has focus.
    pub cursor_visible: bool,

    /// Sets whether the window locks the cursor inside its borders when the window has focus.
    pub cursor_locked: bool,

    /// Sets the WindowMode.
    pub window_mode: WindowMode,

    /// Selected monitor for borderless fullscreen. None lets the platform choose.
    pub fullscreen_monitor: Option<MonitorHandle>,

    /// Sets whether the background of the window should be transparent.
    pub transparent: bool,

    /// Sets whether the native window should be shown after creation.
    pub visible: bool,
}

impl Default for WindowStateDesc {
    fn default() -> Self {
        WindowStateDesc {
            title: "Default Window".to_string(),
            width: 1280.0,
            height: 720.0,
            position: None,
            resizable: true,
            decorations: true,
            cursor_locked: false,
            cursor_visible: true,
            window_mode: WindowMode::Windowed(false),
            fullscreen_monitor: None,
            transparent: false,
            visible: true,
        }
    }
}

/// winit::window::Window is lacking some state tracking, so we wrap it in this struct to keep track
pub struct WindowState {
    window: Arc<Window>,
    desc: WindowStateDesc,
    #[cfg(not(target_os = "macos"))]
    cursor_grab_pending: bool,
}

impl WindowState {
    pub fn new(event_loop: &winit::event_loop::ActiveEventLoop, desc: &WindowStateDesc) -> Self {
        // https://docs.rs/winit/latest/winit/window/struct.Window.html#method.default_attributes
        let mut winit_window_attributes = Window::default_attributes();

        winit_window_attributes = match desc.window_mode {
            WindowMode::BorderlessFullscreen => winit_window_attributes.with_fullscreen(Some(
                Fullscreen::Borderless(desc.fullscreen_monitor.clone()),
            )),
            WindowMode::Windowed(windowed) => {
                let WindowStateDesc {
                    width,
                    height,
                    position,
                    ..
                } = *desc;

                if let Some(position) = position {
                    winit_window_attributes = winit_window_attributes.with_position(
                        LogicalPosition::new(position[0] as f64, position[1] as f64),
                    );
                }
                winit_window_attributes =
                    winit_window_attributes.with_inner_size(LogicalSize::new(width, height));

                winit_window_attributes.with_maximized(windowed)
            }
        }
        // set window to be invisible first to avoid flickering during window creation
        .with_visible(false)
        .with_resizable(desc.resizable)
        .with_decorations(desc.decorations)
        .with_transparent(desc.transparent);

        let winit_window_attributes = winit_window_attributes.with_title(&desc.title);
        let window = event_loop.create_window(winit_window_attributes).unwrap();

        // Keep creation hidden to avoid flicker, then show only when requested.
        if desc.visible {
            window.set_visible(true);
        }
        #[cfg(target_os = "linux")]
        if !desc.visible && window.xdg_toplevel().is_some() {
            // Wayland cannot hide/unhide mapped windows and winit ignores
            // WindowAttributes::visible there. Request minimization as the
            // best available hidden-mode fallback while keeping the normal
            // Vulkan surface/swapchain path alive.
            window.set_minimized(true);
            log::info!(
                "Wayland does not support hidden windows; requested minimized hidden window"
            );
        }

        #[cfg(target_os = "macos")]
        let state = Self {
            window: Arc::new(window),
            desc: desc.clone(),
        };
        #[cfg(not(target_os = "macos"))]
        let mut state = Self {
            window: Arc::new(window),
            desc: desc.clone(),
            cursor_grab_pending: false,
        };
        // Apply initial cursor state
        #[cfg(target_os = "macos")]
        if desc.visible && desc.cursor_locked {
            macos_cursor::grab_and_hide();
        }

        #[cfg(not(target_os = "macos"))]
        if desc.visible {
            state.window.set_cursor_visible(desc.cursor_visible);
            state.apply_cursor_grab();
        }

        state
    }

    pub fn window(&self) -> Arc<Window> {
        self.window.clone()
    }

    #[allow(dead_code)]
    pub fn get_window_state_desc(&self) -> &WindowStateDesc {
        &self.desc
    }

    pub fn toggle_fullscreen(&mut self) {
        if self.desc.window_mode == WindowMode::BorderlessFullscreen {
            return;
        }
        if let WindowMode::Windowed(windowed) = &mut self.desc.window_mode {
            *windowed = !*windowed;
            self.window.set_maximized(*windowed);
        }
    }

    /// Toggles the cursor visibility, this is the only way to change the cursor visibility, do not change it directly, otherwise the internal state will be out of sync.
    #[allow(dead_code)]
    pub fn toggle_cursor_visibility(&mut self) {
        self.set_cursor_visibility(!self.is_cursor_visible());
    }

    pub fn is_cursor_visible(&self) -> bool {
        self.desc.cursor_visible
    }

    /// Sets the cursor visibility. On macOS this is handled by the grab/release
    /// via native NSCursor APIs; on other platforms it uses winit.
    pub fn set_cursor_visibility(&mut self, cursor_visible: bool) {
        self.desc.cursor_visible = cursor_visible;
        if !self.desc.visible {
            return;
        }
        // On macOS, visibility is managed by grab_and_hide / release_and_show.
        // On other platforms, fall back to winit.
        #[cfg(not(target_os = "macos"))]
        self.window.set_cursor_visible(cursor_visible);
    }

    /// Toggles the cursor grab, this is the only way to change the cursor grab, do not change it directly, otherwise the internal state will be out of sync.
    #[allow(dead_code)]
    pub fn toggle_cursor_grab(&mut self) {
        self.set_cursor_grab(!self.get_cursor_grab());
    }

    pub fn get_cursor_grab(&self) -> bool {
        self.desc.cursor_locked
    }

    /// Sets the cursor grab using native macOS APIs. On macOS, winit's
    /// `CursorGrabMode::Confined` is unsupported and `set_cursor_visible` can SIGBUS,
    /// so we bypass winit entirely.
    ///
    /// Idempotent: skips native calls if the state hasn't changed, preventing
    /// `NSCursor::hide/unhide` reference count imbalance.
    pub fn set_cursor_grab(&mut self, cursor_locked: bool) {
        if self.desc.cursor_locked == cursor_locked {
            return;
        }
        self.desc.cursor_locked = cursor_locked;
        self.desc.cursor_visible = !cursor_locked;

        if !self.desc.visible {
            #[cfg(not(target_os = "macos"))]
            {
                self.cursor_grab_pending = false;
            }
            return;
        }

        #[cfg(target_os = "macos")]
        if cursor_locked {
            macos_cursor::grab_and_hide();
        } else {
            macos_cursor::release_and_show();
        }

        #[cfg(not(target_os = "macos"))]
        {
            self.window.set_cursor_visible(self.desc.cursor_visible);
            if !cursor_locked {
                self.cursor_grab_pending = false;
            }
            self.apply_cursor_grab();
        }
    }

    /// No-op on macOS (native APIs handle grab state persistently).
    /// Retained for API compatibility.
    pub fn maintain_cursor_grab(&mut self) {
        if !self.desc.visible {
            return;
        }
        #[cfg(not(target_os = "macos"))]
        if self.cursor_grab_pending {
            self.apply_cursor_grab();
        }
    }

    /// Size of the physical window, in (width, height).
    pub fn window_extent(&self) -> Extent2D {
        let size = self.window().inner_size();
        Extent2D::new(size.width, size.height)
    }

    pub fn center_cursor(&self) -> Option<(f32, f32)> {
        if !self.desc.visible {
            return None;
        }

        let extent = self.window_extent();
        let center = PhysicalPosition::new(extent.width as f64 * 0.5, extent.height as f64 * 0.5);
        match self.window.set_cursor_position(center) {
            Ok(()) => Some((center.x as f32, center.y as f32)),
            Err(err) => {
                log::warn!("Failed to center cursor: {:?}", err);
                None
            }
        }
    }

    pub fn is_minimized(&self) -> bool {
        self.window.is_minimized().unwrap_or(false)
    }

    /// Return scale factor accounted window size.
    #[allow(dead_code)]
    pub fn resolution(&self) -> [f32; 2] {
        let size = self.window_extent();
        let scale_factor = self.window().scale_factor();
        [
            (size.width as f64 / scale_factor) as f32,
            (size.height as f64 / scale_factor) as f32,
        ]
    }

    #[cfg(not(target_os = "macos"))]
    fn get_cursor_grab_mode(locked: bool) -> CursorGrabMode {
        if !locked {
            return CursorGrabMode::None;
        }
        CursorGrabMode::Confined
    }

    #[cfg(not(target_os = "macos"))]
    fn apply_cursor_grab(&mut self) {
        let mode = Self::get_cursor_grab_mode(self.desc.cursor_locked);
        match self.window.set_cursor_grab(mode) {
            Ok(_) => self.cursor_grab_pending = false,
            Err(e) => {
                if self.desc.cursor_locked {
                    if !self.cursor_grab_pending {
                        log::warn!("Failed to grab cursor (will retry): {:?}", e);
                    } else {
                        log::debug!("Retrying cursor grab failed: {:?}", e);
                    }
                    self.cursor_grab_pending = true;
                } else {
                    log::warn!("Failed to release cursor grab: {:?}", e);
                }
            }
        }
    }
}
