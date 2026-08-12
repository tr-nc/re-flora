use super::world_edits::BuildEdit;
use super::world_ops::{affected_chunk_indices_for_bound, FloraBrushEdit};
use crate::builder::{ContreeBuildJob, ContreeBuilder, SceneAccelBuilder, SurfaceBuilder};
use crate::geom::UAabb3;
use crate::util::BENCH;
use anyhow::{Context, Result};
use glam::UVec3;
use std::time::{Duration, Instant};

/// Concrete builder borrows used by one physical terrain publication operation.
///
/// This is a bundle of the existing Vulkan implementation, not an adapter or substitutable GPU
/// interface. The builders retain their independent owners and consumers.
pub(crate) struct PhysicalTerrainBuilders<'a> {
    surface_builder: &'a mut SurfaceBuilder,
    contree_builder: &'a mut ContreeBuilder,
    scene_accel_builder: &'a mut SceneAccelBuilder,
}

impl<'a> PhysicalTerrainBuilders<'a> {
    pub(crate) fn new(
        surface_builder: &'a mut SurfaceBuilder,
        contree_builder: &'a mut ContreeBuilder,
        scene_accel_builder: &'a mut SceneAccelBuilder,
    ) -> Self {
        Self {
            surface_builder,
            contree_builder,
            scene_accel_builder,
        }
    }

