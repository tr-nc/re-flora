use glam::{IVec3, UVec3};
use rapier3d::prelude::{
    ColliderBuilder, ColliderHandle, IVector, PhysicsWorld, SharedShape, Vector, Voxels,
};
#[cfg(test)]
use rapier3d::prelude::{AxisMask, VoxelState};
use std::collections::HashMap;

pub const STATIC_VOXEL_BRICK_DIM: u32 = 32;
const STATIC_VOXEL_BRICK_VOLUME: usize =
    STATIC_VOXEL_BRICK_DIM as usize * STATIC_VOXEL_BRICK_DIM as usize * STATIC_VOXEL_BRICK_DIM as usize;
const OCCUPANCY_WORD_COUNT: usize = STATIC_VOXEL_BRICK_VOLUME.div_ceil(u64::BITS as usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StaticVoxelBrickId(pub IVec3);

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

    pub fn from_x_fastest_voxel_types(
        voxel_types: &[u8],
    ) -> Result<Self, BrickOccupancyError> {
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
                        filled: next & (1_u64 << bit) != 0,
                    });
                }
                changed &= changed - 1;
            }
        }
        changes
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
    filled: bool,
}

pub struct CollisionWorld {
    physics: PhysicsWorld,
    static_bricks: HashMap<StaticVoxelBrickId, StaticVoxelBrick>,
    static_brick_revisions: HashMap<StaticVoxelBrickId, u64>,
}

impl Default for CollisionWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl CollisionWorld {
    pub fn new() -> Self {
        Self {
            physics: PhysicsWorld::new(),
            static_bricks: HashMap::new(),
            static_brick_revisions: HashMap::new(),
        }
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
        let mut voxels = Voxels::new(Vector::splat(1.0), &occupancy.filled_rapier_voxels());

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
            ColliderBuilder::new(SharedShape::new(voxels))
                .translation(Vector::new(origin.x as f32, origin.y as f32, origin.z as f32)),
            None,
        );
        self.static_bricks.insert(
            id,
            StaticVoxelBrick {
                occupancy,
                collider,
            },
        );
    }

    fn update_existing_brick(
        &mut self,
        id: StaticVoxelBrickId,
        occupancy: BrickOccupancy,
        changes: &[VoxelChange],
    ) -> StaticVoxelBrickUpdate {
        let collider = self
            .static_bricks
            .get(&id)
            .expect("updated collision brick must exist")
            .collider;
        let mut voxels = self.clone_brick_voxels(collider);
        for change in changes {
            voxels.set_voxel(to_rapier_ivec(change.local_voxel), change.filled);
        }

        for direction in FACE_NEIGHBORS {
            let boundary_changes = changes
                .iter()
                .copied()
                .filter(|change| voxel_is_on_face(change.local_voxel, direction))
                .collect::<Vec<_>>();
            if boundary_changes.is_empty() {
                continue;
            }

            let neighbor_id = StaticVoxelBrickId(id.0 + direction);
            let Some(neighbor_handle) = self
                .static_bricks
                .get(&neighbor_id)
                .map(|brick| brick.collider)
            else {
                continue;
            };
            let mut neighbor = self.clone_brick_voxels(neighbor_handle);
            let origin_shift = brick_origin_shift(direction);
            for change in boundary_changes {
                voxels.propagate_voxel_change(
                    &mut neighbor,
                    to_rapier_ivec(change.local_voxel),
                    origin_shift,
                );
            }
            self.set_brick_voxels(neighbor_handle, neighbor);
        }

        let collider_present = !occupancy.is_empty();
        if collider_present {
            self.set_brick_voxels(collider, voxels);
            self.static_bricks
                .get_mut(&id)
                .expect("updated collision brick must exist")
                .occupancy = occupancy;
        } else {
            self.physics.remove_collider(collider);
            self.static_bricks.remove(&id);
        }

        StaticVoxelBrickUpdate::Applied {
            changed_voxels: changes.len(),
            collider_present,
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

fn voxel_is_on_face(voxel: IVec3, direction: IVec3) -> bool {
    let max = STATIC_VOXEL_BRICK_DIM as i32 - 1;
    (direction.x < 0 && voxel.x == 0)
        || (direction.x > 0 && voxel.x == max)
        || (direction.y < 0 && voxel.y == 0)
        || (direction.y > 0 && voxel.y == max)
        || (direction.z < 0 && voxel.z == 0)
        || (direction.z > 0 && voxel.z == max)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_voxel(local: UVec3) -> BrickOccupancy {
        BrickOccupancy::from_filled_voxels([local])
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
}
