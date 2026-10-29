//! Bounded conservative soil movement. A plan owns a complete replacement region;
//! failed/blocked plans never change their input and never expose a half-cut furrow.
use crate::builder::{VOXEL_TYPE_DIRT, VOXEL_TYPE_MASK};
use crate::geom::UAabb3;
use crate::terrain_deposition::{deposit_in_bounds_fast, DepositionRequest};
use glam::{UVec3, Vec3};

pub(crate) struct SoilMovePlan {
    pub origin: UVec3,
    pub dim: UVec3,
    pub data: Vec<u8>,
    pub deposited: Vec<UVec3>,
    pub moved: u32,
}

fn index(p: UVec3, origin: UVec3, dim: UVec3) -> usize {
    let p = p - origin;
    ((p.z * dim.y + p.y) * dim.x + p.x) as usize
}

/// Cut three layers of exposed soil in a horizontal capsule and settle exactly
/// those parcels into two side banks through the existing deposition planner.
/// `sample` is an exclusive, authoritative local region, not a heightfield.
/// Cohesive existing terrain stays supported; only the moved parcels settle.
pub(crate) fn plow(
    original: &[u8],
    sample: UAabb3,
    start: Vec3,
    end: Vec3,
    width: f32,
    repose_slope: f32,
) -> Option<SoilMovePlan> {
    let origin = sample.min();
    if sample.max().cmple(origin).any() {
        return None;
    }
    let dim = sample.max() - origin;
    if !start.is_finite()
        || !end.is_finite()
        || !width.is_finite()
        || !(2. ..=6.).contains(&width)
        || !repose_slope.is_finite()
        || repose_slope < 0.25
        || dim.min_element() == 0
        || dim.max_element() > 128
        || original.len() != dim.x as usize * dim.y as usize * dim.z as usize
        || start.distance(end) > 32.
    {
        return None;
    }
    let mut data = original.to_vec();
    let direction = Vec3::new(end.x - start.x, 0., end.z - start.z);
    let sideways = if direction.length_squared() > 0.01 {
        Vec3::new(-direction.z, 0., direction.x).normalize()
    } else {
        Vec3::Z
    };
    let mut removed = Vec::new();
    for z in origin.z..sample.max().z {
        for x in origin.x..sample.max().x {
            let delta = Vec3::new(x as f32 + 0.5 - start.x, 0., z as f32 + 0.5 - start.z);
            let t = if direction.length_squared() > 0.01 {
                (delta.dot(direction) / direction.length_squared()).clamp(0., 1.)
            } else {
                0.
            };
            if (delta - direction * t).length_squared() > width * width {
                continue;
            }
            let y = start.lerp(end, t).y;
            let high = ((y + 1.).floor().max(0.) as u32).min(sample.max().y - 1);
            let low = ((y - 3.).floor().max(0.) as u32).max(origin.y);
            if low > high {
                continue;
            }
            let mut top = None;
            for level in (low..=high).rev() {
                let p = UVec3::new(x, level, z);
                let material = data[index(p, origin, dim)] as u32 & VOXEL_TYPE_MASK as u32;
                if material != 0 {
                    if material == VOXEL_TYPE_DIRT
                        && level + 1 < sample.max().y
                        && data[index(p + UVec3::Y, origin, dim)] & VOXEL_TYPE_MASK == 0
                    {
                        top = Some(level);
                    }
                    break;
                }
            }
            if let Some(top) = top {
                for y in (top.saturating_sub(2).max(low)..=top).rev() {
                    let i = index(UVec3::new(x, y, z), origin, dim);
                    if data[i] as u32 & VOXEL_TYPE_MASK as u32 != VOXEL_TYPE_DIRT {
                        break;
                    }
                    removed.push(data[i] & !0x40); // grass coverage is derived at the new site
                    data[i] = 0;
                }
            }
        }
    }
    if removed.is_empty() {
        return None;
    }
    let moved = removed.len() as u32;
    let mut deposited = Vec::with_capacity(removed.len());
    for (side, volume) in [(-1., moved / 2), (1., moved - moved / 2)] {
        let shift = sideways * (side * (width + 6.)) + Vec3::Y * 2.;
        let result = deposit_in_bounds_fast(
            &mut data,
            origin,
            dim,
            None,
            DepositionRequest {
                start: start + shift,
                end: end + shift,
                radius: 1.,
                volume,
                clump_radius: 0,
                repose_slope,
                seed: 0x51a7 ^ volume,
            },
            sample,
        );
        // Roll back the entire speculative CPU plan, including successful grains
        // on the other side, if any part cannot fit. No GPU mutation has occurred.
        if result.added != volume || result.needs_more_space || result.needs_more_height {
            return None;
        }
        deposited.extend(result.deposited_voxels);
    }
    for (p, byte) in deposited.iter().zip(removed) {
        data[index(*p, origin, dim)] = byte;
    }
    // Crop the one atomic upload. Unchanged materials and every soil moisture
    // parcel are preserved; a publication failure cannot split debit and credit.
    let mut min = sample.max();
    let mut max = origin;
    for z in 0..dim.z {
        for y in 0..dim.y {
            for x in 0..dim.x {
                let p = origin + UVec3::new(x, y, z);
                let i = index(p, origin, dim);
                if data[i] != original[i] {
                    min = min.min(p);
                    max = max.max(p);
                }
            }
        }
    }
    if min.cmpgt(max).any() {
        return None;
    }
    let cropped_dim = max - min + UVec3::ONE;
    let mut cropped = Vec::with_capacity(cropped_dim.element_product() as usize);
    for z in min.z..=max.z {
        for y in min.y..=max.y {
            let i = index(UVec3::new(min.x, y, z), origin, dim);
            cropped.extend_from_slice(&data[i..i + cropped_dim.x as usize]);
        }
    }
    Some(SoilMovePlan {
        origin: min,
        dim: cropped_dim,
        data: cropped,
        deposited,
        moved,
    })
}

