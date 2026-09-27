//! Shared immutable model surfaces, restored from f7b7e036 and extended to flowers.
//! ModelPixelFrame owns this cache; PipelineTopology owns its bake pipeline.
//! GpuPagedStorage owns allocation/addressing and ready-slot residency.
use crate::{model_assets, resource::Resource};
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
const FORMAT_VERSION: u32 = 3;
static SHAPES: LazyLock<[u32; KINDS]> = LazyLock::new(|| {
    [
        64,
        1,
        ANIMATION_FRAMES,
        crate::flora::models::flowers()
            .iter()
            .map(|f| 1 + f.heads.len() as u32)
            .sum(),
    ]
});

pub fn flower_source(model: usize, part: usize) -> u32 {
    let flowers = crate::flora::models::flowers();
    assert!(part <= flowers[model].heads.len());
    FLOWER_SOURCE_BASE
        + flowers[..model]
            .iter()
            .map(|f| 1 + f.heads.len() as u32)
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
pub(super) struct Source {
    pub(super) triangles: Vec<Triangle>,
    pub(super) ranges: Vec<[u32; 4]>,
    pub(super) frames: Vec<[f32; 4]>,
    palette: Vec<[f32; 4]>,
}
pub(super) fn source() -> Source {
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
    for (model, flower) in crate::flora::models::flowers().iter().enumerate() {
        let first = triangles.len() as u32;
        for t in &flower.triangles {
            let mut gpu = triangle(t.positions, [t.normal; 3], [Vec2::ZERO; 3], 0);
            gpu.uv01 = [
                f32::from(t.color[0]) / 255.,
                f32::from(t.color[1]) / 255.,
                f32::from(t.color[2]) / 255.,
                0.,
            ];
            let material = palette
                .iter()
                .position(|color| *color == gpu.uv01)
                .unwrap_or_else(|| {
                    palette.push(gpu.uv01);
                    palette.len() - 1
                });
            gpu.uv2[3] = material as f32;
            triangles.push(gpu);
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
    Source {
        triangles,
        ranges,
        frames,
        palette,
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
        let s = source();
        Self {
            model_bake_triangles: upload(bytemuck::cast_slice(&s.triangles)),
            model_bake_ranges: upload(bytemuck::cast_slice(&s.ranges)),
            model_bake_frames: upload(bytemuck::cast_slice(&s.frames)),
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Spec {
    pub(super) kind: u32,
    pub(super) resolution: u32,
    pub(super) views: u32,
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
#[derive(Clone)]
pub(super) struct CacheFrame {
    validation: Arc<Buffer>,
    entries: Arc<Buffer>,
}
impl CacheFrame {
    pub fn bindings(&self) -> [(&'static str, DescriptorResource<'_>); 2] {
        [
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
}
pub(super) struct ModelPixelCache {
    device: Device,
    allocator: Allocator,
    storage: GpuPagedStorage,
    active: [Option<(Spec, GpuStorageAllocation)>; KINDS],
    frames: Vec<Option<CacheFrame>>,
    diagnostics: Vec<Vec<(Spec, Arc<Buffer>)>>,
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
        views: u32,
        resolutions: [u32; KINDS],
    ) -> Result<()> {
        while self.frames.len() <= slot {
            self.frames.push(None);
            self.diagnostics.push(Vec::new());
        }
        for (spec, evidence) in self.diagnostics[slot].drain(..) {
            super::model_pixel_bake_validation::validate(spec, &evidence.read_back()?)?;
        }
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
            });
        }
        self.storage.begin_frame(slot);
        pipeline.begin_transient_descriptor_frame(slot);
        self.frames[slot]
            .as_ref()
            .unwrap()
            .validation
            .fill(&[0u32; KINDS * 4])?;
        let requested: [Spec; KINDS] = std::array::from_fn(|kind| Spec {
            kind: kind as u32,
            resolution: resolutions[kind],
            views,
        });
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
                views,
                shapes: SHAPES[spec.kind as usize],
                storage: allocation.handle(),
                verify: 0,
                evidence_stride: 0,
            };
            pipeline.record(
                cmd,
                Extent3D::new(spec.resolution, spec.resolution, views * push.shapes),
                Some(bytemuck::bytes_of(&push)),
            );
            if self.review {
                self.storage
                    .use_in_frame(slot, cmd, allocation, BufferUse::ComputeRead);
                let (stride, cases) = super::model_pixel_bake_validation::layout(*spec);
                let evidence = buffer(&self.device, &self.allocator, stride * cases * 16)?;
                let descriptors = [
                    (
                        "model_cache_validation",
                        DescriptorResource::Buffer(&self.frames[slot].as_ref().unwrap().validation),
                    ),
                    ("model_bake_evidence", DescriptorResource::Buffer(&evidence)),
                ];
                let verify = BakePush { verify: 1, ..push };
                pipeline.record_with_descriptors(
                    cmd,
                    &descriptors,
                    Extent3D::new(spec.resolution, spec.resolution, views * push.shapes),
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
                self.diagnostics[slot].push((*spec, evidence));
            }
            log::info!("[MODEL_CACHE_BUILD] kind={} views={views} resolution={} shapes={} bytes={} blocks={} resident_bytes={} version={FORMAT_VERSION}",spec.kind,spec.resolution,push.shapes,spec.records()?*SURFACE_BYTES,allocation.block_count(),allocation.resident_bytes());
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
        Ok(())
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
        let s = source();
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
        assert!(s.palette.len() < 256);
        for triangle in &s.triangles[s.ranges[FLOWER_SOURCE_BASE as usize][0] as usize..] {
            assert_eq!(s.palette[triangle.uv2[3] as usize], triangle.uv01);
        }
        for (model, f) in crate::flora::models::flowers().iter().enumerate() {
            for (part_index, p) in std::iter::once(&f.whole).chain(&f.heads).enumerate() {
                let id = flower_source(model, part_index) as usize;
                assert_eq!(s.frames[id], p.center.extend(p.radius).to_array());
                assert_eq!(s.ranges[id][1], p.triangles.end - p.triangles.start);
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
                views: 512
            }
            .records()
            .unwrap()
                * SURFACE_BYTES,
            4u64 << 30
        );
        assert!(Spec {
            kind: 0,
            resolution: 16,
            views: 0
        }
        .records()
        .is_err());
        assert_eq!(std::mem::size_of::<Entry>(), 40);
        assert_eq!(std::mem::offset_of!(Entry, resolution), 24);
        assert_eq!(std::mem::size_of::<BakePush>(), 48);
        assert_eq!(std::mem::offset_of!(BakePush, storage), 16);
    }
}
