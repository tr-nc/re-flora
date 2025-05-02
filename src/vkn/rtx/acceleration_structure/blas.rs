use super::resources::AccResources;
use crate::{
    util::ShaderCompiler,
    vkn::{execute_one_time_command, Allocator, Buffer, BufferUsage, ShaderModule, VulkanContext},
};
use ash::vk;

pub struct Blas {
    blas: vk::AccelerationStructureKHR,
    resources: AccResources,
}

impl Blas {
    pub fn new(
        context: &VulkanContext,
        allocator: Allocator,
        acc_device: &ash::khr::acceleration_structure::Device,
        shader_compiler: &ShaderCompiler,
    ) -> Self {
        let device = context.device();

        let vert_maker_sm = ShaderModule::from_glsl(
            device,
            shader_compiler,
            "shader/acc_struct/vert_maker.comp",
            "main",
        )
        .unwrap();

        let resources = AccResources::new(device.clone(), allocator.clone(), &vert_maker_sm);

        let triangles_data = vk::AccelerationStructureGeometryTrianglesDataKHR {
            vertex_format: vk::Format::R32G32B32_SFLOAT,
            vertex_data: vk::DeviceOrHostAddressConstKHR {
                device_address: resources.vertices.device_address(),
            },
            vertex_stride: 4 * 3, // TODO: or 4*4?
            max_vertex: 7,        // maxVertex is the number of vertices in vertexData minus one.
            index_type: vk::IndexType::UINT32,
            index_data: vk::DeviceOrHostAddressConstKHR {
                device_address: resources.indices.device_address(),
            },
            transform_data: vk::DeviceOrHostAddressConstKHR { device_address: 0 },
            ..Default::default()
        };

        let geom: vk::AccelerationStructureGeometryKHR = vk::AccelerationStructureGeometryKHR {
            geometry_type: vk::GeometryTypeKHR::TRIANGLES,
            geometry: vk::AccelerationStructureGeometryDataKHR {
                triangles: triangles_data,
            },
            flags: vk::GeometryFlagsKHR::OPAQUE,
            ..Default::default()
        };

        const PRIMITIVE_COUNT: u32 = 12;

        //

        let build_info_for_query = vk::AccelerationStructureBuildGeometryInfoKHR {
            ty: vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
            flags: vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE,
            mode: vk::BuildAccelerationStructureModeKHR::BUILD,
            geometry_count: 1,
            p_geometries: &geom,
            ..Default::default()
        };
        let mut size_info_to_query = vk::AccelerationStructureBuildSizesInfoKHR::default();
        unsafe {
            acc_device.get_acceleration_structure_build_sizes(
                vk::AccelerationStructureBuildTypeKHR::DEVICE,
                &build_info_for_query,
                &[PRIMITIVE_COUNT],
                &mut size_info_to_query,
            );
        };

        let acceleration_structure_size = size_info_to_query.acceleration_structure_size;
        // exactly one of update_scratch_size and build_scratch_size should be 0
        let scratch_buf_size = size_info_to_query
            .update_scratch_size
            .max(size_info_to_query.build_scratch_size);

        log::debug!(
            "acceleration_structure_size: {}",
            acceleration_structure_size
        );
        log::debug!("scratch_buf_size: {}", scratch_buf_size);

        let acc_buf = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR),
            gpu_allocator::MemoryLocation::GpuOnly,
            acceleration_structure_size,
        );

        let acc_create_info = vk::AccelerationStructureCreateInfoKHR {
            ty: vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
            buffer: acc_buf.as_raw(),
            size: acceleration_structure_size,
            offset: 0,
            ..Default::default()
        };

        let blas = unsafe {
            acc_device
                .create_acceleration_structure(&acc_create_info, None)
                .expect("Failed to create BLAS")
        };

        let scratch_buf = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS | vk::BufferUsageFlags::STORAGE_BUFFER,
            ),
            gpu_allocator::MemoryLocation::GpuOnly,
            scratch_buf_size,
        );

        let build_info = vk::AccelerationStructureBuildGeometryInfoKHR {
            ty: vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
            flags: vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE,
            mode: vk::BuildAccelerationStructureModeKHR::BUILD,
            geometry_count: 1,
            p_geometries: &geom,
            dst_acceleration_structure: blas,
            scratch_data: vk::DeviceOrHostAddressKHR {
                device_address: scratch_buf.device_address(),
            },
            ..Default::default()
        };

        let range_info = vk::AccelerationStructureBuildRangeInfoKHR {
            primitive_count: PRIMITIVE_COUNT,
            ..Default::default()
        };

        execute_one_time_command(
            &device,
            context.command_pool(),
            &context.get_general_queue(),
            |cmdbuf| {
                unsafe {
                    acc_device.cmd_build_acceleration_structures(
                        cmdbuf.as_raw(),
                        &[build_info],
                        &[&[range_info]],
                    );
                }
            },
        );

        return Self { blas, resources };
    }
}