    fn reborrow(&mut self) -> PhysicalTerrainBuilders<'_> {
        PhysicalTerrainBuilders {
            surface_builder: &mut *self.surface_builder,
            contree_builder: &mut *self.contree_builder,
            scene_accel_builder: &mut *self.scene_accel_builder,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum PhysicalTerrainSurfaceMode {
    Rebuild { place_flora: bool },
    PreserveFlora(FloraBrushEdit),
}

impl PhysicalTerrainSurfaceMode {
    fn place_flora(self) -> bool {
        matches!(self, Self::Rebuild { place_flora: true })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PhysicalTerrainPublicationState {
    Preparing,
    Published,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PhysicalTerrainPublicationProgress {
    Preparing {
        prepared_chunks: usize,
        total_chunks: usize,
    },
    Published {
        chunks: usize,
    },
}

struct DirectChunkRebuildRecord {
    chunk_id: UVec3,
    active_voxel_len: u32,
    surface_elapsed: Duration,
    contree_elapsed: Duration,
    scene_elapsed: Duration,
    scene_offsets: Option<(u64, u64)>,
    contree_skipped: bool,
    contree_done: bool,
}

struct PendingDirectContree {
    record_index: usize,
    job: ContreeBuildJob,
}

/// One complete physical Visible Terrain Publication.
///
/// Runtime drives this deep module to completion synchronously. Loading advances the same
/// implementation once per frame. Surface -> Contree -> scene-texture ordering, the pending GPU
/// job, empty-chunk behavior, completeness, metrics, and terminal failure all remain private.
pub(crate) struct PhysicalTerrainPublication {
    chunk_ids: Vec<UVec3>,
    next_chunk_index: usize,
    mode: PhysicalTerrainSurfaceMode,
    voxel_dim_per_chunk: UVec3,
    records: Vec<DirectChunkRebuildRecord>,
    pending_contree: Option<PendingDirectContree>,
    state: PhysicalTerrainPublicationState,
    rebuild_start: Option<Instant>,
}

impl PhysicalTerrainPublication {
    pub(crate) fn from_build_edit(edit: BuildEdit, voxel_dim_per_chunk: UVec3) -> Result<Self> {
        let (chunk_ids, mode) = match edit {
            BuildEdit::RebuildMesh(bound) => (
                affected_chunk_indices_for_bound(bound, voxel_dim_per_chunk),
                PhysicalTerrainSurfaceMode::Rebuild { place_flora: true },
            ),
            BuildEdit::RebuildMeshWithoutFlora(bound) => (
                affected_chunk_indices_for_bound(bound, voxel_dim_per_chunk),
                PhysicalTerrainSurfaceMode::Rebuild { place_flora: false },
            ),
            BuildEdit::RebuildChunks(chunk_ids) => (
                chunk_ids,
                PhysicalTerrainSurfaceMode::Rebuild { place_flora: true },
            ),
            BuildEdit::RebuildChunksWithoutFlora(chunk_ids) => (
                chunk_ids,
                PhysicalTerrainSurfaceMode::Rebuild { place_flora: false },
            ),
        };
        Self::new(chunk_ids, mode, voxel_dim_per_chunk)
    }

    pub(crate) fn loading(chunk_ids: Vec<UVec3>, voxel_dim_per_chunk: UVec3) -> Result<Self> {
        Self::new(
            chunk_ids,
            PhysicalTerrainSurfaceMode::Rebuild { place_flora: false },
            voxel_dim_per_chunk,
        )
    }

    pub(crate) fn preserving_flora(
        bound: UAabb3,
        flora_edit: FloraBrushEdit,
        voxel_dim_per_chunk: UVec3,
    ) -> Result<Self> {
        Self::new(
            affected_chunk_indices_for_bound(bound, voxel_dim_per_chunk),
            PhysicalTerrainSurfaceMode::PreserveFlora(flora_edit),
            voxel_dim_per_chunk,
        )
    }

    fn new(
        chunk_ids: Vec<UVec3>,
        mode: PhysicalTerrainSurfaceMode,
        voxel_dim_per_chunk: UVec3,
    ) -> Result<Self> {
        anyhow::ensure!(
            !chunk_ids.is_empty(),
            "physical visible terrain publication requires at least one chunk"
        );
        if chunk_ids.len() > 1 {
            log::debug!(
                "[QUEUE][DIRECT_MULTI_REBUILD] preparing {} chunks with pipelined Contree waits: {:?}",
                chunk_ids.len(),
                chunk_ids,
            );
        }
        let chunk_count = chunk_ids.len();
        Ok(Self {
            chunk_ids,
            next_chunk_index: 0,
            mode,
            voxel_dim_per_chunk,
            records: Vec::with_capacity(chunk_count),
            pending_contree: None,
            state: PhysicalTerrainPublicationState::Preparing,
            rebuild_start: None,
        })
    }

    pub(crate) fn advance(
        &mut self,
        mut builders: PhysicalTerrainBuilders<'_>,
    ) -> Result<PhysicalTerrainPublicationProgress> {
        match self.state {
            PhysicalTerrainPublicationState::Published => {
                return Ok(PhysicalTerrainPublicationProgress::Published {
                    chunks: self.chunk_ids.len(),
                });
            }
            PhysicalTerrainPublicationState::Failed => {
                anyhow::bail!("physical visible terrain publication is terminally failed");
            }
            PhysicalTerrainPublicationState::Preparing => {}
        }

        let result = self.advance_preparing(&mut builders);
        if result.is_err() {
            if let Some(pending) = self.pending_contree.take() {
                builders
                    .contree_builder
                    .discard_build_and_alloc(pending.job);
            }
            self.state = PhysicalTerrainPublicationState::Failed;
        }
        result
    }

    pub(crate) fn run_to_completion(
        &mut self,
        mut builders: PhysicalTerrainBuilders<'_>,
    ) -> Result<usize> {
        loop {
            match self.advance(builders.reborrow())? {
                PhysicalTerrainPublicationProgress::Preparing { .. } => {}
                PhysicalTerrainPublicationProgress::Published { chunks } => return Ok(chunks),
            }
        }
    }

    pub(crate) fn abort(&mut self, contree_builder: &mut ContreeBuilder) {
        if let Some(pending) = self.pending_contree.take() {
            contree_builder.discard_build_and_alloc(pending.job);
        }
        if self.state == PhysicalTerrainPublicationState::Preparing {
            self.state = PhysicalTerrainPublicationState::Failed;
        }
    }

    fn advance_preparing(
        &mut self,
        builders: &mut PhysicalTerrainBuilders<'_>,
    ) -> Result<PhysicalTerrainPublicationProgress> {
        self.rebuild_start.get_or_insert_with(Instant::now);
        let chunk_id = self.chunk_ids[self.next_chunk_index];
        let atlas_offset = chunk_id * self.voxel_dim_per_chunk;
        let surface_start = Instant::now();
        let active_voxel_len = match builders
            .surface_builder
            .build_surface(chunk_id, self.mode.place_flora())
        {
            Ok(active_voxel_len) => active_voxel_len,
            Err(error) => {
                finish_pending_direct_contree(
                    builders.contree_builder,
                    &mut self.pending_contree,
                    &mut self.records,
                )?;
                return Err(error)
                    .with_context(|| format!("failed to build Surface for chunk {chunk_id}"));
            }
        };
        let surface_elapsed = surface_start.elapsed();
        BENCH
            .lock()
            .unwrap()
            .record("build_surface", surface_elapsed);

        if let PhysicalTerrainSurfaceMode::PreserveFlora(flora_edit) = self.mode {
            builders.surface_builder.edit_flora_instances_for_brush(
                chunk_id,
                flora_edit.start,
                flora_edit.end,
                flora_edit.radius,
                flora_edit.tick,
                flora_edit.spawn_time_ms,
            )?;
        }

        let record_index = self.records.len();
        self.records.push(DirectChunkRebuildRecord {
            chunk_id,
            active_voxel_len,
            surface_elapsed,
            contree_elapsed: Duration::ZERO,
            scene_elapsed: Duration::ZERO,
            scene_offsets: None,
            contree_skipped: active_voxel_len == 0,
            contree_done: false,
        });

        // A later Surface build runs behind the previous Contree job on the same queue. Finishing
        // that previous job here retains the one-job pipeline while keeping ownership private.
        finish_pending_direct_contree(
            builders.contree_builder,
            &mut self.pending_contree,
            &mut self.records,
        )?;

        if active_voxel_len == 0 {
            let contree_start = Instant::now();
            let contree = builders
                .contree_builder
                .clear_empty_surface_chunk(atlas_offset);
            let contree_elapsed = contree_start.elapsed();
            let record = &mut self.records[record_index];
            record.contree_elapsed = contree_elapsed;
            record.scene_offsets = contree.scene_offsets;
            record.contree_done = true;
            BENCH
                .lock()
                .unwrap()
                .record("build_and_alloc", contree_elapsed);
        } else if matches!(self.mode, PhysicalTerrainSurfaceMode::PreserveFlora(_)) {
            let contree_start = Instant::now();
            let scene_offsets = builders.contree_builder.build_and_alloc(atlas_offset)?;
            let contree_elapsed = contree_start.elapsed();
            let record = &mut self.records[record_index];
            record.contree_elapsed = contree_elapsed;
            record.scene_offsets = scene_offsets;
            record.contree_done = true;
            BENCH
                .lock()
                .unwrap()
                .record("build_and_alloc", contree_elapsed);
        } else {
            let contree_submit_start = Instant::now();
            let job = builders
                .contree_builder
                .submit_build_and_alloc(atlas_offset)
                .with_context(|| format!("failed to submit Contree for chunk {chunk_id}"))?;
            self.records[record_index].contree_elapsed += contree_submit_start.elapsed();
            self.pending_contree = Some(PendingDirectContree { record_index, job });
        }

        self.next_chunk_index += 1;
        if self.next_chunk_index < self.chunk_ids.len() {
            return Ok(PhysicalTerrainPublicationProgress::Preparing {
                prepared_chunks: self.next_chunk_index,
                total_chunks: self.chunk_ids.len(),
            });
        }

        finish_pending_direct_contree(
            builders.contree_builder,
            &mut self.pending_contree,
            &mut self.records,
        )?;
        self.publish_scene_records(builders.scene_accel_builder)?;
        self.state = PhysicalTerrainPublicationState::Published;
        Ok(PhysicalTerrainPublicationProgress::Published {
            chunks: self.chunk_ids.len(),
        })
    }

    fn publish_scene_records(&mut self, scene_accel_builder: &mut SceneAccelBuilder) -> Result<()> {
        anyhow::ensure!(
            self.records.len() == self.chunk_ids.len()
                && self.records.iter().all(|record| record.contree_done),
            "physical visible terrain publication was incomplete before Scene publication: prepared {} of {} chunks",
            self.records.iter().filter(|record| record.contree_done).count(),
            self.chunk_ids.len(),
        );

        let mut scene_total = Duration::ZERO;
        for record in &mut self.records {
            let scene_start = Instant::now();
            scene_accel_builder
                .update_scene_tex(record.chunk_id, record.scene_offsets)
                .with_context(|| {
                    format!(
                        "failed to publish Scene acceleration entry for chunk {}",
                        record.chunk_id
                    )
                })?;
            if record.scene_offsets.is_none() {
                log::debug!("Cleared scene tex because the chunk is empty");
            }
            record.scene_elapsed = scene_start.elapsed();
            scene_total += record.scene_elapsed;
            log_direct_chunk_rebuild_record(record);
        }

        let surface_total = self.records.iter().fold(Duration::ZERO, |total, record| {
            total + record.surface_elapsed
        });
        let contree_total = self.records.iter().fold(Duration::ZERO, |total, record| {
            total + record.contree_elapsed
        });
        let contree_skipped_count = self
            .records
            .iter()
            .filter(|record| record.contree_skipped)
            .count();
        log::debug!(
            "[PERF][MESH_REBUILD] chunks {} rebuilt {} total {:.2}ms surface {:.2}ms contree {:.2}ms scene_tex {:.2}ms contree_skipped {} place_flora {}",
            self.chunk_ids.len(),
            self.records.len(),
            self.rebuild_start
                .as_ref()
                .expect("physical publication started before Scene publication")
                .elapsed()
                .as_secs_f32()
                * 1000.0,
            surface_total.as_secs_f32() * 1000.0,
            contree_total.as_secs_f32() * 1000.0,
            scene_total.as_secs_f32() * 1000.0,
            contree_skipped_count,
            self.mode.place_flora(),
        );
        Ok(())
    }
}

fn finish_pending_direct_contree(
    contree_builder: &mut ContreeBuilder,
    pending: &mut Option<PendingDirectContree>,
    records: &mut [DirectChunkRebuildRecord],
) -> Result<()> {
    let Some(pending_job) = pending.take() else {
        return Ok(());
    };

    let finish_start = Instant::now();
    let job = pending_job.job;
    let contree_result = match contree_builder.wait_build_and_alloc(&job) {
        Ok(()) => contree_builder.finish_build_and_alloc(job),
        Err(err) => {
            contree_builder.discard_build_and_alloc(job);
            Err(err)
        }
    };
    let finish_elapsed = finish_start.elapsed();

    let record = &mut records[pending_job.record_index];
    record.contree_elapsed += finish_elapsed;

    match contree_result {
        Ok(contree) => {
            record.scene_offsets = contree.scene_offsets;
            record.contree_done = true;
            BENCH
                .lock()
                .unwrap()
                .record("build_and_alloc", record.contree_elapsed);
            Ok(())
        }
        Err(err) => Err(err).with_context(|| {
            format!(
                "failed to finish Contree publication for chunk {} after pipelined submit",
                record.chunk_id
            )
        }),
    }
}

fn log_direct_chunk_rebuild_record(record: &DirectChunkRebuildRecord) {
    log::debug!(
        "[PERF][MESH_REBUILD_CHUNK] chunk {:?} total {:.2}ms surface {:.2}ms contree {:.2}ms scene_tex {:.2}ms active_voxels {} contree_skipped {}",
        record.chunk_id,
        (record.surface_elapsed + record.contree_elapsed + record.scene_elapsed).as_secs_f32()
            * 1000.0,
        record.surface_elapsed.as_secs_f32() * 1000.0,
        record.contree_elapsed.as_secs_f32() * 1000.0,
        record.scene_elapsed.as_secs_f32() * 1000.0,
        record.active_voxel_len,
        record.contree_skipped,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publication_requires_work_before_any_builder_stage_can_run() {
        let error = PhysicalTerrainPublication::loading(Vec::new(), UVec3::splat(256))
            .err()
            .expect("an empty publication must be rejected");
        assert!(error.to_string().contains("requires at least one chunk"));
    }

    #[test]
    fn loading_request_starts_preparing_without_claiming_publication() {
        let chunks = vec![UVec3::ZERO, UVec3::X];
        let publication =
            PhysicalTerrainPublication::loading(chunks.clone(), UVec3::splat(256)).unwrap();

        assert_eq!(publication.chunk_ids, chunks);
        assert_eq!(publication.next_chunk_index, 0);
        assert_eq!(
            publication.state,
            PhysicalTerrainPublicationState::Preparing
        );
        assert!(publication.records.is_empty());
        assert!(publication.pending_contree.is_none());
        assert!(publication.rebuild_start.is_none());
    }
}
