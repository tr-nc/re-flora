use crate::{
    CommandBuffer, CommandPool, Fence, PipelineWaitStage, Semaphore, SubmitDesc, SubmitSignal,
    SubmitWait,
};

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

    // Keep the dynamically loaded Vulkan loader alive for every function
    // pointer stored in the instance/device dispatch tables.
    _entry: Entry,
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

#[cfg(target_os = "macos")]
fn packaged_root() -> Option<std::path::PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.to_path_buf()))
}

#[cfg(target_os = "macos")]
fn configure_packaged_vulkan_runtime() {
    if std::env::var_os("VK_ICD_FILENAMES").is_some()
        || std::env::var_os("VK_DRIVER_FILES").is_some()
    {
        return;
    }

    let Some(root) = packaged_root() else {
        return;
    };
    let icd_path = root.join("vulkan/icd.d/MoltenVK_icd.json");
    if icd_path.exists() {
        std::env::set_var("VK_ICD_FILENAMES", &icd_path);
        log::info!("Using packaged MoltenVK ICD: {}", icd_path.display());
    }
}

#[cfg(not(target_os = "macos"))]
fn configure_packaged_vulkan_runtime() {}

#[cfg(target_os = "macos")]
unsafe fn load_vulkan_entry() -> Result<Entry, ash::LoadingError> {
    if let Some(root) = packaged_root() {
        for candidate in [
            root.join("lib/libvulkan.1.dylib"),
            root.join("lib/libvulkan.dylib"),
        ] {
            if candidate.exists() {
                match Entry::load_from(candidate.as_os_str()) {
                    Ok(entry) => {
                        log::info!("Loaded packaged Vulkan loader: {}", candidate.display());
                        return Ok(entry);
                    }
                    Err(err) => {
                        log::warn!(
                            "Failed to load packaged Vulkan loader {}: {}",
                            candidate.display(),
                            err
                        );
                    }
                }
            }
        }
    }

    Entry::load()
}

#[cfg(not(target_os = "macos"))]
unsafe fn load_vulkan_entry() -> Result<Entry, ash::LoadingError> {
    Entry::load()
}

impl VulkanContext {
    pub fn new(window: &Window, desc: VulkanContextDesc) -> Self {
        configure_packaged_vulkan_runtime();
        let entry = unsafe {
            load_vulkan_entry()
                .unwrap_or_else(|err| panic!("failed to load Vulkan loader: {err}"))
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
            _entry: entry,
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
        let command_buffers = [command_buffer];
        let waits = [SubmitWait::new(
            "swapchain.image_available",
            image_available,
            PipelineWaitStage::ColorAttachmentOutput,
        )];
        let signals = [SubmitSignal::new("frame.render_finished", render_finished)];
        let desc = SubmitDesc::new(
            "main.render",
            &command_buffers,
            &waits,
            &signals,
            Some(fence),
        );
        self.device().submit_to_queue(&self.get_general_queue(), desc)
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
