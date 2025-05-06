use crate::vkn::{
    rtx::acceleration_structure::utils::{build_acc, query_properties},
    Allocator, Blas, Buffer, BufferUsage, VulkanContext,
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

    instance_buf: Buffer,
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
        let instance_data_size = size_of::<vk::AccelerationStructureInstanceKHR>() as u64;
        let instance_buf = Buffer::new_sized(
            vulkan_ctx.device().clone(),
            allocator.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR
                    | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            ),
            gpu_allocator::MemoryLocation::CpuToGpu,
            instance_data_size,
        );

        Tlas {
            vulkan_ctx,
            allocator,
            acc_device,
            instance_buf,
            tlas: None,
            _buffer: None,
        }
    }

    pub fn build(&mut self, blas: &Blas) {
        let geom = make_tlas_geom(blas, &self.instance_buf);

        // TODO: maybe reuse the scratch buffer / tlas handle later
        let (tlas_size, scratch_buf_size) = query_properties(
            &self.acc_device,
            geom,
            &[1], // one instance
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
            1, // one instance
            1, // one instance
        );

        self.tlas = Some(tlas);
        self._buffer = Some(buffer);

        fn make_tlas_geom<'a>(
            blas: &'a Blas,
            instance_buffer: &'a Buffer,
        ) -> vk::AccelerationStructureGeometryKHR<'a> {
            let instance = vk::AccelerationStructureInstanceKHR {
                transform: vk::TransformMatrixKHR {
                    // matrix is a 3x4 row-major affine transformation matrix
                    matrix: [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0],
                },
                // instanceCustomIndex is a 24-bit application-specified index value accessible to ray shaders in the InstanceCustomIndexKHR built-in
                // mask is an 8-bit visibility mask for the geometry. The instance may only be hit if Cull Mask & instance.mask != 0
                instance_custom_index_and_mask: vk::Packed24_8::new(0, 0xFF),
                instance_shader_binding_table_record_offset_and_flags: vk::Packed24_8::new(
                    0,
                    vk::GeometryInstanceFlagsKHR::TRIANGLE_FACING_CULL_DISABLE.as_raw() as u8,
                ),
                acceleration_structure_reference: vk::AccelerationStructureReferenceKHR {
                    device_handle: blas.get_device_address().unwrap(),
                },
            };

            instance_buffer
                .fill(&[instance])
                .expect("Failed to fill instance buffer");

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
