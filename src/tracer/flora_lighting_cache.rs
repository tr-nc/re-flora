use re_flora_vkn::{vk, Allocator, Buffer, BufferUsage, Device, MemoryLocation};
use std::sync::Arc;

pub const FLORA_LIGHTING_CACHE_ENTRY_BYTES: u64 = 16;

#[derive(Default)]
pub struct FloraLightingCache {
    frames: Vec<Option<FloraLightingCacheFrame>>,
}

struct FloraLightingCacheFrame {
    buffer: Arc<Buffer>,
    capacity_entries: u32,
}

impl FloraLightingCache {
    pub fn ensure_capacity(
        &mut self,
        device: Device,
        allocator: Allocator,
        frame_slot: usize,
        required_entries: u32,
    ) {
        if self.frames.len() <= frame_slot {
            self.frames.resize_with(frame_slot + 1, || None);
        }
        if self.frames[frame_slot]
            .as_ref()
            .is_some_and(|frame| frame.capacity_entries >= required_entries)
        {
            return;
        }

        let capacity_entries = cache_capacity_entries(required_entries);
        let buffer = Buffer::new_sized(
            device,
            allocator,
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            MemoryLocation::GpuOnly,
            u64::from(capacity_entries) * FLORA_LIGHTING_CACHE_ENTRY_BYTES,
        );
        log::info!(
            "[DDGI][FLORA_CACHE] allocated frame_slot={} entries={} bytes={} mib={:.2}",
            frame_slot,
            capacity_entries,
            u64::from(capacity_entries) * FLORA_LIGHTING_CACHE_ENTRY_BYTES,
            u64::from(capacity_entries) as f64 * FLORA_LIGHTING_CACHE_ENTRY_BYTES as f64
                / (1024.0 * 1024.0),
        );
        self.frames[frame_slot] = Some(FloraLightingCacheFrame {
            buffer: Arc::new(buffer),
            capacity_entries,
        });
    }

    pub fn buffer(&self, frame_slot: usize) -> Arc<Buffer> {
        self.frames[frame_slot]
            .as_ref()
            .expect("flora lighting cache capacity must be ensured before use")
            .buffer
            .clone()
    }
}

fn cache_capacity_entries(required_entries: u32) -> u32 {
    required_entries.max(1).next_power_of_two()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_capacity_is_nonzero_and_grows_by_powers_of_two() {
        assert_eq!(cache_capacity_entries(0), 1);
        assert_eq!(cache_capacity_entries(1), 1);
        assert_eq!(cache_capacity_entries(2), 2);
        assert_eq!(cache_capacity_entries(3), 4);
        assert_eq!(cache_capacity_entries(1_000_000), 1_048_576);
    }
}
