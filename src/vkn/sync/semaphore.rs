use crate::vkn::Device;
use ash::vk;
use std::sync::Arc;

struct SemaphoreInner {
    device: Device,
    semaphore: vk::Semaphore,
}

impl Drop for SemaphoreInner {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_semaphore(self.semaphore, None);
        }
    }
}

#[derive(Clone)]
pub struct Semaphore(Arc<SemaphoreInner>);

impl std::ops::Deref for Semaphore {
    type Target = vk::Semaphore;
    fn deref(&self) -> &Self::Target {
        &self.0.semaphore
    }
}

impl Semaphore {
    pub fn new(device: &Device) -> Self {
        let semaphore = Self::create_semaphore(device);
        Self(Arc::new(SemaphoreInner {
            device: device.clone(),
            semaphore,
        }))
    }

    pub fn as_raw(&self) -> vk::Semaphore {
        self.0.semaphore
    }

    fn create_semaphore(device: &Device) -> vk::Semaphore {
        let semaphore_info = vk::SemaphoreCreateInfo::default();
        unsafe { device.create_semaphore(&semaphore_info, None).unwrap() }
    }
}
