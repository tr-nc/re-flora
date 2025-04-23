use crate::vkn::{Allocator, BufferLayout, CommandBuffer, Device};

use super::BufferUsage;
use ash::vk;
use core::slice;
use gpu_allocator::{
    vulkan::{Allocation, AllocationCreateDesc, AllocationScheme},
    MemoryLocation,
};
use std::ops::Deref;

struct BufferDesc {
    pub layout: Option<BufferLayout>,
    pub size: Option<vk::DeviceSize>,
    pub usage: BufferUsage,
    pub _location: MemoryLocation,
}

pub struct Buffer {
    device: Device,
    allocator: Allocator,
    buffer: vk::Buffer,
    allocated_mem: Allocation,
    desc: BufferDesc,
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
    /// Creates a buffer from a provided struct layout.
    ///
    /// This constructor is useful when the buffer will store structured data described
    /// by a layout. The layout defines the size and memory requirements.
    ///
    /// # Parameters
    /// * `device` - The Vulkan device
    /// * `allocator` - Memory allocator for buffer allocation
    /// * `layout` - Structure layout describing the buffer contents
    /// * `additional_usages` - Any additional buffer usage flags beyond what's inferred from the layout
    /// * `location` - Memory location (device or host visible memory)
    pub fn from_buffer_layout(
        device: Device,
        mut allocator: Allocator,
        layout: BufferLayout,
        additional_usages: BufferUsage,
        location: MemoryLocation,
    ) -> Self {
        let mut usages = BufferUsage::from_reflect_descriptor_type(layout.descriptor_type);
        usages.union_with(&additional_usages);

        let buffer_info = vk::BufferCreateInfo::default()
            .size(layout.get_size() as _)
            .usage(usages.as_raw())
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe { device.create_buffer(&buffer_info, None).unwrap() };
        let requirements = unsafe { device.get_buffer_memory_requirements(buffer) };

        let allocated_mem = allocator
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
                .bind_buffer_memory(buffer, allocated_mem.memory(), allocated_mem.offset())
                .unwrap()
        };

        let desc = BufferDesc {
            usage: usages,
            _location: location,
            layout: Some(layout),
            size: None,
        };

