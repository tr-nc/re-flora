//! Shared immutable model surfaces, restored from f7b7e036 and extended to flowers.
//! ModelPixelFrame owns this cache; PipelineTopology owns its bake pipeline.
//! GpuPagedStorage owns allocation/addressing and ready-slot residency.
use crate::{
    flora::models::{self, Shape},
    model_assets,
    resource::Resource,
};
use anyhow::{ensure, Context, Result};
use bytemuck::{Pod, Zeroable};
use glam::{Vec2, Vec3};
use re_flora_vkn::{
    vk, Allocator, Buffer, BufferUsage, BufferUse, CommandBuffer, ComputePipeline,
    DescriptorResource, Device, Extent3D, GpuPagedStorage, GpuStorageAllocation, GpuStorageHandle,
    MemoryLocation, VulkanContext,
};
use resource_container_derive::ResourceContainer;
use std::{
    alloc::Layout,
    sync::{Arc, LazyLock},
};

pub const ANIMATION_FRAMES: u32 = 32;
pub const BUTTERFLY_SOURCE_BASE: u32 = 65;
pub const FLOWER_SOURCE_BASE: u32 = BUTTERFLY_SOURCE_BASE + ANIMATION_FRAMES;
const KINDS: usize = 4; // leaves, shared attached/fallen apple, butterflies, flowers
const SURFACE_BYTES: u64 = 32;
const FORMAT_VERSION: u32 = 7; // Flower normalMaterial.w now carries packed atlas UVs.
static SHAPES: LazyLock<[u32; KINDS]> = LazyLock::new(|| {
    [
        64,
        1,
        ANIMATION_FRAMES,
        flower_cache_representatives()
            .iter()
            .map(|&i| 1 + crate::flora::models::flowers()[i].heads.len() as u32)
            .sum(),
    ]
});

