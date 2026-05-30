use ash::vk;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use crate::Device;

struct FenceInner {
    device: Device,
    fence: vk::Fence,
    recycle_to_gpu_job_pool: bool,
    completed_for_reuse: AtomicBool,
}

impl Drop for FenceInner {
    fn drop(&mut self) {
        if self.recycle_to_gpu_job_pool && self.completed_for_reuse.load(Ordering::Acquire) {
            self.device.recycle_gpu_job_fence(self.fence);
        } else {
            unsafe {
                self.device.destroy_fence(self.fence, None);
            }
        }
    }
}

#[derive(Clone)]
pub struct Fence(Arc<FenceInner>);

impl Fence {
    pub fn new(device: &Device, is_signaled: bool) -> Self {
        let fence = Self::create_fence(device, is_signaled);
        Self::from_raw(device, fence, false)
    }

    pub(crate) fn new_pooled_gpu_job(device: &Device) -> ash::prelude::VkResult<Self> {
        let fence = match device.acquire_gpu_job_fence()? {
            Some(fence) => fence,
            None => Self::create_fence(device, false),
        };
        Ok(Self::from_raw(device, fence, true))
    }

    fn from_raw(device: &Device, fence: vk::Fence, recycle_to_gpu_job_pool: bool) -> Self {
        Self(Arc::new(FenceInner {
            device: device.clone(),
            fence,
            recycle_to_gpu_job_pool,
            completed_for_reuse: AtomicBool::new(false),
        }))
    }

    pub(crate) fn mark_completed_for_reuse(&self) {
        self.0.completed_for_reuse.store(true, Ordering::Release);
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
