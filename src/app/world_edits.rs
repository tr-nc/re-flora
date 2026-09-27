use crate::geom::{BvhNode, Cuboid, RoundCone, Sphere, Torus, UAabb3};
use crate::tree_gen::TreeDesc;
use crate::{
    app::world_ops,
    builder::{PlainBuilder, VOXEL_TYPE_CHERRY_WOOD},
};
use anyhow::Result;
use glam::{Quat, UVec3, Vec2, Vec3};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub(crate) enum TreePlacement {
    /// Place the tree at an exact world position (height already resolved).
    World(Vec3),
}

#[derive(Clone, Copy, Debug, Default)]
#[allow(dead_code)]
pub(crate) struct TreeAddOptions {
    pub(crate) assign_new_id: bool,
}

impl TreeAddOptions {
    #[allow(dead_code)]
    pub(crate) fn with_new_id(mut self) -> Self {
        self.assign_new_id = true;
        self
    }
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct TreePlacementEdit {
    pub(crate) tree_desc: TreeDesc,
    pub(crate) placement: TreePlacement,
    pub(crate) options: TreeAddOptions,
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub(crate) struct FencePostPlacementEdit {
    pub(crate) horizontal: Vec2,
    pub(crate) height: f32,
    pub(crate) half_width: f32,
    pub(crate) half_depth: f32,
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub(crate) struct CubePlacementEdit {
    pub(crate) center: Vec3,
    pub(crate) size: f32,
    pub(crate) rotation: Quat,
    pub(crate) voxel_type: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct ClearVoxelRegionEdit {
    pub(crate) offset: UVec3,
    pub(crate) dim: UVec3,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct TerrainRemovalEdit {
    pub(crate) center: Vec3,
    pub(crate) radius: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct TerrainBrushEdit {
    pub(crate) start: Vec3,
    pub(crate) end: Vec3,
    pub(crate) radius: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum VoxelAtlasStateWrite {
    #[default]
    MaterialDefault,
    Clear,
}

impl TerrainBrushEdit {
    pub(crate) fn from_previous_center(
        previous_center: Option<Vec3>,
        current_center: Vec3,
        radius: f32,
    ) -> Self {
        Self {
            start: previous_center.unwrap_or(current_center),
            end: current_center,
            radius,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum VoxelEdit {
    StampRoundCones {
        bvh_nodes: Vec<BvhNode>,
        round_cones: Vec<RoundCone>,
        voxel_type: u32,
    },
    ReplaceRoundConeVoxelType {
        bvh_nodes: Vec<BvhNode>,
        round_cones: Vec<RoundCone>,
        target_voxel_type: u32,
        fill_voxel_type: u32,
    },
    StampCuboids {
        bvh_nodes: Vec<BvhNode>,
        cuboids: Vec<Cuboid>,
        voxel_type: u32,
        atlas_state_write: VoxelAtlasStateWrite,
    },
    #[allow(dead_code)]
    StampSpheres {
        bvh_nodes: Vec<BvhNode>,
        spheres: Vec<Sphere>,
        voxel_type: u32,
    },
    StampToruses {
        bvh_nodes: Vec<BvhNode>,
        toruses: Vec<Torus>,
        voxel_type: u32,
    },
    StampSurfaceSpheres {
        bvh_nodes: Vec<BvhNode>,
        spheres: Vec<Sphere>,
        voxel_type: u32,
    },
    ClearVoxelRegion(ClearVoxelRegionEdit),
}

#[derive(Clone, Debug)]
#[allow(clippy::enum_variant_names)]
pub(crate) enum BuildEdit {
    RebuildMesh(UAabb3),
    RebuildMeshWithoutFlora(UAabb3),
    #[allow(dead_code)]
    RebuildChunks(Vec<UVec3>),
    #[allow(dead_code)]
    RebuildChunksWithoutFlora(Vec<UVec3>),
}

#[derive(Clone, Debug)]
enum WorldEditPublication {
    DeferredUntilLoading,
    Terrain(UAabb3),
    Trees(Vec<UAabb3>),
}

/// The atlas mutation prepared by one authoritative world-edit transaction.
///
/// Visible Terrain Publication consumes this value so physical rebuilding and downstream
/// observation remain owned by the semantic publication module.
#[derive(Clone, Debug)]
pub(crate) struct WorldEditMutation {
    build_edits: Vec<BuildEdit>,
    mutation_elapsed: Duration,
}

impl WorldEditMutation {
    pub(crate) fn into_parts(self) -> (Vec<BuildEdit>, Duration) {
        (self.build_edits, self.mutation_elapsed)
    }
}

/// The observable result of one complete authoritative world-edit transaction.
#[derive(Clone, Debug)]
pub(crate) struct WorldEditOutcome {
    pub(crate) mutation_elapsed: Duration,
}

/// One semantic change to the authoritative voxel world.
///
/// Callers describe the intended world change. This module privately owns atlas-first ordering,
/// the physical rebuild required to make the result visible, and the flora policy for that kind of
/// change. Loading-only mutations deliberately defer publication to the existing loading
/// transaction, which preserves loaded flora instead of regenerating it.
#[derive(Clone, Debug)]
pub(crate) struct WorldEditTransaction {
    voxel_edits: Vec<VoxelEdit>,
    publication: WorldEditPublication,
}

impl WorldEditTransaction {
    pub(crate) fn during_loading(voxel_edits: Vec<VoxelEdit>) -> Self {
        Self {
            voxel_edits,
            publication: WorldEditPublication::DeferredUntilLoading,
        }
    }

    pub(crate) fn terrain_change(voxel_edits: Vec<VoxelEdit>, affected_voxels: UAabb3) -> Self {
        Self {
            voxel_edits,
            publication: WorldEditPublication::Terrain(affected_voxels),
        }
    }

    pub(crate) fn tree_changes(voxel_edits: Vec<VoxelEdit>, affected_regions: Vec<UAabb3>) -> Self {
        Self {
            voxel_edits,
            publication: WorldEditPublication::Trees(affected_regions),
        }
    }

    pub(crate) fn execute(
        self,
        plain_builder: &mut PlainBuilder,
        voxel_dim_per_chunk: UVec3,
    ) -> Result<Option<WorldEditMutation>> {
        let Self {
            voxel_edits,
            publication,
        } = self;
        let build_edits = match publication {
            WorldEditPublication::DeferredUntilLoading => None,
            WorldEditPublication::Terrain(bound) => {
                anyhow::ensure!(
                    !world_ops::affected_chunk_indices_for_bound(bound, voxel_dim_per_chunk)
                        .is_empty(),
                    "terrain world edit requires at least one affected chunk"
                );
                Some(vec![BuildEdit::RebuildMesh(bound)])
            }
            WorldEditPublication::Trees(affected_regions) => {
                let chunk_ids = tree_chunks(&affected_regions, voxel_dim_per_chunk);
                chunk_bound(&chunk_ids, voxel_dim_per_chunk)?;
                Some(vec![BuildEdit::RebuildChunksWithoutFlora(chunk_ids)])
            }
        };

        let mutation_started_at = Instant::now();
        for edit in voxel_edits {
            apply_voxel_edit(plain_builder, edit)?;
        }
        let mutation_elapsed = mutation_started_at.elapsed();

        let Some(build_edits) = build_edits else {
            return Ok(None);
        };
        Ok(Some(WorldEditMutation {
            build_edits,
            mutation_elapsed,
        }))
    }

    #[cfg(test)]
    pub(crate) fn planned_cuboid_voxel_type_at(&self, point: Vec3) -> Option<u32> {
        self.voxel_edits.iter().fold(None, |voxel_type, edit| {
            let VoxelEdit::StampCuboids {
                cuboids,
                voxel_type: fill,
                ..
            } = edit
            else {
                return voxel_type;
            };
            cuboids
                .iter()
                .any(|cuboid| {
                    let bound = cuboid.aabb();
                    point.cmpge(bound.min()).all() && point.cmple(bound.max()).all()
                })
                .then_some(*fill)
                .or(voxel_type)
        })
    }

    #[cfg(test)]
    pub(crate) fn affected_voxels(&self, voxel_dim_per_chunk: UVec3) -> Result<Option<UAabb3>> {
        match &self.publication {
            WorldEditPublication::DeferredUntilLoading => Ok(None),
            WorldEditPublication::Terrain(bound) => Ok(Some(*bound)),
            WorldEditPublication::Trees(affected_regions) => {
                let chunk_ids = tree_chunks(affected_regions, voxel_dim_per_chunk);
                chunk_bound(&chunk_ids, voxel_dim_per_chunk).map(Some)
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn voxel_edits(&self) -> &[VoxelEdit] {
        &self.voxel_edits
    }
}

fn apply_voxel_edit(plain_builder: &mut PlainBuilder, edit: VoxelEdit) -> Result<()> {
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
        VoxelEdit::ReplaceRoundConeVoxelType {
            bvh_nodes,
            round_cones,
            target_voxel_type,
            fill_voxel_type,
        } => plain_builder.chunk_replace_voxel_type_in_round_cones(
            &bvh_nodes,
            &round_cones,
            target_voxel_type,
            fill_voxel_type,
        ),
        VoxelEdit::StampCuboids {
            bvh_nodes,
            cuboids,
            voxel_type,
            atlas_state_write,
        } => {
            if voxel_type == VOXEL_TYPE_CHERRY_WOOD {
                plain_builder.chunk_modify_cuboids(&bvh_nodes, &cuboids)
            } else {
                plain_builder.chunk_modify_cuboids_with_voxel_type_and_state(
                    &bvh_nodes,
                    &cuboids,
                    voxel_type,
                    atlas_state_write == VoxelAtlasStateWrite::Clear,
                )
            }
        }
        VoxelEdit::StampSpheres {
            bvh_nodes,
            spheres,
            voxel_type,
        } => plain_builder.chunk_modify_spheres_with_voxel_type(&bvh_nodes, &spheres, voxel_type),
        VoxelEdit::StampToruses {
            bvh_nodes,
            toruses,
            voxel_type,
        } => plain_builder.chunk_modify_toruses_with_voxel_type(&bvh_nodes, &toruses, voxel_type),
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

fn chunk_bound(chunk_ids: &[UVec3], voxel_dim_per_chunk: UVec3) -> Result<UAabb3> {
    let min_chunk = chunk_ids
        .iter()
        .copied()
        .reduce(UVec3::min)
        .ok_or_else(|| anyhow::anyhow!("tree world edit requires at least one affected chunk"))?;
    let max_chunk = chunk_ids
        .iter()
        .copied()
        .reduce(UVec3::max)
        .expect("a minimum chunk implies a maximum chunk");
    Ok(UAabb3::new(
        min_chunk * voxel_dim_per_chunk,
        (max_chunk + UVec3::ONE) * voxel_dim_per_chunk,
    ))
}

fn tree_chunks(affected_regions: &[UAabb3], voxel_dim_per_chunk: UVec3) -> Vec<UVec3> {
    let mut chunk_ids = affected_regions
        .iter()
        .flat_map(|bound| world_ops::affected_chunk_indices_for_bound(*bound, voxel_dim_per_chunk))
        .collect::<Vec<_>>();
    chunk_ids.sort_unstable_by_key(|chunk_id| chunk_id.to_array());
    chunk_ids.dedup();
    chunk_ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_batch_has_one_chunk_aligned_visible_world_outcome() {
        let chunk_dim = UVec3::splat(256);
        let plan = WorldEditTransaction::tree_changes(
            Vec::new(),
            vec![
                UAabb3::new(UVec3::new(10, 20, 30), UVec3::new(20, 30, 40)),
                UAabb3::new(UVec3::new(270, 20, 30), UVec3::new(280, 30, 40)),
                UAabb3::new(UVec3::new(10, 20, 30), UVec3::new(20, 30, 40)),
            ],
        );

        assert_eq!(
            plan.affected_voxels(chunk_dim).unwrap(),
            Some(UAabb3::new(UVec3::ZERO, UVec3::new(512, 256, 256))),
        );
    }

    #[test]
    fn loading_changes_defer_visible_world_publication() {
        assert_eq!(
            WorldEditTransaction::during_loading(Vec::new())
                .affected_voxels(UVec3::splat(256))
                .unwrap(),
            None,
        );
    }
}