/// Only the shovel owns these parcels while they are outside terrain. Stored
/// bytes retain moisture; the derived grass bit is never portable inventory.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub(crate) struct CarriedSoil(Vec<u8>);
impl CarriedSoil {
    pub const CAPACITY: usize = 4096;
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.0.len() <= Self::CAPACITY
                && self
                    .0
                    .iter()
                    .all(|v| v & VOXEL_TYPE_MASK == VOXEL_TYPE_DIRT as u8),
            "invalid carried soil"
        );
        Ok(())
    }
    pub fn count(&self) -> u32 {
        self.0.len() as u32
    }
    pub fn parcels(&self) -> &[u8] {
        &self.0
    }
    pub fn debit_committed(&mut self, count: u32) {
        self.0.drain(..count as usize);
    }
}

/// One bounded scoop, leaving non-soil and all bytes outside the sphere intact.
/// Neither input nor inventory changes until the caller commits the returned plan.
pub(crate) fn scoop(
    original: &[u8],
    sample: UAabb3,
    center: Vec3,
    radius: f32,
) -> Option<(SoilMovePlan, CarriedSoil)> {
    if !center.is_finite()
        || !(2. ..=6.).contains(&radius)
        || sample.min().cmpge(sample.max()).any()
    {
        return None;
    }
    let origin = sample.min();
    let dim = sample.max() - origin;
    if dim.max_element() > 20 || original.len() != dim.element_product() as usize {
        return None;
    }
    let mut data = original.to_vec();
    let mut parcels = Vec::new();
    for z in 0..dim.z {
        for y in 0..dim.y {
            for x in 0..dim.x {
                let p = origin + UVec3::new(x, y, z);
                let i = index(p, origin, dim);
                if (p.as_vec3() + Vec3::splat(0.5)).distance_squared(center) <= radius * radius
                    && data[i] & VOXEL_TYPE_MASK == VOXEL_TYPE_DIRT as u8
                {
                    parcels.push(data[i] & !0x40);
                    data[i] = 0;
                }
            }
        }
    }
    if parcels.is_empty() || parcels.len() > CarriedSoil::CAPACITY {
        return None;
    }
    Some((
        SoilMovePlan {
            origin,
            dim,
            data,
            deposited: Vec::new(),
            moved: parcels.len() as u32,
        },
        CarriedSoil(parcels),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bed() -> (UAabb3, Vec<u8>) {
        let dim = UVec3::new(64, 24, 64);
        let mut data = vec![0; dim.element_product() as usize];
        for z in 0..64 {
            for y in 0..8 {
                for x in 0..64 {
                    data[index(UVec3::new(x, y, z), UVec3::ZERO, dim)] =
                        2 | ((x % 4) as u8 * 16) | 0x40;
                }
            }
        }
        (UAabb3::new(UVec3::ZERO, dim), data)
    }
    fn apply(data: &mut [u8], sample: UAabb3, plan: &SoilMovePlan) {
        for z in 0..plan.dim.z {
            for y in 0..plan.dim.y {
                for x in 0..plan.dim.x {
                    let p = plan.origin + UVec3::new(x, y, z);
                    data[index(p, sample.min(), sample.max() - sample.min())] =
                        plan.data[index(p, plan.origin, plan.dim)];
                }
            }
        }
    }
    fn moisture_histogram(data: &[u8]) -> [usize; 4] {
        let mut counts = [0; 4];
        for &b in data {
            if b & 15 == 2 {
                counts[(b as usize >> 4) & 3] += 1;
            }
        }
        counts
    }
    #[test]
    fn scoop_moves_only_dirt_into_finite_state_preserving_all_other_voxels() {
        let dim = UVec3::splat(16);
        let sample = UAabb3::new(UVec3::ZERO, dim);
        let mut data = vec![2; 4096];
        data[index(UVec3::splat(8), UVec3::ZERO, dim)] = 7;
        let original = data.clone();
        let (plan, carry) = scoop(&data, sample, Vec3::splat(8.), 6.).unwrap();
        carry.validate().unwrap();
        assert_eq!(data, original, "planning is cancellable");
        apply(&mut data, sample, &plan);
        assert_eq!(data[index(UVec3::splat(8), UVec3::ZERO, dim)], 7);
        assert_eq!(
            data.iter().filter(|&&b| b & 15 == 2).count() + carry.count() as usize,
            4095
        );
        assert!(carry.count() > 100);
    }
    #[test]
    fn drag_carves_and_banks_without_losing_any_soil_or_moisture() {
        let (sample, mut data) = bed();
        let before = data.clone();
        let plan = plow(
            &data,
            sample,
            Vec3::new(20., 8., 32.),
            Vec3::new(44., 8., 32.),
            3.,
            1.1,
        )
        .unwrap();
        assert!(plan.moved > 100);
        apply(&mut data, sample, &plan);
        assert_eq!(moisture_histogram(&data), moisture_histogram(&before));
        let dim = sample.max();
        assert_eq!(data[index(UVec3::new(32, 7, 32), UVec3::ZERO, dim)] & 15, 0);
        assert!(data
            .iter()
            .zip(&before)
            .any(|(&a, &b)| a & 15 == 2 && b == 0));
        assert_ne!(data, before);
    }
    #[test]
    fn blocked_side_cancels_even_after_other_side_has_deposited() {
        let (sample, mut data) = bed();
        let dim = sample.max();
        for z in 38..64 {
            for y in 8..24 {
                for x in 0..64 {
                    data[index(UVec3::new(x, y, z), UVec3::ZERO, dim)] = 1;
                }
            }
        }
        let before = data.clone();
        assert!(plow(
            &data,
            sample,
            Vec3::new(20., 8., 32.),
            Vec3::new(44., 8., 32.),
            3.,
            1.1
        )
        .is_none());
        assert_eq!(data, before);
    }
    #[test]
    fn unsupported_edges_invalid_requests_and_rock_never_destroy_material() {
        let (sample, data) = bed();
        assert!(plow(
            &data,
            sample,
            Vec3::new(20., 8., 2.),
            Vec3::new(44., 8., 2.),
            3.,
            1.1
        )
        .is_none());
        assert!(plow(&data, sample, Vec3::NAN, Vec3::ZERO, 3., 1.1).is_none());
        let rock = vec![1; data.len()];
        assert!(plow(&rock, sample, Vec3::splat(12.), Vec3::splat(12.), 3., 1.1).is_none());
    }
}
