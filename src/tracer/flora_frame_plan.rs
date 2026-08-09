use super::LodState;
use crate::geom::Aabb3;
use glam::{Mat4, Vec3};
use std::ops::Range;

#[derive(Clone, Copy, Debug)]
pub(super) struct FloraFramePlanConfig {
    pub(super) camera_position: Vec3,
    pub(super) view_projection: Mat4,
    pub(super) lod_distance: f32,
    pub(super) draw_distance: f32,
    pub(super) chunk_count: usize,
    pub(super) species_count: usize,
    pub(super) max_cache_entries: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FloraFrameBatch {
    species_index: usize,
    lod_state: LodState,
    chunk_index: usize,
    instance_count: u32,
    mesh_voxel_count: u32,
    lighting_cache_offset: u32,
}

impl FloraFrameBatch {
    pub(super) fn species_index(self) -> usize {
        self.species_index
    }

    pub(super) fn lod_state(self) -> LodState {
        self.lod_state
    }

    pub(super) fn chunk_index(self) -> usize {
        self.chunk_index
    }

    pub(super) fn instance_count(self) -> u32 {
        self.instance_count
    }

    pub(super) fn mesh_voxel_count(self) -> u32 {
        self.mesh_voxel_count
    }

    pub(super) fn lighting_cache_offset(self) -> u32 {
        self.lighting_cache_offset
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FloraFrameBatchGroup {
    species_index: usize,
    lod_state: LodState,
    batch_range: Range<usize>,
}

impl FloraFrameBatchGroup {
    pub(super) fn species_index(&self) -> usize {
        self.species_index
    }

    pub(super) fn lod_state(&self) -> LodState {
        self.lod_state
    }
}

#[derive(Debug, Default)]
pub(super) struct FloraFramePlan {
    batches: Vec<FloraFrameBatch>,
    groups: Vec<FloraFrameBatchGroup>,
    required_cache_entries: u32,
    instance_count: u64,
}

impl FloraFramePlan {
    pub(super) fn build<ChunkBounds, InstanceCount, SpeciesEnabled, MeshVoxelCount>(
        config: FloraFramePlanConfig,
        chunk_bounds: ChunkBounds,
        instance_count: InstanceCount,
        species_enabled: SpeciesEnabled,
        mesh_voxel_count: MeshVoxelCount,
    ) -> Self
    where
        ChunkBounds: Fn(usize) -> Aabb3,
        InstanceCount: Fn(usize, usize) -> u32,
        SpeciesEnabled: Fn(usize) -> bool,
        MeshVoxelCount: Fn(usize, LodState) -> u32,
    {
        let mut lod0_chunks = Vec::new();
        let mut lod1_chunks = Vec::new();
        for chunk_index in 0..config.chunk_count {
            let bounds = chunk_bounds(chunk_index);
            if !bounds.is_inside_frustum(config.view_projection) {
                continue;
            }
            let distance = config.camera_position.distance(bounds.center());
            if distance > config.draw_distance {
                continue;
            }
            if distance <= config.lod_distance {
                lod0_chunks.push(chunk_index);
            } else {
                lod1_chunks.push(chunk_index);
            }
        }

        let mut batches = Vec::new();
        let mut groups = Vec::new();
        let mut required_cache_entries = 0u32;
        let mut total_instance_count = 0u64;
        for species_index in 0..config.species_count {
            if !species_enabled(species_index) {
                continue;
            }
            for (lod_state, chunk_indices) in [
                (LodState::Lod0, lod0_chunks.as_slice()),
                (LodState::Lod1, lod1_chunks.as_slice()),
            ] {
                let batch_start = batches.len();
                let voxel_count = mesh_voxel_count(species_index, lod_state);
                for &chunk_index in chunk_indices {
                    let batch_instance_count = instance_count(chunk_index, species_index);
                    if batch_instance_count == 0 {
                        continue;
                    }
                    assert!(
                        voxel_count > 0,
                        "flora species {species_index} {lod_state:?} has instances but no mesh voxels"
                    );
                    let batch_cache_entries = batch_instance_count
                        .checked_mul(voxel_count)
                        .expect("flora lighting cache batch size must fit u32");
                    let next_cache_entries = required_cache_entries
                        .checked_add(batch_cache_entries)
                        .expect("flora lighting cache plan size must fit u32");
                    assert!(
                        next_cache_entries <= config.max_cache_entries,
                        "visible flora need {next_cache_entries} lighting cache entries, max is {}",
                        config.max_cache_entries,
                    );
                    batches.push(FloraFrameBatch {
                        species_index,
                        lod_state,
                        chunk_index,
                        instance_count: batch_instance_count,
                        mesh_voxel_count: voxel_count,
                        lighting_cache_offset: required_cache_entries,
                    });
                    required_cache_entries = next_cache_entries;
                    total_instance_count += u64::from(batch_instance_count);
                }
                if batches.len() > batch_start {
                    groups.push(FloraFrameBatchGroup {
                        species_index,
                        lod_state,
                        batch_range: batch_start..batches.len(),
                    });
                }
            }
        }

        Self {
            batches,
            groups,
            required_cache_entries,
            instance_count: total_instance_count,
        }
    }

    pub(super) fn batches(&self) -> &[FloraFrameBatch] {
        &self.batches
    }

    pub(super) fn groups(&self) -> &[FloraFrameBatchGroup] {
        &self.groups
    }

    pub(super) fn group_batches(&self, group: &FloraFrameBatchGroup) -> &[FloraFrameBatch] {
        &self.batches[group.batch_range.clone()]
    }

    pub(super) fn required_cache_entries(&self) -> u32 {
        self.required_cache_entries
    }

    pub(super) fn instance_count(&self) -> u64 {
        self.instance_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds_at(center: Vec3) -> Aabb3 {
        Aabb3::new(center - Vec3::splat(0.05), center + Vec3::splat(0.05))
    }

    #[test]
    fn frame_plan_is_the_single_ordered_source_for_cache_and_draw_batches() {
        let bounds = [
            bounds_at(Vec3::new(0.2, 0.2, 0.2)),
            bounds_at(Vec3::new(0.8, 0.2, 0.2)),
            bounds_at(Vec3::new(0.95, 0.2, 0.2)),
            bounds_at(Vec3::new(2.0, 0.2, 0.2)),
        ];
        let counts = [[2, 9, 1], [3, 9, 4], [5, 9, 6], [7, 9, 8]];
        let voxel_counts = [[5, 2], [7, 3], [11, 4]];
        let plan = FloraFramePlan::build(
            FloraFramePlanConfig {
                camera_position: Vec3::ZERO,
                view_projection: Mat4::IDENTITY,
                lod_distance: 0.5,
                draw_distance: 0.9,
                chunk_count: bounds.len(),
                species_count: counts[0].len(),
                max_cache_entries: 1_000,
            },
            |chunk_index| bounds[chunk_index].clone(),
            |chunk_index, species_index| counts[chunk_index][species_index],
            |species_index| species_index != 1,
            |species_index, lod_state| {
                voxel_counts[species_index][usize::from(lod_state == LodState::Lod1)]
            },
        );

        assert_eq!(
            plan.batches(),
            &[
                FloraFrameBatch {
                    species_index: 0,
                    lod_state: LodState::Lod0,
                    chunk_index: 0,
                    instance_count: 2,
                    mesh_voxel_count: 5,
                    lighting_cache_offset: 0,
                },
                FloraFrameBatch {
                    species_index: 0,
                    lod_state: LodState::Lod1,
                    chunk_index: 1,
                    instance_count: 3,
                    mesh_voxel_count: 2,
                    lighting_cache_offset: 10,
                },
                FloraFrameBatch {
                    species_index: 2,
                    lod_state: LodState::Lod0,
                    chunk_index: 0,
                    instance_count: 1,
                    mesh_voxel_count: 11,
                    lighting_cache_offset: 16,
                },
                FloraFrameBatch {
                    species_index: 2,
                    lod_state: LodState::Lod1,
                    chunk_index: 1,
                    instance_count: 4,
                    mesh_voxel_count: 4,
                    lighting_cache_offset: 27,
                },
            ]
        );
        assert_eq!(
            plan.groups(),
            &[
                FloraFrameBatchGroup {
                    species_index: 0,
                    lod_state: LodState::Lod0,
                    batch_range: 0..1,
                },
                FloraFrameBatchGroup {
                    species_index: 0,
                    lod_state: LodState::Lod1,
                    batch_range: 1..2,
                },
                FloraFrameBatchGroup {
                    species_index: 2,
                    lod_state: LodState::Lod0,
                    batch_range: 2..3,
                },
                FloraFrameBatchGroup {
                    species_index: 2,
                    lod_state: LodState::Lod1,
                    batch_range: 3..4,
                },
            ]
        );
        assert_eq!(plan.required_cache_entries(), 43);
        assert_eq!(plan.instance_count(), 10);
    }

    #[test]
    fn empty_or_culled_chunks_do_not_consume_cache_offsets() {
        let bounds = [
            bounds_at(Vec3::new(0.2, 0.2, 0.2)),
            bounds_at(Vec3::new(2.0, 0.2, 0.2)),
        ];
        let plan = FloraFramePlan::build(
            FloraFramePlanConfig {
                camera_position: Vec3::ZERO,
                view_projection: Mat4::IDENTITY,
                lod_distance: 0.5,
                draw_distance: 1.0,
                chunk_count: bounds.len(),
                species_count: 1,
                max_cache_entries: 8,
            },
            |chunk_index| bounds[chunk_index].clone(),
            |_chunk_index, _species_index| 0,
            |_species_index| true,
            |_species_index, _lod_state| 4,
        );

        assert!(plan.batches().is_empty());
        assert_eq!(plan.required_cache_entries(), 0);
        assert_eq!(plan.instance_count(), 0);
    }

    #[test]
    #[should_panic(expected = "visible flora need 10 lighting cache entries, max is 9")]
    fn frame_plan_enforces_the_cache_address_space_limit() {
        let bounds = [bounds_at(Vec3::new(0.2, 0.2, 0.2))];
        FloraFramePlan::build(
            FloraFramePlanConfig {
                camera_position: Vec3::ZERO,
                view_projection: Mat4::IDENTITY,
                lod_distance: 0.5,
                draw_distance: 1.0,
                chunk_count: bounds.len(),
                species_count: 1,
                max_cache_entries: 9,
            },
            |chunk_index| bounds[chunk_index].clone(),
            |_chunk_index, _species_index| 2,
            |_species_index| true,
            |_species_index, _lod_state| 5,
        );
    }
}
