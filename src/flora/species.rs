use crate::flora::construct::{gen_ember_bloom, gen_lavender, gen_short_grass, gen_tall_grass};
use crate::tracer::Vertex;
use anyhow::Result;

pub const MAX_FLORA_SPECIES: usize = 4;
pub const LAVENDER_SPECIES_INDEX: u32 = 2;
pub const EMBER_BLOOM_SPECIES_INDEX: u32 = 3;
pub const AUTHORED_PLANT_SPECIES_INDICES: [u32; 2] =
    [LAVENDER_SPECIES_INDEX, EMBER_BLOOM_SPECIES_INDEX];
pub const FLORA_OCCUPANCY_SELECTION_GRASS_MIX: u32 = 254;

pub type MeshGeneratorFn = fn(bool) -> Result<(Vec<Vertex>, Vec<u32>)>;

#[derive(Clone, Copy, Debug)]
pub struct FloraPaintBrushSettings {
    /// How often a held paint stroke refreshes its capsule coverage for this flora selection.
    pub dab_interval_ms: u64,
    /// How often a held authored-plant stroke releases a new batch of plants.
    pub release_interval_ms: u64,
    /// Soft blue-noise spacing target in surface voxels. Zero disables authored release.
    pub soft_spacing_voxels: u32,
    /// Number of authored plants to release per interval for this flora selection.
    pub plants_per_release: u32,
}

impl FloraPaintBrushSettings {
    pub const fn new(
        dab_interval_ms: u64,
        release_interval_ms: u64,
        soft_spacing_voxels: u32,
        plants_per_release: u32,
    ) -> Self {
        Self {
            dab_interval_ms,
            release_interval_ms,
            soft_spacing_voxels,
            plants_per_release,
        }
    }

    pub const fn dense(dab_interval_ms: u64) -> Self {
        Self::new(dab_interval_ms, dab_interval_ms, 0, 0)
    }
}

pub const GRASS_MIX_PAINT_BRUSH_SETTINGS: FloraPaintBrushSettings =
    FloraPaintBrushSettings::dense(80);

const SPECIAL_FLORA_PAINT_DAB_INTERVAL_MS: u64 = 50;
const SPECIAL_FLORA_PAINT_RELEASE_INTERVAL_MS: u64 = 500;
const SPECIAL_FLORA_PAINT_SOFT_SPACING_VOXELS: u32 = 20;

pub const LAVENDER_PAINT_BRUSH_SETTINGS: FloraPaintBrushSettings = FloraPaintBrushSettings::new(
    SPECIAL_FLORA_PAINT_DAB_INTERVAL_MS,
    SPECIAL_FLORA_PAINT_RELEASE_INTERVAL_MS,
    SPECIAL_FLORA_PAINT_SOFT_SPACING_VOXELS,
    1,
);

pub const EMBER_BLOOM_PAINT_BRUSH_SETTINGS: FloraPaintBrushSettings = FloraPaintBrushSettings::new(
    SPECIAL_FLORA_PAINT_DAB_INTERVAL_MS,
    SPECIAL_FLORA_PAINT_RELEASE_INTERVAL_MS,
    SPECIAL_FLORA_PAINT_SOFT_SPACING_VOXELS,
    1,
);

#[derive(Clone, Copy)]
pub struct FloraSpeciesDesc {
    pub key: &'static str,
    #[allow(dead_code)]
    pub display_name: &'static str,
    pub default_bottom_color: [u8; 3],
    pub default_tip_color: [u8; 3],
    pub mesh_generator: MeshGeneratorFn,
    pub paint_brush: FloraPaintBrushSettings,
}

impl FloraSpeciesDesc {
    pub const fn new(
        key: &'static str,
        display_name: &'static str,
        default_bottom_color: [u8; 3],
        default_tip_color: [u8; 3],
        mesh_generator: MeshGeneratorFn,
        paint_brush: FloraPaintBrushSettings,
    ) -> Self {
        Self {
            key,
            display_name,
            default_bottom_color,
            default_tip_color,
            mesh_generator,
            paint_brush,
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
        FloraPaintBrushSettings::dense(80),
    ),
    FloraSpeciesDesc::new(
        "short_grass",
        "Short Grass",
        [61, 163, 59],
        [168, 227, 0],
        gen_short_grass,
        FloraPaintBrushSettings::dense(80),
    ),
    FloraSpeciesDesc::new(
        "lavender",
        "Lavender",
        [74, 165, 0],
        [85, 0, 207],
        gen_lavender,
        LAVENDER_PAINT_BRUSH_SETTINGS,
    ),
    FloraSpeciesDesc::new(
        "ember_bloom",
        "Ember Bloom",
        [42, 138, 102],
        [255, 141, 78],
        gen_ember_bloom,
        EMBER_BLOOM_PAINT_BRUSH_SETTINGS,
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
    FloraPaintSelection::Species(LAVENDER_SPECIES_INDEX),
    FloraPaintSelection::Species(EMBER_BLOOM_SPECIES_INDEX),
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

pub fn flora_paint_brush_settings(selection: FloraPaintSelection) -> FloraPaintBrushSettings {
    match selection {
        FloraPaintSelection::GrassMix => GRASS_MIX_PAINT_BRUSH_SETTINGS,
        FloraPaintSelection::Species(species_idx) => species()
            .get(species_idx as usize)
            .map(|species| species.paint_brush)
            .unwrap_or(GRASS_MIX_PAINT_BRUSH_SETTINGS),
    }
}

pub fn is_authored_plant_species_index(species_idx: u32) -> bool {
    AUTHORED_PLANT_SPECIES_INDICES.contains(&species_idx)
}

pub fn assert_species_limit() {
    assert!(
        species_count() <= MAX_FLORA_SPECIES,
        "Defined {} flora species but MAX_FLORA_SPECIES is {}",
        species_count(),
        MAX_FLORA_SPECIES
    );
}
