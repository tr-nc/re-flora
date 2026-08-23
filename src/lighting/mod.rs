use bytemuck::Zeroable;
use glam::{Vec2, Vec3};
use std::sync::Arc;

use crate::generated::gpu_structs::{LightGpu, LocalLightInfo};

mod registry;
mod voxel_emissive;
pub(crate) use registry::*;
pub(crate) use voxel_emissive::*;

pub(crate) const LOCAL_LIGHT_GPU_ABI_VERSION: u32 = 2;
pub(crate) const LOCAL_LIGHT_FLAG_DDGI_TRACE_DIAGNOSTICS: u32 = 1 << 0;
/// First production small-N budget. CPU providers and the registry remain unbounded; selection is
/// explicit and can be replaced by a clustered/tiled policy without changing provider APIs.
pub(crate) const LOCAL_LIGHT_GPU_CAPACITY: usize = 8;
const LOCAL_LIGHT_KIND_POINT: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct LightId {
    slot: u32,
    generation: u32,
}

impl LightId {
    pub(crate) fn slot(self) -> u32 {
        self.slot
    }

    pub(crate) fn generation(self) -> u32 {
        self.generation
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PointLight {
    /// Position in authoritative world units. Terrain and raster hit positions use this space.
    pub position: Vec3,
    pub color: Vec3,
    pub intensity: f32,
    /// Near-field attenuation clamp radius in world units.
    pub source_radius: f32,
    /// Finite support radius in world units.
    pub range: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SpotLight {
    pub point: PointLight,
    pub direction: Vec3,
    pub inner_cone_radians: f32,
    pub outer_cone_radians: f32,
}

impl SpotLight {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        position: Vec3,
        direction: Vec3,
        color: Vec3,
        intensity: f32,
        source_radius: f32,
        range: f32,
        inner_cone_radians: f32,
        outer_cone_radians: f32,
    ) -> Result<Self, LocalLightValidationError> {
        let point = PointLight::new(position, color, intensity, source_radius, range)?;
        if !direction.is_finite()
            || direction.length_squared() <= 1.0e-8
            || !inner_cone_radians.is_finite()
            || !outer_cone_radians.is_finite()
            || inner_cone_radians < 0.0
            || inner_cone_radians > outer_cone_radians
            || outer_cone_radians >= std::f32::consts::FRAC_PI_2
        {
            return Err(LocalLightValidationError::InvalidSpotLight);
        }
        Ok(Self {
            point,
            direction: direction.normalize(),
            inner_cone_radians,
            outer_cone_radians,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RectangleAreaLight {
    pub position: Vec3,
    pub normal: Vec3,
    pub tangent: Vec3,
    pub half_extents: Vec2,
    pub color: Vec3,
    pub radiance: f32,
    pub range: f32,
}

impl RectangleAreaLight {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        position: Vec3,
        normal: Vec3,
        tangent: Vec3,
        half_extents: Vec2,
        color: Vec3,
        radiance: f32,
        range: f32,
    ) -> Result<Self, LocalLightValidationError> {
        if !position.is_finite()
            || !normal.is_finite()
            || normal.length_squared() <= 1.0e-8
            || !tangent.is_finite()
            || tangent.length_squared() <= 1.0e-8
            || !half_extents.is_finite()
            || half_extents.min_element() <= 0.0
            || !color.is_finite()
            || color.min_element() < 0.0
            || !radiance.is_finite()
            || radiance < 0.0
            || !range.is_finite()
            || range <= 0.0
        {
            return Err(LocalLightValidationError::InvalidAreaLight);
        }
        let normal = normal.normalize();
        let tangent = tangent - normal * tangent.dot(normal);
        if tangent.length_squared() <= 1.0e-8 {
            return Err(LocalLightValidationError::InvalidAreaLight);
        }
        Ok(Self {
            position,
            normal,
            tangent: tangent.normalize(),
            half_extents,
            color,
            radiance,
            range,
        })
    }
}

impl PointLight {
    pub(crate) fn new(
        position: Vec3,
        color: Vec3,
        intensity: f32,
        source_radius: f32,
        range: f32,
    ) -> Result<Self, LocalLightValidationError> {
        if !position.is_finite()
            || !color.is_finite()
            || color.min_element() < 0.0
            || !intensity.is_finite()
            || intensity < 0.0
            || !source_radius.is_finite()
            || source_radius <= 0.0
            || !range.is_finite()
            || range <= 0.0
            || source_radius > range
        {
            return Err(LocalLightValidationError::InvalidPointLight);
        }
        Ok(Self {
            position,
            color,
            intensity,
            source_radius,
            range,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum LocalLight {
    Point(PointLight),
    Spot(SpotLight),
    Area(RectangleAreaLight),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocalLightKind {
    Point,
    Spot,
    Area,
}

impl LocalLight {
    pub(crate) fn kind(self) -> LocalLightKind {
        match self {
            Self::Point(_) => LocalLightKind::Point,
            Self::Spot(_) => LocalLightKind::Spot,
            Self::Area(_) => LocalLightKind::Area,
        }
    }

    pub(crate) fn influence_bound(self) -> LocalLightInfluenceBound {
        match self {
            Self::Point(point) => LocalLightInfluenceBound::around(point.position, point.range),
            Self::Spot(spot) => {
                LocalLightInfluenceBound::around(spot.point.position, spot.point.range)
            }
            Self::Area(area) => {
                let bitangent = area.normal.cross(area.tangent);
                let shape_extent = area.tangent.abs() * area.half_extents.x
                    + bitangent.abs() * area.half_extents.y;
                LocalLightInfluenceBound::new(
                    area.position - shape_extent - Vec3::splat(area.range),
                    area.position + shape_extent + Vec3::splat(area.range),
                )
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LocalLightInfluenceBound {
    min: Vec3,
    max: Vec3,
}

impl LocalLightInfluenceBound {
    pub(crate) fn new(min: Vec3, max: Vec3) -> Self {
        debug_assert!(min.is_finite() && max.is_finite() && min.cmple(max).all());
        Self { min, max }
    }

    fn around(position: Vec3, range: f32) -> Self {
        Self::new(position - Vec3::splat(range), position + Vec3::splat(range))
    }

    pub(crate) fn min(self) -> Vec3 {
        self.min
    }

    pub(crate) fn max(self) -> Vec3 {
        self.max
    }

    pub(crate) fn union(self, other: Self) -> Self {
        Self::new(self.min.min(other.min), self.max.max(other.max))
    }
}

pub(crate) fn evaluate_unshadowed_point_irradiance(
    light: PointLight,
    receiver_position: Vec3,
    receiver_normal: Vec3,
) -> Vec3 {
    let to_light = light.position - receiver_position;
    let distance_squared = to_light.length_squared();
    if !distance_squared.is_finite() || distance_squared <= 1.0e-12 {
        return Vec3::ZERO;
    }
    let distance = distance_squared.sqrt();
    if distance >= light.range {
        return Vec3::ZERO;
    }
    let normal = receiver_normal.normalize_or_zero();
    let cosine = normal.dot(to_light / distance).max(0.0);
    if cosine <= 0.0 {
        return Vec3::ZERO;
    }
    let normalized_distance = distance / light.range;
    let range_window = (1.0 - normalized_distance.powi(4)).max(0.0).powi(2);
    let attenuation = range_window / distance_squared.max(light.source_radius.powi(2));
    light.color * (light.intensity * cosine * attenuation)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocalLightValidationError {
    InvalidPointLight,
    InvalidSpotLight,
    InvalidAreaLight,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocalLightMutationError {
    StaleId(LightId),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LocalLightRecord {
    id: LightId,
    source: LocalLightSourceId,
    light: LocalLight,
}

impl LocalLightRecord {
    pub(crate) fn id(self) -> LightId {
        self.id
    }

    pub(crate) fn light(self) -> LocalLight {
        self.light
    }

    pub(crate) fn source(self) -> LocalLightSourceId {
        self.source
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct LocalLightSnapshot {
    source_revision: u64,
    registry_revision: u64,
    lights: Arc<[LocalLightRecord]>,
}

impl LocalLightSnapshot {
    pub(crate) fn revision(&self) -> u64 {
        self.registry_revision
    }

    pub(crate) fn source_revision(&self) -> u64 {
        self.source_revision
    }

    pub(crate) fn registry_revision(&self) -> u64 {
        self.registry_revision
    }

    pub(crate) fn lights(&self) -> &[LocalLightRecord] {
        &self.lights
    }

    pub(crate) fn diff(&self, next: &Self) -> Vec<LocalLightChange> {
        let mut changes = Vec::new();
        let mut before_index = 0;
        let mut after_index = 0;
        while before_index < self.lights.len() || after_index < next.lights.len() {
            match (
                self.lights.get(before_index).copied(),
                next.lights.get(after_index).copied(),
            ) {
                (Some(before), Some(after)) if before.id == after.id => {
                    if before.light != after.light {
                        changes.push(LocalLightChange::Updated { before, after });
                    }
                    before_index += 1;
                    after_index += 1;
                }
                (Some(before), Some(after)) if before.id < after.id => {
                    changes.push(LocalLightChange::Removed(before));
                    before_index += 1;
                }
                (Some(_), Some(after)) => {
                    changes.push(LocalLightChange::Added(after));
                    after_index += 1;
                }
                (Some(before), None) => {
                    changes.push(LocalLightChange::Removed(before));
                    before_index += 1;
                }
                (None, Some(after)) => {
                    changes.push(LocalLightChange::Added(after));
                    after_index += 1;
                }
                (None, None) => break,
            }
        }
        changes
    }

    pub(crate) fn apply_budget(&self, budget: LocalLightBudget) -> LocalLightBudgetResult {
        StableSmallNLocalLightSelector.select(self, budget)
    }
}

/// Replaceable CPU selection seam. Phase 3 deliberately uses a camera-independent stable order;
/// a future clustered/tiled selector can implement this interface without widening provider APIs.
pub(crate) trait LocalLightSelector {
    fn select(
        &self,
        snapshot: &LocalLightSnapshot,
        budget: LocalLightBudget,
    ) -> LocalLightBudgetResult;
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct StableSmallNLocalLightSelector;

impl LocalLightSelector for StableSmallNLocalLightSelector {
    fn select(
        &self,
        snapshot: &LocalLightSnapshot,
        budget: LocalLightBudget,
    ) -> LocalLightBudgetResult {
        let mut ordered = snapshot.lights.to_vec();
        ordered.sort_by_key(|record| (record.source, record.id));
        let mut accepted =
            Vec::with_capacity(budget.point_light_capacity.min(snapshot.lights.len()));
        let mut overflow = Vec::new();
        for record in ordered {
            let reason = match record.light.kind() {
                LocalLightKind::Point if accepted.len() < budget.point_light_capacity => {
                    accepted.push(record);
                    continue;
                }
                LocalLightKind::Point => LocalLightOverflowReason::Capacity,
                LocalLightKind::Spot | LocalLightKind::Area => {
                    LocalLightOverflowReason::UnsupportedKind
                }
            };
            overflow.push(LocalLightOverflow {
                id: record.id,
                source: record.source,
                reason,
            });
        }
        LocalLightBudgetResult { accepted, overflow }
    }
}

impl LocalLightSnapshot {
    pub(crate) fn influence_bound(&self) -> Option<LocalLightInfluenceBound> {
        self.lights.iter().fold(None, |bound, record| {
            let light_bound = record.light.influence_bound();
            Some(
                bound.map_or(light_bound, |bound: LocalLightInfluenceBound| {
                    bound.union(light_bound)
                }),
            )
        })
    }

    pub(crate) fn impact_bound_to(&self, next: &Self) -> Option<LocalLightInfluenceBound> {
        self.diff(next).into_iter().fold(None, |bound, change| {
            let change_bound = match change {
                LocalLightChange::Added(after) => after.light.influence_bound(),
                LocalLightChange::Updated { before, after } => before
                    .light
                    .influence_bound()
                    .union(after.light.influence_bound()),
                LocalLightChange::Removed(before) => before.light.influence_bound(),
            };
            Some(
                bound.map_or(change_bound, |bound: LocalLightInfluenceBound| {
                    bound.union(change_bound)
                }),
            )
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum LocalLightChange {
    Added(LocalLightRecord),
    Updated {
        before: LocalLightRecord,
        after: LocalLightRecord,
    },
    Removed(LocalLightRecord),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LocalLightBudget {
    point_light_capacity: usize,
}

impl LocalLightBudget {
    pub(crate) fn point_lights(point_light_capacity: usize) -> Self {
        Self {
            point_light_capacity,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocalLightOverflowReason {
    Capacity,
    UnsupportedKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LocalLightOverflow {
    pub id: LightId,
    pub source: LocalLightSourceId,
    pub reason: LocalLightOverflowReason,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LocalLightBudgetResult {
    accepted: Vec<LocalLightRecord>,
    overflow: Vec<LocalLightOverflow>,
}

#[derive(Clone, Debug)]
pub(crate) struct LocalLightGpuSnapshot {
    pub info: LocalLightInfo,
    pub lights: [LightGpu; LOCAL_LIGHT_GPU_CAPACITY],
    pub overflow: Vec<LocalLightOverflow>,
}

/// Copyable transport payload. DDGI stores this value inside its immutable authored-lighting
/// snapshot, then uploads it into the private builder volume exactly once per transport revision.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LocalLightGpuPayload {
    pub info: LocalLightInfo,
    pub lights: [LightGpu; LOCAL_LIGHT_GPU_CAPACITY],
}

impl PartialEq for LocalLightGpuPayload {
    fn eq(&self, other: &Self) -> bool {
        bytemuck::bytes_of(&self.info) == bytemuck::bytes_of(&other.info)
            && bytemuck::cast_slice::<LightGpu, u8>(&self.lights)
                == bytemuck::cast_slice::<LightGpu, u8>(&other.lights)
    }
}

impl Eq for LocalLightGpuPayload {}

impl LocalLightGpuPayload {
    pub(crate) fn empty(source_revision: u64) -> Self {
        Self {
            info: LocalLightInfo {
                abi_version: LOCAL_LIGHT_GPU_ABI_VERSION,
                count: 0,
                capacity: LOCAL_LIGHT_GPU_CAPACITY as u32,
                overflow_count: 0,
                source_revision_low: source_revision as u32,
                source_revision_high: (source_revision >> 32) as u32,
                registry_revision_low: source_revision as u32,
                registry_revision_high: (source_revision >> 32) as u32,
                live_revision_low: source_revision as u32,
                live_revision_high: (source_revision >> 32) as u32,
                transport_revision: 0,
                flags: 0,
            },
            lights: [LightGpu::zeroed(); LOCAL_LIGHT_GPU_CAPACITY],
        }
    }

    pub(crate) fn source_revision(self) -> u64 {
        u64::from(self.info.source_revision_low) | (u64::from(self.info.source_revision_high) << 32)
    }

    pub(crate) fn registry_revision(self) -> u64 {
        u64::from(self.info.registry_revision_low)
            | (u64::from(self.info.registry_revision_high) << 32)
    }

    pub(crate) fn live_revision(self) -> u64 {
        u64::from(self.info.live_revision_low) | (u64::from(self.info.live_revision_high) << 32)
    }

    pub(crate) fn selection_eq(self, other: Self) -> bool {
        self.info.abi_version == other.info.abi_version
            && self.count() == other.count()
            && self.info.capacity == other.info.capacity
            && bytemuck::cast_slice::<LightGpu, u8>(&self.lights[..self.count() as usize])
                == bytemuck::cast_slice::<LightGpu, u8>(&other.lights[..other.count() as usize])
    }

    pub(crate) fn for_radiance_identity(mut self) -> Self {
        self.info.source_revision_low = 0;
        self.info.source_revision_high = 0;
        self.info.registry_revision_low = 0;
        self.info.registry_revision_high = 0;
        self.info.overflow_count = 0;
        self.info.transport_revision = 0;
        self.info.flags = 0;
        self
    }

    pub(crate) fn with_transport_revision(mut self, revision: u32) -> Self {
        self.info.transport_revision = revision;
        self
    }

    pub(crate) fn count(self) -> u32 {
        self.info.count.min(self.info.capacity)
    }

    pub(crate) fn light_index(self, id: LightId) -> Option<usize> {
        self.lights
            .iter()
            .take(self.count() as usize)
            .position(|light| {
                light.abi_version == LOCAL_LIGHT_GPU_ABI_VERSION
                    && light.stable_id_slot == id.slot
                    && light.stable_id_generation == id.generation
            })
    }

    pub(crate) fn influence_bound(self) -> Option<LocalLightInfluenceBound> {
        self.lights
            .iter()
            .take(self.count() as usize)
            .filter(|light| {
                light.abi_version == LOCAL_LIGHT_GPU_ABI_VERSION
                    && light.kind == LOCAL_LIGHT_KIND_POINT
            })
            .map(|light| LocalLightInfluenceBound::around(Vec3::from(light.position), light.range))
            .reduce(LocalLightInfluenceBound::union)
    }
}

impl LocalLightGpuSnapshot {
    pub(crate) fn from_authoritative(
        snapshot: &LocalLightSnapshot,
        budget: LocalLightBudget,
        transport_revision: u32,
    ) -> Self {
        let budget = LocalLightBudget::point_lights(
            budget.point_light_capacity.min(LOCAL_LIGHT_GPU_CAPACITY),
        );
        let result = snapshot.apply_budget(budget);
        let mut lights = std::array::from_fn(|_| LightGpu::zeroed());
        for (gpu, record) in lights.iter_mut().zip(result.accepted.iter().copied()) {
            let LocalLight::Point(point) = record.light() else {
                unreachable!("Phase 2 GPU budget only accepts point lights")
            };
            *gpu = LightGpu {
                abi_version: LOCAL_LIGHT_GPU_ABI_VERSION,
                kind: LOCAL_LIGHT_KIND_POINT,
                stable_id_slot: record.id().slot,
                stable_id_generation: record.id().generation,
                position: point.position.to_array(),
                range: point.range,
                color: point.color.to_array(),
                intensity: point.intensity,
                direction: Vec3::ZERO.to_array(),
                source_radius: point.source_radius,
                shape_params: [0.0; 4],
            };
        }
        Self {
            info: LocalLightInfo {
                abi_version: LOCAL_LIGHT_GPU_ABI_VERSION,
                count: result.accepted.len() as u32,
                capacity: LOCAL_LIGHT_GPU_CAPACITY as u32,
                overflow_count: result.overflow.len() as u32,
                source_revision_low: snapshot.source_revision as u32,
                source_revision_high: (snapshot.source_revision >> 32) as u32,
                registry_revision_low: snapshot.registry_revision as u32,
                registry_revision_high: (snapshot.registry_revision >> 32) as u32,
                live_revision_low: snapshot.registry_revision as u32,
                live_revision_high: (snapshot.registry_revision >> 32) as u32,
                transport_revision,
                flags: 0,
            },
            lights,
            overflow: result.overflow,
        }
    }

    pub(crate) fn with_flags(mut self, flags: u32) -> Self {
        self.info.flags = flags;
        self
    }

    pub(crate) fn with_live_revision(mut self, revision: u64) -> Self {
        self.info.live_revision_low = revision as u32;
        self.info.live_revision_high = (revision >> 32) as u32;
        self
    }

    pub(crate) fn payload(&self) -> LocalLightGpuPayload {
        LocalLightGpuPayload {
            info: self.info,
            lights: self.lights,
        }
    }
}

impl LocalLightBudgetResult {
    pub(crate) fn accepted(&self) -> &[LocalLightRecord] {
        &self.accepted
    }

    pub(crate) fn overflow(&self) -> &[LocalLightOverflow] {
        &self.overflow
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    fn point(position: Vec3) -> LocalLight {
        LocalLight::Point(
            PointLight::new(position, Vec3::ONE, 12.0, 0.02, 1.5)
                .expect("test point light must be valid"),
        )
    }

    #[test]
    fn removed_light_id_cannot_update_a_reused_slot() {
        let mut lights = LocalLightRegistry::default();
        let removed = lights.add(point(Vec3::new(0.2, 0.4, 0.6)));
        assert_eq!(removed.slot(), 0);
        assert_eq!(removed.generation(), 1);

        assert_eq!(lights.remove(removed), Ok(()));
        let replacement = lights.add(point(Vec3::new(0.7, 0.4, 0.6)));

        assert_eq!(replacement.slot(), removed.slot());
        assert_eq!(replacement.generation(), 2);
        assert_eq!(
            lights.update(removed, point(Vec3::new(0.9, 0.4, 0.6))),
            Err(LocalLightMutationError::StaleId(removed))
        );
        assert_eq!(lights.snapshot().lights()[0].id(), replacement);
    }

    #[test]
    fn snapshot_diff_names_only_real_lifecycle_changes_in_id_order() {
        let mut lights = LocalLightRegistry::default();
        let first = lights.add(point(Vec3::new(0.2, 0.4, 0.6)));
        let second = lights.add(point(Vec3::new(0.8, 0.4, 0.6)));
        let before = lights.snapshot();

        assert_eq!(
            lights.update(second, point(Vec3::new(0.8, 0.4, 0.6))),
            Ok(())
        );
        assert_eq!(lights.snapshot().revision(), before.revision());

        lights.remove(first).unwrap();
        lights
            .update(second, point(Vec3::new(0.7, 0.4, 0.6)))
            .unwrap();
        let third = lights.add(point(Vec3::new(0.1, 0.4, 0.6)));
        let after = lights.snapshot();

        assert_eq!(third.slot(), first.slot());
        assert_eq!(
            before.diff(&after),
            vec![
                LocalLightChange::Removed(before.lights()[0]),
                LocalLightChange::Added(after.lights()[0]),
                LocalLightChange::Updated {
                    before: before.lights()[1],
                    after: after.lights()[1],
                },
            ]
        );
        assert_eq!(after.lights()[1].id(), second);
    }

    #[test]
    fn point_upload_budget_is_explicit_and_deterministic() {
        let mut lights = LocalLightRegistry::default();
        let first = lights.add(point(Vec3::new(0.2, 0.4, 0.6)));
        let second = lights.add(point(Vec3::new(0.8, 0.4, 0.6)));

        let upload = lights
            .snapshot()
            .apply_budget(LocalLightBudget::point_lights(1));

        assert_eq!(
            upload
                .accepted()
                .iter()
                .map(|light| light.id())
                .collect::<Vec<_>>(),
            vec![first]
        );
        assert_eq!(
            upload.overflow(),
            &[LocalLightOverflow {
                id: second,
                source: lights.snapshot().lights()[1].source(),
                reason: LocalLightOverflowReason::Capacity,
            }]
        );
    }

    #[test]
    fn local_light_kinds_reserve_distinct_point_spot_and_area_domain_types() {
        let point = point(Vec3::new(0.2, 0.4, 0.6));
        let spot = LocalLight::Spot(
            SpotLight::new(
                Vec3::new(0.2, 0.4, 0.6),
                -Vec3::Y,
                Vec3::ONE,
                12.0,
                0.02,
                1.5,
                0.25,
                0.5,
            )
            .unwrap(),
        );
        let area = LocalLight::Area(
            RectangleAreaLight::new(
                Vec3::new(0.2, 0.4, 0.6),
                Vec3::Y,
                Vec3::X,
                glam::Vec2::new(0.1, 0.2),
                Vec3::ONE,
                4.0,
                1.5,
            )
            .unwrap(),
        );

        assert_eq!(point.kind(), LocalLightKind::Point);
        assert_eq!(spot.kind(), LocalLightKind::Spot);
        assert_eq!(area.kind(), LocalLightKind::Area);
    }

    #[test]
    fn movement_and_removal_dirty_the_old_and_current_point_influence() {
        let mut lights = LocalLightRegistry::default();
        let empty = lights.snapshot();
        let id = lights.add(point(Vec3::new(2.0, 3.0, 4.0)));
        let added = lights.snapshot();
        assert_eq!(
            empty.impact_bound_to(&added),
            Some(LocalLightInfluenceBound::new(
                Vec3::new(0.5, 1.5, 2.5),
                Vec3::new(3.5, 4.5, 5.5),
            ))
        );

        lights.update(id, point(Vec3::new(4.0, 3.0, 4.0))).unwrap();
        let moved = lights.snapshot();
        assert_eq!(
            added.impact_bound_to(&moved),
            Some(LocalLightInfluenceBound::new(
                Vec3::new(0.5, 1.5, 2.5),
                Vec3::new(5.5, 4.5, 5.5),
            ))
        );

        lights.remove(id).unwrap();
        assert_eq!(
            moved.impact_bound_to(&lights.snapshot()),
            moved.influence_bound()
        );
    }

    #[test]
    fn two_point_lights_add_linearly_in_scene_irradiance_units() {
        let first = PointLight::new(Vec3::Y, Vec3::new(1.0, 0.5, 0.25), 3.0, 0.01, 2.0).unwrap();
        let second = PointLight::new(Vec3::Y, Vec3::new(1.0, 0.5, 0.25), 5.0, 0.01, 2.0).unwrap();
        let result = evaluate_unshadowed_point_irradiance(first, Vec3::ZERO, Vec3::Y)
            + evaluate_unshadowed_point_irradiance(second, Vec3::ZERO, Vec3::Y);

        assert!(
            (result - Vec3::new(7.03125, 3.515625, 1.7578125))
                .abs()
                .max_element()
                < 1.0e-6
        );
    }

    #[test]
    fn versioned_gpu_snapshot_preserves_stable_identity_and_reports_overflow() {
        use crate::generated::gpu_structs::{LightGpu, LocalLightInfo};

        assert_eq!(std::mem::size_of::<LightGpu>(), 80);
        assert_eq!(std::mem::size_of::<LocalLightInfo>(), 48);

        let mut lights = LocalLightRegistry::default();
        let accepted_id = lights.add(point(Vec3::new(0.2, 0.4, 0.6)));
        let overflow_id = lights.add(point(Vec3::new(0.8, 0.4, 0.6)));
        let gpu = LocalLightGpuSnapshot::from_authoritative(
            &lights.snapshot(),
            LocalLightBudget::point_lights(1),
            7,
        );

        assert_eq!(gpu.info.abi_version, LOCAL_LIGHT_GPU_ABI_VERSION);
        assert_eq!(gpu.info.count, 1);
        assert_eq!(gpu.info.capacity, LOCAL_LIGHT_GPU_CAPACITY as u32);
        assert_eq!(gpu.info.overflow_count, 1);
        assert_eq!(gpu.info.source_revision_low, 2);
        assert_eq!(gpu.info.source_revision_high, 0);
        assert_eq!(gpu.info.registry_revision_low, 2);
        assert_eq!(gpu.info.live_revision_low, 2);
        assert_eq!(gpu.info.transport_revision, 7);
        assert_eq!(gpu.lights[0].stable_id_slot, accepted_id.slot());
        assert_eq!(gpu.lights[0].stable_id_generation, accepted_id.generation());
        assert_eq!(gpu.overflow[0].id, overflow_id);
    }

    #[test]
    fn production_consumers_share_the_voxel_visible_local_light_evaluator() {
        for source in [
            include_str!("../../shader/slang/tracer.slang"),
            include_str!("../../shader/slang/flora_lighting_cache.comp.slang"),
            include_str!("../../shader/slang/tree_leaf_lighting_cache.comp.slang"),
        ] {
            assert!(source.contains("import local_lighting;"));
            assert!(source.contains("evaluateVoxelVisibleLocalLightIrradiance"));
        }
        let ddgi = include_str!("../../shader/slang/ddgi_probe_trace.slang");
        assert!(ddgi.contains("import local_lighting;"));
        assert!(ddgi.contains("evaluateVoxelVisibleLocalLight("));

        let evaluator = include_str!("../../shader/slang/local_lighting.slang");
        assert!(evaluator.contains("for (uint lightIndex = 0u;"));
        assert!(evaluator.contains("marchScene("));
        assert!(evaluator.contains("shadowHit.distance"));
        assert!(evaluator.contains("light.position - shadowOrigin"));
        assert!(evaluator.contains("segmentLength - light.source_radius"));

        let diagnostic =
            include_str!("../../shader/slang/local_light_visibility_diagnostic.comp.slang");
        assert!(diagnostic.contains("findLocalLightIndexByStableId"));
        assert!(diagnostic.contains("evaluateVoxelVisibleLocalLightAtIndex"));
        assert!(!diagnostic.contains("localLightCount(local_light_info) == 1u"));
    }

    #[test]
    fn emissive_voxel_material_adds_surface_radiance_in_terrain_path_and_ddgi() {
        let types = include_str!("../../shader/slang/voxel_types.slang");
        assert!(types.contains("VOXEL_TYPE_EMISSIVE = 8u"));
        let material = include_str!("../../shader/slang/tracer_material.slang");
        assert!(material.contains("voxelSurfaceEmission"));
        assert!(material.contains("ddgiVoxelSurfaceEmission"));
        let terrain = include_str!("../../shader/slang/tracer.slang");
        assert!(terrain.contains("color += voxelSurfaceEmission"));
        assert!(terrain.contains("throughput * voxelSurfaceEmission"));
        let ddgi = include_str!("../../shader/slang/ddgi_probe_trace.slang");
        assert!(ddgi.contains("ddgiVoxelSurfaceEmission"));
    }

    #[test]
    fn provider_rebuild_and_reorder_preserve_unchanged_light_ids() {
        let provider = ProviderId::new(7);
        let first_key = SourceLightKey::new(11, 0);
        let second_key = SourceLightKey::new(22, 0);
        let mut registry = LocalLightRegistry::default();

        registry
            .reconcile(
                LocalLightProviderSnapshot::new(
                    provider,
                    1,
                    [
                        SourceLight::new(first_key, point(Vec3::new(0.2, 0.4, 0.6))),
                        SourceLight::new(second_key, point(Vec3::new(0.8, 0.4, 0.6))),
                    ],
                )
                .unwrap(),
            )
            .unwrap();
        let first_id = registry.light_id(provider, first_key).unwrap();
        let second_id = registry.light_id(provider, second_key).unwrap();
        let registry_revision = registry.registry_revision();

        let outcome = registry
            .reconcile(
                LocalLightProviderSnapshot::new(
                    provider,
                    2,
                    [
                        SourceLight::new(second_key, point(Vec3::new(0.8, 0.4, 0.6))),
                        SourceLight::new(first_key, point(Vec3::new(0.2, 0.4, 0.6))),
                    ],
                )
                .unwrap(),
            )
            .unwrap();

        assert_eq!(registry.light_id(provider, first_key), Some(first_id));
        assert_eq!(registry.light_id(provider, second_key), Some(second_id));
        assert_eq!(registry.registry_revision(), registry_revision);
        assert_eq!(outcome.source_revision, 2);
        assert_eq!(outcome.registry_revision, registry_revision);
        assert_eq!(outcome.added, 0);
        assert_eq!(outcome.updated, 0);
        assert_eq!(outcome.removed, 0);
        assert_eq!(registry.snapshot().source_revision(), 2);
        assert_eq!(registry.snapshot().registry_revision(), registry_revision);
        assert_ne!(
            registry.snapshot().source_revision(),
            registry.snapshot().registry_revision()
        );
    }

    #[test]
    fn provider_lifecycle_reuses_slots_only_with_a_new_generation() {
        let provider = ProviderId::new(8);
        let removed_key = SourceLightKey::new(1, 0);
        let retained_key = SourceLightKey::new(2, 0);
        let added_key = SourceLightKey::new(3, 0);
        let mut registry = LocalLightRegistry::default();
        registry
            .reconcile(
                LocalLightProviderSnapshot::new(
                    provider,
                    1,
                    [
                        SourceLight::new(removed_key, point(Vec3::X)),
                        SourceLight::new(retained_key, point(Vec3::Y)),
                    ],
                )
                .unwrap(),
            )
            .unwrap();
        let removed_id = registry.light_id(provider, removed_key).unwrap();
        let retained_id = registry.light_id(provider, retained_key).unwrap();

        let outcome = registry
            .reconcile(
                LocalLightProviderSnapshot::new(
                    provider,
                    2,
                    [
                        SourceLight::new(retained_key, point(Vec3::new(0.0, 2.0, 0.0))),
                        SourceLight::new(added_key, point(Vec3::Z)),
                    ],
                )
                .unwrap(),
            )
            .unwrap();
        let added_id = registry.light_id(provider, added_key).unwrap();

        assert_eq!((outcome.added, outcome.updated, outcome.removed), (1, 1, 1));
        assert_eq!(registry.light_id(provider, retained_key), Some(retained_id));
        assert_eq!(added_id.slot(), removed_id.slot());
        assert_eq!(added_id.generation(), removed_id.generation() + 1);
        assert_eq!(registry.snapshot().lights().len(), 2);

        let disappearance = registry.remove_provider(provider);
        assert_eq!(disappearance.removed, 2);
        assert!(registry.snapshot().lights().is_empty());
        assert_eq!(registry.light_id(provider, retained_key), None);
        assert_eq!(registry.light_id(provider, added_key), None);

        registry
            .reconcile(
                LocalLightProviderSnapshot::new(
                    provider,
                    1,
                    [SourceLight::new(removed_key, point(Vec3::X))],
                )
                .unwrap(),
            )
            .unwrap();
        let rebuilt_id = registry.light_id(provider, removed_key).unwrap();
        assert_ne!(rebuilt_id, removed_id);
        assert!(rebuilt_id.generation() > removed_id.generation());
    }

    #[test]
    fn provider_snapshot_rejects_duplicate_and_conflicting_source_revisions() {
        let provider = ProviderId::new(9);
        let key = SourceLightKey::new(4, 5);
        assert_eq!(
            LocalLightProviderSnapshot::new(
                provider,
                1,
                [
                    SourceLight::new(key, point(Vec3::X)),
                    SourceLight::new(key, point(Vec3::Y)),
                ],
            ),
            Err(LocalLightProviderSnapshotError::DuplicateSourceKey { provider, key })
        );

        let mut registry = LocalLightRegistry::default();
        registry
            .reconcile(
                LocalLightProviderSnapshot::new(
                    provider,
                    2,
                    [SourceLight::new(key, point(Vec3::X))],
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            registry.reconcile(
                LocalLightProviderSnapshot::new(
                    provider,
                    2,
                    [SourceLight::new(key, point(Vec3::Y))],
                )
                .unwrap(),
            ),
            Err(LocalLightReconcileError::SourceRevisionCollision {
                provider,
                revision: 2,
            })
        );
        assert_eq!(
            registry.reconcile(
                LocalLightProviderSnapshot::new(
                    provider,
                    1,
                    [SourceLight::new(key, point(Vec3::X))],
                )
                .unwrap(),
            ),
            Err(LocalLightReconcileError::StaleSourceRevision {
                provider,
                current: 2,
                incoming: 1,
            })
        );
    }

    #[test]
    fn small_n_selection_uses_provider_source_order_and_reports_every_rejection() {
        let mut registry = LocalLightRegistry::default();
        let later_provider = ProviderId::new(20);
        let earlier_provider = ProviderId::new(10);
        registry
            .reconcile(
                LocalLightProviderSnapshot::new(
                    later_provider,
                    1,
                    [
                        SourceLight::new(SourceLightKey::new(2, 0), point(Vec3::X)),
                        SourceLight::new(SourceLightKey::new(1, 0), point(Vec3::Y)),
                    ],
                )
                .unwrap(),
            )
            .unwrap();
        registry
            .reconcile(
                LocalLightProviderSnapshot::new(
                    earlier_provider,
                    1,
                    [
                        SourceLight::new(SourceLightKey::new(9, 0), point(Vec3::Z)),
                        SourceLight::new(
                            SourceLightKey::new(10, 0),
                            LocalLight::Spot(
                                SpotLight::new(
                                    Vec3::Y,
                                    -Vec3::Y,
                                    Vec3::ONE,
                                    1.0,
                                    0.01,
                                    1.0,
                                    0.1,
                                    0.2,
                                )
                                .unwrap(),
                            ),
                        ),
                    ],
                )
                .unwrap(),
            )
            .unwrap();

        let selection = registry
            .snapshot()
            .apply_budget(LocalLightBudget::point_lights(2));
        assert_eq!(
            selection
                .accepted()
                .iter()
                .map(|record| record.source())
                .collect::<Vec<_>>(),
            vec![
                LocalLightSourceId::new(earlier_provider, SourceLightKey::new(9, 0)),
                LocalLightSourceId::new(later_provider, SourceLightKey::new(1, 0)),
            ]
        );
        assert_eq!(selection.overflow().len(), 2);
        assert_eq!(
            selection.overflow()[0].source,
            LocalLightSourceId::new(earlier_provider, SourceLightKey::new(10, 0))
        );
        assert_eq!(
            selection.overflow()[0].reason,
            LocalLightOverflowReason::UnsupportedKind
        );
        assert_eq!(
            selection.overflow()[1].source,
            LocalLightSourceId::new(later_provider, SourceLightKey::new(2, 0))
        );
        assert_eq!(
            selection.overflow()[1].reason,
            LocalLightOverflowReason::Capacity
        );
    }

    #[test]
    fn rejected_source_churn_does_not_change_the_selected_gpu_radiance_identity() {
        let provider = ProviderId::new(30);
        let mut registry = LocalLightRegistry::default();
        let selected_sources = (0..LOCAL_LIGHT_GPU_CAPACITY).map(|index| {
            SourceLight::new(
                SourceLightKey::new(index as u64, 0),
                point(Vec3::new(index as f32, 1.0, 0.0)),
            )
        });
        registry
            .reconcile(LocalLightProviderSnapshot::new(provider, 1, selected_sources).unwrap())
            .unwrap();
        let before = LocalLightGpuSnapshot::from_authoritative(
            &registry.snapshot(),
            LocalLightBudget::point_lights(LOCAL_LIGHT_GPU_CAPACITY),
            0,
        )
        .with_live_revision(7)
        .payload();

        let with_rejected = (0..=LOCAL_LIGHT_GPU_CAPACITY).map(|index| {
            SourceLight::new(
                SourceLightKey::new(index as u64, 0),
                point(Vec3::new(index as f32, 1.0, 0.0)),
            )
        });
        registry
            .reconcile(LocalLightProviderSnapshot::new(provider, 2, with_rejected).unwrap())
            .unwrap();
        let after = LocalLightGpuSnapshot::from_authoritative(
            &registry.snapshot(),
            LocalLightBudget::point_lights(LOCAL_LIGHT_GPU_CAPACITY),
            0,
        )
        .with_live_revision(7)
        .payload();

        assert!(before.selection_eq(after));
        assert_eq!(
            before.for_radiance_identity(),
            after.for_radiance_identity()
        );
        assert_ne!(before.registry_revision(), after.registry_revision());
    }

    #[test]
    fn gpu_identity_lookup_handles_nonzero_index_reorder_overflow_and_removal() {
        let provider = ProviderId::new(31);
        let target_key = SourceLightKey::new(2, 0);
        let overflow_key = SourceLightKey::new(9, 0);
        let mut registry = LocalLightRegistry::default();
        let reversed = (1..=9).rev().map(|index| {
            SourceLight::new(
                SourceLightKey::new(index, 0),
                point(Vec3::new(index as f32 * 0.01, 1.0, 0.0)),
            )
        });
        registry
            .reconcile(LocalLightProviderSnapshot::new(provider, 1, reversed).unwrap())
            .unwrap();
        let target_id = registry.light_id(provider, target_key).unwrap();
        let overflow_id = registry.light_id(provider, overflow_key).unwrap();
        let selected = LocalLightGpuSnapshot::from_authoritative(
            &registry.snapshot(),
            LocalLightBudget::point_lights(LOCAL_LIGHT_GPU_CAPACITY),
            0,
        )
        .payload();

        assert_eq!(selected.light_index(target_id), Some(1));
        assert_eq!(selected.light_index(overflow_id), None);

        let without_target = (1..=9).filter(|index| *index != 2).map(|index| {
            SourceLight::new(
                SourceLightKey::new(index, 0),
                point(Vec3::new(index as f32 * 0.01, 1.0, 0.0)),
            )
        });
        registry
            .reconcile(LocalLightProviderSnapshot::new(provider, 2, without_target).unwrap())
            .unwrap();
        let after_removal = LocalLightGpuSnapshot::from_authoritative(
            &registry.snapshot(),
            LocalLightBudget::point_lights(LOCAL_LIGHT_GPU_CAPACITY),
            0,
        )
        .payload();
        assert_eq!(after_removal.light_index(target_id), None);
    }
}
