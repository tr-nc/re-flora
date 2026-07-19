use crate::builder::{ContreeBuilder, ContreeCpuVoxelBlock, ContreeCpuVoxelBlockExport};
use crate::tracer::{
    collision_probe_apple_offsets, voxel_apple_offsets, DynamicFruitRenderInstance, Tracer,
    TREE_FRUIT_MAX_RADIUS_VOXELS,
};
use anyhow::Context;
use glam::{IVec3, UVec3, Vec3};
use re_flora_physics::{
    BrickOccupancy, CollisionWorld, DynamicBodyDesc, DynamicBodyId, DynamicColliderShape,
    StaticVoxelBrickId, StaticVoxelBrickUpdate, STATIC_VOXEL_BRICK_DIM,
};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::time::Instant;

const STARTUP_TERRAIN_BRICK_ID: StaticVoxelBrickId = StaticVoxelBrickId(IVec3::new(8, 3, 8));
#[cfg(test)]
const STARTUP_TERRAIN_BRICK_MIN: UVec3 = UVec3::new(256, 96, 256);
const VOXELS_PER_WORLD_UNIT: f32 = 256.0;
// Keep the probe inside the only terrain-collision brick currently imported, but place it on the
// camera-facing side of the startup tree so the trunk does not hide the probe fruit.
const COLLISION_PROBE_SPAWN_VOXELS: Vec3 = Vec3::new(276.0, 152.0, 280.0);
const COLLISION_PROBE_GRAVITY_VOXELS: Vec3 = Vec3::new(0.0, -9.8 * VOXELS_PER_WORLD_UNIT, 0.0);
// Release measurements put one real 32-cubed Contree export plus Rapier update at about 1.2 ms.
// Keeping this at one avoids terrain edits turning a single render frame into an unbounded scan.
const MAX_TERRAIN_COLLIDER_BRICKS_PER_FRAME: usize = 1;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct TreeFruitSpec {
    pub id: u64,
    pub position_voxels: UVec3,
    pub grow_start_age: f32,
    pub full_growth_age: f32,
    pub drop_age: f32,
    pub linear_velocity_voxels: Vec3,
    pub angular_velocity: Vec3,
}

