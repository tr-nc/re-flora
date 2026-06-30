use super::App;
use glam::{Vec2, Vec3};

const SPRINKLER_MOISTURE_RADIUS: f32 = 0.30;
const SPRINKLER_MOISTURE_PER_SECOND: f32 = 1.35;
const SPRINKLER_MOISTURE_INITIAL_STRENGTH: f32 = 0.38;
const WATERING_BRUSH_MOISTURE_PER_DAB: f32 = 0.68;
const WATERING_BRUSH_MERGE_RADIUS_FACTOR: f32 = 0.42;
const TERRAIN_MOISTURE_DRY_RATE_PER_SECOND: f32 = 0.018;
const TERRAIN_MOISTURE_MIN_STRENGTH: f32 = 0.025;
const TERRAIN_MOISTURE_MAX_RADIUS: f32 = 0.34;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerrainMoistureSource {
    Sprinkler(u32),
    Brush(u32),
}

#[derive(Clone, Copy, Debug)]
struct TerrainMoisturePatch {
    source: TerrainMoistureSource,
    center: Vec3,
    radius: f32,
    strength: f32,
}

pub(super) struct TerrainMoistureSystem {
    patches: Vec<TerrainMoisturePatch>,
    next_brush_patch_id: u32,
}

impl Default for TerrainMoistureSystem {
    fn default() -> Self {
        Self {
            patches: Vec::new(),
            next_brush_patch_id: 1,
        }
    }
}

impl TerrainMoistureSystem {
    pub(super) fn update(&mut self, dt: f32) {
        if dt <= 0.0 || self.patches.is_empty() {
            return;
        }

        let dry_amount = TERRAIN_MOISTURE_DRY_RATE_PER_SECOND * dt;
        for patch in &mut self.patches {
            patch.strength = (patch.strength - dry_amount).max(0.0);
        }
        self.patches
            .retain(|patch| patch.strength >= TERRAIN_MOISTURE_MIN_STRENGTH);
    }

    pub(super) fn add_sprinkler_water(&mut self, sprinkler_id: u32, center: Vec3, amount: f32) {
        self.upsert_source_patch(
            TerrainMoistureSource::Sprinkler(sprinkler_id),
            center,
            SPRINKLER_MOISTURE_RADIUS,
            amount,
            SPRINKLER_MOISTURE_INITIAL_STRENGTH,
        );
    }

    pub(super) fn add_watering_brush(&mut self, center: Vec3, radius: f32) {
        let radius = radius.clamp(0.01, TERRAIN_MOISTURE_MAX_RADIUS);
        let center_xz = Vec2::new(center.x, center.z);
        let merge_distance = radius * WATERING_BRUSH_MERGE_RADIUS_FACTOR;

        if let Some(existing) = self.patches.iter_mut().find(|patch| {
            matches!(patch.source, TerrainMoistureSource::Brush(_))
                && center_xz.distance(Vec2::new(patch.center.x, patch.center.z)) <= merge_distance
        }) {
            existing.radius = existing.radius.max(radius).min(TERRAIN_MOISTURE_MAX_RADIUS);
            existing.strength = (existing.strength + WATERING_BRUSH_MOISTURE_PER_DAB).min(1.0);
            return;
        }

        let brush_id = self.next_brush_patch_id;
        self.next_brush_patch_id = self.next_brush_patch_id.wrapping_add(1).max(1);
        self.patches.push(TerrainMoisturePatch {
            source: TerrainMoistureSource::Brush(brush_id),
            center,
            radius,
            strength: WATERING_BRUSH_MOISTURE_PER_DAB,
        });
        self.enforce_capacity();
    }

    fn upsert_source_patch(
        &mut self,
        source: TerrainMoistureSource,
        center: Vec3,
        radius: f32,
        amount: f32,
        initial_strength: f32,
    ) {
        let radius = radius.clamp(0.01, TERRAIN_MOISTURE_MAX_RADIUS);
        let amount = amount.clamp(0.0, 1.0);
        if amount <= 0.0 {
            return;
        }

        if let Some(existing) = self.patches.iter_mut().find(|patch| patch.source == source) {
            // Keep sourced patches pinned to their source. This avoids the old distance-merge
            // behavior where adding another sprinkler could pull an existing wet spot away from
            // the sprinkler that created it.
            existing.center = center;
            existing.radius = radius;
            existing.strength = (existing.strength + amount).min(1.0);
            return;
        }

        self.patches.push(TerrainMoisturePatch {
            source,
            center,
            radius,
            strength: amount.max(initial_strength).min(1.0),
        });
        self.enforce_capacity();
    }

