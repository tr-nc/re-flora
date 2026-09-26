//! Model pre-generation: sources, view/animation keys and rebuild policy live
//! here. Opaque GPU storage, paging, addressing and residency do not.
use crate::{model_assets, resource::Resource};
use anyhow::{ensure, Context, Result};
use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use re_flora_vkn::{
    vk, Allocator, Buffer, BufferUsage, BufferUse, CommandBuffer, ComputePipeline, DescriptorPool,
    DescriptorResource, Device, Extent3D, GpuPagedStorage, GpuStorageAllocation, GpuStorageHandle,
    MemoryLocation, ShaderModule, VulkanContext,
};
use resource_container_derive::ResourceContainer;
use std::{alloc::Layout, sync::Arc};

pub const ANIMATION_FRAMES: u32 = 32;
pub const BUTTERFLY_SOURCE_BASE: u32 = 65;
const SHAPES: [u32; 3] = [64, 1, ANIMATION_FRAMES];
const SURFACE_BYTES: u64 = 32;
const FORMAT_VERSION: u32 = 1;
pub fn animation_frame(phase: f32) -> u32 {
    ((phase.rem_euclid(1.) * ANIMATION_FRAMES as f32).round() as u32) % ANIMATION_FRAMES
}
pub fn animation_phase(frame: u32) -> f32 {
    frame as f32 / ANIMATION_FRAMES as f32
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Triangle {
    a: [f32; 4],
    e1: [f32; 4],
    e2: [f32; 4],
    normals: [[f32; 4]; 3],
    uv01: [f32; 4],
    uv2: [f32; 4],
}
fn triangle(p: [Vec3; 3], normals: [Vec3; 3], uv: [glam::Vec2; 3], material: u32) -> Triangle {
    Triangle {
        a: p[0].extend(0.).to_array(),
        e1: (p[1] - p[0]).extend(0.).to_array(),
        e2: (p[2] - p[0]).extend(0.).to_array(),
        normals: normals.map(|v| v.extend(0.).to_array()),
        uv01: [uv[0].x, uv[0].y, uv[1].x, uv[1].y],
        uv2: [uv[2].x, uv[2].y, material as f32, 0.],
    }
}
// One immutable generation input. Offline loading can later supply the same
// surface records without changing storage, instance lighting or display.
fn source() -> (Vec<Triangle>, Vec<[u32; 4]>) {
    let mut triangles = Vec::new();
    let mut ranges = Vec::new();
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
            [glam::Vec2::ZERO; 3],
            apple.materials[ids[0]] as u32,
        ));
    }
    ranges.push([start, triangles.len() as u32 - start, 1, 0]);
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
    }
    (triangles, ranges)
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
    pub model_cache_validation: Resource<Buffer>,
    pub model_cache_entries: Resource<Buffer>,
}
impl CacheResources {
    pub fn new(device: Device, allocator: Allocator) -> Self {
        let create = |bytes| {
            Buffer::new_sized(
                device.clone(),
                allocator.clone(),
                BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
                MemoryLocation::CpuToGpu,
                bytes,
            )
        };
        let (triangles, ranges) = source();
        let geometry = create(std::mem::size_of_val(triangles.as_slice()) as u64);
        geometry.fill(&triangles).expect("canonical model source");
        let range_buffer = create(std::mem::size_of_val(ranges.as_slice()) as u64);
        range_buffer.fill(&ranges).expect("canonical model ranges");
        let entries = create((3 * std::mem::size_of::<Entry>()) as u64);
        entries.fill(&[Entry::zeroed(); 3]).unwrap();
        let validation = create(32);
        validation.fill(&[0u32; 8]).unwrap();
        Self {
            model_bake_triangles: Resource::new(geometry),
            model_bake_ranges: Resource::new(range_buffer),
            model_cache_validation: Resource::new(validation),
            model_cache_entries: Resource::new(entries),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Spec {
    kind: u32,
    resolution: u32,
    views: u32,
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
// Scalars after the opaque handle match Slang's structured-buffer layout.
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
}
#[derive(Clone)]
pub struct CacheFrame {
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
pub struct ModelPixelCache {
    device: Device,
    allocator: Allocator,
    pipeline: ComputePipeline,
    storage: GpuPagedStorage,
    active: [Option<(Spec, GpuStorageAllocation)>; 3],
    frames: Vec<Option<CacheFrame>>,
    review: bool,
}
impl ModelPixelCache {
    pub fn new(
        context: &VulkanContext,
        allocator: Allocator,
        pool: &DescriptorPool,
        resources: &super::TracerResources,
    ) -> Self {
        let device = context.device();
        let shader =
            ShaderModule::from_precompiled(device, "shader/models/model_pixel_bake.comp", "main")
                .unwrap();
        let pipeline = ComputePipeline::new(
            device,
            &shader,
            pool,
            &[&resources.model_cache, &resources.butterfly_mesh],
        );
        Self {
            device: device.clone(),
            storage: GpuPagedStorage::new(context, allocator.clone()),
            allocator,
            pipeline,
            active: std::array::from_fn(|_| None),
            frames: Vec::new(),
            review: std::env::var_os("RE_FLORA_MODEL_CACHE_REVIEW").is_some(),
        }
    }
    /// A completed frame slot is required. Allocate all replacements first;
    /// failure is explicit and leaves the old generation unpublished/unchanged.
    /// There is no memory-budget fallback or per-instance geometry generation.
    pub fn prepare(
        &mut self,
        slot: usize,
        cmd: &CommandBuffer,
        views: u32,
        resolutions: [u32; 3],
    ) -> Result<()> {
        while self.frames.len() <= slot {
            self.frames.push(None);
        }
        if let Some(frame) = &self.frames[slot] {
            if self.review {
                let bytes = frame.validation.read_back()?;
                let counters: &[u32] = bytemuck::try_cast_slice(&bytes)
                    .map_err(|e| anyhow::anyhow!("cache counters: {e}"))?;
                if counters.iter().any(|&v| v != 0) {
                    log::info!(
                        "[MODEL_CACHE_ORACLE] leaf={} apple={} butterfly={} mismatches={}/{}/{}",
                        counters[0],
                        counters[2],
                        counters[4],
                        counters[1],
                        counters[3],
                        counters[5]
                    );
                    ensure!(
                        counters[1] == 0 && counters[3] == 0 && counters[5] == 0,
                        "stored/baked canonical surface mismatch"
                    );
                }
            }
        } else {
            self.frames[slot] = Some(CacheFrame {
                validation: buffer(&self.device, &self.allocator, 32)?,
                entries: buffer(
                    &self.device,
                    &self.allocator,
                    3 * std::mem::size_of::<Entry>(),
                )?,
            });
        }
        self.storage.begin_frame(slot);
        self.frames[slot]
            .as_ref()
            .unwrap()
            .validation
            .fill(&[0u32; 8])?;
        // Zero exists only for the pre-existing continuous numerical diagnostics.
        // It is never a runtime cache setting or a resource-pressure fallback.
        if views == 0 {
            self.active = std::array::from_fn(|_| None);
            self.frames[slot]
                .as_ref()
                .unwrap()
                .entries
                .fill(&[Entry::zeroed(); 3])?;
            return Ok(());
        }
        let requested: [Spec; 3] = std::array::from_fn(|kind| Spec {
            kind: kind as u32,
            resolution: resolutions[kind].clamp(8, 64),
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
                .with_context(||format!("cannot create model surfaces for {spec:?}; free GPU memory or reduce requested resolutions/view count"))?;
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
            };
            self.pipeline.record(
                cmd,
                Extent3D::new(spec.resolution, spec.resolution, views * push.shapes),
                Some(bytemuck::bytes_of(&push)),
            );
            log::info!("[MODEL_CACHE_BUILD] kind={} views={views} resolution={} shapes={} bytes={} blocks={} resident_bytes={} version={FORMAT_VERSION}",
                spec.kind,spec.resolution,push.shapes,spec.records()?*SURFACE_BYTES,allocation.block_count(),allocation.resident_bytes());
        }
        for (spec, allocation) in replacements {
            self.active[spec.kind as usize] = Some((spec, allocation));
        }
        let mut entries = [Entry::zeroed(); 3];
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
            .expect("prepared model cache")
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
    fn source_has_all_shapes_and_canonical_animation_frames() {
        let (triangles, ranges) = source();
        assert_eq!(std::mem::size_of::<Triangle>(), 128);
        assert_eq!(model_assets::LEAF_VARIANT_COUNT, SHAPES[0] as usize);
        assert_eq!(ranges.len(), 65 + ANIMATION_FRAMES as usize);
        for (index, r) in ranges.iter().enumerate() {
            assert_eq!(
                r[1],
                if index < 64 {
                    32
                } else if index == 64 {
                    780
                } else {
                    156
                }
            );
            assert!(r[0] as usize + r[1] as usize <= triangles.len());
            for t in &triangles[r[0] as usize..(r[0] + r[1]) as usize] {
                let a = Vec3::from_slice(&t.a);
                for p in [a, a + Vec3::from_slice(&t.e1), a + Vec3::from_slice(&t.e2)] {
                    assert!(p.is_finite() && p.length() < if index == 64 { 1.55 } else { 1.7 });
                }
            }
        }
    }
    #[test]
    fn frame_selection_wraps_and_has_bounded_error() {
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
    fn all_supported_specifications_describe_the_full_data_without_budget_truncation() {
        for kind in 0..3 {
            for views in 8..=512 {
                for resolution in 8..=64 {
                    let spec = Spec {
                        kind,
                        resolution,
                        views,
                    };
                    assert_eq!(
                        spec.records().unwrap(),
                        u64::from(SHAPES[kind as usize])
                            * u64::from(views)
                            * u64::from(resolution).pow(2)
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
        assert_eq!(std::mem::size_of::<BakePush>(), 40);
        assert_eq!(std::mem::offset_of!(BakePush, storage), 16);
    }
}
