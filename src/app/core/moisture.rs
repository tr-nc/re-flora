use super::App;
use glam::{Vec2, Vec3};

const SPRINKLER_MOISTURE_RADIUS: f32 = 0.18;
const SPRINKLER_MOISTURE_PER_SECOND: f32 = 0.24;
const TERRAIN_MOISTURE_DRY_RATE_PER_SECOND: f32 = 0.012;
const TERRAIN_MOISTURE_MIN_STRENGTH: f32 = 0.025;
const TERRAIN_MOISTURE_MAX_RADIUS: f32 = 0.22;

#[derive(Clone, Copy, Debug)]
struct TerrainMoisturePatch {
    center: Vec3,
    radius: f32,
    strength: f32,
}

#[derive(Default)]
pub(super) struct TerrainMoistureSystem {
    patches: Vec<TerrainMoisturePatch>,
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

    pub(super) fn add_water(&mut self, center: Vec3, radius: f32, amount: f32) {
        let radius = radius.clamp(0.01, TERRAIN_MOISTURE_MAX_RADIUS);
        let amount = amount.clamp(0.0, 1.0);
        if amount <= 0.0 {
            return;
        }

        let center_xz = Vec2::new(center.x, center.z);
        if let Some(existing) = self.patches.iter_mut().find(|patch| {
            let patch_xz = Vec2::new(patch.center.x, patch.center.z);
            center_xz.distance(patch_xz) <= (patch.radius + radius) * 0.62
        }) {
            let old_weight = existing.strength.max(0.05);
            let new_weight = amount.max(0.05);
            existing.center = (existing.center * old_weight + center * new_weight)
                / (old_weight + new_weight).max(0.001);
            existing.radius = existing.radius.max(radius).min(TERRAIN_MOISTURE_MAX_RADIUS);
            existing.strength = (existing.strength + amount).min(1.0);
            return;
        }

        self.patches.push(TerrainMoisturePatch {
            center,
            radius,
            strength: amount.min(1.0),
        });

        if self.patches.len() > crate::tracer::TERRAIN_MOISTURE_PATCH_CAPACITY {
            if let Some((weakest_index, _)) =
                self.patches.iter().enumerate().min_by(|(_, a), (_, b)| {
                    a.strength
                        .partial_cmp(&b.strength)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
            {
                self.patches.swap_remove(weakest_index);
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

impl App {
    pub(super) fn update_sprinkler_moisture(&mut self, dt: f32) {
        if dt <= 0.0 || self.sprinkler_records.is_empty() {
            return;
        }

        let amount = SPRINKLER_MOISTURE_PER_SECOND * dt;
        for sprinkler in &self.sprinkler_records {
            self.terrain_moisture.add_water(
                sprinkler.base_position,
                SPRINKLER_MOISTURE_RADIUS,
                amount,
            );
        }
    }
}