fn flower_cache_representatives() -> &'static [usize] {
    static REPRESENTATIVES: LazyLock<Vec<usize>> = LazyLock::new(|| {
        let flowers = models::flowers();
        flowers
            .iter()
            .enumerate()
            .filter_map(|(i, flower)| {
                (!flowers[..i]
                    .iter()
                    .any(|prior| prior.cache_family == flower.cache_family))
                .then_some(i)
            })
            .collect()
    });
    &REPRESENTATIVES
}
pub fn flower_source(model: usize, part: usize) -> u32 {
    let flowers = models::flowers();
    assert!(part <= flowers[model].heads.len());
    let representative = flower_cache_representatives()
        .iter()
        .position(|&i| flowers[i].cache_family == flowers[model].cache_family)
        .expect("flower cache family must have a representative");
    FLOWER_SOURCE_BASE
        + flower_cache_representatives()[..representative]
            .iter()
            .map(|&i| 1 + flowers[i].heads.len() as u32)
            .sum::<u32>()
        + part as u32
}
pub fn animation_frame(phase: f32) -> u32 {
    ((phase.rem_euclid(1.) * ANIMATION_FRAMES as f32).round() as u32) % ANIMATION_FRAMES
}
pub fn animation_phase(frame: u32) -> f32 {
    frame as f32 / ANIMATION_FRAMES as f32
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct Triangle {
    pub(super) a: [f32; 4],
    pub(super) e1: [f32; 4],
    pub(super) e2: [f32; 4],
    pub(super) normals: [[f32; 4]; 3],
    pub(super) uv01: [f32; 4],
    pub(super) uv2: [f32; 4],
}
fn triangle(p: [Vec3; 3], normals: [Vec3; 3], uv: [Vec2; 3], material: u32) -> Triangle {
    Triangle {
        a: p[0].extend(0.).to_array(),
        e1: (p[1] - p[0]).extend(0.).to_array(),
        e2: (p[2] - p[0]).extend(0.).to_array(),
        normals: normals.map(|v| v.extend(0.).to_array()),
        uv01: [uv[0].x, uv[0].y, uv[1].x, uv[1].y],
        uv2: [uv[2].x, uv[2].y, material as f32, 0.],
    }
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FlowerPart {
    range: [u32; 4],
    center_radius: [f32; 4],
    stem: [f32; 4], // tip x/y, max bend fraction, non-pixel stem triangle count
    distribution: [f32; 4], // base layers, edge, mean, standard deviation
}
pub(super) struct Source {
    pub(super) triangles: Vec<Triangle>,
    pub(super) ranges: Vec<[u32; 4]>,
    pub(super) frames: Vec<[f32; 4]>,
    palette: Vec<[f32; 4]>,
    flower_parts: Vec<FlowerPart>,
    flower_root_radius: f32,
    flower_spawn_height: f32,
}
impl Source {
    fn flower_culling_padding(&self, world_scale: f32, overshoot_voxels: f32) -> (Vec3, Vec3) {
        let radius = self.flower_root_radius * world_scale;
        // flowerPlantFrame starts one voxel above the planted base; spawn can
        // rise from -(plantHeight + 1) to the configured overshoot. Growth <= 1.
        (
            Vec3::splat(radius) + Vec3::Y * (self.flower_spawn_height * world_scale + 1. / 256.),
            Vec3::splat(radius) + Vec3::Y * ((overshoot_voxels.max(0.) + 1.) / 256.),
        )
    }
}
pub(super) fn source(shape: Shape) -> Source {
    let mut triangles = Vec::new();
    let mut ranges = Vec::new();
    let mut frames = Vec::new();
    let mut palette = Vec::new();
    let leaves = model_assets::leaf_variants();
    let transforms = leaves.transforms(0., 0);
    for variant in 0..64 {
        let start = triangles.len() as u32;
        for t in leaves.triangles.iter().filter(|t| t.node == variant) {
            let m = transforms[t.node];
            let nm = m.inverse().transpose();
            triangles.push(triangle(
                t.positions.map(|p| m.transform_point3(p)),
                t.normals.map(|n| nm.transform_vector3(n).normalize()),
                t.uvs,
                0,
            ));
        }
        ranges.push([start, triangles.len() as u32 - start, 0, variant as u32]);
        frames.push([0., 0., 0., 1.7]);
    }
    let apple = super::apple_preview::mesh();
    let start = triangles.len() as u32;
    for indices in apple.indices.chunks_exact(3) {
        let ids = [
            indices[0] as usize,
            indices[1] as usize,
            indices[2] as usize,
        ];
        triangles.push(triangle(
            ids.map(|i| apple.positions[i] * 0.5),
            ids.map(|i| apple.normals[i]),
            [Vec2::ZERO; 3],
            apple.materials[ids[0]] as u32,
        ));
    }
    ranges.push([start, triangles.len() as u32 - start, 1, 0]);
    frames.push([0., 0., 0., 1.55]);
    let butterfly = model_assets::butterfly();
    for frame in 0..ANIMATION_FRAMES {
        let transforms = butterfly.transforms(animation_phase(frame), 0);
        let root = transforms[butterfly.node("Flight pose")].w_axis.truncate();
        let start = triangles.len() as u32;
        for t in &butterfly.triangles {
            let m = transforms[t.node];
            triangles.push(triangle(
                t.positions.map(|p| m.transform_point3(p) - root),
                [Vec3::Z; 3],
                t.uvs,
                0,
            ));
        }
        ranges.push([start, triangles.len() as u32 - start, 2, frame]);
        frames.push([0., 0., 0., 1.7]);
    }
    let mut flower_parts = Vec::new();
    let mut flower_root_radius = 4f32; // Preserve the old minimum culling margin.
    let mut flower_spawn_height = 0f32;
    for (model, authored) in models::flowers().iter().enumerate() {
        let flower = authored.transformed(shape);
        palette.extend(authored.palette.iter().map(|rgb| {
            [
                f32::from(rgb[0]) / 255.,
                f32::from(rgb[1]) / 255.,
                f32::from(rgb[2]) / 255.,
                0.,
            ]
        }));
        let shared = (flower_source(model, 0) as usize) < ranges.len();
        let first = if shared {
            ranges[flower_source(model, 0) as usize][0]
        } else {
            triangles.len() as u32
        };
        // Bound growth/yaw and attachment wind transport, including quad corners.
        // Spawn translation is vertical and bounded separately, not a scale.
        for part in std::iter::once(&flower.whole).chain(&flower.heads) {
            flower_root_radius = flower_root_radius.max(
                flower.column.tip().length()
                    + (part.center - flower.column.tip()).length()
                    + part.radius * std::f32::consts::SQRT_2
                    + flower.column.tip().y * models::MAX_BEND_FRACTION,
            );
        }
        flower_spawn_height = flower_spawn_height.max(flower.whole.center.y + flower.whole.radius);
        for part_index in 0..=models::MAX_HEADS {
            let part = if part_index == 0 {
                Some(&flower.whole)
            } else {
                flower.heads.get(part_index - 1)
            };
            flower_parts.push(part.map_or(FlowerPart::zeroed(), |part| FlowerPart {
                range: [
                    first + part.triangles.start,
                    part.triangles.end - part.triangles.start,
                    flower_source(model, part_index),
                    flower.heads.len() as u32,
                ],
                center_radius: part.center.extend(part.radius).to_array(),
                distribution: flower.distribution,
                stem: [
                    flower.column.tip().x,
                    flower.column.tip().y,
                    models::MAX_BEND_FRACTION,
                    flower.stem_triangles as f32,
                ],
            }));
        }
        for (index, t) in flower.triangles.iter().enumerate() {
            let mut gpu = triangle(t.positions, [t.normal; 3], t.uvs, 0);
            gpu.normals[0][3] = t.anchor.y;
            // Cache stores atlas UVs, never species color. The shared palette
            // decoder publishes the same sRGB8 atlas used by the web.
            if shared {
                assert_eq!(
                    bytemuck::bytes_of(&gpu),
                    bytemuck::bytes_of(&triangles[first as usize + index]),
                    "flower family geometry differs: {}",
                    flower.id
                );
            } else {
                triangles.push(gpu);
            }
        }
        if shared {
            continue;
        }
        for (part_index, part) in std::iter::once(&flower.whole)
            .chain(&flower.heads)
            .enumerate()
        {
            assert_eq!(ranges.len() as u32, flower_source(model, part_index));
            let shape = ranges.len() as u32 - FLOWER_SOURCE_BASE;
            ranges.push([
                first + part.triangles.start,
                part.triangles.end - part.triangles.start,
                3,
                shape,
            ]);
            frames.push(part.center.extend(part.radius).to_array());
        }
    }
    // Keep the four palette entries/species at their stable prefix. Their w
    // components describe a resolved atlas in the same buffer, avoiding a new
    // descriptor or any change to the 32-byte baked-surface storage ABI.
    for (model, flower) in models::flowers().iter().enumerate() {
        let texture = &flower.color_texture;
        let base = model * models::HEAD_PALETTE_SIZE;
        palette[base][3] = palette.len() as f32;
        palette[base + 1][3] = texture.width as f32;
        palette[base + 2][3] = texture.height as f32;
        palette.extend(texture.rgb.chunks_exact(3).map(|rgb| {
            let linear = |v: u8| {
                let v = f32::from(v) / 255.;
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            [linear(rgb[0]), linear(rgb[1]), linear(rgb[2]), 0.]
        }));
    }
    Source {
        triangles,
        ranges,
        frames,
        palette,
        flower_parts,
        flower_root_radius,
        flower_spawn_height,
    }
}
fn buffer(device: &Device, allocator: &Allocator, bytes: usize) -> Result<Arc<Buffer>> {
    Ok(Arc::new(Buffer::try_new_sized(
        device.clone(),
        allocator.clone(),
        BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
        MemoryLocation::CpuToGpu,
        bytes as u64,
    )?))
}
#[derive(ResourceContainer)]
pub struct CacheResources {
    pub model_bake_triangles: Resource<Buffer>,
    pub model_bake_ranges: Resource<Buffer>,
    pub model_bake_frames: Resource<Buffer>,
    pub model_cache_sources: Resource<Buffer>,
    pub model_cache_palette: Resource<Buffer>,
    pub model_bake_evidence: Resource<Buffer>,
    pub model_cache_validation: Resource<Buffer>,
    pub model_cache_entries: Resource<Buffer>,
    pub model_view_azimuths: Resource<Buffer>,
}
impl CacheResources {
    pub fn new(device: Device, allocator: Allocator) -> Self {
        let upload = |bytes: &[u8]| {
            let b = Buffer::new_sized(
                device.clone(),
                allocator.clone(),
                BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
                MemoryLocation::CpuToGpu,
                bytes.len() as u64,
            );
            b.fill_range_with_raw_u8(0, bytes)
                .expect("immutable model cache source");
            Resource::new(b)
        };
        let s = source(Shape::default());
        Self {
            // Only initial descriptors; real immutable generations are frame-owned.
            model_bake_triangles: upload(&[0; 128]),
            model_bake_ranges: upload(&[0; 16]),
            model_bake_frames: upload(&[0; 16]),
            model_view_azimuths: upload(&[0; 16]),
            model_cache_sources: upload(bytemuck::cast_slice(
                &s.ranges.iter().map(|r| [r[2], r[3]]).collect::<Vec<_>>(),
            )),
            model_cache_palette: upload(bytemuck::cast_slice(&s.palette)),
            model_cache_entries: upload(bytemuck::cast_slice(&[Entry::zeroed(); KINDS])),
            model_bake_evidence: upload(bytemuck::cast_slice(&[[0f32; 4]])),
            model_cache_validation: upload(bytemuck::cast_slice(&[0u32; KINDS * 4])),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Spec {
    pub(super) kind: u32,
    pub(super) resolution: u32,
    pub(super) views: u32,
    pub(super) shape: Shape,
}
fn requested_specs(views: [u32; KINDS], resolutions: [u32; KINDS], shape: Shape) -> [Spec; KINDS] {
    std::array::from_fn(|kind| Spec {
        kind: kind as u32,
        resolution: resolutions[kind],
        views: views[kind],
        shape: if kind == 3 {
            shape.normalized()
        } else {
            Shape::default()
        },
    })
}
impl Spec {
    fn records(self) -> Result<u64> {
        ensure!(
            (8..=64).contains(&self.resolution) && (8..=512).contains(&self.views),
            "invalid model bake specification: {self:?}"
        );
        let shapes = *SHAPES
            .get(self.kind as usize)
            .context("invalid model kind")?;
        Ok(u64::from(shapes) * u64::from(self.views) * u64::from(self.resolution).pow(2))
    }
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Entry {
    storage: GpuStorageHandle,
    resolution: u32,
    views: u32,
    shapes: u32,
    version: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BakePush {
    kind: u32,
    resolution: u32,
    views: u32,
    shapes: u32,
    storage: GpuStorageHandle,
    verify: u32,
    evidence_stride: u32,
}
struct SourceGeneration {
    shape: Shape,
    cpu: Source,
    triangles: Arc<Buffer>,
    ranges: Arc<Buffer>,
    frames: Arc<Buffer>,
    parts: Arc<Buffer>,
    evidence_placeholder: Arc<Buffer>,
}
impl SourceGeneration {
    fn new(device: &Device, allocator: &Allocator, shape: Shape) -> Result<Self> {
        let cpu = source(shape);
        let upload = |bytes: &[u8]| -> Result<Arc<Buffer>> {
            let b = buffer(device, allocator, bytes.len())?;
            b.fill_range_with_raw_u8(0, bytes)?;
            Ok(b)
        };
        Ok(Self {
            shape,
            triangles: upload(bytemuck::cast_slice(&cpu.triangles))?,
            ranges: upload(bytemuck::cast_slice(&cpu.ranges))?,
            frames: upload(bytemuck::cast_slice(&cpu.frames))?,
            parts: upload(bytemuck::cast_slice(&cpu.flower_parts))?,
            evidence_placeholder: upload(&[0; 16])?,
            cpu,
        })
    }
    fn bake_bindings<'a>(
        &'a self,
        directions: &'a Directions,
        validation: &'a Buffer,
        evidence: &'a Buffer,
    ) -> [(&'static str, DescriptorResource<'a>); 6] {
        [
            (
                "model_bake_triangles",
                DescriptorResource::Buffer(&self.triangles),
            ),
            (
                "model_bake_ranges",
                DescriptorResource::Buffer(&self.ranges),
            ),
            (
                "model_bake_frames",
                DescriptorResource::Buffer(&self.frames),
            ),
            (
                "model_view_azimuths",
                DescriptorResource::Buffer(&directions.buffer),
            ),
            (
                "model_cache_validation",
                DescriptorResource::Buffer(validation),
            ),
            ("model_bake_evidence", DescriptorResource::Buffer(evidence)),
        ]
    }
}
struct Directions {
    count: u32,
    buffer: Arc<Buffer>,
}
impl Directions {
    fn new(device: &Device, allocator: &Allocator, count: u32) -> Result<Self> {
        let buffer = buffer(device, allocator, count as usize * 16)?;
        buffer.fill(&super::model_pixel_views::azimuths(count))?;
        log::info!(
            "[MODEL_CACHE_DIRECTIONS] count={count} bytes={} demand_sized=true",
            buffer.get_size_bytes()
        );
        Ok(Self { count, buffer })
    }
}
#[derive(Clone)]
pub(super) struct CacheFrame {
    validation: Arc<Buffer>,
    entries: Arc<Buffer>,
    source: Arc<SourceGeneration>,
    directions: Arc<Directions>,
}
impl CacheFrame {
    pub fn bindings(&self) -> [(&'static str, DescriptorResource<'_>); 3] {
        [
            (
                "model_view_azimuths",
                DescriptorResource::Buffer(&self.directions.buffer),
            ),
            (
                "model_cache_entries",
                DescriptorResource::Buffer(&self.entries),
            ),
            (
                "model_cache_validation",
                DescriptorResource::Buffer(&self.validation),
            ),
        ]
    }
    pub fn flower_parts(&self) -> (&'static str, DescriptorResource<'_>) {
        (
            "flower_parts",
            DescriptorResource::Buffer(&self.source.parts),
        )
    }
    pub fn flower_stem_index_count(&self, model: usize) -> u32 {
        self.source.cpu.flower_parts[model * (models::MAX_HEADS + 1)].stem[3] as u32 * 3
    }
    pub fn flower_triangles(&self) -> (&'static str, DescriptorResource<'_>) {
        (
            "flower_triangles",
            DescriptorResource::Buffer(&self.source.triangles),
        )
    }
}
pub(super) struct ModelPixelCache {
    device: Device,
    allocator: Allocator,
    storage: GpuPagedStorage,
    active: [Option<(Spec, GpuStorageAllocation)>; KINDS],
    frames: Vec<Option<CacheFrame>>,
    diagnostics: Vec<Vec<(Spec, Arc<SourceGeneration>, Arc<Buffer>)>>,
    source: Option<Arc<SourceGeneration>>,
    directions: Option<Arc<Directions>>,
    review: bool,
}
impl ModelPixelCache {
    pub fn new(context: &VulkanContext, allocator: Allocator) -> Self {
        Self {
            device: context.device().clone(),
            storage: GpuPagedStorage::new(context, allocator.clone()),
            allocator,
            active: std::array::from_fn(|_| None),
            frames: Vec::new(),
            diagnostics: Vec::new(),
            source: None,
            directions: None,
            review: std::env::var_os("RE_FLORA_MODEL_CACHE_REVIEW").is_some(),
        }
    }
    /// Called in the ready frame slot, before any consumer. Allocate every
    /// replacement before publishing anything; never silently fall back to live.
    pub fn prepare(
        &mut self,
        slot: usize,
        cmd: &CommandBuffer,
        pipeline: &ComputePipeline,
        views: [u32; KINDS],
        resolutions: [u32; KINDS],
        shape: Shape,
    ) -> Result<()> {
        while self.frames.len() <= slot {
            self.frames.push(None);
            self.diagnostics.push(Vec::new());
        }
        for (spec, source, evidence) in self.diagnostics[slot].drain(..) {
            super::model_pixel_bake_validation::validate(
                spec,
                &source.cpu,
                &evidence.read_back()?,
            )?;
        }
        let requested = requested_specs(views, resolutions, shape);
        for spec in requested {
            spec.records()?;
        }
        let source = match &self.source {
            Some(old) if old.shape == shape => old.clone(),
            _ => Arc::new(SourceGeneration::new(&self.device, &self.allocator, shape)?),
        };
        let count = *views.iter().max().unwrap();
        let directions = match &self.directions {
            Some(old) if old.count == count => old.clone(),
            _ => Arc::new(Directions::new(&self.device, &self.allocator, count)?),
        };
        if let Some(frame) = &self.frames[slot] {
            if self.review {
                let bytes = frame.validation.read_back()?;
                let counters: &[u32] = bytemuck::try_cast_slice(&bytes)
                    .map_err(|e| anyhow::anyhow!("cache counters: {e}"))?;
                for kind in 0..KINDS {
                    let at = KINDS * 2 + kind * 2;
                    if counters[at] > 0 {
                        log::info!(
                            "[MODEL_CACHE_BAKE_CHECK] kind={kind} checked={} mismatches={}",
                            counters[at],
                            counters[at + 1]
                        );
                        ensure!(
                            counters[at + 1] == 0,
                            "stored/baked canonical surface mismatch"
                        );
                    }
                }
                if counters.iter().any(|&v| v != 0) {
                    log::info!(
                        "[MODEL_CACHE_CONSUMED] leaf={} apple={} butterfly={} flower={}",
                        counters[0],
                        counters[2],
                        counters[4],
                        counters[6]
                    );
                }
            }
        } else {
            self.frames[slot] = Some(CacheFrame {
                validation: buffer(&self.device, &self.allocator, KINDS * 16)?,
                entries: buffer(
                    &self.device,
                    &self.allocator,
                    KINDS * std::mem::size_of::<Entry>(),
                )?,
                source: source.clone(),
                directions: directions.clone(),
            });
        }
        self.storage.begin_frame(slot);
        pipeline.begin_transient_descriptor_frame(slot);
        self.frames[slot]
            .as_ref()
            .unwrap()
            .validation
            .fill(&[0u32; KINDS * 4])?;
        // Replace only this completed slot's source/direction leases. Other slots
        // and native transient descriptors keep old immutable buffers alive.
        let frame = self.frames[slot].as_mut().unwrap();
        frame.source = source.clone();
        frame.directions = directions.clone();
        let mut replacements = Vec::new();
        for spec in requested {
            if self.active[spec.kind as usize]
                .as_ref()
                .is_some_and(|(old, _)| *old == spec)
            {
                continue;
            }
            let allocation=self.storage.allocate(spec.records()?,Layout::from_size_align(SURFACE_BYTES as usize,16).unwrap())
                .with_context(||format!("cannot prepare model surface cache {spec:?}; free GPU memory or reduce resolution/view count"))?;
            replacements.push((spec, allocation));
        }
        for (spec, allocation) in &replacements {
            self.storage
                .use_in_frame(slot, cmd, allocation, BufferUse::ComputeWrite);
            let push = BakePush {
                kind: spec.kind,
                resolution: spec.resolution,
                views: spec.views,
                shapes: SHAPES[spec.kind as usize],
                storage: allocation.handle(),
                verify: 0,
                evidence_stride: 0,
            };
            let descriptors =
                source.bake_bindings(&directions, &frame.validation, &source.evidence_placeholder);
            pipeline.record_with_descriptors(
                cmd,
                &descriptors,
                Extent3D::new(spec.resolution, spec.resolution, spec.views * push.shapes),
                Some(bytemuck::bytes_of(&push)),
            )?;
            if self.review {
                self.storage
                    .use_in_frame(slot, cmd, allocation, BufferUse::ComputeRead);
                let (stride, cases) =
                    super::model_pixel_bake_validation::layout(*spec, &source.cpu);
                let evidence = buffer(&self.device, &self.allocator, stride * cases * 16)?;
                let descriptors = source.bake_bindings(&directions, &frame.validation, &evidence);
                let verify = BakePush { verify: 1, ..push };
                pipeline.record_with_descriptors(
                    cmd,
                    &descriptors,
                    Extent3D::new(spec.resolution, spec.resolution, spec.views * push.shapes),
                    Some(bytemuck::bytes_of(&verify)),
                )?;
                let diagnostic = BakePush {
                    verify: 2,
                    evidence_stride: stride as u32,
                    ..push
                };
                pipeline.record_with_descriptors(
                    cmd,
                    &descriptors,
                    Extent3D::new(spec.resolution, spec.resolution, cases as u32),
                    Some(bytemuck::bytes_of(&diagnostic)),
                )?;
                cmd.use_buffer(&evidence, BufferUse::HostRead);
                self.diagnostics[slot].push((*spec, source.clone(), evidence));
            }
            log::info!("[MODEL_CACHE_BUILD] kind={} views={} resolution={} shapes={} bytes={} blocks={} resident_bytes={} version={FORMAT_VERSION} head_scale={} height_scale={}",spec.kind,spec.views,spec.resolution,push.shapes,spec.records()?*SURFACE_BYTES,allocation.block_count(),allocation.resident_bytes(),spec.shape.head_scale,spec.shape.height_scale);
        }
        for (spec, allocation) in replacements {
            self.active[spec.kind as usize] = Some((spec, allocation));
        }
        let mut entries = [Entry::zeroed(); KINDS];
        for (kind, item) in self.active.iter().enumerate() {
            let (spec, allocation) = item.as_ref().expect("all model specifications prepared");
            self.storage
                .use_in_frame(slot, cmd, allocation, BufferUse::ComputeRead);
            entries[kind] = Entry {
                storage: allocation.handle(),
                resolution: spec.resolution,
                views: spec.views,
                shapes: SHAPES[kind],
                version: FORMAT_VERSION,
            };
        }
        self.frames[slot].as_ref().unwrap().entries.fill(&entries)?;
        self.source = Some(source);
        self.directions = Some(directions);
        if self.review {
            let active: u64 = self
                .active
                .iter()
                .flatten()
                .map(|(_, a)| a.resident_bytes())
                .sum();
            let direction = self.directions.as_ref().unwrap();
            log::info!("[MODEL_CACHE_RESIDENCY] slot={slot} active_bytes={active} retained_bytes={} direction_count={} direction_bytes={}", self.storage.resident_bytes(), direction.count, direction.buffer.get_size_bytes());
        }
        Ok(())
    }
    pub fn flower_culling_padding(&self, world_scale: f32, overshoot_voxels: f32) -> (Vec3, Vec3) {
        self.source
            .as_ref()
            .expect("prepared flower source")
            .cpu
            .flower_culling_padding(world_scale, overshoot_voxels)
    }
    pub fn frame(&self, slot: usize) -> CacheFrame {
        self.frames[slot]
            .as_ref()
            .expect("prepared shared model cache")
            .clone()
    }
    pub fn finish(&self, slot: usize, cmd: &CommandBuffer) {
        if self.review {
            cmd.use_buffer(
                &self.frames[slot].as_ref().unwrap().validation,
                BufferUse::HostRead,
            );
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_consumers_have_shared_canonical_sources() {
        let s = source(Shape::default());
        assert_eq!(std::mem::size_of::<Triangle>(), 128);
        assert_eq!(model_assets::LEAF_VARIANT_COUNT, SHAPES[0] as usize);
        assert_eq!(s.ranges.len(), SHAPES.iter().sum::<u32>() as usize);
        assert_eq!(s.frames.len(), s.ranges.len());
        let mut shapes = [0; KINDS];
        for (index, r) in s.ranges.iter().enumerate() {
            let kind = r[2] as usize;
            assert_eq!(r[3], shapes[kind]);
            shapes[kind] += 1;
            assert!(r[1] > 0 && (r[0] + r[1]) as usize <= s.triangles.len());
            assert!(s.frames[index].iter().all(|v| v.is_finite()) && s.frames[index][3] > 0.);
            if index < 64 {
                assert_eq!(r[1], 32);
            } else if index == 64 {
                assert_eq!(r[1], 780);
            }
        }
        assert_eq!(shapes, *SHAPES);
        assert_eq!(
            s.palette.len(),
            models::MODEL_COUNT * models::HEAD_PALETTE_SIZE
                + models::flowers()
                    .iter()
                    .map(|f| f.color_texture.width * f.color_texture.height)
                    .sum::<usize>()
        );
        for (model, flower) in models::flowers().iter().enumerate() {
            for (role, rgb) in flower.palette.iter().enumerate() {
                assert_eq!(
                    s.palette[model * models::HEAD_PALETTE_SIZE + role][..3],
                    rgb.map(|v| f32::from(v) / 255.)
                );
            }
        }
        for (model, flower) in models::flowers().iter().enumerate() {
            let base = model * models::HEAD_PALETTE_SIZE;
            let offset = s.palette[base][3] as usize;
            assert_eq!(s.palette[base + 1][3], flower.color_texture.width as f32);
            assert_eq!(s.palette[base + 2][3], flower.color_texture.height as f32);
            assert!(offset >= models::MODEL_COUNT * models::HEAD_PALETTE_SIZE);
            for (texel, rgb) in flower.color_texture.rgb.chunks_exact(3).enumerate() {
                for channel in 0..3 {
                    let v = f32::from(rgb[channel]) / 255.;
                    let expected = if v <= 0.04045 {
                        v / 12.92
                    } else {
                        ((v + 0.055) / 1.055).powf(2.4)
                    };
                    assert!((s.palette[offset + texel][channel] - expected).abs() < 1e-7);
                }
            }
        }
        assert_ne!(flower_source(1, 0), flower_source(3, 0));
        assert_ne!(
            s.palette[models::HEAD_PALETTE_SIZE],
            s.palette[3 * models::HEAD_PALETTE_SIZE]
        );
        for (model, f) in crate::flora::models::flowers().iter().enumerate() {
            for (part_index, p) in std::iter::once(&f.whole).chain(&f.heads).enumerate() {
                let id = flower_source(model, part_index) as usize;
                assert_eq!(s.frames[id], p.center.extend(p.radius).to_array());
                assert_eq!(s.ranges[id][1], p.triangles.end - p.triangles.start);
            }
        }
    }
    #[test]
    fn flower_controls_only_invalidate_the_flower_bank_and_size_current_demand() {
        let baseline = requested_specs([16; KINDS], [32; KINDS], Shape::default());
        for (views, resolution, shape) in [
            (512, 8, Shape::default()),
            (8, 64, Shape::default()),
            (
                16,
                32,
                Shape {
                    head_scale: 2.,
                    height_scale: 1.,
                    ..Shape::default()
                },
            ),
            (
                16,
                32,
                Shape {
                    head_scale: 1.,
                    height_scale: 2.,
                    ..Shape::default()
                },
            ),
        ] {
            let specs = requested_specs([16, 16, 16, views], [32, 32, 32, resolution], shape);
            assert_eq!(&specs[..3], &baseline[..3]);
            assert_ne!(specs[3], baseline[3]);
            assert_eq!(
                specs[3].records().unwrap(),
                u64::from(SHAPES[3]) * u64::from(views) * u64::from(resolution).pow(2)
            );
            assert_eq!(
                super::super::model_pixel_views::azimuths(views.max(16)).len(),
                views.max(16) as usize
            );
        }
        // Global view changes no longer modify the independently configured flower bank.
        assert_eq!(
            requested_specs([512, 512, 512, 16], [32; KINDS], Shape::default())[3],
            baseline[3]
        );
    }

    #[test]
    fn transformed_voxel_stems_and_cached_heads_share_exact_source_and_bounds() {
        let base = source(Shape::default());
        for shape in [
            Shape::default(),
            Shape::MAX,
            Shape {
                head_scale: 0.25,
                height_scale: 4.,
                ..Shape::default()
            },
            Shape {
                head_scale: 4.,
                height_scale: 0.25,
                ..Shape::default()
            },
        ] {
            let s = source(shape);
            let first = s.ranges[FLOWER_SOURCE_BASE as usize][0] as usize;
            assert_eq!(
                bytemuck::cast_slice::<_, u8>(&s.triangles[..first]),
                bytemuck::cast_slice::<_, u8>(&base.triangles[..first])
            );
            assert_eq!(s.palette, base.palette);
            assert_eq!(
                s.ranges[..FLOWER_SOURCE_BASE as usize],
                base.ranges[..FLOWER_SOURCE_BASE as usize]
            );
            for (model, authored) in models::flowers().iter().enumerate() {
                let flower = authored.transformed(shape);
                let whole = &s.flower_parts[model * 4];
                assert_eq!(whole.stem[3] as u32, flower.stem_triangles);
                assert_eq!(whole.distribution, flower.distribution);
                assert_eq!(whole.distribution[3], shape.height_variance.sqrt());
                for z in [-8., -3., 0., 1., 3., 8.] {
                    let layers = shape.layers_for_normal(authored.column.count(), z);
                    let edge = whole.distribution[1];
                    let tip = Vec3::new(
                        whole.stem[0].min(edge * (layers - 1) as f32 * 0.25),
                        layers as f32 * edge,
                        0.,
                    );
                    for head in &flower.heads {
                        let center = head.center + tip - flower.column.tip();
                        assert!(
                            center.length()
                                + head.radius * std::f32::consts::SQRT_2
                                + tip.y * models::MAX_BEND_FRACTION
                                <= s.flower_root_radius
                        );
                    }
                }
                assert_eq!(
                    &whole.stem[..3],
                    &[
                        flower.column.tip().x,
                        flower.column.tip().y,
                        models::MAX_BEND_FRACTION
                    ]
                );
                for (i, triangle) in flower.triangles.iter().enumerate() {
                    let stored = s.triangles[whole.range[0] as usize + i];
                    assert_eq!(stored.a, triangle.positions[0].extend(0.).to_array());
                    assert_eq!(
                        stored.e1,
                        (triangle.positions[1] - triangle.positions[0])
                            .extend(0.)
                            .to_array()
                    );
                    assert_eq!(
                        stored.normals[0],
                        triangle.normal.extend(triangle.anchor.y).to_array()
                    );
                    assert!(triangle
                        .positions
                        .iter()
                        .all(|p| p.length() < s.flower_root_radius));
                }
                for (part, p) in std::iter::once(&flower.whole)
                    .chain(&flower.heads)
                    .enumerate()
                {
                    let native = &s.flower_parts[model * 4 + part];
                    let index = native.range[2] as usize;
                    assert_eq!(native.range[..2], s.ranges[index][..2]);
                    assert_eq!(native.center_radius, s.frames[index]);
                    assert_eq!(native.center_radius, p.center.extend(p.radius).to_array());
                }
            }
        }
        assert_eq!(std::mem::size_of::<FlowerPart>(), 64);
        assert_eq!(
            std::mem::size_of::<crate::generated::gpu_structs::PushConstantFlowerPixel>(),
            48
        );
    }

    #[test]
    fn flower_culling_contains_grown_wind_rotated_display_quads_and_spawn_translation() {
        for shape in [
            Shape::default(),
            Shape::MAX,
            Shape {
                head_scale: 4.,
                height_scale: 0.25,
                ..Shape::default()
            },
            Shape {
                head_scale: 0.25,
                height_scale: 4.,
                ..Shape::default()
            },
        ] {
            let source = source(shape);
            for size in [0.5, 2.] {
                let scale = models::WORLD_SCALE * size;
                let overshoot = 12.;
                let (below, above) = source.flower_culling_padding(scale, overshoot);
                for part in source.flower_parts.iter().filter(|p| p.range[1] != 0) {
                    let center = Vec3::from_slice(&part.center_radius);
                    let radius = part.center_radius[3];
                    for tilt in [-1.2, 0., 1.2] {
                        let pose =
                            glam::Quat::from_rotation_z(tilt) * glam::Quat::from_rotation_y(0.7);
                        for growth in [0.1, 0.55, 1.] {
                            for spawn in [
                                -source.flower_spawn_height * scale - 1. / 256.,
                                0.,
                                overshoot / 256.,
                            ] {
                                for x in [-1., 1.] {
                                    for y in [-1., 1.] {
                                        let p = pose
                                            * (center + Vec3::new(x, y, 0.) * radius)
                                            * scale
                                            * growth
                                            + Vec3::Y * (spawn + 1. / 256.);
                                        assert!(p.cmpge(-below).all() && p.cmple(above).all());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn frame_selection_wraps_with_bounded_error() {
        for i in -1000..1000 {
            let p = i as f32 / 317.;
            let f = animation_frame(p);
            assert!(f < ANIMATION_FRAMES);
            assert!(
                ((animation_phase(f) - p + 0.5).rem_euclid(1.) - 0.5).abs()
                    <= 0.5 / ANIMATION_FRAMES as f32 + 1e-6
            );
        }
    }
    #[test]
    fn full_supported_data_is_not_truncated_to_a_per_bank_budget() {
        for kind in 0..KINDS {
            for views in 8..=512 {
                for resolution in 8..=64 {
                    let s = Spec {
                        kind: kind as u32,
                        resolution,
                        views,
                        shape: Shape::default(),
                    };
                    assert_eq!(
                        s.records().unwrap(),
                        u64::from(SHAPES[kind]) * u64::from(views) * u64::from(resolution).pow(2)
                    );
                }
            }
        }
        assert_eq!(
            Spec {
                kind: 0,
                resolution: 64,
                views: 512,
                shape: Shape::default(),
            }
            .records()
            .unwrap()
                * SURFACE_BYTES,
            4u64 << 30
        );
        assert!(Spec {
            kind: 0,
            resolution: 16,
            views: 0,
            shape: Shape::default(),
        }
        .records()
        .is_err());
        assert_eq!(std::mem::size_of::<Entry>(), 40);
        assert_eq!(std::mem::offset_of!(Entry, resolution), 24);
        assert_eq!(std::mem::size_of::<BakePush>(), 48);
        assert_eq!(std::mem::offset_of!(BakePush, storage), 16);
    }
}
