use super::{
    device::Device, instance::Instance, physical_device::PhysicalDevice, queue::QueueFamilyIndices,
    surface::Surface,
};
use ash::{prelude::VkResult, vk, Entry};
use winit::window::Window;

pub struct VulkanContextDesc {
    pub name: String,
}

pub struct VulkanContext {
    pub device: Device,
    pub surface: Surface,
    pub instance: Instance,
    pub physical_device: PhysicalDevice,
    pub queue_family_indices: QueueFamilyIndices,
}

impl VulkanContext {
    pub fn new(window: &Window, desc: VulkanContextDesc) -> Self {
        let entry = Entry::linked();

        let instance = Instance::new(&entry, window, &desc.name);

        let surface = Surface::new(&entry, &instance, window);

        let (physical_device, queue_family_indices) = PhysicalDevice::new(&instance, &surface);

        let device = Device::new(&instance, &physical_device, &queue_family_indices);

        Self {
            instance,
            surface,
            physical_device,
            queue_family_indices,
            device,
        }
    }

    /// Wait for the device to become idle
    #[allow(dead_code)]
    pub fn wait_device_idle(&self) -> VkResult<()> {
        unsafe { self.device.as_raw().device_wait_idle() }
    }

    /// Wait for all fences without a timeout
    #[allow(dead_code)]
    pub fn wait_for_fences(&self, fences: &[vk::Fence]) -> VkResult<()> {
        unsafe {
            self.device
                .as_raw()
                .wait_for_fences(fences, true, std::u64::MAX)
        }
    }

    /// Obtains the general queue from the device
    #[allow(dead_code)]
    pub fn get_general_queue(&self) -> vk::Queue {
        unsafe {
            self.device
                .as_raw()
                .get_device_queue(self.queue_family_indices.general, 0)
        }
    }

    /// Obtains the transfer-only queue from the device
    #[allow(dead_code)]
    pub fn get_transfer_only_queue(&self) -> vk::Queue {
        unsafe {
            self.device
                .as_raw()
                .get_device_queue(self.queue_family_indices.transfer_only, 0)
        }
    }
}

impl Drop for VulkanContext {
    fn drop(&mut self) {
        log::info!("Destroying Vulkan Context");
    }
}
