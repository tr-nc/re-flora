//! Finite-volume surface deposition, independent of input, inventory and falling-particle FX.
//!
//! Samples clustered, soft-edged footprints. Each parcel follows a local supported surface
//! downhill before adding one empty voxel. Existing voxels (including soil state) never change.
//! Columns are local air intervals, not a world heightfield: ceilings stop deposition.
use crate::builder::{VOXEL_TYPE_DIRT, VOXEL_TYPE_MASK};
use crate::geom::UAabb3;
use glam::{UVec3, Vec3};
use std::collections::HashMap;
use std::time::{Duration, Instant};

const SETTLE_STEPS: u32 = 16;
pub(crate) const MAX_DEPOSITION_BATCH: u32 = 32_768;

#[derive(Clone, Copy, Debug)]
pub(crate) struct DepositionRequest {
    /// Voxel-space stroke, not a vertical stamp volume.
    pub start: Vec3,
    pub end: Vec3,
    pub radius: f32,
    pub volume: u32,
    pub clump_radius: u32,
    pub repose_slope: f32,
    pub seed: u32,
}

impl DepositionRequest {
    fn valid(self) -> bool {
        self.start.is_finite()
            && self.end.is_finite()
            && self.radius.is_finite()
            && self.radius > 0.0
            && (1..=4).contains(&self.clump_radius)
            && self.repose_slope.is_finite()
            && self.repose_slope >= 0.25
            && self.volume <= MAX_DEPOSITION_BATCH
    }

    /// Exclusive sampling bounds. Only X/Z depend on spread; Y retains the legal world range.
    pub(crate) fn sampling_bounds(self, legal: UAabb3) -> Option<UAabb3> {
        if !self.valid() || self.volume == 0 || legal.min().cmpge(legal.max()).any() {
            return None;
        }
        let halo = self.radius + self.clump_radius as f32 + SETTLE_STEPS as f32 + 1.0;
        let low = (self.start.min(self.end) - Vec3::splat(halo)).floor();
        let high = (self.start.max(self.end) + Vec3::splat(halo)).ceil();
        let min = UVec3::new(
            low.x.max(legal.min().x as f32) as u32,
            legal.min().y,
            low.z.max(legal.min().z as f32) as u32,
        );
        let max = UVec3::new(
            high.x.min(legal.max().x as f32).max(0.0) as u32,
            legal.max().y,
            high.z.min(legal.max().z as f32).max(0.0) as u32,
        );
        min.cmplt(max).all().then_some(UAabb3::new(min, max))
    }
}

#[derive(Debug, Default)]
pub(crate) struct DepositionResult {
    pub added: u32,
    /// Inclusive changed bounds, suitable for Visible Terrain Publication.
    pub changed: Option<UAabb3>,
}

#[derive(Clone, Copy)]
struct Column {
    next: u32,
    ceiling: u32,
}

struct Rng(u32);
impl Rng {
    fn unit(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 >> 8) as f32 / 16_777_216.0
    }

    fn disk(&mut self, radius: f32, soft: bool) -> (f32, f32) {
        let u = self.unit();
        // Density tends to zero at the footprint edge rather than tracing a hard circle.
        let r = radius
            * if soft {
                (1.0 - (1.0 - u).cbrt()).sqrt()
            } else {
                u.sqrt()
            };
        let angle = self.unit() * std::f32::consts::TAU;
        (r * angle.cos(), r * angle.sin())
    }
}

