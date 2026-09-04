mod utils;

mod accel_struct;
pub use accel_struct::*;
use ash::{khr, vk};

use crate::{Allocator, Buffer, VulkanContext};

#[cfg(feature = "rtx-voxel-experiment")]
pub struct ProfiledAccelerationStructure {
    pub acceleration_structure: AccelStruct,
    pub acceleration_structure_bytes: u64,
    pub scratch_bytes: u64,
    pub host_build_ms: f64,
    pub gpu_build_ms: f64,
}

#[cfg(feature = "rtx-voxel-experiment")]
pub fn build_aabb_blas_profiled(
    vulkan_ctx: &VulkanContext,
    allocator: Allocator,
    acc_device: khr::acceleration_structure::Device,
    aabbs: &Buffer,
    primitive_count: u32,
) -> ProfiledAccelerationStructure {
    assert!(primitive_count > 0, "an AABB BLAS must contain a primitive");
    let aabb_data = vk::AccelerationStructureGeometryAabbsDataKHR::default()
        .data(vk::DeviceOrHostAddressConstKHR {
            device_address: aabbs.device_address(),
        })
        .stride(std::mem::size_of::<vk::AabbPositionsKHR>() as u64);
    let geom = vk::AccelerationStructureGeometryKHR {
        geometry_type: vk::GeometryTypeKHR::AABBS,
        geometry: vk::AccelerationStructureGeometryDataKHR { aabbs: aabb_data },
        flags: vk::GeometryFlagsKHR::empty(),
        ..Default::default()
    };
    build_profiled(
        vulkan_ctx,
        allocator,
        acc_device,
        geom,
        primitive_count,
        vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
    )
}

/// PROTOTYPE/TRACER BULLET: build one static, opaque triangle geometry for peak traversal.
#[cfg(feature = "rtx-voxel-experiment")]
pub fn build_triangle_blas_profiled(
    vulkan_ctx: &VulkanContext,
    allocator: Allocator,
    acc_device: khr::acceleration_structure::Device,
    vertices: &Buffer,
    vertex_count: u32,
    indices: &Buffer,
    primitive_count: u32,
) -> ProfiledAccelerationStructure {
    assert!(vertex_count > 0, "a triangle BLAS must contain vertices");
    assert!(primitive_count > 0, "a triangle BLAS must contain primitives");
    let triangles = vk::AccelerationStructureGeometryTrianglesDataKHR::default()
        .vertex_format(vk::Format::R32G32B32_SFLOAT)
        .vertex_data(vk::DeviceOrHostAddressConstKHR {
            device_address: vertices.device_address(),
        })
        .vertex_stride(std::mem::size_of::<[f32; 3]>() as u64)
        .max_vertex(vertex_count - 1)
        .index_type(vk::IndexType::UINT32)
        .index_data(vk::DeviceOrHostAddressConstKHR {
            device_address: indices.device_address(),
        });
    let geom = vk::AccelerationStructureGeometryKHR {
        geometry_type: vk::GeometryTypeKHR::TRIANGLES,
        geometry: vk::AccelerationStructureGeometryDataKHR { triangles },
        flags: vk::GeometryFlagsKHR::OPAQUE,
        ..Default::default()
    };
    build_profiled(
        vulkan_ctx,
        allocator,
        acc_device,
        geom,
        primitive_count,
        vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
    )
}

#[cfg(feature = "rtx-voxel-experiment")]
pub fn build_tlas_profiled(
    vulkan_ctx: &VulkanContext,
    allocator: Allocator,
    acc_device: khr::acceleration_structure::Device,
    instances: &Buffer,
    instance_count: u32,
) -> ProfiledAccelerationStructure {
    assert!(instance_count > 0, "a TLAS must contain an instance");
    let instances_data = vk::AccelerationStructureGeometryInstancesDataKHR::default()
        .array_of_pointers(false)
        .data(vk::DeviceOrHostAddressConstKHR {
            device_address: instances.device_address(),
        });
    let geom = vk::AccelerationStructureGeometryKHR {
        geometry_type: vk::GeometryTypeKHR::INSTANCES,
        geometry: vk::AccelerationStructureGeometryDataKHR {
            instances: instances_data,
        },
        flags: vk::GeometryFlagsKHR::empty(),
        ..Default::default()
    };
    build_profiled(
        vulkan_ctx,
        allocator,
        acc_device,
        geom,
        instance_count,
        vk::AccelerationStructureTypeKHR::TOP_LEVEL,
    )
}

#[cfg(feature = "rtx-voxel-experiment")]
fn build_profiled(
    vulkan_ctx: &VulkanContext,
    allocator: Allocator,
    acc_device: khr::acceleration_structure::Device,
    geom: vk::AccelerationStructureGeometryKHR<'_>,
    primitive_count: u32,
    acceleration_structure_type: vk::AccelerationStructureTypeKHR,
) -> ProfiledAccelerationStructure {
    let flags = vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE;
    let (acceleration_structure_bytes, scratch_bytes) = utils::query_properties(
        &acc_device,
        geom,
        &[primitive_count],
        acceleration_structure_type,
        flags,
        vk::BuildAccelerationStructureModeKHR::BUILD,
        1,
    );
    let acceleration_structure = utils::create_acc(
        vulkan_ctx.device(),
        &allocator,
        acc_device.clone(),
        acceleration_structure_bytes,
        acceleration_structure_type,
    );
    let timing = utils::build_acc_profiled(
        vulkan_ctx,
        allocator,
        scratch_bytes,
        geom,
        &acc_device,
        &acceleration_structure,
        acceleration_structure_type,
        flags,
        primitive_count,
    );
    ProfiledAccelerationStructure {
        acceleration_structure,
        acceleration_structure_bytes,
        scratch_bytes,
        host_build_ms: timing.host_ms,
        gpu_build_ms: timing.gpu_ms,
    }
}

#[allow(dead_code)]
#[allow(clippy::too_many_arguments)]
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
        vulkan_ctx,
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

    #[allow(clippy::needless_return)]
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
    fn make_tlas_geom<'a>(
        instances: &'a Buffer,
        geom_flags: vk::GeometryFlagsKHR,
    ) -> vk::AccelerationStructureGeometryKHR<'a> {
        vk::AccelerationStructureGeometryKHR {
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
        }
    }

    let geom = make_tlas_geom(instances, geom_flags);

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
        allocator,
        acc_device.clone(),
        tlas_size,
        vk::AccelerationStructureTypeKHR::TOP_LEVEL,
    );

    utils::build_or_update_acc(
        vulkan_ctx,
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

    dst_tlas
}
