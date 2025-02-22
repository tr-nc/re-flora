use ash::{ext::debug_utils, khr::surface, prelude::VkResult, vk, Device, Entry, Instance};
use winit::window::Window;

use super::context_builder;

pub struct ContextCreateInfo {
    pub name: String,
}

pub struct QueueFamilyIndices {
    /// Guaranteed to support GRAPHICS + PRESENT + COMPUTE + TRANSFER,
    /// and should be used for all main tasks
    pub general: u32,
    /// Exclusive to transfer operations, may be slower, but enables
    /// potential parallelism for background transfer operations
    pub transfer_only: u32,
}

impl QueueFamilyIndices {
    pub fn get_all_indices(&self) -> Vec<u32> {
        vec![self.general, self.transfer_only]
    }
}

pub struct VulkanContext {
    pub instance: Instance,
    pub physical_device: vk::PhysicalDevice,
    pub device: Device,
    pub command_pool: vk::CommandPool,
    pub surface: surface::Instance,
    pub surface_khr: vk::SurfaceKHR,
    debug_utils: debug_utils::Instance,
    debug_utils_messenger: vk::DebugUtilsMessengerEXT,
    queue_family_indices: QueueFamilyIndices,
}

impl VulkanContext {
    pub fn new(window: &Window, create_info: ContextCreateInfo) -> Self {
        let entry = Entry::linked();

        let (instance, debug_utils, debug_utils_messenger) =
            context_builder::instance::create_vulkan_instance(&entry, window, &create_info.name);

        let (surface_khr, surface) =
            context_builder::surface::create_surface(&entry, &instance, window);

        let (physical_device, queue_family_indices) =
            context_builder::physical_device::create_physical_device(
                &instance,
                &surface,
                surface_khr,
            );

        let device = context_builder::device::create_device(
            &instance,
            physical_device,
            &queue_family_indices,
        );

        let command_pool = context_builder::command_pool::create_command_pool(
            &device,
            queue_family_indices.general,
        );

        Self {
            instance,
            debug_utils,
            debug_utils_messenger,
            surface,
            surface_khr,
            physical_device,
            queue_family_indices,
            device,
            command_pool,
        }
    }

    /// Wait for the device to become idle
    pub fn wait_device_idle(&self) -> VkResult<()> {
        unsafe { self.device.device_wait_idle() }
    }

    /// Wait for all fences without timeout
    pub fn wait_for_fences(&self, fences: &[vk::Fence]) -> VkResult<()> {
        unsafe { self.device.wait_for_fences(fences, true, std::u64::MAX) }
    }

    /// Obtains the general queue from the device
    pub fn get_general_queue(&self) -> vk::Queue {
        unsafe {
            self.device
                .get_device_queue(self.queue_family_indices.general, 0)
        }
    }

    /// Obtains the transfer-only queue from the device
    pub fn get_transfer_only_queue(&self) -> vk::Queue {
        unsafe {
            self.device
                .get_device_queue(self.queue_family_indices.transfer_only, 0)
        }
    }
}

impl Drop for VulkanContext {
    fn drop(&mut self) {
        log::info!("Destroying Vulkan Context");
        unsafe {
            self.device.device_wait_idle().unwrap();
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_device(None);
            self.surface.destroy_surface(self.surface_khr, None);
            self.debug_utils
                .destroy_debug_utils_messenger(self.debug_utils_messenger, None);
            self.instance.destroy_instance(None);
        }
    }
}
