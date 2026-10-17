//! Staged sprinkler poses and frame-owned mist work buffers. Only acquired slots may publish.
use anyhow::{ensure, Result};
use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use re_flora_vkn::{vk, Allocator, Buffer, BufferUsage, Device, MemoryLocation};

pub const SPRINKLER_RENDER_CAPACITY: usize = 256;
pub const SPRINKLER_MIST_LOBES: u32 = 48;
pub const SPRINKLER_MIST_TILE_SIZE: u32 = 16;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
struct SprinklerInstanceGpu {
    base_position: [f32; 3],
    animation_phase: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct SprinklerRenderInstance {
    pub base_position: Vec3,
    pub animation_phase: f32,
}

struct SprinklerFrame {
    instances: Buffer,
    published: Vec<SprinklerInstanceGpu>,
    lobes: Buffer,
    bounds: Buffer,
    tiles: Buffer,
    tile_capacity: u64,
}

pub struct SprinklerRendererResources {
    device: Device,
    allocator: Allocator,
    pending: Vec<SprinklerInstanceGpu>,
    frames: Vec<SprinklerFrame>,
    slot: usize,
    pub instance_count: u32,
}

fn tile_buffer_bytes(width: u32, height: u32) -> u64 {
    u64::from(width.div_ceil(SPRINKLER_MIST_TILE_SIZE))
        * u64::from(height.div_ceil(SPRINKLER_MIST_TILE_SIZE))
        * SPRINKLER_RENDER_CAPACITY.div_ceil(32) as u64
        * 4
}

impl SprinklerRendererResources {
    pub fn new(device: Device, allocator: Allocator) -> Self {
        Self {
            device,
            allocator,
            pending: Vec::new(),
            frames: Vec::new(),
            slot: 0,
            instance_count: 0,
        }
    }

    pub fn upload(&mut self, instances: &[SprinklerRenderInstance]) -> Result<()> {
        let pending = encode_instances(instances)?;
        self.instance_count = pending.len() as u32;
        self.pending = pending;
        Ok(())
    }

    fn buffer(&self, bytes: u64, location: MemoryLocation) -> Result<Buffer> {
        Buffer::try_new_sized(
            self.device.clone(),
            self.allocator.clone(),
            BufferUsage::from_flags(
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC,
            ),
            location,
            bytes,
        )
    }

    pub fn prepare_frame(&mut self, slot: usize, width: u32, height: u32) -> Result<()> {
        self.slot = slot;
        if self.pending.is_empty() {
            return Ok(());
        }
        let tile_bytes = tile_buffer_bytes(width, height).max(4);
        while self.frames.len() <= slot {
            self.frames.push(SprinklerFrame {
                instances: self.buffer(
                    (std::mem::size_of::<SprinklerInstanceGpu>() * SPRINKLER_RENDER_CAPACITY)
                        as u64,
                    MemoryLocation::CpuToGpu,
                )?,
                published: Vec::new(),
                lobes: self.buffer(
                    (std::mem::size_of::<crate::generated::gpu_structs::SprinklerMistLobe>()
                        * SPRINKLER_MIST_LOBES as usize
                        * SPRINKLER_RENDER_CAPACITY) as u64,
                    MemoryLocation::GpuOnly,
                )?,
                bounds: self.buffer(
                    (std::mem::size_of::<crate::generated::gpu_structs::SprinklerMistBounds>()
                        * SPRINKLER_RENDER_CAPACITY) as u64,
                    MemoryLocation::GpuOnly,
                )?,
                tiles: self.buffer(tile_bytes, MemoryLocation::GpuOnly)?,
                tile_capacity: tile_bytes,
            });
        }
        if self.frames[slot].tile_capacity < tile_bytes {
            // This slot's fence has completed; no other frame owns its buffer.
            self.frames[slot].tiles = self.buffer(tile_bytes, MemoryLocation::GpuOnly)?;
            self.frames[slot].tile_capacity = tile_bytes;
        }
        let frame = &mut self.frames[slot];
        if frame.published != self.pending {
            frame.instances.fill(&self.pending)?;
            frame.published.clone_from(&self.pending);
        }
        Ok(())
    }

    pub fn instances(&self) -> &Buffer {
        &self.frames[self.slot].instances
    }

    pub fn mist_lobes(&self) -> &Buffer {
        &self.frames[self.slot].lobes
    }

    pub fn mist_bounds(&self) -> &Buffer {
        &self.frames[self.slot].bounds
    }

    pub fn mist_tiles(&self) -> &Buffer {
        &self.frames[self.slot].tiles
    }
}

fn encode_instances(instances: &[SprinklerRenderInstance]) -> Result<Vec<SprinklerInstanceGpu>> {
    ensure!(
        instances.len() <= SPRINKLER_RENDER_CAPACITY,
        "sprinkler render capacity exceeded: {} > {}",
        instances.len(),
        SPRINKLER_RENDER_CAPACITY
    );
    instances
        .iter()
        .map(|instance| {
            ensure!(
                instance.base_position.is_finite() && instance.animation_phase.is_finite(),
                "sprinkler render pose must be finite"
            );
            Ok(SprinklerInstanceGpu {
                base_position: instance.base_position.to_array(),
                animation_phase: instance.animation_phase.rem_euclid(1.),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprinkler_pose_upload_is_bounded_finite_and_normalizes_phase() {
        let pose = SprinklerRenderInstance {
            base_position: Vec3::ONE,
            animation_phase: -0.25,
        };
        let encoded = encode_instances(&[pose]).unwrap();
        assert_eq!(std::mem::size_of::<SprinklerInstanceGpu>(), 16);
        assert_eq!(encoded[0].base_position, [1.; 3]);
        assert_eq!(encoded[0].animation_phase, 0.75);
        assert!(encode_instances(&[]).unwrap().is_empty());
        assert!(encode_instances(&[pose; SPRINKLER_RENDER_CAPACITY]).is_ok());
        assert!(encode_instances(&[pose; SPRINKLER_RENDER_CAPACITY + 1]).is_err());
        assert!(encode_instances(&[SprinklerRenderInstance {
            animation_phase: f32::NAN,
            ..pose
        }])
        .is_err());
    }

    #[test]
    fn mist_frame_work_is_bounded_and_tiles_cover_partial_edges_without_overflow() {
        assert!(SPRINKLER_MIST_LOBES <= 64 && SPRINKLER_MIST_LOBES % 4 == 0);
        assert_eq!(
            std::mem::size_of::<crate::generated::gpu_structs::SprinklerMistLobe>(),
            48
        );
        assert_eq!(
            std::mem::size_of::<crate::generated::gpu_structs::SprinklerMistBounds>(),
            48
        );
        assert_eq!(tile_buffer_bytes(1, 1), 32);
        assert_eq!(tile_buffer_bytes(16, 16), 32);
        assert_eq!(tile_buffer_bytes(17, 17), 128);
        assert_eq!(tile_buffer_bytes(1024, 576), 64 * 36 * 32);
        assert!(tile_buffer_bytes(u32::MAX, u32::MAX) > u64::from(u32::MAX));
    }
}
