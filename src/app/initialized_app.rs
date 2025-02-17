use crate::{
    vkn::context::{Context, ContextCreateInfo},
    window::{WindowDescriptor, WindowMode, WindowState},
};
use winit::{
    event::{ElementState, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::KeyCode,
    window::WindowId,
};

pub struct InitializedApp {
    window_state: WindowState,
    is_resize_pending: bool,
}

impl InitializedApp {
    pub fn new(_event_loop: &ActiveEventLoop) -> Self {
        let window_descriptor = WindowDescriptor {
            title: "Flora".to_owned(),
            window_mode: WindowMode::Windowed,
            // cursor_locked: true,
            // cursor_visible: false,
            ..Default::default()
        };
        let window_state = WindowState::new(_event_loop, &window_descriptor);

        Self {
            window_state,
            is_resize_pending: false,
        }
    }

    pub fn init(&mut self) {
        let context_create_info = ContextCreateInfo {
            name: "Flora".into(),
        };
        Context::new(&self.window_state.window(), context_create_info);
    }

    pub fn on_window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            // close the loop, therefore the window, when close button is clicked
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }

            // never happened and never tested, take caution
            WindowEvent::ScaleFactorChanged {
                scale_factor: _scale_factor,
                inner_size_writer: _inner_size_writer,
            } => {
                self.is_resize_pending = true;
            }

            // resize the window
            WindowEvent::Resized(_) => {
                self.is_resize_pending = true;
            }

            WindowEvent::KeyboardInput { event, .. } => {
                // close the loop when escape key is pressed
                if event.state == ElementState::Pressed && event.physical_key == KeyCode::Escape {
                    event_loop.exit();
                    return;
                }

                if event.state == ElementState::Pressed && event.physical_key == KeyCode::KeyE {
                    self.window_state.toggle_cursor_visibility();
                    self.window_state.toggle_cursor_grab();
                }

                if !self.window_state.is_cursor_visible() {
                    // self.camera.handle_keyboard(&event);
                }
            }

            // redraw the window
            WindowEvent::RedrawRequested => {
                // when the windiw is resized, redraw is called afterwards, so when the window is minimized, return
                if self.window_state.is_minimized() {
                    return;
                }

                // resize the window if needed
                if self.is_resize_pending {
                    self.resize();
                }
            }
            _ => (),
        }
    }

    pub fn on_device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        _event: winit::event::DeviceEvent,
    ) {
        // Handle device events here
    }

    pub fn on_about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Handle about to wait here
    }

    fn resize(&mut self) {
        // Resize the window here
    }
}
