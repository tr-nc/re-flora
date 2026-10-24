use super::TreeDesc;
use serde::{Deserialize, Serialize};

/// Persistent species identity. An individual additionally owns its authored description and seed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TreeSpecies {
    #[default]
    GardenTree,
    Conifer,
    AppleTree,
    Pine,
    WeepingWillow,
}

impl TreeSpecies {
    #[cfg(test)]
    pub const ALL: [Self; 5] = [
        Self::GardenTree,
        Self::Conifer,
        Self::AppleTree,
        Self::Pine,
        Self::WeepingWillow,
    ];

    /// Legacy identities remain loadable, but new gardens offer the finished species.
    pub const PLANTABLE: [Self; 3] = [Self::AppleTree, Self::Pine, Self::WeepingWillow];

    pub fn uses_axis_model(self, legacy_axis_model: bool) -> bool {
        match self {
            Self::GardenTree => false,
            Self::Conifer => legacy_axis_model,
            Self::AppleTree | Self::Pine | Self::WeepingWillow => true,
        }
    }

    pub fn bears_apples(self) -> bool {
        matches!(self, Self::GardenTree | Self::AppleTree)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::GardenTree => "Garden Tree",
            Self::Conifer => "Conifer (legacy)",
            Self::AppleTree => "Apple Tree",
            Self::Pine => "Pine",
            Self::WeepingWillow => "Weeping Willow",
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
            Self::AppleTree | Self::Pine | Self::WeepingWillow => {
                // Keep live authoring for the tuned species; switching species starts its own
                // coherent profile, carrying only the shared planting size and individual seed.
                if authored.species == self {
                    let mut desc = authored.clone();
                    desc.axis_model = true;
                    return desc;
                }
                let mut desc = TreeDesc {
                    species: self,
                    axis_model: true,
                    size: authored.size,
                    trunk_thickness: 0.32,
                    leaves_size_level: 5,
                    leaf_offset: 0,
                    leaf_density: 0.035,
                    leaf_spray_width_ratio: 0.95,
                    leaf_spray_thickness_ratio: 0.85,
                    leaf_spray_tip_offset_ratio: 0.10,
                    fruit_spawn_probability: 0.45,
                    fruit_side_offset_voxels: 1.,
                    fruit_side_offset_variance_voxels: 2.,
                    fruit_down_offset_voxels: 14.,
                    fruit_down_offset_variance_voxels: 4.,
                    enable_subdivision: false,
                    ..TreeDesc::default()
                };
                desc.branching.seed = authored.branching.seed;
                if self != Self::AppleTree {
                    desc.leaves_size_level = 4;
                }
                if self == Self::Pine {
                    desc.trunk_thickness = 0.19;
                    desc.leaf_density = 0.09;
                    desc.leaf_spray_width_ratio = 0.75;
                    desc.leaf_spray_thickness_ratio = 0.45;
                } else if self == Self::WeepingWillow {
                    desc.trunk_thickness = 0.38;
                    desc.leaf_density = 0.22;
                    desc.leaf_spray_width_ratio = 0.35;
                    desc.leaf_spray_thickness_ratio = 0.22;
                    desc.leaf_spray_tip_offset_ratio = -0.35;
                }
                if !self.bears_apples() {
                    desc.fruit_spawn_probability = 0.;
                }
                desc
            }
            Self::Conifer => {
                // The recursive parameters are the A-mode comparison, not the axis architecture.
                let mut desc = TreeDesc {
                    species: Self::Conifer,
                    axis_model: true,
                    size: authored.size,
                    ..TreeDesc::default()
                };
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