/// Mutates only empty atlas bytes. `floor` is an explicit external support plane (e.g. the
/// rooftop model), never an implicit floor for empty space. Offset/dim describe the readback.
pub(crate) fn deposit(
    data: &mut [u8],
    offset: UVec3,
    dim: UVec3,
    floor: Option<u32>,
    request: DepositionRequest,
) -> DepositionResult {
    let mut result = DepositionResult::default();
    if !request.valid()
        || dim.min_element() == 0
        || data.len() != dim.x as usize * dim.y as usize * dim.z as usize
    {
        return result;
    }
    let mut columns: HashMap<(u32, u32), Option<Column>> = HashMap::new();
    let mut rng = Rng(request.seed.max(1));
    let mut remaining = 0;
    let mut center = request.end;
    let mut clump_radius = 1.0;
    let mut changed_min = UVec3::splat(u32::MAX);
    let mut changed_max = UVec3::ZERO;
    let index = |x: u32, y: u32, z: u32| ((z * dim.y + y) * dim.x + x) as usize;

    for _ in 0..request.volume {
        if remaining == 0 {
            center = request.start.lerp(request.end, rng.unit());
            let (dx, dz) = rng.disk(request.radius, true);
            center.x += dx;
            center.z += dz;
            clump_radius = request.clump_radius as f32 * (0.5 + 0.5 * rng.unit());
            remaining = ((clump_radius.powi(3) * 4.0).round() as u32).max(1);
        }
        remaining -= 1;
        let (dx, dz) = rng.disk(clump_radius, false);
        let world = Vec3::new(center.x + dx, center.y, center.z + dz).floor();
        if world.x < offset.x as f32
            || world.z < offset.z as f32
            || world.x >= (offset.x + dim.x) as f32
            || world.z >= (offset.z + dim.z) as f32
        {
            continue;
        }
        let mut x = world.x as u32 - offset.x;
        let mut z = world.z as u32 - offset.z;
        let anchor = (center.y - offset.y as f32)
            .ceil()
            .clamp(0.0, (dim.y - 1) as f32) as u32;
        let floor_local = floor
            .and_then(|y| y.checked_sub(offset.y))
            .filter(|y| *y < dim.y);
        let column_at = |columns: &mut HashMap<_, _>, x, z| {
            *columns
                .entry((x, z))
                .or_insert_with(|| find_column(data, dim, x, z, anchor, floor_local))
        };
        let Some(mut column) = column_at(&mut columns, x, z) else {
            continue;
        };
        for _ in 0..SETTLE_STEPS {
            let mut best = (x, z, column);
            let turn = (rng.unit() * 8.0) as usize;
            const NEIGHBORS: [(i32, i32); 8] = [
                (1, 0),
                (1, 1),
                (0, 1),
                (-1, 1),
                (-1, 0),
                (-1, -1),
                (0, -1),
                (1, -1),
            ];
            for n in 0..8 {
                let (dx, dz) = NEIGHBORS[(n + turn) % 8];
                let nx = x as i32 + dx;
                let nz = z as i32 + dz;
                if nx < 0 || nz < 0 || nx >= dim.x as i32 || nz >= dim.z as i32 {
                    continue;
                }
                let Some(next) = column_at(&mut columns, nx as u32, nz as u32) else {
                    continue;
                };
                let distance = if dx != 0 && dz != 0 {
                    std::f32::consts::SQRT_2
                } else {
                    1.0
                };
                if (column.next + 1) as f32 - next.next as f32 > request.repose_slope * distance
                    && next.next < best.2.next
                    && next.next < next.ceiling
                    // Do not descend into an air interval through its solid ceiling.
                    && column.next <= next.ceiling
                {
                    best = (nx as u32, nz as u32, next);
                }
            }
            if (best.0, best.1) == (x, z) {
                break;
            }
            (x, z, column) = best;
        }
        if column.next >= column.ceiling {
            continue;
        }
        let pos = UVec3::new(x, column.next, z) + offset;
        // Column cache advances only after a real write; every accepted parcel has one owner.
        debug_assert_eq!(data[index(x, column.next, z)] & VOXEL_TYPE_MASK, 0);
        data[index(x, column.next, z)] = VOXEL_TYPE_DIRT as u8;
        columns.insert(
            (x, z),
            Some(Column {
                next: column.next + 1,
                ..column
            }),
        );
        result.added += 1;
        changed_min = changed_min.min(pos);
        changed_max = changed_max.max(pos);
    }
    if result.added > 0 {
        result.changed = Some(UAabb3::new(changed_min, changed_max));
    }
    result
}

fn find_column(
    data: &[u8],
    dim: UVec3,
    x: u32,
    z: u32,
    anchor: u32,
    floor: Option<u32>,
) -> Option<Column> {
    let solid = |y| data[((z * dim.y + y) * dim.x + x) as usize] & VOXEL_TYPE_MASK != 0;
    let mut y = anchor;
    if solid(y) {
        // A ceiling underside is not a route to the ground above it.
        if y > 0 && !solid(y - 1) && floor != Some(y) {
            return None;
        }
        let mut ascending_soil = false;
        while y < dim.y && solid(y) {
            let ty = data[((z * dim.y + y) * dim.x + x) as usize] & VOXEL_TYPE_MASK;
            // Following accumulated soil upward must not cross a fixed cap/other substrate.
            // This also keeps a stale ground hit from escaping a cavity filled last batch.
            if ascending_soil && ty != VOXEL_TYPE_DIRT as u8 {
                return None;
            }
            ascending_soil |= ty == VOXEL_TYPE_DIRT as u8;
            y += 1;
        }
    } else {
        while y > 0 && !solid(y - 1) && floor != Some(y) {
            y -= 1;
        }
        if y == 0 && floor != Some(0) {
            return None;
        }
    }
    if y >= dim.y {
        return None;
    }
    let next = y;
    while y < dim.y && !solid(y) {
        y += 1;
    }
    Some(Column { next, ceiling: y })
}

