use super::{App, VisibleTerrainChange, CHUNK_DIM, VOXEL_DIM_PER_CHUNK};
use crate::app::world_edits::BuildEdit;
use crate::builder::VOXEL_TYPE_MASK;
use crate::geom::UAabb3;
use glam::UVec3;
use std::collections::VecDeque;
use std::time::Instant;

// A 24-voxel halo fully contains a compact island near the 16k global particle
// capacity, while the hard block budget keeps release-time work bounded.
const ANALYSIS_HALO_VOXELS: u32 = 24;
const MAX_ANALYSIS_VOXELS: u64 = 16 * 1024 * 1024;

#[derive(Default)]
pub(super) struct TerrainConnectivityRuntime {
    pending_edited_voxels_inclusive: Option<UAabb3>,
}

impl TerrainConnectivityRuntime {
    pub(super) fn record_edit(&mut self, edited_voxels_inclusive: UAabb3) {
        self.pending_edited_voxels_inclusive = Some(
            self.pending_edited_voxels_inclusive
                .map_or(edited_voxels_inclusive, |pending| {
                    pending.union_with(&edited_voxels_inclusive)
                }),
        );
    }

    fn take_edit_region(&mut self, world_dim: UVec3) -> Option<(UAabb3, UAabb3)> {
        let inclusive = self.pending_edited_voxels_inclusive.take()?;
        let edited_max_exclusive = inclusive.max().saturating_add(UVec3::ONE).min(world_dim);
        let edited = UAabb3::new(inclusive.min().min(world_dim), edited_max_exclusive);
        if edited.min().cmpge(edited.max()).any() {
            return None;
        }

        let halo = UVec3::splat(ANALYSIS_HALO_VOXELS);
        let block = UAabb3::new(
            edited.min().saturating_sub(halo),
            edited.max().saturating_add(halo).min(world_dim),
        );
        Some((edited, block))
    }
}

fn voxel_count(bound: UAabb3) -> u64 {
    let dim = bound.dimensions();
    u64::from(dim.x) * u64::from(dim.y) * u64::from(dim.z)
}

fn select_components_for_particle_capacity(
    components: Vec<DetachedVoxelComponent>,
    particle_capacity: usize,
) -> (Vec<(UVec3, u8)>, usize) {
    let mut selected_voxels = Vec::new();
    let mut skipped_components = 0;
    for component in components {
        if component.voxels.len() <= particle_capacity.saturating_sub(selected_voxels.len()) {
            selected_voxels.extend(component.voxels);
        } else {
            skipped_components += 1;
        }
    }
    (selected_voxels, skipped_components)
}

impl App {
    pub(super) fn record_player_terrain_connectivity_edit(&mut self, bound: UAabb3) {
        if self.player_tools.continuous_hold_active() {
            self.terrain_connectivity.record_edit(bound);
        }
    }

