//! Small GPU scene-query answers, including exact barycentric rest-space wood.
//! Full wood vertices never cross back to the CPU for picking.
use super::{TerrainRayQuery, Tracer};
use anyhow::{ensure, Result};
use glam::Vec3;
use re_flora_vkn::{
    vk, AcquiredFrame, Allocator, Buffer, BufferUsage, BufferUse, CommandBuffer,
    DescriptorResource, DescriptorWrite, Device, Extent3D, FrameReadbackProbe, MemoryLocation,
    PreparedDescriptorGeneration,
};
use std::collections::VecDeque;

pub(super) const PICK_BYTES: u64 = 48;
const NO_PRIMITIVE: u32 = u32::MAX;

// Structured-buffer element ABI in terrain_query.slang::B_SurfacePickResult.
#[repr(C, align(16))]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct PickRaw {
    pub position: [f32; 4],
    pub rest_position: [f32; 4],
    pub metadata: [u32; 4],
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SurfacePickHit {
    pub world_position: Vec3,
    pub rest_position: Vec3,
    pub distance: f32,
    pub voxel_type: u32,
    pub tree_primitive: Option<u32>,
}
impl SurfacePickHit {
    pub fn tree_rest_position(self) -> Option<Vec3> {
        self.tree_primitive.map(|_| self.rest_position)
    }
}

fn decode(raw: PickRaw) -> Result<Option<SurfacePickHit>> {
    if raw.metadata[0] == 0 {
        return Ok(None);
    }
    let hit = SurfacePickHit {
        world_position: Vec3::from_slice(&raw.position),
        rest_position: Vec3::from_slice(&raw.rest_position),
        distance: raw.position[3],
        voxel_type: raw.metadata[2],
        tree_primitive: (raw.metadata[1] != NO_PRIMITIVE).then_some(raw.metadata[1]),
    };
    ensure!(
        raw.metadata[0] == 1
            && hit.world_position.is_finite()
            && hit.rest_position.is_finite()
            && hit.distance.is_finite()
            && hit.distance >= 0.
            && (1..=15).contains(&hit.voxel_type),
        "invalid GPU surface pick result"
    );
    ensure!(
        hit.tree_primitive.is_none() || hit.voxel_type == 5,
        "GPU tree pick has non-wood material"
    );
    Ok(Some(hit))
}

impl Tracer {
    /// Explicit, on-demand query for edits/validation. Hover will use a frame
    /// readback instead; never call this blocking path once per hover frame.
    pub(crate) fn query_surface_pick_now(
        &mut self,
        ray: TerrainRayQuery,
    ) -> Result<Option<SurfacePickHit>> {
        if !ray.origin.is_finite()
            || !ray.direction.is_finite()
            || ray.direction.length_squared() < 1e-8
        {
            return Ok(None);
        }
        self.resources
            .terrain_query
            .terrain_query_count
            .fill(&[1u32, 0, 0, 0])?;
        self.resources.terrain_query.terrain_query_info.fill(&[[
            ray.origin.x,
            ray.origin.y,
            ray.origin.z,
            -1.,
            ray.direction.x,
            ray.direction.y,
            ray.direction.z,
            0.,
        ]])?;
        let command = CommandBuffer::new(self.vulkan_ctx.device(), self.vulkan_ctx.command_pool());
        command.begin(true);
        self.record_tree_skin(&command);
        self.record_surface_pick_query(&command);
        self.resources
            .terrain_query
            .terrain_pick_result
            .record_copy_to_buffer(
                &command,
                &self.resources.terrain_query.terrain_pick_readback,
                PICK_BYTES,
                0,
                0,
            );
        command.use_buffer(
            &self.resources.terrain_query.terrain_pick_readback,
            BufferUse::HostRead,
        );
        command.end();
        command
            .submit_gpu_job(&self.vulkan_ctx.get_general_queue(), "surface.pick.edit")?
            .wait_complete()?;
        let bytes = self
            .resources
            .terrain_query
            .terrain_pick_readback
            .read_back_range(0, PICK_BYTES)?;
        let hit = decode(bytemuck::pod_read_unaligned(&bytes))?;
        if let Some(primitive) = hit.and_then(|h| h.tree_primitive) {
            ensure!(
                (primitive as usize) < self.raster_trees.scene.primitives.len(),
                "GPU pick primitive outside publication"
            );
        }
        Ok(hit)
    }

