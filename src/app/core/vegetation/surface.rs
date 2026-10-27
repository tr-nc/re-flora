//! Derived surface preparation for a desired forest, never a second canonical tree owner.
use super::{App, TreeRecord};
use crate::{
    geom::{RoundCone, UAabb3},
    tracer::{PreparedRasterTreePublication, RasterTreeMesh},
    tree_gen::{pose::BranchPose, Tree},
};
use anyhow::{ensure, Context, Result};
use glam::{UVec3, Vec3};
use std::{collections::BTreeMap, sync::Arc, time::Instant};

/// Small snapshot of just the facts consumed by extraction/binding. No foliage,
/// lifecycle, or canonical state is cloned into the derived-publication owner.
pub(super) struct TreeSurfaceSource {
    id: u32,
    position: Vec3,
    bound: UAabb3,
    cones: Vec<RoundCone>,
    tree: Arc<Tree>,
    poses: Vec<BranchPose>,
    fruits: Vec<(UVec3, usize, Vec3)>,
}
impl TreeSurfaceSource {
    pub(super) fn from_records<'a>(
        records: impl IntoIterator<Item = (u32, &'a TreeRecord)>,
    ) -> Vec<Self> {
        let mut sources: Vec<_> = records
            .into_iter()
            .map(|(id, record)| Self {
                id,
                position: record.position,
                bound: record.bound,
                cones: record.trunk_geometry.round_cones.clone(),
                tree: record.rest_tree.clone(),
                poses: record.pose.branches().to_vec(),
                fruits: record
                    .fruit_specs
                    .iter()
                    .map(|fruit| {
                        let (branch, pivot) = fruit
                            .attachment
                            .expect("published fruit requires wood support");
                        (fruit.position_voxels, branch, pivot)
                    })
                    .collect(),
            })
            .collect();
        // Wood has no tree ID: overlapping ownership and attachments choose the lowest stable ID.
        sources.sort_unstable_by_key(|source| source.id);
        sources
    }
}

