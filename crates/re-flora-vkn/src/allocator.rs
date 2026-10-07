use super::{Device, VulkanContext};
use ash::vk;
use gpu_allocator::vulkan::{
    Allocation, AllocationCreateDesc, Allocator as GpuAllocator, AllocatorCreateDesc,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Default)]
struct DeviceLocalUsage(AtomicU64);

impl DeviceLocalUsage {
    fn add(&self, flags: vk::MemoryPropertyFlags, bytes: u64) {
        if flags.contains(vk::MemoryPropertyFlags::DEVICE_LOCAL) {
            self.0.fetch_add(bytes, Ordering::Relaxed);
        }
    }

    fn remove(&self, flags: vk::MemoryPropertyFlags, bytes: u64) {
        if flags.contains(vk::MemoryPropertyFlags::DEVICE_LOCAL) {
            self.0.fetch_sub(bytes, Ordering::Relaxed);
        }
    }
}

#[derive(Clone)]
pub struct Allocator {
    device: Device,
    allocator: Arc<Mutex<GpuAllocator>>,
    device_local_usage: Arc<DeviceLocalUsage>,
}

impl Allocator {
    pub fn new(device: &Device, allocator: Arc<Mutex<gpu_allocator::vulkan::Allocator>>) -> Self {
        Self {
            device: device.clone(),
            allocator,
            device_local_usage: Arc::default(),
        }
    }

    pub fn new_for_context(vulkan_ctx: &VulkanContext) -> Self {
        let device = vulkan_ctx.device();
        let allocator_create_info = AllocatorCreateDesc {
            instance: vulkan_ctx.instance().as_raw().clone(),
            device: device.as_raw().clone(),
            physical_device: vulkan_ctx.physical_device().as_raw(),
            debug_settings: Default::default(),
            buffer_device_address: true,
            allocation_sizes: Default::default(),
        };
        let gpu_allocator = GpuAllocator::new(&allocator_create_info)
            .expect("Failed to create gpu allocator");
        Self::new(device, Arc::new(Mutex::new(gpu_allocator)))
    }

    fn get_allocator(&self) -> MutexGuard<'_, GpuAllocator> {
        self.allocator.lock().unwrap()
    }

    pub fn allocate_memory(
        &mut self,
        create_info: &AllocationCreateDesc,
    ) -> Result<Allocation, String> {
        let allocation = self
            .get_allocator()
            .allocate(create_info)
            .map_err(|e| e.to_string())?;
        self.device_local_usage
            .add(allocation.memory_properties(), allocation.size());
        Ok(allocation)
    }

    /// Live resource bytes in DEVICE_LOCAL memory allocated through this wrapper
    /// and its clones. Not whole-GPU usage or physical residency; excludes pool
    /// slack, swapchain images and driver-owned memory. On UMA this is shared RAM.
    pub fn device_local_allocated_bytes(&self) -> u64 {
        self.device_local_usage.0.load(Ordering::Relaxed)
    }

    pub fn destroy_buffer(&mut self, buffer: vk::Buffer, allocation: Allocation) {
        let mut allocator = self.get_allocator();

        let flags = allocation.memory_properties();
        let bytes = allocation.size();
        allocator
            .free(allocation)
            .expect("Failed to free buffer memory");
        self.device_local_usage.remove(flags, bytes);
        unsafe { self.device.destroy_buffer(buffer, None) };
    }

    pub fn destroy_image(&mut self, image: vk::Image, allocation: Allocation) {
        let mut allocator = self.get_allocator();

        let flags = allocation.memory_properties();
        let bytes = allocation.size();
        allocator
            .free(allocation)
            .expect("Failed to free image memory");
        self.device_local_usage.remove(flags, bytes);
        unsafe { self.device.destroy_image(image, None) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_only_device_local_allocations_across_clones_and_frees() {
        let usage = Arc::new(DeviceLocalUsage::default());
        let clone = usage.clone();
        let local = vk::MemoryPropertyFlags::DEVICE_LOCAL;
        let host = vk::MemoryPropertyFlags::HOST_VISIBLE;
        usage.add(host, 4096);
        usage.add(local, 1024);
        clone.add(local | host, 2048);
        assert_eq!(usage.0.load(Ordering::Relaxed), 3072);
        clone.remove(local, 1024);
        usage.remove(host, 4096);
        assert_eq!(usage.0.load(Ordering::Relaxed), 2048);
        usage.remove(local | host, 2048);
        assert_eq!(usage.0.load(Ordering::Relaxed), 0);
    }
}