    fn record_surface_pick_query(&self, command: &CommandBuffer) {
        for buffer in [
            &self.resources.terrain_query.terrain_query_count,
            &self.resources.terrain_query.terrain_query_info,
        ] {
            command.use_buffer(buffer, BufferUse::HostWrite);
        }
        self.pipeline_topology.compute().terrain_query_ppl.record(
            command,
            Extent3D::new(1, 1, 1),
            None,
        );
    }
}

// Two rays per frame: one captured edit and a latest-wins hover. Per-slot
// mapped inputs are not rewritten until acquire has completed their readers.
const BATCH: usize = 2;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct PickTicket(u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PickPurpose {
    Hover,
    Edit,
    Kick,
    Validation,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SceneKey {
    terrain: u32,
    trees: u64,
    enabled: bool,
    wind: bool,
}
#[derive(Clone, Copy)]
struct Request {
    ticket: PickTicket,
    ray: TerrainRayQuery,
    key: SceneKey,
    purpose: PickPurpose,
    reference: Option<crate::tree_gen::skin::SurfaceHit>,
}
pub(crate) struct PickAnswer {
    pub ticket: PickTicket,
    pub ray: TerrainRayQuery,
    pub purpose: PickPurpose,
    pub hit: Option<SurfacePickHit>,
    key: SceneKey,
    stale: bool,
}
impl PickAnswer {
    /// Opaque transport facts for gameplay-session tests; scene freshness itself
    /// remains covered by this module's real SceneKey checks.
    #[cfg(test)]
    pub(crate) fn test_fixture(
        ticket: u64,
        purpose: PickPurpose,
        ray: TerrainRayQuery,
        hit: Option<SurfacePickHit>,
    ) -> Self {
        Self {
            ticket: PickTicket(ticket),
            purpose,
            ray,
            hit,
            key: SceneKey {
                terrain: 0,
                trees: 0,
                enabled: true,
                wind: true,
            },
            stale: false,
        }
    }
    fn matches(&self, key: SceneKey) -> bool {
        !self.stale && self.key == key
    }
    fn stale(request: Request) -> Self {
        Self {
            ticket: request.ticket,
            ray: request.ray,
            purpose: request.purpose,
            hit: None,
            key: request.key,
            stale: true,
        }
    }
}
struct Flight {
    requests: Vec<Request>,
    probe: FrameReadbackProbe,
    _descriptors: PreparedDescriptorGeneration,
}
struct Bank {
    count: Buffer,
    rays: Buffer,
    readback: Buffer,
    flight: Option<Flight>,
}
impl Bank {
    fn new(device: Device, allocator: Allocator) -> Result<Self> {
        let buffer = |usage, location, bytes| {
            Buffer::try_new_sized(
                device.clone(),
                allocator.clone(),
                BufferUsage::from_flags(usage),
                location,
                bytes,
            )
        };
        Ok(Self {
            count: buffer(
                vk::BufferUsageFlags::UNIFORM_BUFFER,
                MemoryLocation::CpuToGpu,
                16,
            )?,
            rays: buffer(
                vk::BufferUsageFlags::STORAGE_BUFFER,
                MemoryLocation::CpuToGpu,
                BATCH as u64 * 32,
            )?,
            readback: buffer(
                vk::BufferUsageFlags::TRANSFER_DST,
                MemoryLocation::GpuToCpu,
                BATCH as u64 * PICK_BYTES,
            )?,
            flight: None,
        })
    }
    fn collect(&mut self, answers: &mut VecDeque<PickAnswer>) -> Result<()> {
        let Some(flight) = self.flight.take() else {
            return Ok(());
        };
        let bytes = self
            .readback
            .read_back_range(0, flight.requests.len() as u64 * PICK_BYTES)?;
        for (request, bytes) in flight
            .requests
            .into_iter()
            .zip(bytes.chunks_exact(PICK_BYTES as usize))
        {
            let hit = decode(bytemuck::pod_read_unaligned(bytes))?;
            if request.purpose == PickPurpose::Validation {
                let reference = request
                    .reference
                    .ok_or_else(|| anyhow::anyhow!("async pick reference missed"))?;
                let pick = hit.ok_or_else(|| anyhow::anyhow!("async GPU pick missed"))?;
                ensure!(
                    pick.tree_primitive.is_some()
                        && pick.world_position.distance(reference.world_position) < 3e-5
                        && pick.rest_position.distance(reference.rest_position) < 3e-5,
                    "async GPU pick differs from same-frame CPU reference"
                );
                log::info!("[SURFACE_PICK][ASYNC] passed world=true rest=true bytes=48 frame_fence=true query_wait=false");
            }
            answers.push_back(PickAnswer {
                ticket: request.ticket,
                ray: request.ray,
                purpose: request.purpose,
                key: request.key,
                stale: false,
                hit,
            });
        }
        Ok(())
    }
}
#[derive(Default)]
pub(super) struct SurfacePicker {
    generation: u64,
    next_ticket: u64,
    queued: VecDeque<Request>,
    answers: VecDeque<PickAnswer>,
    banks: Vec<Bank>,
}
impl SurfacePicker {
    pub(super) fn generation(&self) -> u64 {
        self.generation
    }
    pub fn invalidate_geometry(&mut self) {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("pick generation overflow");
    }
    fn enqueue(
        &mut self,
        ray: TerrainRayQuery,
        key: SceneKey,
        purpose: PickPurpose,
    ) -> Result<PickTicket> {
        ensure!(
            ray.origin.is_finite()
                && ray.direction.is_finite()
                && ray.direction.length_squared() >= 1e-8,
            "invalid pick ray"
        );
        if purpose == PickPurpose::Hover {
            self.queued.retain(|r| r.purpose != PickPurpose::Hover);
        }
        ensure!(
            self.queued.len() < BATCH && self.answers.len() < 16,
            "surface pick queue full"
        );
        self.next_ticket = self
            .next_ticket
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("pick ticket overflow"))?;
        let ticket = PickTicket(self.next_ticket);
        self.queued.push_back(Request {
            ticket,
            ray,
            key,
            purpose,
            reference: None,
        });
        Ok(ticket)
    }
}
impl Tracer {
    pub(crate) fn cancel_surface_pick(&mut self, ticket: PickTicket) {
        self.surface_picker.queued.retain(|r| r.ticket != ticket);
    }
    fn pick_scene_key(&self, terrain: u32) -> SceneKey {
        SceneKey {
            terrain,
            trees: self.surface_picker.generation,
            enabled: self.raster_trees.enabled,
            wind: self.raster_trees.gpu_wind_active,
        }
    }
    pub(crate) fn request_surface_pick(
        &mut self,
        ray: TerrainRayQuery,
        terrain: u32,
        purpose: PickPurpose,
    ) -> Result<PickTicket> {
        let key = self.pick_scene_key(terrain);
        self.surface_picker.enqueue(ray, key, purpose)
    }
    pub(crate) fn pick_answer_is_current(&self, answer: &PickAnswer, terrain: u32) -> bool {
        answer.matches(self.pick_scene_key(terrain))
            && answer
                .hit
                .and_then(|h| h.tree_primitive)
                .is_none_or(|p| (p as usize) < self.raster_trees.scene.primitives.len())
    }
    /// Status checks only; never wait for a ray. Completed-slot collection below
    /// also handles a fence reset by the acquire seam before this poll observes it.
    pub(crate) fn poll_surface_picks(&mut self) -> Result<Vec<PickAnswer>> {
        for bank in &mut self.surface_picker.banks {
            if let Some(flight) = &bank.flight {
                if flight.probe.is_complete()? {
                    bank.collect(&mut self.surface_picker.answers)?;
                }
            }
        }
        Ok(self.surface_picker.answers.drain(..).collect())
    }
    pub(crate) fn complete_surface_pick_slot(&mut self, frame_slot: usize) -> Result<()> {
        self.complete_fruit_release_slot(frame_slot)?;
        if let Some(bank) = self.surface_picker.banks.get_mut(frame_slot) {
            bank.collect(&mut self.surface_picker.answers)?;
        }
        Ok(())
    }
    /// Record after the normal frame's skin/refit producer. Copies just 48 bytes
    /// per answer. Transient descriptor leases inherit all current scene buffers.
    pub(crate) fn record_frame_surface_picks(
        &mut self,
        frame: &AcquiredFrame,
        terrain: u32,
    ) -> Result<()> {
        self.record_frame_fruit_releases(frame)?;
        if self.surface_picker.queued.is_empty() {
            return Ok(());
        }
        let key = self.pick_scene_key(terrain);
        let mut requests = Vec::with_capacity(BATCH);
        for request in self.surface_picker.queued.drain(..) {
            if request.key == key {
                requests.push(request);
            } else {
                self.surface_picker
                    .answers
                    .push_back(PickAnswer::stale(request));
            }
        }
        if requests.is_empty() {
            return Ok(());
        }
        while self.surface_picker.banks.len() <= frame.frame_slot() {
            self.surface_picker.banks.push(Bank::new(
                self.vulkan_ctx.device().clone(),
                self.allocator.clone(),
            )?);
        }
        for request in &mut requests {
            if request.purpose == PickPurpose::Validation {
                let surface = self
                    .raster_trees
                    .posed_surface
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("missing async reference pose"))?;
                let triangle = self
                    .raster_trees
                    .scene
                    .primitives
                    .iter()
                    .max_by(|a, b| {
                        let height = |t: &&[u32; 4]| {
                            (surface.position(t[0] as usize).y
                                + surface.position(t[1] as usize).y
                                + surface.position(t[2] as usize).y)
                                / 3.
                        };
                        height(a).total_cmp(&height(b))
                    })
                    .ok_or_else(|| anyhow::anyhow!("empty async reference mesh"))?;
                let [a, b, c] = [
                    surface.position(triangle[0] as usize),
                    surface.position(triangle[1] as usize),
                    surface.position(triangle[2] as usize),
                ];
                let normal = (b - a).cross(c - a).normalize();
                request.ray = TerrainRayQuery {
                    origin: (a + b + c) / 3. + normal * 0.01,
                    direction: -normal,
                };
                request.reference = self.raster_trees.raycast(
                    request.ray.origin,
                    request.ray.direction,
                    0..self.raster_trees.scene.primitives.len() as u32,
                );
            }
        }
        let bank = &mut self.surface_picker.banks[frame.frame_slot()];
        ensure!(
            bank.flight.is_none(),
            "pick readback slot reused before completion"
        );
        bank.count.fill(&[requests.len() as u32, 0, 0, 0])?;
        let rays: Vec<[f32; 8]> = requests
            .iter()
            .map(|r| {
                [
                    r.ray.origin.x,
                    r.ray.origin.y,
                    r.ray.origin.z,
                    -1.,
                    r.ray.direction.x,
                    r.ray.direction.y,
                    r.ray.direction.z,
                    0.,
                ]
            })
            .collect();
        bank.rays.fill(&rays)?;
        let command = frame.command_buffer();
        command.use_buffer(&bank.count, BufferUse::HostWrite);
        command.use_buffer(&bank.rays, BufferUse::HostWrite);
        let pipeline = &self.pipeline_topology.compute().terrain_query_ppl;
        let descriptors = pipeline.record_with_overrides(
            command,
            &[
                DescriptorWrite {
                    name: "terrain_query_count",
                    resource: DescriptorResource::Buffer(&bank.count),
                },
                DescriptorWrite {
                    name: "terrain_query_info",
                    resource: DescriptorResource::Buffer(&bank.rays),
                },
            ],
            Extent3D::new(requests.len() as u32, 1, 1),
            None,
        )?;
        self.resources
            .terrain_query
            .terrain_pick_result
            .record_copy_to_buffer(
                command,
                &bank.readback,
                requests.len() as u64 * PICK_BYTES,
                0,
                0,
            );
        command.use_buffer(&bank.readback, BufferUse::HostRead);
        bank.flight = Some(Flight {
            requests,
            probe: frame.readback_probe(),
            _descriptors: descriptors,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{align_of, offset_of, size_of};
    fn key() -> SceneKey {
        SceneKey {
            terrain: 17,
            trees: 9,
            enabled: true,
            wind: true,
        }
    }
    fn ray(x: f32) -> TerrainRayQuery {
        TerrainRayQuery {
            origin: Vec3::new(x, 1., 1.),
            direction: Vec3::NEG_Y,
        }
    }
    #[test]
    fn queued_hover_coalesces_without_overwriting_captured_click() {
        let mut picker = SurfacePicker::default();
        let edit = picker.enqueue(ray(1.), key(), PickPurpose::Edit).unwrap();
        let hover = picker.enqueue(ray(2.), key(), PickPurpose::Hover).unwrap();
        let latest = picker.enqueue(ray(3.), key(), PickPurpose::Hover).unwrap();
        assert!(latest > hover && hover > edit);
        assert_eq!(picker.queued.len(), 2);
        assert_eq!(picker.queued[0].ticket, edit);
        assert_eq!(picker.queued[0].ray.origin, ray(1.).origin);
        assert_eq!(picker.queued[1].ray.origin, ray(3.).origin);
        assert!(picker.enqueue(ray(4.), key(), PickPurpose::Edit).is_err());
        let old_generation = picker.generation;
        picker.invalidate_geometry();
        assert_eq!(picker.generation, old_generation + 1);
        // In-flight/captured requests keep their original stamp for rejection.
        assert_eq!(picker.queued[0].key, key());
    }
    #[test]
    fn stale_geometry_terrain_wind_and_fallback_answers_cannot_edit() {
        let request = Request {
            ticket: PickTicket(7),
            ray: ray(1.),
            key: key(),
            purpose: PickPurpose::Edit,
            reference: None,
        };
        let current = PickAnswer {
            ticket: request.ticket,
            ray: request.ray,
            purpose: request.purpose,
            hit: None,
            key: request.key,
            stale: false,
        };
        assert!(current.matches(key()));
        assert!(!current.matches(SceneKey {
            terrain: 18,
            ..key()
        }));
        assert!(!current.matches(SceneKey { trees: 10, ..key() }));
        assert!(!current.matches(SceneKey {
            wind: false,
            ..key()
        }));
        assert!(!current.matches(SceneKey {
            enabled: false,
            ..key()
        }));
        assert!(!PickAnswer::stale(request).matches(key()));
    }
    #[test]
    fn pick_layout_and_decode_preserve_world_rest_and_integer_identity() {
        assert_eq!(size_of::<PickRaw>(), PICK_BYTES as usize);
        assert_eq!(align_of::<PickRaw>(), 16);
        assert_eq!(offset_of!(PickRaw, rest_position), 16);
        assert_eq!(offset_of!(PickRaw, metadata), 32);
        let raw = PickRaw {
            position: [1., 2., 3., 4.],
            rest_position: [5., 6., 7., 0.],
            metadata: [1, 340443, 5, 0],
        };
        let hit = decode(raw).unwrap().unwrap();
        assert_eq!(hit.world_position, Vec3::new(1., 2., 3.));
        assert_eq!(hit.tree_rest_position(), Some(Vec3::new(5., 6., 7.)));
        assert_eq!(hit.tree_primitive, Some(340443));
        let voxel = decode(PickRaw {
            metadata: [1, NO_PRIMITIVE, 2, 0],
            ..raw
        })
        .unwrap()
        .unwrap();
        assert!(voxel.tree_rest_position().is_none());
        assert!(decode(PickRaw {
            metadata: [0; 4],
            ..raw
        })
        .unwrap()
        .is_none());
        assert!(decode(PickRaw {
            position: [f32::NAN; 4],
            ..raw
        })
        .is_err());
        assert!(decode(PickRaw {
            metadata: [1, 0, 2, 0],
            ..raw
        })
        .is_err());
    }
}
