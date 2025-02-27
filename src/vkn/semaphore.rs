use ash::vk;

use super::Device;

pub struct Semaphore {
    device: Device,
    pub semaphore: vk::Semaphore,
}

impl Drop for Semaphore {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_semaphore(self.semaphore, None);
        }
    }
}

impl Semaphore {
    pub fn new(device: &Device) -> Self {
        let semaphore = Self::create_semaphore(device);
        Self {
            device: device.clone(),
            semaphore,
        }
    }

    pub fn as_raw(&self) -> vk::Semaphore {
        self.semaphore
    }

    fn create_semaphore(device: &Device) -> vk::Semaphore {
        let semaphore_info = vk::SemaphoreCreateInfo::default();
        unsafe { device.create_semaphore(&semaphore_info, None).unwrap() }
    }
}
