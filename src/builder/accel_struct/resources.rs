use ash::vk;

use crate::vkn::{AccelStruct, Allocator, Buffer, BufferUsage, ShaderModule, VulkanContext};

pub struct AccelStructResources {
    pub blas: Option<AccelStruct>,
    pub tlas: Option<AccelStruct>,

    pub make_unit_grass_info: Buffer,
    pub vertices: Buffer,
    pub indices: Buffer,
    pub blas_build_result: Buffer,

    pub instance_info: Buffer,
    pub instance_descriptor: Buffer,
    pub tlas_instances: Buffer,
    // pub instance_maker_indirect: Buffer,
}

impl AccelStructResources {
    pub fn new(
        vulkan_ctx: VulkanContext,
        allocator: Allocator,
        make_unit_grass_sm: &ShaderModule,
        instance_maker_sm: &ShaderModule,
        vertices_buffer_max_len: u64,
        indices_buffer_max_len: u64,
        tlas_instance_cap: u64,
    ) -> Self {
        let device = vulkan_ctx.device();

        let make_unit_grass_info_layout = make_unit_grass_sm
            .get_buffer_layout("U_MakeUnitGrassInfo")
            .unwrap();
        let make_unit_grass_info = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            make_unit_grass_info_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::CpuToGpu,
        );

        let vertices_layout = make_unit_grass_sm.get_buffer_layout("B_Vertices").unwrap();
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

        let indices_layout = make_unit_grass_sm.get_buffer_layout("B_Indices").unwrap();
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

        let blas_build_result_layout = make_unit_grass_sm
            .get_buffer_layout("B_BlasBuildResult")
            .unwrap();
        let blas_build_result = Buffer::from_buffer_layout(
            device.clone(),
            allocator.clone(),
            blas_build_result_layout.clone(),
            BufferUsage::empty(),
            gpu_allocator::MemoryLocation::GpuToCpu,
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
            gpu_allocator::MemoryLocation::CpuToGpu,
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
            gpu_allocator::MemoryLocation::CpuToGpu,
            tlas_instance_cap,
        );

        Self {
            blas: None,
            tlas: None,

            make_unit_grass_info,
            vertices,
            indices,
            blas_build_result,

            instance_info,
            instance_descriptor,
            tlas_instances,
        }
    }
}
