use crate::{Allocator, BufferLayout, CommandBuffer, Device, MemoryLocation};

use super::BufferUsage;
use anyhow::Result;
use ash::vk;
use core::slice;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc, AllocationScheme};
use std::fmt;
use std::ops::Deref;
use std::sync::Arc;

impl fmt::Debug for BufferDesc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BufferDesc")
            .field("layout", &self.layout)
            .field("size", &self.size)
            .field("element_length", &self.element_length)
            .field("usage", &self.usage)
            .field("location", &self._location)
            .finish()
    }
}

struct BufferDesc {
    pub layout: Option<BufferLayout>,
    pub size: Option<vk::DeviceSize>,
    pub element_length: u64, // array of length elements
    pub usage: BufferUsage,
    pub _location: MemoryLocation,
}

struct BufferInner {
    device: Device,
    allocator: Allocator,
    buffer: vk::Buffer,
    allocated_mem: Allocation,
    desc: BufferDesc,
}

/// Owned Vulkan buffer allocation.
///
/// Clones are residency leases for the same buffer and allocation. The Vulkan
/// buffer and its allocation are released only after the final lease drops.
#[derive(Clone)]
pub struct Buffer(Arc<BufferInner>);

impl fmt::Debug for Buffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Buffer")
            .field("buffer", &self.0.buffer)
            .field("size_bytes", &self.get_size_bytes())
            .field("element_size_bytes", &self.get_element_size_bytes())
            .field("desc", &self.0.desc)
            .finish()
    }
}

impl Drop for BufferInner {
    fn drop(&mut self) {
        let allocated_mem = std::mem::take(&mut self.allocated_mem);
        self.allocator.destroy_buffer(self.buffer, allocated_mem);
    }
}

impl Deref for Buffer {
    type Target = vk::Buffer;
    fn deref(&self) -> &Self::Target {
        &self.0.buffer
    }
}

impl Buffer {
    pub fn from_uniform_layout(device: Device, allocator: Allocator, layout: BufferLayout) -> Self {
        Self::from_buffer_layout(
            device,
            allocator,
            layout,
            BufferUsage::empty(),
            MemoryLocation::CpuToGpu,
        )
    }

    #[allow(dead_code)]
    pub fn new_uniform<T: bytemuck::Pod>(device: Device, allocator: Allocator) -> Self {
        Self::new_sized(
            device,
            allocator,
            BufferUsage::from_flags(vk::BufferUsageFlags::UNIFORM_BUFFER),
            MemoryLocation::CpuToGpu,
            std::mem::size_of::<T>() as u64,
        )
    }

    pub fn from_buffer_layout(
        device: Device,
        allocator: Allocator,
        layout: BufferLayout,
        additional_usages: BufferUsage,
        location: MemoryLocation,
    ) -> Self {
        Self::create_buffer_with_layout(device, allocator, layout, additional_usages, location, 1)
    }

    pub fn from_buffer_layout_arraylike(
        device: Device,
        allocator: Allocator,
        layout: BufferLayout,
        additional_usages: BufferUsage,
        location: MemoryLocation,
        element_length: u64,
    ) -> Self {
        Self::create_buffer_with_layout(
            device,
            allocator,
            layout,
            additional_usages,
            location,
            element_length,
        )
    }

    pub fn device_address(&self) -> vk::DeviceAddress {
        let res;
        unsafe {
            res = self
                .0
                .device
                .get_buffer_device_address(&vk::BufferDeviceAddressInfo {
                    buffer: self.0.buffer,
                    ..Default::default()
                });
        }
        res
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
                location: location.into(),
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

        Self(Arc::new(BufferInner {
            device,
            allocator,
            buffer,
            allocated_mem,
            desc,
        }))
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
                location: location.into(),
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

        Self(Arc::new(BufferInner {
            device,
            allocator,
            buffer,
            allocated_mem,
            desc,
        }))
    }

    pub fn get_element_size_bytes(&self) -> u64 {
        if let Some(size) = self.0.desc.size {
            return size;
        }

        if let Some(layout) = self.0.desc.layout.as_ref() {
            return layout.get_size_bytes();
        }

        unreachable!("Buffer has no layout or size set!");
    }

    pub fn get_size_bytes(&self) -> u64 {
        // allocated_mem.size() would give the wrong result because the allocated size
        // is implementation related, so it may overallocate
        self.get_element_size_bytes() * self.0.desc.element_length
    }

    /// Returns the buffer usage flags.
    pub fn get_usage(&self) -> BufferUsage {
        self.0.desc.usage
    }

    pub fn get_layout(&self) -> Option<&BufferLayout> {
        self.0.desc.layout.as_ref()
    }

    /// Returns the memory location of the buffer.
    #[allow(dead_code)]
    pub fn get_location(&self) -> MemoryLocation {
        self.0.desc._location
    }

    fn map_buffer_mem_and_write(&self, data: &[u8], byte_offset: u64) -> Result<()> {
        // try to get the raw mapped pointer
        if let Some(ptr) = self.0.allocated_mem.mapped_ptr() {
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
            Err(anyhow::anyhow!("Failed to map buffer memory"))
        }
    }

