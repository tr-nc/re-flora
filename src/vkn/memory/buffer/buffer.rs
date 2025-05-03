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
    pub element_length: u64, // array of length elements
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
    pub fn from_buffer_layout(
        device: Device,
        allocator: Allocator,
        layout: BufferLayout,
        additional_usages: BufferUsage,
        location: MemoryLocation,
    ) -> Self {
        return Self::create_buffer_with_layout(
            device,
            allocator,
            layout,
            additional_usages,
            location,
            1,
        );
    }

    pub fn from_buffer_layout_arraylike(
        device: Device,
        allocator: Allocator,
        layout: BufferLayout,
        additional_usages: BufferUsage,
        location: MemoryLocation,
        element_length: u64,
    ) -> Self {
        return Self::create_buffer_with_layout(
            device,
            allocator,
            layout,
            additional_usages,
            location,
            element_length,
        );
    }

    pub fn device_address(&self) -> vk::DeviceAddress {
        let res;
        unsafe {
            res = self
                .device
                .get_buffer_device_address(&vk::BufferDeviceAddressInfo {
                    buffer: self.buffer,
                    ..Default::default()
                });
        }
        return res;
    }

    fn create_buffer_with_layout(
        device: Device,
        mut allocator: Allocator,
        layout: BufferLayout,
        additional_usages: BufferUsage,
        location: MemoryLocation,
        element_length: u64,
    ) -> Self {
        let mut usages = BufferUsage::from_reflect_descriptor_type(layout.descriptor_type);
        usages.union_with(&additional_usages);

        let buffer_info = vk::BufferCreateInfo::default()
            .size(layout.get_size_bytes() * element_length)
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
            element_length,
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

    // TODO: deprecate this one?
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
            element_length: 1, // TODO: or?
        };

        Self {
            device,
            allocator,
            buffer,
            allocated_mem,
            desc,
        }
    }

    pub fn get_element_size_bytes(&self) -> u64 {
        if let Some(size) = self.desc.size {
            return size;
        }

        if let Some(layout) = self.desc.layout.as_ref() {
            return layout.get_size_bytes();
        }

        unreachable!("Buffer has no layout or size set!");
    }

    pub fn get_size_bytes(&self) -> u64 {
        // allocated_mem.size() would give the wrong result because the allocated size
        // is implementation related, so it may overallocate
        self.get_element_size_bytes() * self.desc.element_length
    }

    /// Returns the buffer usage flags.
    pub fn get_usage(&self) -> BufferUsage {
        self.desc.usage
    }

    pub fn get_layout(&self) -> Option<&BufferLayout> {
        self.desc.layout.as_ref()
    }

    /// Returns the memory location of the buffer.
    pub fn _get_location(&self) -> MemoryLocation {
        self.desc._location
    }

    fn map_buffer_mem_and_write(&self, data: &[u8], byte_offset: u64) -> Result<(), String> {
        // Try to get the raw mapped pointer
        if let Some(ptr) = self.allocated_mem.mapped_ptr() {
            unsafe {
                let base_ptr = ptr.as_ptr();
                let target_ptr = base_ptr.add(byte_offset as usize);
                let mut align = ash::util::Align::new(
                    target_ptr,
                    std::mem::align_of::<u8>() as vk::DeviceSize, // u8 has alignment 1
                    data.len() as vk::DeviceSize,
                );
                align.copy_from_slice(data);
            }
            Ok(())
        } else {
            Err("Failed to map buffer memory".into())
        }
    }

    pub fn fill_element_with_raw_u8(&self, data: &[u8], element_idx: u64) -> Result<(), String> {
        if data.len() != self.get_element_size_bytes() as usize {
            return Err(format!(
                "Data size {} does not match element size {}",
                data.len(),
                self.get_element_size_bytes()
            ));
        }

        if element_idx >= self.desc.element_length {
            return Err(format!(
                "Element index {} out of bounds for element length {}",
                element_idx, self.desc.element_length
            ));
        }

        let offset = element_idx * self.get_element_size_bytes();
        self.map_buffer_mem_and_write(data, offset)
    }

    pub fn fill_with_raw_u8(&self, data: &[u8]) -> Result<(), String> {
        // validation: check if data size matches buffer size
        if data.len() != self.get_size_bytes() as usize {
            return Err(format!(
                "Data size {} does not match buffer size {}",
                data.len(),
                self.get_size_bytes()
            ));
        }
        self.map_buffer_mem_and_write(data, 0)
    }

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
            let size = self.get_size_bytes() as usize;
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
