use super::Device;
use ash::vk;
use gpu_allocator::{
    vulkan::{Allocation, AllocationCreateDesc, AllocationScheme, Allocator as GpuAllocator},
    MemoryLocation,
};
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

    fn get_allocator(&self) -> MutexGuard<GpuAllocator> {
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

    pub fn create_buffer(
        &mut self,
        size: usize,
        usage: vk::BufferUsageFlags,
    ) -> (vk::Buffer, Allocation) {
        let buffer_info = vk::BufferCreateInfo::default()
            .size(size as _)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe { self.device.create_buffer(&buffer_info, None).unwrap() };
        let requirements = unsafe { self.device.get_buffer_memory_requirements(buffer) };

        let mut allocator = self.get_allocator();

        let allocation = allocator
            .allocate(&AllocationCreateDesc {
                name: "",
                requirements,
                location: MemoryLocation::CpuToGpu,
                linear: true,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .expect("Failed to allocate buffer memory");

        unsafe {
            self.device
                .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
                .unwrap()
        };

        (buffer, allocation)
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

    pub fn update_buffer<T: Copy>(&mut self, memory: &mut Allocation, data: &[T]) {
        let size = std::mem::size_of_val(data) as _;
        unsafe {
            let data_ptr = memory.mapped_ptr().unwrap().as_ptr();
            let mut align = ash::util::Align::new(data_ptr, std::mem::align_of::<T>() as _, size);
            align.copy_from_slice(data);
        };
    }
}
