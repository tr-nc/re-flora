use crate::vkn::Buffer;
use ash::{khr, vk};
use std::{ops::Deref, sync::Arc};

struct AccelStructInner {
    acc_device: khr::acceleration_structure::Device,
    blas: vk::AccelerationStructureKHR,
    // must be kept alive until AS is destroyed
    buffer: Buffer,
}

impl Drop for AccelStructInner {
    fn drop(&mut self) {
        unsafe {
            self.acc_device
                .destroy_acceleration_structure(self.blas, None);
        }
    }
}

#[derive(Clone)]
pub struct AccelStruct(Arc<AccelStructInner>);

impl Deref for AccelStruct {
    type Target = vk::AccelerationStructureKHR;

    fn deref(&self) -> &Self::Target {
        &self.0.blas
    }
}
impl AccelStruct {
    /// Create a new BLAS handle from a built AS and its buffer.
    pub fn new(
        acc_device: khr::acceleration_structure::Device,
        blas: vk::AccelerationStructureKHR,
        buffer: Buffer,
    ) -> Self {
        AccelStruct(Arc::new(AccelStructInner {
            acc_device,
            blas,
            buffer,
        }))
    }

    /// Get the raw AS handle.
    pub fn as_raw(&self) -> vk::AccelerationStructureKHR {
        self.0.blas
    }

    /// Query the device address of the AS.
    pub fn get_device_address(&self) -> u64 {
        unsafe {
            self.0.acc_device.get_acceleration_structure_device_address(
                &vk::AccelerationStructureDeviceAddressInfoKHR {
                    acceleration_structure: self.0.blas,
                    ..Default::default()
                },
            )
        }
    }
}
