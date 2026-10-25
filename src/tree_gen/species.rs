use super::{growth::rules, TreeDesc};
use serde::{Deserialize, Serialize};

/// Persistent species identity. An individual additionally owns its authored description and seed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TreeSpecies {
    /// Decode-only marker: never a selectable or independently generated architecture.
    #[serde(rename = "garden_tree")]
    RetiredGardenTree,
    Conifer,
    #[default]
    AppleTree,
    Pine,
    WeepingWillow,
}

/// The single registration of a species' architecture, foliage, fruit and authoring defaults.
/// Rendering and publication consume its generated skeleton, not species-name switches.
struct SpeciesDefinition {
    label: &'static str,
    growth: rules::GrowthRule,
    apples: bool,
    authoring: AuthoringDefaults,
}

enum AuthoringDefaults {
    LegacyConifer,
    Garden(GardenDefaults),
}

struct GardenDefaults {
    trunk_thickness: f32,
    leaf_size_level: u32,
    leaf_density: f32,
    spray_width: f32,
    spray_thickness: f32,
    spray_tip_offset: f32,
}

impl TreeSpecies {
    #[cfg(test)]
    pub const ALL: [Self; 4] = [
        Self::Conifer,
        Self::AppleTree,
        Self::Pine,
        Self::WeepingWillow,
    ];

    /// Legacy identities remain loadable, but new gardens offer the finished species.
    pub const PLANTABLE: [Self; 3] = [Self::AppleTree, Self::Pine, Self::WeepingWillow];

    fn definition(self) -> &'static SpeciesDefinition {
        match self {
            Self::RetiredGardenTree => Self::AppleTree.definition(),
            Self::Conifer => &SpeciesDefinition {
                label: "Conifer (legacy)",
                growth: rules::LEGACY_CONIFER,
                apples: false,
                authoring: AuthoringDefaults::LegacyConifer,
            },
            Self::AppleTree => &SpeciesDefinition {
                label: "Apple Tree",
                growth: rules::APPLE,
                apples: true,
                authoring: AuthoringDefaults::Garden(GardenDefaults {
                    trunk_thickness: 0.32,
                    leaf_size_level: 5,
                    leaf_density: 0.035,
                    spray_width: 0.95,
                    spray_thickness: 0.85,
                    spray_tip_offset: 0.10,
                }),
            },
            Self::Pine => &SpeciesDefinition {
                label: "Pine",
                growth: rules::PINE,
                apples: false,
                authoring: AuthoringDefaults::Garden(GardenDefaults {
                    trunk_thickness: 0.19,
                    leaf_size_level: 4,
                    leaf_density: 0.09,
                    spray_width: 0.75,
                    spray_thickness: 0.45,
                    spray_tip_offset: 0.10,
                }),
            },
            Self::WeepingWillow => &SpeciesDefinition {
                label: "Weeping Willow",
                growth: rules::WILLOW,
                apples: false,
                authoring: AuthoringDefaults::Garden(GardenDefaults {
                    trunk_thickness: 0.38,
                    leaf_size_level: 4,
                    leaf_density: 0.22,
                    spray_width: 0.35,
                    spray_thickness: 0.22,
                    spray_tip_offset: -0.35,
                }),
            },
        }
    }

    pub(super) fn growth_rule(self, legacy_axis_model: bool) -> rules::GrowthRule {
        let rule = self.definition().growth;
        if matches!(rule.architecture, rules::Architecture::LegacyConifer) && !legacy_axis_model {
            rules::RECURSIVE
        } else {
            rule
        }
    }

    pub fn uses_axis_model(self, legacy_axis_model: bool) -> bool {
        !matches!(
            self.growth_rule(legacy_axis_model).architecture,
            rules::Architecture::Recursive
        )
    }

    pub fn bears_apples(self) -> bool {
        self.definition().apples
    }

    pub fn label(self) -> &'static str {
        self.definition().label
    }

    pub fn mature_desc(self, authored: &TreeDesc) -> TreeDesc {
        if self == Self::RetiredGardenTree {
            return Self::AppleTree.mature_desc(authored);
        }
        match &self.definition().authoring {
            AuthoringDefaults::LegacyConifer => legacy_conifer_desc(authored),
            AuthoringDefaults::Garden(defaults) => {
                // A profile is a switching default, never a replacement for saved live authoring.
                if authored.species == self {
                    let mut desc = authored.clone();
                    desc.axis_model = true;
                    return desc;
                }
                let mut desc = TreeDesc {
                    species: self,
                    axis_model: true,
                    size: authored.size,
                    trunk_thickness: defaults.trunk_thickness,
                    leaves_size_level: defaults.leaf_size_level,
                    leaf_offset: 0,
                    leaf_density: defaults.leaf_density,
                    leaf_spray_width_ratio: defaults.spray_width,
                    leaf_spray_thickness_ratio: defaults.spray_thickness,
                    leaf_spray_tip_offset_ratio: defaults.spray_tip_offset,
                    fruit_spawn_probability: if self.bears_apples() { 0.45 } else { 0. },
                    fruit_side_offset_voxels: 1.,
                    fruit_side_offset_variance_voxels: 2.,
                    fruit_down_offset_voxels: 14.,
                    fruit_down_offset_variance_voxels: 4.,
                    enable_subdivision: false,
                    ..TreeDesc::legacy_wire_defaults()
                };
                desc.branching.seed = authored.branching.seed;
                desc
            }
        }
    }
}

fn legacy_conifer_desc(authored: &TreeDesc) -> TreeDesc {
    // Preserve this authoring recipe along with both legacy generators for saved gardens.
    let mut desc = TreeDesc {
        species: TreeSpecies::Conifer,
        axis_model: true,
        size: authored.size,
        ..TreeDesc::legacy_wire_defaults()
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
