//! Staged sprinkler poses and frame-owned mist work buffers. Only acquired slots may publish.
use anyhow::{ensure, Result};
use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use re_flora_vkn::{vk, Allocator, Buffer, BufferUsage, Device, MemoryLocation};

pub const SPRINKLER_RENDER_CAPACITY: usize = 256;
pub const SPRINKLER_MIST_LOBES: u32 = 256;
pub const SPRINKLER_DROPLETS: u32 = 128;
// One bounded field shared by ALL sprinklers, not one volume per source.
// The shader reads this side from the frame header; no duplicated shader constant.
pub const SPRINKLER_AIR_GRID_SIDE: u32 = 96;
pub const SPRINKLER_AIR_GRID_BYTES: u64 = SPRINKLER_AIR_GRID_SIDE as u64
    * SPRINKLER_AIR_GRID_SIDE as u64
    * SPRINKLER_AIR_GRID_SIDE as u64
    * 4;

#[cfg(test)]
fn sprinkler_mist_sample_count(rotary: bool) -> u32 {
    // Eight launch-velocity bands × 32 ages. Original A/B keeps its exact 4×12 budget.
    if rotary {
        SPRINKLER_MIST_LOBES
    } else {
        48
    }
}
pub const SPRINKLER_MIST_TILE_SIZE: u32 = 16;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
struct SprinklerInstanceGpu {
    base_position: [f32; 3],
    animation_phase: f32,
    kind: [u32; 4],
}

#[derive(Clone, Copy, Debug)]
pub struct SprinklerRenderInstance {
    pub base_position: Vec3,
    pub animation_phase: f32,
    pub kind: crate::sprinkler::Kind,
}

struct SprinklerFrame {
    instances: Buffer,
    published: Vec<SprinklerInstanceGpu>,
    lobes: Buffer,
    droplets: Buffer,
    bounds: Buffer,
    water_contacts: Buffer,
    vegetation_spray: Buffer,
    water_bounds: Buffer,
    tiles: Buffer,
    tile_capacity: u64,
    air_info: Buffer,
    air_density: Option<Buffer>,
}

