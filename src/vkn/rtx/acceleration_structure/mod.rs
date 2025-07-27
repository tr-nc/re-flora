mod utils;

mod accel_struct;
pub use accel_struct::*;
use ash::{khr, vk};

use crate::vkn::{Allocator, Buffer, VulkanContext};

#[allow(dead_code)]
pub fn build_or_update_blas(
    vulkan_ctx: &VulkanContext,
    allocator: Allocator,
    acc_device: khr::acceleration_structure::Device,
    vertices: &Buffer,
    indices: &Buffer,
    geom_flags: vk::GeometryFlagsKHR,
    vertices_count: u32,
    primitive_count: u32,
    previous_blas: &Option<AccelStruct>,
    is_dynamic: bool,
    is_building: bool,
) -> AccelStruct {
    if !is_building && previous_blas.is_none() {
        panic!("Cannot update BLAS without a previous one");
    }
    if is_building && previous_blas.is_some() {
        panic!("Cannot build BLAS with a previous one");
    }

    let geom = make_geometry(
        vertices,
        indices,
        get_vertex_stride(vertices),
        vertices_count,
        geom_flags,
    );

    let dev = vulkan_ctx.device();
    let acc_flags = if is_dynamic {
        vk::BuildAccelerationStructureFlagsKHR::ALLOW_UPDATE
    } else {
        vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE
    };
    let mode = if is_building {
        vk::BuildAccelerationStructureModeKHR::BUILD
    } else {
        vk::BuildAccelerationStructureModeKHR::UPDATE
    };

    // query sizes for BLAS + scratch
    let (as_size, scratch_size) = utils::query_properties(
        &acc_device,
        geom,
        &[primitive_count],
        vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
        acc_flags,
        mode,
        1,
    );

    // allocate destination AS (new or update)
    let new_blas = utils::create_acc(
        dev,
        &allocator,
        acc_device.clone(),
        as_size,
        vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
    );

    // build or update
    utils::build_or_update_acc(
        &vulkan_ctx,
        allocator.clone(),
        scratch_size,
        geom,
        &acc_device,
        previous_blas,
        &new_blas,
        vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
        acc_flags,
        mode,
        primitive_count,
        1,
    );

    return new_blas;

    fn get_vertex_stride(vertices_buf: &Buffer) -> u64 {
        vertices_buf
            .get_layout()
            .unwrap()
            .root_member
            .get_size_bytes()
    }

    fn make_geometry<'a>(
        vertices: &'a Buffer,
        indices: &'a Buffer,
        vertex_stride: u64,
        max_vertex: u32,
        flags: vk::GeometryFlagsKHR,
    ) -> vk::AccelerationStructureGeometryKHR<'a> {
        let triangles = vk::AccelerationStructureGeometryTrianglesDataKHR {
            vertex_format: vk::Format::R32G32B32_SFLOAT,
            vertex_data: vk::DeviceOrHostAddressConstKHR {
                device_address: vertices.device_address(),
            },
            vertex_stride,
            max_vertex,
            index_type: vk::IndexType::UINT32,
            index_data: vk::DeviceOrHostAddressConstKHR {
                device_address: indices.device_address(),
            },
            transform_data: vk::DeviceOrHostAddressConstKHR { device_address: 0 },
            ..Default::default()
        };
        vk::AccelerationStructureGeometryKHR {
            geometry_type: vk::GeometryTypeKHR::TRIANGLES,
            geometry: vk::AccelerationStructureGeometryDataKHR { triangles },
            flags,
            ..Default::default()
        }
    }
}

#[allow(dead_code)]
pub fn build_tlas(
    vulkan_ctx: &VulkanContext,
    allocator: &Allocator,
    acc_device: khr::acceleration_structure::Device,
    instances: &Buffer,
    instance_count: u32,
    geom_flags: vk::GeometryFlagsKHR,
) -> AccelStruct {
    let geom = make_tlas_geom(&instances, geom_flags);

    // TODO: maybe reuse the scratch buffer / tlas handle later
    let (tlas_size, scratch_buf_size) = utils::query_properties(
        &acc_device,
        geom,
        &[instance_count],
        vk::AccelerationStructureTypeKHR::TOP_LEVEL,
        vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE,
        vk::BuildAccelerationStructureModeKHR::BUILD,
        1, // one instance
    );

    let dst_tlas = utils::create_acc(
        vulkan_ctx.device(),
        &allocator,
        acc_device.clone(),
        tlas_size,
        vk::AccelerationStructureTypeKHR::TOP_LEVEL,
    );

    utils::build_or_update_acc(
        &vulkan_ctx,
        allocator.clone(),
        scratch_buf_size,
        geom,
        &acc_device,
        &None,
        &dst_tlas,
        vk::AccelerationStructureTypeKHR::TOP_LEVEL,
        vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE,
        vk::BuildAccelerationStructureModeKHR::BUILD,
        instance_count,
        1, // one instance
    );

    return dst_tlas;

    fn make_tlas_geom<'a>(
        instances: &'a Buffer,
        geom_flags: vk::GeometryFlagsKHR,
    ) -> vk::AccelerationStructureGeometryKHR<'a> {
        return vk::AccelerationStructureGeometryKHR {
            geometry_type: vk::GeometryTypeKHR::INSTANCES,
            flags: geom_flags,
            geometry: vk::AccelerationStructureGeometryDataKHR {
                instances: vk::AccelerationStructureGeometryInstancesDataKHR {
                    array_of_pointers: vk::FALSE,
                    data: vk::DeviceOrHostAddressConstKHR {
                        device_address: instances.device_address(),
                    },
                    ..Default::default()
                },
            },
            ..Default::default()
        };
    }
}
