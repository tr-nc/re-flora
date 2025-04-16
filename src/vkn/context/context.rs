use super::{
    device::Device, instance::Instance, physical_device::PhysicalDevice, queue::QueueFamilyIndices,
    surface::Surface, Queue,
};
use ash::{prelude::VkResult, vk, Entry};
use std::sync::Arc;
use winit::window::Window;

pub struct VulkanContextDesc {
    pub name: String,
}

struct VulkanContextInner {
    device: Device,
    surface: Surface,
    instance: Instance,
    physical_device: PhysicalDevice,
    queue_family_indices: QueueFamilyIndices,
}

impl Drop for VulkanContextInner {
    fn drop(&mut self) {
        log::info!("Destroying Vulkan Context");
    }
}

#[derive(Clone)]
pub struct VulkanContext(Arc<VulkanContextInner>);

impl VulkanContext {
    pub fn new(window: &Window, desc: VulkanContextDesc) -> Self {
        let entry = Entry::linked();

        let instance = Instance::new(&entry, window, &desc.name);
        let surface = Surface::new(&entry, &instance, window);
        let (physical_device, queue_family_indices) = PhysicalDevice::new(&instance, &surface);
        let device = Device::new(&instance, &physical_device, &queue_family_indices);

        Self(Arc::new(VulkanContextInner {
            device,
            surface,
            instance,
            physical_device,
            queue_family_indices,
        }))
    }

    /// Wait for all fences without a timeout
    pub fn wait_for_fences(&self, fences: &[vk::Fence]) -> VkResult<()> {
        unsafe {
            self.0
                .device
                .as_raw()
                .wait_for_fences(fences, true, std::u64::MAX)
        }
    }

    pub fn get_general_queue(&self) -> Queue {
        self.device().get_queue(self.0.queue_family_indices.general)
    }

    /// Obtains the transfer-only queue from the device
    pub fn _get_transfer_only_queue(&self) -> vk::Queue {
        unsafe {
            self.0
                .device
                .as_raw()
                .get_device_queue(self.0.queue_family_indices.transfer_only, 0)
        }
    }

    /// Expose references to inner fields if needed
    pub fn device(&self) -> &Device {
        &self.0.device
    }

    pub fn surface(&self) -> &Surface {
        &self.0.surface
    }

    pub fn instance(&self) -> &Instance {
        &self.0.instance
    }

    pub fn physical_device(&self) -> &PhysicalDevice {
        &self.0.physical_device
    }

    pub fn queue_family_indices(&self) -> &QueueFamilyIndices {
        &self.0.queue_family_indices
    }
}
