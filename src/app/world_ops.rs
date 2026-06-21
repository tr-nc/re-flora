use crate::app::world_edits::{BuildEdit, VoxelEdit, WorldBuildBackend, WorldEditPlan};
use crate::builder::{
    ContreeBuildJob, ContreeBuilder, PlainBuilder, SceneAccelBuilder, SurfaceBuilder,
    VOXEL_TYPE_CHERRY_WOOD,
};
use crate::flora::species::{FloraPaintBrushSettings, FloraPaintSelection};
use crate::geom::UAabb3;
use crate::util::BENCH;
use anyhow::Result;
use glam::{UVec3, Vec3};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
pub(crate) struct FloraBrushEdit {
    pub(crate) start: Vec3,
    pub(crate) end: Vec3,
    pub(crate) radius: f32,
    pub(crate) tick: u32,
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
    failed: bool,
}

struct PendingDirectContree {
    record_index: usize,
    job: ContreeBuildJob,
}

fn finish_pending_direct_contree(
    contree_builder: &mut ContreeBuilder,
    pending: &mut Option<PendingDirectContree>,
    records: &mut [DirectChunkRebuildRecord],
) {
    let Some(pending_job) = pending.take() else {
        return;
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
        }
        Err(err) => {
            record.failed = true;
            log::error!(
                "Failed to finish contree for chunk {} after pipelined submit: {}",
                record.chunk_id,
                err
            );
        }
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

struct BuilderOnlyWorldBackend<'a> {
    plain_builder: &'a mut PlainBuilder,
    surface_builder: &'a mut SurfaceBuilder,
    contree_builder: &'a mut ContreeBuilder,
    scene_accel_builder: &'a mut SceneAccelBuilder,
    voxel_dim_per_chunk: UVec3,
}

impl WorldBuildBackend for BuilderOnlyWorldBackend<'_> {
    fn apply_voxel_edit(&mut self, edit: VoxelEdit) -> Result<()> {
        apply_voxel_edit(self.plain_builder, edit)
    }

    fn apply_build_edit(&mut self, edit: BuildEdit) -> Result<()> {
        apply_build_edit(
            self.surface_builder,
            self.contree_builder,
            self.scene_accel_builder,
            self.voxel_dim_per_chunk,
            edit,
        )
    }
}

pub(crate) fn apply_voxel_edit(plain_builder: &mut PlainBuilder, edit: VoxelEdit) -> Result<()> {
    match edit {
        VoxelEdit::ClearVoxelRegion(edit) => plain_builder.chunk_init(edit.offset, edit.dim),
        VoxelEdit::StampRoundCones {
            bvh_nodes,
            round_cones,
            voxel_type,
        } => {
            if voxel_type == VOXEL_TYPE_CHERRY_WOOD {
                plain_builder.chunk_modify(&bvh_nodes, &round_cones)
            } else {
                plain_builder.chunk_modify_with_voxel_type(&bvh_nodes, &round_cones, voxel_type)
            }
        }
        VoxelEdit::StampCuboids {
            bvh_nodes,
            cuboids,
            voxel_type,
        } => {
            if voxel_type == VOXEL_TYPE_CHERRY_WOOD {
                plain_builder.chunk_modify_cuboids(&bvh_nodes, &cuboids)
            } else {
                plain_builder.chunk_modify_cuboids_with_voxel_type(&bvh_nodes, &cuboids, voxel_type)
            }
        }
        VoxelEdit::StampSurfaceSpheres {
            bvh_nodes,
            spheres,
            voxel_type,
        } => plain_builder
            .chunk_modify_surface_spheres_with_voxel_type(
                &bvh_nodes, &spheres, voxel_type, None, None, None,
            )
            .map(|_| ()),
    }
}

pub(crate) fn apply_build_edit(
    surface_builder: &mut SurfaceBuilder,
    contree_builder: &mut ContreeBuilder,
    scene_accel_builder: &mut SceneAccelBuilder,
    voxel_dim_per_chunk: UVec3,
    edit: BuildEdit,
) -> Result<()> {
    match edit {
        BuildEdit::RebuildMesh(bound) => mesh_generate(
            surface_builder,
            contree_builder,
            scene_accel_builder,
            voxel_dim_per_chunk,
            bound,
        ),
        BuildEdit::RebuildChunks(chunk_ids) => mesh_generate_chunks(
            surface_builder,
            contree_builder,
            scene_accel_builder,
            voxel_dim_per_chunk,
            chunk_ids,
        ),
    }
}

