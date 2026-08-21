use crate::geom::{BvhNode, Cuboid, RoundCone, Sphere, Torus, UAabb3};
use crate::tree_gen::TreeDesc;
use glam::{Quat, UVec3, Vec2, Vec3};

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub(crate) enum TreePlacement {
    /// Place the tree at an exact world position (height already resolved).
    World(Vec3),
}

#[derive(Clone, Copy, Debug, Default)]
#[allow(dead_code)]
pub(crate) struct TreeAddOptions {
    pub(crate) clean_before_add: bool,
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
    StampCuboids {
        bvh_nodes: Vec<BvhNode>,
        cuboids: Vec<Cuboid>,
        voxel_type: u32,
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

#[derive(Clone, Debug, Default)]
pub(crate) struct WorldEditPlan {
    pub(crate) voxel_edits: Vec<VoxelEdit>,
    pub(crate) build_edits: Vec<BuildEdit>,
}

impl WorldEditPlan {
    pub(crate) fn with_voxel(edit: VoxelEdit) -> Self {
        Self {
            voxel_edits: vec![edit],
            build_edits: vec![],
        }
    }

    #[allow(dead_code)]
    pub(crate) fn with_build(edit: BuildEdit) -> Self {
        Self {
            voxel_edits: vec![],
            build_edits: vec![edit],
        }
    }

    pub(crate) fn with_voxel_and_build(voxel_edit: VoxelEdit, build_edit: BuildEdit) -> Self {
        Self {
            voxel_edits: vec![voxel_edit],
            build_edits: vec![build_edit],
        }
    }
}
