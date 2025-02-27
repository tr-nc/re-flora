use super::{instance::Instance, physical_device::PhysicalDevice, queue::QueueFamilyIndices};
use ash::{khr::swapchain, vk};
use std::collections::HashSet;

use std::sync::Arc;

struct DeviceInner {
    device: ash::Device,
}

impl Drop for DeviceInner {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().unwrap();
            self.device.destroy_device(None);
        }
    }
}

#[derive(Clone)]
pub struct Device(Arc<DeviceInner>);

impl std::ops::Deref for Device {
    type Target = ash::Device;
    fn deref(&self) -> &Self::Target {
        &self.0.device
    }
}

impl Device {
    pub fn new(
        instance: &Instance,
        physical_device: &PhysicalDevice,
        queue_family_indices: &QueueFamilyIndices,
    ) -> Self {
        let device = create_device(
            instance.as_raw(),
            physical_device.as_raw(),
            queue_family_indices,
        );
        Self(Arc::new(DeviceInner { device }))
    }

    pub fn as_raw(&self) -> &ash::Device {
        &self.0.device
    }
}

fn create_device(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    queue_family_indices: &QueueFamilyIndices,
) -> ash::Device {
    let queue_priorities = [1.0f32];
    let queue_create_infos = {
        let mut indices = HashSet::new();
        for idx in queue_family_indices.get_all_indices() {
            indices.insert(idx);
        }
        indices
            .into_iter()
            .map(|index| {
                vk::DeviceQueueCreateInfo::default()
                    .queue_family_index(index)
                    .queue_priorities(&queue_priorities)
            })
            .collect::<Vec<_>>()
    };

    let device_extensions_ptrs = [
        swapchain::NAME.as_ptr(),
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        ash::khr::portability_subset::NAME.as_ptr(),
    ];

    let device_create_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queue_create_infos)
        .enabled_extension_names(&device_extensions_ptrs);

    let device = unsafe {
        instance
            .create_device(physical_device, &device_create_info, None)
            .expect("Failed to create logical device")
    };

    device
}