        Self {
            device,
            allocator,
            buffer,
            allocated_mem,
            desc,
        }
    }

    /// Creates a buffer with a specific size.
    ///
    /// This constructor is useful when the buffer size is known but doesn't
    /// have a specific structure layout.
    ///
    /// # Parameters
    /// * `device` - The Vulkan device
    /// * `allocator` - Memory allocator for buffer allocation
    /// * `usage` - Buffer usage flags
    /// * `location` - Memory location (device or host visible memory)
    /// * `size` - The size of the buffer in bytes
    pub fn new_sized(
        device: Device,
        mut allocator: Allocator,
        usage: BufferUsage,
        location: MemoryLocation,
        size: u64,
    ) -> Self {
        let buffer_info = vk::BufferCreateInfo::default()
            .size(size as _)
            .usage(usage.as_raw())
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe { device.create_buffer(&buffer_info, None).unwrap() };
        let requirements = unsafe { device.get_buffer_memory_requirements(buffer) };

        let allocated_mem = allocator
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
                .bind_buffer_memory(buffer, allocated_mem.memory(), allocated_mem.offset())
                .unwrap()
        };

        let desc = BufferDesc {
            usage,
            _location: location,
            layout: None,
            size: Some(size as vk::DeviceSize),
        };

        Self {
            device,
            allocator,
            buffer,
            allocated_mem,
            desc,
        }
    }

    /// Returns the size of the buffer in bytes.
    ///
    /// If the buffer was created with a specific size, returns that size.
    /// Otherwise, returns the size from the buffer's layout.
    pub fn get_size(&self) -> vk::DeviceSize {
        // allocated_mem.size() would give the wrong result because the allocated size
        // is implementation related, so it may overallocate

        if let Some(size) = self.desc.size {
            return size;
        }

        self.desc
            .layout
            .as_ref()
            .expect("Size and Layout fields are both not set!")
            .get_size() as vk::DeviceSize
    }

    /// Returns the buffer usage flags.
    pub fn get_usage(&self) -> BufferUsage {
        self.desc.usage
    }

    pub fn get_buffer_layout(&self) -> Option<&BufferLayout> {
        self.desc.layout.as_ref()
    }

    /// Returns the memory location of the buffer.
    pub fn _get_location(&self) -> MemoryLocation {
        self.desc._location
    }

    /// Fills the buffer with raw u8 data.
    ///
    /// # Parameters
    /// * `data` - The u8 array to copy into the buffer
    ///
    /// # Returns
    /// * `Ok(())` if the operation was successful
    /// * `Err` with a description if the data size doesn't match the buffer size
    ///   or if memory mapping failed
    pub fn fill_with_raw_u8(&self, data: &[u8]) -> Result<(), String> {
        // validation: check if data size matches buffer size
        if data.len() != self.get_size() as usize {
            return Err(format!(
                "Data size {} does not match buffer size {}",
                data.len(),
                self.get_size()
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

    /// Fills the buffer with raw u32 data.
    ///
    /// # Parameters
    /// * `data` - The u32 array to copy into the buffer
    ///
    /// # Returns
    /// * `Ok(())` if the operation was successful
    /// * `Err` with a description if the data size doesn't match the buffer size
    ///  or if memory mapping failed
    #[allow(dead_code)]
    pub fn fill_with_raw_u32(&self, data: &[u32]) -> Result<(), String> {
        let data_u8: &[u8] = unsafe {
            std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * size_of::<u32>())
        };
        self.fill_with_raw_u8(data_u8)
    }

    /// Fills the buffer with generic typed data.
    ///
    /// # Type Parameters
    /// * `T` - The type of data to fill the buffer with (must implement Copy)
    ///
    /// # Parameters
    /// * `data` - Slice of data to copy into the buffer
    ///
    /// # Returns
    /// * `Ok(())` if the operation was successful
    /// * `Err` with a description if memory mapping failed
    pub fn fill<T: Copy>(&self, data: &[T]) -> Result<(), String> {
        if let Some(ptr) = self.allocated_mem.mapped_ptr() {
            let size_of_slice = std::mem::size_of_val(data) as vk::DeviceSize;
            unsafe {
                let mut align = ash::util::Align::new(
                    ptr.as_ptr(),
                    std::mem::align_of::<T>() as vk::DeviceSize,
                    size_of_slice as vk::DeviceSize,
                );
                align.copy_from_slice(data);
            };
            Ok(())
        } else {
            return Err("Failed to map buffer memory".to_string());
        }
    }

    /// Reads raw data from the buffer.
    ///
    /// # Returns
    /// * `Ok(Vec<u8>)` containing the buffer's data if successful
    /// * `Err` with a description if memory mapping failed
    pub fn fetch_raw(&self) -> Result<Vec<u8>, String> {
        if let Some(ptr) = self.allocated_mem.mapped_ptr() {
            let size = self.get_size() as usize;
            let mut data: Vec<u8> = vec![0; size];
            unsafe {
                let mapped_slice: &mut [u8] = slice::from_raw_parts_mut(ptr.as_ptr().cast(), size);
                data.copy_from_slice(mapped_slice);
            }
            Ok(data)
        } else {
            Err("Failed to map buffer memory".to_string())
        }
    }

    pub fn record_copy_to_buffer(
        &self,
        cmdbuf: &CommandBuffer,
        dst_buffer: &Buffer,
        size: u64,
        src_offset: u64,
        dst_offset: u64,
    ) {
        let copy_region = vk::BufferCopy::default()
            .src_offset(src_offset)
            .dst_offset(dst_offset)
            .size(size);

        unsafe {
            self.device.cmd_copy_buffer(
                cmdbuf.as_raw(),
                self.as_raw(),
                dst_buffer.as_raw(),
                &[copy_region],
            );
        }
    }

    /// Returns the raw Vulkan buffer handle.
    pub fn as_raw(&self) -> vk::Buffer {
        self.buffer
    }
}
