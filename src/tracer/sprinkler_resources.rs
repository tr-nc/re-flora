//! Staged sprinkler poses. Only an acquired frame slot may publish GPU instances.
use anyhow::{ensure, Result};
use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use re_flora_vkn::{vk, Allocator, Buffer, BufferUsage, Device, MemoryLocation};

pub const SPRINKLER_RENDER_CAPACITY: usize = 256;

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

pub struct SprinklerRendererResources {
    device: Device,
    allocator: Allocator,
    pending: Vec<SprinklerInstanceGpu>,
    frames: Vec<(Buffer, Vec<SprinklerInstanceGpu>)>,
    slot: usize,
    pub instance_count: u32,
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

    pub fn prepare_frame(&mut self, slot: usize) -> Result<()> {
        self.slot = slot;
        if self.pending.is_empty() {
            return Ok(());
        }
        while self.frames.len() <= slot {
            self.frames.push((
                Buffer::try_new_sized(
                    self.device.clone(),
                    self.allocator.clone(),
                    BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
                    MemoryLocation::CpuToGpu,
                    (std::mem::size_of::<SprinklerInstanceGpu>() * SPRINKLER_RENDER_CAPACITY)
                        as u64,
                )?,
                Vec::new(),
            ));
        }
        let (buffer, published) = &mut self.frames[slot];
        if *published != self.pending {
            buffer.fill(&self.pending)?;
            published.clone_from(&self.pending);
        }
        Ok(())
    }

    pub fn instances(&self) -> &Buffer {
        &self.frames[self.slot].0
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
}
