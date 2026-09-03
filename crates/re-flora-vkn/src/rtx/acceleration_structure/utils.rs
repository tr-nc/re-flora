use ash::{khr, vk};

use crate::{
    execute_one_time_command, Allocator, Buffer, BufferUsage, Device, MemoryLocation,
    PipelineStage, TimestampQueryPool, VulkanContext,
};
use std::time::Instant;

use super::AccelStruct;

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
    acc_device: khr::acceleration_structure::Device,
    acceleration_structure_size: u64,
    acc_type: vk::AccelerationStructureTypeKHR,
) -> AccelStruct {
    let buf_usage_flags = BufferUsage::from_flags(
        vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR
            | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
    );

    let acc_buf = Buffer::new_sized(
        device.clone(),
        allocator.clone(),
        buf_usage_flags,
        MemoryLocation::GpuOnly,
        acceleration_structure_size,
    );

    let acc_create_info = vk::AccelerationStructureCreateInfoKHR {
        ty: acc_type,
        buffer: acc_buf.as_raw(),
        size: acceleration_structure_size,
        offset: 0,
        ..Default::default()
    };

    let accel_struct = unsafe {
        acc_device
            .create_acceleration_structure(&acc_create_info, None)
            .expect("Failed to create BLAS")
    };

    AccelStruct::new(acc_device, accel_struct, acc_buf)
}

#[allow(clippy::too_many_arguments)]
pub fn build_or_update_acc(
    vulkan_ctx: &VulkanContext,
    allocator: Allocator,
    scratch_buf_size: u64,
    geom: vk::AccelerationStructureGeometryKHR,
    acc_device: &khr::acceleration_structure::Device,
    src_accel_struct: &Option<AccelStruct>,
    dst_accel_struct: &AccelStruct,
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
        src_acceleration_structure: {
            if let Some(src) = src_accel_struct {
                src.as_raw()
            } else {
                vk::AccelerationStructureKHR::null()
            }
        },
        dst_acceleration_structure: dst_accel_struct.as_raw(),
        scratch_data: vk::DeviceOrHostAddressKHR {
            device_address: scratch_buf.device_address(),
        },
        ..Default::default()
    };

    let range_info = vk::AccelerationStructureBuildRangeInfoKHR {
        primitive_count,
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
            MemoryLocation::GpuOnly,
            scratch_buf_size,
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AccelerationStructureCommandTiming {
    pub host_ms: f64,
    pub gpu_ms: f64,
}

#[allow(clippy::too_many_arguments)]
pub fn build_acc_profiled(
    vulkan_ctx: &VulkanContext,
    allocator: Allocator,
    scratch_buf_size: u64,
    geom: vk::AccelerationStructureGeometryKHR,
    acc_device: &khr::acceleration_structure::Device,
    dst_accel_struct: &AccelStruct,
    acc_type: vk::AccelerationStructureTypeKHR,
    acc_flags: vk::BuildAccelerationStructureFlagsKHR,
    primitive_count: u32,
) -> AccelerationStructureCommandTiming {
    let scratch_alignment = acceleration_structure_scratch_alignment(vulkan_ctx);
    let scratch_buf = make_aligned_scratch_buf(
        vulkan_ctx,
        allocator,
        scratch_buf_size,
        scratch_alignment,
    );
    let scratch_address = align_up(scratch_buf.device_address(), scratch_alignment);
    let build_info = vk::AccelerationStructureBuildGeometryInfoKHR {
        ty: acc_type,
        flags: acc_flags,
        mode: vk::BuildAccelerationStructureModeKHR::BUILD,
        geometry_count: 1,
        p_geometries: &geom,
        dst_acceleration_structure: dst_accel_struct.as_raw(),
        scratch_data: vk::DeviceOrHostAddressKHR {
            device_address: scratch_address,
        },
        ..Default::default()
    };
    let range_info = vk::AccelerationStructureBuildRangeInfoKHR {
        primitive_count,
        ..Default::default()
    };
    let timestamps = TimestampQueryPool::maybe_new(vulkan_ctx, 2, "RTX_AS_BUILD")
        .expect("RTX experiment requires GPU timestamps");
    let started = Instant::now();
    execute_one_time_command(
        vulkan_ctx.device(),
        vulkan_ctx.command_pool(),
        &vulkan_ctx.get_general_queue(),
        |cmdbuf| unsafe {
            timestamps.record_reset(cmdbuf, 2);
            timestamps.record_timestamp(cmdbuf, PipelineStage::TOP_OF_PIPE, 0);
            acc_device.cmd_build_acceleration_structures(
                cmdbuf.as_raw(),
                &[build_info],
                &[&[range_info]],
            );
            timestamps.record_timestamp(cmdbuf, PipelineStage::BOTTOM_OF_PIPE, 1);
        },
    );
    let host_ms = started.elapsed().as_secs_f64() * 1_000.0;
    let ticks = timestamps
        .read_u64(2)
        .expect("RTX AS build timestamp query must complete after queue idle");
    let gpu_ms = ticks[1].saturating_sub(ticks[0]) as f64
        * timestamps.timestamp_period_ns() as f64
        / 1_000_000.0;
    AccelerationStructureCommandTiming { host_ms, gpu_ms }
}

fn acceleration_structure_scratch_alignment(vulkan_ctx: &VulkanContext) -> u64 {
    let mut as_properties = vk::PhysicalDeviceAccelerationStructurePropertiesKHR::default();
    let mut properties = vk::PhysicalDeviceProperties2::default().push_next(&mut as_properties);
    unsafe {
        vulkan_ctx.instance().as_raw().get_physical_device_properties2(
            vulkan_ctx.physical_device().as_raw(),
            &mut properties,
        );
    }
    u64::from(as_properties.min_acceleration_structure_scratch_offset_alignment.max(1))
}

fn make_aligned_scratch_buf(
    vulkan_ctx: &VulkanContext,
    allocator: Allocator,
    scratch_buf_size: u64,
    alignment: u64,
) -> Buffer {
    Buffer::new_sized(
        vulkan_ctx.device().clone(),
        allocator,
        BufferUsage::from_flags(
            vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS | vk::BufferUsageFlags::STORAGE_BUFFER,
        ),
        MemoryLocation::GpuOnly,
        scratch_buf_size + alignment - 1,
    )
}

fn align_up(value: u64, alignment: u64) -> u64 {
    value.div_ceil(alignment) * alignment
}
