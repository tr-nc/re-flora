use super::{Allocator, Device};
use ash::vk;
use gpu_allocator::{
    vulkan::{Allocation, AllocationCreateDesc, AllocationScheme},
    MemoryLocation,
};
use std::ops::Deref;

pub struct Buffer {
    _device: Device,
    allocator: Allocator,
    buffer: vk::Buffer,
    memory: Allocation,
}

impl Drop for Buffer {
    fn drop(&mut self) {
        let allocation = std::mem::take(&mut self.memory);
        self.allocator.destroy_buffer(self.buffer, allocation);
    }
}

impl Deref for Buffer {
    type Target = vk::Buffer;
    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl Buffer {
    pub fn new_sized(
        device: &Device,
        allocator: &mut Allocator,
        usage: vk::BufferUsageFlags,
        buffer_size: usize,
    ) -> Self {
        let buffer_info = vk::BufferCreateInfo::default()
            .size(buffer_size as _)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe { device.create_buffer(&buffer_info, None).unwrap() };
        let requirements = unsafe { device.get_buffer_memory_requirements(buffer) };

        let memory = allocator
            .allocate_memory(&AllocationCreateDesc {
                name: "",
                requirements,
                location: MemoryLocation::CpuToGpu,
                linear: true,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .expect("Failed to allocate buffer memory");

        unsafe {
            device
                .bind_buffer_memory(buffer, memory.memory(), memory.offset())
                .unwrap()
        };

        Self {
            _device: device.clone(),
            allocator: allocator.clone(),
            buffer,
            memory,
        }
    }

    pub fn fill<T: Copy>(&mut self, data: &[T]) {
        let size = std::mem::size_of_val(data) as _;
        unsafe {
            let data_ptr = self.memory.mapped_ptr().unwrap().as_ptr();
            let mut align = ash::util::Align::new(data_ptr, std::mem::align_of::<T>() as _, size);
            align.copy_from_slice(data);
        };
    }

    pub fn as_raw(&self) -> vk::Buffer {
        self.buffer
    }
}
