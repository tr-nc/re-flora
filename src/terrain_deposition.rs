//! Finite-volume surface deposition, independent of input, inventory and falling-particle FX.
//!
//! Samples individual grains or optional clustered, soft-edged footprints. Each parcel follows a local supported surface
//! downhill before adding one empty voxel. Existing voxels (including soil state) never change.
//! Columns are local air intervals, not a world heightfield: ceilings stop deposition.
use crate::builder::{VOXEL_TYPE_DIRT, VOXEL_TYPE_MASK};
use crate::geom::UAabb3;
use glam::{UVec3, Vec3};
use std::collections::HashMap;
use std::time::{Duration, Instant};

// Initial readback reach, not a physical runout or pile-footprint limit.
const INITIAL_SETTLE_HALO: u32 = 32;
// Every f32 rate >= 1 is represented exactly at this scale. Integer nanosecond credit
// avoids frame-by-frame floating-point rounding losing a voxel at an integral boundary.
const FLOW_RATE_SCALE: u128 = 1 << 23;
const FLOW_EVENT_UNITS: u128 = FLOW_RATE_SCALE * 1_000_000_000;
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
            && self.radius >= 0.0
            && self.clump_radius <= 4
            && self.repose_slope.is_finite()
            && self.repose_slope >= 0.25
            && self.volume <= MAX_DEPOSITION_BATCH
    }

    pub(crate) fn horizontal_halo(self) -> f32 {
        self.radius + self.clump_radius as f32 + INITIAL_SETTLE_HALO as f32 + 1.
    }

    /// Minimum sampled height for the fast planner. Unknown space above remains expandable.
    pub(crate) fn initial_sampling_height(self, legal: UAabb3) -> u32 {
        (self.start.y.max(self.end.y).ceil() + 2.)
            .clamp(legal.min().y as f32 + 1., legal.max().y as f32) as u32
            - legal.min().y
    }

    /// Exclusive sampling bounds. Only X/Z depend on spread; original mode retains full legal Y.
    pub(crate) fn sampling_bounds(self, legal: UAabb3) -> Option<UAabb3> {
        self.sampling_bounds_with_halo(legal, self.horizontal_halo())
    }

    pub(crate) fn sampling_bounds_with_halo(self, legal: UAabb3, halo: f32) -> Option<UAabb3> {
        if !self.valid()
            || self.volume == 0
            || legal.min().cmpge(legal.max()).any()
            || !halo.is_finite()
            || halo < self.horizontal_halo()
        {
            return None;
        }
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

/// A spatial estimate only, never cached occupancy. Every request still reads authoritative bytes.
#[derive(Default)]
pub(crate) struct DepositionSamplingHint {
    last: Option<(Vec3, UAabb3, f32)>,
}

impl DepositionSamplingHint {
    pub(crate) fn initial_halo(&self, request: DepositionRequest, legal: UAabb3) -> f32 {
        let base = request.horizontal_halo();
        let Some((center, previous_legal, halo)) = self.last else {
            return base;
        };
        let delta = request.end - center;
        if legal == previous_legal && delta.x.abs().max(delta.z.abs()) <= INITIAL_SETTLE_HALO as f32
        {
            base.max(halo)
        } else {
            base
        }
    }

    pub(crate) fn remember(&mut self, request: DepositionRequest, legal: UAabb3, halo: f32) {
        self.last = Some((request.end, legal, halo));
    }

    pub(crate) fn reset(&mut self) {
        self.last = None;
    }
}

#[derive(Debug, Default, PartialEq)]
pub(crate) struct DepositionResult {
    pub added: u32,
    /// Exact committed writes. Sampling reach and the bounding box are not removal masks.
    pub deposited_voxels: Vec<UVec3>,
    /// Inclusive changed bounds, suitable for Visible Terrain Publication.
    pub changed: Option<UAabb3>,
    /// Speculative plan reached unread legal space; discard it and retry before uploading.
    pub needs_more_space: bool,
    /// Occupancy reaches the unread top; discard this plan and increase sampled height.
    pub needs_more_height: bool,
    pub sampling_halo: f32,
}

#[derive(Clone, Copy)]
struct Column {
    next: u32,
    ceiling: u32,
}

#[derive(Clone, Copy)]
enum ColumnSlot {
    Unread,
    Known(Option<Column>),
}

enum ColumnCache {
    Sparse(HashMap<(u32, u32), Option<Column>>),
    Dense { width: u32, slots: Vec<ColumnSlot> },
}
impl ColumnCache {
    fn new(dim: UVec3, dense: bool) -> Self {
        if dense {
            Self::Dense {
                width: dim.x,
                slots: vec![ColumnSlot::Unread; dim.x as usize * dim.z as usize],
            }
        } else {
            Self::Sparse(HashMap::new())
        }
    }
    fn get_or_find(
        &mut self,
        x: u32,
        z: u32,
        find: impl FnOnce() -> Option<Column>,
    ) -> Option<Column> {
        match self {
            Self::Sparse(columns) => *columns.entry((x, z)).or_insert_with(find),
            Self::Dense { width, slots } => {
                let slot = &mut slots[(z * *width + x) as usize];
                match *slot {
                    ColumnSlot::Known(column) => column,
                    ColumnSlot::Unread => {
                        let column = find();
                        *slot = ColumnSlot::Known(column);
                        column
                    }
                }
            }
        }
    }
    fn set(&mut self, x: u32, z: u32, column: Column) {
        match self {
            Self::Sparse(columns) => {
                columns.insert((x, z), Some(column));
            }
            Self::Dense { width, slots } => {
                slots[(z * *width + x) as usize] = ColumnSlot::Known(Some(column));
            }
        }
    }
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
#[cfg(test)]
fn deposit(
    data: &mut [u8],
    offset: UVec3,
    dim: UVec3,
    floor: Option<u32>,
    request: DepositionRequest,
) -> DepositionResult {
    let mut candidate = data.to_vec();
    let legal = UAabb3::new(offset, offset + dim);
    let reference = deposit_in_bounds(data, offset, dim, floor, request, legal);
    let optimized = deposit_in_bounds_fast(&mut candidate, offset, dim, floor, request, legal);
    assert_eq!(
        reference, optimized,
        "dense column lookup must preserve every planner result"
    );
    assert_eq!(
        data, candidate,
        "dense column lookup must preserve seeded atlas bytes"
    );
    reference
}

pub(crate) fn deposit_in_bounds(
    data: &mut [u8],
    offset: UVec3,
    dim: UVec3,
    floor: Option<u32>,
    request: DepositionRequest,
    legal: UAabb3,
) -> DepositionResult {
    deposit_impl(data, offset, dim, floor, request, legal, false)
}

pub(crate) fn deposit_in_bounds_fast(
    data: &mut [u8],
    offset: UVec3,
    dim: UVec3,
    floor: Option<u32>,
    request: DepositionRequest,
    legal: UAabb3,
) -> DepositionResult {
    deposit_impl(data, offset, dim, floor, request, legal, true)
}

fn deposit_impl(
    data: &mut [u8],
    offset: UVec3,
    dim: UVec3,
    floor: Option<u32>,
    request: DepositionRequest,
    legal: UAabb3,
    dense_columns: bool,
) -> DepositionResult {
    let mut result = DepositionResult::default();
    if !request.valid()
        || dim.min_element() == 0
        || data.len() != dim.x as usize * dim.y as usize * dim.z as usize
    {
        return result;
    }
    let mut columns = ColumnCache::new(dim, dense_columns);
    let unread_height = offset.y + dim.y < legal.max().y;
    let mut needs_more_height = false;
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
        let mut column_at = |columns: &mut ColumnCache, x, z| {
            columns.get_or_find(x, z, || {
                find_column(
                    data,
                    dim,
                    (x, z),
                    anchor,
                    floor_local,
                    unread_height,
                    &mut needs_more_height,
                )
            })
        };
        let Some(mut column) = column_at(&mut columns, x, z) else {
            continue;
        };
        if unread_height && column.next >= dim.y {
            needs_more_height = true;
            continue;
        }
        let source = (x, z);
        let source_height = (column.next + 1) as f32;
        // Every chosen hop strictly decreases the integer column height; this bound
        // guarantees termination without capping a tall pile's physical spread.
        for _ in 0..dim.y {
            // Loose soil cannot hold another grain on a one-column/one-axis pedestal,
            // even when a deliberately steep repose setting would otherwise let it stop.
            let supported_base = soil_base_is_broad(data, dim, x, z, column.next, floor_local);
            let mut best = (x, z, column);
            let mut best_grade = 0.;
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
                    let wx = offset.x as i64 + nx as i64;
                    let wz = offset.z as i64 + nz as i64;
                    let minimum_height = floor_local.unwrap_or(0) as f32;
                    let distance = (nx as f32 - source.0 as f32).hypot(nz as f32 - source.1 as f32);
                    if wx >= legal.min().x as i64
                        && wx < legal.max().x as i64
                        && wz >= legal.min().z as i64
                        && wz < legal.max().z as i64
                        && (!supported_base
                            || source_height - minimum_height > request.repose_slope * distance)
                    {
                        result.needs_more_space = true;
                    }
                    continue;
                }
                let Some(next) = column_at(&mut columns, nx as u32, nz as u32) else {
                    continue;
                };
                // Carry the grain's source height and true radial travel through the entire
                // slide. Resetting the comparison after each integer voxel hop turns both
                // unit and diagonal steps into a one-voxel limit: a square pyramid metric.
                let step = if dx != 0 && dz != 0 {
                    std::f32::consts::SQRT_2
                } else {
                    1.
                };
                let grade = column.next.saturating_sub(next.next) as f32 / step;
                // Keep the exact hypot metric, but only pay for eligible downhill hops.
                // Original mode retains the original evaluation path for runtime comparison.
                if dense_columns
                    && !(next.next <= column.next
                        && grade > best_grade
                        && next.next < next.ceiling
                        && column.next <= next.ceiling)
                {
                    continue;
                }
                let distance = (nx as f32 - source.0 as f32).hypot(nz as f32 - source.1 as f32);
                if (!supported_base || source_height - next.next as f32 > request.repose_slope * distance)
                    && next.next <= column.next
                    && grade > best_grade
                    && next.next < next.ceiling
                    // Do not descend into an air interval through its solid ceiling.
                    && column.next <= next.ceiling
                {
                    best = (nx as u32, nz as u32, next);
                    best_grade = grade;
                }
            }
            if (best.0, best.1) == (x, z) {
                break;
            }
            (x, z, column) = best;
        }
        if unread_height && column.next >= dim.y {
            needs_more_height = true;
        }
        if column.next >= column.ceiling
            || !soil_base_is_broad(data, dim, x, z, column.next, floor_local)
        {
            continue;
        }
        let pos = UVec3::new(x, column.next, z) + offset;
        // Column cache advances only after a real write; every accepted parcel has one owner.
        debug_assert_eq!(data[index(x, column.next, z)] & VOXEL_TYPE_MASK, 0);
        data[index(x, column.next, z)] = VOXEL_TYPE_DIRT as u8;
        columns.set(
            x,
            z,
            Column {
                next: column.next + 1,
                ..column
            },
        );
        result.added += 1;
        result.deposited_voxels.push(pos);
        changed_min = changed_min.min(pos);
        changed_max = changed_max.max(pos);
    }
    result.needs_more_height = needs_more_height;
    if result.added > 0 {
        result.changed = Some(UAabb3::new(changed_min, changed_max));
    }
    result
}