    #[allow(dead_code)]
    pub fn fill_element_with_raw_u8(&self, data: &[u8], element_idx: u64) -> Result<()> {
        if data.len() != self.get_element_size_bytes() as usize {
            return Err(anyhow::anyhow!(
                "Data size {} does not match element size {}",
                data.len(),
                self.get_element_size_bytes()
            ));
        }

        if element_idx >= self.0.desc.element_length {
            return Err(anyhow::anyhow!(
                "Element index {} out of bounds for element length {}",
                element_idx,
                self.0.desc.element_length
            ));
        }

        let offset = element_idx * self.get_element_size_bytes();
        self.map_buffer_mem_and_write(data, offset)
    }

    pub fn fill_with_raw_u8(&self, data: &[u8]) -> Result<()> {
        // validation: check if data size matches buffer size
        if data.len() != self.get_size_bytes() as usize {
            return Err(anyhow::anyhow!(
                "Data size {} does not match buffer size {}",
                data.len(),
                self.get_size_bytes()
            ));
        }
        self.map_buffer_mem_and_write(data, 0)
    }

    pub fn fill_range_with_raw_u8(&self, byte_offset: u64, data: &[u8]) -> Result<()> {
        let byte_count = data.len() as u64;
        let buffer_size = self.get_size_bytes();
        if byte_offset > buffer_size || byte_count > buffer_size - byte_offset {
            return Err(anyhow::anyhow!(
                "Write range [{}, {}) is outside buffer size {}",
                byte_offset,
                byte_offset.saturating_add(byte_count),
                buffer_size
            ));
        }
        self.map_buffer_mem_and_write(data, byte_offset)
    }

    #[allow(dead_code)]
    pub fn fill_with_raw_u32(&self, data: &[u32]) -> Result<()> {
        let data_u8: &[u8] = unsafe {
            std::slice::from_raw_parts(data.as_ptr() as *const u8, std::mem::size_of_val(data))
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
    pub fn fill<T: Copy>(&self, data: &[T]) -> Result<()> {
        if let Some(ptr) = self.0.allocated_mem.mapped_ptr() {
            let size_of_slice = std::mem::size_of_val(data) as vk::DeviceSize;
            unsafe {
                let mut align = ash::util::Align::new(
                    ptr.as_ptr(),
                    std::mem::align_of::<T>() as vk::DeviceSize,
                    size_of_slice as vk::DeviceSize,
                );
                align.copy_from_slice(data);
            };
            return Ok(());
        }
        Err(anyhow::anyhow!("Failed to map buffer memory"))
    }

    /// Fills the buffer with a single `Pod` value (uniform buffer convenience method).
    ///
    /// The value is serialized via `bytemuck::bytes_of` and written to offset 0.
    /// The buffer size must exactly match `size_of::<T>()`.
    pub fn fill_uniform<T: bytemuck::Pod>(&self, value: &T) -> Result<()> {
        self.fill_with_raw_u8(bytemuck::bytes_of(value))
    }

    /// Reads raw data from the buffer.
    ///
    /// # Returns
    /// * `Ok(Vec<u8>)` containing the buffer's data if successful
    /// * `Err` with a description if memory mapping failed
    pub fn read_back(&self) -> Result<Vec<u8>> {
        self.read_back_range(0, self.get_size_bytes())
    }

    /// Reads a byte range from the buffer.
    ///
    /// # Parameters
    /// * `byte_offset` - First byte to copy from the mapped buffer
    /// * `byte_count` - Number of bytes to copy
    ///
    /// # Returns
    /// * `Ok(Vec<u8>)` containing the requested byte range if successful
    /// * `Err` if the range is outside the buffer or memory mapping failed
    pub fn read_back_range(&self, byte_offset: u64, byte_count: u64) -> Result<Vec<u8>> {
        let buffer_size = self.get_size_bytes();
        if byte_offset > buffer_size || byte_count > buffer_size - byte_offset {
            return Err(anyhow::anyhow!(
                "Readback range [{}, {}) is outside buffer size {}",
                byte_offset,
                byte_offset.saturating_add(byte_count),
                buffer_size
            ));
        }

        if let Some(ptr) = self.0.allocated_mem.mapped_ptr() {
            let byte_offset = byte_offset as usize;
            let byte_count = byte_count as usize;
            let mut data: Vec<u8> = vec![0; byte_count];
            unsafe {
                let mapped_slice: &[u8] =
                    slice::from_raw_parts(ptr.as_ptr().cast::<u8>().add(byte_offset), byte_count);
                data.copy_from_slice(mapped_slice);
            }
            Ok(data)
        } else {
            Err(anyhow::anyhow!("Failed to map buffer memory"))
        }
    }

    #[allow(dead_code)]
    pub fn record_fill(&self, cmdbuf: &CommandBuffer, offset: u64, size: u64, value: u32) {
        unsafe {
            self.0
                .device
                .cmd_fill_buffer(cmdbuf.as_raw(), self.as_raw(), offset, size, value);
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
            self.0.device.cmd_copy_buffer(
                cmdbuf.as_raw(),
                self.as_raw(),
                dst_buffer.as_raw(),
                &[copy_region],
            );
        }
    }

    /// Returns the raw Vulkan buffer handle.
    pub fn as_raw(&self) -> vk::Buffer {
        self.0.buffer
    }
}

#[cfg(test)]
mod tests {
    use super::Buffer;
    use crate::Image;

    fn assert_clone<T: Clone>() {}

    #[test]
    fn owned_buffer_and_image_handles_are_leaseable() {
        assert_clone::<Buffer>();
        assert_clone::<Image>();
    }
}
