use crate::vkn::{
    rtx::acceleration_structure::utils::{build_acc, query_properties},
    Allocator, Buffer, BufferUsage, VulkanContext,
};
use ash::{khr, vk};

use super::utils::create_acc;

pub struct Tlas {
    vulkan_ctx: VulkanContext,
    allocator: Allocator,

    acc_device: khr::acceleration_structure::Device,

    tlas: Option<vk::AccelerationStructureKHR>,
    // must be kept alive until the TLAS is destroyed
    _buffer: Option<Buffer>,
}

impl Drop for Tlas {
    fn drop(&mut self) {
        if let Some(tlas) = self.tlas {
            unsafe {
                self.acc_device.destroy_acceleration_structure(tlas, None);
            }
        }
    }
}

impl Tlas {
    pub fn new(
        vulkan_ctx: VulkanContext,
        allocator: Allocator,
        acc_device: khr::acceleration_structure::Device,
    ) -> Self {
        Tlas {
            vulkan_ctx,
            allocator,
            acc_device,
            tlas: None,
            _buffer: None,
        }
    }

    pub fn build(&mut self, instances_buf: &Buffer, instance_count: u32) {
        let geom = make_tlas_geom(&instances_buf);

        // TODO: maybe reuse the scratch buffer / tlas handle later
        let (tlas_size, scratch_buf_size) = query_properties(
            &self.acc_device,
            geom,
            &[instance_count],
            vk::AccelerationStructureTypeKHR::TOP_LEVEL,
            vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE,
            vk::BuildAccelerationStructureModeKHR::BUILD,
            1, // one instance
        );

        let (tlas, buffer) = create_acc(
            self.vulkan_ctx.device(),
            &self.allocator,
            &self.acc_device,
            tlas_size,
            vk::AccelerationStructureTypeKHR::TOP_LEVEL,
        );

        build_acc(
            &self.vulkan_ctx,
            self.allocator.clone(),
            scratch_buf_size,
            geom,
            &self.acc_device,
            tlas,
            vk::AccelerationStructureTypeKHR::TOP_LEVEL,
            vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE,
            vk::BuildAccelerationStructureModeKHR::BUILD,
            instance_count,
            1, // one instance
        );

        self.tlas = Some(tlas);
        self._buffer = Some(buffer);

        fn make_tlas_geom<'a>(
            instance_buffer: &'a Buffer,
        ) -> vk::AccelerationStructureGeometryKHR<'a> {
            return vk::AccelerationStructureGeometryKHR {
                geometry_type: vk::GeometryTypeKHR::INSTANCES,
                flags: vk::GeometryFlagsKHR::OPAQUE,
                geometry: vk::AccelerationStructureGeometryDataKHR {
                    instances: vk::AccelerationStructureGeometryInstancesDataKHR {
                        array_of_pointers: vk::FALSE,
                        data: vk::DeviceOrHostAddressConstKHR {
                            device_address: instance_buffer.device_address(),
                        },
                        ..Default::default()
                    },
                },
                ..Default::default()
            };
        }
    }

    pub fn as_raw(&self) -> vk::AccelerationStructureKHR {
        assert!(self.tlas.is_some(), "TLAS not built yet");
        self.tlas.unwrap()
    }
}