/// Rigid substrate / an explicit model plane can support a first grain. Stacking on
/// soil additionally needs solid shoulders under it in both horizontal axes. This is
/// a contact constraint, not a smoothing/deletion pass or a cap on a whole pile's height.
fn soil_base_is_broad(data: &[u8], dim: UVec3, x: u32, z: u32, y: u32, floor: Option<u32>) -> bool {
    if y == 0 || floor == Some(y) {
        return true;
    }
    let solid = |x, z| data[((z * dim.y + y - 1) * dim.x + x) as usize] & VOXEL_TYPE_MASK;
    if solid(x, z) != VOXEL_TYPE_DIRT as u8 {
        return true;
    }
    let x_support = (x > 0 && solid(x - 1, z) != 0) || (x + 1 < dim.x && solid(x + 1, z) != 0);
    let z_support = (z > 0 && solid(x, z - 1) != 0) || (z + 1 < dim.z && solid(x, z + 1) != 0);
    x_support && z_support
}

fn find_column(
    data: &[u8],
    dim: UVec3,
    (x, z): (u32, u32),
    anchor: u32,
    floor: Option<u32>,
    unread_height: bool,
    needs_more_height: &mut bool,
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
        *needs_more_height |= unread_height;
        return None;
    }
    let next = y;
    while y < dim.y && !solid(y) {
        y += 1;
    }
    Some(Column { next, ceiling: y })
}

