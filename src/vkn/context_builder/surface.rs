use ash::{khr::surface, vk::SurfaceKHR};
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};

pub fn create_surface(
    entry: &ash::Entry,
    instance: &ash::Instance,
    window: &winit::window::Window,
) -> (SurfaceKHR, surface::Instance) {
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
