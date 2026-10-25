//! Bounded GPU flower-pose publication. Readback is collected only after the
//! acquired frame slot's existing fence; no query submission or CPU/GPU wait.
use super::*;
use crate::particles::flower_visits::{FlowerId, FlowerTarget};
use bytemuck::{Pod, Zeroable};
use re_flora_vkn::Device;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

const CAPACITY: usize = 512;
const POSE_BYTES: u64 = 48;
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Pose {
    position_radius: [f32; 4],
    normal_growth: [f32; 4],
    tangent_valid: [f32; 4],
}
#[derive(Clone, Copy)]
struct Request {
    id: FlowerId,
    chunk: usize,
    species: u32,
    plant_slot: u32,
    distance: f32,
}
struct Pending {
    requests: Vec<Request>,
    time: f32,
    epoch: u64,
}
struct Bank {
    requests: Arc<Buffer>,
    output: Arc<Buffer>,
    readback: Arc<Buffer>,
    pending: Option<Pending>,
}
impl Bank {
    fn new(device: Device, allocator: Allocator) -> Result<Self> {
        let buffer = |flags, location, bytes| {
            Buffer::try_new_sized(
                device.clone(),
                allocator.clone(),
                BufferUsage::from_flags(flags),
                location,
                bytes,
            )
            .map(Arc::new)
        };
        Ok(Self {
            requests: buffer(
                vk::BufferUsageFlags::STORAGE_BUFFER,
                MemoryLocation::CpuToGpu,
                CAPACITY as u64 * 16,
            )?,
            output: buffer(
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC,
                MemoryLocation::GpuOnly,
                CAPACITY as u64 * POSE_BYTES,
            )?,
            readback: buffer(
                vk::BufferUsageFlags::TRANSFER_DST,
                MemoryLocation::GpuToCpu,
                CAPACITY as u64 * POSE_BYTES,
            )?,
            pending: None,
        })
    }
}
#[derive(Default)]
pub(super) struct FlowerVisits {
    enabled: bool,
    epoch: u64,
    banks: Vec<Bank>,
    current: HashMap<FlowerId, u32>,
    latest: Vec<FlowerTarget>,
    latest_time: f32,
    observation_time: f32,
    reported: bool,
}
impl FlowerVisits {
    fn clear(&mut self) {
        self.epoch = self.epoch.checked_add(1).expect("flower epoch overflow");
        self.current.clear();
        self.latest.clear();
        self.latest_time = 0.;
        self.reported = false;
    }
    pub(super) fn output(&self, slot: usize) -> Arc<Buffer> {
        self.banks[slot].output.clone()
    }
    pub(super) fn indices(&self) -> &HashMap<FlowerId, u32> {
        &self.current
    }
}
fn plan(
    surface: &SurfaceResources,
    epoch: u64,
    camera: Vec3,
    held: &HashSet<FlowerId>,
) -> Vec<Request> {
    let mut requests = Vec::new();
    for (chunk, (_, instances)) in surface.instances.chunk_flora_instances.iter().enumerate() {
        let mut slots = [0; species::MAX_FLORA_SPECIES];
        for plant in &instances.authored_response_instances {
            let species = plant.species_index as usize;
            let slot = slots[species];
            slots[species] += 1;
            let Some(model) = species::species()[species].model_flower else {
                continue;
            };
            let distance = (plant.base_world_vox.as_vec3() / 256.).distance_squared(camera);
            for head in 0..crate::flora::models::flowers()[model].heads.len() as u32 {
                let id = FlowerId {
                    epoch,
                    plant: plant.response_id,
                    head,
                };
                if distance <= 1.8_f32.powi(2) || held.contains(&id) {
                    requests.push(Request {
                        id,
                        chunk,
                        species: species as u32,
                        plant_slot: slot,
                        distance: if held.contains(&id) { -1. } else { distance },
                    });
                }
            }
        }
    }
    requests.sort_by(|a, b| a.distance.total_cmp(&b.distance).then(a.id.cmp(&b.id)));
    requests.truncate(CAPACITY);
    requests.sort_by_key(|r| (r.chunk, r.species, r.plant_slot, r.id.head));
    requests
}
impl Tracer {
    pub(crate) fn set_flower_visits_enabled(&mut self, enabled: bool) {
        if self.flower_visits.enabled != enabled {
            self.flower_visits.clear();
            self.flower_visits.enabled = enabled;
            log::info!("[BUTTERFLY_FORAGING] enabled={enabled} source=shared_gpu_flower_pose");
        }
    }
    pub(crate) fn invalidate_flower_visits(&mut self) {
        self.flower_visits.clear();
    }
    pub(crate) fn flower_visit_targets(
        &mut self,
        surface: &SurfaceResources,
        time: f32,
    ) -> &[FlowerTarget] {
        // Freshness/velocity use the simulation observation clock, not the
        // shader animation time (which capture/lighting fixtures can freeze).
        self.flower_visits.observation_time = time;
        if !self.flower_visits.enabled {
            return &[];
        }
        let live: HashSet<_> = surface
            .instances
            .chunk_flora_instances
            .iter()
            .flat_map(|(_, r)| r.authored_response_instances.iter().map(|p| p.response_id))
            .collect();
        self.flower_visits.latest.retain(|t| {
            live.contains(&t.id.plant)
                && time - t.observed_at <= 0.5
                && t.id.epoch == self.flower_visits.epoch
        });
        &self.flower_visits.latest
    }
    /// The caller has just acquired this slot and waited its old fence.
    pub(crate) fn complete_flower_visits_slot(&mut self, slot: usize) -> Result<()> {
        let sampler = &mut self.flower_visits;
        let Some(bank) = sampler.banks.get_mut(slot) else {
            return Ok(());
        };
        let Some(pending) = bank.pending.take() else {
            return Ok(());
        };
        if pending.epoch != sampler.epoch || pending.time < sampler.latest_time {
            return Ok(());
        }
        let bytes = bank
            .readback
            .read_back_range(0, pending.requests.len() as u64 * POSE_BYTES)?;
        let poses: &[Pose] = bytemuck::try_cast_slice(&bytes)
            .map_err(|e| anyhow::anyhow!("flower pose readback ABI: {e}"))?;
        let previous: HashMap<_, _> = sampler.latest.iter().map(|t| (t.id, *t)).collect();
        sampler.latest.clear();
        for (r, pose) in pending.requests.iter().zip(poses) {
            let position = Vec3::from_slice(&pose.position_radius);
            let normal = Vec3::from_slice(&pose.normal_growth);
            let tangent = Vec3::from_slice(&pose.tangent_valid);
            if pose.tangent_valid[3] < 0.5 {
                continue;
            }
            anyhow::ensure!(
                position.is_finite()
                    && normal.is_finite()
                    && tangent.is_finite()
                    && pose.position_radius[3].is_finite()
                    && pose.position_radius[3] > 0.
                    && (normal.length_squared() - 1.).abs() < 1e-3,
                "invalid GPU flower contact"
            );
            let velocity = previous
                .get(&r.id)
                .map_or(Vec3::ZERO, |old| {
                    let dt = pending.time - old.observed_at;
                    if dt > 1e-5 && dt < 0.5 {
                        (position - old.position) / dt
                    } else {
                        Vec3::ZERO
                    }
                })
                .clamp_length_max(0.3);
            sampler.latest.push(FlowerTarget {
                id: r.id,
                species: r.species,
                position,
                normal,
                tangent,
                radius: pose.position_radius[3],
                velocity,
                observed_at: pending.time,
            });
        }
        sampler.latest_time = pending.time;
        if !sampler.latest.is_empty() && !sampler.reported {
            sampler.reported = true;
            log::info!("[BUTTERFLY_FORAGING][CONTACTS] count={} bytes={} frame_fence=true query_wait=false authored_surface=true", sampler.latest.len(), bytes.len());
        }
        Ok(())
    }
    pub(super) fn record_flower_visits(
        &mut self,
        cmd: &CommandBuffer,
        surface: &SurfaceResources,
        time: f32,
        slot: usize,
        flora_visible: bool,
    ) -> Result<()> {
        let enabled = self.flower_visits.enabled
            || std::env::var_os("RE_FLORA_FLOWER_CONTACT_REVIEW").is_some();
        let sampler = &mut self.flower_visits;
        while sampler.banks.len() <= slot {
            sampler.banks.push(Bank::new(
                self.vulkan_ctx.device().clone(),
                self.allocator.clone(),
            )?);
        }
        let held = self.model_mesh_frame.flower_hosts();
        let requests = if enabled && flora_visible {
            plan(surface, sampler.epoch, self.camera.position(), &held)
        } else {
            Vec::new()
        };
        sampler.current = requests
            .iter()
            .enumerate()
            .map(|(i, r)| (r.id, i as u32))
            .collect();
        let bank = &mut sampler.banks[slot];
        // The empty original mode binds a valid dummy buffer but records no work.
        if requests.is_empty() {
            sampler.latest.clear();
            return Ok(());
        }
        let inputs: Vec<[u32; 4]> = requests
            .iter()
            .map(|r| [r.plant_slot, r.id.head, 0, 0])
            .collect();
        bank.requests
            .fill_range_with_raw_u8(0, bytemuck::cast_slice(&inputs))?;
        let parts = self.model_mesh_frame.flower_parts_buffer();
        let pipeline = &self.pipeline_topology.compute().flower_visit_ppl;
        pipeline.begin_transient_descriptor_frame(slot);
        let mut first = 0;
        while first < requests.len() {
            let r = requests[first];
            let count = requests[first..]
                .iter()
                .take_while(|q| q.chunk == r.chunk && q.species == r.species)
                .count();
            let chunk = &surface.instances.chunk_flora_instances[r.chunk].1;
            let push = crate::generated::gpu_structs::PushConstantFlowerPixel {
                chunk_world_offset: chunk.chunk_world_offset.to_array(),
                species: r.species,
                response_offset: self
                    .vegetation_response
                    .flower_offset(r.chunk, r.species as usize),
                time,
                world_scale: crate::flora::models::WORLD_SCALE
                    * self.flower_model_settings.size_scale,
                tile_first: first as u32,
                prepare_object: count as u32,
                resolution: 0,
                padding: 0,
                _pad0: Default::default(),
            };
            pipeline.record_with_descriptors(
                cmd,
                &[
                    (
                        "flora_instances",
                        DescriptorResource::Buffer(&chunk.resource.instances_buf),
                    ),
                    (
                        "plant_hydration_values",
                        DescriptorResource::Buffer(&chunk.plant_hydration),
                    ),
                    ("flower_parts", DescriptorResource::Buffer(&parts)),
                    (
                        "flower_visit_requests",
                        DescriptorResource::Buffer(&bank.requests),
                    ),
                    (
                        "flower_visit_output",
                        DescriptorResource::Buffer(&bank.output),
                    ),
                    self.vegetation_response.descriptors()[0],
                    self.vegetation_response.descriptors()[1],
                ],
                Extent3D::new(count as u32, 1, 1),
                Some(bytemuck::bytes_of(&push)),
            )?;
            first += count;
        }
        bank.output.record_copy_to_buffer(
            cmd,
            &bank.readback,
            requests.len() as u64 * POSE_BYTES,
            0,
            0,
        );
        cmd.use_buffer(&bank.readback, BufferUse::HostRead);
        bank.pending = Some(Pending {
            requests,
            time: sampler.observation_time,
            epoch: sampler.epoch,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn abi_and_epochs_are_bounded_and_invalidate_old_hosts() {
        assert_eq!(std::mem::size_of::<Pose>() as u64, POSE_BYTES);
        let mut visits = FlowerVisits::default();
        let before = visits.epoch;
        visits.clear();
        assert_ne!(before, visits.epoch);
        assert!(visits.latest.is_empty() && visits.current.is_empty());
    }
}