pub(crate) fn execute_edit_plan_on_builders(
    plain_builder: &mut PlainBuilder,
    surface_builder: &mut SurfaceBuilder,
    contree_builder: &mut ContreeBuilder,
    scene_accel_builder: &mut SceneAccelBuilder,
    voxel_dim_per_chunk: UVec3,
    plan: WorldEditPlan,
) -> Result<()> {
    let mut backend = BuilderOnlyWorldBackend {
        plain_builder,
        surface_builder,
        contree_builder,
        scene_accel_builder,
        voxel_dim_per_chunk,
    };
    execute_edit_plan_on_backend(&mut backend, plan)
}

pub(crate) fn execute_edit_plan_on_backend<B: WorldBuildBackend>(
    backend: &mut B,
    plan: WorldEditPlan,
) -> Result<()> {
    for edit in plan.voxel_edits {
        backend.apply_voxel_edit(edit)?;
    }

    for edit in plan.build_edits {
        backend.apply_build_edit(edit)?;
    }

    Ok(())
}

pub(crate) fn mesh_generate(
    surface_builder: &mut SurfaceBuilder,
    contree_builder: &mut ContreeBuilder,
    scene_accel_builder: &mut SceneAccelBuilder,
    voxel_dim_per_chunk: UVec3,
    bound: UAabb3,
) -> Result<()> {
    mesh_generate_chunks(
        surface_builder,
        contree_builder,
        scene_accel_builder,
        voxel_dim_per_chunk,
        affected_chunk_indices_for_bound(bound, voxel_dim_per_chunk),
    )
}

pub(crate) fn mesh_generate_chunks(
    surface_builder: &mut SurfaceBuilder,
    contree_builder: &mut ContreeBuilder,
    scene_accel_builder: &mut SceneAccelBuilder,
    voxel_dim_per_chunk: UVec3,
    chunk_ids: Vec<UVec3>,
) -> Result<()> {
    let rebuild_start = Instant::now();
    let chunk_count = chunk_ids.len();
    if chunk_count > 1 {
        log::debug!(
            "[QUEUE][DIRECT_MULTI_REBUILD] rebuilding {} chunks synchronously in one call with pipelined contree waits: {:?}",
            chunk_count,
            chunk_ids,
        );
    }

    let mut records = Vec::with_capacity(chunk_count);
    let mut pending_contree: Option<PendingDirectContree> = None;
    let mut contree_skipped_count = 0;
    let mut surface_total = Duration::ZERO;

    for chunk_id in chunk_ids {
        let atlas_offset = chunk_id * voxel_dim_per_chunk;

        let surface_start = Instant::now();
        let active_voxel_len = match surface_builder.build_surface(chunk_id, true) {
            Ok(active_voxel_len) => active_voxel_len,
            Err(e) => {
                finish_pending_direct_contree(contree_builder, &mut pending_contree, &mut records);
                log::error!("Failed to build surface for chunk {}: {}", chunk_id, e);
                continue;
            }
        };

        let surface_elapsed = surface_start.elapsed();
        surface_total += surface_elapsed;
        BENCH
            .lock()
            .unwrap()
            .record("build_surface", surface_elapsed);

        let record_index = records.len();
        records.push(DirectChunkRebuildRecord {
            chunk_id,
            active_voxel_len,
            surface_elapsed,
            contree_elapsed: Duration::ZERO,
            scene_elapsed: Duration::ZERO,
            scene_offsets: None,
            contree_skipped: active_voxel_len == 0,
            contree_done: false,
            failed: false,
        });

        // The surface build submitted after the previous contree build runs on the same queue.
        // Once it has returned, the previous contree GPU job should usually already be satisfied;
        // finishing it here avoids an extra blocking wait between every pair of chunks.
        finish_pending_direct_contree(contree_builder, &mut pending_contree, &mut records);

        if active_voxel_len == 0 {
            contree_skipped_count += 1;
            let contree_start = Instant::now();
            let contree = contree_builder.clear_empty_surface_chunk(atlas_offset);
            let contree_elapsed = contree_start.elapsed();
            let record = &mut records[record_index];
            record.contree_elapsed = contree_elapsed;
            record.scene_offsets = contree.scene_offsets;
            record.contree_done = true;
            BENCH
                .lock()
                .unwrap()
                .record("build_and_alloc", contree_elapsed);
            continue;
        }

        let contree_submit_start = Instant::now();
        match contree_builder.submit_build_and_alloc(atlas_offset) {
            Ok(job) => {
                records[record_index].contree_elapsed += contree_submit_start.elapsed();
                pending_contree = Some(PendingDirectContree { record_index, job });
            }
            Err(e) => {
                records[record_index].failed = true;
                log::error!("Failed to submit contree for chunk {}: {}", chunk_id, e);
            }
        }
    }

    finish_pending_direct_contree(contree_builder, &mut pending_contree, &mut records);

    let mut rebuilt_chunk_count = 0;
    let mut scene_total = Duration::ZERO;
    for record in &mut records {
        if record.failed || !record.contree_done {
            continue;
        }

        let scene_start = Instant::now();
        if let Some((node_buffer_offset, leaf_buffer_offset)) = record.scene_offsets {
            scene_accel_builder.update_scene_tex(
                record.chunk_id,
                Some((node_buffer_offset, leaf_buffer_offset)),
            )?;
        } else {
            scene_accel_builder.update_scene_tex(record.chunk_id, None)?;
            log::debug!("Cleared scene tex because the chunk is empty");
        }
        record.scene_elapsed = scene_start.elapsed();
        scene_total += record.scene_elapsed;
        rebuilt_chunk_count += 1;

        log_direct_chunk_rebuild_record(record);
    }

    let contree_total = records.iter().fold(Duration::ZERO, |total, record| {
        total + record.contree_elapsed
    });

    log::debug!(
        "[PERF][MESH_REBUILD] chunks {} rebuilt {} total {:.2}ms surface {:.2}ms contree {:.2}ms scene_tex {:.2}ms contree_skipped {}",
        chunk_count,
        rebuilt_chunk_count,
        rebuild_start.elapsed().as_secs_f32() * 1000.0,
        surface_total.as_secs_f32() * 1000.0,
        contree_total.as_secs_f32() * 1000.0,
        scene_total.as_secs_f32() * 1000.0,
        contree_skipped_count,
    );

    Ok(())
}

