use super::app::App;
use winit::{
    application::ApplicationHandler, event::WindowEvent, event_loop::ActiveEventLoop,
    window::WindowId,
};

#[derive(Default)]
pub struct AppController {
    initialized: Option<App>,
}

impl ApplicationHandler for AppController {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.initialized = Some(App::new(event_loop));
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if let Some(initialized) = &mut self.initialized {
            initialized.on_window_event(event_loop, id, event);
        } else {
            panic!("App is not initialized");
        }
    }

    fn device_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        if let Some(initialized) = &mut self.initialized {
            initialized.on_device_event(event_loop, device_id, event);
        } else {
            panic!("App is not initialized");
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(initialized) = &mut self.initialized {
            initialized.on_about_to_wait(_event_loop);
        } else {
            panic!("App is not initialized");
        }
    }
}
