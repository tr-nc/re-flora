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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TreeFoliageKind {
    Leaves,
    Apples,
}

#[derive(Clone, Debug)]
pub(super) struct TreeFoliageInput {
    pub(super) tree_id: u32,
    pub(super) bounds: Aabb3,
    pub(super) instance_count: u32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct TreeFoliageMainConfig {
    pub(super) camera_position: Vec3,
    pub(super) view_projection: Mat4,
    pub(super) lod_distance: f32,
    pub(super) draw_distance: f32,
    pub(super) render_leaves: bool,
    pub(super) render_apples: bool,
    pub(super) lighting_cache_start: u32,
    pub(super) max_cache_entries: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TreeFoliageBatch {
    kind: TreeFoliageKind,
    tree_id: u32,
    lod_state: LodState,
    instance_count: u32,
    lighting_cache_offset: Option<u32>,
}

impl TreeFoliageBatch {
    pub(super) fn kind(self) -> TreeFoliageKind {
        self.kind
    }

    pub(super) fn tree_id(self) -> u32 {
        self.tree_id
    }

    pub(super) fn lod_state(self) -> LodState {
        self.lod_state
    }

    pub(super) fn instance_count(self) -> u32 {
        self.instance_count
    }

    pub(super) fn lighting_cache_offset(self) -> Option<u32> {
        self.lighting_cache_offset
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TreeFoliageBatchGroup {
    kind: TreeFoliageKind,
    lod_state: LodState,
    batch_range: Range<usize>,
}

impl TreeFoliageBatchGroup {
    pub(super) fn kind(&self) -> TreeFoliageKind {
        self.kind
    }

    pub(super) fn lod_state(&self) -> LodState {
        self.lod_state
    }

    pub(super) fn batch_range(&self) -> Range<usize> {
        self.batch_range.clone()
    }
}

#[derive(Debug, Default)]
pub(super) struct TreeFoliageFramePlan {
    batches: Vec<TreeFoliageBatch>,
    groups: Vec<TreeFoliageBatchGroup>,
    required_lighting_cache_entries: u32,
}

impl TreeFoliageFramePlan {
    pub(super) fn for_main<Leaves, Apples>(
        config: TreeFoliageMainConfig,
        leaves: Leaves,
        apples: Apples,
    ) -> Self
    where
        Leaves: IntoIterator<Item = TreeFoliageInput>,
        Apples: IntoIterator<Item = TreeFoliageInput>,
    {
        let mut plan = Self::default();
        plan.append_main_kind(
            TreeFoliageKind::Leaves,
            config.render_leaves,
            config,
            leaves,
        );
        plan.append_main_kind(
            TreeFoliageKind::Apples,
            config.render_apples,
            config,
            apples,
        );
        plan
    }

    pub(super) fn for_shadow<Leaves, Apples>(leaves: Leaves, apples: Apples) -> Self
    where
        Leaves: IntoIterator<Item = TreeFoliageInput>,
        Apples: IntoIterator<Item = TreeFoliageInput>,
    {
        let mut plan = Self::default();
        plan.append_shadow_kind(TreeFoliageKind::Leaves, leaves);
        plan.append_shadow_kind(TreeFoliageKind::Apples, apples);
        plan
    }

    fn append_main_kind(
        &mut self,
        kind: TreeFoliageKind,
        enabled: bool,
        config: TreeFoliageMainConfig,
        inputs: impl IntoIterator<Item = TreeFoliageInput>,
    ) {
        if !enabled {
            return;
        }

        let mut lod0_batches = Vec::new();
        let mut lod1_batches = Vec::new();
        for input in inputs {
            if input.instance_count == 0 {
                continue;
            }
            if !input.bounds.is_inside_frustum(config.view_projection) {
                continue;
            }
            let distance = config.camera_position.distance(input.bounds.center());
            if distance > config.draw_distance {
                continue;
            }
            let lod_state = if distance <= config.lod_distance {
                LodState::Lod0
            } else {
                LodState::Lod1
            };
            let batch = TreeFoliageBatch {
                kind,
                tree_id: input.tree_id,
                lod_state,
                instance_count: input.instance_count,
                lighting_cache_offset: None,
            };
            match lod_state {
                LodState::Lod0 => lod0_batches.push(batch),
                LodState::Lod1 => lod1_batches.push(batch),
            }
        }

        if kind == TreeFoliageKind::Leaves {
            for batch in lod0_batches.iter_mut().chain(&mut lod1_batches) {
                let cache_offset = config
                    .lighting_cache_start
                    .checked_add(self.required_lighting_cache_entries)
                    .expect("tree-leaf lighting cache offset must fit u32");
                let next_cache_offset = cache_offset
                    .checked_add(batch.instance_count)
                    .expect("tree-leaf lighting cache plan size must fit u32");
                assert!(
                    next_cache_offset <= config.max_cache_entries,
                    "visible raster flora need {next_cache_offset} lighting cache entries, max is {}",
                    config.max_cache_entries,
                );
                batch.lighting_cache_offset = Some(cache_offset);
                self.required_lighting_cache_entries += batch.instance_count;
            }
        }

        self.append_group(kind, LodState::Lod0, lod0_batches);
        self.append_group(kind, LodState::Lod1, lod1_batches);
    }

    fn append_shadow_kind(
        &mut self,
        kind: TreeFoliageKind,
        inputs: impl IntoIterator<Item = TreeFoliageInput>,
    ) {
        let batches = inputs
            .into_iter()
            .filter(|input| input.instance_count > 0)
            .map(|input| TreeFoliageBatch {
                kind,
                tree_id: input.tree_id,
                lod_state: LodState::Lod1,
                instance_count: input.instance_count,
                lighting_cache_offset: None,
            })
            .collect();
        self.append_group(kind, LodState::Lod1, batches);
    }

    fn append_group(
        &mut self,
        kind: TreeFoliageKind,
        lod_state: LodState,
        batches: Vec<TreeFoliageBatch>,
    ) {
        if batches.is_empty() {
            return;
        }
        let batch_start = self.batches.len();
        self.batches.extend(batches);
        self.groups.push(TreeFoliageBatchGroup {
            kind,
            lod_state,
            batch_range: batch_start..self.batches.len(),
        });
    }

    pub(super) fn batches(&self) -> &[TreeFoliageBatch] {
        &self.batches
    }

    pub(super) fn groups(&self) -> &[TreeFoliageBatchGroup] {
        &self.groups
    }

    pub(super) fn required_lighting_cache_entries(&self) -> u32 {
        self.required_lighting_cache_entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds_at(center: Vec3) -> Aabb3 {
        Aabb3::new(center - Vec3::splat(0.05), center + Vec3::splat(0.05))
    }

    fn tree_input(tree_id: u32, center: Vec3, instance_count: u32) -> TreeFoliageInput {
        TreeFoliageInput {
            tree_id,
            bounds: bounds_at(center),
            instance_count,
        }
    }

    #[test]
    fn main_tree_foliage_frame_plan_culls_draw_distance_and_assigns_lod_at_the_boundary() {
        let plan = TreeFoliageFramePlan::for_main(
            TreeFoliageMainConfig {
                camera_position: Vec3::ZERO,
                view_projection: Mat4::IDENTITY,
                lod_distance: 0.5,
                draw_distance: 0.9,
                render_leaves: true,
                render_apples: false,
                lighting_cache_start: 100,
                max_cache_entries: u32::MAX,
            },
            [
                tree_input(10, Vec3::new(0.2, 0.0, 0.0), 3),
                tree_input(11, Vec3::new(0.5, 0.0, 0.0), 4),
                tree_input(12, Vec3::new(0.8, 0.0, 0.0), 5),
                tree_input(13, Vec3::new(0.9, 0.0, 0.0), 6),
                tree_input(14, Vec3::new(0.95, 0.0, 0.0), 7),
            ],
            [],
        );

        assert_eq!(
            plan.batches(),
            &[
                TreeFoliageBatch {
                    kind: TreeFoliageKind::Leaves,
                    tree_id: 10,
                    lod_state: LodState::Lod0,
                    instance_count: 3,
                    lighting_cache_offset: Some(100),
                },
                TreeFoliageBatch {
                    kind: TreeFoliageKind::Leaves,
                    tree_id: 11,
                    lod_state: LodState::Lod0,
                    instance_count: 4,
                    lighting_cache_offset: Some(103),
                },
                TreeFoliageBatch {
                    kind: TreeFoliageKind::Leaves,
                    tree_id: 12,
                    lod_state: LodState::Lod1,
                    instance_count: 5,
                    lighting_cache_offset: Some(107),
                },
                TreeFoliageBatch {
                    kind: TreeFoliageKind::Leaves,
                    tree_id: 13,
                    lod_state: LodState::Lod1,
                    instance_count: 6,
                    lighting_cache_offset: Some(112),
                },
            ]
        );
        assert_eq!(
            plan.groups(),
            &[
                TreeFoliageBatchGroup {
                    kind: TreeFoliageKind::Leaves,
                    lod_state: LodState::Lod0,
                    batch_range: 0..2,
                },
                TreeFoliageBatchGroup {
                    kind: TreeFoliageKind::Leaves,
                    lod_state: LodState::Lod1,
                    batch_range: 2..4,
                },
            ]
        );
        assert_eq!(plan.required_lighting_cache_entries(), 18);
    }

    #[test]
    #[should_panic(expected = "visible raster flora need 11 lighting cache entries, max is 10")]
    fn main_tree_foliage_frame_plan_enforces_combined_flora_cache_capacity() {
        let _plan = TreeFoliageFramePlan::for_main(
            TreeFoliageMainConfig {
                camera_position: Vec3::ZERO,
                view_projection: Mat4::IDENTITY,
                lod_distance: 0.5,
                draw_distance: 1.0,
                render_leaves: true,
                render_apples: false,
                lighting_cache_start: 8,
                max_cache_entries: 10,
            },
            [tree_input(10, Vec3::new(0.2, 0.0, 0.0), 3)],
            [],
        );
    }

    #[test]
    fn main_tree_foliage_frame_plan_honors_enablement_zero_filtering_and_stable_group_order() {
        let plan = TreeFoliageFramePlan::for_main(
            TreeFoliageMainConfig {
                camera_position: Vec3::ZERO,
                view_projection: Mat4::IDENTITY,
                lod_distance: 0.5,
                draw_distance: 2.0,
                render_leaves: false,
                render_apples: true,
                lighting_cache_start: 0,
                max_cache_entries: u32::MAX,
            },
            [tree_input(10, Vec3::new(0.2, 0.0, 0.0), 9)],
            [
                tree_input(20, Vec3::new(0.8, 0.0, 0.0), 2),
                tree_input(21, Vec3::new(0.2, 0.0, 0.0), 0),
                tree_input(22, Vec3::new(0.3, 0.0, 0.0), 3),
                tree_input(23, Vec3::new(1.2, 0.0, 0.0), 4),
                tree_input(24, Vec3::new(0.7, 0.0, 0.0), 5),
            ],
        );

        assert_eq!(
            plan.batches(),
            &[
                TreeFoliageBatch {
                    kind: TreeFoliageKind::Apples,
                    tree_id: 22,
                    lod_state: LodState::Lod0,
                    instance_count: 3,
                    lighting_cache_offset: None,
                },
                TreeFoliageBatch {
                    kind: TreeFoliageKind::Apples,
                    tree_id: 20,
                    lod_state: LodState::Lod1,
                    instance_count: 2,
                    lighting_cache_offset: None,
                },
                TreeFoliageBatch {
                    kind: TreeFoliageKind::Apples,
                    tree_id: 24,
                    lod_state: LodState::Lod1,
                    instance_count: 5,
                    lighting_cache_offset: None,
                },
            ]
        );
        assert_eq!(
            plan.groups(),
            &[
                TreeFoliageBatchGroup {
                    kind: TreeFoliageKind::Apples,
                    lod_state: LodState::Lod0,
                    batch_range: 0..1,
                },
                TreeFoliageBatchGroup {
                    kind: TreeFoliageKind::Apples,
                    lod_state: LodState::Lod1,
                    batch_range: 1..3,
                },
            ]
        );
    }

    #[test]
    fn main_tree_foliage_frame_plan_snapshots_identity_and_orders_all_kind_lod_groups() {
        let mut leaves = vec![
            tree_input(10, Vec3::new(0.8, 0.0, 0.0), 2),
            tree_input(11, Vec3::new(0.2, 0.0, 0.0), 3),
        ];
        let apples = vec![
            tree_input(20, Vec3::new(0.7, 0.0, 0.0), 4),
            tree_input(21, Vec3::new(0.3, 0.0, 0.0), 5),
        ];
        let plan = TreeFoliageFramePlan::for_main(
            TreeFoliageMainConfig {
                camera_position: Vec3::ZERO,
                view_projection: Mat4::IDENTITY,
                lod_distance: 0.5,
                draw_distance: 1.0,
                render_leaves: true,
                render_apples: true,
                lighting_cache_start: 0,
                max_cache_entries: u32::MAX,
            },
            leaves.clone(),
            apples,
        );
        leaves[0].tree_id = 99;
        leaves[0].instance_count = 99;

        let expected = [
            TreeFoliageBatch {
                kind: TreeFoliageKind::Leaves,
                tree_id: 11,
                lod_state: LodState::Lod0,
                instance_count: 3,
                lighting_cache_offset: Some(0),
            },
            TreeFoliageBatch {
                kind: TreeFoliageKind::Leaves,
                tree_id: 10,
                lod_state: LodState::Lod1,
                instance_count: 2,
                lighting_cache_offset: Some(3),
            },
            TreeFoliageBatch {
                kind: TreeFoliageKind::Apples,
                tree_id: 21,
                lod_state: LodState::Lod0,
                instance_count: 5,
                lighting_cache_offset: None,
            },
            TreeFoliageBatch {
                kind: TreeFoliageKind::Apples,
                tree_id: 20,
                lod_state: LodState::Lod1,
                instance_count: 4,
                lighting_cache_offset: None,
            },
        ];
        assert_eq!(plan.batches(), &expected);
        assert_eq!(plan.groups().len(), 4);
        for (index, group) in plan.groups().iter().enumerate() {
            assert_eq!(
                &plan.batches()[group.batch_range()],
                &expected[index..index + 1],
            );
        }
    }

    #[test]
    fn shadow_tree_foliage_frame_plan_is_unculled_zero_filtered_and_fixed_to_lod1() {
        let plan = TreeFoliageFramePlan::for_shadow(
            [
                tree_input(10, Vec3::splat(99.0), 2),
                tree_input(11, Vec3::ZERO, 0),
                tree_input(12, Vec3::splat(-99.0), 3),
            ],
            [
                tree_input(20, Vec3::splat(123.0), 4),
                tree_input(21, Vec3::ZERO, 0),
            ],
        );

        assert_eq!(
            plan.batches(),
            &[
                TreeFoliageBatch {
                    kind: TreeFoliageKind::Leaves,
                    tree_id: 10,
                    lod_state: LodState::Lod1,
                    instance_count: 2,
                    lighting_cache_offset: None,
                },
                TreeFoliageBatch {
                    kind: TreeFoliageKind::Leaves,
                    tree_id: 12,
                    lod_state: LodState::Lod1,
                    instance_count: 3,
                    lighting_cache_offset: None,
                },
                TreeFoliageBatch {
                    kind: TreeFoliageKind::Apples,
                    tree_id: 20,
                    lod_state: LodState::Lod1,
                    instance_count: 4,
                    lighting_cache_offset: None,
                },
            ]
        );
        assert_eq!(
            plan.groups(),
            &[
                TreeFoliageBatchGroup {
                    kind: TreeFoliageKind::Leaves,
                    lod_state: LodState::Lod1,
                    batch_range: 0..2,
                },
                TreeFoliageBatchGroup {
                    kind: TreeFoliageKind::Apples,
                    lod_state: LodState::Lod1,
                    batch_range: 2..3,
                },
            ]
        );
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
