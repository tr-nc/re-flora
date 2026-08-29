use crate::builder::SurfaceBuilder;
use crate::flora::species::{FloraPaintBrushSettings, FloraPaintSelection};
use crate::geom::UAabb3;
use crate::util::BENCH;
use anyhow::Result;
use glam::{UVec3, Vec3};
use std::time::Instant;

#[derive(Clone, Copy, Debug)]
pub(crate) struct FloraBrushEdit {
    pub(crate) start: Vec3,
    pub(crate) end: Vec3,
    pub(crate) radius: f32,
    pub(crate) tick: u32,
    pub(crate) spawn_time_ms: u32,
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
            flora_edit.spawn_time_ms,
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
            flora_edit.spawn_time_ms,
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
            flora_edit.spawn_time_ms,
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
