//! GPU quality classifications belong to a source batch, not a compacted row.
//! No radiance/visibility history is retained; exact fallbacks always trace now.
//! Descriptor owners retain replaced buffers until their in-flight frame retires.
use re_flora_vkn::{vk, Allocator, Buffer, BufferUsage, CommandBuffer, Device, MemoryLocation};
use std::{collections::HashMap, sync::Arc};

const ENTRY_BYTES: u64 = 48; // FloraLightStability in flora_light_stability.slang
const RETAIN_FRAMES: u32 = 60;

struct Batch {
    buffer: Arc<Buffer>,
    capacity: u32,
    last_frame: u32,
}

#[derive(Default)]
pub(super) struct FloraLightStability {
    batches: HashMap<(usize, usize), Batch>,
    dummy: Option<Arc<Buffer>>,
    frame: u32,
}

impl FloraLightStability {
    pub(super) fn begin_frame(&mut self, enabled: bool) {
        self.frame = self.frame.wrapping_add(1);
        let frame = self.frame;
        self.batches
            .retain(|_, batch| enabled && frame.wrapping_sub(batch.last_frame) <= RETAIN_FRAMES);
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn ensure(
        &mut self,
        device: Device,
        allocator: Allocator,
        cmdbuf: &CommandBuffer,
        batch: (usize, usize),
        entries: u32,
        enabled: bool,
    ) -> Arc<Buffer> {
        if !enabled {
            return self
                .dummy
                .get_or_insert_with(|| allocate(device, allocator, cmdbuf, 1))
                .clone();
        }
        let frame = self.frame;
        let entry = self.batches.entry(batch).or_insert_with(|| Batch {
            buffer: allocate(device.clone(), allocator.clone(), cmdbuf, entries),
            capacity: capacity(entries),
            last_frame: frame,
        });
        if entry.capacity < entries {
            entry.capacity = capacity(entries);
            entry.buffer = allocate(device, allocator, cmdbuf, entries);
        }
        entry.last_frame = frame;
        entry.buffer.clone()
    }
}

fn capacity(entries: u32) -> u32 {
    entries.max(1).next_power_of_two()
}

fn allocate(
    device: Device,
    allocator: Allocator,
    cmdbuf: &CommandBuffer,
    entries: u32,
) -> Arc<Buffer> {
    let entries = capacity(entries);
    let bytes = u64::from(entries) * ENTRY_BYTES;
    let buffer = Arc::new(Buffer::new_sized(
        device,
        allocator,
        BufferUsage::from_flags(
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
        ),
        MemoryLocation::GpuOnly,
        bytes,
    ));
    buffer.record_fill(cmdbuf, 0, bytes, 0);
    log::info!("[LIGHT_RIS][STABILITY] allocated entries={entries} bytes={bytes}");
    buffer
}