#[allow(dead_code)]
pub(crate) fn mesh_generate_preserve_flora_for_brush_edit(
    surface_builder: &mut SurfaceBuilder,
    contree_builder: &mut ContreeBuilder,
    scene_accel_builder: &mut SceneAccelBuilder,
    voxel_dim_per_chunk: UVec3,
    bound: UAabb3,
    flora_edit: FloraBrushEdit,
) -> Result<()> {
    let affected_chunk_indices =
        get_affected_chunk_indices(bound.min(), bound.max(), voxel_dim_per_chunk);
    if affected_chunk_indices.len() > 1 {
        log::debug!(
            "[QUEUE][DIRECT_MULTI_REBUILD] preserve-flora brush edit rebuilding {} chunks synchronously: {:?}",
            affected_chunk_indices.len(),
            affected_chunk_indices,
        );
    }

    for chunk_id in affected_chunk_indices {
        mesh_generate_chunk_preserve_flora_for_brush_edit(
            surface_builder,
            contree_builder,
            scene_accel_builder,
            voxel_dim_per_chunk,
            chunk_id,
            flora_edit,
        )?;
    }

    Ok(())
}

#[allow(dead_code)]
pub(crate) fn mesh_generate_chunk_preserve_flora_for_brush_edit(
    surface_builder: &mut SurfaceBuilder,
    contree_builder: &mut ContreeBuilder,
    scene_accel_builder: &mut SceneAccelBuilder,
    voxel_dim_per_chunk: UVec3,
    chunk_id: UVec3,
    flora_edit: FloraBrushEdit,
) -> Result<()> {
    let atlas_offset = chunk_id * voxel_dim_per_chunk;

    let surface_start = Instant::now();
    let active_voxel_len = match surface_builder.build_surface(chunk_id, false) {
        Ok(active_voxel_len) => active_voxel_len,
        Err(e) => {
            log::error!("Failed to build surface for chunk {}: {}", chunk_id, e);
            return Ok(());
        }
    };
    let surface_elapsed = surface_start.elapsed();
    BENCH
        .lock()
        .unwrap()
        .record("build_surface", surface_elapsed);

    surface_builder.edit_flora_instances_for_brush(
        chunk_id,
        flora_edit.start,
        flora_edit.end,
        flora_edit.radius,
        flora_edit.tick,
    )?;

    let contree_start = Instant::now();
    let scene_offsets = if active_voxel_len == 0 {
        contree_builder
            .clear_empty_surface_chunk(atlas_offset)
            .scene_offsets
    } else {
        contree_builder.build_and_alloc(atlas_offset)?
    };
    let contree_elapsed = contree_start.elapsed();
    BENCH
        .lock()
        .unwrap()
        .record("build_and_alloc", contree_elapsed);

    if let Some(res) = scene_offsets {
        let (node_buffer_offset, leaf_buffer_offset) = res;
        scene_accel_builder
            .update_scene_tex(chunk_id, Some((node_buffer_offset, leaf_buffer_offset)))?;
    } else {
        scene_accel_builder.update_scene_tex(chunk_id, None)?;
        log::debug!("Cleared scene tex because the chunk is empty");
    }

    Ok(())
}

