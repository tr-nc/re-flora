use glam::{Vec2, Vec3};
use std::sync::Arc;

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
    pub position: Vec3,
    pub color: Vec3,
    pub intensity: f32,
    pub source_radius: f32,
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
    light: LocalLight,
}

impl LocalLightRecord {
    pub(crate) fn id(self) -> LightId {
        self.id
    }

    pub(crate) fn light(self) -> LocalLight {
        self.light
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct LocalLightSnapshot {
    revision: u64,
    lights: Arc<[LocalLightRecord]>,
}

impl LocalLightSnapshot {
    pub(crate) fn revision(&self) -> u64 {
        self.revision
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
        let mut accepted = Vec::with_capacity(budget.point_light_capacity.min(self.lights.len()));
        let mut overflow = Vec::new();
        for record in self.lights.iter().copied() {
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
                reason,
            });
        }
        LocalLightBudgetResult { accepted, overflow }
    }

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
    pub reason: LocalLightOverflowReason,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LocalLightBudgetResult {
    accepted: Vec<LocalLightRecord>,
    overflow: Vec<LocalLightOverflow>,
}

impl LocalLightBudgetResult {
    pub(crate) fn accepted(&self) -> &[LocalLightRecord] {
        &self.accepted
    }

    pub(crate) fn overflow(&self) -> &[LocalLightOverflow] {
        &self.overflow
    }
}

#[derive(Debug)]
struct LocalLightSlot {
    generation: u32,
    light: Option<LocalLight>,
}

#[derive(Debug, Default)]
pub(crate) struct LocalLightDomain {
    revision: u64,
    slots: Vec<LocalLightSlot>,
    snapshot: LocalLightSnapshot,
}

impl LocalLightDomain {
    pub(crate) fn add(&mut self, light: LocalLight) -> LightId {
        let slot = self
            .slots
            .iter()
            .position(|slot| slot.light.is_none())
            .unwrap_or_else(|| {
                self.slots.push(LocalLightSlot {
                    generation: 1,
                    light: None,
                });
                self.slots.len() - 1
            });
        let entry = &mut self.slots[slot];
        let id = LightId {
            slot: slot as u32,
            generation: entry.generation,
        };
        entry.light = Some(light);
        self.publish_snapshot();
        id
    }

    pub(crate) fn update(
        &mut self,
        id: LightId,
        light: LocalLight,
    ) -> Result<(), LocalLightMutationError> {
        let Some(slot) = self.slots.get_mut(id.slot as usize) else {
            return Err(LocalLightMutationError::StaleId(id));
        };
        if slot.generation != id.generation || slot.light.is_none() {
            return Err(LocalLightMutationError::StaleId(id));
        }
        if slot.light == Some(light) {
            return Ok(());
        }
        slot.light = Some(light);
        self.publish_snapshot();
        Ok(())
    }

    pub(crate) fn remove(&mut self, id: LightId) -> Result<(), LocalLightMutationError> {
        let Some(slot) = self.slots.get_mut(id.slot as usize) else {
            return Err(LocalLightMutationError::StaleId(id));
        };
        if slot.generation != id.generation || slot.light.take().is_none() {
            return Err(LocalLightMutationError::StaleId(id));
        }
        slot.generation = slot.generation.wrapping_add(1).max(1);
        self.publish_snapshot();
        Ok(())
    }

    pub(crate) fn snapshot(&self) -> LocalLightSnapshot {
        self.snapshot.clone()
    }

    fn publish_snapshot(&mut self) {
        self.revision = self.revision.wrapping_add(1).max(1);
        let lights: Vec<_> = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(slot, entry)| {
                entry.light.map(|light| LocalLightRecord {
                    id: LightId {
                        slot: slot as u32,
                        generation: entry.generation,
                    },
                    light,
                })
            })
            .collect();
        self.snapshot = LocalLightSnapshot {
            revision: self.revision,
            lights: lights.into(),
        };
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
        let mut lights = LocalLightDomain::default();
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
        let mut lights = LocalLightDomain::default();
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
        let mut lights = LocalLightDomain::default();
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
        let mut lights = LocalLightDomain::default();
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
}
