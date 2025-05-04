use crate::vkn::{
    rtx::acceleration_structure::utils::{build_acc, query_properties},
    Allocator, Buffer, VulkanContext,
};
use ash::{khr, vk};

use super::utils::create_acc;

pub struct Tlas {
    acc_device: khr::acceleration_structure::Device,

    tlas: vk::AccelerationStructureKHR,
    buffer: Buffer,
}

// TODO: refactor this after testing
impl Drop for Tlas {
    fn drop(&mut self) {
        unsafe {
            self.acc_device
                .destroy_acceleration_structure(self.tlas, None);
        }
    }
}

impl Tlas {
    /// Build a TLAS containing exactly one instance of `blas` at the origin.
    pub fn new(
        context: &VulkanContext,
        allocator: Allocator,
        acc_device: khr::acceleration_structure::Device,
        geom: vk::AccelerationStructureGeometryKHR,
    ) -> Self {
        const PRIMITIVE_COUNT: u32 = 12; // TODO: this should be read back later

        let (tlas_size, scratch_buf_size) = query_properties(
            &acc_device,
            geom,
            &[1], // one instance
            vk::AccelerationStructureTypeKHR::TOP_LEVEL,
            vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE,
            vk::BuildAccelerationStructureModeKHR::BUILD,
            1, // one instance
        );

        let (tlas, buffer) = create_acc(
            context.device(),
            &allocator,
            &acc_device,
            tlas_size,
            vk::AccelerationStructureTypeKHR::TOP_LEVEL,
        );

        // https://zhuanlan.zhihu.com/p/663942790

        build_acc(
            context,
            allocator,
            scratch_buf_size,
            geom,
            &acc_device,
            tlas,
            vk::AccelerationStructureTypeKHR::TOP_LEVEL,
            vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE,
            vk::BuildAccelerationStructureModeKHR::BUILD,
            1, // one instance
            1, // one instance
        );

        Tlas {
            acc_device,
            tlas,
            buffer,
        }
    }

    pub fn as_raw(&self) -> vk::AccelerationStructureKHR {
        self.tlas
    }

    pub fn get_device_address(&self) -> u64 {
        return unsafe {
            self.acc_device.get_acceleration_structure_device_address(
                &vk::AccelerationStructureDeviceAddressInfoKHR {
                    acceleration_structure: self.tlas,
                    ..Default::default()
                },
            )
        };
    }
}
