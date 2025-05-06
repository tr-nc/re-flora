use ash::{khr, vk};

use crate::vkn::{execute_one_time_command, Allocator, Buffer, BufferUsage, Device, VulkanContext};

/// Returns: (acceleration_structure_size, scratch_buf_size)
pub fn query_properties<'a>(
    acc_device: &khr::acceleration_structure::Device,
    geom: vk::AccelerationStructureGeometryKHR<'a>,
    max_primitive_counts: &[u32],
    acc_type: vk::AccelerationStructureTypeKHR,
    acc_flags: vk::BuildAccelerationStructureFlagsKHR,
    acc_mode: vk::BuildAccelerationStructureModeKHR,
    geom_count: u32,
) -> (u64, u64) {
    let build_info_for_query = vk::AccelerationStructureBuildGeometryInfoKHR {
        ty: acc_type,
        flags: acc_flags,
        mode: acc_mode,
        geometry_count: geom_count,
        p_geometries: &geom,
        ..Default::default()
    };
    let mut size_info_to_query = vk::AccelerationStructureBuildSizesInfoKHR::default();
    unsafe {
        acc_device.get_acceleration_structure_build_sizes(
            vk::AccelerationStructureBuildTypeKHR::DEVICE,
            &build_info_for_query,
            max_primitive_counts,
            &mut size_info_to_query,
        );
    };

    let acceleration_structure_size = size_info_to_query.acceleration_structure_size;
    // exactly one of update_scratch_size and build_scratch_size should be 0
    let scratch_buf_size = size_info_to_query
        .update_scratch_size
        .max(size_info_to_query.build_scratch_size);

    (acceleration_structure_size, scratch_buf_size)
}

pub fn create_acc(
    device: &Device,
    allocator: &Allocator,
    acc_device: &khr::acceleration_structure::Device,
    acceleration_structure_size: u64,
    acc_type: vk::AccelerationStructureTypeKHR,
) -> (vk::AccelerationStructureKHR, Buffer) {
    let buf_usage_flags = BufferUsage::from_flags(
        vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR
            | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
    );

    let acc_buf = Buffer::new_sized(
        device.clone(),
        allocator.clone(),
        buf_usage_flags,
        gpu_allocator::MemoryLocation::GpuOnly,
        acceleration_structure_size,
    );

    let acc_create_info = vk::AccelerationStructureCreateInfoKHR {
        ty: acc_type,
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

    return (blas, acc_buf);
}

pub fn build_acc(
    vulkan_ctx: &VulkanContext,
    allocator: Allocator,
    scratch_buf_size: u64,
    geom: vk::AccelerationStructureGeometryKHR,
    acc_device: &khr::acceleration_structure::Device,
    acc: vk::AccelerationStructureKHR,
    acc_type: vk::AccelerationStructureTypeKHR,
    acc_flags: vk::BuildAccelerationStructureFlagsKHR,
    acc_mode: vk::BuildAccelerationStructureModeKHR,
    primitive_count: u32,
    geom_count: u32,
) {
    let scratch_buf = make_scratch_buf(vulkan_ctx, allocator, scratch_buf_size);

    let build_info = vk::AccelerationStructureBuildGeometryInfoKHR {
        ty: acc_type,
        flags: acc_flags,
        mode: acc_mode,
        geometry_count: geom_count,
        p_geometries: &geom,
        dst_acceleration_structure: acc,
        scratch_data: vk::DeviceOrHostAddressKHR {
            device_address: scratch_buf.device_address(),
        },
        ..Default::default()
    };

    let range_info = vk::AccelerationStructureBuildRangeInfoKHR {
        primitive_count: primitive_count,
        ..Default::default()
    };

    execute_one_time_command(
        vulkan_ctx.device(),
        vulkan_ctx.command_pool(),
        &vulkan_ctx.get_general_queue(),
        |cmdbuf| unsafe {
            acc_device.cmd_build_acceleration_structures(
                cmdbuf.as_raw(),
                &[build_info],
                &[&[range_info]],
            );
        },
    );

    fn make_scratch_buf(
        vulkan_ctx: &VulkanContext,
        allocator: Allocator,
        scratch_buf_size: u64,
    ) -> Buffer {
        log::debug!("Scratch buffer size: {}", scratch_buf_size);
        Buffer::new_sized(
            vulkan_ctx.device().clone(),
            allocator.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS | vk::BufferUsageFlags::STORAGE_BUFFER,
            ),
            gpu_allocator::MemoryLocation::GpuOnly,
            scratch_buf_size,
        )
    }
}
