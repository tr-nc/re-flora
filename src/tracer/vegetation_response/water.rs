//! Compact per-receiver surface water and motion. Immutable identity publication
//! is separate from frame-owned GPU state. No per-frame plant readback.
use super::*;
use glam::UVec3;
use std::sync::Arc;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Input {
    key: [u32; 4],
    history: [u32; 4],
    anchor: [u32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Step {
    dt: f32,
    count: u32,
    sources: u32,
    lobes: u32,
    reset: u32,
    remap: u32,
    enabled: u32,
    reserved: u32,
}
#[derive(Clone)]
enum Instances {
    Grass(Arc<[crate::builder::Instance]>),
    Tree(Arc<[crate::builder::TreeLeafInstance]>),
}
#[derive(Clone)]
struct Source {
    owner: u64,
    species: u32,
    origin: UVec3,
    instances: Instances,
}
impl Source {
    fn matches(&self, other: &Self) -> bool {
        self.owner == other.owner
            && self.species == other.species
            && self.origin == other.origin
            && match (&self.instances, &other.instances) {
                (Instances::Grass(a), Instances::Grass(b)) => Arc::ptr_eq(a, b),
                (Instances::Tree(a), Instances::Tree(b)) => Arc::ptr_eq(a, b),
                _ => false,
            }
    }
}
struct Frame {
    outputs: [Buffer; 2],
    index: usize,
    capacity: usize,
    // Keep immutable publications alive until this frame slot's fence.
    layout: Option<(Buffer, Buffer)>,
}
pub(in crate::tracer) struct PublishedSpray {
    pub lobes: Buffer,
    pub bounds: Buffer,
    pub count: u32,
    pub emitter_revision: u64,
    pub terrain_revision: Option<u32>,
}
#[derive(Default)]
pub(in crate::tracer) struct WaterResponse {
    sources: Vec<Source>,
    identities: HashMap<([u32; 4], u64), u32>,
    pub indices: HashMap<[u32; 4], u32>,
    inputs: Option<Buffer>,
    lookup: Option<Buffer>,
    dummy: Option<Buffer>,
    frames: Vec<Option<Frame>>,
    slot: usize,
    count: u32,
    leaf_revision: u64,
    previous: Option<Buffer>,
    last_time: Option<f32>,
    enabled: bool,
}
fn buffer(device: &Device, allocator: &Allocator, bytes: u64, host: bool) -> Result<Buffer> {
    Buffer::try_new_sized(
        device.clone(),
        allocator.clone(),
        BufferUsage::from_flags(
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC,
        ),
        if host {
            MemoryLocation::CpuToGpu
        } else {
            MemoryLocation::GpuOnly
        },
        bytes.max(80),
    )
}
fn unpack(p: u32) -> UVec3 {
    UVec3::new(p & 1023, (p >> 10) & 1023, (p >> 20) & 1023)
}
fn key(root: UVec3, species: u32) -> [u32; 4] {
    [root.x, root.y, root.z, species]
}
fn hash(k: [u32; 4]) -> u32 {
    let mut h = k[0].wrapping_mul(73856093)
        ^ k[1].wrapping_mul(19349663)
        ^ k[2].wrapping_mul(83492791)
        ^ k[3].wrapping_mul(2654435761);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb352d);
    h ^ (h >> 15)
}
fn lookup_table(indices: &HashMap<[u32; 4], u32>, leaf_map: &[u32]) -> Vec<[u32; 4]> {
    let capacity = (indices.len().max(1) * 2).next_power_of_two();
    let map_start = 1 + capacity * 2;
    let mut words = vec![[u32::MAX; 4]; map_start + leaf_map.len()];
    words[0] = [
        (capacity - 1) as u32,
        1,
        map_start as u32,
        leaf_map.len() as u32,
    ];
    for (&key, &index) in indices {
        let start = hash(key) as usize & (capacity - 1);
        let slot = (0..capacity)
            .map(|p| 1 + ((start + p) & (capacity - 1)) * 2)
            .find(|&p| words[p][3] == u32::MAX)
            .expect("half-full hash table");
        words[slot] = key;
        words[slot + 1] = [index, 0, 0, 0];
    }
    for (slot, &index) in leaf_map.iter().enumerate() {
        words[map_start + slot] = [index, 0, 0, 0];
    }
    words
}
type Identities = HashMap<([u32; 4], u64), u32>;
type Indices = HashMap<[u32; 4], u32>;
fn collect_inputs(sources: &[Source], previous: &Identities) -> (Vec<Input>, Identities, Indices) {
    let mut inputs = Vec::new();
    let mut identities = HashMap::new();
    let mut indices = HashMap::new();
    let mut append = |root: UVec3, anchor: UVec3, species: u32, lifetime: u64| {
        let key = key(root, species);
        // Coincident tissue shares the same physical water receiver, not an arbitrary draw slot.
        if indices.contains_key(&key) {
            return;
        }
        let index = inputs.len() as u32;
        indices.insert(key, index);
        identities.insert((key, lifetime), index);
        inputs.push(Input {
            key,
            history: [
                previous.get(&(key, lifetime)).copied().unwrap_or(u32::MAX),
                instance_seed(root),
                0,
                0,
            ],
            anchor: [anchor.x, anchor.y, anchor.z, 0],
        });
    };
    for source in sources {
        match &source.instances {
            Instances::Grass(instances) => {
                for instance in instances.iter() {
                    // Grass packs 8-bit axes plus growth; tree streams use 10-bit axes.
                    let p = instance.packed_local_pos;
                    let root = source.origin
                        + UVec3::new(p & 255, (p >> 8) & 255, (p >> 16) & 255)
                        + UVec3::Y;
                    append(
                        root,
                        root,
                        source.species,
                        u64::from(instance.spawn_start_ms),
                    );
                }
            }
            Instances::Tree(instances) => {
                for instance in instances.iter() {
                    let root = source.origin + unpack(instance.packed_local_pos);
                    let p = instance.packed_leaf_local_pos;
                    let local = unpack(p).as_ivec3() - glam::IVec3::splat(512);
                    let anchor = if source.species == 8 {
                        (root.as_ivec3() - local).as_uvec3()
                    } else {
                        root
                    };
                    append(root, anchor, source.species, source.owner);
                }
            }
        }
    }
    (inputs, identities, indices)
}

impl WaterResponse {
    fn publish_layout(
        &mut self,
        device: &Device,
        allocator: &Allocator,
        surface: &SurfaceResources,
        response: &VegetationResponse,
    ) -> Result<bool> {
        let mut sources = Vec::new();
        for (_, chunk) in &surface.instances.chunk_flora_instances {
            for species in 0..2 {
                sources.push(Source {
                    owner: 0,
                    species: species as u32,
                    origin: chunk.chunk_world_offset,
                    instances: Instances::Grass(chunk.grass_response_instances[species].clone()),
                });
            }
        }
        for (species, trees) in [
            (8, &surface.instances.leaves_instances),
            (9, &surface.instances.apple_instances),
        ] {
            let mut trees: Vec<_> = trees.iter().collect();
            trees.sort_by_key(|(id, _)| **id);
            for (&id, tree) in trees {
                sources.push(Source {
                    owner: u64::from(id) + 1,
                    species,
                    origin: tree.chunk_world_offset,
                    instances: Instances::Tree(tree.resources.response_instances.clone()),
                });
            }
        }
        if self.inputs.is_some()
            && self.leaf_revision == response.leaves.revision()
            && self.sources.len() == sources.len()
            && self.sources.iter().zip(&sources).all(|(a, b)| a.matches(b))
        {
            return Ok(false);
        }
        let (inputs, identities, indices) = collect_inputs(&sources, &self.identities);
        let mut leaf_map = vec![u32::MAX; response.leaves.base() + response.leaves.inputs().len()];
        for (i, input) in response.leaves.inputs().iter().enumerate() {
            let root = (glam::Vec3::from_slice(&input.root) * 256.)
                .round()
                .as_uvec3();
            leaf_map[response.leaves.base() + i] =
                indices.get(&key(root, 8)).copied().unwrap_or(u32::MAX);
        }
        let words = lookup_table(&indices, &leaf_map);
        let input_buffer = buffer(
            device,
            allocator,
            (inputs.len().max(1) * std::mem::size_of::<Input>()) as u64,
            true,
        )?;
        if !inputs.is_empty() {
            input_buffer.fill(&inputs)?;
        }
        let lookup = buffer(device, allocator, (words.len() * 16) as u64, true)?;
        lookup.fill(&words)?;
        self.count = inputs.len() as u32;
        self.sources = sources;
        self.identities = identities;
        self.indices = indices;
        self.inputs = Some(input_buffer);
        self.lookup = Some(lookup);
        self.leaf_revision = response.leaves.revision();
        log::info!("[VEGETATION_WATER][LAYOUT] receivers={} input_bytes={} lookup_bytes={} readback=publication_only",self.count,inputs.len()*48,words.len()*16);
        Ok(true)
    }
    pub fn record(
        &mut self,
        device: Device,
        allocator: Allocator,
        pipeline: &ComputePipeline,
        cmdbuf: &CommandBuffer,
        surface: &SurfaceResources,
        response: &VegetationResponse,
        slot: usize,
        time: f32,
        enabled: bool,
        spray: Option<&PublishedSpray>,
    ) -> Result<()> {
        if self.dummy.is_none() {
            let dummy = buffer(&device, &allocator, 80, true)?;
            dummy.fill(&[0u32; 20])?;
            self.dummy = Some(dummy);
        }
        let remap = enabled && self.publish_layout(&device, &allocator, surface, response)?;
        let reset = !self.enabled
            || self.previous.is_none()
            || self.last_time.is_none_or(|last| time < last);
        let dt = self.last_time.map_or(0., |last| (time - last).max(0.));
        self.frames
            .resize_with(self.frames.len().max(slot + 1), || None);
        let count = if enabled { self.count } else { 0 };
        if self.frames[slot]
            .as_ref()
            .is_none_or(|f| f.capacity < (count as usize).max(1))
        {
            let capacity = (count as usize).max(1).next_power_of_two();
            self.frames[slot] = Some(Frame {
                outputs: [
                    buffer(&device, &allocator, capacity as u64 * 32, false)?,
                    buffer(&device, &allocator, capacity as u64 * 32, false)?,
                ],
                index: 0,
                capacity,
                layout: None,
            });
            log::info!(
                "[VEGETATION_WATER][MEMORY] slot={slot} capacity={capacity} state_bytes={}",
                capacity * 64
            );
        }
        let frame = self.frames[slot].as_mut().unwrap();
        frame.index ^= 1;
        frame.layout = if enabled {
            Some((
                self.inputs.as_ref().unwrap().clone(),
                self.lookup.as_ref().unwrap().clone(),
            ))
        } else {
            None
        };
        let output = &frame.outputs[frame.index];
        let dummy = self.dummy.as_ref().unwrap();
        let step = Step {
            dt,
            count,
            sources: spray.map_or(0, |s| s.count),
            lobes: super::super::SPRINKLER_MIST_LOBES,
            reset: u32::from(reset),
            remap: u32::from(remap),
            enabled: u32::from(enabled),
            reserved: 0,
        };
        pipeline.begin_transient_descriptor_frame(slot);
        if count > 0 {
            pipeline.record_with_descriptors(
                cmdbuf,
                &[
                    (
                        "water_inputs",
                        DescriptorResource::Buffer(self.inputs.as_ref().unwrap()),
                    ),
                    (
                        "water_previous",
                        DescriptorResource::Buffer(self.previous.as_ref().unwrap_or(dummy)),
                    ),
                    ("water_output", DescriptorResource::Buffer(output)),
                    (
                        "water_spray",
                        DescriptorResource::Buffer(spray.map_or(dummy, |s| &s.lobes)),
                    ),
                    (
                        "water_spray_bounds",
                        DescriptorResource::Buffer(spray.map_or(dummy, |s| &s.bounds)),
                    ),
                ],
                Extent3D::new(count, 1, 1),
                Some(bytemuck::bytes_of(&step)),
            )?;
        }
        self.previous = Some(output.clone());
        self.last_time = Some(time);
        self.enabled = enabled;
        self.slot = slot;
        Ok(())
    }
    pub fn descriptors(&self) -> [(&'static str, DescriptorResource<'_>); 2] {
        [
            (
                "vegetation_water_lookup",
                DescriptorResource::Buffer(if self.enabled {
                    self.lookup.as_ref().unwrap()
                } else {
                    self.dummy.as_ref().unwrap()
                }),
            ),
            (
                "vegetation_water_states",
                DescriptorResource::Buffer(self.previous.as_ref().unwrap()),
            ),
        ]
    }
    pub fn invalidate_history(&mut self) {
        self.last_time = None;
        self.previous = None;
    }
    pub fn diagnostic_source(&self) -> Option<(&Buffer, u32)> {
        self.enabled
            .then(|| self.previous.as_ref().map(|b| (b, self.count)))
            .flatten()
    }
    /// Explicit legacy visual fixture only; live releases use sparse frame copies.
    pub fn read_fruit_poses(
        &self,
        context: &re_flora_vkn::VulkanContext,
        allocator: Allocator,
        roots: &[UVec3],
    ) -> Result<Vec<(glam::Vec3, glam::Vec3)>> {
        use re_flora_vkn::{execute_one_time_command, BufferUse};
        if roots.is_empty() || !self.enabled {
            return Ok(vec![(glam::Vec3::ZERO, glam::Vec3::ZERO); roots.len()]);
        }
        let bytes = roots.len() as u64 * 32;
        let readback = Buffer::try_new_sized(
            context.device().clone(),
            allocator,
            BufferUsage::from_flags(vk::BufferUsageFlags::TRANSFER_DST),
            MemoryLocation::GpuToCpu,
            bytes,
        )?;
        execute_one_time_command(
            context.device(),
            context.command_pool(),
            &context.get_general_queue(),
            |cmd| -> Result<()> {
                readback.record_fill(cmd, 0, bytes, 0);
                for (index, &root) in roots.iter().enumerate() {
                    if let Some((source, offset)) = self.fruit_source(root) {
                        source.record_copy_to_buffer(cmd, &readback, 32, offset, index as u64 * 32);
                    }
                }
                cmd.use_buffer(&readback, BufferUse::HostRead);
                Ok(())
            },
        )?;
        Ok(readback
            .read_back_range(0, bytes)?
            .chunks_exact(32)
            .map(|b| {
                let state: [f32; 8] = bytemuck::pod_read_unaligned(b);
                (
                    glam::Vec3::from_slice(&state[..3]),
                    glam::Vec3::from_slice(&state[4..7]),
                )
            })
            .collect())
    }

    pub fn fruit_source(&self, root: UVec3) -> Option<(Buffer, u64)> {
        self.enabled
            .then(|| {
                self.indices
                    .get(&key(root, 9))
                    .and_then(|&i| self.previous.clone().map(|b| (b, u64::from(i) * 32)))
            })
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn grass(values: &[(u32, u32, u32)]) -> Source {
        Source {
            owner: 0,
            species: 0,
            origin: UVec3::new(256, 0, 256),
            instances: Instances::Grass(
                values
                    .iter()
                    .map(|&(x, birth, growth)| crate::builder::Instance {
                        packed_local_pos: x | (127 << 8) | (30 << 16) | (growth << 24),
                        spawn_start_ms: birth,
                    })
                    .collect::<Vec<_>>()
                    .into(),
            ),
        }
    }
    #[test]
    fn draw_reorder_growth_and_deletion_do_not_transfer_water_to_a_new_plant() {
        let (first, previous, _) = collect_inputs(
            &[grass(&[(10, 100, 10), (20, 200, 255), (30, 300, 255)])],
            &HashMap::new(),
        );
        assert_eq!(first[0].key, [266, 128, 286, 0]);
        assert!(first.iter().all(|i| i.history[0] == u32::MAX));
        let (second, current, _) = collect_inputs(
            &[grass(&[(30, 300, 255), (10, 100, 255), (40, 400, 255)])],
            &previous,
        );
        assert_eq!(
            second.iter().map(|i| i.history[0]).collect::<Vec<_>>(),
            [2, 0, u32::MAX]
        );
        let (third, _, _) = collect_inputs(&[grass(&[(20, 200, 255), (10, 999, 255)])], &current);
        assert!(third.iter().all(|i| i.history[0] == u32::MAX));
        let mut short = grass(&[(10, 100, 255)]);
        short.species = 1;
        assert_eq!(
            collect_inputs(&[short], &previous).0[0].history[0],
            u32::MAX
        );
    }
    #[test]
    fn tree_height_anchor_and_lifetime_are_not_flattened_into_xz() {
        let mut source = Source {
            owner: 7,
            species: 8,
            origin: UVec3::splat(256),
            instances: Instances::Tree(Arc::from([
                crate::builder::TreeLeafInstance {
                    packed_local_pos: 10 | (20 << 10) | (30 << 20),
                    packed_leaf_local_pos: 513 | (510 << 10) | (515 << 20),
                },
                crate::builder::TreeLeafInstance {
                    packed_local_pos: 10 | (40 << 10) | (30 << 20),
                    packed_leaf_local_pos: 513 | (510 << 10) | (515 << 20),
                },
            ])),
        };
        let (first, previous, indices) = collect_inputs(&[source.clone()], &HashMap::new());
        assert_eq!(indices.len(), 2);
        assert_eq!(first[0].anchor, [265, 278, 283, 0]);
        source.owner = 8;
        assert!(collect_inputs(&[source], &previous)
            .0
            .iter()
            .all(|i| i.history[0] == u32::MAX));
    }
    #[test]
    fn table_keeps_full_height_species_keys_and_resolves_collisions() {
        let indices: HashMap<_, _> = (0..10000)
            .map(|i| ([i % 257, i / 257, 57, i % 2], i))
            .collect();
        let words = lookup_table(&indices, &[17, u32::MAX]);
        let mask = words[0][0] as usize;
        for (&key, &expected) in &indices {
            let index = (0..=mask)
                .map(|p| 1 + ((hash(key) as usize + p) & mask) * 2)
                .find(|&p| words[p] == key)
                .unwrap();
            assert_eq!(words[index + 1][0], expected);
        }
        assert_eq!(words[words[0][2] as usize][0], 17);
        assert_eq!(std::mem::size_of::<Input>(), 48);
        assert_eq!(std::mem::size_of::<Step>(), 32);
    }
}
