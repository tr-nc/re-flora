use super::{Device, VulkanContext};
use ash::vk;
use gpu_allocator::vulkan::{
    Allocation, AllocationCreateDesc, Allocator as GpuAllocator, AllocatorCreateDesc,
};
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Clone)]
pub struct Allocator {
    device: Device,
    allocator: Arc<Mutex<GpuAllocator>>,
}

impl Allocator {
    pub fn new(device: &Device, allocator: Arc<Mutex<gpu_allocator::vulkan::Allocator>>) -> Self {
        Self {
            device: device.clone(),
            allocator,
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
        self.get_allocator()
            .allocate(create_info)
            .map_err(|e| e.to_string())
    }

    pub fn destroy_buffer(&mut self, buffer: vk::Buffer, allocation: Allocation) {
        let mut allocator = self.get_allocator();

        allocator
            .free(allocation)
            .expect("Failed to free buffer memory");
        unsafe { self.device.destroy_buffer(buffer, None) };
    }

    pub fn destroy_image(&mut self, image: vk::Image, allocation: Allocation) {
        let mut allocator = self.get_allocator();

        allocator
            .free(allocation)
            .expect("Failed to free image memory");
        unsafe { self.device.destroy_image(image, None) };
    }
}
