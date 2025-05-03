use crate::vkn::{
    rtx::acceleration_structure::utils::{build_acc, query_properties},
    Allocator, Buffer, BufferUsage, VulkanContext,
};
use ash::{khr, vk};
use std::mem::size_of;

use super::utils::create_acc;

pub struct Tlas {
    acc_device: khr::acceleration_structure::Device,

    tlas: vk::AccelerationStructureKHR,
    instance_buffer: Buffer,
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
        blas: vk::AccelerationStructureKHR,
    ) -> Self {
        let device = context.device();

        let blas_addr = unsafe {
            acc_device.get_acceleration_structure_device_address(
                &vk::AccelerationStructureDeviceAddressInfoKHR {
                    acceleration_structure: blas,
                    ..Default::default()
                },
            )
        };

        let instance = vk::AccelerationStructureInstanceKHR {
            transform: vk::TransformMatrixKHR {
                // matrix is a 3x4 row-major affine transformation matrix
                matrix: [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            },
            // instanceCustomIndex is a 24-bit application-specified index value accessible to ray shaders in the InstanceCustomIndexKHR built-in
            // mask is an 8-bit visibility mask for the geometry. The instance may only be hit if Cull Mask & instance.mask != 0
            instance_custom_index_and_mask: vk::Packed24_8::new(0, 0xFF),
            instance_shader_binding_table_record_offset_and_flags: vk::Packed24_8::new(0, 0),
            acceleration_structure_reference: vk::AccelerationStructureReferenceKHR {
                device_handle: blas_addr,
            },
        };

        // 3. Upload that into a small host‐visible buffer
        let instance_data_size = size_of::<vk::AccelerationStructureInstanceKHR>() as u64;
        let instance_buffer = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR
                    | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            ),
            gpu_allocator::MemoryLocation::CpuToGpu,
            instance_data_size,
        );

        instance_buffer
            .fill(&[instance])
            .expect("Failed to fill instance buffer");

        // 4. Build size query for the TLAS
        let geom = vk::AccelerationStructureGeometryKHR {
            geometry_type: vk::GeometryTypeKHR::INSTANCES,
            geometry: vk::AccelerationStructureGeometryDataKHR {
                instances: vk::AccelerationStructureGeometryInstancesDataKHR {
                    array_of_pointers: vk::FALSE,
                    data: vk::DeviceOrHostAddressConstKHR {
                        device_address: instance_buffer.device_address(),
                    },
                    ..Default::default()
                },
            },
            flags: vk::GeometryFlagsKHR::OPAQUE,
            ..Default::default()
        };

        let (tlas_size, scratch_buf_size) = query_properties(
            &acc_device,
            geom,
            &[1], // one instance
            vk::AccelerationStructureTypeKHR::TOP_LEVEL,
            vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE,
            vk::BuildAccelerationStructureModeKHR::BUILD,
            1, // one instance
        );

        let tlas = create_acc(
            context.device(),
            &allocator,
            &acc_device,
            tlas_size,
            vk::AccelerationStructureTypeKHR::TOP_LEVEL,
        );

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
            instance_buffer,
        }
    }

    pub fn as_raw(&self) -> vk::AccelerationStructureKHR {
        self.tlas
    }
}