pub struct SprinklerRendererResources {
    device: Device,
    allocator: Allocator,
    pending: Vec<SprinklerInstanceGpu>,
    frames: Vec<SprinklerFrame>,
    slot: usize,
    pub instance_count: u32,
    revision: u64,
    pub(super) published_water: Option<super::vegetation_response::water::PublishedSpray>,
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
            revision: 0,
            published_water: None,
        }
    }

    pub fn upload(&mut self, instances: &[SprinklerRenderInstance]) -> Result<()> {
        let pending = encode_instances(instances)?;
        self.instance_count = pending.len() as u32;
        if self.pending != pending {
            self.revision = self.revision.wrapping_add(1);
        }
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

    pub fn prepare_frame(
        &mut self,
        slot: usize,
        width: u32,
        height: u32,
        air_enabled: bool,
    ) -> Result<()> {
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
                droplets: self.buffer(
                    (std::mem::size_of::<crate::generated::gpu_structs::SprinklerMistLobe>()
                        * SPRINKLER_DROPLETS as usize
                        * SPRINKLER_RENDER_CAPACITY) as u64,
                    MemoryLocation::GpuOnly,
                )?,
                bounds: self.buffer(
                    (std::mem::size_of::<crate::generated::gpu_structs::SprinklerMistBounds>()
                        * SPRINKLER_RENDER_CAPACITY) as u64,
                    MemoryLocation::GpuOnly,
                )?,
                vegetation_spray: self.buffer(
                    80 * SPRINKLER_MIST_LOBES as u64 * SPRINKLER_RENDER_CAPACITY as u64,
                    MemoryLocation::GpuOnly,
                )?,
                water_contacts: self.buffer(
                    (std::mem::size_of::<crate::generated::gpu_structs::SprinklerWaterContact>()
                        * SPRINKLER_MIST_LOBES as usize
                        * SPRINKLER_RENDER_CAPACITY) as u64,
                    MemoryLocation::GpuOnly,
                )?,
                water_bounds: self.buffer(
                    (std::mem::size_of::<crate::generated::gpu_structs::SprinklerMistBounds>()
                        * SPRINKLER_RENDER_CAPACITY) as u64,
                    MemoryLocation::GpuOnly,
                )?,
                tiles: self.buffer(tile_bytes, MemoryLocation::GpuOnly)?,
                tile_capacity: tile_bytes,
                air_info: self.buffer(
                    std::mem::size_of::<crate::generated::gpu_structs::SprinklerAirInfo>() as u64,
                    MemoryLocation::GpuOnly,
                )?,
                air_density: None,
            });
        }
        if self.frames[slot].tile_capacity < tile_bytes {
            // This slot's fence has completed; no other frame owns its buffer.
            self.frames[slot].tiles = self.buffer(tile_bytes, MemoryLocation::GpuOnly)?;
            self.frames[slot].tile_capacity = tile_bytes;
        }
        if air_enabled && self.frames[slot].air_density.is_none() {
            self.frames[slot].air_density = Some(Buffer::try_new_sized(
                self.device.clone(),
                self.allocator.clone(),
                BufferUsage::from_flags(
                    vk::BufferUsageFlags::STORAGE_BUFFER
                        | vk::BufferUsageFlags::TRANSFER_DST
                        | vk::BufferUsageFlags::TRANSFER_SRC,
                ),
                MemoryLocation::GpuOnly,
                SPRINKLER_AIR_GRID_BYTES,
            )?);
        }
        let frame = &mut self.frames[slot];
        if frame.published != self.pending {
            frame.instances.fill(&self.pending)?;
            frame.published.clone_from(&self.pending);
        }
        Ok(())
    }

    pub fn vegetation_spray(&self) -> &Buffer {
        &self.frames[self.slot].vegetation_spray
    }
    pub(super) fn water_input(
        &self,
        terrain_revision: Option<u32>,
    ) -> Option<&super::vegetation_response::water::PublishedSpray> {
        self.published_water.as_ref().filter(|p| {
            p.count > 0
                && p.emitter_revision == self.revision
                && p.terrain_revision == terrain_revision
                && terrain_revision.is_some()
        })
    }
    pub(super) fn publish_water_input(&mut self, terrain_revision: Option<u32>) {
        self.published_water = Some(super::vegetation_response::water::PublishedSpray {
            lobes: self.vegetation_spray().clone(),
            bounds: self.mist_bounds().clone(),
            count: self.instance_count,
            emitter_revision: self.revision,
            terrain_revision,
        });
    }

    pub fn instances(&self) -> &Buffer {
        &self.frames[self.slot].instances
    }

    pub fn mist_lobes(&self) -> &Buffer {
        &self.frames[self.slot].lobes
    }

    pub fn droplets(&self) -> &Buffer {
        &self.frames[self.slot].droplets
    }

    pub fn mist_bounds(&self) -> &Buffer {
        &self.frames[self.slot].bounds
    }

    pub fn water_contacts(&self) -> &Buffer {
        &self.frames[self.slot].water_contacts
    }

    pub fn water_bounds(&self) -> &Buffer {
        &self.frames[self.slot].water_bounds
    }

    pub fn air_info(&self) -> &Buffer {
        &self.frames[self.slot].air_info
    }

    pub fn air_density(&self) -> &Buffer {
        self.frames[self.slot]
            .air_density
            .as_ref()
            .expect("air field prepared in the acquired frame slot")
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
                kind: [instance.kind as u32, 0, 0, 0],
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn air_field_storage_and_integer_accumulation_are_bounded() {
        assert_eq!(SPRINKLER_AIR_GRID_BYTES, 3_538_944);
        assert!(SPRINKLER_AIR_GRID_SIDE > 4);
        assert_eq!(
            std::mem::size_of::<crate::generated::gpu_structs::SprinklerAirInfo>(),
            48
        );
        let maximum_deposits = SPRINKLER_RENDER_CAPACITY as u64 * u64::from(SPRINKLER_MIST_LOBES);
        assert!(maximum_deposits * 65_535 <= u64::from(u32::MAX));
    }

    #[test]
    fn sprinkler_pose_upload_is_bounded_finite_and_normalizes_phase() {
        let pose = SprinklerRenderInstance {
            base_position: Vec3::ONE,
            animation_phase: -0.25,
            kind: crate::sprinkler::Kind::Tall,
        };
        let encoded = encode_instances(&[pose]).unwrap();
        assert_eq!(std::mem::size_of::<SprinklerInstanceGpu>(), 32);
        assert_eq!(encoded[0].kind, [1, 0, 0, 0]);
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
        assert_eq!(sprinkler_mist_sample_count(false), 48);
        assert_eq!(sprinkler_mist_sample_count(true), 256);
        // Wetting's per-workgroup live-contact index list has this capacity.
        assert!(SPRINKLER_MIST_LOBES <= 256);
        assert!(sprinkler_mist_sample_count(true) % 8 == 0);
        for rotary in [false, true] {
            assert!(sprinkler_mist_sample_count(rotary) <= SPRINKLER_MIST_LOBES);
            // Preparation's 256 lanes cover every mist AND independent droplet
            // slot, including the partially occupied second iteration.
            for drops in [0, SPRINKLER_DROPLETS] {
                let mut visits = vec![0; (sprinkler_mist_sample_count(rotary) + drops) as usize];
                for lane in 0..256usize {
                    for sample in (lane..visits.len()).step_by(256) {
                        visits[sample] += 1;
                    }
                }
                assert!(visits.iter().all(|&n| n == 1));
            }
        }
        assert_eq!(
            std::mem::size_of::<crate::generated::gpu_structs::SprinklerMistLobe>(),
            64
        );
        assert_eq!(
            std::mem::size_of::<crate::generated::gpu_structs::SprinklerWaterContact>(),
            80
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
