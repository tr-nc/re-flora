use super::core::launch_owners::prepare_startup_owners;
use super::core::App;
use crate::RunPlan;
use winit::{
    application::ApplicationHandler, event::WindowEvent, event_loop::ActiveEventLoop,
    window::WindowId,
};

pub struct AppController {
    plan: Option<RunPlan>,
    initialized: Option<App>,
}

impl AppController {
    pub fn new(plan: RunPlan) -> Self {
        Self {
            plan: Some(plan),
            initialized: None,
        }
    }
}

impl ApplicationHandler for AppController {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let RunPlan {
            platform,
            audio,
            world,
            automation,
            scenario,
        } = self.plan.take().expect("launch plan is consumed once");
        let owners = prepare_startup_owners(automation, scenario)
            .unwrap_or_else(|error| panic!("invalid launch ownership: {error}"));
        self.initialized = Some(App::new(event_loop, platform, world, audio, owners).unwrap());
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