/// Time-based material budget. Reset on release/miss/tool change, retaining only the RNG serial.
/// A press starts continuous emission immediately with one configured batch. Frequency
/// budgets subsequent whole batches from that press, without a separate tap/hold delay.
/// Fractional events survive sampling; frame cadence and footprint size do not change flow.
/// Catch-up after a stall is capped at 250 ms; stopped time never produces a giant dose.
#[derive(Debug, Default)]
pub(crate) struct DepositionFlow {
    last: Option<Instant>,
    fraction: u128,
    serial: u32,
}
impl DepositionFlow {
    pub(crate) fn reset(&mut self) {
        self.last = None;
        self.fraction = 0;
    }

    pub(crate) fn take(&mut self, now: Instant, rate: f32, batch_size: u32) -> (u32, u32) {
        let credit = if !rate.is_finite()
            || rate <= 0.
            || batch_size == 0
            || batch_size > MAX_DEPOSITION_BATCH
        {
            self.last = Some(now);
            self.fraction = 0;
            0
        } else if let Some(last) = self.last {
            if now < last {
                self.fraction
            } else {
                self.last = Some(now);
                let elapsed = now
                    .saturating_duration_since(last)
                    .min(Duration::from_millis(250));
                let scaled_rate = (f64::from(rate) * FLOW_RATE_SCALE as f64).round() as u128;
                elapsed
                    .as_nanos()
                    .saturating_mul(scaled_rate)
                    .saturating_add(self.fraction)
            }
        } else {
            // Repeat credit starts at press; only the configured batch interval separates releases.
            self.last = Some(now);
            FLOW_EVENT_UNITS
        };
        let max_batches = MAX_DEPOSITION_BATCH / batch_size.max(1);
        let whole = (credit / FLOW_EVENT_UNITS).min(u128::from(max_batches)) as u32 * batch_size;
        self.fraction = credit % FLOW_EVENT_UNITS;
        self.serial = self.serial.wrapping_add(1);
        (whole, self.serial.wrapping_mul(0x9e3779b9))
    }
}

