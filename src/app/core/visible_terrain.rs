use super::*;
use std::collections::HashSet;

#[derive(Clone, Debug)]
enum VisibleTerrainRebuild {
    BuildEdits(Vec<BuildEdit>),
    PreserveFlora {
        bound: UAabb3,
        flora_edit: world_ops::FloraBrushEdit,
    },
}

/// A semantic request to make one authoritative terrain change fully visible.
///
/// The fields stay private so callers cannot select builder stages or downstream observers.
#[derive(Clone, Debug)]
pub(super) struct VisibleTerrainChange {
    rebuild: VisibleTerrainRebuild,
    affected_voxels: UAabb3,
    terrain_changed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PublishedTerrain {
    pub(super) revision: Option<u32>,
    pub(super) rebuilt_chunks: usize,
}

impl VisibleTerrainChange {
    pub(super) fn from_build_edits(build_edits: Vec<BuildEdit>) -> Result<Option<Self>> {
        let mut affected_voxels: Option<UAabb3> = None;
        for edit in &build_edits {
            let edit_bound = build_edit_bound(edit)?;
            affected_voxels = Some(match affected_voxels {
                Some(bound) => bound.union_with(&edit_bound),
                None => edit_bound,
            });
        }
        let Some(affected_voxels) = affected_voxels else {
            return Ok(None);
        };
        Ok(Some(Self {
            rebuild: VisibleTerrainRebuild::BuildEdits(build_edits),
            affected_voxels,
            terrain_changed: true,
        }))
    }

    pub(super) fn tree_chunks(chunk_ids: Vec<UVec3>) -> Result<Self> {
        Self::from_build_edits(vec![BuildEdit::RebuildChunksWithoutFlora(chunk_ids)])?
            .context("tree publication requires at least one affected chunk")
    }

    pub(super) fn preserving_flora(
        bound: UAabb3,
        flora_edit: world_ops::FloraBrushEdit,
        terrain_changed: bool,
    ) -> Self {
        Self {
            rebuild: VisibleTerrainRebuild::PreserveFlora { bound, flora_edit },
            affected_voxels: bound,
            terrain_changed,
        }
    }

    fn affected_chunks(&self) -> Result<Vec<UVec3>> {
        let mut chunk_ids = match &self.rebuild {
            VisibleTerrainRebuild::BuildEdits(build_edits) => {
                let mut chunk_ids = Vec::new();
                for edit in build_edits {
                    chunk_ids.extend(build_edit_chunks(edit)?);
                }
                chunk_ids
            }
            VisibleTerrainRebuild::PreserveFlora { bound, .. } => {
                world_ops::affected_chunk_indices_for_bound(*bound, VOXEL_DIM_PER_CHUNK)
            }
        };
        let mut seen = HashSet::new();
        chunk_ids.retain(|chunk_id| seen.insert(*chunk_id));
        anyhow::ensure!(
            !chunk_ids.is_empty(),
            "visible terrain publication requires at least one affected chunk"
        );
        Ok(chunk_ids)
    }
}

impl App {
    pub(super) fn publish_visible_terrain(
        &mut self,
        change: VisibleTerrainChange,
    ) -> Result<PublishedTerrain> {
        let chunk_ids = change.affected_chunks()?;
        self.prepare_visible_terrain_publication(&chunk_ids)?;
        let started_at = Instant::now();

        match change.rebuild {
            VisibleTerrainRebuild::BuildEdits(build_edits) => {
                for edit in build_edits {
                    world_ops::apply_build_edit(
                        &mut self.surface_builder,
                        &mut self.contree_builder,
                        &mut self.scene_accel_builder,
                        VOXEL_DIM_PER_CHUNK,
                        edit,
                    )?;
                }
            }
            VisibleTerrainRebuild::PreserveFlora { bound, flora_edit } => {
                world_ops::mesh_generate_preserve_flora_for_brush_edit(
                    &mut self.surface_builder,
                    &mut self.contree_builder,
                    &mut self.scene_accel_builder,
                    VOXEL_DIM_PER_CHUNK,
                    bound,
                    flora_edit,
                )?;
            }
        }

        self.request_vsm_history_reset();
        let revision =
            next_visible_terrain_revision(self.visible_terrain_revision, change.terrain_changed);
        if let Some(revision) = revision {
            self.terrain_physics
                .mark_terrain_chunks_dirty(&chunk_ids, VOXEL_DIM_PER_CHUNK);
            self.tracer
                .observe_published_environment_probe_terrain(revision, change.affected_voxels)?;
            self.visible_terrain_revision = revision;
        }

        log::info!(
            "[PERF][VISIBLE_TERRAIN_PUBLICATION] chunks={} terrain_changed={} revision={:?} elapsed_ms={:.2}",
            chunk_ids.len(),
            change.terrain_changed,
            revision,
            started_at.elapsed().as_secs_f64() * 1000.0,
        );
        Ok(PublishedTerrain {
            revision,
            rebuilt_chunks: chunk_ids.len(),
        })
    }

