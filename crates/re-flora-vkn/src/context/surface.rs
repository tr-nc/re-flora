use ash::{khr::surface, vk};
use std::sync::Arc;
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};

use super::instance::Instance;

struct SurfaceInner {
    surface: surface::Instance,
    surface_khr: vk::SurfaceKHR,
}

impl Drop for SurfaceInner {
    fn drop(&mut self) {
        unsafe {
            self.surface.destroy_surface(self.surface_khr, None);
        }
    }
}

#[derive(Clone)]
pub struct Surface(Arc<SurfaceInner>);

impl std::ops::Deref for Surface {
    type Target = vk::SurfaceKHR;
    fn deref(&self) -> &Self::Target {
        &self.0.surface_khr
    }
}

impl Surface {
    pub fn new(entry: &ash::Entry, instance: &Instance, window: &winit::window::Window) -> Self {
        let (surface_khr, surface) = create_surface(entry, instance.as_raw(), window);
        Self(Arc::new(SurfaceInner {
            surface,
            surface_khr,
        }))
    }

    pub fn surface_instance(&self) -> &surface::Instance {
        &self.0.surface
    }

    pub fn surface_khr(&self) -> vk::SurfaceKHR {
        self.0.surface_khr
    }
}

pub fn create_surface(
    entry: &ash::Entry,
    instance: &ash::Instance,
    window: &winit::window::Window,
) -> (vk::SurfaceKHR, surface::Instance) {
    let surface = surface::Instance::new(entry, instance);
    let surface_khr = unsafe {
        ash_window::create_surface(
            entry,
            instance,
            window.display_handle().unwrap().as_raw(),
            window.window_handle().unwrap().as_raw(),
            None,
        )
        .expect("Failed to create surface")
    };
    (surface_khr, surface)
}
