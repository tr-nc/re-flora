use ash::{khr::surface, vk};
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};

use super::instance::Instance;

pub struct Surface {
    pub surface: surface::Instance,
    pub surface_khr: vk::SurfaceKHR,
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            self.surface.destroy_surface(self.surface_khr, None);
        }
    }
}

impl Surface {
    pub fn new(entry: &ash::Entry, instance: &Instance, window: &winit::window::Window) -> Self {
        let (surface_khr, surface) = create_surface(entry, instance.as_raw(), window);
        Self {
            surface,
            surface_khr,
        }
    }
}

pub fn create_surface(
    entry: &ash::Entry,
    instance: &ash::Instance,
    window: &winit::window::Window,
) -> (vk::SurfaceKHR, surface::Instance) {
    let surface = surface::Instance::new(&entry, &instance);
    let surface_khr = unsafe {
        ash_window::create_surface(
            &entry,
            &instance,
            window.display_handle().unwrap().as_raw(),
            window.window_handle().unwrap().as_raw(),
            None,
        )
        .expect("Failed to create surface")
    };
    (surface_khr, surface)
}