    fn prepare_visible_terrain_publication(&mut self, chunk_ids: &[UVec3]) -> Result<()> {
        if self.terrain_chunk_rebuild_inflight.is_some() {
            anyhow::ensure!(
                self.finish_deferred_chunk_rebuild_blocking(),
                "failed to finish the previous visible terrain publication"
            );
        }
        for &chunk_id in chunk_ids {
            self.deferred_chunk_rebuilds.clear(chunk_id);
        }
        Ok(())
    }
}

fn build_edit_chunks(edit: &BuildEdit) -> Result<Vec<UVec3>> {
    let chunk_ids = match edit {
        BuildEdit::RebuildMesh(bound) | BuildEdit::RebuildMeshWithoutFlora(bound) => {
            world_ops::affected_chunk_indices_for_bound(*bound, VOXEL_DIM_PER_CHUNK)
        }
        BuildEdit::RebuildChunks(chunk_ids) | BuildEdit::RebuildChunksWithoutFlora(chunk_ids) => {
            chunk_ids.clone()
        }
    };
    anyhow::ensure!(
        !chunk_ids.is_empty(),
        "visible terrain build edit requires at least one affected chunk"
    );
    Ok(chunk_ids)
}

fn build_edit_bound(edit: &BuildEdit) -> Result<UAabb3> {
    match edit {
        BuildEdit::RebuildMesh(bound) | BuildEdit::RebuildMeshWithoutFlora(bound) => Ok(*bound),
        BuildEdit::RebuildChunks(chunk_ids) | BuildEdit::RebuildChunksWithoutFlora(chunk_ids) => {
            let min_chunk = chunk_ids
                .iter()
                .copied()
                .reduce(UVec3::min)
                .context("visible terrain build edit requires at least one affected chunk")?;
            let max_chunk = chunk_ids
                .iter()
                .copied()
                .reduce(UVec3::max)
                .expect("a minimum chunk implies a maximum chunk");
            Ok(UAabb3::new(
                min_chunk * VOXEL_DIM_PER_CHUNK,
                (max_chunk + UVec3::ONE) * VOXEL_DIM_PER_CHUNK,
            ))
        }
    }
}

fn next_visible_terrain_revision(current: u32, terrain_changed: bool) -> Option<u32> {
    terrain_changed.then(|| current.wrapping_add(1).max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_batch_has_one_combined_publication_bound_and_unique_chunks() {
        let first = UAabb3::new(UVec3::splat(8), UVec3::splat(16));
        let change = VisibleTerrainChange::from_build_edits(vec![
            BuildEdit::RebuildMesh(first),
            BuildEdit::RebuildChunks(vec![UVec3::new(1, 0, 0), UVec3::new(1, 0, 0)]),
        ])
        .unwrap()
        .unwrap();

        assert_eq!(
            change.affected_voxels,
            first.union_with(&UAabb3::new(
                UVec3::new(1, 0, 0) * VOXEL_DIM_PER_CHUNK,
                UVec3::new(2, 1, 1) * VOXEL_DIM_PER_CHUNK,
            ))
        );
        let chunks = change.affected_chunks().unwrap();
        assert_eq!(
            chunks
                .iter()
                .filter(|&&id| id == UVec3::new(1, 0, 0))
                .count(),
            1
        );
    }

    #[test]
    fn terrain_revision_advances_only_for_a_complete_terrain_change() {
        assert_eq!(next_visible_terrain_revision(7, true), Some(8));
        assert_eq!(next_visible_terrain_revision(u32::MAX, true), Some(1));
        assert_eq!(next_visible_terrain_revision(7, false), None);
    }

    #[test]
    fn empty_build_batch_is_not_a_publication() {
        assert!(VisibleTerrainChange::from_build_edits(Vec::new())
            .unwrap()
            .is_none());
        assert!(
            VisibleTerrainChange::from_build_edits(vec![BuildEdit::RebuildChunks(Vec::new())])
                .is_err()
        );
    }
}
