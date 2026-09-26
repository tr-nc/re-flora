//! Frame-slot-owned, demand-sized tile storage shared by every model adapter.
//! Reusing a slot is safe only after its submission fence has completed, just
//! like the renderer's other transient descriptor and repair allocations.
use anyhow::{ensure, Result};
use re_flora_vkn::{vk, Allocator, Buffer, BufferUsage, Device, MemoryLocation};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

pub const BATCH_TEXELS: usize = 64 * 1024 * 1024 / 16;
// Vulkan's guaranteed Z dispatch range. This splits work in the SAME frame;
// it is a resource constraint, never an object/population limit.
const BATCH_INSTANCES: u32 = 65_535;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileBatch {
    pub first: u32,
    pub count: u32,
    pub texels: usize,
}
/// Contiguous draw batches with compact offsets; invisible models use no texels.
pub fn pack_tiles(models: impl IntoIterator<Item = (u32, bool)>) -> (Vec<u32>, Vec<TileBatch>) {
    let mut offsets = Vec::new();
    let mut batches = Vec::new();
    let mut batch = TileBatch {
        first: 0,
        count: 0,
        texels: 0,
    };
    for (resolution, visible) in models {
        assert!((8..=64).contains(&resolution));
        let cells = if visible {
            (resolution * resolution) as usize
        } else {
            0
        };
        if batch.texels + cells > BATCH_TEXELS || batch.count == BATCH_INSTANCES {
            batches.push(batch);
            batch = TileBatch {
                first: offsets.len() as u32,
                count: 0,
                texels: 0,
            };
        }
        offsets.push(batch.texels as u32);
        batch.count += 1;
        batch.texels += cells;
    }
    if batch.count > 0 {
        batches.push(batch);
    }
    (offsets, batches)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_particle_capacity_is_bounded_without_dropping_instances() {
        let (offsets, batches) = pack_tiles(std::iter::repeat_n((64, true), 16_384));
        assert_eq!(offsets.len(), 16_384);
        assert_eq!(
            batches.iter().map(|b| b.count as usize).sum::<usize>(),
            16_384
        );
        assert!(batches.len() > 1);
        for batch in batches {
            assert!(batch.texels <= BATCH_TEXELS);
            assert_eq!(offsets[batch.first as usize], 0);
            assert_eq!(
                offsets[(batch.first + batch.count - 1) as usize] as usize + 64 * 64,
                batch.texels
            );
        }
    }
    #[test]
    fn large_populations_split_for_dispatch_without_losing_objects() {
        let count = 200_000;
        let (offsets, batches) = pack_tiles(std::iter::repeat_n((8, true), count));
        assert_eq!(offsets.len(), count);
        assert_eq!(
            batches.iter().map(|b| b.count as usize).sum::<usize>(),
            count
        );
        assert!(batches
            .iter()
            .all(|b| b.count <= BATCH_INSTANCES && b.texels <= BATCH_TEXELS));
    }

    #[test]
    fn mixed_resolution_and_offscreen_models_use_only_actual_cells() {
        let (offsets, batches) = pack_tiles([(8, true), (64, false), (16, true), (32, true)]);
        assert_eq!(offsets, [0, 64, 64, 320]);
        assert_eq!(
            batches,
            [TileBatch {
                first: 0,
                count: 4,
                texels: 1344
            }]
        );
        assert!(pack_tiles([]).1.is_empty());
    }
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
enum ModelStorageKey {
    Shared(u64),
    Stream(&'static str, u32, u32),
}

#[derive(Default)]
pub struct ModelPixelTiles {
    frames: Vec<HashMap<(ModelStorageKey, bool), (Arc<Buffer>, usize)>>,
    used: Vec<HashSet<(ModelStorageKey, bool)>>,
}
impl ModelPixelTiles {
    pub fn begin_frame(&mut self, frame: usize) {
        if self.frames.len() <= frame {
            self.frames.resize_with(frame + 1, HashMap::new);
        }
        if self.used.len() <= frame {
            self.used.resize_with(frame + 1, HashSet::new);
        }
        // This frame slot's fence has completed. Keep the last use for reuse,
        // but retire buffers belonging to deleted trees or vanished batches.
        self.frames[frame].retain(|key, _| self.used[frame].contains(key));
        self.used[frame].clear();
    }
    pub fn get(
        &mut self,
        frame: usize,
        key: u64,
        texels: usize,
        device: Device,
        allocator: Allocator,
    ) -> Result<Arc<Buffer>> {
        self.buffer(
            frame,
            (ModelStorageKey::Shared(key), false),
            texels,
            device,
            allocator,
        )
    }

    /// Publish host-authored model inputs using the same fence-slot ownership
    /// as generated tiles. No renderer-side particle population policy.
    pub fn upload(
        &mut self,
        frame: usize,
        key: u64,
        bytes: &[u8],
        device: Device,
        allocator: Allocator,
    ) -> Result<Arc<Buffer>> {
        let buffer = self.buffer(
            frame,
            (ModelStorageKey::Shared(key), true),
            bytes.len().div_ceil(16),
            device,
            allocator,
        )?;
        if !bytes.is_empty() {
            buffer.fill_range_with_raw_u8(0, bytes)?;
        }
        Ok(buffer)
    }

    pub fn get_stream(
        &mut self,
        frame: usize,
        key: (&'static str, u32, u32),
        texels: usize,
        device: Device,
        allocator: Allocator,
    ) -> Result<Arc<Buffer>> {
        self.buffer(
            frame,
            (ModelStorageKey::Stream(key.0, key.1, key.2), false),
            texels,
            device,
            allocator,
        )
    }

    pub fn upload_stream(
        &mut self,
        frame: usize,
        key: (&'static str, u32, u32),
        bytes: &[u8],
        device: Device,
        allocator: Allocator,
    ) -> Result<Arc<Buffer>> {
        let buffer = self.buffer(
            frame,
            (ModelStorageKey::Stream(key.0, key.1, key.2), true),
            bytes.len().div_ceil(16),
            device,
            allocator,
        )?;
        if !bytes.is_empty() {
            buffer.fill_range_with_raw_u8(0, bytes)?;
        }
        Ok(buffer)
    }

    fn buffer(
        &mut self,
        frame: usize,
        key: (ModelStorageKey, bool),
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
        if self.used.len() <= frame {
            self.used.resize_with(frame + 1, HashSet::new);
        }
        self.used[frame].insert(key);
        let slots = &mut self.frames[frame];
        if slots.get(&key).is_none_or(|(_, old)| *old < capacity) {
            let buffer = Buffer::new_sized(
                device,
                allocator,
                BufferUsage::from_flags(
                    vk::BufferUsageFlags::STORAGE_BUFFER
                        | vk::BufferUsageFlags::TRANSFER_SRC // explicit validation/capture; no normal-frame readback
                        | if key.1 {
                            vk::BufferUsageFlags::VERTEX_BUFFER
                        } else {
                            vk::BufferUsageFlags::empty()
                        },
                ),
                if key.1 {
                    MemoryLocation::CpuToGpu
                } else {
                    MemoryLocation::GpuOnly
                },
                (capacity * 16) as u64,
            );
            slots.insert(key, (Arc::new(buffer), capacity));
        }
        Ok(slots[&key].0.clone())
    }
}
