use super::{Allocator, Device};
use ash::vk;
use core::slice;
use gpu_allocator::{
    vulkan::{Allocation, AllocationCreateDesc, AllocationScheme},
    MemoryLocation,
};
use std::ops::Deref;

pub struct Buffer {
    device: Device,
    allocator: Allocator,
    buffer: vk::Buffer,
    allocated_mem: Allocation,
    size: vk::DeviceSize,
}

impl Drop for Buffer {
    fn drop(&mut self) {
        let allocated_mem = std::mem::take(&mut self.allocated_mem);
        self.allocator.destroy_buffer(self.buffer, allocated_mem);
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
        device: Device,
        mut allocator: Allocator,
        usage: vk::BufferUsageFlags,
        location: MemoryLocation,
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
                location: location,
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
            device,
            allocator: allocator,
            buffer,
            allocated_mem: memory,
            size: buffer_size as _,
        }
    }

    pub fn get_size(&self) -> vk::DeviceSize {
        // this seems to give the wrong result!
        // self.allocated_mem.size()

        self.size
    }

    /// Fills the buffer with raw data. The data size must match with the buffer size.
    pub fn fill_raw(&self, data: &[u8]) -> Result<(), String> {
        // validation: check if data size matches buffer size
        if data.len() != self.size as usize {
            return Err(format!(
                "Data size {} does not match buffer size {}",
                data.len(),
                self.size
            ));
        }

        if let Some(ptr) = self.allocated_mem.mapped_ptr() {
            unsafe {
                let mut align = ash::util::Align::new(
                    ptr.as_ptr(),
                    std::mem::align_of::<u8>() as vk::DeviceSize,
                    data.len() as vk::DeviceSize,
                );
                align.copy_from_slice(data);
            };
            Ok(())
        } else {
            return Err("Failed to map buffer memory".to_string());
        }
    }

    pub fn fill<T: Copy>(&self, data: &[T]) -> Result<(), String> {
        if let Some(ptr) = self.allocated_mem.mapped_ptr() {
            let size = std::mem::size_of_val(data);
            unsafe {
                let mut align = ash::util::Align::new(
                    ptr.as_ptr(),
                    std::mem::align_of::<T>() as vk::DeviceSize,
                    size as vk::DeviceSize,
                );
                align.copy_from_slice(data);
            };
            Ok(())
        } else {
            return Err("Failed to map buffer memory".to_string());
        }
    }

    pub fn fetch_raw(&self) -> Result<Vec<u8>, String> {
        if let Some(ptr) = self.allocated_mem.mapped_ptr() {
            let size = self.size;
            let mut data: Vec<u8> = vec![0; size as usize];
            unsafe {
                let mapped_slice: &mut [u8] =
                    slice::from_raw_parts_mut(ptr.as_ptr().cast(), size as usize);
                data.copy_from_slice(mapped_slice);
            }
            Ok(data)
        } else {
            Err("Failed to map buffer memory".to_string())
        }
    }

    pub fn as_raw(&self) -> vk::Buffer {
        self.buffer
    }
}
