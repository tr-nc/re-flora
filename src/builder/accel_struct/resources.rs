use ash::vk;

use crate::vkn::{Allocator, Blas, Buffer, BufferUsage, ShaderModule, Tlas, VulkanContext};

pub struct AccelStructResources {
    pub blas: Blas,
    pub tlas: Tlas,

    pub vertices: Buffer,
    pub indices: Buffer,

    pub instance_info: Buffer,
    pub instance_descriptor: Buffer,
    pub tlas_instances: Buffer,
}

impl AccelStructResources {
    pub fn new(
        vulkan_ctx: VulkanContext,
        allocator: Allocator,
        unit_cube_maker_sm: &ShaderModule,
        instance_maker_sm: &ShaderModule,
        vertices_buffer_max_len: u64,
        indices_buffer_max_len: u64,
        tlas_instance_cap: u64,
    ) -> Self {
        let device = vulkan_ctx.device();

        let accel_struct_device = ash::khr::acceleration_structure::Device::new(
            &vulkan_ctx.instance(),
            &vulkan_ctx.device(),
        );

        let vertices_layout = unit_cube_maker_sm.get_buffer_layout("B_Vertices").unwrap();
        let vertices = Buffer::from_buffer_layout_arraylike(
            device.clone(),
            allocator.clone(),
            vertices_layout.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                    | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR,
            ),
            gpu_allocator::MemoryLocation::GpuOnly,
            vertices_buffer_max_len,
        );
        log::debug!("vertices buffer max len: {}", vertices_buffer_max_len);
        log::debug!("vertices buffer size: {}", vertices.get_size_bytes());

        let indices_layout = unit_cube_maker_sm.get_buffer_layout("B_Indices").unwrap();
        let indices = Buffer::from_buffer_layout_arraylike(
            device.clone(),
            allocator.clone(),
            indices_layout.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                    | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR,
            ),
            gpu_allocator::MemoryLocation::GpuOnly,
            indices_buffer_max_len,
        );
        log::debug!("indices buffer max len: {}", indices_buffer_max_len);
        log::debug!("indices buffer size: {}", indices.get_size_bytes());

        let blas = Blas::new(
            vulkan_ctx.clone(),
            allocator.clone(),
            accel_struct_device.clone(),
        );

        let tlas = Tlas::new(
            vulkan_ctx.clone(),
            allocator.clone(),
            accel_struct_device.clone(),
        );

        let instance_info_layout = instance_maker_sm
            .get_buffer_layout("U_InstanceInfo")
            .unwrap();
        let instance_info = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            instance_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let instance_descriptor_layout = instance_maker_sm
            .get_buffer_layout("B_InstanceDescriptor")
            .unwrap();
        let instance_descriptor = Buffer::from_buffer_layout_arraylike(
            device.clone(),
            allocator.clone(),
            instance_descriptor_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::GpuOnly,
            tlas_instance_cap,
        );

        let tlas_instances_layout = instance_maker_sm
            .get_buffer_layout("B_AccelStructInstances")
            .unwrap();
        let tlas_instances = Buffer::from_buffer_layout_arraylike(
            device.clone(),
            allocator.clone(),
            tlas_instances_layout.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                    | vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR,
            ),
            gpu_allocator::MemoryLocation::GpuOnly,
            tlas_instance_cap,
        );

        Self {
            vertices,
            indices,
            blas,
            tlas,
            instance_info,
            instance_descriptor,
            tlas_instances,
        }
    }
}
