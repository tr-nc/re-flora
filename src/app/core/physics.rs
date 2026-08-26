use crate::builder::{ContreeBuilder, ContreeCpuVoxelBlock, ContreeCpuVoxelBlockExport};
use crate::gameplay::camera::{PlayerWalkMovementRequest, PlayerWalkMovementResult};
use crate::geom::UAabb3;
use crate::tracer::{
    voxel_apple_offsets, DynamicFruitRenderInstance, Tracer, TREE_FRUIT_MAX_RADIUS_VOXELS,
};
use anyhow::Context;
use glam::{IVec3, UVec3, Vec3};
use re_flora_physics::{
    BrickOccupancy, CapsuleCharacterMove, CollisionWorld, DynamicBodyDesc, DynamicBodyId,
    DynamicColliderShape, StaticVoxelBrickId, StaticVoxelBrickUpdate, STATIC_VOXEL_BRICK_DIM,
};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

const VOXELS_PER_WORLD_UNIT: f32 = 256.0;
const PLAYER_CAPSULE_RADIUS_VOXELS: f32 = 4.0;
const PLAYER_CAPSULE_HALF_HEIGHT_VOXELS: f32 = 8.0;
const DYNAMIC_FRUIT_GRAVITY_VOXELS: Vec3 = Vec3::new(0.0, -9.8 * VOXELS_PER_WORLD_UNIT, 0.0);
const APPLE_CONTACT_SKIN_VOXELS: f32 = 0.15;
const APPLE_FRICTION: f32 = 0.82;
const APPLE_RESTITUTION: f32 = 0.12;
const APPLE_LINEAR_DAMPING: f32 = 0.06;
const APPLE_ANGULAR_DAMPING: f32 = 0.10;
// Release measurements put a 32-cubed Contree export plus Rapier update below 0.75 ms. Use a time
// budget for prompt local edits while retaining a hard cap for unusually cheap or deferred work.
const TERRAIN_COLLIDER_UPDATE_BUDGET: Duration = Duration::from_millis(1);
const MAX_TERRAIN_COLLIDER_BRICKS_PER_FRAME: usize = 8;
const WORLD_TERRAIN_COLLIDER_IMPORT_BUDGET: Duration = Duration::from_millis(25);

