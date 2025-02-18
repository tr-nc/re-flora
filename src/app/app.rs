use winit::{
    application::ApplicationHandler, event::WindowEvent, event_loop::ActiveEventLoop,
    window::WindowId,
};

use super::initialized_app::InitializedApp;

#[derive(Default)]
pub struct App {
    initialized: Option<InitializedApp>,
}

impl ApplicationHandler for App {
    // initialize resources here
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.initialized = Some(InitializedApp::new(event_loop));
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        match &mut self.initialized {
            Some(initialized) => {
                initialized.on_window_event(event_loop, id, event);
            }
            None => panic!("App is not initialized"),
        }
    }

    fn device_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        match &mut self.initialized {
            Some(initialized) => {
                initialized.on_device_event(event_loop, device_id, event);
            }
            None => panic!("App is not initialized"),
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        match &mut self.initialized {
            Some(initialized) => {
                initialized.on_about_to_wait(_event_loop);
            }
            None => panic!("App is not initialized"),
        }
    }
}
