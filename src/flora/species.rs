use crate::flora::construct::{
    gen_ember_bloom, gen_kochia, gen_lavender, gen_short_grass, gen_tall_grass,
};
use crate::tracer::voxel_encoding::FloraMeshData;
use anyhow::Result;

pub const MAX_FLORA_SPECIES: usize = 5;
pub const TALL_GRASS_SPECIES_INDEX: u32 = 0;
pub const SHORT_GRASS_SPECIES_INDEX: u32 = 1;
pub const LAVENDER_SPECIES_INDEX: u32 = 2;
pub const EMBER_BLOOM_SPECIES_INDEX: u32 = 3;
pub const KOCHIA_SPECIES_INDEX: u32 = 4;
pub const FLORA_OCCUPANCY_SELECTION_GRASS_MIX: u32 = 254;

/// Growth expression for the four soil-moisture levels stored in the terrain atlas.
pub const DEFAULT_MOISTURE_GROWTH_FACTORS: [f32; 4] = [0.70, 0.82, 0.93, 1.0];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FloraPlacementMode {
    Occupancy,
    Authored,
}

pub type MeshGeneratorFn = fn(bool) -> Result<FloraMeshData>;

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
const SPECIAL_FLORA_PAINT_RELEASE_INTERVAL_MS: u64 = 100;
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

pub const KOCHIA_PAINT_BRUSH_SETTINGS: FloraPaintBrushSettings = FloraPaintBrushSettings::new(
    SPECIAL_FLORA_PAINT_DAB_INTERVAL_MS,
    SPECIAL_FLORA_PAINT_RELEASE_INTERVAL_MS,
    12,
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
    pub placement_mode: FloraPlacementMode,
    /// Radius of this species' contribution to the shared 3D ordinary-grass competition field.
    pub grass_growth_influence_radius_voxels: u32,
    /// Minimum four-bit growth-potential level at the influence center (15 is unrestricted).
    pub grass_growth_influence_min_level: u8,
    /// Environmental growth factor selected by the two-bit moisture level of the root soil voxel.
    pub moisture_growth_factors: [f32; 4],
}

impl FloraSpeciesDesc {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        key: &'static str,
        display_name: &'static str,
        default_bottom_color: [u8; 3],
        default_tip_color: [u8; 3],
        mesh_generator: MeshGeneratorFn,
        paint_brush: FloraPaintBrushSettings,
        placement_mode: FloraPlacementMode,
        grass_growth_influence_radius_voxels: u32,
        grass_growth_influence_min_level: u8,
        moisture_growth_factors: [f32; 4],
    ) -> Self {
        Self {
            key,
            display_name,
            default_bottom_color,
            default_tip_color,
            mesh_generator,
            paint_brush,
            placement_mode,
            grass_growth_influence_radius_voxels,
            grass_growth_influence_min_level,
            moisture_growth_factors,
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
        FloraPlacementMode::Occupancy,
        0,
        15,
        DEFAULT_MOISTURE_GROWTH_FACTORS,
    ),
    FloraSpeciesDesc::new(
        "short_grass",
        "Short Grass",
        [61, 163, 59],
        [168, 227, 0],
        gen_short_grass,
        FloraPaintBrushSettings::dense(80),
        FloraPlacementMode::Occupancy,
        0,
        15,
        DEFAULT_MOISTURE_GROWTH_FACTORS,
    ),
    FloraSpeciesDesc::new(
        "lavender",
        "Lavender",
        [74, 165, 0],
        [85, 0, 207],
        gen_lavender,
        LAVENDER_PAINT_BRUSH_SETTINGS,
        FloraPlacementMode::Authored,
        8,
        6,
        DEFAULT_MOISTURE_GROWTH_FACTORS,
    ),
    FloraSpeciesDesc::new(
        "ember_bloom",
        "Purple Allium",
        [43, 130, 65],
        [211, 107, 174],
        gen_ember_bloom,
        EMBER_BLOOM_PAINT_BRUSH_SETTINGS,
        FloraPlacementMode::Authored,
        8,
        6,
        DEFAULT_MOISTURE_GROWTH_FACTORS,
    ),
    FloraSpeciesDesc::new(
        "kochia",
        "Kochia",
        [79, 125, 58],
        [245, 132, 153],
        gen_kochia,
        KOCHIA_PAINT_BRUSH_SETTINGS,
        FloraPlacementMode::Authored,
        7,
        6,
        DEFAULT_MOISTURE_GROWTH_FACTORS,
    ),
];

pub const TREE_LEAF_RENDER_SPECIES_INDEX: u32 = FLORA_SPECIES.len() as u32;
pub const APPLE_RENDER_SPECIES_INDEX: u32 = TREE_LEAF_RENDER_SPECIES_INDEX + 1;

pub fn species() -> &'static [FloraSpeciesDesc] {
    FLORA_SPECIES
}

pub fn species_count() -> usize {
    FLORA_SPECIES.len()
}

