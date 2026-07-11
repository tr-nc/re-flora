use ash::vk;

use crate::Semaphore;

const MAX_PRESENT_WAITS: usize = 8;

/// A named binary semaphore wait edge for a present operation.
#[derive(Clone, Copy)]
pub struct PresentWait<'a> {
    pub name: &'static str,
    pub semaphore: &'a Semaphore,
}

impl<'a> PresentWait<'a> {
    pub fn new(name: &'static str, semaphore: &'a Semaphore) -> Self {
        Self { name, semaphore }
    }
}

/// Semantic present description.
///
/// This keeps swapchain present dependencies named and vkn-owned so a future
/// profiler can connect render-submit signals to present waits without changing
/// frame call sites again.
#[derive(Clone, Copy)]
pub struct PresentDesc<'a> {
    pub name: &'static str,
    pub image_index: u32,
    pub waits: &'a [PresentWait<'a>],
}

impl<'a> PresentDesc<'a> {
    pub fn new(name: &'static str, image_index: u32, waits: &'a [PresentWait<'a>]) -> Self {
        Self {
            name,
            image_index,
            waits,
        }
    }

    pub(crate) fn assert_supported_sizes(&self) {
        assert!(
            self.waits.len() <= MAX_PRESENT_WAITS,
            "present '{}' has {} waits; max supported without allocation is {}",
            self.name,
            self.waits.len(),
            MAX_PRESENT_WAITS
        );
    }

    pub(crate) fn raw_waits(&self) -> ([vk::Semaphore; MAX_PRESENT_WAITS], usize) {
        let mut raw = [vk::Semaphore::null(); MAX_PRESENT_WAITS];
        for (dst, wait) in raw.iter_mut().zip(self.waits.iter()) {
            *dst = wait.semaphore.as_raw();
        }
        (raw, self.waits.len())
    }
}