    fn enforce_capacity(&mut self) {
        while self.patches.len() > crate::tracer::TERRAIN_MOISTURE_PATCH_CAPACITY {
            if let Some((weakest_brush_index, _)) = self
                .patches
                .iter()
                .enumerate()
                .filter(|(_, patch)| matches!(patch.source, TerrainMoistureSource::Brush(_)))
                .min_by(|(_, a), (_, b)| strength_order(a, b))
            {
                self.patches.swap_remove(weakest_brush_index);
                continue;
            }

            if let Some((weakest_index, _)) = self
                .patches
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| strength_order(a, b))
            {
                self.patches.swap_remove(weakest_index);
            } else {
                break;
            }
        }
    }

    pub(super) fn patch_count(&self) -> usize {
        self.patches.len()
    }

    pub(super) fn shader_patches(
        &self,
    ) -> (
        u32,
        [[f32; 4]; crate::tracer::TERRAIN_MOISTURE_PATCH_CAPACITY],
    ) {
        let mut patches = [[0.0; 4]; crate::tracer::TERRAIN_MOISTURE_PATCH_CAPACITY];
        let patch_count = self
            .patches
            .len()
            .min(crate::tracer::TERRAIN_MOISTURE_PATCH_CAPACITY);
        for (out, patch) in patches
            .iter_mut()
            .zip(self.patches.iter())
            .take(patch_count)
        {
            *out = [patch.center.x, patch.center.z, patch.radius, patch.strength];
        }
        (patch_count as u32, patches)
    }
}

fn strength_order(a: &TerrainMoisturePatch, b: &TerrainMoisturePatch) -> std::cmp::Ordering {
    a.strength
        .partial_cmp(&b.strength)
        .unwrap_or(std::cmp::Ordering::Equal)
}

impl App {
    pub(super) fn update_sprinkler_moisture(&mut self, dt: f32) {
        if dt <= 0.0 || self.sprinkler_records.is_empty() {
            return;
        }

        let amount = SPRINKLER_MOISTURE_PER_SECOND * dt;
        let sprinkler_sources = self
            .sprinkler_records
            .iter()
            .map(|sprinkler| (sprinkler.id, sprinkler.base_position))
            .collect::<Vec<_>>();
        for (sprinkler_id, base_position) in sprinkler_sources {
            self.terrain_moisture
                .add_sprinkler_water(sprinkler_id, base_position, amount);
            if let Err(err) = self.plain_builder.apply_terrain_moisture_brush(
                base_position,
                SPRINKLER_MOISTURE_RADIUS,
                amount,
            ) {
                log::error!(
                    "Failed to write sprinkler moisture into terrain atlas: {}",
                    err
                );
            }
        }
    }

    pub(super) fn add_watering_brush_moisture(&mut self, center: Vec3, radius: f32) {
        self.terrain_moisture.add_watering_brush(center, radius);
        if let Err(err) = self.plain_builder.apply_terrain_moisture_brush(
            center,
            radius,
            WATERING_BRUSH_MOISTURE_PER_DAB,
        ) {
            log::error!(
                "Failed to write watering brush moisture into terrain atlas: {}",
                err
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_near(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 0.0001,
            "expected {expected}, got {actual}"
        );
    }

    fn sprinkler_center(system: &TerrainMoistureSystem, sprinkler_id: u32) -> Vec3 {
        system
            .patches
            .iter()
            .find(|patch| patch.source == TerrainMoistureSource::Sprinkler(sprinkler_id))
            .map(|patch| patch.center)
            .expect("missing sprinkler moisture patch")
    }

    #[test]
    fn adding_nearby_sprinkler_does_not_move_existing_sprinkler_patch() {
        let mut system = TerrainMoistureSystem::default();
        let first = Vec3::new(0.40, 0.20, 0.40);
        let second = Vec3::new(0.48, 0.20, 0.44);

        system.add_sprinkler_water(1, first, 0.5);
        system.add_sprinkler_water(2, second, 0.5);
        system.add_sprinkler_water(1, first, 0.5);

        let first_center = sprinkler_center(&system, 1);
        assert_near(first_center.x, first.x);
        assert_near(first_center.y, first.y);
        assert_near(first_center.z, first.z);
        assert_eq!(system.patch_count(), 2);
    }

    #[test]
    fn watering_brush_does_not_merge_into_sprinkler_patch() {
        let mut system = TerrainMoistureSystem::default();
        let sprinkler = Vec3::new(0.40, 0.20, 0.40);
        let brush = Vec3::new(0.41, 0.20, 0.41);

        system.add_sprinkler_water(7, sprinkler, 0.5);
        system.add_watering_brush(brush, 0.08);

        let sprinkler_center = sprinkler_center(&system, 7);
        assert_near(sprinkler_center.x, sprinkler.x);
        assert_near(sprinkler_center.z, sprinkler.z);
        assert_eq!(system.patch_count(), 2);
    }
}