pub fn is_grass_species_index(species_idx: u32) -> bool {
    species_idx == TALL_GRASS_SPECIES_INDEX || species_idx == SHORT_GRASS_SPECIES_INDEX
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
    FloraPaintSelection::Species(KOCHIA_SPECIES_INDEX),
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

pub fn authored_plant_species_indices() -> impl Iterator<Item = u32> {
    species()
        .iter()
        .enumerate()
        .filter(|(_, species)| species.placement_mode == FloraPlacementMode::Authored)
        .map(|(index, _)| index as u32)
}

pub fn is_authored_plant_species_index(species_idx: u32) -> bool {
    species()
        .get(species_idx as usize)
        .is_some_and(|species| species.placement_mode == FloraPlacementMode::Authored)
}

pub fn assert_species_limit() {
    assert!(
        species_count() <= MAX_FLORA_SPECIES,
        "Defined {} flora species but MAX_FLORA_SPECIES is {}",
        species_count(),
        MAX_FLORA_SPECIES
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flora_registry_slang_const(name: &str) -> u32 {
        let registry = include_str!("../../shader/slang/flora_types.slang");
        let line = registry
            .lines()
            .find(|line| line.contains(&format!("const uint {name}")))
            .unwrap_or_else(|| panic!("missing {name} in flora_types.slang"));
        let (_, value) = line
            .split_once('=')
            .unwrap_or_else(|| panic!("missing '=' for {name} in flora_types.slang"));
        value
            .trim()
            .trim_end_matches(';')
            .trim_end_matches('u')
            .parse()
            .unwrap_or_else(|err| panic!("failed to parse {name} from flora_types.slang: {err}"))
    }

    #[test]
    fn carrot_species_is_not_registered() {
        assert!(!species().iter().any(|species| species.key == "carrot"));
        assert!(!PLAYER_FLORA_PAINT_SELECTIONS.iter().any(|selection| {
            matches!(
                selection,
                FloraPaintSelection::Species(species_idx)
                    if species()
                        .get(*species_idx as usize)
                        .is_some_and(|species| species.key == "carrot")
            )
        }));
    }

    #[test]
    fn authored_species_are_derived_from_registry_metadata() {
        assert_eq!(
            authored_plant_species_indices().collect::<Vec<_>>(),
            vec![
                LAVENDER_SPECIES_INDEX,
                EMBER_BLOOM_SPECIES_INDEX,
                KOCHIA_SPECIES_INDEX,
            ]
        );
        assert!(!is_authored_plant_species_index(0));
        assert!(!is_authored_plant_species_index(species_count() as u32));
    }

    #[test]
    fn butterfly_source_classification_excludes_only_grass_species() {
        assert!(is_grass_species_index(TALL_GRASS_SPECIES_INDEX));
        assert!(is_grass_species_index(SHORT_GRASS_SPECIES_INDEX));
        assert!(!is_grass_species_index(LAVENDER_SPECIES_INDEX));
        assert!(!is_grass_species_index(EMBER_BLOOM_SPECIES_INDEX));
        assert!(!is_grass_species_index(KOCHIA_SPECIES_INDEX));
    }

    #[test]
    fn moisture_growth_curves_are_bounded_and_monotonic() {
        for flora_species in species() {
            let curve = flora_species.moisture_growth_factors;
            assert!(curve.iter().all(|factor| (0.0..=1.0).contains(factor)));
            assert!(curve.windows(2).all(|levels| levels[0] <= levels[1]));
            assert_eq!(curve[3], 1.0);
        }
    }

    #[test]
    fn moisture_growth_samples_the_stored_supporting_voxel() {
        let environment_shader = include_str!("../../shader/slang/surface_flora_vertex.slang");
        assert!(environment_shader.contains("chunkWorldOffset + localBase"));
        assert!(!environment_shader.contains("localBase -"));

        let flora_shader = include_str!("../../shader/slang/flora_vertex.slang");
        assert!(flora_shader.contains("min(competitionFactor, environmentFactor)"));
        assert!(flora_shader.contains("gui_input.flora_growth_override_enabled != 0u"));
        assert!(flora_shader.contains("clamp(gui_input.flora_growth_override, 0.0, 1.0)"));
    }

    #[test]
    fn grass_leaf_shadow_receiver_is_rest_anchored_without_freezing_other_terms() {
        let flora_shader = include_str!("../../shader/slang/flora_vertex.slang");
        assert!(flora_shader.contains("restLeafShadowReceiverPosition"));
        assert!(flora_shader.contains("sampleStylizedVoxelShadowAtLeafReceiver"));

        let shadow_shader = include_str!("../../shader/slang/flora_shadow.slang");
        assert!(shadow_shader.contains("float3 leafReceiverCenter"));
        assert!(shadow_shader.contains("float4 worldPosition = float4(voxelCenter, 1.0)"));
        assert!(
            shadow_shader.contains("float4 leafReceiverPosition = float4(leafReceiverCenter, 1.0)")
        );
    }

    #[test]
    fn render_only_species_indices_match_shader_registry() {
        assert_eq!(
            species_count() as u32,
            flora_registry_slang_const("FLORA_SPECIES_COUNT")
        );
        assert_eq!(
            TREE_LEAF_RENDER_SPECIES_INDEX,
            flora_registry_slang_const("FLORA_SPECIES_TREE_LEAF")
        );
        assert_eq!(
            APPLE_RENDER_SPECIES_INDEX,
            flora_registry_slang_const("FLORA_SPECIES_APPLE")
        );
    }
}
