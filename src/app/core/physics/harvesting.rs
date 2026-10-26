//! Ripe-fruit picking and retirement. Collection removes the registered owner,
//! so late release tickets and repeated clicks cannot create another apple.
use super::*;
use crate::app::core::garden_economy::HarvestKey;
use crate::tracer::TerrainRayQuery;

#[derive(Clone, Copy, Debug)]
pub(in crate::app::core) struct HarvestPick {
    pub key: HarvestKey,
    pub loose: bool,
}

const HARVEST_REACH: f32 = 2.;
const MAX_PICK_CANDIDATES: usize = 256;
fn ripe(fruit: &RegisteredFruit, cycle: f32) -> bool {
    match fruit.phase {
        FruitSweepPhase::Armed | FruitSweepPhase::PendingDrop => {
            cycle >= fruit.spec.full_growth_phase
        }
        FruitSweepPhase::Dropped => fruit.body.is_some(),
    }
}
fn ray_sphere(origin: Vec3, direction: Vec3, center: Vec3, radius: f32) -> Option<f32> {
    let offset = center - origin;
    let along = offset.dot(direction);
    let perpendicular = (offset - direction * along).length_squared();
    let discriminant = radius * radius - perpendicular;
    if discriminant < 0. {
        return None;
    }
    let reach = discriminant.sqrt();
    (along + reach >= 0.).then_some((along - reach).max(0.))
}