pub(super) struct PreparedTreeSurface {
    publication: Option<PreparedRasterTreePublication>,
    collision: Option<super::super::physics::PreparedTreeCollisionSurface>,
    dependencies: crate::tracer::tree_surface_cache::TreeSurfaceDependencies,
    terrain_revision: u32,
    canonical_revision: u64,
    trees: usize,
    cells: usize,
    triangles: usize,
    compile_ms: f64,
}
impl PreparedTreeSurface {
    pub(super) fn prepare(
        app: &mut App,
        sources: &[TreeSurfaceSource],
        canonical_revision: u64,
    ) -> Result<Self> {
        let diagnostic = std::env::var_os("RE_FLORA_TREE_EDIT_DIAGNOSTIC").is_some();
        let wait_started = Instant::now();
        app.vulkan_ctx.device().wait_idle();
        let wait_ms = wait_started.elapsed().as_secs_f64() * 1000.;
        let started = Instant::now();
        let mut read_ms = 0.;
        let mut append_ms = 0.;
        let mut cache_hits = 0;
        let mut mesh = RasterTreeMesh::default();
        let world_dim = app.world_chunk_dim * super::super::VOXEL_DIM_PER_CHUNK;
        let mut read_bounds = Vec::with_capacity(sources.len());
        for source in sources {
            let bound =
                crate::tracer::tree_surface_cache::tree_surface_read_bound(source.bound, world_dim);
            read_bounds.push(bound);
            let origin = bound.min();
            let dim = bound.max().saturating_sub(origin);
            let stage = Instant::now();
            let bytes = app.plain_builder.read_chunk_atlas_region(origin, dim)?;
            read_ms += stage.elapsed().as_secs_f64() * 1000.;
            let stage = Instant::now();
            cache_hits += usize::from(mesh.append_cached_tree_region(
                crate::tracer::TreeSurfaceRegion {
                    id: source.id,
                    tree: &source.tree,
                    tree_origin: source.position,
                    origin,
                    dim,
                    bytes: &bytes,
                    cones: &source.cones,
                },
                &app.tracer.raster_trees.rest_mesh,
            )?);
            append_ms += stage.elapsed().as_secs_f64() * 1000.;
        }
        let stage = Instant::now();
        log::info!(
            "[TREE][COMPILE_CACHE] reused={cache_hits} rebuilt={}",
            sources.len() - cache_hits
        );
        let cells = mesh.finish()?;
        let finish_ms = stage.elapsed().as_secs_f64() * 1000.;
        let stage = Instant::now();
        let mut anchors = BTreeMap::new();
        let bind_started = Instant::now();
        mesh.bind_compiled_fragments()?;
        let mesh_bind_ms = bind_started.elapsed().as_secs_f64() * 1000.;
        for source in sources {
            for (leaf, &branch) in source
                .tree
                .relative_leaf_placements()
                .iter()
                .zip(source.tree.leaf_branch_indices())
            {
                let anchor = (leaf.anchor + source.position * 256.).as_uvec3();
                anchors
                    .entry((anchor.to_array(), false))
                    .or_insert((source.id, branch, None, None));
            }
            for &(fruit, _, pivot) in &source.fruits {
                // Bind at the actual woody support, never at the floating body center.
                let skin = crate::tree_gen::skin::SkinBinding::at_rest_position(
                    &source.tree,
                    pivot - source.position * 256.,
                )?;
                anchors.entry((fruit.to_array(), true)).or_insert((
                    source.id,
                    skin.branch,
                    Some(pivot),
                    Some(skin),
                ));
            }
        }
        mesh.compile_cpu_deformation()?;
        let bind_ms = stage.elapsed().as_secs_f64() * 1000.;
        let stage = Instant::now();
        let by_id: BTreeMap<_, _> = sources.iter().map(|source| (source.id, source)).collect();
        let publication = app.tracer.prepare_static_raster_trees(
            &mesh,
            &cells,
            anchors
                .into_iter()
                .map(
                    |((anchor, _), (tree_id, branch, fruit_pivot, fruit_skin))| {
                        crate::tracer::TreeAttachment {
                            anchor: UVec3::from(anchor),
                            tree_id,
                            branch,
                            fruit_pivot,
                            fruit_skin,
                        }
                    },
                )
                .collect(),
            |id| by_id.get(&id).map(|source| source.poses.as_slice()),
            app.debug_settings.adjustables.raster_tree_wind.value,
        )?;
        let collision = publication
            .as_ref()
            .map(|publication| {
                super::super::physics::TerrainPhysics::prepare_tree_collision_surface(
                    app.visible_terrain_revision,
                    publication.mesh(),
                )
            })
            .transpose()?;
        let upload_ms = stage.elapsed().as_secs_f64() * 1000.;
        if diagnostic {
            log::info!("[TREE_EDIT_DIAG] published={}", publication.is_some());
            log::info!("[TREE_EDIT_DIAG] compile wait_ms={wait_ms:.3} read_ms={read_ms:.3} append_ms={append_ms:.3} finish_ms={finish_ms:.3} bind_ms={bind_ms:.3} mesh_bind_ms={mesh_bind_ms:.3} upload_ms={upload_ms:.3} bounds={read_bounds:?}");
        }
        Ok(Self {
            publication,
            collision,
            dependencies: mesh.terrain_dependencies(),
            terrain_revision: app.visible_terrain_revision,
            canonical_revision,
            trees: sources.len(),
            cells: mesh.cell_count(),
            triangles: mesh.indices.len() / 3,
            compile_ms: started.elapsed().as_secs_f64() * 1000.,
        })
    }

    pub(super) fn validate(&self, app: &App) -> Result<()> {
        ensure!(
            self.terrain_revision == app.visible_terrain_revision,
            "terrain changed during tree surface preparation"
        );
        Ok(())
    }

