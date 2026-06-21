use crate::flora::construct::{gen_ember_bloom, gen_lavender, gen_short_grass, gen_tall_grass};
use crate::tracer::Vertex;
use anyhow::Result;

pub const MAX_FLORA_SPECIES: usize = 4;
pub const FLORA_OCCUPANCY_SELECTION_GRASS_MIX: u32 = 254;

pub type MeshGeneratorFn = fn(bool) -> Result<(Vec<Vertex>, Vec<u32>)>;

#[derive(Clone, Copy)]
pub struct FloraSpeciesDesc {
    pub key: &'static str,
    #[allow(dead_code)]
    pub display_name: &'static str,
    pub default_bottom_color: [u8; 3],
    pub default_tip_color: [u8; 3],
    pub mesh_generator: MeshGeneratorFn,
}

impl FloraSpeciesDesc {
    pub const fn new(
        key: &'static str,
        display_name: &'static str,
        default_bottom_color: [u8; 3],
        default_tip_color: [u8; 3],
        mesh_generator: MeshGeneratorFn,
    ) -> Self {
        Self {
            key,
            display_name,
            default_bottom_color,
            default_tip_color,
            mesh_generator,
        }
    }
}

pub const FLORA_SPECIES: &[FloraSpeciesDesc] = &[
    FloraSpeciesDesc::new(
        "tall_grass",
        "Tall Grass",
        [61, 163, 59],
        [168, 227, 0],
        gen_tall_grass,
    ),
    FloraSpeciesDesc::new(
        "short_grass",
        "Short Grass",
        [61, 163, 59],
        [168, 227, 0],
        gen_short_grass,
    ),
    FloraSpeciesDesc::new(
        "lavender",
        "Lavender",
        [74, 165, 0],
        [85, 0, 207],
        gen_lavender,
    ),
    FloraSpeciesDesc::new(
        "ember_bloom",
        "Ember Bloom",
        [42, 138, 102],
        [255, 141, 78],
        gen_ember_bloom,
    ),
];

pub fn species() -> &'static [FloraSpeciesDesc] {
    FLORA_SPECIES
}

pub fn species_count() -> usize {
    FLORA_SPECIES.len()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FloraPaintSelection {
    GrassMix,
    Species(u32),
}

impl FloraPaintSelection {
    pub fn shader_selection(self) -> u32 {
        match self {
            Self::GrassMix => FLORA_OCCUPANCY_SELECTION_GRASS_MIX,
            Self::Species(species_idx) => {
                debug_assert!(
                    (species_idx as usize) < species_count(),
                    "flora paint species index {} exceeds species count {}",
                    species_idx,
                    species_count()
                );
                species_idx
            }
        }
    }
}

pub const PLAYER_FLORA_PAINT_SELECTIONS: &[FloraPaintSelection] = &[
    FloraPaintSelection::GrassMix,
    FloraPaintSelection::Species(2),
    FloraPaintSelection::Species(3),
];

pub fn flora_paint_selection_label(selection: FloraPaintSelection) -> &'static str {
    match selection {
        FloraPaintSelection::GrassMix => "Grass Mix",
        FloraPaintSelection::Species(species_idx) => species()
            .get(species_idx as usize)
            .map(|species| species.display_name)
            .unwrap_or("Unknown Flora"),
    }
}

pub fn assert_species_limit() {
    assert!(
        species_count() <= MAX_FLORA_SPECIES,
        "Defined {} flora species but MAX_FLORA_SPECIES is {}",
        species_count(),
        MAX_FLORA_SPECIES
    );
}