impl TerrainPhysics {
    pub(in crate::app::core) fn pick_ripe_apple(
        &self,
        ray: TerrainRayQuery,
        occluder: Option<f32>,
        tracer: &Tracer,
    ) -> anyhow::Result<Option<HarvestPick>> {
        anyhow::ensure!(
            ray.origin.is_finite() && ray.direction.is_finite(),
            "invalid harvest ray"
        );
        let Some(direction) = ray.direction.try_normalize() else {
            return Ok(None);
        };
        let limit = occluder.unwrap_or(HARVEST_REACH).min(HARVEST_REACH);
        if !limit.is_finite() || limit < 0. {
            return Ok(None);
        }
        let radius = (TREE_FRUIT_MAX_RADIUS_VOXELS as f32 + 1.) * self.apple_size_scale
            / VOXELS_PER_WORLD_UNIT;
        let mut candidates = Vec::new();
        for (&tree, fruits) in &self.fruits_by_tree {
            for (&id, fruit) in fruits {
                if !ripe(fruit, self.fruit_cycle) {
                    continue;
                }
                let (center, root) = if let Some(body) = fruit.body {
                    let Some(state) = self.collision_world.dynamic_body_state(body) else {
                        continue;
                    };
                    (state.position / VOXELS_PER_WORLD_UNIT, None)
                } else {
                    (
                        fruit.spec.position_voxels.as_vec3() / VOXELS_PER_WORLD_UNIT,
                        Some(fruit.spec.position_voxels),
                    )
                };
                // Generous rest-pose corridor accounts for ordinary canopy sway.
                // Pose reads happen only on a click, never for idle hover.
                if let Some(t) = ray_sphere(ray.origin, direction, center, radius + 0.12)
                    .filter(|t| *t <= HARVEST_REACH + 0.12)
                {
                    candidates.push((t, HarvestKey { tree, fruit: id }, center, root));
                }
            }
        }
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        candidates.truncate(MAX_PICK_CANDIDATES);
        let roots = candidates.iter().filter_map(|c| c.3).collect::<Vec<_>>();
        let poses = if roots.is_empty() {
            Vec::new()
        } else {
            tracer.attached_fruit_handoff(&roots)?
        };
        anyhow::ensure!(poses.len() == roots.len(), "incomplete apple pose readback");
        let mut poses = poses.into_iter();
        let mut hit: Option<(f32, HarvestPick)> = None;
        for (_, key, mut center, root) in candidates {
            if let Some(root) = root {
                center = (root.as_vec3() + poses.next().unwrap().0) / VOXELS_PER_WORLD_UNIT;
            }
            if let Some(distance) = ray_sphere(ray.origin, direction, center, radius)
                .filter(|d| *d <= limit + 0.5 / VOXELS_PER_WORLD_UNIT)
            {
                if hit.is_none_or(|previous| distance < previous.0) {
                    hit = Some((
                        distance,
                        HarvestPick {
                            key,
                            loose: root.is_none(),
                        },
                    ));
                }
            }
        }
        Ok(hit.map(|h| h.1))
    }
    /// One bounded pose readback per suction pulse, shared by attached and loose fruit.
    pub(in crate::app::core) fn vacuum_apples(
        &self,
        beam: super::super::vacuum::Beam,
        tracer: &Tracer,
    ) -> anyhow::Result<Vec<(HarvestKey, Vec3)>> {
        let mut candidates = Vec::new();
        for (&tree, fruits) in &self.fruits_by_tree {
            for (&id, fruit) in fruits {
                if !ripe(fruit, self.fruit_cycle) {
                    continue;
                }
                let (center, root) = if let Some(body) = fruit.body {
                    let Some(state) = self.collision_world.dynamic_body_state(body) else {
                        continue;
                    };
                    (state.position / VOXELS_PER_WORLD_UNIT, None)
                } else {
                    (
                        fruit.spec.position_voxels.as_vec3() / VOXELS_PER_WORLD_UNIT,
                        Some(fruit.spec.position_voxels),
                    )
                };
                if beam.contains(center, 0.12) {
                    candidates.push((HarvestKey { tree, fruit: id }, center, root));
                }
            }
        }
        candidates.sort_by(|a, b| {
            a.1.distance_squared(beam.origin)
                .total_cmp(&b.1.distance_squared(beam.origin))
                .then(a.0.cmp(&b.0))
        });
        candidates.truncate(MAX_PICK_CANDIDATES);
        let roots: Vec<_> = candidates.iter().filter_map(|c| c.2).collect();
        let mut poses = if roots.is_empty() {
            Vec::new()
        } else {
            tracer.attached_fruit_handoff(&roots)?
        }
        .into_iter();
        let mut result = Vec::new();
        for (key, center, root) in candidates {
            let center = if let Some(root) = root {
                (root.as_vec3()
                    + poses
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("incomplete fruit poses"))?
                        .0)
                    / VOXELS_PER_WORLD_UNIT
            } else {
                center
            };
            if beam.contains(center, 0.) {
                result.push((key, center));
            }
        }
        Ok(result)
    }

    pub(in crate::app::core) fn take_harvest_apple(&mut self, key: HarvestKey) -> bool {
        let Some(fruits) = self.fruits_by_tree.get_mut(&key.tree) else {
            return false;
        };
        if !fruits
            .get(&key.fruit)
            .is_some_and(|f| ripe(f, self.fruit_cycle))
        {
            return false;
        }
        let fruit = fruits.remove(&key.fruit).unwrap();
        if let Some(body) = fruit.body {
            self.collision_world.remove_dynamic_body(body);
        }
        self.attached_fruit_refresh_trees.insert(key.tree);
        true
    }
    pub(in crate::app::core) fn publish_harvest_rendering(
        &mut self,
        tracer: &mut Tracer,
    ) -> anyhow::Result<()> {
        self.sync_dynamic_fruit_rendering(tracer)
    }
    pub(in crate::app::core) fn review_harvest_target(
        &self,
        tracer: &Tracer,
        fallen: bool,
    ) -> anyhow::Result<Option<(HarvestKey, Vec3)>> {
        for (&tree, fruits) in &self.fruits_by_tree {
            for (&id, fruit) in fruits.iter().rev() {
                if !ripe(fruit, self.fruit_cycle) || fruit.body.is_some() != fallen {
                    continue;
                }
                let center = if let Some(body) = fruit.body {
                    let Some(state) = self.collision_world.dynamic_body_state(body) else {
                        continue;
                    };
                    state.position / VOXELS_PER_WORLD_UNIT
                } else {
                    (fruit.spec.position_voxels.as_vec3()
                        + tracer.attached_fruit_handoff(&[fruit.spec.position_voxels])?[0].0)
                        / VOXELS_PER_WORLD_UNIT
                };
                return Ok(Some((HarvestKey { tree, fruit: id }, center)));
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn picking_rejects_misses_and_behind_camera_and_obeys_nearest_depth() {
        let origin = Vec3::ZERO;
        let ray = Vec3::Z;
        assert_eq!(ray_sphere(origin, ray, Vec3::Z * 3., 1.), Some(2.));
        assert!(ray_sphere(origin, ray, -Vec3::Z * 3., 1.).is_none());
        assert!(ray_sphere(origin, ray, Vec3::X * 3., 1.).is_none());
        assert_eq!(ray_sphere(origin, ray, Vec3::ZERO, 1.), Some(0.));
        assert!(
            ray_sphere(origin, ray, Vec3::Z * 3., 1.).unwrap() > 1.5,
            "a wall at 1.5 occludes this apple"
        );
    }
    #[test]
    fn actual_registry_retirement_is_ripe_only_and_exactly_once() {
        let mut physics = TerrainPhysics::new(0.1);
        let key = HarvestKey { tree: 3, fruit: 7 };
        let spec = TreeFruitSpec {
            id: key.fruit,
            position_voxels: UVec3::splat(128),
            grow_start_phase: 0.5,
            full_growth_phase: 0.8,
            connection_strength: 0.5,
            linear_velocity_voxels: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
        };
        physics.fruits_by_tree.insert(
            key.tree,
            BTreeMap::from([(key.fruit, RegisteredFruit::held(spec, Vec::new()))]),
        );
        assert!(!physics.take_harvest_apple(key));
        physics.fruit_cycle = 1.;
        assert!(physics.take_harvest_apple(key));
        assert!(!physics.take_harvest_apple(key));
        assert!(physics.attached_tree_fruits(key.tree).is_empty());
    }
}
