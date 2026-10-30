use crate::builder::{ContreeBuilder, ContreeCpuVoxelBlock, ContreeCpuVoxelBlockExport};
use crate::gameplay::camera::{PlayerWalkMovementRequest, PlayerWalkMovementResult};
use crate::geom::UAabb3;
use crate::tracer::{
    voxel_apple_offsets, DynamicFruitRenderInstance, Tracer, TREE_FRUIT_MAX_RADIUS_VOXELS,
};
use anyhow::Context;
use glam::{IVec3, UVec3, Vec3};
use rand::{rngs::SmallRng, RngExt, SeedableRng};
use re_flora_physics::{
    BrickOccupancy, CollisionWorld, DynamicBodyDesc, DynamicBodyId, DynamicColliderShape,
    StaticVoxelBrickId, StaticVoxelBrickUpdate, STATIC_VOXEL_BRICK_DIM,
};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

mod fruit_ground_trace;
mod harvesting;
mod surface_motion;
mod walk_recovery;
use fruit_ground_trace::FruitGroundTrace;

const VOXELS_PER_WORLD_UNIT: f32 = 256.0;
#[cfg(test)]
const PLAYER_CAPSULE_RADIUS_VOXELS: f32 = surface_motion::PLAYER.radius_voxels;
#[cfg(test)]
const PLAYER_CAPSULE_HALF_HEIGHT_VOXELS: f32 = surface_motion::PLAYER.half_height_voxels;
const DYNAMIC_FRUIT_GRAVITY_VOXELS: Vec3 = Vec3::new(0.0, -9.8 * VOXELS_PER_WORLD_UNIT, 0.0);
const APPLE_CONTACT_SKIN_VOXELS: f32 = 0.15;
const APPLE_FRICTION: f32 = 0.82;
const APPLE_RESTITUTION: f32 = 0.12;
const APPLE_LINEAR_DAMPING: f32 = 0.06;
const APPLE_ANGULAR_DAMPING: f32 = 0.10;
// The tilted replay needs extra convergence; the full terrain scene still jitters with 2.
// Spend the extra solve work on fruit contact islands without changing global query/step settings.
const APPLE_ADDITIONAL_SOLVER_ITERATIONS: usize = 4;
// Release measurements put a 32-cubed Contree export plus Rapier update below 0.75 ms. Use a time
// budget for prompt local edits while retaining a hard cap for unusually cheap or deferred work.
const TERRAIN_COLLIDER_UPDATE_BUDGET: Duration = Duration::from_millis(1);
const MAX_TERRAIN_COLLIDER_BRICKS_PER_FRAME: usize = 8;
const WORLD_TERRAIN_COLLIDER_IMPORT_BUDGET: Duration = Duration::from_millis(25);

