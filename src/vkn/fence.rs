use ash::vk;

use super::Device;

pub struct Fence {
    device: Device,
    fence: vk::Fence,
}

impl Drop for Fence {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_fence(self.fence, None);
        }
    }
}

impl Fence {
    pub fn new(device: &Device, is_signaled: bool) -> Self {
        let fence = Self::create_fence(device, is_signaled);
        Self {
            device: device.clone(),
            fence,
        }
    }

    pub fn as_raw(&self) -> vk::Fence {
        self.fence
    }

    fn create_fence(device: &Device, is_signaled: bool) -> vk::Fence {
        let fence_create_flags = if is_signaled {
            vk::FenceCreateFlags::SIGNALED
        } else {
            vk::FenceCreateFlags::empty()
        };
        let create_info = vk::FenceCreateInfo::default().flags(fence_create_flags);
        unsafe { device.create_fence(&create_info, None).unwrap() }
    }
}