fn player_capsule_center_voxels(camera_position: Vec3, camera_height: f32) -> Vec3 {
    let foot_y = camera_position.y - camera_height;
    Vec3::new(
        camera_position.x,
        foot_y
            + (PLAYER_CAPSULE_HALF_HEIGHT_VOXELS + PLAYER_CAPSULE_RADIUS_VOXELS)
                / VOXELS_PER_WORLD_UNIT,
        camera_position.z,
    ) * VOXELS_PER_WORLD_UNIT
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct TreeFruitSpec {
    pub id: u64,
    pub position_voxels: UVec3,
    pub grow_start_phase: f32,
    pub full_growth_phase: f32,
    pub drop_phase: f32,
    pub linear_velocity_voxels: Vec3,
    pub angular_velocity: Vec3,
}

impl TreeFruitSpec {
    pub(super) fn attached_radius_voxels(&self, cycle: f32) -> Option<u32> {
        if cycle < self.grow_start_phase || cycle >= self.drop_phase {
            return None;
        }
        let duration = (self.full_growth_phase - self.grow_start_phase).max(f32::EPSILON);
        let progress = ((cycle - self.grow_start_phase) / duration).clamp(0.0, 1.0);
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

fn fruit_phase_after_cycle_change(
    phase: FruitSweepPhase,
    previous_cycle: f32,
    fruit_cycle: f32,
    drop_phase: f32,
) -> FruitSweepPhase {
    if fruit_cycle < previous_cycle && fruit_cycle < drop_phase {
        FruitSweepPhase::Armed
    } else if fruit_cycle > previous_cycle
        && phase == FruitSweepPhase::Armed
        && previous_cycle < drop_phase
        && fruit_cycle >= drop_phase
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

#[derive(Debug)]
struct WorldTerrainColliderImport {
    remaining: HashSet<StaticVoxelBrickId>,
    total: usize,
    failed: usize,
    started_at: Instant,
}

impl DirtyTerrainBrickQueue {
    fn next_revision(&mut self) -> u64 {
        self.next_revision = self
            .next_revision
            .checked_add(1)
            .expect("terrain collider request revision space exhausted");
        self.next_revision
    }

    fn push(&mut self, id: StaticVoxelBrickId) -> u64 {
        let revision = self.next_revision();
        if self.latest_revisions.insert(id, revision).is_none() {
            self.pending.push_back(id);
        }
        revision
    }

    fn push_urgent(&mut self, id: StaticVoxelBrickId) -> u64 {
        let revision = self.next_revision();
        if self.latest_revisions.insert(id, revision).is_some() {
            let position = self
                .pending
                .iter()
                .position(|pending_id| *pending_id == id)
                .expect("tracked terrain collider brick must be pending");
            self.pending.remove(position);
        }
        self.pending.push_front(id);
        revision
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
    dirty_terrain_bricks: DirtyTerrainBrickQueue,
    imported_terrain_bricks: HashSet<StaticVoxelBrickId>,
    failed_terrain_bricks: HashSet<StaticVoxelBrickId>,
    world_collider_import: Option<WorldTerrainColliderImport>,
    fruits_by_tree: BTreeMap<u32, BTreeMap<u64, RegisteredFruit>>,
    attached_fruit_refresh_trees: HashSet<u32>,
    fruit_cycle: f32,
}

impl TerrainPhysics {
    pub(super) fn new(fruit_cycle: f32) -> Self {
        let mut collision_world = CollisionWorld::new();
        collision_world
            .set_gravity(DYNAMIC_FRUIT_GRAVITY_VOXELS)
            .expect("dynamic fruit gravity constant must be valid");
        Self {
            collision_world,
            dirty_terrain_bricks: DirtyTerrainBrickQueue::default(),
            imported_terrain_bricks: HashSet::new(),
            failed_terrain_bricks: HashSet::new(),
            world_collider_import: None,
            fruits_by_tree: BTreeMap::new(),
            attached_fruit_refresh_trees: HashSet::new(),
            fruit_cycle: fruit_cycle.clamp(0.0, 1.0),
        }
    }

    pub(super) fn move_player_capsule(
        &mut self,
        request: PlayerWalkMovementRequest,
        frame_delta_time: f32,
    ) -> anyhow::Result<PlayerWalkMovementResult> {
        let capsule_center =
            player_capsule_center_voxels(request.camera_position, request.camera_height);
        let movement = self
            .collision_world
            .move_capsule_character(CapsuleCharacterMove {
                center: capsule_center,
                radius: PLAYER_CAPSULE_RADIUS_VOXELS,
                half_height: PLAYER_CAPSULE_HALF_HEIGHT_VOXELS,
                desired_translation: request.desired_translation * VOXELS_PER_WORLD_UNIT,
                dt: frame_delta_time,
            })
            .context("moving the player capsule through terrain")?;

        Ok(PlayerWalkMovementResult {
            translation: movement.translation / VOXELS_PER_WORLD_UNIT,
            grounded: movement.grounded,
        })
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
        if !has_fruit_bodies {
            return self.sync_dynamic_fruit_rendering(tracer);
        }
        let step = self.collision_world.advance(frame_delta_time);
        if step.dropped_seconds > 0.0 {
            log::warn!(
                "[COLLISION][FRUIT] physics hitch dropped {:.3} ms",
                step.dropped_seconds * 1_000.0
            );
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
        tracer: &mut Tracer,
    ) -> anyhow::Result<()> {
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
                    phase: if self.fruit_cycle >= spec.drop_phase {
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

    pub(super) fn set_fruit_cycle(
        &mut self,
        fruit_cycle: f32,
        tracer: &mut Tracer,
    ) -> anyhow::Result<()> {
        let fruit_cycle = fruit_cycle.clamp(0.0, 1.0);
        let previous_cycle = self.fruit_cycle;
        if fruit_cycle < previous_cycle {
            for fruits in self.fruits_by_tree.values_mut() {
                for fruit in fruits.values_mut() {
                    let next_phase = fruit_phase_after_cycle_change(
                        fruit.phase,
                        previous_cycle,
                        fruit_cycle,
                        fruit.spec.drop_phase,
                    );
                    if next_phase == FruitSweepPhase::Armed {
                        if let Some(body) = fruit.body.take() {
                            self.collision_world.remove_dynamic_body(body);
                        }
                    }
                    fruit.phase = next_phase;
                }
            }
        } else if fruit_cycle > previous_cycle {
            for fruits in self.fruits_by_tree.values_mut() {
                for fruit in fruits.values_mut() {
                    fruit.phase = fruit_phase_after_cycle_change(
                        fruit.phase,
                        previous_cycle,
                        fruit_cycle,
                        fruit.spec.drop_phase,
                    );
                }
            }
        }
        self.fruit_cycle = fruit_cycle;
        if fruit_cycle != previous_cycle {
            self.attached_fruit_refresh_trees
                .extend(self.fruits_by_tree.keys().copied());
        }
        self.spawn_pending_fruits()?;
        self.sync_dynamic_fruit_rendering(tracer)
    }

    pub(super) fn attached_tree_fruits(&self, tree_id: u32) -> Vec<AttachedTreeFruit> {
        self.fruits_by_tree
            .get(&tree_id)
            .into_iter()
            .flat_map(BTreeMap::values)
            .filter_map(|fruit| {
                let radius_voxels = match fruit.phase {
                    FruitSweepPhase::Armed => {
                        fruit.spec.attached_radius_voxels(self.fruit_cycle)?
                    }
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
            let desc = apple_dynamic_body_desc(
                spec.position_voxels.as_vec3(),
                spec.linear_velocity_voxels,
                spec.angular_velocity,
            );
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
                "[COLLISION][FRUIT] dropped tree={} fruit={} body={} cycle={:.3} position_voxels={:?}",
                tree_id,
                fruit_id,
                body.get(),
                self.fruit_cycle,
                spec.position_voxels,
            );
        }
        Ok(())
    }

    fn sync_dynamic_fruit_rendering(&self, tracer: &mut Tracer) -> anyhow::Result<()> {
        let mut instances = Vec::new();
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
            tracer.clear_dynamic_fruit_geometry();
            Ok(())
        } else {
            tracer.show_dynamic_fruit_geometry(&instances)
        }
    }

    pub(super) fn mark_terrain_voxels_dirty(&mut self, affected_voxels: UAabb3) {
        for id in terrain_brick_ids_for_voxel_aabb(
            affected_voxels.min().as_ivec3(),
            affected_voxels.max().as_ivec3(),
        ) {
            self.mark_terrain_brick_dirty_urgent(id);
        }
    }

    pub(super) fn begin_world_terrain_collider_import(
        &mut self,
        world_dim_voxels: UVec3,
    ) -> anyhow::Result<usize> {
        anyhow::ensure!(
            self.world_collider_import.is_none(),
            "world terrain collider import is already active"
        );
        let world_max = IVec3::try_from(world_dim_voxels)
            .context("world voxel dimensions exceed signed collider coordinates")?;
        let world_bricks = terrain_brick_ids_for_voxel_aabb(IVec3::ZERO, world_max);
        anyhow::ensure!(
            !world_bricks.is_empty(),
            "world voxel dimensions must be non-zero"
        );

        let total = world_bricks.len();
        self.world_collider_import = Some(WorldTerrainColliderImport {
            remaining: world_bricks.iter().copied().collect(),
            total,
            failed: 0,
            started_at: Instant::now(),
        });

        for brick in world_bricks {
            self.mark_terrain_brick_dirty(brick);
        }
        Ok(total)
    }

    pub(super) fn process_world_terrain_collider_import(
        &mut self,
        contree_builder: &ContreeBuilder,
    ) -> anyhow::Result<(usize, usize)> {
        anyhow::ensure!(
            self.world_collider_import.is_some(),
            "world terrain collider import has not started"
        );

        let frame_started = Instant::now();
        let mut attempts = 0;
        let mut consecutive_deferrals = 0;
        while attempts == 0 || frame_started.elapsed() < WORLD_TERRAIN_COLLIDER_IMPORT_BUDGET {
            let Some(work) = self.dirty_terrain_bricks.front() else {
                break;
            };
            let is_world_brick = self
                .world_collider_import
                .as_ref()
                .is_some_and(|import| import.remaining.contains(&work.id));
            attempts += 1;
            if self.try_refresh_terrain_brick(contree_builder, work) {
                consecutive_deferrals = 0;
                if is_world_brick {
                    let failed = self.failed_terrain_bricks.contains(&work.id);
                    let import = self
                        .world_collider_import
                        .as_mut()
                        .expect("active world collider import disappeared");
                    import.remaining.remove(&work.id);
                    import.failed += usize::from(failed);
                }
            } else {
                self.dirty_terrain_bricks.defer(work.id);
                consecutive_deferrals += 1;
                if consecutive_deferrals >= self.dirty_terrain_bricks.len() {
                    break;
                }
            }
        }

        // Build the character-query BVH incrementally with the collider import. Otherwise the
        // first switch to walk mode would have to index the whole world in one gameplay frame.
        self.collision_world.sync_capsule_character_queries();

        let import = self
            .world_collider_import
            .as_ref()
            .expect("active world collider import disappeared");
        let completed = import.total - import.remaining.len();
        let total = import.total;
        if completed < total {
            anyhow::ensure!(
                !self.dirty_terrain_bricks.pending.is_empty(),
                "world terrain collider queue emptied with {} bricks remaining",
                import.remaining.len()
            );
            return Ok((completed, total));
        }

        let import = self
            .world_collider_import
            .take()
            .expect("completed world collider import disappeared");
        let elapsed = import.started_at.elapsed();
        log::info!(
            "[COLLISION][WORLD] imported logical_bricks={} non_empty_colliders={} elapsed_ms={:.3} avg_ms_per_brick={:.3}",
            import.total,
            self.collision_world.static_brick_count(),
            elapsed.as_secs_f64() * 1_000.0,
            elapsed.as_secs_f64() * 1_000.0 / import.total as f64,
        );
        anyhow::ensure!(
            import.failed == 0,
            "{} world terrain collider bricks failed",
            import.failed
        );
        Ok((import.total, import.total))
    }

    fn mark_terrain_brick_dirty(&mut self, id: StaticVoxelBrickId) {
        let request_revision = self.dirty_terrain_bricks.push(id);
        log::debug!(
            "[COLLISION][TERRAIN_BRICK] queued id={id:?} request_revision={request_revision} pending={}",
            self.dirty_terrain_bricks.len(),
        );
    }

    fn mark_terrain_brick_dirty_urgent(&mut self, id: StaticVoxelBrickId) {
        let request_revision = self.dirty_terrain_bricks.push_urgent(id);
        log::debug!(
            "[COLLISION][TERRAIN_BRICK] queued urgent id={id:?} request_revision={request_revision} pending={}",
            self.dirty_terrain_bricks.len(),
        );
    }

    pub(super) fn process_terrain_collider_updates(&mut self, contree_builder: &ContreeBuilder) {
        let frame_started = Instant::now();
        for attempt in 0..MAX_TERRAIN_COLLIDER_BRICKS_PER_FRAME {
            if attempt > 0 && frame_started.elapsed() >= TERRAIN_COLLIDER_UPDATE_BUDGET {
                break;
            }
            let Some(work) = self.dirty_terrain_bricks.front() else {
                break;
            };
            if !self.try_refresh_terrain_brick(contree_builder, work) {
                self.dirty_terrain_bricks.defer(work.id);
            }
        }
    }

    pub(super) fn terrain_collider_pending_len(&self) -> usize {
        self.dirty_terrain_bricks.len()
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
        if !matches!(update, StaticVoxelBrickUpdate::Stale { .. }) {
            self.imported_terrain_bricks.insert(work.id);
            self.failed_terrain_bricks.remove(&work.id);
        }
        log::debug!(
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

fn apple_convex_points() -> Vec<Vec3> {
    convex_points_for_voxels(voxel_apple_offsets())
}

fn apple_dynamic_body_desc(
    position: Vec3,
    linear_velocity: Vec3,
    angular_velocity: Vec3,
) -> DynamicBodyDesc {
    let mut desc = DynamicBodyDesc::sphere(position, 2.0);
    desc.collider = DynamicColliderShape::ConvexHull {
        points: apple_convex_points(),
    };
    desc.linear_velocity = linear_velocity;
    desc.angular_velocity = angular_velocity;
    desc.friction = APPLE_FRICTION;
    desc.restitution = APPLE_RESTITUTION;
    desc.contact_skin = APPLE_CONTACT_SKIN_VOXELS;
    desc.linear_damping = APPLE_LINEAR_DAMPING;
    desc.angular_damping = APPLE_ANGULAR_DAMPING;
    desc.ccd_enabled = true;
    desc
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
            voxel_min: UVec3::ZERO,
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
            grow_start_phase: 0.55,
            full_growth_phase: 0.70,
            drop_phase: 0.88,
            linear_velocity_voxels: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
        }
    }

    fn two_layer_flat_floor() -> BrickOccupancy {
        BrickOccupancy::from_filled_voxels((0..2).flat_map(|y| {
            (0..STATIC_VOXEL_BRICK_DIM)
                .flat_map(move |z| (0..STATIC_VOXEL_BRICK_DIM).map(move |x| UVec3::new(x, y, z)))
        }))
    }

    #[test]
    fn dropped_apple_stops_visible_motion_on_flat_terrain_within_five_seconds() {
        const MAX_STEPS: usize = 5 * 120;
        let mut world = CollisionWorld::new();
        world.set_gravity(DYNAMIC_FRUIT_GRAVITY_VOXELS).unwrap();
        for z in 0..4 {
            for x in 0..4 {
                world.upsert_static_voxel_brick(
                    StaticVoxelBrickId(IVec3::new(x, 0, z)),
                    1,
                    two_layer_flat_floor(),
                );
            }
        }
        let body = world
            .spawn_dynamic_body(apple_dynamic_body_desc(
                Vec3::new(16.0, 28.0, 16.0),
                Vec3::new(7.0, -2.0, 7.0),
                Vec3::new(2.5, 2.5, -2.5),
            ))
            .unwrap();

        for _ in 0..MAX_STEPS {
            assert_eq!(world.advance(1.0 / 120.0).steps, 1);
        }

        let state = world.dynamic_body_state(body).unwrap();
        assert!(
            state.linear_velocity.x.hypot(state.linear_velocity.z) < 0.01,
            "apple still translating across the floor after five seconds: {state:?}",
        );
        assert!(
            state.angular_velocity.length() < 0.01,
            "apple still visibly rotating after five seconds: {state:?}",
        );
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
        let drop_phase = 0.88;
        let first_drop =
            fruit_phase_after_cycle_change(FruitSweepPhase::Armed, 0.70, 0.90, drop_phase);
        assert_eq!(first_drop, FruitSweepPhase::PendingDrop);

        let rearmed =
            fruit_phase_after_cycle_change(FruitSweepPhase::Dropped, 0.90, 0.60, drop_phase);
        assert_eq!(rearmed, FruitSweepPhase::Armed);
        assert_eq!(
            fruit_phase_after_cycle_change(rearmed, 0.60, 0.90, drop_phase),
            FruitSweepPhase::PendingDrop,
        );
    }

    #[test]
    fn shallow_backward_scrub_above_threshold_does_not_duplicate_a_drop() {
        let phase = fruit_phase_after_cycle_change(FruitSweepPhase::Dropped, 1.0, 0.95, 0.88);
        assert_eq!(phase, FruitSweepPhase::Dropped);
        assert_eq!(
            fruit_phase_after_cycle_change(phase, 0.95, 1.0, 0.88),
            FruitSweepPhase::Dropped,
        );

        assert_eq!(
            fruit_phase_after_cycle_change(FruitSweepPhase::PendingDrop, 1.0, 0.95, 0.88),
            FruitSweepPhase::PendingDrop,
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
    fn world_voxel_bounds_cover_every_global_collider_brick() {
        let bricks = terrain_brick_ids_for_voxel_aabb(IVec3::ZERO, IVec3::splat(512));

        assert_eq!(bricks.len(), 16_usize.pow(3));
        assert_eq!(bricks.first(), Some(&StaticVoxelBrickId(IVec3::ZERO)));
        assert_eq!(bricks.last(), Some(&StaticVoxelBrickId(IVec3::splat(15))));
    }

    #[test]
    fn world_collider_import_enqueues_each_global_brick_once() {
        let mut terrain_physics = TerrainPhysics::new(1.0);

        let total = terrain_physics
            .begin_world_terrain_collider_import(UVec3::splat(512))
            .unwrap();

        assert_eq!(total, 16_usize.pow(3));
        assert_eq!(terrain_physics.dirty_terrain_bricks.len(), total);
        assert_eq!(
            terrain_physics
                .world_collider_import
                .as_ref()
                .unwrap()
                .remaining
                .len(),
            total
        );
    }

    #[test]
    fn player_capsule_bottom_stays_at_the_camera_foot_position() {
        let camera_position = Vec3::new(1.25, 0.75, 0.5);
        let camera_height = 0.08;
        let center = player_capsule_center_voxels(camera_position, camera_height);
        let capsule_bottom =
            center.y - PLAYER_CAPSULE_HALF_HEIGHT_VOXELS - PLAYER_CAPSULE_RADIUS_VOXELS;

        assert_eq!(center.x, camera_position.x * VOXELS_PER_WORLD_UNIT);
        assert_eq!(center.z, camera_position.z * VOXELS_PER_WORLD_UNIT);
        assert!(
            (capsule_bottom / VOXELS_PER_WORLD_UNIT - (camera_position.y - camera_height)).abs()
                < 1.0e-6
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
    fn local_terrain_edit_enqueues_only_intersecting_collider_bricks() {
        let mut terrain_physics = TerrainPhysics::new(1.0);
        let edit_bound = crate::geom::UAabb3::new(UVec3::splat(31), UVec3::splat(33));

        terrain_physics.mark_terrain_voxels_dirty(edit_bound);

        let queued = terrain_physics
            .dirty_terrain_bricks
            .pending
            .iter()
            .copied()
            .collect::<HashSet<_>>();
        let expected = terrain_brick_ids_for_voxel_aabb(
            edit_bound.min().as_ivec3(),
            edit_bound.max().as_ivec3(),
        )
        .into_iter()
        .collect::<HashSet<_>>();
        assert_eq!(queued, expected);
        assert_eq!(queued.len(), 8);
    }

    #[test]
    fn newest_terrain_edit_moves_a_brick_ahead_of_background_backlog() {
        let mut terrain_physics = TerrainPhysics::new(1.0);
        terrain_physics
            .begin_world_terrain_collider_import(UVec3::splat(512))
            .unwrap();
        assert_eq!(terrain_physics.dirty_terrain_bricks.len(), 4_096);

        terrain_physics.mark_terrain_voxels_dirty(crate::geom::UAabb3::new(
            UVec3::new(256, 0, 256),
            UVec3::new(257, 1, 257),
        ));

        assert_eq!(terrain_physics.dirty_terrain_bricks.len(), 4_096);
        assert_eq!(
            terrain_physics.dirty_terrain_bricks.front().unwrap().id,
            StaticVoxelBrickId(IVec3::new(8, 0, 8)),
        );
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
