use std::{ffi::c_void, mem};

use ash::vk;

use crate::vkn::Device;

pub struct Blas {
    blas_handle: vk::AccelerationStructureKHR,
}

struct Vertex {
    pos: [f32; 3],
}

impl Blas {
    pub fn new(device: &Device) -> Self {
        let cube_verts: [Vertex; 8] = [
            Vertex {
                pos: [-1.0, -1.0, -1.0],
            },
            Vertex {
                pos: [1.0, -1.0, -1.0],
            },
            Vertex {
                pos: [1.0, 1.0, -1.0],
            },
            Vertex {
                pos: [-1.0, 1.0, -1.0],
            },
            Vertex {
                pos: [-1.0, -1.0, 1.0],
            },
            Vertex {
                pos: [1.0, -1.0, 1.0],
            },
            Vertex {
                pos: [1.0, 1.0, 1.0],
            },
            Vertex {
                pos: [-1.0, 1.0, 1.0],
            },
        ];

        // 12 triangles, 36 indices
        let cube_indices: [u32; 36] = [
            // -Z face
            0, 1, 2, 2, 3, 0, // +Z face
            4, 6, 5, 6, 4, 7, // -X face
            0, 3, 7, 7, 4, 0, // +X face
            1, 5, 6, 6, 2, 1, // -Y face
            0, 4, 5, 5, 1, 0, // +Y face
            3, 2, 6, 6, 7, 3,
        ];

        let triangles_data = vk::AccelerationStructureGeometryTrianglesDataKHR {
            vertex_format: vk::Format::R32G32B32_SFLOAT,
            vertex_data: vk::DeviceOrHostAddressConstKHR {
                host_address: cube_verts.as_ptr() as *const c_void,
            },
            vertex_stride: mem::size_of::<Vertex>() as vk::DeviceSize,
            max_vertex: cube_verts.len() as u32,
            index_type: vk::IndexType::UINT32,
            index_data: vk::DeviceOrHostAddressConstKHR {
                host_address: cube_indices.as_ptr() as *const c_void,
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

        let blas_handle = vk::AccelerationStructureKHR::null(); // Placeholder, replace with actual handle creation
        let scratch_memory_size: u64 = 1024 * 1024 * 1024; // Placeholder, replace with actual size calculation
        let mut scratch_space = vec![0u8; scratch_memory_size as usize]; // Placeholder, replace with actual buffer creation

        let build_info = vk::AccelerationStructureBuildGeometryInfoKHR {
            ty: vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
            flags: vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE
                | vk::BuildAccelerationStructureFlagsKHR::ALLOW_UPDATE,
            mode: vk::BuildAccelerationStructureModeKHR::BUILD,
            geometry_count: 1,
            p_geometries: &geom,
            dst_acceleration_structure: blas_handle,
            scratch_data: vk::DeviceOrHostAddressKHR {
                host_address: scratch_space.as_ptr() as *mut c_void,
            },
            ..Default::default()
        };
        let range_info = vk::AccelerationStructureBuildRangeInfoKHR {
            primitive_count: 12, // your 12 triangles
            ..Default::default()
        };

        // Finally call the *host* build entrypoint:
        unsafe {
            // device.build_acceleration_structures_khr(&build_info, &[&range_info]);
            // device.cmd_acc

        }

        return Self {
            //
            blas_handle,
        };
    }
}