    pub(super) fn resolve_detached_terrain_after_edit(&mut self) -> anyhow::Result<()> {
        let world_dim = CHUNK_DIM * VOXEL_DIM_PER_CHUNK;
        let Some((edited, block)) = self.terrain_connectivity.take_edit_region(world_dim) else {
            return Ok(());
        };
        let analysis_voxels = voxel_count(block);
        if analysis_voxels > MAX_ANALYSIS_VOXELS {
            log::warn!(
                "[TERRAIN_CONNECTIVITY] skipped oversized release analysis edited={:?}..{:?} block={:?}..{:?} voxels={} budget={}",
                edited.min(),
                edited.max(),
                block.min(),
                block.max(),
                analysis_voxels,
                MAX_ANALYSIS_VOXELS,
            );
            return Ok(());
        }

        let started = Instant::now();
        let mut atlas_voxels = self
            .plain_builder
            .read_chunk_atlas_region(block.min(), block.dimensions())?;
        let candidate_region = UAabb3::new(
            edited.min().saturating_sub(UVec3::ONE),
            edited.max().saturating_add(UVec3::ONE).min(world_dim),
        );
        let components = detached_components_in_edit_region(
            &atlas_voxels,
            block,
            candidate_region,
            VOXEL_TYPE_MASK as u8,
        )?;
        if components.is_empty() {
            log::info!(
                "[TERRAIN_CONNECTIVITY] release checked_voxels={} detached_components=0 elapsed_ms={:.2}",
                analysis_voxels,
                started.elapsed().as_secs_f64() * 1000.0,
            );
            return Ok(());
        }

        let available_particles = self.particle_system.available_capacity();
        let (selected_voxels, skipped_components) =
            select_components_for_particle_capacity(components, available_particles);
        if selected_voxels.is_empty() {
            log::warn!(
                "[TERRAIN_CONNECTIVITY] preserved detached terrain because particle capacity is exhausted available={} skipped_components={}",
                available_particles,
                skipped_components,
            );
            return Ok(());
        }

        let block_dim = block.dimensions();
        let local_index = |world_voxel: UVec3| -> usize {
            let local = world_voxel - block.min();
            (local.x + block_dim.x * (local.y + block_dim.y * local.z)) as usize
        };
        let mut detached_min = world_dim;
        let mut detached_max = UVec3::ZERO;
        for &(world_voxel, _) in &selected_voxels {
            atlas_voxels[local_index(world_voxel)] = 0;
            detached_min = detached_min.min(world_voxel);
            detached_max = detached_max.max(world_voxel);
        }

        self.plain_builder.write_chunk_atlas_region(
            block.min(),
            block.dimensions(),
            &atlas_voxels,
        )?;
        let detached_bound = UAabb3::new(
            detached_min,
            detached_max.saturating_add(UVec3::ONE).min(world_dim),
        );
        let change =
            VisibleTerrainChange::from_build_edits(vec![BuildEdit::RebuildMeshWithoutFlora(
                detached_bound,
            )])?
            .expect("detached terrain voxels always define a visible rebuild");
        self.publish_visible_terrain(change)?;

        let spawned = self.spawn_detached_terrain_voxel_particles(&selected_voxels);
        anyhow::ensure!(
            spawned == selected_voxels.len(),
            "detached terrain cleared {} voxels but spawned only {} particles",
            selected_voxels.len(),
            spawned,
        );
        log::info!(
            "[TERRAIN_CONNECTIVITY] release checked_voxels={} detached_voxels={} spawned_particles={} skipped_components={} elapsed_ms={:.2}",
            analysis_voxels,
            selected_voxels.len(),
            spawned,
            skipped_components,
            started.elapsed().as_secs_f64() * 1000.0,
        );
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DetachedVoxelComponent {
    pub(super) voxels: Vec<(UVec3, u8)>,
}

pub(super) fn detached_components_in_edit_region(
    atlas_voxels: &[u8],
    block: UAabb3,
    edited: UAabb3,
    voxel_type_mask: u8,
) -> anyhow::Result<Vec<DetachedVoxelComponent>> {
    let dim = block.dimensions();
    anyhow::ensure!(dim.cmpgt(UVec3::ZERO).all(), "connectivity block is empty");
    let expected_len = usize::try_from(u64::from(dim.x) * u64::from(dim.y) * u64::from(dim.z))?;
    anyhow::ensure!(
        atlas_voxels.len() == expected_len,
        "connectivity block has {} voxels, expected {} for {:?}",
        atlas_voxels.len(),
        expected_len,
        dim,
    );

    let voxel_type_at = |index: usize| atlas_voxels[index] & voxel_type_mask;
    let index_of = |position: UVec3| -> usize {
        (position.x + dim.x * (position.y + dim.y * position.z)) as usize
    };
    let position_of = |index: usize| -> UVec3 {
        let index = index as u32;
        let plane = dim.x * dim.y;
        let z = index / plane;
        let remainder = index % plane;
        UVec3::new(remainder % dim.x, remainder / dim.x, z)
    };
    let enqueue_solid_neighbors =
        |position: UVec3, visited: &mut [bool], queue: &mut VecDeque<usize>| {
            let mut enqueue = |neighbor: UVec3| {
                let index = index_of(neighbor);
                if !visited[index] && voxel_type_at(index) != 0 {
                    visited[index] = true;
                    queue.push_back(index);
                }
            };
            if position.x > 0 {
                enqueue(position - UVec3::X);
            }
            if position.x + 1 < dim.x {
                enqueue(position + UVec3::X);
            }
            if position.y > 0 {
                enqueue(position - UVec3::Y);
            }
            if position.y + 1 < dim.y {
                enqueue(position + UVec3::Y);
            }
            if position.z > 0 {
                enqueue(position - UVec3::Z);
            }
            if position.z + 1 < dim.z {
                enqueue(position + UVec3::Z);
            }
        };

    // A component reaching the local block boundary may continue to grounded
    // terrain outside the readback. Flood from that boundary first so the
    // local classifier fails closed instead of deleting uncertain geometry.
    let mut anchored = vec![false; expected_len];
    let mut queue = VecDeque::new();
    for z in 0..dim.z {
        for y in 0..dim.y {
            for x in 0..dim.x {
                if x != 0 && y != 0 && z != 0 && x + 1 != dim.x && y + 1 != dim.y && z + 1 != dim.z
                {
                    continue;
                }
                let index = index_of(UVec3::new(x, y, z));
                if voxel_type_at(index) != 0 && !anchored[index] {
                    anchored[index] = true;
                    queue.push_back(index);
                }
            }
        }
    }
    while let Some(index) = queue.pop_front() {
        enqueue_solid_neighbors(position_of(index), &mut anchored, &mut queue);
    }

    let candidate_min = edited.min().max(block.min());
    let candidate_max = edited.max().min(block.max());
    if candidate_min.cmpge(candidate_max).any() {
        return Ok(Vec::new());
    }

    let mut classified = anchored;
    let mut components = Vec::new();
    for world_z in candidate_min.z..candidate_max.z {
        for world_y in candidate_min.y..candidate_max.y {
            for world_x in candidate_min.x..candidate_max.x {
                let local = UVec3::new(world_x, world_y, world_z) - block.min();
                let seed_index = index_of(local);
                if voxel_type_at(seed_index) == 0 || classified[seed_index] {
                    continue;
                }

                classified[seed_index] = true;
                queue.push_back(seed_index);
                let mut voxels = Vec::new();
                while let Some(index) = queue.pop_front() {
                    let local = position_of(index);
                    voxels.push((block.min() + local, voxel_type_at(index)));
                    enqueue_solid_neighbors(local, &mut classified, &mut queue);
                }
                components.push(DetachedVoxelComponent { voxels });
            }
        }
    }

    Ok(components)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(dim: UVec3, position: UVec3) -> usize {
        (position.x + dim.x * (position.y + dim.y * position.z)) as usize
    }

    fn block_with_solids(dim: UVec3, solids: &[(UVec3, u8)]) -> Vec<u8> {
        let mut voxels = vec![0; (dim.x * dim.y * dim.z) as usize];
        for &(position, voxel_type) in solids {
            voxels[index(dim, position)] = voxel_type;
        }
        voxels
    }

    #[test]
    fn floating_component_intersecting_the_edit_region_detaches() {
        let dim = UVec3::splat(7);
        let solids = [(UVec3::new(3, 3, 3), 1), (UVec3::new(4, 3, 3), 2)];
        let voxels = block_with_solids(dim, &solids);

        let components = detached_components_in_edit_region(
            &voxels,
            UAabb3::new(UVec3::ZERO, dim),
            UAabb3::new(UVec3::new(3, 3, 3), UVec3::new(4, 4, 4)),
            0x07,
        )
        .unwrap();

        assert_eq!(components.len(), 1);
        assert_eq!(components[0].voxels, solids);
    }

    #[test]
    fn component_connected_to_world_floor_stays_terrain() {
        let dim = UVec3::splat(7);
        let solids = (0..=3)
            .map(|y| (UVec3::new(3, y, 3), 1))
            .collect::<Vec<_>>();
        let voxels = block_with_solids(dim, &solids);

        let components = detached_components_in_edit_region(
            &voxels,
            UAabb3::new(UVec3::ZERO, dim),
            UAabb3::new(UVec3::new(3, 3, 3), UVec3::new(4, 4, 4)),
            0x07,
        )
        .unwrap();

        assert!(components.is_empty());
    }

    #[test]
    fn component_reaching_local_analysis_edge_is_conservatively_preserved() {
        let block_min = UVec3::new(10, 10, 10);
        let dim = UVec3::splat(7);
        let solids = (3..=6)
            .map(|x| (UVec3::new(x, 3, 3), 1))
            .collect::<Vec<_>>();
        let voxels = block_with_solids(dim, &solids);

        let components = detached_components_in_edit_region(
            &voxels,
            UAabb3::new(block_min, block_min + dim),
            UAabb3::new(
                block_min + UVec3::new(3, 3, 3),
                block_min + UVec3::new(4, 4, 4),
            ),
            0x07,
        )
        .unwrap();

        assert!(components.is_empty());
    }

    #[test]
    fn floating_component_outside_the_edit_region_is_not_reclassified() {
        let dim = UVec3::splat(7);
        let voxels = block_with_solids(dim, &[(UVec3::new(5, 4, 5), 1)]);

        let components = detached_components_in_edit_region(
            &voxels,
            UAabb3::new(UVec3::ZERO, dim),
            UAabb3::new(UVec3::new(2, 2, 2), UVec3::new(4, 4, 4)),
            0x07,
        )
        .unwrap();

        assert!(components.is_empty());
    }

    #[test]
    fn release_region_is_consumed_once_and_expands_by_the_analysis_halo() {
        let mut runtime = TerrainConnectivityRuntime::default();
        runtime.record_edit(UAabb3::new(UVec3::new(20, 30, 40), UVec3::new(24, 34, 44)));

        let (edited, block) = runtime.take_edit_region(UVec3::splat(128)).unwrap();

        assert_eq!(edited.min(), UVec3::new(20, 30, 40));
        assert_eq!(edited.max(), UVec3::new(25, 35, 45));
        assert_eq!(block.min(), UVec3::new(0, 6, 16));
        assert_eq!(block.max(), UVec3::new(49, 59, 69));
        assert!(runtime.take_edit_region(UVec3::splat(128)).is_none());
    }

    #[test]
    fn particle_capacity_never_splits_a_detached_component() {
        let component = |start: u32, count: u32| DetachedVoxelComponent {
            voxels: (start..start + count)
                .map(|x| (UVec3::new(x, 3, 3), 1))
                .collect(),
        };
        let components = vec![component(0, 4), component(10, 3), component(20, 2)];

        let (selected, skipped) = select_components_for_particle_capacity(components, 6);

        assert_eq!(selected.len(), 6);
        assert!(selected
            .iter()
            .all(|(position, _)| position.x < 4 || position.x >= 20));
        assert_eq!(skipped, 1);
    }
}
