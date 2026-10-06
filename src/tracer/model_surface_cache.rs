//! Immutable per-direction N×N model surfaces. Baking is CPU-only and asynchronous;
//! live poses/materials/lighting are applied at draw time. No simulation state is read.
use super::{
    model_geometry::{Source, Triangle},
    model_pixel_views,
};
use anyhow::{bail, Result};
use bytemuck::{Pod, Zeroable};
use glam::{Vec2, Vec3};
use re_flora_vkn::{vk, Allocator, Buffer, BufferUsage, Device, MemoryLocation};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver},
    Arc,
};

pub const MIN_RESOLUTION: u32 = 8;
pub const MAX_RESOLUTION: u32 = 32;
/// Order is apples, butterflies, flower heads; one saved owner per object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    pub views: u32,
    pub resolutions: [u32; 3],
    pub enabled: [bool; 3],
}
impl Default for Options {
    fn default() -> Self {
        Self {
            views: 32,
            resolutions: [32, 22, 16],
            enabled: [true; 3],
        }
    }
}
impl Options {
    pub fn for_kind(self, kind: usize) -> Self {
        let mut enabled = [false; 3];
        enabled[kind] = self.enabled[kind];
        Self {
            views: self.views,
            resolutions: [self.resolutions[kind]; 3],
            enabled,
        }
        .normalized()
    }
    pub fn normalized(self) -> Self {
        Self {
            views: model_pixel_views::runtime_count(self.views),
            resolutions: self
                .resolutions
                .map(|n| n.clamp(MIN_RESOLUTION, MAX_RESOLUTION)),
            ..self
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
struct Sample {
    position: [f32; 4],
    normal: [f32; 4],
    uv: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
struct Entry {
    layout: [u32; 4],
    bounds: [f32; 4],
}
struct Baked {
    samples: Vec<Sample>,
    entries: Vec<Entry>,
}
pub(super) struct Buffers {
    pub samples: Arc<Buffer>,
    pub entries: Arc<Buffer>,
    options: Options,
}
impl Buffers {
    fn upload(
        device: &Device,
        allocator: &Allocator,
        data: Baked,
        options: Options,
    ) -> Result<Self> {
        let upload = |bytes: &[u8]| -> Result<Arc<Buffer>> {
            let buffer = Arc::new(Buffer::try_new_sized(
                device.clone(),
                allocator.clone(),
                BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
                MemoryLocation::CpuToGpu,
                bytes.len().max(16) as u64,
            )?);
            if !bytes.is_empty() {
                buffer.fill_range_with_raw_u8(0, bytes)?;
            }
            Ok(buffer)
        };
        Ok(Self {
            samples: upload(bytemuck::cast_slice(&data.samples))?,
            entries: upload(bytemuck::cast_slice(&data.entries))?,
            options,
        })
    }
    pub fn empty(device: &Device, allocator: &Allocator, sources: usize) -> Result<Arc<Self>> {
        Ok(Arc::new(Self::upload(
            device,
            allocator,
            Baked {
                samples: vec![Sample::default()],
                entries: vec![Entry::default(); sources],
            },
            Options {
                enabled: [false; 3],
                ..Options::default()
            },
        )?))
    }
    pub fn cached(&self, kind: usize) -> bool {
        self.options.enabled[kind]
    }
    pub fn vertices(&self, kind: usize, native: u32) -> u32 {
        if self.options.enabled[kind] {
            self.options.resolutions[kind].pow(2) * 6
        } else {
            native
        }
    }
}
struct Pending {
    receiver: Receiver<Result<Baked>>,
    cancel: Arc<AtomicBool>,
}
impl Drop for Pending {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
#[derive(Default)]
pub(super) struct SurfaceCache {
    key: Option<(u64, Options)>,
    pending: Option<Pending>,
    active: Option<Arc<Buffers>>,
    failed: bool,
}
impl SurfaceCache {
    pub fn prepare(
        &mut self,
        device: &Device,
        allocator: &Allocator,
        generation: u64,
        source: Arc<Source>,
        options: Options,
        fallback: &Arc<Buffers>,
    ) -> Result<Arc<Buffers>> {
        let options = options.normalized();
        if self.key != Some((generation, options)) {
            self.pending = None;
            self.active = None;
            self.key = Some((generation, options));
            self.failed = false;
            if options.enabled.iter().any(|&v| v) {
                let cancel = Arc::new(AtomicBool::new(false));
                let stopped = cancel.clone();
                let (tx, rx) = mpsc::channel();
                if let Err(error) = std::thread::Builder::new()
                    .name("model-surface-bake".into())
                    .spawn(move || {
                        let result = bake(&source, options, &stopped);
                        let _ = tx.send(result);
                    })
                {
                    self.failed = true;
                    log::error!("[MODEL_SURFACE_CACHE] worker creation failed; native models remain active: {error:#}");
                    return Ok(fallback.clone());
                }
                self.pending = Some(Pending {
                    receiver: rx,
                    cancel,
                });
                log::info!("[MODEL_SURFACE_CACHE] queued generation={generation} views={} resolutions={:?} enabled={:?}",options.views,options.resolutions,options.enabled);
            }
        }
        if let Some(pending) = &self.pending {
            match pending.receiver.try_recv() {
                Ok(result) => {
                    self.pending = None;
                    match result {
                        Ok(data) => {
                            let bytes = data.samples.len() * std::mem::size_of::<Sample>();
                            match Buffers::upload(device, allocator, data, options) {
                                Ok(buffers) => self.active = Some(Arc::new(buffers)),
                                Err(error) => {
                                    self.failed = true;
                                    log::error!("[MODEL_SURFACE_CACHE] upload failed; native models remain active: {error:#}");
                                    return Ok(fallback.clone());
                                }
                            }
                            log::info!("[MODEL_SURFACE_CACHE] ready generation={generation} views={} resolutions={:?} bytes={bytes} geometry=prebaked_surfaces depth=per_cell lighting=live",options.views,options.resolutions);
                        }
                        Err(error) => {
                            self.failed = true;
                            log::error!("[MODEL_SURFACE_CACHE] bake failed; native models remain active: {error:#}");
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending = None;
                    self.failed = true;
                    log::error!(
                        "[MODEL_SURFACE_CACHE] worker disconnected; native models remain active"
                    );
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        Ok(self.active.as_ref().unwrap_or(fallback).clone())
    }
    pub fn ready(&self, options: Options) -> bool {
        !options.enabled.iter().any(|&v| v) || self.active.is_some() || self.failed
    }
}
/// A balanced triangle BVH prevents baking cost from multiplying every ray by
/// the full source triangle count. AABB traversal handles zero ray components.
struct Node {
    min: Vec3,
    max: Vec3,
    children: Option<(usize, usize)>,
    ids: Vec<usize>,
}
struct Bvh<'a> {
    triangles: &'a [Triangle],
    nodes: Vec<Node>,
}
impl<'a> Bvh<'a> {
    fn new(triangles: &'a [Triangle]) -> Self {
        let mut b = Self {
            triangles,
            nodes: Vec::new(),
        };
        b.build((0..triangles.len()).collect());
        b
    }
    fn bounds(&self, id: usize) -> (Vec3, Vec3) {
        let t = &self.triangles[id];
        let a = Vec3::from_slice(&t.a);
        let b = a + Vec3::from_slice(&t.e1);
        let c = a + Vec3::from_slice(&t.e2);
        (a.min(b).min(c), a.max(b).max(c))
    }
    fn build(&mut self, mut ids: Vec<usize>) -> usize {
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for &id in &ids {
            let (a, b) = self.bounds(id);
            min = min.min(a);
            max = max.max(b);
        }
        let index = self.nodes.len();
        self.nodes.push(Node {
            min,
            max,
            children: None,
            ids: Vec::new(),
        });
        if ids.len() <= 8 {
            self.nodes[index].ids = ids;
        } else {
            let span = max - min;
            let axis = if span.x >= span.y && span.x >= span.z {
                0
            } else if span.y >= span.z {
                1
            } else {
                2
            };
            ids.sort_by(|&a, &b| {
                let (a0, a1) = self.bounds(a);
                let (b0, b1) = self.bounds(b);
                (a0[axis] + a1[axis]).total_cmp(&(b0[axis] + b1[axis]))
            });
            let right = ids.split_off(ids.len() / 2);
            let a = self.build(ids);
            let b = self.build(right);
            self.nodes[index].children = Some((a, b));
        }
        index
    }
    fn hit(&self, origin: Vec3, direction: Vec3) -> Option<(usize, f32, Vec2)> {
        let mut best = f32::INFINITY;
        let mut hit = None;
        let mut stack = vec![0usize];
        while let Some(index) = stack.pop() {
            let node = &self.nodes[index];
            if !box_hit(node.min, node.max, origin, direction, best) {
                continue;
            }
            if let Some((a, b)) = node.children {
                stack.push(b);
                stack.push(a);
            } else {
                for &id in &node.ids {
                    if let Some((distance, uv)) =
                        triangle_hit(&self.triangles[id], origin, direction)
                    {
                        if distance < best
                            || (distance == best && hit.is_some_and(|(old, _, _)| id < old))
                        {
                            best = distance;
                            hit = Some((id, distance, uv));
                        }
                    }
                }
            }
        }
        hit
    }
}
fn box_hit(min: Vec3, max: Vec3, origin: Vec3, direction: Vec3, best: f32) -> bool {
    let mut near = 0f32;
    let mut far = best;
    for axis in 0..3 {
        if direction[axis].abs() < 1e-20 {
            if origin[axis] < min[axis] - 1e-6 || origin[axis] > max[axis] + 1e-6 {
                return false;
            }
        } else {
            let a = (min[axis] - origin[axis]) / direction[axis];
            let b = (max[axis] - origin[axis]) / direction[axis];
            near = near.max(a.min(b));
            far = far.min(a.max(b));
            if near > far {
                return false;
            }
        }
    }
    true
}
fn triangle_hit(t: &Triangle, origin: Vec3, direction: Vec3) -> Option<(f32, Vec2)> {
    let e1 = Vec3::from_slice(&t.e1);
    let e2 = Vec3::from_slice(&t.e2);
    let p = direction.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-10 {
        return None;
    }
    let v = origin - Vec3::from_slice(&t.a);
    let u = v.dot(p) / det;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = v.cross(e1);
    let w = direction.dot(q) / det;
    if w < 0. || u + w > 1. {
        return None;
    }
    let distance = e2.dot(q) / det;
    (distance >= 0.).then_some((distance, Vec2::new(u, w)))
}
pub(super) fn basis(direction: Vec3) -> (Vec3, Vec3) {
    let reference = if direction.y.abs() > 0.99 {
        Vec3::X
    } else {
        Vec3::Y
    };
    let x = reference.cross(direction).normalize();
    (x, direction.cross(x))
}
fn bake(source: &Source, options: Options, cancel: &AtomicBool) -> Result<Baked> {
    let options = options.normalized();
    let mut entries = vec![Entry::default(); source.ranges.len()];
    let mut samples = Vec::new();
    let heads: Vec<u32> = source
        .flower_parts
        .chunks(4)
        .flat_map(|p| p[1..].iter())
        .filter(|p| p.range[1] > 0)
        .map(|p| p.range[2])
        .collect();
    let needed: usize = source
        .ranges
        .iter()
        .enumerate()
        .filter(|(id, r)| {
            options.enabled[r[2] as usize - 1] && (r[2] != 3 || heads.contains(&(*id as u32)))
        })
        .map(|(_, r)| {
            options.views as usize * options.resolutions[r[2] as usize - 1].pow(2) as usize
        })
        .sum();
    samples.try_reserve_exact(needed)?;
    for (id, range) in source.ranges.iter().enumerate() {
        let kind = range[2] as usize - 1;
        if !options.enabled[kind] || (kind == 2 && !heads.contains(&(id as u32))) {
            continue;
        }
        let n = options.resolutions[kind];
        let triangles = &source.triangles[range[0] as usize..(range[0] + range[1]) as usize];
        let bvh = Bvh::new(triangles);
        let center = if kind == 1 {
            Vec3::ZERO
        } else {
            (bvh.nodes[0].min + bvh.nodes[0].max) * 0.5
        };
        let radius = if kind == 1 {
            1.7
        } else {
            triangles
                .iter()
                .flat_map(|t| {
                    let a = Vec3::from_slice(&t.a);
                    [a, a + Vec3::from_slice(&t.e1), a + Vec3::from_slice(&t.e2)]
                })
                .map(|p| p.distance(center))
                .fold(0f32, f32::max)
                .max(1e-5)
                * 1.01
        };
        let offset = u32::try_from(samples.len())?;
        entries[id] = Entry {
            layout: [offset, n, options.views, 1],
            bounds: center.extend(radius).to_array(),
        };
        for view in 0..options.views {
            if cancel.load(Ordering::Relaxed) {
                bail!("superseded model cache generation");
            }
            let direction = model_pixel_views::direction(view, options.views);
            let (x, y) = basis(direction);
            for row in 0..n {
                for col in 0..n {
                    let plane = center
                        + x * ((col as f32 + 0.5) / n as f32 * 2. - 1.) * radius
                        + y * ((row as f32 + 0.5) / n as f32 * 2. - 1.) * radius;
                    let origin = plane + direction * (radius * 2.);
                    let sample =
                        if let Some((triangle, distance, bary)) = bvh.hit(origin, -direction) {
                            let t = &triangles[triangle];
                            let weights = [1. - bary.x - bary.y, bary.x, bary.y];
                            let normal = (Vec3::from_slice(&t.normals[0]) * weights[0]
                                + Vec3::from_slice(&t.normals[1]) * weights[1]
                                + Vec3::from_slice(&t.normals[2]) * weights[2])
                                .normalize_or_zero();
                            let uv = Vec2::new(t.uv01[0], t.uv01[1]) * weights[0]
                                + Vec2::new(t.uv01[2], t.uv01[3]) * weights[1]
                                + Vec2::new(t.uv2[0], t.uv2[1]) * weights[2];
                            Sample {
                                position: (origin - direction * distance).extend(1.).to_array(),
                                normal: normal.extend(t.uv2[2]).to_array(),
                                uv: [uv.x, uv.y, 0., 0.],
                            }
                        } else {
                            Sample::default()
                        };
                    samples.push(sample);
                }
            }
        }
    }
    Ok(Baked { samples, entries })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn triangle(z: f32) -> Triangle {
        Triangle {
            a: [-1., -1., z, 0.],
            e1: [2., 0., 0., 0.],
            e2: [0., 2., 0., 0.],
            normals: [[0., 0., 1., 0.]; 3],
            uv01: [0., 0., 1., 0.],
            uv2: [0., 1., 2., 0.],
        }
    }
    #[test]
    fn bvh_matches_closest_triangle_and_misses() {
        let triangles = vec![triangle(0.), triangle(0.5)];
        let bvh = Bvh::new(&triangles);
        let (id, t, uv) = bvh.hit(Vec3::new(-0.5, -0.5, 2.), Vec3::NEG_Z).unwrap();
        assert_eq!(id, 1);
        assert_eq!(t, 1.5);
        assert_eq!(uv, Vec2::splat(0.25));
        assert!(bvh.hit(Vec3::new(3., 3., 2.), Vec3::NEG_Z).is_none());
        assert!(bvh.hit(Vec3::Z, Vec3::Z).is_none());
    }
    #[test]
    fn source_samples_are_bounded_and_no_leaf_sources_exist() {
        let source = super::super::model_geometry::source(crate::flora::models::Shape::default());
        let options = Options {
            views: 8,
            resolutions: [8; 3],
            enabled: [true, false, false],
        };
        let data = bake(&source, options, &AtomicBool::new(false)).unwrap();
        assert_eq!(data.samples.len(), 8 * 8 * 8);
        assert_eq!(data.entries[0].layout, [0, 8, 8, 1]);
        assert!(data.entries[1..].iter().all(|e| e.layout == [0; 4]));
        assert!(data.samples.iter().any(|s| s.position[3] == 1.));
        assert!(data.samples.iter().all(|s| s
            .position
            .iter()
            .chain(&s.normal)
            .chain(&s.uv)
            .all(|v| v.is_finite())));
        assert!(bake(&source, options, &AtomicBool::new(true)).is_err());
        assert_eq!(std::mem::size_of::<Sample>(), 48);
        assert_eq!(std::mem::size_of::<Entry>(), 32);
    }
    #[test]
    fn per_kind_keys_ignore_other_switches_and_resolutions() {
        let a = Options::default();
        let mut b = a;
        b.enabled[0] = false;
        b.resolutions[0] = 8;
        assert_eq!(a.for_kind(1), b.for_kind(1));
        assert_eq!(a.for_kind(2), b.for_kind(2));
        assert_ne!(a.for_kind(0), b.for_kind(0));
    }
    #[test]
    fn every_head_and_butterfly_frame_has_a_bounded_cache_entry() {
        let source = super::super::model_geometry::source(crate::flora::models::Shape::default());
        let data = bake(
            &source,
            Options {
                views: 8,
                resolutions: [8; 3],
                enabled: [true; 3],
            },
            &AtomicBool::new(false),
        )
        .unwrap();
        for part in source
            .flower_parts
            .chunks(4)
            .flat_map(|p| &p[1..])
            .filter(|p| p.range[1] > 0)
        {
            let entry = data.entries[part.range[2] as usize];
            assert_eq!(entry.layout[3], 1);
            assert!(entry.layout[0] as usize + 8 * 8 * 8 <= data.samples.len());
        }
        for frame in 0..super::super::model_geometry::ANIMATION_FRAMES {
            let id = super::super::model_geometry::BUTTERFLY_SOURCE_BASE + frame;
            assert_eq!(data.entries[id as usize].bounds, [0., 0., 0., 1.7]);
            let range = source.ranges[id as usize];
            for t in &source.triangles[range[0] as usize..(range[0] + range[1]) as usize] {
                let a = Vec3::from_slice(&t.a);
                for p in [a, a + Vec3::from_slice(&t.e1), a + Vec3::from_slice(&t.e2)] {
                    assert!(p.length() < 1.7);
                }
            }
        }
    }
    #[test]
    fn view_basis_matches_shader_and_forms_rigid_grid() {
        for count in [8, 128, 256, 512] {
            for i in 0..count {
                let z = model_pixel_views::direction(i, count);
                let (x, y) = basis(z);
                assert!(x.dot(y).abs() < 1e-6);
                assert!((x.cross(y) - z).length() < 1e-6);
            }
        }
        assert_eq!(
            Options {
                views: 0,
                resolutions: [0, 64, u32::MAX],
                enabled: [false, true, false]
            }
            .normalized(),
            Options {
                views: 8,
                resolutions: [8, 32, 32],
                enabled: [false, true, false]
            }
        );
    }
}
