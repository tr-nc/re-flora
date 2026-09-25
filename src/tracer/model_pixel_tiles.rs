//! Frame-slot-owned, demand-sized tile storage shared by every model adapter.
//! Reusing a slot is safe only after its submission fence has completed, just
//! like the renderer's other transient descriptor and repair allocations.
use anyhow::{ensure, Result};
use re_flora_vkn::{vk, Allocator, Buffer, BufferUsage, Device, MemoryLocation};
use std::{collections::HashMap, sync::Arc};

#[derive(Default)]
pub struct ModelPixelTiles {
    frames: Vec<HashMap<u64, (Arc<Buffer>, usize)>>,
}
impl ModelPixelTiles {
    pub fn get(
        &mut self,
        frame: usize,
        key: u64,
        texels: usize,
        device: Device,
        allocator: Allocator,
    ) -> Result<Arc<Buffer>> {
        let capacity = texels
            .max(1)
            .checked_next_power_of_two()
            .ok_or_else(|| anyhow::anyhow!("model tile allocation overflow"))?;
        ensure!(
            capacity <= 128 * 1024 * 1024 / 16,
            "model tile batch exceeds portable storage-buffer range"
        );
        if self.frames.len() <= frame {
            self.frames.resize_with(frame + 1, HashMap::new);
        }
        let slots = &mut self.frames[frame];
        if slots.get(&key).is_none_or(|(_, old)| *old < capacity) {
            let buffer = Buffer::new_sized(
                device,
                allocator,
                BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
                MemoryLocation::GpuOnly,
                (capacity * 16) as u64,
            );
            slots.insert(key, (Arc::new(buffer), capacity));
        }
        Ok(slots[&key].0.clone())
    }
}