pub(crate) fn mesh_regenerate_flora_for_brush_edit(
    surface_builder: &mut SurfaceBuilder,
    voxel_dim_per_chunk: UVec3,
    bound: UAabb3,
    flora_edit: FloraBrushEdit,
    paint_selection: FloraPaintSelection,
    paint_dab_serial: u32,
    paint_brush: FloraPaintBrushSettings,
) -> Result<()> {
    let affected_chunk_indices =
        get_affected_chunk_indices(bound.min(), bound.max(), voxel_dim_per_chunk);

    for chunk_id in affected_chunk_indices {
        let now = Instant::now();
        let res = surface_builder.build_surface(chunk_id, false);
        if let Err(e) = res {
            log::error!("Failed to build surface for chunk {}: {}", chunk_id, e);
            continue;
        }
        BENCH.lock().unwrap().record("build_surface", now.elapsed());

        let _regen_stats = surface_builder.regenerate_flora_instances_for_brush(
            chunk_id,
            flora_edit.start,
            flora_edit.end,
            flora_edit.radius,
            flora_edit.tick,
            paint_selection,
            paint_dab_serial,
            paint_brush,
        )?;
    }

    Ok(())
}

pub(crate) fn mesh_remove_flora_for_brush_edit(
    surface_builder: &mut SurfaceBuilder,
    voxel_dim_per_chunk: UVec3,
    bound: UAabb3,
    flora_edit: FloraBrushEdit,
) -> Result<()> {
    let affected_chunk_indices =
        get_affected_chunk_indices(bound.min(), bound.max(), voxel_dim_per_chunk);

    for chunk_id in affected_chunk_indices {
        let now = Instant::now();
        let res = surface_builder.build_surface(chunk_id, false);
        if let Err(e) = res {
            log::error!("Failed to build surface for chunk {}: {}", chunk_id, e);
            continue;
        }
        BENCH.lock().unwrap().record("build_surface", now.elapsed());

        surface_builder.edit_flora_instances_for_brush(
            chunk_id,
            flora_edit.start,
            flora_edit.end,
            flora_edit.radius,
            flora_edit.tick,
        )?;
    }

    Ok(())
}

pub(crate) fn mesh_trim_flora_for_brush_edit(
    surface_builder: &mut SurfaceBuilder,
    voxel_dim_per_chunk: UVec3,
    bound: UAabb3,
    flora_edit: FloraBrushEdit,
    target_age: u32,
) -> Result<Vec<UVec3>> {
    let affected_chunk_indices =
        get_affected_chunk_indices(bound.min(), bound.max(), voxel_dim_per_chunk);
    let mut growing_chunks = Vec::new();

    for chunk_id in affected_chunk_indices {
        let now = Instant::now();
        let res = surface_builder.build_surface(chunk_id, false);
        if let Err(e) = res {
            log::error!("Failed to build surface for chunk {}: {}", chunk_id, e);
            continue;
        }
        BENCH.lock().unwrap().record("build_surface", now.elapsed());

        let regen_stats = surface_builder.trim_flora_instances_for_brush(
            chunk_id,
            flora_edit.start,
            flora_edit.end,
            flora_edit.radius,
            flora_edit.tick,
            target_age,
        )?;
        if regen_stats.has_growing_flora {
            growing_chunks.push(chunk_id);
        }
    }

    Ok(growing_chunks)
}

pub(crate) fn affected_chunk_indices_for_bound(
    bound: UAabb3,
    voxel_dim_per_chunk: UVec3,
) -> Vec<UVec3> {
    if !bound.has_size() {
        return Vec::new();
    }

    get_affected_chunk_indices(bound.min(), bound.max(), voxel_dim_per_chunk)
}

fn get_affected_chunk_indices(
    min_bound: UVec3,
    max_bound: UVec3,
    voxel_dim_per_chunk: UVec3,
) -> Vec<UVec3> {
    let min_chunk_idx = min_bound / voxel_dim_per_chunk;
    let max_chunk_idx = max_bound / voxel_dim_per_chunk;

    let mut affected = Vec::new();
    for x in min_chunk_idx.x..=max_chunk_idx.x {
        for y in min_chunk_idx.y..=max_chunk_idx.y {
            for z in min_chunk_idx.z..=max_chunk_idx.z {
                affected.push(UVec3::new(x, y, z));
            }
        }
    }
    affected
}
