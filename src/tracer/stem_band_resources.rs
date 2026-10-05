//! Resident CPU-path instance stream. Simulation ownership stays with callers.
use super::stem_band_paths::StemBandInstance;
use crate::resource::Resource;
use anyhow::{ensure, Result};
use re_flora_vkn::{
    vk, Allocator, Buffer, BufferUsage, Device, FrameRetirement, FrameRetirementSink,
    MemoryLocation,
};

pub struct StemBandResources {
    device: Device,
    allocator: Allocator,
    retirement: FrameRetirementSink,
    generation: u64,
    capacity: usize,
    pub instances: Resource<Buffer>,
    pub count: u32,
    previous: Vec<StemBandInstance>,
    shadow_changed: bool,
}
impl StemBandResources {
    pub fn new(device: Device, allocator: Allocator, retirement: FrameRetirementSink) -> Self {
        let buffer = make_buffer(&device, &allocator, 1);
        Self {
            device,
            allocator,
            retirement,
            generation: 1,
            capacity: 1,
            instances: Resource::new(buffer),
            count: 0,
            previous: Vec::new(),
            shadow_changed: false,
        }
    }
    pub fn take_shadow_changed(&mut self) -> bool {
        std::mem::take(&mut self.shadow_changed)
    }
    pub fn show(&mut self, instances: &[StemBandInstance]) -> Result<()> {
        ensure!(
            instances.len() <= u32::MAX as usize,
            "stem band draw range overflow"
        );
        for instance in instances {
            let words: &[f32] = bytemuck::cast_slice(std::slice::from_ref(instance));
            ensure!(
                words.iter().all(|v| v.is_finite())
                    && instance.a_radius[3] > 0.
                    && instance.b_radius[3] > 0.,
                "invalid stem band instance"
            );
        }
        if self.previous == instances {
            return Ok(());
        }
        if instances.len() > self.capacity {
            let capacity = instances
                .len()
                .checked_next_power_of_two()
                .expect("stem capacity overflow");
            let buffer = make_buffer(&self.device, &self.allocator, capacity);
            let old = std::mem::replace(&mut *self.instances, buffer);
            self.retirement.retire(FrameRetirement::new(
                "stem_bands.instances",
                self.generation,
                old,
            ));
            self.generation = self
                .generation
                .checked_add(1)
                .expect("stem generation overflow");
            self.capacity = capacity;
        }
        if !instances.is_empty() {
            self.instances.fill(instances)?;
        }
        self.shadow_changed = true;
        self.count = instances.len() as u32;
        self.previous.clear();
        self.previous.extend_from_slice(instances);
        Ok(())
    }
}
fn make_buffer(device: &Device, allocator: &Allocator, capacity: usize) -> Buffer {
    Buffer::new_sized(
        device.clone(),
        allocator.clone(),
        BufferUsage::from_flags(vk::BufferUsageFlags::VERTEX_BUFFER),
        MemoryLocation::CpuToGpu,
        (capacity * std::mem::size_of::<StemBandInstance>()) as u64,
    )
}
