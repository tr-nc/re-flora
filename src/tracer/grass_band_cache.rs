//! Frame-slot-owned GPU pose/color cache; never reads grass back to CPU.
use re_flora_vkn::{vk, Allocator, Buffer, BufferUsage, Device, MemoryLocation};
use std::sync::Arc;

// Two float4s in shader/slang/grass_band_cache.slang; GPU-only data has no CPU mirror.
const POSE_BYTES: u64 = 32;

#[derive(Default)]
pub struct GrassBandCache {
    frames: Vec<Option<(Arc<Buffer>, u32)>>,
}
impl GrassBandCache {
    // Caller has waited for this frame slot before replacing or writing its buffers.
    pub fn ensure(
        &mut self,
        device: Device,
        allocator: Allocator,
        slot: usize,
        entries: u32,
    ) -> Arc<Buffer> {
        self.frames
            .resize_with(self.frames.len().max(slot + 1), || None);
        if self.frames[slot]
            .as_ref()
            .is_none_or(|(_, capacity)| *capacity < entries)
        {
            let capacity = entries.max(1).next_power_of_two();
            let buffer = Buffer::new_sized(
                device,
                allocator,
                BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
                MemoryLocation::GpuOnly,
                u64::from(capacity) * POSE_BYTES,
            );
            log::info!(
                "[STEM_BAND_CACHE] slot={slot} entries={capacity} bytes={}",
                u64::from(capacity) * POSE_BYTES
            );
            self.frames[slot] = Some((Arc::new(buffer), capacity));
        }
        self.frames[slot].as_ref().unwrap().0.clone()
    }
}
