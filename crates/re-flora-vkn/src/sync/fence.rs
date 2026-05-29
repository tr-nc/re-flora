use ash::vk;
use std::sync::Arc;

use crate::Device;

struct FenceInner {
    device: Device,
    fence: vk::Fence,
}

impl Drop for FenceInner {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_fence(self.fence, None);
        }
    }
}

#[derive(Clone)]
pub struct Fence(Arc<FenceInner>);

impl Fence {
    pub fn new(device: &Device, is_signaled: bool) -> Self {
        let fence = Self::create_fence(device, is_signaled);
        Self(Arc::new(FenceInner {
            device: device.clone(),
            fence,
        }))
    }

    pub(crate) fn as_raw(&self) -> vk::Fence {
        self.0.fence
    }

    pub fn wait(&self) -> ash::prelude::VkResult<()> {
        unsafe { self.0.device.wait_for_fences(&[self.0.fence], true, u64::MAX) }
    }

    pub fn reset(&self) -> ash::prelude::VkResult<()> {
        unsafe { self.0.device.reset_fences(&[self.0.fence]) }
    }

    pub fn is_signaled(&self) -> ash::prelude::VkResult<bool> {
        unsafe { self.0.device.get_fence_status(self.0.fence) }
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
