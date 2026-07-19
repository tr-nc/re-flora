use glam::{IVec3, Quat, UVec3, Vec3};
use rapier3d::control::{CharacterAutostep, CharacterLength, KinematicCharacterController};
use rapier3d::parry::query::ShapeCastOptions;
#[cfg(test)]
use rapier3d::prelude::{AxisMask, VoxelState};
use rapier3d::prelude::{
    BroadPhaseBvh, ColliderBuilder, ColliderHandle, IVector, PhysicsWorld, Pose, QueryFilter,
    QueryPipeline, RigidBodyBuilder, RigidBodyHandle, Rotation, Shape, ShapeCastHit, SharedShape,
    Vector, Voxels,
};
use std::collections::{HashMap, HashSet};

pub const STATIC_VOXEL_BRICK_DIM: u32 = 32;
pub const DEFAULT_FIXED_STEP_SECONDS: f32 = 1.0 / 120.0;
pub const DEFAULT_MAX_SUBSTEPS: u32 = 8;
pub const CAPSULE_CHARACTER_COLLISION_OFFSET: f32 = 0.1;
pub const CAPSULE_CHARACTER_MAX_STEP_HEIGHT: f32 = 16.05;
pub const CAPSULE_CHARACTER_MIN_STEP_WIDTH: f32 = 0.5;
pub const CAPSULE_CHARACTER_GROUND_CONTACT_DISTANCE: f32 = 1.25;
pub const CAPSULE_CHARACTER_GROUND_SNAP_DISTANCE: f32 = 16.25;
pub const CAPSULE_CHARACTER_MAX_SLOPE_CLIMB_ANGLE: f32 = std::f32::consts::FRAC_PI_3;
pub const CAPSULE_CHARACTER_MIN_SLOPE_SLIDE_ANGLE: f32 = std::f32::consts::FRAC_PI_3;
pub const CAPSULE_CHARACTER_NORMAL_NUDGE_FACTOR: f32 = 1.0e-4;
pub const CAPSULE_CHARACTER_GROUND_NORMAL_MIN_DOT: f32 = 0.5;
const STATIC_VOXEL_BRICK_VOLUME: usize = STATIC_VOXEL_BRICK_DIM as usize
    * STATIC_VOXEL_BRICK_DIM as usize
    * STATIC_VOXEL_BRICK_DIM as usize;
const OCCUPANCY_WORD_COUNT: usize = STATIC_VOXEL_BRICK_VOLUME.div_ceil(u64::BITS as usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StaticVoxelBrickId(pub IVec3);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DynamicBodyId(u64);

impl DynamicBodyId {
    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug)]
pub enum DynamicColliderShape {
    Sphere { radius: f32 },
    ConvexHull { points: Vec<Vec3> },
}

#[derive(Clone, Debug)]
pub struct DynamicBodyDesc {
    pub position: Vec3,
    pub rotation: Quat,
    pub linear_velocity: Vec3,
    pub angular_velocity: Vec3,
    pub collider: DynamicColliderShape,
    pub mass: f32,
    pub friction: f32,
    pub restitution: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub ccd_enabled: bool,
    pub can_sleep: bool,
}

