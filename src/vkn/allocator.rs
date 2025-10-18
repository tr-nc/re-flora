use super::Device;
use ash::vk;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc, Allocator as GpuAllocator};
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Clone)]
pub struct Allocator {
    device: Device,
    pub allocator: Arc<Mutex<GpuAllocator>>,
}

impl Allocator {
    pub fn new(device: &Device, allocator: Arc<Mutex<gpu_allocator::vulkan::Allocator>>) -> Self {
        Self {
            device: device.clone(),
            allocator,
        }
    }

    fn get_allocator(&self) -> MutexGuard<'_, GpuAllocator> {
        self.allocator.lock().unwrap()
    }

    pub fn allocate_memory(
        &mut self,
        create_info: &AllocationCreateDesc,
    ) -> Result<Allocation, String> {
        self.get_allocator()
            .allocate(&create_info)
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
