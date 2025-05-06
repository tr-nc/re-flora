use super::Queue;
use super::{instance::Instance, physical_device::PhysicalDevice, queue::QueueFamilyIndices};
use ash::vk;
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

    pub fn wait_queue_idle(&self, queue: &Queue) {
        unsafe { self.as_raw().queue_wait_idle(queue.as_raw()).unwrap() };
    }

    #[allow(unused)]
    pub fn wait_idle(&self) {
        unsafe { self.as_raw().device_wait_idle().unwrap() };
    }

    /// Get a queue from the device, only the first queue is returned in current implementation
    pub fn get_queue(&self, queue_family_index: u32) -> Queue {
        let queue = unsafe { self.as_raw().get_device_queue(queue_family_index, 0) };
        Queue::new(queue)
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
        vk::KHR_SWAPCHAIN_NAME.as_ptr(),
        vk::KHR_ACCELERATION_STRUCTURE_NAME.as_ptr(),
        vk::KHR_DEFERRED_HOST_OPERATIONS_NAME.as_ptr(), // must be coupled with ACCLERATION_STRUCTURE
        vk::KHR_RAY_QUERY_NAME.as_ptr(),
        // vk::KHR_RAY_TRACING_PIPELINE_NAME.as_ptr(),
        // vk::KHR_PIPELINE_LIBRARY_NAME.as_ptr(),
        // vk::KHR_BUFFER_DEVICE_ADDRESS_NAME.as_ptr(),
    ];

    let mut buffer_device_address_features = vk::PhysicalDeviceBufferDeviceAddressFeatures {
        buffer_device_address: vk::TRUE,
        ..Default::default()
    };
    let mut physical_device_acceleration_structure_features_khr =
        vk::PhysicalDeviceAccelerationStructureFeaturesKHR {
            acceleration_structure: vk::TRUE,
            ..Default::default()
        };
    let mut physical_device_ray_query_features_khr = vk::PhysicalDeviceRayQueryFeaturesKHR {
        ray_query: vk::TRUE,
        ..Default::default()
    };

    let device_create_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queue_create_infos)
        .enabled_extension_names(&device_extensions_ptrs)
        .push_next(&mut buffer_device_address_features)
        .push_next(&mut physical_device_acceleration_structure_features_khr)
        .push_next(&mut physical_device_ray_query_features_khr);

    let device = unsafe {
        instance
            .create_device(physical_device, &device_create_info, None)
            .expect("Failed to create logical device")
    };

    device
}
