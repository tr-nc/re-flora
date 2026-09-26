//! Shared startup-baked, relightable model surfaces. The public seam owns keys,
//! generation, memory bounds, publication and fence-scoped retirement. Callers
//! only request a configuration and bind the returned frame's resources.
use crate::{model_assets, resource::Resource};
use anyhow::{ensure, Result};
use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use re_flora_vkn::{
    vk, Allocator, Buffer, BufferUsage, BufferUse, CommandBuffer, ComputePipeline, DescriptorPool,
    DescriptorResource, Device, Extent3D, MemoryLocation, ShaderModule,
};
use resource_container_derive::ResourceContainer;
use std::sync::Arc;

pub const ANIMATION_FRAMES: u32 = 32;
const SHAPES: [u32; 3] = [64, 1, ANIMATION_FRAMES];
const FORMAT_VERSION: u32 = 1;
const SURFACE_BYTES: usize = 32;
// Per-kind bounded allocations, also below Vulkan's portable storage range.
// Unsupported combinations explicitly retain the live generator, never a stale
// cache or lower quality. Three kinds use at most 384 MiB per generation.
const MAX_BANK_BYTES: usize = 128 * 1024 * 1024;

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
// Immutable generation input. A later offline loader can supply the same baked
// surface format; it need not change any instance, lighting or display caller.
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
fn buffer(
    device: &Device,
    allocator: &Allocator,
    bytes: usize,
    location: MemoryLocation,
) -> Arc<Buffer> {
    Arc::new(Buffer::new_sized(
        device.clone(),
        allocator.clone(),
        BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
        location,
        bytes as u64,
    ))
}
#[derive(ResourceContainer)]
pub struct CacheResources {
    pub model_bake_triangles: Resource<Buffer>,
    pub model_bake_ranges: Resource<Buffer>,
    pub model_cache_leaves: Resource<Buffer>,
    pub model_cache_apples: Resource<Buffer>,
    pub model_cache_butterflies: Resource<Buffer>,
    pub model_cache_output: Resource<Buffer>,
    pub model_cache_validation: Resource<Buffer>,
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
        let dummy = || {
            let b = create(32);
            b.fill(&[0u32; 8]).unwrap();
            Resource::new(b)
        };
        Self {
            model_bake_triangles: Resource::new(geometry),
            model_bake_ranges: Resource::new(range_buffer),
            model_cache_leaves: dummy(),
            model_cache_apples: dummy(),
            model_cache_butterflies: dummy(),
            model_cache_output: dummy(),
            model_cache_validation: dummy(),
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
    fn bytes(self) -> Option<usize> {
        if !(8..=64).contains(&self.resolution) || !(8..=512).contains(&self.views) {
            return None;
        }
        let cells = SHAPES[self.kind as usize] as usize
            * self.views as usize
            * (self.resolution * self.resolution) as usize;
        let bytes = (cells + 1) * SURFACE_BYTES;
        (bytes <= MAX_BANK_BYTES).then_some(bytes)
    }
}
#[derive(Clone)]
pub struct CacheFrame {
    banks: [Arc<Buffer>; 3],
    validation: Arc<Buffer>,
}
impl CacheFrame {
    pub fn bindings(&self) -> [(&'static str, DescriptorResource<'_>); 4] {
        [
            (
                "model_cache_leaves",
                DescriptorResource::Buffer(&self.banks[0]),
            ),
            (
                "model_cache_apples",
                DescriptorResource::Buffer(&self.banks[1]),
            ),
            (
                "model_cache_butterflies",
                DescriptorResource::Buffer(&self.banks[2]),
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
    active: [Arc<Buffer>; 3],
    keys: [Option<Spec>; 3],
    empty: Arc<Buffer>,
    // Each slot retains the generation consumed by its pending submission.
    frames: Vec<Option<CacheFrame>>,
    review: bool,
}
impl ModelPixelCache {
    pub fn new(
        device: Device,
        allocator: Allocator,
        pool: &DescriptorPool,
        resources: &super::TracerResources,
    ) -> Self {
        let shader =
            ShaderModule::from_precompiled(&device, "shader/models/model_pixel_bake.comp", "main")
                .unwrap();
        let pipeline = ComputePipeline::new(
            &device,
            &shader,
            pool,
            &[&resources.model_cache, &resources.butterfly_mesh],
        );
        let empty = buffer(&device, &allocator, 32, MemoryLocation::CpuToGpu);
        empty.fill(&[0u32; 8]).unwrap();
        Self {
            device,
            allocator,
            pipeline,
            active: std::array::from_fn(|_| empty.clone()),
            keys: [None; 3],
            empty,
            frames: Vec::new(),
            review: std::env::var_os("RE_FLORA_MODEL_CACHE_REVIEW").is_some(),
        }
    }
    /// Called only after this frame slot's fence. Bake before any consumer, keep
    /// other slots' old buffers alive, and rebuild only changed kind keys.
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
        let validation = if let Some(frame) = self.frames[slot].take() {
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
                        "cached/live canonical surface mismatch"
                    );
                }
            }
            frame.validation
        } else {
            buffer(&self.device, &self.allocator, 32, MemoryLocation::CpuToGpu)
        };
        validation.fill(&[0u32; 8])?;
        self.pipeline.begin_transient_descriptor_frame(slot);
        for kind in 0..3 {
            let spec = Spec {
                kind: kind as u32,
                resolution: resolutions[kind].clamp(8, 64),
                views,
            };
            if self.keys[kind] == Some(spec) {
                continue;
            }
            if let Some(bytes) = spec.bytes() {
                let output = buffer(
                    &self.device,
                    &self.allocator,
                    bytes,
                    MemoryLocation::GpuOnly,
                );
                let start = std::time::Instant::now();
                self.pipeline.record_with_descriptors(
                    cmd,
                    &[("model_cache_output", DescriptorResource::Buffer(&output))],
                    Extent3D::new(spec.resolution, spec.resolution, views * SHAPES[kind]),
                    Some(bytemuck::bytes_of(&[
                        spec.kind,
                        spec.resolution,
                        views,
                        SHAPES[kind],
                    ])),
                )?;
                log::info!("[MODEL_CACHE_BUILD] kind={kind} views={views} resolution={} shapes={} bytes={bytes} version={FORMAT_VERSION} record_us={} (not GPU bake time)",
                    spec.resolution,SHAPES[kind],start.elapsed().as_micros());
                self.active[kind] = output;
            } else {
                self.active[kind] = self.empty.clone();
                if views != 0 {
                    log::warn!("[MODEL_CACHE_FALLBACK] kind={kind} views={views} resolution={} exceeds {} MiB bank budget; exact live generation retained",spec.resolution,MAX_BANK_BYTES/1024/1024);
                }
            }
            self.keys[kind] = Some(spec);
        }
        self.frames[slot] = Some(CacheFrame {
            banks: self.active.clone(),
            validation,
        });
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
            let delta = (animation_phase(f) - p + 0.5).rem_euclid(1.) - 0.5;
            assert!(delta.abs() <= 0.5 / ANIMATION_FRAMES as f32 + 1e-6);
        }
    }
    #[test]
    fn configurations_are_checked_before_allocation_and_only_depend_on_bake_inputs() {
        for kind in 0..3 {
            for views in 8..=512 {
                for resolution in [8, 16, 32, 64] {
                    let spec = Spec {
                        kind,
                        resolution,
                        views,
                    };
                    if let Some(bytes) = spec.bytes() {
                        assert!(bytes <= MAX_BANK_BYTES && bytes >= 32);
                    }
                }
            }
        }
        assert_eq!(
            Spec {
                kind: 0,
                resolution: 16,
                views: 16
            }
            .bytes(),
            Some(8 * 1024 * 1024 + 32)
        );
        assert!(Spec {
            kind: 0,
            resolution: 64,
            views: 512
        }
        .bytes()
        .is_none());
        assert!(Spec {
            kind: 0,
            resolution: 16,
            views: 0
        }
        .bytes()
        .is_none());
    }
}