#[cfg(test)]
fn player_capsule_center_voxels(camera_position: Vec3, camera_height: f32) -> Vec3 {
    surface_motion::PLAYER.center_voxels(camera_position - Vec3::Y * camera_height)
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct TreeFruitSpec {
    pub id: u64,
    pub position_voxels: UVec3,
    /// Exact woody support and its owner; populated before tree publication.
    pub attachment: Option<(usize, Vec3)>,
    pub grow_start_phase: f32,
    pub full_growth_phase: f32,
    pub connection_strength: f32,
    pub linear_velocity_voxels: Vec3,
    pub angular_velocity: Vec3,
}

impl TreeFruitSpec {
    pub(super) fn attached_radius_voxels(&self, cycle: f32) -> Option<u32> {
        if cycle < self.grow_start_phase {
            return None;
        }
        if cycle >= self.full_growth_phase {
            return Some(TREE_FRUIT_MAX_RADIUS_VOXELS);
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
    growth_start: f32,
) -> FruitSweepPhase {
    if fruit_cycle < previous_cycle && fruit_cycle < growth_start {
        FruitSweepPhase::Armed
    } else {
        phase
    }
}

#[derive(Clone, Debug)]
struct RegisteredFruit {
    spec: TreeFruitSpec,
    required_bricks: Vec<StaticVoxelBrickId>,
    phase: FruitSweepPhase,
    body: Option<DynamicBodyId>,
    handoff: Option<crate::tracer::FruitReleaseTicket>,
}

impl RegisteredFruit {
    fn held(spec: TreeFruitSpec, required_bricks: Vec<StaticVoxelBrickId>) -> Self {
        Self {
            spec,
            required_bricks,
            phase: FruitSweepPhase::Armed,
            body: None,
            handoff: None,
        }
    }
}

// Per-connection Russian roulette: each eligible apple gets a fresh independent
// draw. Losing a draw changes no gameplay state; there is no quota/minimum winner.
fn request_fruit_kick(
    fruits: &mut BTreeMap<u64, RegisteredFruit>,
    cycle: f32,
    chance_scale: f32,
    mut draw: impl FnMut() -> f32,
) -> usize {
    if !chance_scale.is_finite() || chance_scale <= 0. {
        return 0;
    }
    let mut released = 0;
    for fruit in fruits.values_mut() {
        if fruit.phase != FruitSweepPhase::Armed || cycle < fruit.spec.full_growth_phase {
            continue;
        }
        let threshold = 1. - chance_scale.min(1.) * (1. - fruit.spec.connection_strength);
        let roll = draw();
        if roll.is_finite() && (0. ..=1.).contains(&roll) && roll > threshold {
            fruit.phase = FruitSweepPhase::PendingDrop;
            released += 1;
        }
    }
    released
}

pub(super) struct TreeFruitPublicationCheckpoint {
    tree_id: u32,
    previous: Option<BTreeMap<u64, RegisteredFruit>>,
    attached_refresh_was_pending: bool,
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

pub(super) struct PreparedTreeCollisionSurface {
    positions: Vec<Vec3>,
    topology: Option<Arc<[[u32; 3]]>>,
    geometry: re_flora_physics::PreparedDeformingGeometry,
    cells: Vec<IVec3>,
    revision: Option<u32>,
}

pub(super) struct TerrainPhysics {
    pub(super) collision_world: CollisionWorld,
    tree_surface_revision: Option<u32>,
    tree_surface_topology: Option<Arc<[[u32; 3]]>>,
    tree_surface_positions: Vec<Vec3>,
    dirty_terrain_bricks: DirtyTerrainBrickQueue,
    imported_terrain_bricks: HashSet<StaticVoxelBrickId>,
    failed_terrain_bricks: HashSet<StaticVoxelBrickId>,
    world_collider_import: Option<WorldTerrainColliderImport>,
    fruits_by_tree: BTreeMap<u32, BTreeMap<u64, RegisteredFruit>>,
    attached_fruit_refresh_trees: HashSet<u32>,
    fruit_cycle: f32,
    fruit_kick_rng: SmallRng,
    apple_size_scale: f32,
    ground_trace: Option<FruitGroundTrace>,
}

impl TerrainPhysics {
    pub(super) fn publish_fixed_scene(
        &mut self,
        mesh: &crate::tracer::StaticSceneMesh,
    ) -> anyhow::Result<()> {
        let (positions, triangles) = mesh.collision_geometry();
        let positions: Vec<_> = positions
            .into_iter()
            .map(|p| p * VOXELS_PER_WORLD_UNIT)
            .collect();
        self.collision_world
            .set_fixed_scene_surface(&positions, &triangles)
            .map_err(anyhow::Error::msg)
    }

    /// Test-only broad phase over committed trunk collision triangles. Triangle IDs belong
    /// to that compact topology, not the independent full visual mesh. Padding is read-only.
    #[cfg(test)]
    pub(super) fn tree_ray_candidates(
        &self,
        origin: Vec3,
        direction: Vec3,
        margin: Vec3,
    ) -> Vec<u32> {
        self.collision_world.deforming_ray_candidates_with_margin(
            origin * VOXELS_PER_WORLD_UNIT,
            direction,
            margin * VOXELS_PER_WORLD_UNIT,
        )
    }

    pub(super) fn prepare_tree_collision_surface(
        revision: u32,
        mesh: &crate::tracer::RasterTreeMesh,
    ) -> anyhow::Result<PreparedTreeCollisionSurface> {
        // Build only at geometry publication. In particular, never seed collision
        // with retained trees' current wind poses during placement/restoration.
        let (positions, indices) = mesh.rest_collision_geometry()?;
        let positions: Vec<_> = positions
            .into_iter()
            .map(|p| p * VOXELS_PER_WORLD_UNIT)
            .collect();
        let topology = (!indices.is_empty()).then_some(indices);
        let triangles = topology.as_deref().unwrap_or(&[]);
        let geometry = CollisionWorld::prepare_deforming_geometry(
            re_flora_physics::DeformingGeometry::Triangles {
                positions: &positions,
                indices: triangles,
            },
        )
        .map_err(anyhow::Error::msg)?;
        let cells = mesh
            .solid_cells
            .iter()
            .map(|&p| UVec3::from(p).as_ivec3())
            .collect::<Vec<_>>();
        // Exclude ALL tree wood from terrain collision, not just the trunk subset.
        // Side-only/cut-trunk publications still own exclusions and must be clearable.
        let has_publication = topology.is_some() || !cells.is_empty();
        Ok(PreparedTreeCollisionSurface {
            positions,
            revision: has_publication.then_some(revision),
            topology,
            geometry,
            cells,
        })
    }

    pub(super) fn commit_tree_collision_surface(&mut self, prepared: PreparedTreeCollisionSurface) {
        self.collision_world
            .set_deforming_surface_exclusions(prepared.cells);
        self.collision_world
            .publish_prepared_deforming_geometry(prepared.geometry);
        self.tree_surface_revision = prepared.revision;
        self.tree_surface_topology = prepared.topology;
        self.tree_surface_positions = prepared.positions;
    }

    /// Committed-terrain failure falls back to authoritative voxels. Ordinary
    /// wind frames and wind toggles must never clear/refit the rest collider.
    pub(super) fn clear_tree_collision_surface(&mut self) -> anyhow::Result<()> {
        if self.tree_surface_revision.is_some() {
            self.collision_world
                .set_deforming_surface(&[], &[])
                .map_err(anyhow::Error::msg)?;
            self.collision_world.set_deforming_surface_exclusions([]);
            self.tree_surface_revision = None;
            self.tree_surface_topology = None;
            self.tree_surface_positions.clear();
        }
        Ok(())
    }

    pub(super) fn validate_tree_rest_collision(
        &self,
        mesh: &crate::tracer::RasterTreeMesh,
    ) -> anyhow::Result<()> {
        let (positions, triangles) = mesh.rest_collision_geometry()?;
        anyhow::ensure!(
            self.tree_surface_positions.len() == positions.len()
                && self
                    .tree_surface_positions
                    .iter()
                    .zip(positions)
                    .all(|(old, rest)| *old == rest * VOXELS_PER_WORLD_UNIT)
                && self.tree_surface_topology.as_deref().unwrap_or(&[]) == triangles.as_ref(),
            "tree collision differs from immutable rest geometry"
        );
        log::info!(
            "[TREE][REST_COLLISION] verified=true wind_independent=true trunk_only=true points={} triangles={}",
            self.tree_surface_positions.len(),
            triangles.len()
        );
        Ok(())
    }

    pub(super) fn tree_fruit_publication_checkpoint(
        &self,
        tree_id: u32,
    ) -> TreeFruitPublicationCheckpoint {
        TreeFruitPublicationCheckpoint {
            tree_id,
            previous: self.fruits_by_tree.get(&tree_id).cloned(),
            attached_refresh_was_pending: self.attached_fruit_refresh_trees.contains(&tree_id),
        }
    }

    pub(super) fn new(fruit_cycle: f32) -> Self {
        let mut collision_world = CollisionWorld::new();
        collision_world
            .set_gravity(DYNAMIC_FRUIT_GRAVITY_VOXELS)
            .expect("dynamic fruit gravity constant must be valid");
        Self {
            collision_world,
            tree_surface_revision: None,
            tree_surface_topology: None,
            tree_surface_positions: Vec::new(),
            dirty_terrain_bricks: DirtyTerrainBrickQueue::default(),
            imported_terrain_bricks: HashSet::new(),
            failed_terrain_bricks: HashSet::new(),
            world_collider_import: None,
            fruits_by_tree: BTreeMap::new(),
            attached_fruit_refresh_trees: HashSet::new(),
            fruit_cycle: fruit_cycle.clamp(0.0, 1.0),
            fruit_kick_rng: SmallRng::from_rng(&mut rand::rng()),
            apple_size_scale: 1.0,
            ground_trace: FruitGroundTrace::from_env(),
        }
    }

    pub(super) fn recover_walk_entry(
        &mut self,
        eye: Vec3,
        height: f32,
    ) -> anyhow::Result<Option<Vec3>> {
        Ok(
            walk_recovery::recover(&mut self.collision_world, eye - Vec3::Y * height)?
                .map(|feet| feet + Vec3::Y * height),
        )
    }

    pub(super) fn move_player_capsule(
        &mut self,
        request: PlayerWalkMovementRequest,
        frame_delta_time: f32,
    ) -> anyhow::Result<PlayerWalkMovementResult> {
        self.move_player_capsule_with_ground_detail(request, frame_delta_time, 0.0)
    }

    pub(super) fn move_player_capsule_with_ground_detail(
        &mut self,
        request: PlayerWalkMovementRequest,
        frame_delta_time: f32,
        max_ground_detail_height_voxels: f32,
    ) -> anyhow::Result<PlayerWalkMovementResult> {
        let movement = surface_motion::PLAYER
            .move_by_with_ground_detail(
                &mut self.collision_world,
                request.camera_position - Vec3::Y * request.camera_height,
                request.desired_translation,
                frame_delta_time,
                max_ground_detail_height_voxels,
            )
            .context("moving the player capsule through terrain")?;
        Ok(PlayerWalkMovementResult {
            translation: movement.translation,
            grounded: movement.grounded,
        })
    }

    pub(super) fn pick_walkable_surface(
        &mut self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
    ) -> Option<Vec3> {
        let hit = self.collision_world.cast_character_surface_ray(
            origin * VOXELS_PER_WORLD_UNIT,
            direction,
            max_distance * VOXELS_PER_WORLD_UNIT,
        )?;
        (hit.normal.y >= re_flora_physics::CAPSULE_CHARACTER_GROUND_NORMAL_MIN_DOT)
            .then_some(hit.position / VOXELS_PER_WORLD_UNIT)
    }

    pub(super) fn place_mower_on_surface(&mut self, surface: Vec3) -> anyhow::Result<Option<Vec3>> {
        surface_motion::MOWER
            .place(&mut self.collision_world, surface)
            .context("placing mower on the character collision world")
    }

    pub(super) fn move_mower_on_surface(
        &mut self,
        feet: Vec3,
        translation: Vec3,
        dt: f32,
    ) -> anyhow::Result<Option<Vec3>> {
        surface_motion::MOWER
            .follow(&mut self.collision_world, feet, translation, dt)
            .context("moving mower through the character collision world")
    }

    pub(super) fn mower_support_frame(
        &mut self,
        feet: Vec3,
        rotation: glam::Quat,
    ) -> Option<(Vec3, Vec3)> {
        surface_motion::mower_support(&mut self.collision_world, feet, rotation)
            .map(|frame| (frame.position, frame.normal))
    }

    pub(super) fn set_apple_size_scale(
        &mut self,
        requested: f32,
        tracer: &mut Tracer,
    ) -> anyhow::Result<()> {
        let scale = crate::flora::apple::normalize_size_scale(requested);
        if scale == self.apple_size_scale {
            return Ok(());
        }
        let bodies: Vec<_> = self
            .fruits_by_tree
            .values()
            .flat_map(BTreeMap::values)
            .filter_map(|fruit| fruit.body)
            .collect();
        // A growing collider must not cross into unimported terrain. Until its
        // new footprint is ready, retain the old fallen-fruit render/collision scale.
        let missing: Vec<_> = bodies
            .iter()
            .filter(|_| scale > self.apple_size_scale)
            .filter_map(|&body| self.collision_world.dynamic_body_state(body))
            .flat_map(|state| {
                fruit_drop_path_bricks(state.position.max(Vec3::ZERO).as_uvec3(), scale)
            })
            .filter(|brick| !self.imported_terrain_bricks.contains(brick))
            .collect();
        if !missing.is_empty() {
            for brick in missing {
                self.dirty_terrain_bricks.push(brick);
            }
            return Ok(());
        }
        let desc = scaled_apple_dynamic_body_desc(Vec3::ZERO, Vec3::ZERO, Vec3::ZERO, scale);
        for &body in &bodies {
            self.collision_world.update_dynamic_body_collider(
                body,
                &desc.collider,
                desc.contact_skin,
            )?;
        }
        self.apple_size_scale = scale;
        let mut needed = Vec::new();
        for fruits in self.fruits_by_tree.values_mut() {
            for fruit in fruits.values_mut() {
                fruit.required_bricks = fruit_drop_path_bricks(fruit.spec.position_voxels, scale);
                needed.extend_from_slice(&fruit.required_bricks);
            }
        }
        for brick in needed {
            if !self.imported_terrain_bricks.contains(&brick) {
                self.dirty_terrain_bricks.push(brick);
            }
        }
        log::info!(
            "[APPLE_SIZE] scale={scale} live_bodies={} collider=scaled_convex_hull shadows=scaled",
            bodies.len()
        );
        self.sync_dynamic_fruit_rendering(tracer)
    }

    pub(super) fn take_apple_landings(&mut self) -> Vec<crate::audio::AppleImpact> {
        self.collision_world
            .take_dynamic_body_landings()
            .into_iter()
            .filter(|event| {
                self.fruits_by_tree
                    .values()
                    .flat_map(BTreeMap::values)
                    .any(|fruit| fruit.body == Some(event.body))
            })
            .map(|event| crate::audio::AppleImpact {
                body_id: event.body.get(),
                position: event.position / VOXELS_PER_WORLD_UNIT,
            })
            .collect()
    }

    pub(super) fn advance_dynamic_bodies(
        &mut self,
        frame_delta_time: f32,
        tracer: &mut Tracer,
        apple_size_scale: f32,
        perf_frame: Option<u64>,
    ) -> anyhow::Result<()> {
        let started = perf_frame.map(|_| Instant::now());
        self.set_apple_size_scale(apple_size_scale, tracer)?;
        if let Some(mut trace) = self.ground_trace.take() {
            trace.prepare(self, tracer)?;
            self.ground_trace = Some(trace);
        }
        self.spawn_pending_fruits(tracer)?;
        let has_fruit_bodies = self
            .fruits_by_tree
            .values()
            .flat_map(BTreeMap::values)
            .any(|fruit| fruit.body.is_some());
        let setup_us = started.map_or(0., |s| s.elapsed().as_secs_f64() * 1e6);
        if !has_fruit_bodies && self.collision_world.rope_count() == 0 {
            let result = self.sync_dynamic_fruit_rendering(tracer);
            if let Some(frame) = perf_frame {
                log::info!("[PERF][FRUIT_UPDATE] frame={frame} bodies=0 steps=0 setup_us={setup_us:.2} step_us=0 maintenance_us=0 render_us={:.2}", started.unwrap().elapsed().as_secs_f64()*1e6 - setup_us);
            }
            return result;
        }
        let step_started = perf_frame.map(|_| Instant::now());
        let step = self.collision_world.advance(frame_delta_time);
        let step_us = step_started.map_or(0., |s| s.elapsed().as_secs_f64() * 1e6);
        let maintenance_started = perf_frame.map(|_| Instant::now());
        if let Some(trace) = &mut self.ground_trace {
            trace.stepped(frame_delta_time, step);
        }
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
        let maintenance_us = maintenance_started.map_or(0., |s| s.elapsed().as_secs_f64() * 1e6);
        let render_started = perf_frame.map(|_| Instant::now());
        let result = self
            .sync_dynamic_fruit_rendering(tracer)
            .context("updating dynamic fruit geometry");
        if let Some(frame) = perf_frame {
            let bodies = self
                .fruits_by_tree
                .values()
                .flat_map(BTreeMap::values)
                .filter(|fruit| fruit.body.is_some())
                .count();
            log::info!("[PERF][FRUIT_UPDATE] frame={frame} bodies={bodies} steps={} setup_us={setup_us:.2} step_us={step_us:.2} maintenance_us={maintenance_us:.2} render_us={:.2} dropped_us={:.2}",
                step.steps, render_started.unwrap().elapsed().as_secs_f64()*1e6, step.dropped_seconds*1e6);
        }
        result
    }

    pub(super) fn publish_tree_fruits(
        &mut self,
        tree_id: u32,
        specs: Vec<TreeFruitSpec>,
        tracer: &mut Tracer,
    ) -> anyhow::Result<()> {
        let mut previous = self.fruits_by_tree.remove(&tree_id).unwrap_or_default();
        let mut registered = BTreeMap::new();
        for spec in specs {
            let required_bricks =
                fruit_drop_path_bricks(spec.position_voxels, self.apple_size_scale);
            for &brick in &required_bricks {
                if !self.imported_terrain_bricks.contains(&brick)
                    && !self.failed_terrain_bricks.contains(&brick)
                {
                    self.mark_terrain_brick_dirty(brick);
                }
            }
            let fruit = if let Some(mut existing) = previous.remove(&spec.id) {
                if existing.spec != spec && existing.phase == FruitSweepPhase::PendingDrop {
                    existing.phase = FruitSweepPhase::Armed;
                    existing.handoff = None;
                }
                existing.spec = spec;
                existing.required_bricks = required_bricks;
                existing
            } else {
                RegisteredFruit::held(spec, required_bricks)
            };
            registered.insert(fruit.spec.id, fruit);
        }
        self.fruits_by_tree.insert(tree_id, registered);
        self.spawn_pending_fruits(tracer)?;
        self.sync_dynamic_fruit_rendering(tracer)
    }

    /// Restore a held crop, never releases based on a saved growth value.
    /// Loose bodies are transient. The caller excludes persistent harvest receipts.
    pub(super) fn restore_saved_tree_fruits(
        &mut self,
        tree_id: u32,
        specs: Vec<TreeFruitSpec>,
        tracer: &mut Tracer,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.fruits_by_tree.contains_key(&tree_id),
            "saved tree must replace old fruit first"
        );
        self.fruits_by_tree.insert(
            tree_id,
            specs
                .iter()
                .map(|spec| (spec.id, RegisteredFruit::held(spec.clone(), Vec::new())))
                .collect(),
        );
        self.publish_tree_fruits(tree_id, specs, tracer)
    }

    pub(super) fn unpublish_tree_fruits(
        &mut self,
        tree_id: u32,
        tracer: &mut Tracer,
    ) -> anyhow::Result<()> {
        self.fruits_by_tree.remove(&tree_id);
        self.sync_dynamic_fruit_rendering(tracer)
    }

    pub(super) fn commit_tree_fruit_publication(
        &mut self,
        checkpoint: TreeFruitPublicationCheckpoint,
    ) {
        let retained_bodies = self
            .fruits_by_tree
            .get(&checkpoint.tree_id)
            .into_iter()
            .flat_map(BTreeMap::values)
            .filter_map(|fruit| fruit.body)
            .collect::<HashSet<_>>();
        for body in checkpoint
            .previous
            .into_iter()
            .flat_map(BTreeMap::into_values)
            .filter_map(|fruit| fruit.body)
        {
            if !retained_bodies.contains(&body) {
                self.collision_world.remove_dynamic_body(body);
            }
        }
    }

    pub(super) fn restore_tree_fruit_publication(
        &mut self,
        checkpoint: TreeFruitPublicationCheckpoint,
        tracer: &mut Tracer,
    ) -> anyhow::Result<()> {
        self.restore_tree_fruit_publication_state(checkpoint);
        self.sync_dynamic_fruit_rendering(tracer)
    }

    fn restore_tree_fruit_publication_state(&mut self, checkpoint: TreeFruitPublicationCheckpoint) {
        let previous_bodies = checkpoint
            .previous
            .as_ref()
            .into_iter()
            .flat_map(BTreeMap::values)
            .filter_map(|fruit| fruit.body)
            .collect::<HashSet<_>>();
        if let Some(current) = self.fruits_by_tree.remove(&checkpoint.tree_id) {
            for body in current.into_values().filter_map(|fruit| fruit.body) {
                if !previous_bodies.contains(&body) {
                    self.collision_world.remove_dynamic_body(body);
                }
            }
        }
        if let Some(previous) = checkpoint.previous {
            self.fruits_by_tree.insert(checkpoint.tree_id, previous);
        }
        if checkpoint.attached_refresh_was_pending {
            self.attached_fruit_refresh_trees.insert(checkpoint.tree_id);
        } else {
            self.attached_fruit_refresh_trees
                .remove(&checkpoint.tree_id);
        }
    }

    /// Snapshot restoration first unpublishes all old tree fruits while their bodies
    /// remain checkpointed. Change only the season metadata; do not delete those bodies.
    pub(super) fn set_restored_tree_fruit_cycle(&mut self, cycle: f32) -> anyhow::Result<f32> {
        anyhow::ensure!(
            self.fruits_by_tree.is_empty() && cycle.is_finite() && (0. ..=1.).contains(&cycle),
            "saved fruit cycle requires an unpublished, valid forest"
        );
        let previous = self.fruit_cycle;
        self.fruit_cycle = cycle;
        Ok(previous)
    }

    pub(super) fn restore_tree_fruit_cycle_metadata(&mut self, previous: f32) {
        self.fruit_cycle = previous;
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
                        fruit.spec.grow_start_phase,
                    );
                    if next_phase == FruitSweepPhase::Armed {
                        if let Some(body) = fruit.body.take() {
                            self.collision_world.remove_dynamic_body(body);
                        }
                    }
                    if fruit.phase != next_phase {
                        fruit.handoff = None;
                    }
                    fruit.phase = next_phase;
                }
            }
        }
        self.fruit_cycle = fruit_cycle;
        if fruit_cycle != previous_cycle {
            self.attached_fruit_refresh_trees
                .extend(self.fruits_by_tree.keys().copied());
        }
        self.spawn_pending_fruits(tracer)?;
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
                    // Only a kick requests release. Keep it attached until its collider path
                    // and frame-owned GPU-to-physics pose handoff are both ready.
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

    /// Opt-in visual-fixture focus; no mutation of fruit or physics state.
    pub(super) fn attached_apple_review_center(
        &self,
        tracer: &Tracer,
    ) -> anyhow::Result<Option<Vec3>> {
        let Some(fruit) = self
            .fruits_by_tree
            .values()
            .flat_map(BTreeMap::values)
            .filter(|fruit| fruit.phase == FruitSweepPhase::Armed)
            .max_by_key(|fruit| fruit.spec.position_voxels.z)
        else {
            return Ok(None);
        };
        let root = fruit.spec.position_voxels;
        let poses = tracer.attached_fruit_handoff(&[root])?;
        Ok(poses
            .first()
            .map(|(offset, _)| (root.as_vec3() + *offset) / VOXELS_PER_WORLD_UNIT))
    }

    pub(super) fn request_tree_fruit_kick(&mut self, tree: u32, chance_scale: f32) -> usize {
        let rng = &mut self.fruit_kick_rng;
        self.fruits_by_tree.get_mut(&tree).map_or(0, |fruits| {
            request_fruit_kick(fruits, self.fruit_cycle, chance_scale, || rng.random())
        })
    }

    /// Only the explicit hidden fixture pins luck; live kicks use the resident RNG.
    pub(super) fn seed_fruit_kicks_for_diagnostic(&mut self, seed: u64) {
        self.fruit_kick_rng = SmallRng::seed_from_u64(seed);
    }

    /// Explicit opt-in drop fixtures use the same release policy, not growth-threshold tricks.
    pub(super) fn request_diagnostic_fruit_kicks(&mut self) {
        for fruits in self.fruits_by_tree.values_mut() {
            request_fruit_kick(fruits, self.fruit_cycle, 1., || 1.);
        }
    }

    pub(super) fn tree_fruit_counts(&self, tree: u32) -> (usize, usize, usize) {
        let Some(fruits) = self.fruits_by_tree.get(&tree) else {
            return (0, 0, 0);
        };
        (
            fruits.len(),
            fruits
                .values()
                .filter(|f| f.phase == FruitSweepPhase::Armed)
                .count(),
            fruits
                .values()
                .filter(|f| f.phase == FruitSweepPhase::Dropped)
                .count(),
        )
    }

    fn spawn_pending_fruits(&mut self, tracer: &mut Tracer) -> anyhow::Result<()> {
        let mut completed = Vec::new();
        for (ticket, pose) in tracer.poll_fruit_releases()? {
            for (&tree, fruits) in &mut self.fruits_by_tree {
                for (&id, fruit) in fruits {
                    if fruit.handoff != Some(ticket) {
                        continue;
                    }
                    fruit.handoff = None;
                    if fruit.phase != FruitSweepPhase::PendingDrop {
                        continue;
                    }
                    if let Some(pose) = pose {
                        completed.push((tree, id, fruit.spec.clone(), pose));
                    } else {
                        fruit.phase = FruitSweepPhase::Armed;
                    }
                }
            }
        }
        let imported_terrain_bricks = &self.imported_terrain_bricks;
        let ready = self
            .fruits_by_tree
            .iter()
            .flat_map(|(&tree_id, fruits)| {
                fruits.iter().filter_map(move |(&fruit_id, fruit)| {
                    (fruit.phase == FruitSweepPhase::PendingDrop
                        && fruit.handoff.is_none()
                        && fruit.body.is_none()
                        && fruit
                            .required_bricks
                            .iter()
                            .all(|brick| imported_terrain_bricks.contains(brick)))
                    .then_some((tree_id, fruit_id, fruit.spec.clone()))
                })
            })
            .collect::<Vec<_>>();

        // Register completed tickets before re-admitting unready requests. All copies are
        // recorded in ordinary rendering; this path never submits or waits for a GPU job.
        let completing: HashSet<_> = completed
            .iter()
            .map(|(tree, fruit, _, _)| (*tree, *fruit))
            .collect();
        for (tree, id, spec) in ready {
            if completing.contains(&(tree, id)) {
                continue;
            }
            let ticket = tracer.request_fruit_release(spec.position_voxels)?;
            self.fruits_by_tree
                .get_mut(&tree)
                .unwrap()
                .get_mut(&id)
                .unwrap()
                .handoff = ticket;
        }
        for (tree_id, fruit_id, spec, pose) in completed {
            let release_position = spec.position_voxels.as_vec3() + pose.offset;
            let missing: Vec<_> = fruit_drop_path_bricks(
                release_position.max(Vec3::ZERO).as_uvec3(),
                self.apple_size_scale,
            )
            .into_iter()
            .filter(|brick| !self.imported_terrain_bricks.contains(brick))
            .collect();
            if !missing.is_empty() {
                for brick in missing {
                    self.dirty_terrain_bricks.push(brick);
                }
                continue;
            }
            let mut desc = scaled_apple_dynamic_body_desc(
                release_position,
                spec.linear_velocity_voxels + pose.velocity,
                spec.angular_velocity,
                self.apple_size_scale,
            );
            desc.rotation = pose.rotation;
            desc.angular_velocity += pose.angular_velocity;
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

    fn sync_dynamic_fruit_rendering(&mut self, tracer: &mut Tracer) -> anyhow::Result<()> {
        let mut instances = Vec::new();
        for fruit in self.fruits_by_tree.values().flat_map(BTreeMap::values) {
            if let Some(state) = fruit
                .body
                .and_then(|body| self.collision_world.dynamic_body_state(body))
            {
                instances.push(DynamicFruitRenderInstance::new(
                    state.position / VOXELS_PER_WORLD_UNIT,
                    state.rotation,
                    self.apple_size_scale,
                ));
            }
        }
        if let Some(mut trace) = self.ground_trace.take() {
            trace.record(self, &instances)?;
            self.ground_trace = Some(trace);
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

#[cfg(test)]
fn apple_dynamic_body_desc(
    position: Vec3,
    linear_velocity: Vec3,
    angular_velocity: Vec3,
) -> DynamicBodyDesc {
    scaled_apple_dynamic_body_desc(position, linear_velocity, angular_velocity, 1.0)
}

fn scaled_apple_dynamic_body_desc(
    position: Vec3,
    linear_velocity: Vec3,
    angular_velocity: Vec3,
    scale: f32,
) -> DynamicBodyDesc {
    let scale = crate::flora::apple::normalize_size_scale(scale);
    let mut desc = DynamicBodyDesc::sphere(position, 2.0 * scale);
    desc.collider = DynamicColliderShape::ConvexHull {
        points: apple_convex_points()
            .into_iter()
            .map(|point| point * scale)
            .collect(),
    };
    desc.linear_velocity = linear_velocity;
    desc.angular_velocity = angular_velocity;
    desc.friction = APPLE_FRICTION;
    desc.restitution = APPLE_RESTITUTION;
    desc.contact_skin = APPLE_CONTACT_SKIN_VOXELS * scale;
    desc.linear_damping = APPLE_LINEAR_DAMPING;
    desc.angular_damping = APPLE_ANGULAR_DAMPING;
    desc.ccd_enabled = true;
    desc.additional_solver_iterations = APPLE_ADDITIONAL_SOLVER_ITERATIONS;
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

fn fruit_drop_path_bricks(position_voxels: UVec3, scale: f32) -> Vec<StaticVoxelBrickId> {
    // The convex hull includes voxel-cell corners; a nominal radius of two
    // does not cover every rotated corner. Include contact skin as well.
    static COLLISION_RADIUS: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    let radius = COLLISION_RADIUS.get_or_init(|| {
        apple_convex_points()
            .iter()
            .map(|p| p.length())
            .fold(0., f32::max)
    });
    let fruit_radius_voxels = ((*radius + APPLE_CONTACT_SKIN_VOXELS)
        * crate::flora::apple::normalize_size_scale(scale))
    .ceil() as i32;
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

    fn tree_collision_fixture(
        origin: Vec3,
    ) -> (
        crate::tracer::RasterTreeMesh,
        Vec<crate::tree_gen::pose::BranchPose>,
    ) {
        let mut desc = crate::tree_gen::TreeDesc::default();
        desc.branching.iterations = 3;
        let tree = Arc::new(crate::tree_gen::Tree::new(desc));
        let mut mesh = crate::tracer::RasterTreeMesh::default();
        let voxel_origin = (origin * VOXELS_PER_WORLD_UNIT).as_uvec3();
        mesh.append_region(
            voxel_origin,
            UVec3::splat(4),
            &[5; 64],
            &tree.world_trunks(origin),
        )
        .unwrap();
        mesh.finish().unwrap();
        assert!(!mesh.indices.is_empty());
        mesh.bind_tree(7, origin, &tree, None).unwrap();
        mesh.compile_cpu_deformation().unwrap();
        let poses = vec![
            crate::tree_gen::pose::BranchPose {
                rotation: glam::Quat::IDENTITY,
                translation: Vec3::ZERO,
            };
            tree.branches().len()
        ];
        (mesh, poses)
    }

    /// Two actual wood voxels: one on the root axis, one well inside a lateral limb.
    /// Use structural roles/ancestry as the oracle, not species names or skin weights.
    fn trunk_and_limb_fixture(
        species: crate::tree_gen::TreeSpecies,
        axis_model: bool,
        cut_trunk: bool,
    ) -> (crate::tracer::RasterTreeMesh, UVec3, UVec3) {
        use crate::branch_skeleton::BranchSegmentRole;
        use crate::geom::RoundConeClearanceIndex;
        let mut authored = crate::tree_gen::TreeDesc {
            enable_subdivision: false,
            ..Default::default()
        };
        authored.branching.iterations = 3;
        let mut desc = species.mature_desc(&authored);
        desc.axis_model = axis_model;
        let tree = Arc::new(crate::tree_gen::Tree::new(desc));
        let origin = Vec3::new(1., 0.25, 1.);
        let cones = tree.world_trunks(origin);
        let mut main = Vec::new();
        for branch in tree.branches() {
            main.push(
                branch.role == BranchSegmentRole::MainAxis
                    && branch.parent.is_none_or(|parent| main[parent]),
            );
        }
        let root_cones: Vec<_> = cones
            .iter()
            .zip(tree.trunk_branch_indices())
            .filter(|(_, branch)| main[**branch])
            .map(|(cone, _)| cone.clone())
            .collect();
        let root = RoundConeClearanceIndex::new(&root_cones);
        let select = |on_trunk: bool| {
            for (cone, &branch) in cones.iter().zip(tree.trunk_branch_indices()) {
                if main[branch] != on_trunk {
                    continue;
                }
                for t in [0.5, 0.8, 0.2] {
                    let base = cone.center_a().lerp(cone.center_b(), t).as_ivec3();
                    for z in -2..=2 {
                        for y in -2..=2 {
                            for x in -2..=2 {
                                let cell = base + IVec3::new(x, y, z);
                                let center = cell.as_vec3() + Vec3::splat(0.5);
                                if cell.cmpge(IVec3::ZERO).all()
                                    && cone.signed_distance(center) < 0.
                                    && (on_trunk || root.has_minimum_clearance(center, 2.))
                                {
                                    return cell.as_uvec3();
                                }
                            }
                        }
                    }
                }
            }
            panic!("missing fixture wood: species={species:?} axis_model={axis_model} trunk={on_trunk}");
        };
        let trunk = select(true);
        let limb = select(false);
        let min = trunk.min(limb);
        let dim = trunk.max(limb) - min + UVec3::ONE;
        let mut bytes = vec![0; dim.element_product() as usize];
        for cell in [trunk, limb] {
            if cell == trunk && cut_trunk {
                continue;
            }
            let p = cell - min;
            bytes[(p.x + dim.x * (p.y + dim.y * p.z)) as usize] = 5;
        }
        let mut mesh = crate::tracer::RasterTreeMesh::default();
        mesh.append_region(min, dim, &bytes, &cones).unwrap();
        mesh.finish().unwrap();
        mesh.bind_tree(7, origin, &tree, None).unwrap();
        mesh.compile_cpu_deformation().unwrap();
        assert_eq!(
            mesh.indices.len() / 3,
            if cut_trunk { 12 } else { 24 },
            "resident wood must remain visible/pickable"
        );
        (mesh, trunk, limb)
    }

    #[test]
    fn every_tree_architecture_has_trunk_only_rest_collision_not_lateral_wood() {
        use crate::tree_gen::TreeSpecies;
        // New registered species automatically inherit this actual publication test.
        for (species, axis_model) in TreeSpecies::ALL
            .into_iter()
            .flat_map(|species| [false, true].map(|axis_model| (species, axis_model)))
        {
            let (mesh, trunk, limb) = trunk_and_limb_fixture(species, axis_model, false);
            let prepared = TerrainPhysics::prepare_tree_collision_surface(1, &mesh).unwrap();
            assert_eq!(prepared.topology.as_ref().unwrap().len(), 12,
                "{species:?} axis_model={axis_model}: lateral visual wood leaked into physical collision");
            assert!(prepared
                .positions
                .iter()
                .all(|p| p.cmpge(trunk.as_vec3()).all()
                    && p.cmple((trunk + UVec3::ONE).as_vec3()).all()));
            let mut physics = TerrainPhysics::new(0.);
            physics.commit_tree_collision_surface(prepared);
            let ray = |cell: UVec3| cell.as_vec3() + Vec3::splat(0.5) - Vec3::X * 2.;
            assert!(physics
                .collision_world
                .cast_character_surface_ray(ray(trunk), Vec3::X, 3.)
                .is_some());
            assert!(physics
                .collision_world
                .cast_character_surface_ray(ray(limb), Vec3::X, 3.)
                .is_none());
        }
    }

    #[test]
    fn lateral_wood_stays_excluded_even_when_the_entire_trunk_was_cut() {
        for cut_trunk in [false, true] {
            let (mesh, _, limb) =
                trunk_and_limb_fixture(crate::tree_gen::TreeSpecies::AppleTree, true, cut_trunk);
            let mut physics = TerrainPhysics::new(0.);
            let brick = StaticVoxelBrickId((limb / STATIC_VOXEL_BRICK_DIM).as_ivec3());
            let occupancy = BrickOccupancy::from_filled_voxels([limb % STATIC_VOXEL_BRICK_DIM]);
            physics
                .collision_world
                .upsert_static_voxel_brick(brick, 1, occupancy.clone());
            let origin = limb.as_vec3() + Vec3::splat(0.5) - Vec3::X * 2.;
            assert!(physics
                .collision_world
                .cast_character_surface_ray(origin, Vec3::X, 3.)
                .is_some());
            physics.commit_tree_collision_surface(
                TerrainPhysics::prepare_tree_collision_surface(1, &mesh).unwrap(),
            );
            assert_eq!(physics.tree_surface_revision, Some(1));
            if cut_trunk {
                assert!(physics.tree_surface_topology.is_none());
            }
            assert!(physics
                .collision_world
                .cast_character_surface_ray(origin, Vec3::X, 3.)
                .is_none());
            // A later terrain brick import must apply the same exclusion, too.
            physics
                .collision_world
                .upsert_static_voxel_brick(brick, 2, occupancy);
            assert!(physics
                .collision_world
                .cast_character_surface_ray(origin, Vec3::X, 3.)
                .is_none());
            physics.clear_tree_collision_surface().unwrap();
            assert!(physics
                .collision_world
                .cast_character_surface_ray(origin, Vec3::X, 3.)
                .is_some());
            physics.clear_tree_collision_surface().unwrap();
        }
    }

    #[test]
    fn tree_collision_publication_uses_rest_geometry_not_current_wind() {
        let (mesh, rest_poses) = tree_collision_fixture(Vec3::ZERO);
        let rest = mesh.posed_surface(|_| Some(rest_poses.as_slice())).unwrap();
        let wind_poses: Vec<_> = rest_poses
            .iter()
            .map(|p| crate::tree_gen::pose::BranchPose {
                translation: p.translation + Vec3::X * 32.,
                ..*p
            })
            .collect();
        let wind = mesh.posed_surface(|_| Some(wind_poses.as_slice())).unwrap();
        assert!(mesh.max_displacement(&wind) > 0.01);
        let prepared = TerrainPhysics::prepare_tree_collision_surface(1, &mesh).unwrap();
        // Collision compacts only exposed trunk corners; the full diagnostic
        // visual plan may retain additional unused/binding-distinct corners.
        let rest_points: std::collections::BTreeSet<_> = rest
            .positions()
            .map(|p| (p * VOXELS_PER_WORLD_UNIT).to_array().map(f32::to_bits))
            .collect();
        assert!(!prepared.positions.is_empty());
        assert!(prepared
            .positions
            .iter()
            .all(|p| rest_points.contains(&p.to_array().map(f32::to_bits))));
    }

    #[test]
    fn rest_tree_collision_keeps_player_queries_fixed_and_replaces_only_on_geometry_commit() {
        let (mesh, rest_poses) = tree_collision_fixture(Vec3::ZERO);
        let mut physics = TerrainPhysics::new(0.);
        physics.commit_tree_collision_surface(
            TerrainPhysics::prepare_tree_collision_surface(1, &mesh).unwrap(),
        );
        let origin = Vec3::new(-4., 2., 2.);
        let hit = physics
            .collision_world
            .cast_character_surface_ray(origin, Vec3::X, 20.)
            .unwrap();
        assert!(hit.position.x.abs() < 1e-5);
        let movement = physics
            .collision_world
            .move_capsule_character(re_flora_physics::CapsuleCharacterMove {
                center: origin,
                radius: 0.5,
                half_height: 0.5,
                desired_translation: Vec3::X * 10.,
                dt: 1. / 60.,
            })
            .unwrap();
        assert!(movement.translation.x < 4.);
        let original_topology = physics.tree_surface_topology.as_ref().unwrap().clone();
        let (collision_positions, collision_triangles) = mesh.rest_collision_geometry().unwrap();
        for frame in 0..16 {
            let poses: Vec<_> = rest_poses
                .iter()
                .map(|p| crate::tree_gen::pose::BranchPose {
                    translation: Vec3::X * (frame as f32 * 2.),
                    ..*p
                })
                .collect();
            let surface = mesh.posed_surface(|_| Some(poses.as_slice())).unwrap();
            // Query the collision topology, not visual triangle IDs: compact trees can
            // leave part of this synthetic cube outside root-axis wood. Those visual faces
            // intentionally have no physical counterpart. Probe padding out to its full
            // displacement bound without changing the resident collider.
            let margin = surface.query_margin();
            for (id, t) in collision_triangles.iter().enumerate() {
                let rest_points = t.map(|i| collision_positions[i as usize]);
                let points = rest_points.map(|p| p + margin);
                let normal = (points[1] - points[0])
                    .cross(points[2] - points[0])
                    .normalize_or_zero();
                if normal == Vec3::ZERO {
                    continue;
                }
                let ray_origin = (points[0] + points[1] + points[2]) / 3. + normal * 0.01;
                assert!(crate::tree_gen::skin::intersect_surface_triangle(
                    ray_origin,
                    -normal,
                    points,
                    rest_points
                )
                .is_some());
                assert!(physics
                    .tree_ray_candidates(ray_origin, -normal, margin)
                    .contains(&(id as u32)));
            }
            physics.validate_tree_rest_collision(&mesh).unwrap();
            assert!(Arc::ptr_eq(
                &original_topology,
                physics.tree_surface_topology.as_ref().unwrap()
            ));
            physics.collision_world.advance(1. / 60.);
            let current = physics
                .collision_world
                .cast_character_surface_ray(origin, Vec3::X, 20.)
                .unwrap();
            assert_eq!(current.position, hit.position);
        }
        let (replacement, _) = tree_collision_fixture(Vec3::X * (16. / VOXELS_PER_WORLD_UNIT));
        let prepared = TerrainPhysics::prepare_tree_collision_surface(2, &replacement).unwrap();
        // Preparation and failed preparation do not alter the resident collider.
        assert!(!physics
            .tree_ray_candidates(origin / VOXELS_PER_WORLD_UNIT, Vec3::X, Vec3::ZERO)
            .is_empty());
        let mut invalid = crate::tracer::RasterTreeMesh::default();
        invalid.indices.push(0);
        assert!(TerrainPhysics::prepare_tree_collision_surface(3, &invalid).is_err());
        physics.validate_tree_rest_collision(&mesh).unwrap();
        physics.commit_tree_collision_surface(prepared);
        physics.validate_tree_rest_collision(&replacement).unwrap();
        let shifted = physics
            .collision_world
            .cast_character_surface_ray(origin, Vec3::X, 30.)
            .unwrap();
        assert!((shifted.position.x - 16.).abs() < 1e-5);
        physics.commit_tree_collision_surface(
            TerrainPhysics::prepare_tree_collision_surface(
                4,
                &crate::tracer::RasterTreeMesh::default(),
            )
            .unwrap(),
        );
        assert!(physics
            .collision_world
            .cast_character_surface_ray(origin, Vec3::X, 30.)
            .is_none());
        assert!(physics.tree_surface_revision.is_none());
        // Terrain-publication fallback also clears the mesh and restores voxel ownership.
        physics.commit_tree_collision_surface(
            TerrainPhysics::prepare_tree_collision_surface(5, &mesh).unwrap(),
        );
        physics.clear_tree_collision_surface().unwrap();
        physics.clear_tree_collision_surface().unwrap();
        assert!(physics
            .collision_world
            .cast_character_surface_ray(origin, Vec3::X, 30.)
            .is_none());
    }

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
            attachment: None,
            grow_start_phase: 0.55,
            full_growth_phase: 0.70,
            connection_strength: 0.5,
            linear_velocity_voxels: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
        }
    }

    fn registered_fruit(mut spec: TreeFruitSpec, id: u64, body: DynamicBodyId) -> RegisteredFruit {
        spec.id = id;
        RegisteredFruit {
            spec,
            required_bricks: Vec::new(),
            phase: FruitSweepPhase::Dropped,
            body: Some(body),
            handoff: None,
        }
    }

    fn spawn_test_fruit_body(world: &mut CollisionWorld, x: f32) -> DynamicBodyId {
        world
            .spawn_dynamic_body(apple_dynamic_body_desc(
                Vec3::new(x, 96.0, 64.0),
                Vec3::ZERO,
                Vec3::ZERO,
            ))
            .unwrap()
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
            state.sleeping,
            "resting apple did not naturally sleep: {state:?}"
        );
        assert!(
            state.linear_velocity.x.hypot(state.linear_velocity.z) < 0.01,
            "apple still translating across the floor after five seconds: {state:?}",
        );
        assert!(
            state.angular_velocity.length() < 0.01,
            "apple still visibly rotating after five seconds: {state:?}",
        );
        for _ in 0..120 {
            world.advance(1.0 / 120.0);
            assert_eq!(
                world.dynamic_body_state(body).unwrap(),
                state,
                "resting apple position or rotation keeps changing"
            );
        }
    }

    #[test]
    fn apple_size_scales_collision_and_contact_skin_without_changing_motion() {
        let position = Vec3::new(65., 97., 65.);
        let original = scaled_apple_dynamic_body_desc(position, Vec3::X, Vec3::Y, 1.);
        let small = scaled_apple_dynamic_body_desc(position, Vec3::X, Vec3::Y, 0.5);
        let DynamicColliderShape::ConvexHull {
            points: original_points,
        } = original.collider
        else {
            panic!("apple must use convex collision");
        };
        let DynamicColliderShape::ConvexHull {
            points: small_points,
        } = small.collider
        else {
            panic!("apple must use convex collision");
        };
        assert_eq!(
            small_points,
            original_points
                .into_iter()
                .map(|point| point * 0.5)
                .collect::<Vec<_>>()
        );
        assert_eq!(small.contact_skin, original.contact_skin * 0.5);
        assert_eq!(small.position, original.position);
        assert_eq!(small.linear_velocity, original.linear_velocity);
        assert_eq!(small.angular_velocity, original.angular_velocity);
        let original_bricks: HashSet<_> = fruit_drop_path_bricks(UVec3::new(59, 97, 59), 1.)
            .into_iter()
            .collect();
        let large_bricks: HashSet<_> = fruit_drop_path_bricks(UVec3::new(59, 97, 59), 2.)
            .into_iter()
            .collect();
        assert!(large_bricks.is_superset(&original_bricks));
        assert!(large_bricks.len() > original_bricks.len());
    }

    #[test]
    fn fruit_grows_through_discrete_voxel_radius_stages_then_stays_held() {
        let fruit = fruit_spec();
        assert_eq!(fruit.attached_radius_voxels(0.54), None);
        assert_eq!(fruit.attached_radius_voxels(0.55), Some(1));
        assert_eq!(fruit.attached_radius_voxels(0.60), Some(1));
        assert_eq!(fruit.attached_radius_voxels(0.625), Some(2));
        assert_eq!(
            fruit.attached_radius_voxels(0.70),
            Some(TREE_FRUIT_MAX_RADIUS_VOXELS)
        );
        assert_eq!(
            fruit.attached_radius_voxels(0.88),
            Some(TREE_FRUIT_MAX_RADIUS_VOXELS)
        );
        assert_eq!(
            fruit.attached_radius_voxels(1.),
            Some(TREE_FRUIT_MAX_RADIUS_VOXELS)
        );
    }

    #[test]
    fn committed_fruit_publication_retires_only_stale_body_identity() {
        let mut physics = TerrainPhysics::new(0.5);
        let retained = spawn_test_fruit_body(&mut physics.collision_world, 64.0);
        let stale = spawn_test_fruit_body(&mut physics.collision_world, 65.0);
        let incoming = spawn_test_fruit_body(&mut physics.collision_world, 66.0);
        physics.fruits_by_tree.insert(
            5,
            BTreeMap::from([
                (7, registered_fruit(fruit_spec(), 7, retained)),
                (8, registered_fruit(fruit_spec(), 8, stale)),
            ]),
        );
        let checkpoint = physics.tree_fruit_publication_checkpoint(5);
        physics.fruits_by_tree.insert(
            5,
            BTreeMap::from([
                (7, registered_fruit(fruit_spec(), 7, retained)),
                (9, registered_fruit(fruit_spec(), 9, incoming)),
            ]),
        );

        physics.commit_tree_fruit_publication(checkpoint);

        assert!(physics
            .collision_world
            .dynamic_body_state(retained)
            .is_some());
        assert!(physics
            .collision_world
            .dynamic_body_state(incoming)
            .is_some());
        assert!(physics.collision_world.dynamic_body_state(stale).is_none());
    }

    #[test]
    fn rolled_back_fruit_publication_restores_exact_previous_body_identity() {
        let mut physics = TerrainPhysics::new(0.5);
        let retained = spawn_test_fruit_body(&mut physics.collision_world, 64.0);
        let stale = spawn_test_fruit_body(&mut physics.collision_world, 65.0);
        let incoming = spawn_test_fruit_body(&mut physics.collision_world, 66.0);
        physics.fruits_by_tree.insert(
            5,
            BTreeMap::from([
                (7, registered_fruit(fruit_spec(), 7, retained)),
                (8, registered_fruit(fruit_spec(), 8, stale)),
            ]),
        );
        physics.attached_fruit_refresh_trees.insert(5);
        let checkpoint = physics.tree_fruit_publication_checkpoint(5);
        physics.fruits_by_tree.insert(
            5,
            BTreeMap::from([
                (7, registered_fruit(fruit_spec(), 7, retained)),
                (9, registered_fruit(fruit_spec(), 9, incoming)),
            ]),
        );
        physics.attached_fruit_refresh_trees.remove(&5);

        physics.restore_tree_fruit_publication_state(checkpoint);

        let restored = physics.fruits_by_tree.get(&5).unwrap();
        assert_eq!(restored.keys().copied().collect::<Vec<_>>(), vec![7, 8]);
        assert_eq!(restored[&7].body, Some(retained));
        assert_eq!(restored[&8].body, Some(stale));
        assert!(physics
            .collision_world
            .dynamic_body_state(retained)
            .is_some());
        assert!(physics.collision_world.dynamic_body_state(stale).is_some());
        assert!(physics
            .collision_world
            .dynamic_body_state(incoming)
            .is_none());
        assert!(physics.attached_fruit_refresh_trees.contains(&5));
    }

    #[test]
    fn only_kicks_release_mature_fruit_not_growth_or_elapsed_time() {
        assert_eq!(
            fruit_phase_after_cycle_change(FruitSweepPhase::Armed, 0.7, 1., 0.55),
            FruitSweepPhase::Armed
        );
        assert_eq!(
            fruit_spec().attached_radius_voxels(1.),
            Some(TREE_FRUIT_MAX_RADIUS_VOXELS)
        );
        let mut crop: BTreeMap<_, _> = (0..8)
            .map(|id| {
                let mut spec = fruit_spec();
                spec.id = id;
                (id, RegisteredFruit::held(spec, vec![]))
            })
            .collect();
        assert_eq!(
            request_fruit_kick(&mut crop, 0.6, 0.25, || panic!(
                "unripe fruit must not draw"
            )),
            0
        );
        let mut draws = [0.8, 0.9, 0.8, 0.8, 0.9, 0.8, 0.8, 0.8].into_iter();
        assert_eq!(
            request_fruit_kick(&mut crop, 1., 0.25, || draws.next().unwrap()),
            2
        );
        assert_eq!(
            crop.values()
                .filter(|f| f.phase == FruitSweepPhase::PendingDrop)
                .count(),
            2
        );
        assert_eq!(request_fruit_kick(&mut crop, 1., 0.25, || 0.8), 0);
        assert_eq!(request_fruit_kick(&mut crop, 1., 1., || 0.99), 6);
        assert_eq!(
            request_fruit_kick(&mut crop, 1., 1., || panic!(
                "pending fruit cannot draw twice"
            )),
            0
        );
        assert_eq!(
            fruit_phase_after_cycle_change(FruitSweepPhase::Dropped, 1., 0.4, 0.55),
            FruitSweepPhase::Armed
        );
    }

    #[test]
    fn connection_roulette_can_all_fail_and_retries_draw_fresh_without_weakening() {
        let mut crop = BTreeMap::from([(7, RegisteredFruit::held(fruit_spec(), vec![]))]);
        let spec = crop[&7].spec.clone();
        for roll in [0.1, 0.5] {
            assert_eq!(request_fruit_kick(&mut crop, 1., 1., || roll), 0);
            assert_eq!(crop[&7].spec, spec);
            assert_eq!(crop[&7].phase, FruitSweepPhase::Armed);
            assert!(crop[&7].body.is_none() && crop[&7].handoff.is_none());
        }
        assert_eq!(request_fruit_kick(&mut crop, 1., 1., || 0.51), 1);
        assert_eq!(
            request_fruit_kick(&mut crop, 1., 1., || panic!("no repeated detachment")),
            0
        );
        assert_eq!(
            request_fruit_kick(&mut crop, 1., 0., || panic!("disabled roulette")),
            0
        );
    }

    #[test]
    fn weaker_connections_can_win_when_stronger_ones_do_nothing_without_a_quota() {
        let mut weak = fruit_spec();
        weak.id = 1;
        weak.connection_strength = 0.2;
        let mut strong = fruit_spec();
        strong.id = 2;
        strong.connection_strength = 0.8;
        let mut crop = BTreeMap::from([
            (1, RegisteredFruit::held(weak, vec![])),
            (2, RegisteredFruit::held(strong, vec![])),
        ]);
        assert_eq!(request_fruit_kick(&mut crop, 1., 1., || 0.5), 1);
        assert_eq!(crop[&1].phase, FruitSweepPhase::PendingDrop);
        assert_eq!(crop[&2].phase, FruitSweepPhase::Armed);
        let mut draws = 0;
        assert_eq!(
            request_fruit_kick(&mut crop, 1., 0.25, || {
                draws += 1;
                0.94
            }),
            0
        );
        assert_eq!(draws, 1);
        assert_eq!(request_fruit_kick(&mut crop, 1., 0.25, || 0.96), 1);
        // GPU admission stays bounded; it must not turn many independent winners into a quota.
        let mut large: BTreeMap<_, _> = (0..300)
            .map(|id| {
                let mut s = fruit_spec();
                s.id = id;
                (id, RegisteredFruit::held(s, vec![]))
            })
            .collect();
        assert_eq!(request_fruit_kick(&mut large, 1., 1., || 0.99), 300);
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
        let bricks = fruit_drop_path_bricks(UVec3::new(65, 97, 65), 1.0);
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
