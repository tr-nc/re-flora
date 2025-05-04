use crate::vkn::{
    rtx::acceleration_structure::utils::{build_acc, create_acc, query_properties},
    Allocator, Buffer, VulkanContext,
};
use ash::{
    khr,
    vk::{self},
};

pub struct Blas {
    acc_device: khr::acceleration_structure::Device,

    buffer: Buffer,
    blas: vk::AccelerationStructureKHR,
}

// TODO: refactor this after testing
impl Drop for Blas {
    fn drop(&mut self) {
        unsafe {
            self.acc_device
                .destroy_acceleration_structure(self.blas, None);
        }
    }
}

impl Blas {
    pub fn new(
        vulkan_ctx: &VulkanContext,
        allocator: Allocator,
        acc_device: khr::acceleration_structure::Device,
        geom: vk::AccelerationStructureGeometryKHR,
    ) -> Self {
        let device = vulkan_ctx.device();

        const PRIMITIVE_COUNT: u32 = 12; // TODO: this should be read back later

        let (blas_size, scratch_buf_size) = query_properties(
            &acc_device,
            geom,
            &[PRIMITIVE_COUNT],
            vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
            vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE,
            vk::BuildAccelerationStructureModeKHR::BUILD,
            1,
        );

        let (blas, buffer) = create_acc(
            device,
            &allocator,
            &acc_device,
            blas_size,
            vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
        );

        build_acc(
            vulkan_ctx,
            allocator,
            scratch_buf_size,
            geom,
            &acc_device,
            blas,
            vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
            vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE,
            vk::BuildAccelerationStructureModeKHR::BUILD,
            1,
            PRIMITIVE_COUNT,
        );
        return Self {
            acc_device,
            blas,
            buffer,
        };
    }

    pub fn get_device_address(&self) -> u64 {
        return unsafe {
            self.acc_device.get_acceleration_structure_device_address(
                &vk::AccelerationStructureDeviceAddressInfoKHR {
                    acceleration_structure: self.blas,
                    ..Default::default()
                },
            )
        };
    }
}
