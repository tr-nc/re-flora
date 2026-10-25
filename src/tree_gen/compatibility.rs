//! Decode retired test-tree metadata; never register or select its old morphology.
//! The frozen wire defaults also preserve missing fields in historical Conifer saves.
use super::{growth::TREE_DEFAULT_SIZE, TreeDesc};
use crate::branch_skeleton::BranchingDesc;
use std::f32::consts::PI;

fn legacy_branching_wire_defaults() -> BranchingDesc {
    BranchingDesc {
        seed: 122,
        iterations: 7,
        branch_start_fraction: 1.0 / 6.0,
        branch_end_fraction: 1.0,
        initial_length: 48.0,
        length_dropoff: 0.78,
        spread: 0.0,
        randomness: 0.33,
        vertical_tendency: 0.47,
        branch_angle_min: 24.0 * PI / 180.0,
        branch_angle_max: 48.0 * PI / 180.0,
        branch_probability: 0.82,
        branch_count_min: 2,
        branch_count_max: 3,
        segment_length_variation: 0.12,
        continue_main_axis: false,
    }
}

impl TreeDesc {
    /// Historical wire defaults are needed to read old, partially specified descriptions and
    /// identify their baked wood during migration. This is not a living species profile.
    pub(crate) fn legacy_wire_defaults() -> Self {
        TreeDesc {
            species: super::TreeSpecies::RetiredGardenTree,
            axis_model: false,
            branching: legacy_branching_wire_defaults(),
            growth_age: 1.0,
            size: TREE_DEFAULT_SIZE,
            trunk_thickness: 0.40,
            thickness_reduction: 0.61,
            leaves_size_level: 5,
            leaf_offset: 1,
            leaf_density: 0.055,
            leaf_spray_width_ratio: 0.65,
            leaf_spray_thickness_ratio: 0.35,
            leaf_spray_tip_offset_ratio: 0.25,
            fruit_spawn_probability: 0.30,
            fruit_side_offset_voxels: 0.5,
            fruit_side_offset_variance_voxels: 1.5,
            fruit_down_offset_voxels: 4.0,
            fruit_down_offset_variance_voxels: 2.0,
            fruit_swing_length_voxels: 2.0,
            fruit_swing_max_angle_degrees: 50.0,
            fruit_swing_speed: 2.1,
            fruit_swing_speed_variation: 0.3,
            fruit_swing_min_response: 0.18,
            enable_subdivision: true,
            subdivision_count_min: 6,
            subdivision_count_max: 9,
            subdivision_randomness: 2.6,
            subdivision_randomness_progression: 3.0,
        }
    }
}

impl TreeDesc {
    pub(crate) fn is_retired(&self) -> bool {
        self.species == super::TreeSpecies::RetiredGardenTree
    }

    /// Normalize before age scaling/publication. Old test trees become the current Apple profile,
    /// keeping authored size and individual seed; already-current descriptions stay untouched.
    pub(crate) fn into_current(self) -> Self {
        if self.is_retired() {
            super::TreeSpecies::AppleTree.mature_desc(&self)
        } else {
            self
        }
    }

    /// Reconstruction of old baked wood uses the existing recursive Conifer adapter with the
    /// exact saved fields. Only snapshot migration calls this; no Garden Tree generator remains.
    pub(crate) fn saved_geometry_desc(&self) -> Self {
        let mut desc = self.clone();
        if desc.is_retired() {
            desc.species = super::TreeSpecies::Conifer;
            desc.axis_model = false;
        }
        desc
    }
}