#[cfg(test)]
#[path = "terrain_deposition/report.rs"]
mod report;

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
    fn stationary_pile_cross_sections_are_round_not_grid_aligned_pyramids() {
        let mut data = empty();
        let mut r = request(2048);
        r.radius = 0.;
        r.clump_radius = 0;
        for seed in 1..=8 {
            r.seed = seed;
            assert_eq!(
                deposit(&mut data, UVec3::ZERO, DIM, Some(2), r).added,
                r.volume
            );
        }
        assert_eq!(count(&data), 16384);
        let top = (2..DIM.y)
            .rev()
            .find(|&y| (0..DIM.z).any(|z| (0..DIM.x).any(|x| data[idx(x, y, z)] != 0)))
            .unwrap();
        for y in [2 + (top - 2) / 3, 2 + (top - 2) / 2] {
            let mut widths = Vec::new();
            for direction in 0..16 {
                let angle = direction as f32 * std::f32::consts::PI / 16.;
                let (sin, cos) = angle.sin_cos();
                let mut min = f32::INFINITY;
                let mut max = f32::NEG_INFINITY;
                for z in 0..DIM.z {
                    for x in 0..DIM.x {
                        if data[idx(x, y, z)] != 0 {
                            let projected = x as f32 * cos + z as f32 * sin;
                            min = min.min(projected);
                            max = max.max(projected);
                        }
                    }
                }
                widths.push(max - min);
            }
            let min = widths.iter().copied().fold(f32::INFINITY, f32::min);
            let max = widths.iter().copied().fold(0., f32::max);
            let anisotropy = max / min;
            assert!(anisotropy < 1.15, "stationary pile is directionally biased at y={y}: width ratio {anisotropy:.3}; round sections should not have square/diamond pyramid silhouettes");
        }
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
    fn random_individual_grains_have_a_broad_base_and_no_internal_voids() {
        let mut data = empty();
        for z in 0..DIM.z {
            for x in 0..DIM.x {
                data[idx(x, 1, z)] = 7;
            }
        }
        let mut r = request(1);
        r.clump_radius = 0;
        r.repose_slope = 2.;
        for grain in 1u32..=4096 {
            r.seed = grain.wrapping_mul(0x9e3779b9);
            let result = deposit(&mut data, UVec3::ZERO, DIM, None, r);
            assert_eq!(result.added, 1);
            let p = result.changed.unwrap().min();
            if p.y > 2 {
                let y = p.y - 1;
                let x_support = (p.x > 0 && data[idx(p.x - 1, y, p.z)] != 0)
                    || (p.x + 1 < DIM.x && data[idx(p.x + 1, y, p.z)] != 0);
                let z_support = (p.z > 0 && data[idx(p.x, y, p.z - 1)] != 0)
                    || (p.z + 1 < DIM.z && data[idx(p.x, y, p.z + 1)] != 0);
                assert!(x_support && z_support, "grain {grain} created a narrow elevated column at {p:?}: x_support={x_support}, z_support={z_support}");
            }
        }
        assert_eq!(count(&data), 4096);
        let heights: Vec<u32> = (0..DIM.z)
            .flat_map(|z| {
                let data = &data;
                (0..DIM.x).map(move |x| {
                    (2..DIM.y)
                        .filter(|&y| data[idx(x, y, z)] == VOXEL_TYPE_DIRT as u8)
                        .count() as u32
                })
            })
            .collect();
        for layer in 1..=*heights.iter().max().unwrap() {
            assert_eq!(
                super::report::layer_holes(&heights, DIM, layer),
                0,
                "layer {layer} has an interior mesh-like hole"
            );
        }
        for z in 0..DIM.z {
            for x in 0..DIM.x {
                let mut air = false;
                for y in 2..DIM.y {
                    if data[idx(x, y, z)] == 0 {
                        air = true;
                    } else {
                        assert!(!air, "soil bridged an unfilled column at ({x}, {y}, {z})");
                    }
                }
            }
        }
    }

    #[test]
    fn targeted_single_grain_changes_exactly_one_voxel() {
        let mut data = empty();
        let mut r = request(1);
        r.start = Vec3::new(24.5, 2., 24.5);
        r.end = r.start;
        r.radius = 0.;
        r.clump_radius = 0;
        let result = deposit(&mut data, UVec3::ZERO, DIM, Some(2), r);
        assert_eq!(result.added, 1);
        assert_eq!(count(&data), 1);
        assert_eq!(data[idx(24, 2, 24)], VOXEL_TYPE_DIRT as u8);
        assert_eq!(
            result.changed.unwrap(),
            UAabb3::new(UVec3::new(24, 2, 24), UVec3::new(24, 2, 24))
        );
    }

    #[test]
    fn clicks_and_holds_release_configurable_batches_of_individual_grains() {
        let start = Instant::now();
        for batch_size in [1, 10, 50, 512, 1024] {
            for rate in [1., 20., 120.] {
                let mut flow = DepositionFlow::default();
                assert_eq!(
                    flow.take(start, rate, batch_size).0,
                    batch_size,
                    "one click must emit the configured number of atomic voxels"
                );
                let mut added = batch_size;
                // Continuous batch credit starts at press, with no tap/hold grace.
                for i in 1..=20 {
                    let volume = flow
                        .take(start + Duration::from_millis(i * 50), rate, batch_size)
                        .0;
                    assert_eq!(volume % batch_size, 0);
                    added += volume;
                }
                assert_eq!(added, batch_size * (1 + rate as u32));
                flow.reset();
                assert_eq!(
                    flow.take(start + Duration::from_secs(10), rate, batch_size)
                        .0,
                    batch_size
                );
            }
        }
    }

    #[test]
    fn cropped_height_replay_matches_full_occupancy_including_caps_and_state_bytes() {
        let dim = UVec3::new(96, 64, 96);
        let legal = UAabb3::new(UVec3::ZERO, dim);
        let idx = |x, y, z| ((z * dim.y + y) * dim.x + x) as usize;
        for with_cap in [false, true] {
            let mut original = vec![0; (dim.x * dim.y * dim.z) as usize];
            for z in 0..dim.z {
                for x in 0..dim.x {
                    original[idx(x, 1, z)] = VOXEL_TYPE_DIRT as u8 | 0x30;
                }
            }
            if with_cap {
                for z in 42..55 {
                    for x in 42..55 {
                        original[idx(x, 12, z)] = crate::builder::VOXEL_TYPE_ROCK as u8;
                    }
                }
            }
            let r = DepositionRequest {
                start: Vec3::new(48., 2., 48.),
                end: Vec3::new(48., 2., 48.),
                radius: 13.,
                volume: 8192,
                clump_radius: 0,
                repose_slope: 1.,
                seed: 71,
            };
            let mut reference = original.clone();
            let full = deposit_in_bounds(&mut reference, UVec3::ZERO, dim, None, r, legal);
            let mut height = r.initial_sampling_height(legal);
            let mut retries = 0;
            loop {
                let sampled = UVec3::new(dim.x, height, dim.z);
                let mut data = vec![0; (sampled.x * sampled.y * sampled.z) as usize];
                for z in 0..dim.z {
                    for y in 0..height {
                        for x in 0..dim.x {
                            data[((z * height + y) * dim.x + x) as usize] = original[idx(x, y, z)];
                        }
                    }
                }
                let result =
                    deposit_in_bounds_fast(&mut data, UVec3::ZERO, sampled, None, r, legal);
                if result.needs_more_height {
                    height = (height * 2).min(dim.y);
                    retries += 1;
                    continue;
                }
                assert_eq!(result, full);
                let mut merged = original.clone();
                for z in 0..dim.z {
                    for y in 0..height {
                        for x in 0..dim.x {
                            merged[idx(x, y, z)] = data[((z * height + y) * dim.x + x) as usize];
                        }
                    }
                }
                assert_eq!(
                    merged, reference,
                    "cropped replay changed seeded geometry or old state bytes"
                );
                break;
            }
            assert!(
                retries > 0,
                "fixture must exercise discarded top-truncated plans"
            );
        }
    }

    #[test]
    fn sampling_hint_is_only_a_local_reach_estimate_not_an_occupancy_cache() {
        let legal = UAabb3::new(UVec3::ZERO, UVec3::new(256, 64, 256));
        let r = request(512);
        let base = r.horizontal_halo();
        let mut hint = DepositionSamplingHint::default();
        assert_eq!(hint.initial_halo(r, legal), base);
        hint.remember(r, legal, base * 2.);
        assert_eq!(hint.initial_halo(r, legal), base * 2.);
        assert_eq!(
            hint.initial_halo(
                DepositionRequest {
                    radius: base * 4.,
                    ..r
                },
                legal
            ),
            base * 4. + INITIAL_SETTLE_HALO as f32 + r.clump_radius as f32 + 1.
        );
        assert_eq!(
            hint.initial_halo(
                DepositionRequest {
                    end: r.end + Vec3::X * 64.,
                    ..r
                },
                legal
            ),
            base
        );
        assert_eq!(
            hint.initial_halo(r, UAabb3::new(UVec3::ZERO, UVec3::new(512, 64, 512))),
            base
        );
        hint.reset();
        assert_eq!(hint.initial_halo(r, legal), base);
    }

    #[test]
    fn unread_space_requests_a_retry_but_world_edges_do_not() {
        let dim = UVec3::new(16, 64, 16);
        let offset = UVec3::new(40, 0, 40);
        let mut data = vec![0; (dim.x * dim.y * dim.z) as usize];
        let r = DepositionRequest {
            start: Vec3::new(48., 2., 48.),
            end: Vec3::new(48., 2., 48.),
            radius: 0.,
            volume: 8192,
            clump_radius: 0,
            repose_slope: 1.,
            seed: 7,
        };
        let result = deposit_in_bounds(
            &mut data,
            offset,
            dim,
            Some(2),
            r,
            UAabb3::new(UVec3::ZERO, UVec3::new(128, 64, 128)),
        );
        assert!(
            result.needs_more_space,
            "an undersized readback must not become a pile wall"
        );
        data.fill(0);
        let bounded = deposit(&mut data, offset, dim, Some(2), r);
        assert!(
            !bounded.needs_more_space,
            "real world/work-area edges cannot expand"
        );
    }

    #[test]
    fn expanded_replay_matches_one_full_plan_without_duplicating_material() {
        let dim = UVec3::new(96, 64, 96);
        let legal = UAabb3::new(UVec3::ZERO, dim);
        let r = DepositionRequest {
            start: Vec3::new(48., 2., 48.),
            end: Vec3::new(48., 2., 48.),
            radius: 0.,
            volume: 16384,
            clump_radius: 0,
            repose_slope: 1.,
            seed: 713,
        };
        let mut original = vec![0; (dim.x * dim.y * dim.z) as usize];
        for seed in 1..=2 {
            deposit(
                &mut original,
                UVec3::ZERO,
                dim,
                Some(2),
                DepositionRequest { seed, ..r },
            );
        }
        let mut expected = original.clone();
        let full = deposit(&mut expected, UVec3::ZERO, dim, Some(2), r);
        let mut halo = r.horizontal_halo();
        let mut retries = 0;
        loop {
            let sample = r.sampling_bounds_with_halo(legal, halo).unwrap();
            let size = sample.max() - sample.min();
            let mut data = vec![0; (size.x * size.y * size.z) as usize];
            for z in 0..size.z {
                for y in 0..size.y {
                    for x in 0..size.x {
                        let p = sample.min() + UVec3::new(x, y, z);
                        data[((z * size.y + y) * size.x + x) as usize] =
                            original[((p.z * dim.y + p.y) * dim.x + p.x) as usize];
                    }
                }
            }
            let result = deposit_in_bounds(&mut data, sample.min(), size, Some(2), r, legal);
            if result.needs_more_space {
                halo *= 2.;
                retries += 1;
                continue;
            }
            assert_eq!(result.added, r.volume);
            assert_eq!(result.added, full.added);
            assert_eq!(result.changed, full.changed);
            for z in 0..size.z {
                for y in 0..size.y {
                    for x in 0..size.x {
                        let p = sample.min() + UVec3::new(x, y, z);
                        assert_eq!(
                            data[((z * size.y + y) * size.x + x) as usize],
                            expected[((p.z * dim.y + p.y) * dim.x + p.x) as usize]
                        );
                    }
                }
            }
            break;
        }
        assert!(
            retries > 0,
            "fixture must exercise discarded speculative plans"
        );
    }

    #[test]
    fn a_large_pile_outgrows_the_initial_readback_reach() {
        let dim = UVec3::new(96, 64, 96);
        let mut data = vec![0; (dim.x * dim.y * dim.z) as usize];
        let mut r = DepositionRequest {
            start: Vec3::new(48., 2., 48.),
            end: Vec3::new(48., 2., 48.),
            radius: 0.,
            volume: 16384,
            clump_radius: 0,
            repose_slope: 1.,
            seed: 1,
        };
        for seed in 1..=4 {
            r.seed = seed;
            assert_eq!(
                deposit(&mut data, UVec3::ZERO, dim, Some(2), r).added,
                r.volume
            );
        }
        let base: Vec<u32> = (0..dim.x)
            .filter(|&x| data[((48 * dim.y + 2) * dim.x + x) as usize] != 0)
            .collect();
        assert!(
            base.last().unwrap() - base.first().unwrap() > 64,
            "soil footprint was capped at the old 32-voxel slide/readback reach"
        );
        assert_eq!(
            data.iter().filter(|&&v| v == VOXEL_TYPE_DIRT as u8).count(),
            65536
        );
    }

    #[test]
    fn holding_repeats_from_press_without_a_click_hold_delay() {
        let start = Instant::now();
        for batch_size in [1, 10, 50, 512, 1024] {
            for rate in [20., 120., 1.] {
                let mut flow = DepositionFlow::default();
                assert_eq!(flow.take(start, rate, batch_size).0, batch_size);
                assert_eq!(
                    flow.take(start, rate, batch_size).0,
                    0,
                    "the press must not emit twice at the same instant"
                );
                let mut sampled_at = start;
                for repeat in 1..=2 {
                    let due = start
                        + Duration::from_nanos(
                            (1_000_000_000_f64 * f64::from(repeat) / f64::from(rate)).ceil() as u64,
                        );
                    // Keep sampling low rates too: a one-second gap would exercise the
                    // intentional stall catch-up cap, not ordinary continuous holding.
                    while due.duration_since(sampled_at) > Duration::from_millis(100) {
                        sampled_at += Duration::from_millis(100);
                        assert_eq!(flow.take(sampled_at, rate, batch_size).0, 0);
                    }
                    assert_eq!(
                        flow.take(due - Duration::from_nanos(1), rate, batch_size).0,
                        0
                    );
                    assert_eq!(flow.take(due, rate, batch_size).0, batch_size, "continuous emission must start at the first configured interval, without tap/hold grace");
                    sampled_at = due;
                }
            }
        }
    }

    #[test]
    fn sub_period_holds_do_not_repeat_and_bad_rates_cannot_create_material() {
        let start = Instant::now();
        let mut flow = DepositionFlow::default();
        assert_eq!(flow.take(start, 20., 1).0, 1);
        for ms in [1, 10, 25, 49] {
            assert_eq!(flow.take(start + Duration::from_millis(ms), 20., 1).0, 0);
        }
        assert_eq!(flow.take(start + Duration::from_millis(50), 20., 1).0, 1);
        assert_eq!(flow.take(start + Duration::from_millis(100), 20., 1).0, 1);
        assert_eq!(flow.take(start + Duration::from_millis(250), 20., 1).0, 3);
        assert_eq!(
            flow.take(start + Duration::from_secs(100), 20., 1).0,
            5,
            "stall catch-up must be capped at 250 ms"
        );
        for rate in [0., -1., f32::NAN, f32::INFINITY] {
            flow.reset();
            assert_eq!(flow.take(start, rate, 1).0, 0);
        }
    }

    #[test]
    fn batch_changes_apply_to_the_next_release_and_large_budgets_stay_bounded() {
        let start = Instant::now();
        let mut flow = DepositionFlow::default();
        assert_eq!(flow.take(start, 20., 10).0, 10);
        assert_eq!(flow.take(start + Duration::from_millis(25), 20., 10).0, 0);
        assert_eq!(flow.take(start + Duration::from_millis(50), 20., 50).0, 50);
        let now = start + Duration::from_secs(100);
        assert_eq!(
            flow.take(now, f32::MAX, 50).0,
            (MAX_DEPOSITION_BATCH / 50) * 50
        );
        assert_eq!(
            flow.take(now, 20., 50).0,
            0,
            "capped excess must not become a backlog"
        );
        for size in [0, u32::MAX] {
            flow.reset();
            assert_eq!(flow.take(now, 20., size).0, 0);
        }
    }

    #[test]
    fn a_narrow_soil_pedestal_is_not_allowed_to_grow_into_a_needle() {
        let mut data = empty();
        data[idx(24, 1, 24)] = 7;
        let mut r = request(1);
        r.start = Vec3::new(24.5, 2., 24.5);
        r.end = r.start;
        r.radius = 0.;
        r.clump_radius = 0;
        r.repose_slope = 2.;
        assert_eq!(deposit(&mut data, UVec3::ZERO, DIM, None, r).added, 1);
        r.volume = 8;
        assert_eq!(deposit(&mut data, UVec3::ZERO, DIM, None, r).added, 0);
        assert_eq!(count(&data), 1);
        assert_eq!(data[idx(24, 1, 24)], 7);
    }

    #[test]
    fn time_budget_preserves_fractions_and_is_sampling_rate_independent() {
        let start = Instant::now();
        for batch_size in [1, 10, 50, 512, 1024] {
            let mut fast = DepositionFlow::default();
            let mut slow = DepositionFlow::default();
            let mut a = fast.take(start, 101.25, batch_size).0;
            let mut b = slow.take(start, 101.25, batch_size).0;
            for i in 1..=100 {
                a += fast
                    .take(start + Duration::from_millis(i * 10), 101.25, batch_size)
                    .0;
            }
            for i in 1..=10 {
                b += slow
                    .take(start + Duration::from_millis(i * 100), 101.25, batch_size)
                    .0;
            }
            assert_eq!(a, b);
            assert_eq!(a, 102 * batch_size);
            fast.reset();
            assert_eq!(
                fast.take(start + Duration::from_secs(100), 101.25, batch_size)
                    .0,
                batch_size
            );
        }
    }
}
