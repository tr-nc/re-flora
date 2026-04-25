use crate::app::world_edits::{BuildEdit, VoxelEdit, WorldBuildBackend, WorldEditPlan};
use crate::builder::{
    ContreeBuilder, PlainBuilder, SceneAccelBuilder, SurfaceBuilder, VOXEL_TYPE_CHERRY_WOOD,
};
use crate::geom::UAabb3;
use crate::util::BENCH;
use anyhow::Result;
use glam::{UVec3, Vec3};
use std::time::Instant;

pub(crate) struct FloraSphereEdit {
    pub(crate) center: Vec3,
    pub(crate) radius: f32,
    pub(crate) tick: u32,
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
                &bvh_nodes, &spheres, voxel_type, None, None,
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
    let affected_chunk_indices =
        get_affected_chunk_indices(bound.min(), bound.max(), voxel_dim_per_chunk);

    for chunk_id in affected_chunk_indices {
        let atlas_offset = chunk_id * voxel_dim_per_chunk;

        let surface_start = Instant::now();
        let res = surface_builder.build_surface(chunk_id, true);
        if let Err(e) = res {
            log::error!("Failed to build surface for chunk {}: {}", chunk_id, e);
            continue;
        }

        let surface_elapsed = surface_start.elapsed();
        BENCH
            .lock()
            .unwrap()
            .record("build_surface", surface_elapsed);

        let contree_start = Instant::now();
        let res = contree_builder.build_and_alloc(atlas_offset).unwrap();
        let contree_elapsed = contree_start.elapsed();
        BENCH
            .lock()
            .unwrap()
            .record("build_and_alloc", contree_elapsed);

        let scene_start = Instant::now();
        if let Some(res) = res {
            let (node_buffer_offset, leaf_buffer_offset) = res;
            scene_accel_builder
                .update_scene_tex(chunk_id, Some((node_buffer_offset, leaf_buffer_offset)))?;
        } else {
            scene_accel_builder.update_scene_tex(chunk_id, None)?;
            log::debug!("Cleared scene tex because the chunk is empty");
        }
        let scene_elapsed = scene_start.elapsed();

        log::info!(
            "[TERRAIN_EDIT] chunk {:?} rebuild surface={:.3}ms contree={:.3}ms scene_tex={:.3}ms",
            chunk_id,
            surface_elapsed.as_secs_f64() * 1000.0,
            contree_elapsed.as_secs_f64() * 1000.0,
            scene_elapsed.as_secs_f64() * 1000.0,
        );
    }

    Ok(())
}

pub(crate) fn mesh_generate_preserve_flora_for_sphere_edit(
    surface_builder: &mut SurfaceBuilder,
    contree_builder: &mut ContreeBuilder,
    scene_accel_builder: &mut SceneAccelBuilder,
    voxel_dim_per_chunk: UVec3,
    bound: UAabb3,
    flora_edit: FloraSphereEdit,
) -> Result<()> {
    let affected_chunk_indices =
        get_affected_chunk_indices(bound.min(), bound.max(), voxel_dim_per_chunk);

    for chunk_id in affected_chunk_indices {
        let atlas_offset = chunk_id * voxel_dim_per_chunk;

        let surface_start = Instant::now();
        let res = surface_builder.build_surface(chunk_id, false);
        if let Err(e) = res {
            log::error!("Failed to build surface for chunk {}: {}", chunk_id, e);
            continue;
        }
        let surface_elapsed = surface_start.elapsed();
        BENCH
            .lock()
            .unwrap()
            .record("build_surface", surface_elapsed);

        surface_builder.edit_flora_instances(
            chunk_id,
            flora_edit.center,
            flora_edit.radius,
            flora_edit.tick,
        )?;

        let contree_start = Instant::now();
        let res = contree_builder.build_and_alloc(atlas_offset).unwrap();
        let contree_elapsed = contree_start.elapsed();
        BENCH
            .lock()
            .unwrap()
            .record("build_and_alloc", contree_elapsed);

        let scene_start = Instant::now();
        if let Some(res) = res {
            let (node_buffer_offset, leaf_buffer_offset) = res;
            scene_accel_builder
                .update_scene_tex(chunk_id, Some((node_buffer_offset, leaf_buffer_offset)))?;
        } else {
            scene_accel_builder.update_scene_tex(chunk_id, None)?;
            log::debug!("Cleared scene tex because the chunk is empty");
        }
        let scene_elapsed = scene_start.elapsed();

        log::info!(
            "[TERRAIN_EDIT] chunk {:?} rebuild surface={:.3}ms contree={:.3}ms scene_tex={:.3}ms",
            chunk_id,
            surface_elapsed.as_secs_f64() * 1000.0,
            contree_elapsed.as_secs_f64() * 1000.0,
            scene_elapsed.as_secs_f64() * 1000.0,
        );
    }

    Ok(())
}

pub(crate) fn mesh_regenerate_flora_for_sphere_edit(
    surface_builder: &mut SurfaceBuilder,
    voxel_dim_per_chunk: UVec3,
    bound: UAabb3,
    flora_edit: FloraSphereEdit,
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

        let _regen_stats = surface_builder.regenerate_flora_instances(
            chunk_id,
            flora_edit.center,
            flora_edit.radius,
            flora_edit.tick,
        )?;
    }

    Ok(())
}

pub(crate) fn mesh_trim_flora_for_sphere_edit(
    surface_builder: &mut SurfaceBuilder,
    voxel_dim_per_chunk: UVec3,
    bound: UAabb3,
    flora_edit: FloraSphereEdit,
    target_age: u32,
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

        let _regen_stats = surface_builder.trim_flora_instances(
            chunk_id,
            flora_edit.center,
            flora_edit.radius,
            flora_edit.tick,
            target_age,
        )?;
    }

    Ok(())
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
