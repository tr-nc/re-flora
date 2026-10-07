use super::TreeDesc;
use serde::{Deserialize, Serialize};

/// Persistent species identity. An individual additionally owns its authored description and seed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TreeSpecies {
    #[default]
    GardenTree,
    Conifer,
}

impl TreeSpecies {
    pub const ALL: [Self; 2] = [Self::GardenTree, Self::Conifer];

    pub fn label(self) -> &'static str {
        match self {
            Self::GardenTree => "Garden Tree",
            Self::Conifer => "Conifer",
        }
    }

    pub fn mature_desc(self, authored: &TreeDesc) -> TreeDesc {
        match self {
            Self::GardenTree => {
                let mut desc = authored.clone();
                desc.species = Self::GardenTree;
                desc.axis_model = false;
                desc
            }
            Self::Conifer => {
                // The recursive parameters are the A-mode comparison, not the axis architecture.
                let mut desc = TreeDesc::default();
                desc.species = Self::Conifer;
                desc.axis_model = true;
                desc.size = authored.size;
                desc.branching.seed = authored.branching.seed;
                desc.trunk_thickness = 0.16;
                desc.thickness_reduction = 0.66;
                desc.branching.iterations = 6;
                desc.branching.initial_length = 42.;
                desc.branching.length_dropoff = 0.76;
                desc.branching.continue_main_axis = true;
                desc.branching.branch_count_min = 2;
                desc.branching.branch_count_max = 2;
                desc.branching.branch_probability = 0.65;
                desc.branching.branch_angle_min = 65_f32.to_radians();
                desc.branching.branch_angle_max = 85_f32.to_radians();
                desc.branching.vertical_tendency = 0.15;
                desc.branching.randomness = 0.12;
                desc.leaves_size_level = 4;
                desc.leaf_offset = 0;
                desc.leaf_density = 0.045;
                desc.leaf_spray_width_ratio = 0.65;
                desc.leaf_spray_thickness_ratio = 0.3;
                desc.leaf_spray_tip_offset_ratio = 0.15;
                desc.fruit_spawn_probability = 0.;
                desc.enable_subdivision = false;
                desc
            }
        }
    }
}
