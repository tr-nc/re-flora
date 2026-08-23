use std::collections::{BTreeMap, BTreeSet};

use glam::UVec3;

use crate::{builder::ContreeCpuVoxelSourceDependency, geom::UAabb3};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EmissiveVoxelScanReason {
    RuntimeEdit,
    ConservativeChunk,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EmissiveVoxelScanWork {
    chunk_idx: UVec3,
    cell_bound: UAabb3,
    dependency: ContreeCpuVoxelSourceDependency,
    queued_frame: u64,
    reason: EmissiveVoxelScanReason,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct EmissiveVoxelScanBacklog {
    priority_cells: usize,
    full_cells: usize,
    awaiting_dependency_cells: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EmissiveVoxelScanSchedulerError {
    InvalidGrid,
    InvalidTrustedBound(UAabb3),
    UnknownChunk(UVec3),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct GridCoord([u32; 3]);

impl GridCoord {
    fn new(value: UVec3) -> Self {
        Self(value.to_array())
    }

    fn get(self) -> UVec3 {
        UVec3::from_array(self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PendingCell {
    dependency: ContreeCpuVoxelSourceDependency,
    queued_frame: u64,
}

#[derive(Clone, Debug)]
struct FullChunkScan {
    dependency: ContreeCpuVoxelSourceDependency,
    queued_frame: u64,
    cells: BTreeSet<GridCoord>,
}

#[derive(Clone, Debug, Default)]
struct ChunkScanState {
    observed: Option<ContreeCpuVoxelSourceDependency>,
    awaiting_dependency: BTreeMap<GridCoord, u64>,
    priority: BTreeMap<GridCoord, PendingCell>,
    full: Option<FullChunkScan>,
}

/// Plans bounded, revision-matched emitter scans without owning Contree or the light provider.
/// Runtime edits stay high priority while conservative startup/load scans advance round-robin.
#[derive(Clone, Debug)]
struct EmissiveVoxelScanScheduler {
    chunk_dim: UVec3,
    voxel_dim_per_chunk: UVec3,
    cell_dim: UVec3,
    world_voxel_dim: UVec3,
    cells_per_chunk: usize,
    chunks: BTreeMap<GridCoord, ChunkScanState>,
    full_chunk_cursor: usize,
    full_turn_when_single_budget: bool,
}

impl EmissiveVoxelScanScheduler {
    fn new(
        chunk_dim: UVec3,
        voxel_dim_per_chunk: UVec3,
        cell_dim: UVec3,
    ) -> Result<Self, EmissiveVoxelScanSchedulerError> {
        if chunk_dim.cmpeq(UVec3::ZERO).any()
            || voxel_dim_per_chunk.cmpeq(UVec3::ZERO).any()
            || cell_dim.cmpeq(UVec3::ZERO).any()
            || (voxel_dim_per_chunk % cell_dim).cmpne(UVec3::ZERO).any()
        {
            return Err(EmissiveVoxelScanSchedulerError::InvalidGrid);
        }
        let Some(world_voxel_dim) = chunk_dim.checked_mul(voxel_dim_per_chunk) else {
            return Err(EmissiveVoxelScanSchedulerError::InvalidGrid);
        };
        let cells_per_axis = voxel_dim_per_chunk / cell_dim;
        let Some(cells_per_chunk) = usize::try_from(
            u64::from(cells_per_axis.x)
                .checked_mul(u64::from(cells_per_axis.y))
                .and_then(|count| count.checked_mul(u64::from(cells_per_axis.z)))
                .ok_or(EmissiveVoxelScanSchedulerError::InvalidGrid)?,
        )
        .ok() else {
            return Err(EmissiveVoxelScanSchedulerError::InvalidGrid);
        };
        let mut chunks = BTreeMap::new();
        for z in 0..chunk_dim.z {
            for y in 0..chunk_dim.y {
                for x in 0..chunk_dim.x {
                    chunks.insert(
                        GridCoord::new(UVec3::new(x, y, z)),
                        ChunkScanState::default(),
                    );
                }
            }
        }
        Ok(Self {
            chunk_dim,
            voxel_dim_per_chunk,
            cell_dim,
            world_voxel_dim,
            cells_per_chunk,
            chunks,
            full_chunk_cursor: 0,
            full_turn_when_single_budget: false,
        })
    }

    /// Records a trustworthy half-open terrain-edit bound. It remains pending until a new Contree
    /// source dependency is published, so scans can never read the previous source revision.
    fn mark_trusted_change(
        &mut self,
        bound: UAabb3,
        frame: u64,
    ) -> Result<usize, EmissiveVoxelScanSchedulerError> {
        if bound.min().cmpge(bound.max()).any() || bound.max().cmpgt(self.world_voxel_dim).any() {
            return Err(EmissiveVoxelScanSchedulerError::InvalidTrustedBound(bound));
        }
        let mut touched = 0;
        for cell in grid_cells_intersecting(bound, self.cell_dim) {
            let chunk_idx = (cell.get() * self.cell_dim) / self.voxel_dim_per_chunk;
            let state = self
                .chunks
                .get_mut(&GridCoord::new(chunk_idx))
                .expect("validated world cell must belong to one configured chunk");
            state
                .awaiting_dependency
                .entry(cell)
                .and_modify(|queued_frame| *queued_frame = (*queued_frame).min(frame))
                .or_insert(frame);
            touched += 1;
        }
        Ok(touched)
    }

    /// Observes a published immutable Contree dependency. A revision without a matching trusted
    /// bound is treated as a load/rebuild and conservatively schedules the whole chunk.
    fn observe_dependency(
        &mut self,
        dependency: ContreeCpuVoxelSourceDependency,
        frame: u64,
    ) -> Result<(), EmissiveVoxelScanSchedulerError> {
        if dependency.chunk_idx.cmpge(self.chunk_dim).any() {
            return Err(EmissiveVoxelScanSchedulerError::UnknownChunk(
                dependency.chunk_idx,
            ));
        }
        if dependency.source_revision.is_none() {
            return Ok(());
        }
        let chunk = GridCoord::new(dependency.chunk_idx);
        let state = self
            .chunks
            .get_mut(&chunk)
            .expect("validated chunk must have scheduler state");
        if state.observed == Some(dependency) {
            return Ok(());
        }
        state.observed = Some(dependency);

        if state.awaiting_dependency.is_empty() {
            state.priority.clear();
            state.full = Some(FullChunkScan {
                dependency,
                queued_frame: frame,
                cells: cells_in_chunk(
                    dependency.chunk_idx,
                    self.voxel_dim_per_chunk,
                    self.cell_dim,
                ),
            });
            return Ok(());
        }

        let awaiting = std::mem::take(&mut state.awaiting_dependency);
        if awaiting.len() == self.cells_per_chunk {
            let queued_frame = awaiting.values().copied().min().unwrap_or(frame);
            state.priority.clear();
            state.full = Some(FullChunkScan {
                dependency,
                queued_frame,
                cells: cells_in_chunk(
                    dependency.chunk_idx,
                    self.voxel_dim_per_chunk,
                    self.cell_dim,
                ),
            });
            return Ok(());
        }

        for pending in state.priority.values_mut() {
            pending.dependency = dependency;
        }
        if let Some(full) = state.full.as_mut() {
            full.dependency = dependency;
        }
        for (cell, queued_frame) in awaiting {
            if let Some(full) = state.full.as_mut() {
                full.cells.remove(&cell);
            }
            state
                .priority
                .entry(cell)
                .and_modify(|pending| {
                    pending.dependency = dependency;
                    pending.queued_frame = pending.queued_frame.min(queued_frame);
                })
                .or_insert(PendingCell {
                    dependency,
                    queued_frame,
                });
        }
        if state
            .full
            .as_ref()
            .is_some_and(|full| full.cells.is_empty())
        {
            state.full = None;
        }
        Ok(())
    }

    fn backlog(&self) -> EmissiveVoxelScanBacklog {
        EmissiveVoxelScanBacklog {
            priority_cells: self.chunks.values().map(|state| state.priority.len()).sum(),
            full_cells: self
                .chunks
                .values()
                .filter_map(|state| state.full.as_ref())
                .map(|full| full.cells.len())
                .sum(),
            awaiting_dependency_cells: self
                .chunks
                .values()
                .map(|state| state.awaiting_dependency.len())
                .sum(),
        }
    }

    fn take_work(&mut self, budget: usize) -> Vec<EmissiveVoxelScanWork> {
        let mut work = Vec::with_capacity(budget);
        let mut guaranteed_full_taken = false;
        while work.len() < budget {
            let backlog = self.backlog();
            let has_priority = backlog.priority_cells > 0;
            let has_full = backlog.full_cells > 0;
            if !has_priority && !has_full {
                break;
            }

            let select_full = if has_priority && has_full {
                if budget == 1 {
                    let select_full = self.full_turn_when_single_budget;
                    self.full_turn_when_single_budget = !self.full_turn_when_single_budget;
                    select_full
                } else {
                    !guaranteed_full_taken && work.len() + 1 == budget
                }
            } else {
                has_full
            };
            let next = if select_full {
                guaranteed_full_taken = true;
                self.pop_full()
            } else {
                self.pop_priority()
            };
            if let Some(next) = next {
                work.push(next);
            } else if let Some(next) = self.pop_full().or_else(|| self.pop_priority()) {
                work.push(next);
            } else {
                break;
            }
        }
        work
    }

    /// Returns failed work only if its immutable dependency is still current. An old read or
    /// failure can therefore never overwrite a task already promoted to a newer source revision.
    fn retry(&mut self, work: EmissiveVoxelScanWork) -> bool {
        let Some(state) = self.chunks.get_mut(&GridCoord::new(work.chunk_idx)) else {
            return false;
        };
        if state.observed != Some(work.dependency) {
            return false;
        }
        let cell = GridCoord::new(work.cell_bound.min() / self.cell_dim);
        match work.reason {
            EmissiveVoxelScanReason::RuntimeEdit => {
                if let Some(full) = state.full.as_mut() {
                    full.cells.remove(&cell);
                }
                state.priority.entry(cell).or_insert(PendingCell {
                    dependency: work.dependency,
                    queued_frame: work.queued_frame,
                });
            }
            EmissiveVoxelScanReason::ConservativeChunk => {
                if state.priority.contains_key(&cell) {
                    return false;
                }
                state
                    .full
                    .get_or_insert_with(|| FullChunkScan {
                        dependency: work.dependency,
                        queued_frame: work.queued_frame,
                        cells: BTreeSet::new(),
                    })
                    .cells
                    .insert(cell);
            }
        }
        true
    }

    fn pop_priority(&mut self) -> Option<EmissiveVoxelScanWork> {
        let selected = self
            .chunks
            .iter()
            .filter_map(|(chunk, state)| {
                state
                    .priority
                    .iter()
                    .next()
                    .map(|(cell, pending)| (pending.queued_frame, *chunk, *cell))
            })
            .min()?;
        let (_, chunk, cell) = selected;
        let pending = self.chunks.get_mut(&chunk)?.priority.remove(&cell)?;
        Some(self.work(
            chunk,
            cell,
            pending.dependency,
            pending.queued_frame,
            EmissiveVoxelScanReason::RuntimeEdit,
        ))
    }

    fn pop_full(&mut self) -> Option<EmissiveVoxelScanWork> {
        let chunk_count = self.chunks.len();
        for offset in 0..chunk_count {
            let index = (self.full_chunk_cursor + offset) % chunk_count;
            let chunk = GridCoord::new(chunk_from_linear(index, self.chunk_dim));
            let state = self.chunks.get_mut(&chunk)?;
            let Some(full) = state.full.as_mut() else {
                continue;
            };
            let Some(cell) = full.cells.pop_first() else {
                state.full = None;
                continue;
            };
            let dependency = full.dependency;
            let queued_frame = full.queued_frame;
            if full.cells.is_empty() {
                state.full = None;
            }
            self.full_chunk_cursor = (index + 1) % chunk_count;
            return Some(self.work(
                chunk,
                cell,
                dependency,
                queued_frame,
                EmissiveVoxelScanReason::ConservativeChunk,
            ));
        }
        None
    }

    fn work(
        &self,
        chunk: GridCoord,
        cell: GridCoord,
        dependency: ContreeCpuVoxelSourceDependency,
        queued_frame: u64,
        reason: EmissiveVoxelScanReason,
    ) -> EmissiveVoxelScanWork {
        let min = cell.get() * self.cell_dim;
        EmissiveVoxelScanWork {
            chunk_idx: chunk.get(),
            cell_bound: UAabb3::new(min, min + self.cell_dim),
            dependency,
            queued_frame,
            reason,
        }
    }
}

fn grid_cells_intersecting(bound: UAabb3, cell_dim: UVec3) -> impl Iterator<Item = GridCoord> {
    let first = bound.min() / cell_dim;
    let last = (bound.max() - UVec3::ONE) / cell_dim;
    (first.z..=last.z).flat_map(move |z| {
        (first.y..=last.y)
            .flat_map(move |y| (first.x..=last.x).map(move |x| GridCoord::new(UVec3::new(x, y, z))))
    })
}

fn cells_in_chunk(
    chunk_idx: UVec3,
    voxel_dim_per_chunk: UVec3,
    cell_dim: UVec3,
) -> BTreeSet<GridCoord> {
    let min = chunk_idx * voxel_dim_per_chunk;
    let max = min + voxel_dim_per_chunk;
    grid_cells_intersecting(UAabb3::new(min, max), cell_dim).collect()
}

fn chunk_from_linear(index: usize, chunk_dim: UVec3) -> UVec3 {
    let index = index as u32;
    let x = index % chunk_dim.x;
    let yz = index / chunk_dim.x;
    let z = yz % chunk_dim.z;
    let y = yz / chunk_dim.z;
    UVec3::new(x, y, z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::ContreeCpuVoxelSourceDependency;
    use glam::UVec3;

    fn scheduler() -> EmissiveVoxelScanScheduler {
        EmissiveVoxelScanScheduler::new(UVec3::ONE, UVec3::splat(256), UVec3::splat(16)).unwrap()
    }

    fn dependency(revision: u64) -> ContreeCpuVoxelSourceDependency {
        dependency_for(UVec3::ZERO, revision)
    }

    fn dependency_for(chunk_idx: UVec3, revision: u64) -> ContreeCpuVoxelSourceDependency {
        ContreeCpuVoxelSourceDependency {
            chunk_idx,
            source_revision: Some(revision),
            is_present: true,
        }
    }

    #[test]
    fn initial_dependency_without_trusted_bound_schedules_one_progressive_chunk_scan() {
        let mut scheduler = scheduler();

        scheduler.observe_dependency(dependency(1), 0).unwrap();

        assert_eq!(scheduler.backlog().full_cells, 4_096);
        assert_eq!(scheduler.backlog().priority_cells, 0);
        let work = scheduler.take_work(2);
        assert_eq!(work.len(), 2);
        assert!(work
            .iter()
            .all(|work| work.reason == EmissiveVoxelScanReason::ConservativeChunk));
        assert!(work
            .iter()
            .all(|work| work.dependency.source_revision == Some(1)));
    }

    #[test]
    fn trusted_local_edit_schedules_only_its_intersecting_cell() {
        let mut scheduler = scheduler();
        scheduler.observe_dependency(dependency(1), 0).unwrap();
        assert_eq!(scheduler.take_work(4_096).len(), 4_096);
        let edited = crate::geom::UAabb3::new(UVec3::new(33, 2, 3), UVec3::new(34, 3, 4));

        assert_eq!(scheduler.mark_trusted_change(edited, 10).unwrap(), 1);
        scheduler.observe_dependency(dependency(2), 12).unwrap();

        assert_eq!(scheduler.backlog().priority_cells, 1);
        assert_eq!(scheduler.backlog().full_cells, 0);
        let work = scheduler.take_work(64);
        assert_eq!(work.len(), 1);
        assert_eq!(work[0].reason, EmissiveVoxelScanReason::RuntimeEdit);
        assert_eq!(work[0].cell_bound.min(), UVec3::new(32, 0, 0));
        assert_eq!(work[0].cell_bound.max(), UVec3::new(48, 16, 16));
        assert_eq!(work[0].dependency.source_revision, Some(2));
        assert_eq!(work[0].queued_frame, 10);
    }

    #[test]
    fn local_edit_preempts_initial_backlog_but_full_scan_still_advances() {
        let mut scheduler = scheduler();
        scheduler.observe_dependency(dependency(1), 0).unwrap();
        assert_eq!(
            scheduler.take_work(1)[0].reason,
            EmissiveVoxelScanReason::ConservativeChunk
        );
        let edited = crate::geom::UAabb3::new(UVec3::splat(240), UVec3::splat(241));
        scheduler.mark_trusted_change(edited, 3).unwrap();
        scheduler.observe_dependency(dependency(2), 4).unwrap();

        let priority = scheduler.take_work(1);
        assert_eq!(priority[0].reason, EmissiveVoxelScanReason::RuntimeEdit);
        assert_eq!(priority[0].dependency.source_revision, Some(2));
        let fair_progress = scheduler.take_work(1);
        assert_eq!(
            fair_progress[0].reason,
            EmissiveVoxelScanReason::ConservativeChunk
        );
        assert_eq!(fair_progress[0].dependency.source_revision, Some(2));
    }

    #[test]
    fn dependency_change_without_trusted_bound_conservatively_rescans_whole_chunk() {
        let mut scheduler = scheduler();
        scheduler.observe_dependency(dependency(1), 0).unwrap();
        assert_eq!(scheduler.take_work(4_096).len(), 4_096);

        scheduler.observe_dependency(dependency(2), 20).unwrap();

        assert_eq!(scheduler.backlog().full_cells, 4_096);
        assert_eq!(scheduler.backlog().priority_cells, 0);
    }

    #[test]
    fn stale_retry_cannot_replace_newer_priority_work() {
        let mut scheduler = scheduler();
        scheduler
            .mark_trusted_change(crate::geom::UAabb3::new(UVec3::ZERO, UVec3::ONE), 1)
            .unwrap();
        scheduler.observe_dependency(dependency(1), 2).unwrap();
        let stale = scheduler.take_work(1).pop().unwrap();
        scheduler
            .mark_trusted_change(crate::geom::UAabb3::new(UVec3::ZERO, UVec3::ONE), 3)
            .unwrap();
        scheduler.observe_dependency(dependency(2), 4).unwrap();

        assert!(!scheduler.retry(stale));
        let current = scheduler.take_work(1).pop().unwrap();
        assert_eq!(current.dependency.source_revision, Some(2));
        assert_eq!(current.queued_frame, 3);
    }

    #[test]
    fn conservative_backlog_round_robins_chunks_and_full_bound_uses_full_queue() {
        let mut scheduler = EmissiveVoxelScanScheduler::new(
            UVec3::new(2, 1, 1),
            UVec3::splat(32),
            UVec3::splat(16),
        )
        .unwrap();
        scheduler
            .observe_dependency(dependency_for(UVec3::ZERO, 1), 0)
            .unwrap();
        scheduler
            .observe_dependency(dependency_for(UVec3::X, 1), 0)
            .unwrap();

        let first = scheduler.take_work(4);
        assert_eq!(
            first.iter().map(|work| work.chunk_idx).collect::<Vec<_>>(),
            vec![UVec3::ZERO, UVec3::X, UVec3::ZERO, UVec3::X]
        );

        let whole_second_chunk =
            crate::geom::UAabb3::new(UVec3::new(32, 0, 0), UVec3::new(64, 32, 32));
        assert_eq!(
            scheduler
                .mark_trusted_change(whole_second_chunk, 10)
                .unwrap(),
            8
        );
        scheduler
            .observe_dependency(dependency_for(UVec3::X, 2), 11)
            .unwrap();
        assert_eq!(
            scheduler.chunks[&GridCoord::new(UVec3::X)].priority.len(),
            0
        );
        assert_eq!(
            scheduler.chunks[&GridCoord::new(UVec3::X)]
                .full
                .as_ref()
                .unwrap()
                .cells
                .len(),
            8
        );
    }
}