/// Time-based material budget. Reset on release/miss/tool change, retaining only the RNG serial.
/// A click releases one small quantum immediately; holding adds elapsed-time volume. Fractional
/// voxels survive sampling, so ordinary frame cadence and footprint size do not change flow.
/// Catch-up after a stall is capped at 250 ms; stopped time never produces a giant dose.
#[derive(Debug, Default)]
pub(crate) struct DepositionFlow {
    last: Option<Instant>,
    fraction: f64,
    serial: u32,
}
impl DepositionFlow {
    pub(crate) fn reset(&mut self) {
        self.last = None;
        self.fraction = 0.0;
    }

    pub(crate) fn take(&mut self, now: Instant, rate: f32, quantum: Duration) -> (u32, u32) {
        let elapsed = self
            .last
            .map_or(quantum, |last| now.saturating_duration_since(last));
        self.last = Some(now);
        let amount = if rate.is_finite() {
            f64::from(rate.max(0.0)) * elapsed.as_secs_f64().min(0.25) + self.fraction
        } else {
            0.0
        };
        let whole = amount.floor().min(f64::from(MAX_DEPOSITION_BATCH)) as u32;
        self.fraction = amount - amount.floor();
        self.serial = self.serial.wrapping_add(1);
        (whole, self.serial.wrapping_mul(0x9e3779b9))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(volume: u32) -> DepositionRequest {
        DepositionRequest {
            start: Vec3::new(24., 2., 24.),
            end: Vec3::new(24., 2., 24.),
            radius: 8.,
            volume,
            clump_radius: 2,
            repose_slope: 1.0,
            seed: 71,
        }
    }
    fn count(data: &[u8]) -> u32 {
        data.iter()
            .filter(|v| **v & VOXEL_TYPE_MASK == VOXEL_TYPE_DIRT as u8)
            .count() as u32
    }
    const DIM: UVec3 = UVec3::new(48, 48, 48);
    fn empty() -> Vec<u8> {
        vec![0; 48 * 48 * 48]
    }
    fn idx(x: u32, y: u32, z: u32) -> usize {
        ((z * DIM.y + y) * DIM.x + x) as usize
    }

    #[test]
    fn finite_sparse_coverage_accumulates_real_volume() {
        let mut data = empty();
        let first = deposit(&mut data, UVec3::ZERO, DIM, Some(2), request(64));
        assert_eq!(first.added, 64);
        assert_eq!(count(&data), 64);
        let first_height = first.changed.unwrap().max().y;
        let second = deposit(&mut data, UVec3::ZERO, DIM, Some(2), request(2048));
        assert_eq!(second.added, 2048);
        assert_eq!(count(&data), 2112);
        assert!(second.changed.unwrap().max().y > first_height);
    }

    #[test]
    fn repeated_deposition_has_no_brush_height_cap() {
        let mut data = empty();
        let mut r = request(2048);
        r.radius = 2.;
        r.clump_radius = 1;
        for seed in 1..5 {
            r.seed = seed;
            assert_eq!(
                deposit(&mut data, UVec3::ZERO, DIM, Some(2), r).added,
                r.volume
            );
        }
        assert_eq!(count(&data), 8192);
        assert!(data
            .iter()
            .enumerate()
            .any(|(i, v)| *v != 0 && (i as u32 / DIM.x) % DIM.y > 2 + r.radius as u32));
    }

    #[test]
    fn no_implicit_floor_and_no_material_loss_on_rejection() {
        let mut data = empty();
        assert_eq!(
            deposit(&mut data, UVec3::ZERO, DIM, None, request(128)).added,
            0
        );
        assert_eq!(count(&data), 0);
    }

    #[test]
    fn any_solid_substrate_works_without_overwriting_old_soil_state() {
        let mut data = empty();
        for z in 0..DIM.z {
            for x in 0..DIM.x {
                data[idx(x, 1, z)] = 0x87;
            }
        }
        data[idx(24, 2, 24)] = 0x32;
        let before = data.clone();
        let result = deposit(&mut data, UVec3::ZERO, DIM, None, request(512));
        assert_eq!(result.added, 512);
        for (old, new) in before.iter().zip(&data) {
            if *old != 0 {
                assert_eq!(old, new);
            } else {
                assert!(*new == 0 || *new == 2);
            }
        }
    }

    #[test]
    fn local_air_intervals_do_not_jump_to_a_cave_roof() {
        let mut data = empty();
        for z in 0..DIM.z {
            for x in 0..DIM.x {
                data[idx(x, 1, z)] = 7;
                data[idx(x, 12, z)] = 0x87;
            }
        }
        assert_eq!(
            deposit(&mut data, UVec3::ZERO, DIM, None, request(1024)).added,
            1024
        );
        assert!(data
            .iter()
            .enumerate()
            .all(|(i, v)| *v != 2 || (i as u32 / DIM.x) % DIM.y < 12));
        let mut underside = request(64);
        underside.start.y = 12.;
        underside.end.y = 12.;
        let mut blank_roof = empty();
        for z in 0..DIM.z {
            for x in 0..DIM.x {
                blank_roof[idx(x, 12, z)] = 7;
            }
        }
        assert_eq!(
            deposit(&mut blank_roof, UVec3::ZERO, DIM, None, underside).added,
            0
        );
    }

    #[test]
    fn deposition_is_deterministic_and_bounds_include_every_change() {
        let mut a = empty();
        let mut b = empty();
        let r = request(600);
        let result = deposit(&mut a, UVec3::ZERO, DIM, Some(2), r);
        deposit(&mut b, UVec3::ZERO, DIM, Some(2), r);
        assert_eq!(a, b);
        let bounds = result.changed.unwrap();
        for z in 0..DIM.z {
            for y in 0..DIM.y {
                for x in 0..DIM.x {
                    if a[idx(x, y, z)] != 0 {
                        let p = UVec3::new(x, y, z);
                        assert!(p.cmpge(bounds.min()).all() && p.cmple(bounds.max()).all());
                    }
                }
            }
        }
    }

    #[test]
    fn repeated_cavity_fill_never_crosses_a_fixed_ceiling() {
        let mut data = empty();
        for z in 0..DIM.z {
            for x in 0..DIM.x {
                data[idx(x, 1, z)] = 7;
                data[idx(x, 8, z)] = 0x87;
            }
        }
        let mut r = request(4096);
        let mut added = 0;
        for seed in 1..=4 {
            r.seed = seed;
            added += deposit(&mut data, UVec3::ZERO, DIM, None, r).added;
        }
        assert_eq!(count(&data), added);
        assert!(data
            .iter()
            .enumerate()
            .all(|(i, v)| *v != 2 || (i as u32 / DIM.x) % DIM.y < 8));
    }

    #[test]
    fn small_partial_doses_and_world_ceiling_keep_exact_ownership() {
        let mut data = empty();
        assert_eq!(
            deposit(&mut data, UVec3::ZERO, DIM, Some(2), request(3)).added,
            3
        );
        assert_eq!(count(&data), 3);
        let mut blocked = vec![7; data.len()];
        let before = blocked.clone();
        assert_eq!(
            deposit(&mut blocked, UVec3::ZERO, DIM, None, request(64)).added,
            0
        );
        assert_eq!(blocked, before);
        let mut high = request(64);
        high.start.y = 47.;
        high.end.y = 47.;
        let mut top = empty();
        let result = deposit(&mut top, UVec3::ZERO, DIM, Some(47), high);
        assert_eq!(count(&top), result.added);
        assert!(result.added <= high.volume);
    }

    #[test]
    fn fractional_surface_hits_do_not_reject_a_thin_platform_top() {
        let mut data = empty();
        for z in 0..DIM.z {
            for x in 0..DIM.x {
                data[idx(x, 10, z)] = 7;
            }
        }
        let mut r = request(64);
        r.start.y = 10.9999;
        r.end.y = 10.9999;
        assert_eq!(deposit(&mut data, UVec3::ZERO, DIM, None, r).added, 64);
        assert!(data
            .iter()
            .enumerate()
            .all(|(i, v)| *v != 2 || (i as u32 / DIM.x) % DIM.y >= 11));
    }

    #[test]
    fn sampling_clips_to_legal_space_but_not_brush_height() {
        let legal = UAabb3::new(UVec3::new(10, 2, 10), DIM);
        let bounds = request(1).sampling_bounds(legal).unwrap();
        assert_eq!(bounds.min().y, 2);
        assert_eq!(bounds.max().y, DIM.y);
        assert!(bounds.min().cmpge(legal.min()).all() && bounds.max().cmple(legal.max()).all());
        let mut invalid = request(1);
        invalid.radius = f32::NAN;
        assert!(invalid.sampling_bounds(legal).is_none());
    }

    #[test]
    fn time_budget_preserves_fractions_and_is_sampling_rate_independent() {
        let start = Instant::now();
        let quantum = Duration::from_millis(80);
        let mut fast = DepositionFlow::default();
        let mut slow = DepositionFlow::default();
        let mut a = fast.take(start, 101.25, quantum).0;
        let mut b = slow.take(start, 101.25, quantum).0;
        for i in 1..=100 {
            a += fast
                .take(start + Duration::from_millis(i * 10), 101.25, quantum)
                .0;
        }
        for i in 1..=10 {
            b += slow
                .take(start + Duration::from_millis(i * 100), 101.25, quantum)
                .0;
        }
        assert_eq!(a, b);
        assert_eq!(a, 109);
        fast.reset();
        assert_eq!(
            fast.take(start + Duration::from_secs(100), 101.25, quantum)
                .0,
            8
        );
    }
}
