use crate::vkn::{
    rtx::acceleration_structure::utils::{build_acc, create_acc, query_properties},
    Allocator, Buffer, VulkanContext,
};
use ash::{khr, vk};

pub struct Blas {
    vulkan_ctx: VulkanContext,
    allocator: Allocator,
    acc_device: khr::acceleration_structure::Device,

    blas: Option<vk::AccelerationStructureKHR>,
    // must be kept alive until the BLAS is destroyed
    _buffer: Option<Buffer>,
}

impl Drop for Blas {
    fn drop(&mut self) {
        if let Some(blas) = self.blas {
            unsafe {
                self.acc_device.destroy_acceleration_structure(blas, None);
            }
        }
    }
}

impl Blas {
    /// Create an empty BLAS wrapper. Call `build` to actually allocate & build it.
    pub fn new(
        vulkan_ctx: VulkanContext,
        allocator: Allocator,
        acc_device: khr::acceleration_structure::Device,
    ) -> Self {
        Blas {
            vulkan_ctx,
            allocator,
            acc_device,
            blas: None,
            _buffer: None,
        }
    }

    /// Build the bottom‐level acceleration structure from the given geometry.
    pub fn build(
        &mut self,
        vertices_buf: &Buffer,
        indices_buf: &Buffer,
        geom_flags: vk::GeometryFlagsKHR,
        primitive_count: u32,
        vertices_count: u32,
    ) {
        let geom = make_blas_geom(
            vertices_buf,
            indices_buf,
            get_vertex_stride(vertices_buf),
            vertices_count,
            geom_flags,
        );

        let device = self.vulkan_ctx.device();

        // Query the sizes we need for BLAS and scratch
        let (blas_size, scratch_buf_size) = query_properties(
            &self.acc_device,
            geom,
            &[primitive_count],
            vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
            vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE,
            vk::BuildAccelerationStructureModeKHR::BUILD,
            1, // one build
        );

        // Allocate the BLAS handle and its backing buffer
        let (blas, buffer) = create_acc(
            device,
            &self.allocator,
            &self.acc_device,
            blas_size,
            vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
        );

        // Record and submit the build commands
        build_acc(
            &self.vulkan_ctx,
            self.allocator.clone(),
            scratch_buf_size,
            geom,
            &self.acc_device,
            blas,
            vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
            vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE,
            vk::BuildAccelerationStructureModeKHR::BUILD,
            primitive_count,
            1,
        );

        self.blas = Some(blas);
        self._buffer = Some(buffer);

        fn get_vertex_stride(vertices_buf: &Buffer) -> u64 {
            let layout = &vertices_buf.get_layout().unwrap().root_member;
            layout.get_size_bytes()
        }

        fn make_blas_geom<'a>(
            vertices_buf: &'a Buffer,
            indices_buf: &'a Buffer,
            vertex_stride: u64,
            max_vertex: u32,
            geom_flags: vk::GeometryFlagsKHR,
        ) -> vk::AccelerationStructureGeometryKHR<'a> {
            let triangles_data = vk::AccelerationStructureGeometryTrianglesDataKHR {
                vertex_format: vk::Format::R32G32B32_SFLOAT,
                vertex_data: vk::DeviceOrHostAddressConstKHR {
                    device_address: vertices_buf.device_address(),
                },
                vertex_stride: vertex_stride, // the stride in bytes between each vertex
                max_vertex: max_vertex,       // the number of vertices in vertex_data minus one
                index_type: vk::IndexType::UINT32,
                index_data: vk::DeviceOrHostAddressConstKHR {
                    device_address: indices_buf.device_address(),
                },
                transform_data: vk::DeviceOrHostAddressConstKHR { device_address: 0 },
                ..Default::default()
            };

            return vk::AccelerationStructureGeometryKHR {
                geometry_type: vk::GeometryTypeKHR::TRIANGLES,
                geometry: vk::AccelerationStructureGeometryDataKHR {
                    triangles: triangles_data,
                },
                flags: geom_flags,
                ..Default::default()
            };
        }
    }

    /// Query the device address of the BLAS, if built.
    pub fn get_device_address(&self) -> Option<u64> {
        self.blas.map(|blas| unsafe {
            self.acc_device.get_acceleration_structure_device_address(
                &vk::AccelerationStructureDeviceAddressInfoKHR {
                    acceleration_structure: blas,
                    ..Default::default()
                },
            )
        })
    }
}
