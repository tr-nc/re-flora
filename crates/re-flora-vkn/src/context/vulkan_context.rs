use crate::{CommandBuffer, CommandPool, Fence, Semaphore};

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
    // notice: the order matters
    fast_access_items: FastAccessItems,

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

struct FastAccessItems {
    command_pool: CommandPool,
}

impl FastAccessItems {
    pub fn new(device: &Device, queue_family_indices: &QueueFamilyIndices) -> Self {
        let command_pool = CommandPool::new(device, queue_family_indices.general);
        Self { command_pool }
    }
}

#[derive(Clone)]
pub struct VulkanContext(Arc<VulkanContextInner>);

impl VulkanContext {
    pub fn new(window: &Window, desc: VulkanContextDesc) -> Self {
        let entry = unsafe {
            Entry::load().unwrap_or_else(|err| panic!("failed to load Vulkan loader: {err}"))
        };

        let instance = Instance::new(&entry, window, &desc.name);
        let surface = Surface::new(&entry, &instance, window);
        let (physical_device, queue_family_indices) = PhysicalDevice::new(&instance, &surface);
        let device = Device::new(&instance, &physical_device, &queue_family_indices);

        let fast_access_items = FastAccessItems::new(&device, &queue_family_indices);

        Self(Arc::new(VulkanContextInner {
            fast_access_items,

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
                .wait_for_fences(fences, true, u64::MAX)
        }
    }

    pub fn submit_render_commands(
        &self,
        command_buffer: &CommandBuffer,
        image_available: &Semaphore,
        render_finished: &Semaphore,
        fence: &Fence,
    ) -> VkResult<()> {
        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let wait_semaphores = [image_available.as_raw()];
        let signal_semaphores = [render_finished.as_raw()];
        let command_buffers = [command_buffer.as_raw()];
        let submit_info = [vk::SubmitInfo::default()
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&wait_stages)
            .command_buffers(&command_buffers)
            .signal_semaphores(&signal_semaphores)];

        unsafe {
            self.device().as_raw().queue_submit(
                self.get_general_queue().as_raw(),
                &submit_info,
                fence.as_raw(),
            )
        }
    }

    pub fn get_general_queue(&self) -> Queue {
        self.device().get_queue(self.0.queue_family_indices.general)
    }

    /// Obtains the transfer-only queue from the device
    #[allow(dead_code)]
    pub fn get_transfer_only_queue(&self) -> vk::Queue {
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

    pub fn command_pool(&self) -> &CommandPool {
        &self.0.fast_access_items.command_pool
    }
}
