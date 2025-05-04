use std::sync::Arc;

use winit::{
    dpi::{LogicalPosition, LogicalSize},
    window::{CursorGrabMode, Fullscreen, Window},
};

/// Defines the way a window
/// is displayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowMode {
    #[allow(dead_code)]
    Windowed,
    #[allow(dead_code)]
    BorderlessFullscreen,
}

/// Describes the information
/// needed for creating a
/// window.
#[derive(Debug, Clone)]
pub struct WindowStateDesc {
    /// The requested logical
    /// width of the window's
    /// client area.
    ///
    /// May vary from the
    /// physical width due to
    /// different pixel
    /// density on different
    /// monitors.
    pub width: f32,

    /// The requested logical
    /// height of the window's
    /// client area.
    ///
    /// May vary from the
    /// physical height due to
    /// different pixel
    /// density on different
    /// monitors.
    pub height: f32,

    /// The position on the
    /// screen that the window
    /// will be centered at.
    ///
    /// If set to `None`, some
    /// platform-specific
    /// position will be
    /// chosen.
    pub position: Option<[f32; 2]>,

    /// Sets the title that
    /// displays on the window
    /// top bar, on the system
    /// task bar and other OS
    /// specific places.
    pub title: String,

    /// Sets whether the
    /// window is resizable.
    pub resizable: bool,

    /// Sets whether the
    /// window should have
    /// borders and bars.
    pub decorations: bool,

    /// Sets whether the
    /// cursor is visible when
    /// the window has focus.
    pub cursor_visible: bool,

    /// Sets whether the
    /// window locks the
    /// cursor inside its
    /// borders when the
    /// window has focus.
    pub cursor_locked: bool,

    /// Sets the WindowMode.
    pub window_mode: WindowMode,

    /// Sets whether the
    /// background of the
    /// window should be
    /// transparent.
    pub transparent: bool,
}

impl Default for WindowStateDesc {
    fn default() -> Self {
        WindowStateDesc {
            title: "Default Window".to_string(),
            width: 128.0,
            height: 72.0,
            position: None,
            resizable: true,
            decorations: true,
            cursor_locked: false,
            cursor_visible: true,
            window_mode: WindowMode::Windowed,
            transparent: false,
        }
    }
}

/// winit::window::Window is
/// lacking some state
/// tracking, so we wrap it in
/// this struct to keep track
pub struct WindowState {
    window: Arc<Window>,

    desc: WindowStateDesc,
}

impl WindowState {
    pub fn new(event_loop: &winit::event_loop::ActiveEventLoop, desc: &WindowStateDesc) -> Self {
        // https://docs.rs/winit/latest/winit/window/struct.Window.html#method.default_attributes
        let mut winit_window_attributes = Window::default_attributes();

        winit_window_attributes = match desc.window_mode {
            WindowMode::BorderlessFullscreen => winit_window_attributes
                .with_fullscreen(Some(Fullscreen::Borderless(event_loop.primary_monitor()))),
            WindowMode::Windowed => {
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
                winit_window_attributes.with_inner_size(LogicalSize::new(width, height))
            }
        }
        // set window to be invisible first to avoid flickering during window creation
        .with_visible(false)
        .with_resizable(desc.resizable)
        .with_decorations(desc.decorations)
        .with_transparent(desc.transparent);

        let winit_window_attributes = winit_window_attributes.with_title(&desc.title);
        let window = event_loop.create_window(winit_window_attributes).unwrap();

        let res = window.set_cursor_grab(Self::get_cursor_grab_mode(desc.cursor_locked));
        if let Err(e) = res {
            eprintln!("Failed to grab cursor: {:?}", e);
        }

        window.set_cursor_visible(desc.cursor_visible);

        // set the window to visible
        // after it has been created
        window.set_visible(true);

        Self {
            window: Arc::new(window),
            desc: desc.clone(),
        }
    }

    pub fn window(&self) -> Arc<Window> {
        self.window.clone()
    }

    #[allow(dead_code)]
    pub fn get_window_state_desc(&self) -> &WindowStateDesc {
        &self.desc
    }

    /// Toggles the cursor
    /// visibility, this is
    /// the only way to change
    /// the cursor visibility,
    /// do not change it
    /// directly, otherwise
    /// the internal state
    /// will be out of sync.
    pub fn toggle_cursor_visibility(&mut self) {
        self.set_cursor_visibility(!self.is_cursor_visible());
    }

    pub fn is_cursor_visible(&self) -> bool {
        self.desc.cursor_visible
    }

    /// Sets the cursor
    /// visibility, this is
    /// the only way to change
    /// the cursor visibility,
    /// do not change it
    /// directly, otherwise
    /// the internal state
    /// will be out of sync.
    pub fn set_cursor_visibility(&mut self, cursor_visible: bool) {
        self.desc.cursor_visible = cursor_visible;
        self.window.set_cursor_visible(cursor_visible);
    }

    /// Toggles the cursor
    /// grab, this is the only
    /// way to change the
    /// cursor grab, do not
    /// change it directly,
    /// otherwise the internal
    /// state will be out of
    /// sync.
    pub fn toggle_cursor_grab(&mut self) {
        self.set_cursor_grab(!self.get_cursor_grab());
    }

    pub fn get_cursor_grab(&self) -> bool {
        self.desc.cursor_locked
    }

    /// Sets the cursor grab,
    /// this is the only way
    /// to change the cursor
    /// grab, do not change it
    /// directly, otherwise
    /// the internal state
    /// will be out of sync.
    pub fn set_cursor_grab(&mut self, cursor_locked: bool) {
        self.desc.cursor_locked = cursor_locked;
        let res = self
            .window
            .set_cursor_grab(Self::get_cursor_grab_mode(cursor_locked));
        if let Err(e) = res {
            eprintln!("Failed to grab cursor: {:?}", e);
        }
    }

    /// Size of the physical
    /// window, in (width,
    /// height).
    pub fn window_size(&self) -> [u32; 2] {
        let size = self.window().inner_size();
        [size.width, size.height]
    }

    pub fn is_minimized(&self) -> bool {
        self.window.is_minimized().unwrap()
    }

    /// Return scale factor
    /// accounted window size.
    #[allow(dead_code)]
    pub fn resolution(&self) -> [f32; 2] {
        let size = self.window_size();
        let scale_factor = self.window().scale_factor();
        [
            (size[0] as f64 / scale_factor) as f32,
            (size[1] as f64 / scale_factor) as f32,
        ]
    }

    /// Return aspect ratio of
    /// the window. (width /
    /// height)
    #[allow(dead_code)]
    pub fn aspect_ratio(&self) -> f32 {
        let dims = self.window_size();
        dims[0] as f32 / dims[1] as f32
    }

    /// Returns the cursor grab mode that should be used for the current platform.
    fn get_cursor_grab_mode(locked: bool) -> CursorGrabMode {
        if !locked {
            return CursorGrabMode::None;
        }
        return CursorGrabMode::Confined;
    }
}
