//! Fence-slot-owned publication of leaf occupancy/growth. CPU lifecycle edits
//! never overwrite a storage buffer still referenced by an in-flight draw.
use anyhow::Result;
use re_flora_vkn::{vk, Allocator, Buffer, BufferUsage, Device, MemoryLocation};

pub struct LeafStatePublication {
    values: Vec<f32>,
    revision: u64,
    frames: Vec<(Buffer, u64)>,
    current: usize,
}

impl LeafStatePublication {
    pub fn new(count: usize, device: Device, allocator: Allocator) -> Self {
        let values = vec![1.0; count.max(1)];
        let buffer = Self::allocate(values.len(), device, allocator);
        buffer.fill(&values).expect("initialize leaf state");
        Self {
            values,
            revision: 0,
            frames: vec![(buffer, 0)],
            current: 0,
        }
    }

    fn allocate(count: usize, device: Device, allocator: Allocator) -> Buffer {
        Buffer::new_sized(
            device,
            allocator,
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            MemoryLocation::CpuToGpu,
            count.max(1) as u64 * 4,
        )
    }

    pub fn set(&mut self, values: &[f32]) {
        assert_eq!(values.len(), self.values.len());
        if self.values != values {
            self.values.copy_from_slice(values);
            self.revision += 1;
        }
    }

    /// Called only after acquiring this frame slot's fence.
    pub fn publish(&mut self, slot: usize, device: Device, allocator: Allocator) -> Result<()> {
        while self.frames.len() <= slot {
            self.frames.push((
                Self::allocate(self.values.len(), device.clone(), allocator.clone()),
                u64::MAX,
            ));
        }
        let (buffer, revision) = &mut self.frames[slot];
        if *revision != self.revision {
            buffer.fill(&self.values)?;
            *revision = self.revision;
        }
        self.current = slot;
        Ok(())
    }

    /// Rendering consumes occupancy; it does not decide which leaves exist.
    pub fn live_indices(&self) -> impl Iterator<Item = u32> + '_ {
        self.values
            .iter()
            .enumerate()
            .filter_map(|(i, &growth)| (growth > 0.0).then_some(i as u32))
    }

    pub fn buffer(&self) -> &Buffer {
        &self.frames[self.current].0
    }
}