impl TreeFruitSpec {
    pub(super) fn attached_radius_voxels(&self, age: f32) -> Option<u32> {
        if age < self.grow_start_age || age >= self.drop_age {
            return None;
        }
        let duration = (self.full_growth_age - self.grow_start_age).max(f32::EPSILON);
        let progress = ((age - self.grow_start_age) / duration).clamp(0.0, 1.0);
        let radius = 1 + (progress * TREE_FRUIT_MAX_RADIUS_VOXELS as f32).floor() as u32;
        Some(radius.min(TREE_FRUIT_MAX_RADIUS_VOXELS))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct AttachedTreeFruit {
    pub position_voxels: UVec3,
    pub radius_voxels: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FruitSweepPhase {
    Armed,
    PendingDrop,
    Dropped,
}

fn fruit_phase_after_age_change(
    phase: FruitSweepPhase,
    previous_age: f32,
    tree_age: f32,
    drop_age: f32,
) -> FruitSweepPhase {
    if tree_age < previous_age {
        if tree_age < drop_age {
            FruitSweepPhase::Armed
        } else {
            FruitSweepPhase::Dropped
        }
    } else if tree_age > previous_age
        && phase == FruitSweepPhase::Armed
        && previous_age < drop_age
        && tree_age >= drop_age
    {
        FruitSweepPhase::PendingDrop
    } else {
        phase
    }
}

#[derive(Debug)]
struct RegisteredFruit {
    spec: TreeFruitSpec,
    required_bricks: Vec<StaticVoxelBrickId>,
    phase: FruitSweepPhase,
    body: Option<DynamicBodyId>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum StartupTerrainBrickState {
    #[default]
    Pending,
    Imported,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DirtyTerrainBrickWork {
    id: StaticVoxelBrickId,
    request_revision: u64,
}

#[derive(Debug, Default)]
struct DirtyTerrainBrickQueue {
    pending: VecDeque<StaticVoxelBrickId>,
    latest_revisions: HashMap<StaticVoxelBrickId, u64>,
    next_revision: u64,
}

impl DirtyTerrainBrickQueue {
    fn push(&mut self, id: StaticVoxelBrickId) -> u64 {
        self.next_revision = self
            .next_revision
            .checked_add(1)
            .expect("terrain collider request revision space exhausted");
        if self
            .latest_revisions
            .insert(id, self.next_revision)
            .is_none()
        {
            self.pending.push_back(id);
        }
        self.next_revision
    }

    fn front(&self) -> Option<DirtyTerrainBrickWork> {
        let id = *self.pending.front()?;
        Some(DirtyTerrainBrickWork {
            id,
            request_revision: *self
                .latest_revisions
                .get(&id)
                .expect("pending terrain collider brick must have a revision"),
        })
    }

    fn complete(&mut self, work: DirtyTerrainBrickWork) {
        if self.latest_revisions.get(&work.id).copied() != Some(work.request_revision) {
            self.defer(work.id);
            return;
        }
        self.pop_expected_front(work.id);
        self.latest_revisions.remove(&work.id);
    }

    fn defer(&mut self, id: StaticVoxelBrickId) {
        self.pop_expected_front(id);
        self.pending.push_back(id);
    }

    fn pop_expected_front(&mut self, id: StaticVoxelBrickId) {
        let popped = self.pending.pop_front();
        debug_assert_eq!(popped, Some(id));
    }

    fn len(&self) -> usize {
        self.pending.len()
    }
}

pub(super) struct TerrainPhysics {
    collision_world: CollisionWorld,
    startup_brick_state: StartupTerrainBrickState,
    dirty_terrain_bricks: DirtyTerrainBrickQueue,
    collision_probe_body: Option<DynamicBodyId>,
    collision_probe_mesh_uploaded: bool,
    imported_terrain_bricks: HashSet<StaticVoxelBrickId>,
    failed_terrain_bricks: HashSet<StaticVoxelBrickId>,
    fruits_by_tree: BTreeMap<u32, BTreeMap<u64, RegisteredFruit>>,
    attached_fruit_refresh_trees: HashSet<u32>,
    tree_age: f32,
}

impl TerrainPhysics {
    pub(super) fn new() -> Self {
        let mut collision_world = CollisionWorld::new();
        collision_world
            .set_gravity(COLLISION_PROBE_GRAVITY_VOXELS)
            .expect("collision probe gravity constant must be valid");
        let mut terrain_physics = Self {
            collision_world,
            startup_brick_state: StartupTerrainBrickState::Pending,
            dirty_terrain_bricks: DirtyTerrainBrickQueue::default(),
            collision_probe_body: None,
            collision_probe_mesh_uploaded: false,
            imported_terrain_bricks: HashSet::new(),
            failed_terrain_bricks: HashSet::new(),
            fruits_by_tree: BTreeMap::new(),
            attached_fruit_refresh_trees: HashSet::new(),
            tree_age: 1.0,
        };
        terrain_physics.mark_terrain_brick_dirty(STARTUP_TERRAIN_BRICK_ID);
        terrain_physics
    }

    pub(super) fn collision_probe_ready(&self) -> bool {
        self.startup_brick_state == StartupTerrainBrickState::Imported
    }

    pub(super) fn collision_probe_active(&self) -> bool {
        self.collision_probe_body.is_some()
    }

    pub(super) fn collision_probe_status(&self) -> String {
        match self.startup_brick_state {
            StartupTerrainBrickState::Pending => "Terrain collider loading...".to_owned(),
            StartupTerrainBrickState::Failed => "Terrain collider failed to load".to_owned(),
            StartupTerrainBrickState::Imported => {
                let Some(id) = self.collision_probe_body else {
                    return "Ready".to_owned();
                };
                let Some(state) = self.collision_world.dynamic_body_state(id) else {
                    return "Probe body unavailable".to_owned();
                };
                let position = state.position / VOXELS_PER_WORLD_UNIT;
                let speed = state.linear_velocity.length() / VOXELS_PER_WORLD_UNIT;
                let motion = if state.sleeping { "resting" } else { "moving" };
                format!(
                    "{motion} at ({:.3}, {:.3}, {:.3})\nspeed {:.2} world units/s",
                    position.x, position.y, position.z, speed
                )
            }
        }
    }

    pub(super) fn drop_collision_probe(&mut self, tracer: &mut Tracer) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.collision_probe_ready(),
            "terrain collision brick is not ready"
        );

        self.clear_collision_probe(tracer);
        if !self.collision_probe_mesh_uploaded {
            tracer
                .upload_collision_probe_geometry()
                .context("uploading collision probe geometry")?;
            self.collision_probe_mesh_uploaded = true;
        }

        let collider_points = collision_probe_convex_points();
        let collider_point_count = collider_points.len();
        let mut desc = DynamicBodyDesc::sphere(COLLISION_PROBE_SPAWN_VOXELS, 4.0);
        desc.collider = DynamicColliderShape::ConvexHull {
            points: collider_points,
        };
        desc.linear_velocity = Vec3::new(7.0, 0.0, 2.0);
        desc.angular_velocity = Vec3::new(3.2, 1.7, -4.0);
        desc.friction = 0.85;
        desc.restitution = 0.08;
        desc.linear_damping = 0.08;
        desc.angular_damping = 0.12;
        desc.ccd_enabled = true;

        let body = self
            .collision_world
            .spawn_dynamic_body(desc)
            .context("spawning collision probe body")?;
        self.collision_world
            .dynamic_body_state(body)
            .context("reading newly spawned collision probe body")?;
        self.collision_probe_body = Some(body);
        if let Err(err) = self.sync_dynamic_fruit_rendering(tracer) {
            self.collision_world.remove_dynamic_body(body);
            self.collision_probe_body = None;
            return Err(err).context("showing collision probe geometry");
        }
        log::info!(
            "[COLLISION][PROBE] dropped body={} spawn_voxels={:?} convex_points={}",
            body.get(),
            COLLISION_PROBE_SPAWN_VOXELS,
            collider_point_count,
        );
        Ok(())
    }

    pub(super) fn clear_collision_probe(&mut self, tracer: &mut Tracer) {
        if let Some(body) = self.collision_probe_body.take() {
            self.collision_world.remove_dynamic_body(body);
        }
        if let Err(err) = self.sync_dynamic_fruit_rendering(tracer) {
            log::error!("Failed to refresh dynamic fruit rendering after clearing probe: {err:#}");
        }
    }

    pub(super) fn advance_dynamic_bodies(
        &mut self,
        frame_delta_time: f32,
        tracer: &mut Tracer,
    ) -> anyhow::Result<()> {
        self.spawn_pending_fruits()?;
        let has_fruit_bodies = self
            .fruits_by_tree
            .values()
            .flat_map(BTreeMap::values)
            .any(|fruit| fruit.body.is_some());
        if self.collision_probe_body.is_none() && !has_fruit_bodies {
            return self.sync_dynamic_fruit_rendering(tracer);
        }
        let step = self.collision_world.advance(frame_delta_time);
        if step.dropped_seconds > 0.0 {
            log::warn!(
                "[COLLISION][PROBE] physics hitch dropped {:.3} ms",
                step.dropped_seconds * 1_000.0
            );
        }
        if let Some(body) = self.collision_probe_body {
            if self.collision_world.dynamic_body_state(body).is_none() {
                self.collision_probe_body = None;
                log::error!("collision probe body disappeared from physics world");
            }
        }
        for fruits in self.fruits_by_tree.values_mut() {
            for fruit in fruits.values_mut() {
                if fruit
                    .body
                    .is_some_and(|body| self.collision_world.dynamic_body_state(body).is_none())
                {
                    log::error!(
                        "dynamic fruit {} disappeared from physics world",
                        fruit.spec.id
                    );
                    fruit.body = None;
                    fruit.phase = FruitSweepPhase::Dropped;
                }
            }
        }
        self.sync_dynamic_fruit_rendering(tracer)
            .context("updating dynamic fruit geometry")
    }

    pub(super) fn register_tree_fruits(
        &mut self,
        tree_id: u32,
        specs: Vec<TreeFruitSpec>,
        tree_age: f32,
        tracer: &mut Tracer,
    ) -> anyhow::Result<()> {
        let registry_was_empty = self.fruits_by_tree.is_empty();
        let mut previous = self.fruits_by_tree.remove(&tree_id).unwrap_or_default();
        let mut registered = BTreeMap::new();
        for spec in specs {
            let required_bricks = fruit_drop_path_bricks(spec.position_voxels);
            for &brick in &required_bricks {
                if !self.imported_terrain_bricks.contains(&brick)
                    && !self.failed_terrain_bricks.contains(&brick)
                {
                    self.mark_terrain_brick_dirty(brick);
                }
            }
            let fruit = if let Some(mut existing) = previous.remove(&spec.id) {
                existing.spec = spec;
                existing.required_bricks = required_bricks;
                existing
            } else {
                RegisteredFruit {
                    phase: if tree_age >= spec.drop_age {
                        FruitSweepPhase::PendingDrop
                    } else {
                        FruitSweepPhase::Armed
                    },
                    spec,
                    required_bricks,
                    body: None,
                }
            };
            registered.insert(fruit.spec.id, fruit);
        }
        for stale in previous.into_values() {
            if let Some(body) = stale.body {
                self.collision_world.remove_dynamic_body(body);
            }
        }
        self.fruits_by_tree.insert(tree_id, registered);
        if registry_was_empty {
            self.tree_age = tree_age.clamp(0.0, 1.0);
        }
        self.spawn_pending_fruits()?;
        self.sync_dynamic_fruit_rendering(tracer)
    }

    pub(super) fn unregister_tree_fruits(
        &mut self,
        tree_id: u32,
        tracer: &mut Tracer,
    ) -> anyhow::Result<()> {
        if let Some(fruits) = self.fruits_by_tree.remove(&tree_id) {
            for fruit in fruits.into_values() {
                if let Some(body) = fruit.body {
                    self.collision_world.remove_dynamic_body(body);
                }
            }
        }
        self.sync_dynamic_fruit_rendering(tracer)
    }

    pub(super) fn set_tree_age(
        &mut self,
        tree_age: f32,
        tracer: &mut Tracer,
    ) -> anyhow::Result<()> {
        let tree_age = tree_age.clamp(0.0, 1.0);
        let previous_age = self.tree_age;
        if tree_age < previous_age {
            for fruits in self.fruits_by_tree.values_mut() {
                for fruit in fruits.values_mut() {
                    if let Some(body) = fruit.body.take() {
                        self.collision_world.remove_dynamic_body(body);
                    }
                    fruit.phase = fruit_phase_after_age_change(
                        fruit.phase,
                        previous_age,
                        tree_age,
                        fruit.spec.drop_age,
                    );
                }
            }
        } else if tree_age > previous_age {
            for fruits in self.fruits_by_tree.values_mut() {
                for fruit in fruits.values_mut() {
                    fruit.phase = fruit_phase_after_age_change(
                        fruit.phase,
                        previous_age,
                        tree_age,
                        fruit.spec.drop_age,
                    );
                }
            }
        }
        self.tree_age = tree_age;
        if tree_age != previous_age {
            self.attached_fruit_refresh_trees
                .extend(self.fruits_by_tree.keys().copied());
        }
        self.spawn_pending_fruits()?;
        self.sync_dynamic_fruit_rendering(tracer)
    }

    pub(super) fn attached_tree_fruits(
        &self,
        tree_id: u32,
        tree_age: f32,
    ) -> Vec<AttachedTreeFruit> {
        let tree_age = tree_age.clamp(0.0, 1.0);
        self.fruits_by_tree
            .get(&tree_id)
            .into_iter()
            .flat_map(BTreeMap::values)
            .filter_map(|fruit| {
                let radius_voxels = match fruit.phase {
                    FruitSweepPhase::Armed => fruit.spec.attached_radius_voxels(tree_age)?,
                    // Crossing the threshold only requests a drop. Keep the fruit attached until
                    // every collider brick along its fall path is imported and detachment can be
                    // represented continuously by a dynamic body.
                    FruitSweepPhase::PendingDrop => TREE_FRUIT_MAX_RADIUS_VOXELS,
                    FruitSweepPhase::Dropped => return None,
                };
                Some(AttachedTreeFruit {
                    position_voxels: fruit.spec.position_voxels,
                    radius_voxels,
                })
            })
            .collect()
    }

    pub(super) fn take_attached_fruit_refresh_trees(&mut self) -> Vec<u32> {
        let mut tree_ids = self
            .attached_fruit_refresh_trees
            .drain()
            .collect::<Vec<_>>();
        tree_ids.sort_unstable();
        tree_ids
    }

    fn spawn_pending_fruits(&mut self) -> anyhow::Result<()> {
        let imported_terrain_bricks = &self.imported_terrain_bricks;
        let ready = self
            .fruits_by_tree
            .iter()
            .flat_map(|(&tree_id, fruits)| {
                fruits.iter().filter_map(move |(&fruit_id, fruit)| {
                    (fruit.phase == FruitSweepPhase::PendingDrop
                        && fruit.body.is_none()
                        && fruit
                            .required_bricks
                            .iter()
                            .all(|brick| imported_terrain_bricks.contains(brick)))
                    .then_some((tree_id, fruit_id, fruit.spec.clone()))
                })
            })
            .collect::<Vec<_>>();

        for (tree_id, fruit_id, spec) in ready {
            let mut desc = DynamicBodyDesc::sphere(spec.position_voxels.as_vec3(), 2.0);
            desc.collider = DynamicColliderShape::ConvexHull {
                points: apple_convex_points(),
            };
            desc.linear_velocity = spec.linear_velocity_voxels;
            desc.angular_velocity = spec.angular_velocity;
            desc.friction = 0.82;
            desc.restitution = 0.12;
            desc.linear_damping = 0.06;
            desc.angular_damping = 0.10;
            desc.ccd_enabled = true;
            let body = self
                .collision_world
                .spawn_dynamic_body(desc)
                .with_context(|| format!("spawning fruit {fruit_id} for tree {tree_id}"))?;
            let fruit = self
                .fruits_by_tree
                .get_mut(&tree_id)
                .and_then(|fruits| fruits.get_mut(&fruit_id))
                .context("pending fruit disappeared while spawning")?;
            fruit.body = Some(body);
            fruit.phase = FruitSweepPhase::Dropped;
            self.attached_fruit_refresh_trees.insert(tree_id);
            log::info!(
                "[COLLISION][FRUIT] dropped tree={} fruit={} body={} age={:.3} position_voxels={:?}",
                tree_id,
                fruit_id,
                body.get(),
                self.tree_age,
                spec.position_voxels,
            );
        }
        Ok(())
    }

    fn sync_dynamic_fruit_rendering(&self, tracer: &mut Tracer) -> anyhow::Result<()> {
        let mut instances = Vec::new();
        if let Some(body) = self.collision_probe_body {
            if let Some(state) = self.collision_world.dynamic_body_state(body) {
                instances.push(DynamicFruitRenderInstance::new(
                    state.position / VOXELS_PER_WORLD_UNIT,
                    state.rotation,
                    2.0,
                ));
            }
        }
        for fruit in self.fruits_by_tree.values().flat_map(BTreeMap::values) {
            if let Some(state) = fruit
                .body
                .and_then(|body| self.collision_world.dynamic_body_state(body))
            {
                instances.push(DynamicFruitRenderInstance::new(
                    state.position / VOXELS_PER_WORLD_UNIT,
                    state.rotation,
                    1.0,
                ));
            }
        }
        if instances.is_empty() {
            tracer.clear_collision_probe_geometry();
            Ok(())
        } else {
            tracer.show_dynamic_fruit_geometry(&instances)
        }
    }

    pub(super) fn mark_terrain_voxel_bound_dirty(&mut self, bound: crate::geom::UAabb3) {
        for id in terrain_brick_ids_for_inclusive_uvec_aabb(bound.min(), bound.max()) {
            self.mark_terrain_brick_dirty(id);
        }
    }

    pub(super) fn mark_terrain_chunks_dirty(&mut self, chunk_ids: &[UVec3], chunk_dim: UVec3) {
        for &chunk_id in chunk_ids {
            let min = (chunk_id * chunk_dim).as_ivec3();
            let max = min + chunk_dim.as_ivec3();
            for id in terrain_brick_ids_for_voxel_aabb(min, max) {
                self.mark_terrain_brick_dirty(id);
            }
        }
    }

    fn mark_terrain_brick_dirty(&mut self, id: StaticVoxelBrickId) {
        let request_revision = self.dirty_terrain_bricks.push(id);
        log::debug!(
            "[COLLISION][TERRAIN_BRICK] queued id={id:?} request_revision={request_revision} pending={}",
            self.dirty_terrain_bricks.len(),
        );
    }

    pub(super) fn process_terrain_collider_updates(&mut self, contree_builder: &ContreeBuilder) {
        for _ in 0..MAX_TERRAIN_COLLIDER_BRICKS_PER_FRAME {
            let Some(work) = self.dirty_terrain_bricks.front() else {
                break;
            };
            if !self.try_refresh_terrain_brick(contree_builder, work) {
                self.dirty_terrain_bricks.defer(work.id);
            }
        }
    }

    /// Returns true when the work item reached a terminal result and was removed from the queue.
    /// A CPU Contree cache miss is explicitly non-terminal and remains queued for a later frame.
    fn try_refresh_terrain_brick(
        &mut self,
        contree_builder: &ContreeBuilder,
        work: DirtyTerrainBrickWork,
    ) -> bool {
        let total_start = Instant::now();
        let brick_min = work.id.0 * STATIC_VOXEL_BRICK_DIM as i32;
        let brick_min_u = match UVec3::try_from(brick_min) {
            Ok(min) => min,
            Err(_) => {
                self.finish_failed_terrain_brick(
                    work,
                    format!("brick has negative voxel minimum {brick_min:?}"),
                );
                return true;
            }
        };

        let export_start = Instant::now();
        let snapshot = contree_builder.cpu_voxel_source_snapshot();
        let export = snapshot.export_voxel_block(brick_min_u, UVec3::splat(STATIC_VOXEL_BRICK_DIM));
        let export_elapsed = export_start.elapsed();
        let block = match export {
            Ok(ContreeCpuVoxelBlockExport::Ready(block)) => block,
            Ok(ContreeCpuVoxelBlockExport::NotReady(not_ready)) => {
                log::debug!(
                    "[COLLISION][TERRAIN_BRICK] deferred id={:?} request_revision={} pending_chunks={:?} export_ms={:.3} pending={}",
                    work.id,
                    work.request_revision,
                    not_ready.pending_chunks,
                    export_elapsed.as_secs_f64() * 1_000.0,
                    self.dirty_terrain_bricks.len(),
                );
                return false;
            }
            Err(err) => {
                self.finish_failed_terrain_brick(work, format!("export failed: {err}"));
                return true;
            }
        };

        let source_revision = match single_source_revision(&block) {
            Ok(revision) => revision,
            Err(err) => {
                self.finish_failed_terrain_brick(
                    work,
                    format!(
                        "source dependency validation failed: {err}; dependencies={:?}",
                        block.source_dependencies
                    ),
                );
                return true;
            }
        };

        let occupancy_start = Instant::now();
        let occupancy = match BrickOccupancy::from_x_fastest_voxel_types(&block.voxel_types) {
            Ok(occupancy) => occupancy,
            Err(err) => {
                self.finish_failed_terrain_brick(work, format!("occupancy build failed: {err}"));
                return true;
            }
        };
        let occupancy_elapsed = occupancy_start.elapsed();
        let solid_count = occupancy.filled_count();

        let physics_start = Instant::now();
        let update =
            self.collision_world
                .upsert_static_voxel_brick(work.id, source_revision, occupancy);
        let physics_elapsed = physics_start.elapsed();
        let changed_voxels = match update {
            StaticVoxelBrickUpdate::Applied { changed_voxels, .. } => changed_voxels,
            StaticVoxelBrickUpdate::Unchanged | StaticVoxelBrickUpdate::Stale { .. } => 0,
        };

        self.dirty_terrain_bricks.complete(work);
        if work.id == STARTUP_TERRAIN_BRICK_ID
            && !matches!(update, StaticVoxelBrickUpdate::Stale { .. })
        {
            self.startup_brick_state = StartupTerrainBrickState::Imported;
        }
        if !matches!(update, StaticVoxelBrickUpdate::Stale { .. }) {
            self.imported_terrain_bricks.insert(work.id);
            self.failed_terrain_bricks.remove(&work.id);
        }
        log::info!(
            "[COLLISION][TERRAIN_BRICK] refreshed id={:?} min={:?} request_revision={} source_revision={} dependencies={:?} solids={} changed_voxels={} update={:?} export_ms={:.3} occupancy_ms={:.3} physics_ms={:.3} total_ms={:.3} pending={}",
            work.id,
            brick_min_u,
            work.request_revision,
            source_revision,
            block.source_dependencies,
            solid_count,
            changed_voxels,
            update,
            export_elapsed.as_secs_f64() * 1_000.0,
            occupancy_elapsed.as_secs_f64() * 1_000.0,
            physics_elapsed.as_secs_f64() * 1_000.0,
            total_start.elapsed().as_secs_f64() * 1_000.0,
            self.dirty_terrain_bricks.len(),
        );
        true
    }

    fn finish_failed_terrain_brick(&mut self, work: DirtyTerrainBrickWork, error: String) {
        self.dirty_terrain_bricks.complete(work);
        self.failed_terrain_bricks.insert(work.id);
        if work.id == STARTUP_TERRAIN_BRICK_ID {
            self.startup_brick_state = StartupTerrainBrickState::Failed;
        }
        log::error!(
            "[COLLISION][TERRAIN_BRICK] refresh failed id={:?} request_revision={} error={} pending={}",
            work.id,
            work.request_revision,
            error,
            self.dirty_terrain_bricks.len(),
        );
    }
}

fn terrain_brick_ids_for_voxel_aabb(min: IVec3, max_exclusive: IVec3) -> Vec<StaticVoxelBrickId> {
    if min.cmpge(max_exclusive).any() {
        return Vec::new();
    }
    let dim = STATIC_VOXEL_BRICK_DIM as i32;
    let first = min.div_euclid(IVec3::splat(dim));
    let last = (max_exclusive - IVec3::ONE).div_euclid(IVec3::splat(dim));
    let mut ids = Vec::new();
    for z in first.z..=last.z {
        for y in first.y..=last.y {
            for x in first.x..=last.x {
                ids.push(StaticVoxelBrickId(IVec3::new(x, y, z)));
            }
        }
    }
    ids
}

fn terrain_brick_ids_for_inclusive_uvec_aabb(
    min: UVec3,
    max_inclusive: UVec3,
) -> Vec<StaticVoxelBrickId> {
    if min.cmpgt(max_inclusive).any() {
        return Vec::new();
    }
    let Some(max_exclusive) = max_inclusive
        .x
        .checked_add(1)
        .zip(max_inclusive.y.checked_add(1))
        .zip(max_inclusive.z.checked_add(1))
        .map(|((x, y), z)| UVec3::new(x, y, z))
    else {
        log::error!(
            "[COLLISION][TERRAIN_BRICK] inclusive voxel bound overflows: min={min:?} max={max_inclusive:?}"
        );
        return Vec::new();
    };
    let (Ok(min), Ok(max_exclusive)) = (IVec3::try_from(min), IVec3::try_from(max_exclusive))
    else {
        log::error!(
            "[COLLISION][TERRAIN_BRICK] voxel bound exceeds signed collider coordinates: min={min:?} max_exclusive={max_exclusive:?}"
        );
        return Vec::new();
    };
    terrain_brick_ids_for_voxel_aabb(min, max_exclusive)
}

fn collision_probe_convex_points() -> Vec<Vec3> {
    convex_points_for_voxels(collision_probe_apple_offsets())
}

fn apple_convex_points() -> Vec<Vec3> {
    convex_points_for_voxels(voxel_apple_offsets())
}

fn convex_points_for_voxels(voxels: Vec<IVec3>) -> Vec<Vec3> {
    let mut corners = HashSet::new();
    for voxel in voxels {
        for x in 0..=1 {
            for y in 0..=1 {
                for z in 0..=1 {
                    corners.insert(voxel + IVec3::new(x, y, z));
                }
            }
        }
    }
    let mut corners = corners.into_iter().collect::<Vec<_>>();
    corners.sort_unstable_by_key(|corner| (corner.x, corner.y, corner.z));
    corners.into_iter().map(IVec3::as_vec3).collect()
}

fn fruit_drop_path_bricks(position_voxels: UVec3) -> Vec<StaticVoxelBrickId> {
    let fruit_radius_voxels = TREE_FRUIT_MAX_RADIUS_VOXELS as i32;
    let position = position_voxels.as_ivec3();
    let min = IVec3::new(
        position.x - fruit_radius_voxels,
        0,
        position.z - fruit_radius_voxels,
    );
    let max_exclusive = IVec3::new(
        position.x + fruit_radius_voxels + 1,
        position.y + fruit_radius_voxels + 1,
        position.z + fruit_radius_voxels + 1,
    );
    terrain_brick_ids_for_voxel_aabb(min, max_exclusive)
}

fn single_source_revision(block: &ContreeCpuVoxelBlock) -> Result<u64, &'static str> {
    let [dependency] = block.source_dependencies.as_slice() else {
        return Err("expected exactly one source dependency");
    };
    dependency
        .source_revision
        .ok_or("source chunk has no revision")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::ContreeCpuVoxelSourceDependency;

    fn block_with_dependencies(
        source_dependencies: Vec<ContreeCpuVoxelSourceDependency>,
    ) -> ContreeCpuVoxelBlock {
        ContreeCpuVoxelBlock {
            voxel_min: STARTUP_TERRAIN_BRICK_MIN,
            dim: UVec3::splat(STATIC_VOXEL_BRICK_DIM),
            voxel_dim_per_chunk: UVec3::splat(256),
            voxel_types: vec![0; STATIC_VOXEL_BRICK_DIM.pow(3) as usize],
            source_dependencies,
        }
    }

    fn fruit_spec() -> TreeFruitSpec {
        TreeFruitSpec {
            id: 7,
            position_voxels: UVec3::new(64, 96, 64),
            grow_start_age: 0.55,
            full_growth_age: 0.70,
            drop_age: 0.88,
            linear_velocity_voxels: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
        }
    }

    #[test]
    fn fruit_grows_through_discrete_voxel_radius_stages_then_detaches() {
        let fruit = fruit_spec();
        assert_eq!(fruit.attached_radius_voxels(0.54), None);
        assert_eq!(fruit.attached_radius_voxels(0.55), Some(1));
        assert_eq!(fruit.attached_radius_voxels(0.60), Some(1));
        assert_eq!(fruit.attached_radius_voxels(0.625), Some(2));
        assert_eq!(fruit.attached_radius_voxels(0.70), Some(2));
        assert_eq!(fruit.attached_radius_voxels(0.88), None);
    }

    #[test]
    fn backward_scrub_rearms_each_fruit_for_another_upward_sweep() {
        let drop_age = 0.88;
        let first_drop = fruit_phase_after_age_change(FruitSweepPhase::Armed, 0.70, 0.90, drop_age);
        assert_eq!(first_drop, FruitSweepPhase::PendingDrop);

        let rearmed = fruit_phase_after_age_change(FruitSweepPhase::Dropped, 0.90, 0.60, drop_age);
        assert_eq!(rearmed, FruitSweepPhase::Armed);
        assert_eq!(
            fruit_phase_after_age_change(rearmed, 0.60, 0.90, drop_age),
            FruitSweepPhase::PendingDrop,
        );
    }

    #[test]
    fn shallow_backward_scrub_above_threshold_does_not_duplicate_a_drop() {
        let phase = fruit_phase_after_age_change(FruitSweepPhase::Dropped, 1.0, 0.95, 0.88);
        assert_eq!(phase, FruitSweepPhase::Dropped);
        assert_eq!(
            fruit_phase_after_age_change(phase, 0.95, 1.0, 0.88),
            FruitSweepPhase::Dropped,
        );
    }

    #[test]
    fn fruit_drop_path_requests_every_vertical_collider_brick() {
        let bricks = fruit_drop_path_bricks(UVec3::new(65, 97, 65));
        for y in 0..=3 {
            assert!(bricks.contains(&StaticVoxelBrickId(IVec3::new(2, y, 2))));
        }
    }

    #[test]
    fn startup_brick_bounds_match_its_physics_id() {
        assert_eq!(
            STARTUP_TERRAIN_BRICK_ID.0.as_uvec3() * STATIC_VOXEL_BRICK_DIM,
            STARTUP_TERRAIN_BRICK_MIN
        );
    }

    #[test]
    fn dirty_brick_queue_coalesces_to_the_latest_request_revision() {
        let id = StaticVoxelBrickId(IVec3::new(8, 3, 8));
        let mut queue = DirtyTerrainBrickQueue::default();
        assert_eq!(queue.push(id), 1);
        assert_eq!(queue.push(id), 2);
        assert_eq!(queue.len(), 1);

        let work = queue.front().unwrap();
        assert_eq!(work.request_revision, 2);
        queue.complete(work);
        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn deferred_brick_work_remains_queued_for_retry() {
        let first = StaticVoxelBrickId(IVec3::new(8, 3, 8));
        let second = StaticVoxelBrickId(IVec3::new(9, 3, 8));
        let mut queue = DirtyTerrainBrickQueue::default();
        queue.push(first);
        queue.push(second);

        let not_ready = queue.front().unwrap();
        queue.defer(not_ready.id);

        assert_eq!(queue.len(), 2);
        assert_eq!(queue.front().unwrap().id, second);
        queue.complete(queue.front().unwrap());
        assert_eq!(queue.front().unwrap().id, first);
    }

    #[test]
    fn stale_completion_does_not_drop_a_newer_coalesced_request() {
        let id = StaticVoxelBrickId(IVec3::new(8, 3, 8));
        let mut queue = DirtyTerrainBrickQueue::default();
        queue.push(id);
        let old = queue.front().unwrap();
        queue.push(id);

        queue.complete(old);

        assert_eq!(queue.len(), 1);
        assert!(queue.front().unwrap().request_revision > old.request_revision);
    }

    #[test]
    fn voxel_aabb_to_bricks_uses_exclusive_max_and_euclidean_negative_division() {
        assert_eq!(
            terrain_brick_ids_for_voxel_aabb(IVec3::ZERO, IVec3::splat(32)),
            vec![StaticVoxelBrickId(IVec3::ZERO)]
        );
        assert_eq!(
            terrain_brick_ids_for_voxel_aabb(IVec3::splat(32), IVec3::splat(33)),
            vec![StaticVoxelBrickId(IVec3::ONE)]
        );
        assert_eq!(
            terrain_brick_ids_for_voxel_aabb(IVec3::splat(-32), IVec3::ZERO),
            vec![StaticVoxelBrickId(IVec3::splat(-1))]
        );
        assert_eq!(
            terrain_brick_ids_for_voxel_aabb(IVec3::splat(-1), IVec3::ONE),
            vec![
                StaticVoxelBrickId(IVec3::new(-1, -1, -1)),
                StaticVoxelBrickId(IVec3::new(0, -1, -1)),
                StaticVoxelBrickId(IVec3::new(-1, 0, -1)),
                StaticVoxelBrickId(IVec3::new(0, 0, -1)),
                StaticVoxelBrickId(IVec3::new(-1, -1, 0)),
                StaticVoxelBrickId(IVec3::new(0, -1, 0)),
                StaticVoxelBrickId(IVec3::new(-1, 0, 0)),
                StaticVoxelBrickId(IVec3::ZERO),
            ]
        );
    }

    #[test]
    fn inclusive_voxel_aabb_marks_the_brick_containing_its_maximum_voxel() {
        assert_eq!(
            terrain_brick_ids_for_inclusive_uvec_aabb(UVec3::new(31, 7, 9), UVec3::new(32, 7, 9),),
            vec![
                StaticVoxelBrickId(IVec3::new(0, 0, 0)),
                StaticVoxelBrickId(IVec3::new(1, 0, 0)),
            ]
        );
        assert_eq!(
            terrain_brick_ids_for_inclusive_uvec_aabb(UVec3::splat(32), UVec3::splat(32)),
            vec![StaticVoxelBrickId(IVec3::ONE)]
        );
    }

    #[test]
    fn collision_probe_convex_points_cover_visible_apple_bounds() {
        let points = collision_probe_convex_points();
        let min = points.iter().copied().reduce(Vec3::min).unwrap();
        let max = points.iter().copied().reduce(Vec3::max).unwrap();
        let probe_radius = TREE_FRUIT_MAX_RADIUS_VOXELS as f32 * 2.0;

        assert!(points.len() > collision_probe_apple_offsets().len());
        assert_eq!(min, Vec3::splat(-probe_radius));
        assert_eq!(max, Vec3::splat(probe_radius));
    }

    #[test]
    fn collision_probe_spawns_inside_imported_brick_xz_bounds() {
        let probe_radius = Vec3::splat(TREE_FRUIT_MAX_RADIUS_VOXELS as f32 * 2.0);
        let brick_min = STARTUP_TERRAIN_BRICK_MIN.as_vec3();
        let brick_max = brick_min + Vec3::splat(STATIC_VOXEL_BRICK_DIM as f32);

        assert!((COLLISION_PROBE_SPAWN_VOXELS - probe_radius).x >= brick_min.x);
        assert!((COLLISION_PROBE_SPAWN_VOXELS - probe_radius).z >= brick_min.z);
        assert!((COLLISION_PROBE_SPAWN_VOXELS + probe_radius).x <= brick_max.x);
        assert!((COLLISION_PROBE_SPAWN_VOXELS + probe_radius).z <= brick_max.z);
    }

    #[test]
    fn accepts_one_present_source_revision() {
        let block = block_with_dependencies(vec![ContreeCpuVoxelSourceDependency {
            chunk_idx: UVec3::new(1, 0, 1),
            source_revision: Some(42),
            is_present: true,
        }]);

        assert_eq!(single_source_revision(&block), Ok(42));
    }

    #[test]
    fn accepts_revision_from_an_absent_source_chunk_for_empty_tombstones() {
        let block = block_with_dependencies(vec![ContreeCpuVoxelSourceDependency {
            chunk_idx: UVec3::new(1, 0, 1),
            source_revision: Some(43),
            is_present: false,
        }]);

        assert_eq!(single_source_revision(&block), Ok(43));
    }

    #[test]
    fn rejects_missing_or_ambiguous_source_dependencies() {
        let missing_revision = block_with_dependencies(vec![ContreeCpuVoxelSourceDependency {
            chunk_idx: UVec3::new(1, 0, 1),
            source_revision: None,
            is_present: true,
        }]);
        assert_eq!(
            single_source_revision(&missing_revision),
            Err("source chunk has no revision")
        );

        let no_dependencies = block_with_dependencies(Vec::new());
        assert_eq!(
            single_source_revision(&no_dependencies),
            Err("expected exactly one source dependency")
        );
    }
}
