use crate::Buffer;
use ash::{khr, vk};
use std::{ops::Deref, sync::Arc};

struct AccelStructInner {
    acc_device: khr::acceleration_structure::Device,
    blas: vk::AccelerationStructureKHR,

    // Retained through vkDestroyAccelerationStructureKHR in Drop.
    _backing_buffer: Buffer,
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
        backing_buffer: Buffer,
    ) -> Self {
        AccelStruct(Arc::new(AccelStructInner {
            acc_device,
            blas,
            _backing_buffer: backing_buffer,
        }))
    }

    /// Get the raw AS handle.
    pub fn as_raw(&self) -> vk::AccelerationStructureKHR {
        self.0.blas
    }

    /// Query the device address of the AS.
    #[allow(dead_code)]
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

#[cfg(test)]
mod tests {
    use super::{AccelStruct, AccelStructInner};
    use crate::Buffer;

    fn assert_clone<T: Clone>() {}

    fn retained_backing_buffer(inner: &AccelStructInner) -> &Buffer {
        &inner._backing_buffer
    }

    #[test]
    fn acceleration_structure_and_backing_buffer_are_leaseable() {
        assert_clone::<AccelStruct>();
        assert_clone::<Buffer>();

        let accessor: for<'a> fn(&'a AccelStructInner) -> &'a Buffer = retained_backing_buffer;
        let _ = accessor;
    }
}
