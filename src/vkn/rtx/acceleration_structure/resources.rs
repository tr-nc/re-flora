use ash::vk;

use crate::vkn::{Allocator, Buffer, BufferUsage, Device, ShaderModule};

pub struct Resources {
    pub vertices: Buffer,
    pub indices: Buffer,
    pub tlas_instance_buffer: Buffer,
}

impl Resources {
    pub fn new(device: Device, allocator: Allocator, vert_maker_sm: &ShaderModule) -> Self {
        let vertices_layout = vert_maker_sm.get_buffer_layout("B_Vertices").unwrap();
        let vertices = Buffer::from_buffer_layout_arraylike(
            device.clone(),
            allocator.clone(),
            vertices_layout.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                    | vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR,
            ),
            gpu_allocator::MemoryLocation::GpuOnly,
            10000,
        );

        let indices_layout = vert_maker_sm.get_buffer_layout("B_Indices").unwrap();
        let indices = Buffer::from_buffer_layout_arraylike(
            device.clone(),
            allocator.clone(),
            indices_layout.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                    | vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR,
            ),
            gpu_allocator::MemoryLocation::GpuOnly,
            10000,
        );

        let instance_data_size = size_of::<vk::AccelerationStructureInstanceKHR>() as u64;
        log::debug!("Instance data size: {}", instance_data_size);
        let tlas_instance_buffer = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR
                    | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            ),
            gpu_allocator::MemoryLocation::CpuToGpu,
            instance_data_size,
        );

        log::debug!("TLAS instance buffer: {:?}", tlas_instance_buffer.as_raw());

        Self {
            vertices,
            indices,
            tlas_instance_buffer,
        }
    }
}