impl DynamicBodyDesc {
    pub fn sphere(position: Vec3, radius: f32) -> Self {
        Self {
            position,
            rotation: Quat::IDENTITY,
            linear_velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
            collider: DynamicColliderShape::Sphere { radius },
            mass: 1.0,
            friction: 0.8,
            restitution: 0.05,
            linear_damping: 0.1,
            angular_damping: 0.1,
            ccd_enabled: true,
            can_sleep: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DynamicBodyState {
    pub position: Vec3,
    pub rotation: Quat,
    pub linear_velocity: Vec3,
    pub angular_velocity: Vec3,
    pub sleeping: bool,
}

/// A single collision encountered while resolving a capsule character movement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CapsuleCharacterCollision {
    /// The terrain surface normal opposing the character movement.
    pub normal: Vec3,
    /// The portion of the requested translation applied before this collision.
    pub translation_applied: Vec3,
    /// The requested translation still unresolved when this collision occurred.
    pub translation_remaining: Vec3,
    /// The distance traveled along the cast direction before impact.
    pub time_of_impact: f32,
}

/// Input for one kinematic capsule movement query, in physics voxel units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CapsuleCharacterMove {
    /// World-space center of the capsule.
    pub center: Vec3,
    /// Radius of both hemispheres and the cylindrical section.
    pub radius: f32,
    /// Half-height of the cylindrical segment, excluding the hemispherical caps.
    pub half_height: f32,
    /// World-space translation the character would like to apply this frame.
    pub desired_translation: Vec3,
    /// Duration of this movement in seconds.
    pub dt: f32,
}

/// Collision-corrected result of one kinematic capsule movement query.
#[derive(Clone, Debug, PartialEq)]
pub struct CapsuleCharacterMoveResult {
    pub translation: Vec3,
    pub grounded: bool,
    pub is_sliding_down_slope: bool,
    pub collisions: Vec<CapsuleCharacterCollision>,
}

#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq)]
pub enum CapsuleCharacterMoveError {
    #[error("capsule character {field} must be finite")]
    NonFinite { field: &'static str },
    #[error("capsule character {field} must be greater than zero, got {value}")]
    NonPositive { field: &'static str, value: f32 },
    #[error("capsule character {field} must be at least zero, got {value}")]
    Negative { field: &'static str, value: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FixedStepResult {
    pub steps: u32,
    pub simulated_seconds: f32,
    pub dropped_seconds: f32,
    pub interpolation_alpha: f32,
}

#[derive(Clone, Debug, thiserror::Error, PartialEq)]
pub enum DynamicBodyError {
    #[error("dynamic body {field} must be finite")]
    NonFinite { field: &'static str },
    #[error("dynamic body {field} must be at least zero, got {value}")]
    Negative { field: &'static str, value: f32 },
    #[error("dynamic body {field} must be greater than zero, got {value}")]
    NonPositive { field: &'static str, value: f32 },
    #[error("dynamic body rotation must have non-zero length")]
    ZeroRotation,
    #[error("convex hull points do not form a three-dimensional convex shape")]
    InvalidConvexHull,
    #[error("dynamic body identifier space is exhausted")]
    IdExhausted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrickOccupancy {
    words: Vec<u64>,
    filled_count: usize,
}

#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum BrickOccupancyError {
    #[error("voxel brick source has {actual} elements, expected {expected}")]
    WrongElementCount { actual: usize, expected: usize },
}

impl BrickOccupancy {
    pub fn empty() -> Self {
        Self {
            words: vec![0; OCCUPANCY_WORD_COUNT],
            filled_count: 0,
        }
    }

    pub fn from_x_fastest_voxel_types(voxel_types: &[u8]) -> Result<Self, BrickOccupancyError> {
        if voxel_types.len() != STATIC_VOXEL_BRICK_VOLUME {
            return Err(BrickOccupancyError::WrongElementCount {
                actual: voxel_types.len(),
                expected: STATIC_VOXEL_BRICK_VOLUME,
            });
        }

        let mut occupancy = Self::empty();
        for (index, voxel_type) in voxel_types.iter().copied().enumerate() {
            if voxel_type != 0 {
                occupancy.set_index(index, true);
            }
        }
        Ok(occupancy)
    }

    pub fn from_filled_voxels(voxels: impl IntoIterator<Item = UVec3>) -> Self {
        let mut occupancy = Self::empty();
        for voxel in voxels {
            assert!(
                voxel.cmplt(UVec3::splat(STATIC_VOXEL_BRICK_DIM)).all(),
                "local collision voxel {voxel:?} is outside a {STATIC_VOXEL_BRICK_DIM}-cubed brick"
            );
            occupancy.set(voxel, true);
        }
        occupancy
    }

    pub fn is_empty(&self) -> bool {
        self.filled_count == 0
    }

    pub fn filled_count(&self) -> usize {
        self.filled_count
    }

    pub fn contains(&self, voxel: UVec3) -> bool {
        local_index(voxel).is_some_and(|index| self.contains_index(index))
    }

    fn set(&mut self, voxel: UVec3, filled: bool) {
        let index = local_index(voxel).expect("local voxel must be inside its collision brick");
        self.set_index(index, filled);
    }

    fn set_index(&mut self, index: usize, filled: bool) {
        let word = index / u64::BITS as usize;
        let mask = 1_u64 << (index % u64::BITS as usize);
        let was_filled = self.words[word] & mask != 0;
        if was_filled == filled {
            return;
        }

        if filled {
            self.words[word] |= mask;
            self.filled_count += 1;
        } else {
            self.words[word] &= !mask;
            self.filled_count -= 1;
        }
    }

    fn contains_index(&self, index: usize) -> bool {
        let word = index / u64::BITS as usize;
        let mask = 1_u64 << (index % u64::BITS as usize);
        self.words[word] & mask != 0
    }

    fn filled_rapier_voxels(&self) -> Vec<IVector> {
        let mut voxels = Vec::with_capacity(self.filled_count);
        for index in 0..STATIC_VOXEL_BRICK_VOLUME {
            if self.contains_index(index) {
                voxels.push(to_rapier_ivec(local_coords(index).as_ivec3()));
            }
        }
        voxels
    }

    fn changes_from(&self, previous: &Self) -> Vec<VoxelChange> {
        let mut changes = Vec::new();
        for (word_index, (&next, &old)) in self.words.iter().zip(&previous.words).enumerate() {
            let mut changed = next ^ old;
            while changed != 0 {
                let bit = changed.trailing_zeros() as usize;
                let index = word_index * u64::BITS as usize + bit;
                if index < STATIC_VOXEL_BRICK_VOLUME {
                    changes.push(VoxelChange {
                        local_voxel: local_coords(index).as_ivec3(),
                    });
                }
                changed &= changed - 1;
            }
        }
        changes
    }

    fn filled_local_bounds(&self) -> Option<(IVec3, IVec3)> {
        let mut mins = IVec3::splat(STATIC_VOXEL_BRICK_DIM as i32);
        let mut maxs = IVec3::ZERO;
        let mut found = false;
        for index in 0..STATIC_VOXEL_BRICK_VOLUME {
            if !self.contains_index(index) {
                continue;
            }
            let voxel = local_coords(index).as_ivec3();
            mins = mins.min(voxel);
            maxs = maxs.max(voxel + IVec3::ONE);
            found = true;
        }
        found.then_some((mins, maxs))
    }
}

impl Default for BrickOccupancy {
    fn default() -> Self {
        Self::empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StaticVoxelBrickUpdate {
    Applied {
        changed_voxels: usize,
        collider_present: bool,
    },
    Unchanged,
    Stale {
        current_revision: u64,
    },
}

struct StaticVoxelBrick {
    occupancy: BrickOccupancy,
    collider: ColliderHandle,
}

#[derive(Clone, Copy)]
struct VoxelChange {
    local_voxel: IVec3,
}

pub struct CollisionWorld {
    physics: PhysicsWorld,
    capsule_character_broad_phase: BroadPhaseBvh,
    capsule_character_modified_colliders: HashSet<ColliderHandle>,
    capsule_character_removed_colliders: HashSet<ColliderHandle>,
    static_bricks: HashMap<StaticVoxelBrickId, StaticVoxelBrick>,
    static_brick_revisions: HashMap<StaticVoxelBrickId, u64>,
    dynamic_bodies: HashMap<DynamicBodyId, RigidBodyHandle>,
    next_dynamic_body_id: u64,
    fixed_step_seconds: f32,
    max_substeps: u32,
    fixed_step_accumulator: f32,
}

impl Default for CollisionWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl CollisionWorld {
    pub fn new() -> Self {
        let mut physics = PhysicsWorld::new();
        physics.integration_parameters.dt = DEFAULT_FIXED_STEP_SECONDS;
        Self {
            physics,
            capsule_character_broad_phase: BroadPhaseBvh::new(),
            capsule_character_modified_colliders: HashSet::new(),
            capsule_character_removed_colliders: HashSet::new(),
            static_bricks: HashMap::new(),
            static_brick_revisions: HashMap::new(),
            dynamic_bodies: HashMap::new(),
            next_dynamic_body_id: 1,
            fixed_step_seconds: DEFAULT_FIXED_STEP_SECONDS,
            max_substeps: DEFAULT_MAX_SUBSTEPS,
            fixed_step_accumulator: 0.0,
        }
    }

    pub fn set_gravity(&mut self, gravity: Vec3) -> Result<(), DynamicBodyError> {
        validate_finite_vec3("gravity", gravity)?;
        self.physics.gravity = to_rapier_vec(gravity);
        Ok(())
    }

    pub fn gravity(&self) -> Vec3 {
        from_rapier_vec(self.physics.gravity)
    }

    pub fn set_fixed_step(
        &mut self,
        fixed_step_seconds: f32,
        max_substeps: u32,
    ) -> Result<(), DynamicBodyError> {
        validate_positive("fixed_step_seconds", fixed_step_seconds)?;
        if max_substeps == 0 {
            return Err(DynamicBodyError::NonPositive {
                field: "max_substeps",
                value: 0.0,
            });
        }
        self.fixed_step_seconds = fixed_step_seconds;
        self.max_substeps = max_substeps;
        self.fixed_step_accumulator = 0.0;
        self.physics.integration_parameters.dt = fixed_step_seconds;
        Ok(())
    }

    pub fn fixed_step_seconds(&self) -> f32 {
        self.fixed_step_seconds
    }

    pub fn dynamic_body_count(&self) -> usize {
        self.dynamic_bodies.len()
    }

    pub fn spawn_dynamic_body(
        &mut self,
        desc: DynamicBodyDesc,
    ) -> Result<DynamicBodyId, DynamicBodyError> {
        let rotation = validate_dynamic_body_desc(&desc)?;
        let shape = build_dynamic_shape(&desc.collider)?;
        let id = DynamicBodyId(self.next_dynamic_body_id);
        self.next_dynamic_body_id = self
            .next_dynamic_body_id
            .checked_add(1)
            .ok_or(DynamicBodyError::IdExhausted)?;

        let body = RigidBodyBuilder::dynamic()
            .pose(Pose::from_parts(to_rapier_vec(desc.position), rotation))
            .linvel(to_rapier_vec(desc.linear_velocity))
            .angvel(to_rapier_vec(desc.angular_velocity))
            .linear_damping(desc.linear_damping)
            .angular_damping(desc.angular_damping)
            .ccd_enabled(desc.ccd_enabled)
            .can_sleep(desc.can_sleep)
            .user_data(id.get() as u128);
        let collider = ColliderBuilder::new(shape)
            .mass(desc.mass)
            .friction(desc.friction)
            .restitution(desc.restitution)
            .user_data(id.get() as u128);
        let (body_handle, _) = self.physics.insert(body, collider);
        self.dynamic_bodies.insert(id, body_handle);
        Ok(id)
    }

    pub fn remove_dynamic_body(&mut self, id: DynamicBodyId) -> bool {
        let Some(handle) = self.dynamic_bodies.remove(&id) else {
            return false;
        };
        self.physics.remove_body(handle).is_some()
    }

    pub fn dynamic_body_state(&self, id: DynamicBodyId) -> Option<DynamicBodyState> {
        let body = &self.physics.bodies[*self.dynamic_bodies.get(&id)?];
        Some(DynamicBodyState {
            position: from_rapier_vec(body.translation()),
            rotation: from_rapier_rotation(*body.rotation()),
            linear_velocity: from_rapier_vec(body.linvel()),
            angular_velocity: from_rapier_vec(body.angvel()),
            sleeping: body.is_sleeping(),
        })
    }

    pub fn advance(&mut self, elapsed_seconds: f32) -> FixedStepResult {
        if !elapsed_seconds.is_finite() || elapsed_seconds <= 0.0 {
            return FixedStepResult {
                steps: 0,
                simulated_seconds: 0.0,
                dropped_seconds: 0.0,
                interpolation_alpha: self.fixed_step_accumulator / self.fixed_step_seconds,
            };
        }

        self.fixed_step_accumulator += elapsed_seconds;
        let max_accumulated = self.fixed_step_seconds * self.max_substeps as f32;
        let dropped_seconds = (self.fixed_step_accumulator - max_accumulated).max(0.0);
        self.fixed_step_accumulator = self.fixed_step_accumulator.min(max_accumulated);

        let mut steps = 0;
        while steps < self.max_substeps && self.fixed_step_accumulator >= self.fixed_step_seconds {
            // Rapier clears collider change flags while stepping. Keep the terrain-only query BVH
            // current first so character queries stay valid even when their next query happens
            // after a dynamic simulation step.
            self.sync_capsule_character_broad_phase();
            self.physics.step();
            self.fixed_step_accumulator =
                (self.fixed_step_accumulator - self.fixed_step_seconds).max(0.0);
            steps += 1;
        }

        FixedStepResult {
            steps,
            simulated_seconds: steps as f32 * self.fixed_step_seconds,
            dropped_seconds,
            interpolation_alpha: (self.fixed_step_accumulator / self.fixed_step_seconds)
                .clamp(0.0, 1.0),
        }
    }

    /// Resolves a classic upright capsule character movement against static voxel terrain.
    ///
    /// This is a query-only kinematic controller: callers own character position and velocity and
    /// apply the returned translation themselves. Dynamic bodies are deliberately absent from the
    /// dedicated terrain query broad-phase, so fruit and other simulated objects do not block the
    /// player.
    pub fn move_capsule_character(
        &mut self,
        movement: CapsuleCharacterMove,
    ) -> Result<CapsuleCharacterMoveResult, CapsuleCharacterMoveError> {
        validate_capsule_character_move(movement)?;
        self.sync_capsule_character_broad_phase();

        let controller = KinematicCharacterController {
            up: Vector::Y,
            offset: CharacterLength::Absolute(CAPSULE_CHARACTER_COLLISION_OFFSET),
            slide: true,
            autostep: Some(CharacterAutostep {
                max_height: CharacterLength::Absolute(CAPSULE_CHARACTER_MAX_STEP_HEIGHT),
                min_width: CharacterLength::Absolute(CAPSULE_CHARACTER_MIN_STEP_WIDTH),
                include_dynamic_bodies: false,
            }),
            // Ground snapping is performed below with an explicit terrain-only shape cast. This
            // also produces reliable grounding on Parry's composite voxel shapes.
            snap_to_ground: None,
            max_slope_climb_angle: CAPSULE_CHARACTER_MAX_SLOPE_CLIMB_ANGLE,
            min_slope_slide_angle: CAPSULE_CHARACTER_MIN_SLOPE_SLIDE_ANGLE,
            normal_nudge_factor: CAPSULE_CHARACTER_NORMAL_NUDGE_FACTOR,
        };
        let shape = SharedShape::capsule_y(movement.half_height, movement.radius);
        let pose = Pose::from_translation(to_rapier_vec(movement.center));
        let query_pipeline = self.capsule_character_broad_phase.as_query_pipeline(
            self.physics.narrow_phase.query_dispatcher(),
            &self.physics.bodies,
            &self.physics.colliders,
            QueryFilter::default().exclude_sensors(),
        );
        let mut desired_translation = to_rapier_vec(movement.desired_translation);
        let grounded_at_start = capsule_ground_hit(
            &query_pipeline,
            shape.as_ref(),
            &pose,
            CAPSULE_CHARACTER_GROUND_CONTACT_DISTANCE,
        )
        .is_some_and(|hit| hit.normal1.y >= CAPSULE_CHARACTER_GROUND_NORMAL_MIN_DOT);
        if grounded_at_start && desired_translation.y < 0.0 {
            // Gravity should not pull an already-grounded character through the controller skin.
            // The post-movement ground cast below supplies normal ground snapping.
            desired_translation.y = 0.0;
        }

        let mut collisions = Vec::new();
        let effective = controller.move_shape(
            movement.dt,
            &query_pipeline,
            shape.as_ref(),
            &pose,
            desired_translation,
            |collision| {
                collisions.push(CapsuleCharacterCollision {
                    normal: from_rapier_vec(collision.hit.normal1),
                    translation_applied: from_rapier_vec(collision.translation_applied),
                    translation_remaining: from_rapier_vec(collision.translation_remaining),
                    time_of_impact: collision.hit.time_of_impact,
                });
            },
        );

        let mut translation = effective.translation;
        let landed_during_move = collisions
            .iter()
            .any(|collision| collision.normal.y >= CAPSULE_CHARACTER_GROUND_NORMAL_MIN_DOT);
        let mut grounded = effective.grounded || landed_during_move;
        if movement.desired_translation.y <= 0.0 && (grounded_at_start || grounded) {
            let final_pose = Pose::from_translation(translation) * pose;
            if let Some(hit) = capsule_ground_hit(
                &query_pipeline,
                shape.as_ref(),
                &final_pose,
                CAPSULE_CHARACTER_GROUND_SNAP_DISTANCE,
            ) {
                if hit.normal1.y >= CAPSULE_CHARACTER_GROUND_NORMAL_MIN_DOT {
                    translation.y -= hit.time_of_impact;
                    grounded = true;
                }
            }
        }

        Ok(CapsuleCharacterMoveResult {
            translation: from_rapier_vec(translation),
            grounded,
            is_sliding_down_slope: effective.is_sliding_down_slope,
            collisions,
        })
    }

    /// Applies pending static-terrain changes to the character-query acceleration structure.
    ///
    /// Character movement calls this automatically. Loading code may call it proactively so a
    /// large terrain import is distributed across loading frames instead of the first player move.
    pub fn sync_capsule_character_queries(&mut self) {
        self.sync_capsule_character_broad_phase();
    }

    pub fn static_brick_count(&self) -> usize {
        self.static_bricks.len()
    }

    pub fn static_brick_revision(&self, id: StaticVoxelBrickId) -> Option<u64> {
        self.static_brick_revisions.get(&id).copied()
    }

    pub fn upsert_static_voxel_brick(
        &mut self,
        id: StaticVoxelBrickId,
        revision: u64,
        occupancy: BrickOccupancy,
    ) -> StaticVoxelBrickUpdate {
        if let Some(current_revision) = self.static_brick_revisions.get(&id).copied() {
            if revision < current_revision {
                return StaticVoxelBrickUpdate::Stale { current_revision };
            }
            if revision == current_revision {
                return StaticVoxelBrickUpdate::Unchanged;
            }
        }

        let update = if let Some(existing) = self.static_bricks.get(&id) {
            let changes = occupancy.changes_from(&existing.occupancy);
            if changes.is_empty() {
                StaticVoxelBrickUpdate::Applied {
                    changed_voxels: 0,
                    collider_present: true,
                }
            } else {
                self.update_existing_brick(id, occupancy, &changes)
            }
        } else if occupancy.is_empty() {
            StaticVoxelBrickUpdate::Applied {
                changed_voxels: 0,
                collider_present: false,
            }
        } else {
            let changed_voxels = occupancy.filled_count();
            self.insert_new_brick(id, occupancy);
            StaticVoxelBrickUpdate::Applied {
                changed_voxels,
                collider_present: true,
            }
        };

        self.static_brick_revisions.insert(id, revision);
        update
    }

    pub fn remove_static_voxel_brick(
        &mut self,
        id: StaticVoxelBrickId,
        revision: u64,
    ) -> StaticVoxelBrickUpdate {
        self.upsert_static_voxel_brick(id, revision, BrickOccupancy::empty())
    }

    fn insert_new_brick(&mut self, id: StaticVoxelBrickId, occupancy: BrickOccupancy) {
        let changed_bounds = occupancy
            .filled_local_bounds()
            .map(|bounds| brick_local_bounds_to_world(id, bounds));
        let mut voxels = fresh_voxels(&occupancy);

        for direction in FACE_NEIGHBORS {
            let neighbor_id = StaticVoxelBrickId(id.0 + direction);
            let Some(neighbor_handle) = self
                .static_bricks
                .get(&neighbor_id)
                .map(|brick| brick.collider)
            else {
                continue;
            };
            let mut neighbor = self.clone_brick_voxels(neighbor_handle);
            voxels.combine_voxel_states(&mut neighbor, brick_origin_shift(direction));
            self.set_brick_voxels(neighbor_handle, neighbor);
        }

        let origin = id.0 * STATIC_VOXEL_BRICK_DIM as i32;
        let collider = self.physics.insert_collider(
            ColliderBuilder::new(SharedShape::new(voxels)).translation(Vector::new(
                origin.x as f32,
                origin.y as f32,
                origin.z as f32,
            )),
            None,
        );
        self.capsule_character_modified_colliders.insert(collider);
        self.static_bricks.insert(
            id,
            StaticVoxelBrick {
                occupancy,
                collider,
            },
        );
        if let Some((mins, maxs)) = changed_bounds {
            self.wake_dynamic_bodies_in_aabb(mins, maxs);
        }
    }

    fn update_existing_brick(
        &mut self,
        id: StaticVoxelBrickId,
        occupancy: BrickOccupancy,
        changes: &[VoxelChange],
    ) -> StaticVoxelBrickUpdate {
        let changed_bounds = voxel_changes_local_bounds(changes)
            .map(|bounds| brick_local_bounds_to_world(id, bounds));
        let collider = self
            .static_bricks
            .get(&id)
            .expect("updated collision brick must exist")
            .collider;

        let collider_present = !occupancy.is_empty();
        if collider_present {
            self.static_bricks
                .get_mut(&id)
                .expect("updated collision brick must exist")
                .occupancy = occupancy;
        } else {
            self.capsule_character_removed_colliders.insert(collider);
            self.capsule_character_modified_colliders.remove(&collider);
            self.physics.remove_collider(collider);
            self.static_bricks.remove(&id);
        }

        // Rebuild fresh voxel shapes instead of mutating Parry's chunk BVH with thousands of
        // `set_voxel` calls. Rebuild neighbors of changed faces too because their boundary states
        // may have been contributed by the previous version of this brick and can therefore need
        // bits cleared as well as added.
        self.rebuild_brick_and_changed_face_neighbors(id, changes);
        if let Some((mins, maxs)) = changed_bounds {
            self.wake_dynamic_bodies_in_aabb(mins, maxs);
        }

        StaticVoxelBrickUpdate::Applied {
            changed_voxels: changes.len(),
            collider_present,
        }
    }

    fn rebuild_brick_and_changed_face_neighbors(
        &mut self,
        changed_id: StaticVoxelBrickId,
        changes: &[VoxelChange],
    ) {
        let mut rebuilt = HashMap::new();
        for id in
            std::iter::once(changed_id).chain(FACE_NEIGHBORS.into_iter().filter_map(|direction| {
                changes
                    .iter()
                    .any(|change| voxel_is_on_face(change.local_voxel, direction))
                    .then_some(StaticVoxelBrickId(changed_id.0 + direction))
            }))
        {
            let Some(brick) = self.static_bricks.get(&id) else {
                continue;
            };
            rebuilt.insert(id, fresh_voxels(&brick.occupancy));
        }

        let rebuilt_ids = rebuilt.keys().copied().collect::<Vec<_>>();
        for id in rebuilt_ids.iter().copied() {
            for direction in FACE_NEIGHBORS {
                let neighbor_id = StaticVoxelBrickId(id.0 + direction);
                if rebuilt.contains_key(&neighbor_id) {
                    if brick_id_precedes(neighbor_id, id) {
                        continue;
                    }

                    let mut voxels = rebuilt
                        .remove(&id)
                        .expect("rebuilt collision brick must exist");
                    let neighbor = rebuilt
                        .get_mut(&neighbor_id)
                        .expect("rebuilt collision neighbor must exist");
                    voxels.combine_voxel_states(neighbor, brick_origin_shift(direction));
                    rebuilt.insert(id, voxels);
                } else {
                    let Some(neighbor_handle) = self
                        .static_bricks
                        .get(&neighbor_id)
                        .map(|brick| brick.collider)
                    else {
                        continue;
                    };
                    let mut neighbor = self.clone_brick_voxels(neighbor_handle);
                    rebuilt
                        .get_mut(&id)
                        .expect("rebuilt collision brick must exist")
                        .combine_voxel_states(&mut neighbor, brick_origin_shift(direction));
                }
            }
        }

        for (id, voxels) in rebuilt {
            let collider = self
                .static_bricks
                .get(&id)
                .expect("rebuilt collision brick must exist")
                .collider;
            self.set_brick_voxels(collider, voxels);
        }
    }

    fn clone_brick_voxels(&self, collider: ColliderHandle) -> Voxels {
        self.physics.colliders[collider]
            .shape()
            .as_voxels()
            .expect("static voxel brick collider must remain a Voxels shape")
            .clone()
    }

    fn set_brick_voxels(&mut self, collider: ColliderHandle, voxels: Voxels) {
        self.physics.colliders[collider].set_shape(SharedShape::new(voxels));
        self.capsule_character_modified_colliders.insert(collider);
    }

    fn sync_capsule_character_broad_phase(&mut self) {
        if self.capsule_character_modified_colliders.is_empty()
            && self.capsule_character_removed_colliders.is_empty()
        {
            return;
        }

        let mut modified = self
            .capsule_character_modified_colliders
            .drain()
            .collect::<Vec<_>>();
        let mut removed = self
            .capsule_character_removed_colliders
            .drain()
            .collect::<Vec<_>>();
        modified.sort_unstable_by_key(|handle| handle.into_raw_parts());
        removed.sort_unstable_by_key(|handle| handle.into_raw_parts());
        let mut events = Vec::new();
        self.capsule_character_broad_phase.update(
            &self.physics.integration_parameters,
            &self.physics.colliders,
            &self.physics.bodies,
            &modified,
            &removed,
            &mut events,
        );
    }

    fn wake_dynamic_bodies_in_aabb(&mut self, mins: Vec3, maxs: Vec3) -> usize {
        let mut handles = Vec::new();
        let prediction_distance = self.physics.integration_parameters.prediction_distance();
        for handle in self.dynamic_bodies.values().copied() {
            let body = &self.physics.bodies[handle];
            let intersects = body.colliders().iter().copied().any(|collider_handle| {
                let aabb = self.physics.colliders[collider_handle]
                    .compute_collision_aabb(prediction_distance);
                aabbs_intersect(
                    from_rapier_vec(aabb.mins),
                    from_rapier_vec(aabb.maxs),
                    mins,
                    maxs,
                )
            });
            if intersects {
                handles.push(handle);
            }
        }
        for handle in handles.iter().copied() {
            self.physics.bodies[handle].wake_up(true);
        }
        handles.len()
    }

    #[cfg(test)]
    fn voxel_state(&self, id: StaticVoxelBrickId, local_voxel: IVec3) -> Option<VoxelState> {
        let brick = self.static_bricks.get(&id)?;
        self.physics.colliders[brick.collider]
            .shape()
            .as_voxels()
            .expect("static voxel brick collider must remain a Voxels shape")
            .voxel_state(to_rapier_ivec(local_voxel))
    }
}

fn capsule_ground_hit(
    queries: &QueryPipeline<'_>,
    shape: &dyn Shape,
    pose: &Pose,
    max_distance: f32,
) -> Option<ShapeCastHit> {
    queries
        .cast_shape(
            pose,
            -Vector::Y,
            shape,
            ShapeCastOptions {
                max_time_of_impact: max_distance,
                target_distance: CAPSULE_CHARACTER_COLLISION_OFFSET,
                stop_at_penetration: false,
                compute_impact_geometry_on_penetration: true,
            },
        )
        .map(|(_, hit)| hit)
}

const FACE_NEIGHBORS: [IVec3; 6] = [
    IVec3::NEG_X,
    IVec3::X,
    IVec3::NEG_Y,
    IVec3::Y,
    IVec3::NEG_Z,
    IVec3::Z,
];

fn local_index(voxel: UVec3) -> Option<usize> {
    if voxel.cmpge(UVec3::splat(STATIC_VOXEL_BRICK_DIM)).any() {
        return None;
    }
    Some(
        ((voxel.z as usize * STATIC_VOXEL_BRICK_DIM as usize + voxel.y as usize)
            * STATIC_VOXEL_BRICK_DIM as usize)
            + voxel.x as usize,
    )
}

fn local_coords(index: usize) -> UVec3 {
    let x = index % STATIC_VOXEL_BRICK_DIM as usize;
    let yz = index / STATIC_VOXEL_BRICK_DIM as usize;
    let y = yz % STATIC_VOXEL_BRICK_DIM as usize;
    let z = yz / STATIC_VOXEL_BRICK_DIM as usize;
    UVec3::new(x as u32, y as u32, z as u32)
}

fn to_rapier_ivec(value: IVec3) -> IVector {
    IVector::new(value.x, value.y, value.z)
}

fn brick_origin_shift(direction: IVec3) -> IVector {
    to_rapier_ivec(direction * STATIC_VOXEL_BRICK_DIM as i32)
}

fn fresh_voxels(occupancy: &BrickOccupancy) -> Voxels {
    Voxels::new(Vector::splat(1.0), &occupancy.filled_rapier_voxels())
}

fn voxel_is_on_face(voxel: IVec3, direction: IVec3) -> bool {
    let max = STATIC_VOXEL_BRICK_DIM as i32 - 1;
    (direction.x < 0 && voxel.x == 0)
        || (direction.x > 0 && voxel.x == max)
        || (direction.y < 0 && voxel.y == 0)
        || (direction.y > 0 && voxel.y == max)
        || (direction.z < 0 && voxel.z == 0)
        || (direction.z > 0 && voxel.z == max)
}

fn brick_id_precedes(left: StaticVoxelBrickId, right: StaticVoxelBrickId) -> bool {
    (left.0.x, left.0.y, left.0.z) < (right.0.x, right.0.y, right.0.z)
}

fn validate_dynamic_body_desc(desc: &DynamicBodyDesc) -> Result<Rotation, DynamicBodyError> {
    validate_finite_vec3("position", desc.position)?;
    validate_finite_vec3("linear_velocity", desc.linear_velocity)?;
    validate_finite_vec3("angular_velocity", desc.angular_velocity)?;
    validate_positive("mass", desc.mass)?;
    validate_nonnegative("friction", desc.friction)?;
    validate_nonnegative("restitution", desc.restitution)?;
    validate_nonnegative("linear_damping", desc.linear_damping)?;
    validate_nonnegative("angular_damping", desc.angular_damping)?;
    if !desc.rotation.is_finite() {
        return Err(DynamicBodyError::NonFinite { field: "rotation" });
    }
    let max_component = desc
        .rotation
        .to_array()
        .into_iter()
        .map(f32::abs)
        .fold(0.0_f32, f32::max);
    if max_component <= f32::MIN_POSITIVE {
        return Err(DynamicBodyError::ZeroRotation);
    }
    let scaled_rotation = Quat::from_xyzw(
        desc.rotation.x / max_component,
        desc.rotation.y / max_component,
        desc.rotation.z / max_component,
        desc.rotation.w / max_component,
    );
    Ok(to_rapier_rotation(scaled_rotation.normalize()))
}

fn validate_capsule_character_move(
    movement: CapsuleCharacterMove,
) -> Result<(), CapsuleCharacterMoveError> {
    if !movement.center.is_finite() {
        return Err(CapsuleCharacterMoveError::NonFinite { field: "center" });
    }
    if !movement.desired_translation.is_finite() {
        return Err(CapsuleCharacterMoveError::NonFinite {
            field: "desired_translation",
        });
    }
    validate_capsule_positive("radius", movement.radius)?;
    validate_capsule_nonnegative("half_height", movement.half_height)?;
    validate_capsule_positive("dt", movement.dt)?;
    Ok(())
}

fn validate_capsule_positive(
    field: &'static str,
    value: f32,
) -> Result<(), CapsuleCharacterMoveError> {
    if !value.is_finite() {
        Err(CapsuleCharacterMoveError::NonFinite { field })
    } else if value <= 0.0 {
        Err(CapsuleCharacterMoveError::NonPositive { field, value })
    } else {
        Ok(())
    }
}

fn validate_capsule_nonnegative(
    field: &'static str,
    value: f32,
) -> Result<(), CapsuleCharacterMoveError> {
    if !value.is_finite() {
        Err(CapsuleCharacterMoveError::NonFinite { field })
    } else if value < 0.0 {
        Err(CapsuleCharacterMoveError::Negative { field, value })
    } else {
        Ok(())
    }
}

fn build_dynamic_shape(shape: &DynamicColliderShape) -> Result<SharedShape, DynamicBodyError> {
    match shape {
        DynamicColliderShape::Sphere { radius } => {
            validate_positive("sphere radius", *radius)?;
            Ok(SharedShape::ball(*radius))
        }
        DynamicColliderShape::ConvexHull { points } => {
            for point in points {
                validate_finite_vec3("convex hull point", *point)?;
            }
            if !points_form_three_dimensional_shape(points) {
                return Err(DynamicBodyError::InvalidConvexHull);
            }
            SharedShape::convex_hull(
                &points
                    .iter()
                    .copied()
                    .map(to_rapier_vec)
                    .collect::<Vec<_>>(),
            )
            .ok_or(DynamicBodyError::InvalidConvexHull)
        }
    }
}

fn points_form_three_dimensional_shape(points: &[Vec3]) -> bool {
    const RELATIVE_EPSILON: f32 = 1.0e-5;
    let Some(&origin) = points.first() else {
        return false;
    };
    let (mins, maxs) = points.iter().copied().fold(
        (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
        |(mins, maxs), point| (mins.min(point), maxs.max(point)),
    );
    let scale = (maxs - mins).length();
    if scale <= f32::MIN_POSITIVE {
        return false;
    }
    let tolerance = scale * RELATIVE_EPSILON;
    let Some(axis) = points
        .iter()
        .copied()
        .map(|point| point - origin)
        .find(|offset| offset.length() > tolerance)
    else {
        return false;
    };
    let Some(normal) = points
        .iter()
        .copied()
        .map(|point| axis.cross(point - origin))
        .find(|normal| normal.length() > tolerance * axis.length())
    else {
        return false;
    };
    let normal = normal.normalize();
    points
        .iter()
        .copied()
        .any(|point| normal.dot(point - origin).abs() > tolerance)
}

fn validate_finite_vec3(field: &'static str, value: Vec3) -> Result<(), DynamicBodyError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(DynamicBodyError::NonFinite { field })
    }
}

fn validate_positive(field: &'static str, value: f32) -> Result<(), DynamicBodyError> {
    if !value.is_finite() {
        Err(DynamicBodyError::NonFinite { field })
    } else if value <= 0.0 {
        Err(DynamicBodyError::NonPositive { field, value })
    } else {
        Ok(())
    }
}

fn validate_nonnegative(field: &'static str, value: f32) -> Result<(), DynamicBodyError> {
    if !value.is_finite() {
        Err(DynamicBodyError::NonFinite { field })
    } else if value < 0.0 {
        Err(DynamicBodyError::Negative { field, value })
    } else {
        Ok(())
    }
}

fn to_rapier_vec(value: Vec3) -> Vector {
    Vector::from_array(value.to_array())
}

fn from_rapier_vec(value: Vector) -> Vec3 {
    Vec3::from_array(value.to_array())
}

fn to_rapier_rotation(value: Quat) -> Rotation {
    Rotation::from_xyzw(value.x, value.y, value.z, value.w)
}

fn from_rapier_rotation(value: Rotation) -> Quat {
    Quat::from_xyzw(value.x, value.y, value.z, value.w)
}

fn voxel_changes_local_bounds(changes: &[VoxelChange]) -> Option<(IVec3, IVec3)> {
    let first = changes.first()?.local_voxel;
    let mut mins = first;
    let mut maxs = first + IVec3::ONE;
    for change in &changes[1..] {
        mins = mins.min(change.local_voxel);
        maxs = maxs.max(change.local_voxel + IVec3::ONE);
    }
    Some((mins, maxs))
}

fn brick_local_bounds_to_world(
    id: StaticVoxelBrickId,
    local_bounds: (IVec3, IVec3),
) -> (Vec3, Vec3) {
    let origin = id.0 * STATIC_VOXEL_BRICK_DIM as i32;
    (
        (origin + local_bounds.0).as_vec3(),
        (origin + local_bounds.1).as_vec3(),
    )
}

fn aabbs_intersect(a_mins: Vec3, a_maxs: Vec3, b_mins: Vec3, b_maxs: Vec3) -> bool {
    a_mins.cmple(b_maxs).all() && b_mins.cmple(a_maxs).all()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_voxel(local: UVec3) -> BrickOccupancy {
        BrickOccupancy::from_filled_voxels([local])
    }

    fn two_layer_floor() -> BrickOccupancy {
        BrickOccupancy::from_filled_voxels((0..2).flat_map(|y| {
            (0..STATIC_VOXEL_BRICK_DIM)
                .flat_map(move |z| (0..STATIC_VOXEL_BRICK_DIM).map(move |x| UVec3::new(x, y, z)))
        }))
    }

    fn occupancy_where(mut filled: impl FnMut(UVec3) -> bool) -> BrickOccupancy {
        let mut occupancy = BrickOccupancy::empty();
        for z in 0..STATIC_VOXEL_BRICK_DIM {
            for y in 0..STATIC_VOXEL_BRICK_DIM {
                for x in 0..STATIC_VOXEL_BRICK_DIM {
                    let voxel = UVec3::new(x, y, z);
                    if filled(voxel) {
                        occupancy.set(voxel, true);
                    }
                }
            }
        }
        occupancy
    }

    fn free_faces(world: &CollisionWorld, id: StaticVoxelBrickId, local: IVec3) -> AxisMask {
        world
            .voxel_state(id, local)
            .expect("test voxel must exist")
            .free_faces()
    }

    #[test]
    fn occupancy_uses_x_fastest_order() {
        let mut voxel_types = vec![0; STATIC_VOXEL_BRICK_VOLUME];
        voxel_types[1] = 7;
        voxel_types[STATIC_VOXEL_BRICK_DIM as usize] = 8;
        let occupancy = BrickOccupancy::from_x_fastest_voxel_types(&voxel_types).unwrap();

        assert_eq!(occupancy.filled_count(), 2);
        assert!(occupancy.contains(UVec3::new(1, 0, 0)));
        assert!(occupancy.contains(UVec3::new(0, 1, 0)));
        assert!(!occupancy.contains(UVec3::new(0, 0, 1)));
    }

    #[test]
    fn rejects_stale_and_duplicate_revisions() {
        let id = StaticVoxelBrickId(IVec3::ZERO);
        let mut world = CollisionWorld::new();
        let filled = one_voxel(UVec3::ZERO);

        assert!(matches!(
            world.upsert_static_voxel_brick(id, 2, filled),
            StaticVoxelBrickUpdate::Applied { .. }
        ));
        assert_eq!(
            world.upsert_static_voxel_brick(id, 2, BrickOccupancy::empty()),
            StaticVoxelBrickUpdate::Unchanged
        );
        assert_eq!(
            world.upsert_static_voxel_brick(id, 1, BrickOccupancy::empty()),
            StaticVoxelBrickUpdate::Stale {
                current_revision: 2
            }
        );
        assert_eq!(world.static_brick_count(), 1);
    }

    #[test]
    fn inserting_adjacent_bricks_hides_the_shared_faces() {
        let left = StaticVoxelBrickId(IVec3::ZERO);
        let right = StaticVoxelBrickId(IVec3::X);
        let mut world = CollisionWorld::new();

        world.upsert_static_voxel_brick(
            left,
            1,
            one_voxel(UVec3::new(STATIC_VOXEL_BRICK_DIM - 1, 0, 0)),
        );
        world.upsert_static_voxel_brick(right, 1, one_voxel(UVec3::ZERO));

        assert!(!free_faces(
            &world,
            left,
            IVec3::new(STATIC_VOXEL_BRICK_DIM as i32 - 1, 0, 0)
        )
        .contains(AxisMask::X_POS));
        assert!(!free_faces(&world, right, IVec3::ZERO).contains(AxisMask::X_NEG));
    }

    #[test]
    fn boundary_removal_and_reinsert_update_the_neighbor() {
        let left = StaticVoxelBrickId(IVec3::ZERO);
        let right = StaticVoxelBrickId(IVec3::X);
        let left_voxel = UVec3::new(STATIC_VOXEL_BRICK_DIM - 1, 0, 0);
        let mut world = CollisionWorld::new();

        world.upsert_static_voxel_brick(left, 1, one_voxel(left_voxel));
        world.upsert_static_voxel_brick(right, 1, one_voxel(UVec3::ZERO));

        assert_eq!(
            world.remove_static_voxel_brick(left, 2),
            StaticVoxelBrickUpdate::Applied {
                changed_voxels: 1,
                collider_present: false,
            }
        );
        assert_eq!(world.static_brick_count(), 1);
        assert!(free_faces(&world, right, IVec3::ZERO).contains(AxisMask::X_NEG));

        world.upsert_static_voxel_brick(left, 3, one_voxel(left_voxel));
        assert_eq!(world.static_brick_count(), 2);
        assert!(!free_faces(&world, right, IVec3::ZERO).contains(AxisMask::X_NEG));
    }

    #[test]
    fn repeated_large_terrain_edits_rebuild_voxel_bvhs() {
        let id = StaticVoxelBrickId(IVec3::ZERO);
        let mut world = CollisionWorld::new();
        let full = occupancy_where(|_| true);
        let left_half = occupancy_where(|voxel| voxel.x < STATIC_VOXEL_BRICK_DIM / 2);
        let right_half = occupancy_where(|voxel| voxel.x >= STATIC_VOXEL_BRICK_DIM / 2);

        world.upsert_static_voxel_brick(id, 1, full.clone());
        let body = world
            .spawn_dynamic_body(DynamicBodyDesc::sphere(
                Vec3::new(16.0, STATIC_VOXEL_BRICK_DIM as f32 + 1.0, 16.0),
                1.0,
            ))
            .unwrap();

        for (revision, occupancy) in [
            left_half,
            right_half,
            one_voxel(UVec3::new(31, 31, 31)),
            full,
            BrickOccupancy::empty(),
        ]
        .into_iter()
        .enumerate()
        {
            assert!(matches!(
                world.upsert_static_voxel_brick(id, revision as u64 + 2, occupancy),
                StaticVoxelBrickUpdate::Applied { .. }
            ));
            assert_eq!(world.advance(DEFAULT_FIXED_STEP_SECONDS).steps, 1);
            assert!(world.dynamic_body_state(body).is_some());
        }

        assert_eq!(world.static_brick_count(), 0);
    }

    #[test]
    fn removing_many_terrain_bricks_in_one_step_keeps_broad_phase_consistent() {
        let mut world = CollisionWorld::new();
        let ids = (0..4)
            .flat_map(|x| (0..4).map(move |z| StaticVoxelBrickId(IVec3::new(x * 2, 0, z * 2))))
            .collect::<Vec<_>>();

        for id in ids.iter().copied() {
            world.upsert_static_voxel_brick(id, 1, one_voxel(UVec3::ZERO));
        }
        world.advance(DEFAULT_FIXED_STEP_SECONDS);
        for id in ids.iter().copied() {
            world.remove_static_voxel_brick(id, 2);
        }

        assert_eq!(world.advance(DEFAULT_FIXED_STEP_SECONDS).steps, 1);
        assert_eq!(world.static_brick_count(), 0);
    }

    #[test]
    fn empty_brick_tombstone_rejects_an_older_reinsert() {
        let id = StaticVoxelBrickId(IVec3::new(4, 5, 6));
        let mut world = CollisionWorld::new();

        world.remove_static_voxel_brick(id, 9);
        assert_eq!(world.static_brick_count(), 0);
        assert_eq!(world.static_brick_revision(id), Some(9));
        assert_eq!(
            world.upsert_static_voxel_brick(id, 8, one_voxel(UVec3::ZERO)),
            StaticVoxelBrickUpdate::Stale {
                current_revision: 9
            }
        );
        assert_eq!(world.static_brick_count(), 0);
    }

    #[test]
    fn fixed_step_accumulates_partial_frames_and_caps_catch_up() {
        let mut world = CollisionWorld::new();
        world.set_gravity(Vec3::ZERO).unwrap();
        let mut desc = DynamicBodyDesc::sphere(Vec3::ZERO, 0.5);
        desc.linear_velocity = Vec3::new(12.0, 0.0, 0.0);
        desc.linear_damping = 0.0;
        desc.angular_damping = 0.0;
        let body = world.spawn_dynamic_body(desc).unwrap();
        let half_step = world.fixed_step_seconds() * 0.5;

        assert_eq!(world.advance(half_step).steps, 0);
        assert_eq!(world.advance(half_step).steps, 1);
        let state = world.dynamic_body_state(body).unwrap();
        assert!((state.position.x - 0.1).abs() < 1.0e-4);

        let capped = world.advance(1.0);
        assert_eq!(capped.steps, DEFAULT_MAX_SUBSTEPS);
        assert!(capped.dropped_seconds > 0.9);
        assert!(capped.interpolation_alpha <= 1.0);
    }

    #[test]
    fn dynamic_body_ids_are_stable_and_removal_is_idempotent() {
        let mut world = CollisionWorld::new();
        let first = world
            .spawn_dynamic_body(DynamicBodyDesc::sphere(Vec3::ZERO, 1.0))
            .unwrap();
        let second = world
            .spawn_dynamic_body(DynamicBodyDesc::sphere(Vec3::X, 1.0))
            .unwrap();

        assert_ne!(first, second);
        assert_eq!(world.dynamic_body_count(), 2);
        assert!(world.remove_dynamic_body(first));
        assert!(!world.remove_dynamic_body(first));
        assert!(world.dynamic_body_state(first).is_none());
        assert!(world.dynamic_body_state(second).is_some());
    }

    #[test]
    fn rejects_flat_convex_hulls() {
        let mut world = CollisionWorld::new();
        let mut desc = DynamicBodyDesc::sphere(Vec3::ZERO, 1.0);
        desc.collider = DynamicColliderShape::ConvexHull {
            points: vec![Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::X + Vec3::Y],
        };

        assert_eq!(
            world.spawn_dynamic_body(desc),
            Err(DynamicBodyError::InvalidConvexHull)
        );
        assert_eq!(world.dynamic_body_count(), 0);
    }

    #[test]
    fn accepts_small_convex_hulls_and_stably_normalizes_large_rotations() {
        let mut world = CollisionWorld::new();
        let scale = 1.0e-3;
        let mut desc = DynamicBodyDesc::sphere(Vec3::ZERO, 1.0);
        desc.collider = DynamicColliderShape::ConvexHull {
            points: vec![
                Vec3::ZERO,
                Vec3::X * scale,
                Vec3::Y * scale,
                Vec3::Z * scale,
            ],
        };
        desc.rotation = Quat::from_xyzw(f32::MAX, f32::MAX, 0.0, f32::MAX);

        let body = world.spawn_dynamic_body(desc).unwrap();
        let rotation = world.dynamic_body_state(body).unwrap().rotation;
        assert!(rotation.is_finite());
        assert!((rotation.length_squared() - 1.0).abs() < 1.0e-5);
    }

    #[test]
    fn dynamic_sphere_rolls_across_a_combined_brick_seam() {
        let mut world = CollisionWorld::new();
        world.upsert_static_voxel_brick(StaticVoxelBrickId(IVec3::ZERO), 1, two_layer_floor());
        world.upsert_static_voxel_brick(StaticVoxelBrickId(IVec3::X), 1, two_layer_floor());
        let mut desc = DynamicBodyDesc::sphere(Vec3::new(20.0, 4.005, 16.0), 2.0);
        desc.linear_velocity = Vec3::new(4.0, 0.0, 0.0);
        desc.angular_velocity = Vec3::new(0.0, 0.0, -2.0);
        desc.linear_damping = 0.0;
        desc.angular_damping = 0.0;
        desc.restitution = 0.0;
        desc.can_sleep = false;
        let body = world.spawn_dynamic_body(desc).unwrap();

        let mut max_abs_vertical_velocity_near_seam = 0.0_f32;
        for _ in 0..720 {
            assert_eq!(world.advance(DEFAULT_FIXED_STEP_SECONDS).steps, 1);
            let state = world.dynamic_body_state(body).unwrap();
            if (28.0..=36.0).contains(&state.position.x) {
                max_abs_vertical_velocity_near_seam =
                    max_abs_vertical_velocity_near_seam.max(state.linear_velocity.y.abs());
            }
        }
        let state = world.dynamic_body_state(body).unwrap();
        assert!(state.position.x > 40.0, "final state: {state:?}");
        assert!(
            max_abs_vertical_velocity_near_seam < 0.02,
            "vertical seam kick: {max_abs_vertical_velocity_near_seam}"
        );
    }

    #[test]
    fn intersecting_terrain_edit_wakes_a_sleeping_body() {
        let brick = StaticVoxelBrickId(IVec3::ZERO);
        let mut world = CollisionWorld::new();
        world.upsert_static_voxel_brick(brick, 1, one_voxel(UVec3::ZERO));
        let body = world
            .spawn_dynamic_body(DynamicBodyDesc::sphere(Vec3::new(0.5, 1.501, 0.5), 0.5))
            .unwrap();
        let handle = world.dynamic_bodies[&body];
        let collider = world.physics.bodies[handle].colliders()[0];
        assert!(world.physics.colliders[collider].compute_aabb().mins.y > 1.0);
        world.physics.bodies[handle].sleep();
        assert!(world.dynamic_body_state(body).unwrap().sleeping);

        world.remove_static_voxel_brick(brick, 2);

        assert!(!world.dynamic_body_state(body).unwrap().sleeping);
    }
}