    pub(super) fn commit(self, app: &mut App) {
        debug_assert_eq!(self.terrain_revision, app.visible_terrain_revision);
        if let Some(publication) = self.publication {
            app.tracer.commit_static_raster_trees(publication);
        }
        if let Some(collision) = self.collision {
            app.terrain_physics.commit_tree_collision_surface(collision);
        }
        app.tracer.raster_trees.source.compiled(
            self.terrain_revision,
            self.canonical_revision,
            self.dependencies,
        );
        app.tracer.raster_trees.enabled = true;
        log::info!("[TREE][RASTER_STATIC] revision={} trees={} surface_cells={} triangles={} compile_ms={:.3} query_primitives={} secondary_geometry=published_tree_surface",
            self.terrain_revision, self.trees, self.cells, self.triangles, self.compile_ms, app.tracer.raster_trees.scene.primitives.len());
    }
}

impl App {
    /// Opt-in hidden smoke: exercise a real preparation rejection after atlas stamping,
    /// including an existing hand-carved voxel and underlying non-tree terrain bytes.
    pub(in crate::app::core) fn exercise_tree_surface_rejection(&mut self) -> Result<()> {
        let hole = UVec3::from(
            *self
                .tracer
                .raster_trees
                .rest_mesh
                .solid_cells
                .iter()
                .next()
                .context("rollback fixture requires existing wood")?,
        );
        self.execute_world_edit(super::WorldEditTransaction::tree_changes(
            vec![super::VoxelEdit::ClearVoxelRegion(
                super::ClearVoxelRegionEdit {
                    offset: hole,
                    dim: UVec3::ONE,
                },
            )],
            vec![UAabb3::new(hole, hole + UVec3::ONE)],
        ))?;
        self.sync_static_raster_trees()?;
        let id = self.trees.placement_id(true);
        let generation = self.trees.allocate_canopy_acoustic_generation();
        let desc = self.debug_settings.tree.desc.clone();
        let compiled = super::TreePlacementService::compile(
            desc.clone(),
            self.debug_tree_pos + Vec3::X * 0.25,
            UAabb3::default(),
            self.debug_settings.adjustables.tree_age.value,
            generation,
            16,
        );
        let mut publication = super::PreparedTreePublication::new(id, desc.clone(), compiled);
        // Deliberately inconsistent prepared facts, never a normal runtime mode.
        let mut invalid = desc;
        invalid.size *= 0.125;
        publication.record.rest_tree = Arc::new(Tree::new(invalid));
        let mut bound = publication.record.bound;
        for record in self.trees.records.values() {
            bound = bound.union_with(&record.bound);
        }
        let bound = crate::tracer::tree_surface_cache::tree_surface_read_bound(
            bound,
            self.world_chunk_dim * super::super::VOXEL_DIM_PER_CHUNK,
        );
        let origin = bound.min();
        let dim = bound.max().saturating_sub(origin);
        let before = self.plain_builder.read_chunk_atlas_region(origin, dim)?;
        let revision = self.trees.canonical_revision;
        let count = self.trees.records.len();
        let fingerprint = self.tracer.raster_trees.rest_mesh.rest_fingerprint();
        let error = self
            .transact_garden_trees(|trees, host| trees.place(publication, host))
            .expect_err("invalid surface must reject placement");
        ensure!(
            format!("{error:#}").contains("unbound GPU tree vertex"),
            "wrong rejection: {error:#}"
        );
        ensure!(
            self.trees.canonical_revision == revision
                && self.trees.records.len() == count
                && !self.trees.contains(id),
            "failed surface preparation changed canonical trees"
        );
        ensure!(
            self.tracer.raster_trees.enabled
                && self.tracer.raster_trees.rest_mesh.rest_fingerprint() == fingerprint,
            "failed preparation changed the resident garden surface"
        );
        let after = self.plain_builder.read_chunk_atlas_region(origin, dim)?;
        ensure!(
            before == after,
            "tree rejection did not restore exact prior atlas bytes (including edits and soil)"
        );
        self.tracer.validate_gpu_tree_surface()?;
        log::info!("[TREE][PUBLICATION_ROLLBACK] passed exact_atlas=true canonical=true resident_surface=true rejection={error:#}");
        Ok(())
    }
}
