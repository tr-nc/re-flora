use super::*;

#[test]
fn opening_trees_capture_the_same_saved_recipe_as_grow() {
    let mut settings = DebugSettings::load();
    settings.tree.desc = TreeSpecies::AppleTree.default_desc();
    // Non-default authoring must survive too, not just the two planting sliders.
    settings.tree.desc.leaf_density = 0.07;
    settings.tree.desc.leaf_spray_width_ratio = 1.2;
    settings.tree.desc.fruit_down_offset_voxels = 4.5;
    for (scale, density, mature) in [(0.5, 0., false), (1., 0.1, true), (2., 0.8, false)] {
        settings.adjustables.tree_planting_scale.value = scale;
        settings.adjustables.tree_planting_apple_density.value = density;
        settings.adjustables.tree_planting_instant_mature.value = mature;
        let loaded = &settings;
        let grow = super::super::vegetation::planting_description(loaded, TreeSpecies::AppleTree);
        assert_eq!(
            grow.size,
            TreeSpecies::AppleTree.default_desc().size * scale
        );
        assert_eq!(grow.fruit_spawn_probability, density);
        for seed in [layout::DEFAULT_SEED, 0, u32::MAX] {
            for index in 0..layout::TREE_CENTERS.len() {
                let opening = opening_tree_description(loaded, seed, index);
                let mut expected = grow.clone();
                expected.branching.seed = layout::hash(seed ^ index as u32 ^ 0x74726565) as u64;
                assert_eq!(
                    opening, expected,
                    "opening must differ only in individual seed"
                );
                assert_eq!(opening.leaf_density, 0.07);
                assert_eq!(opening.leaf_spray_width_ratio, 1.2);
                assert_eq!(opening.fruit_down_offset_voxels, 4.5);
                // Persistence retains the captured description rather than regenerating it.
                let serialized = serde_json::to_string(&opening).unwrap();
                let restored: TreeDesc = serde_json::from_str(&serialized).unwrap();
                assert_eq!(opening, restored);
            }
        }
    }
}

#[test]
fn opening_keeps_apple_species_when_the_authoring_editor_contains_another_tree() {
    let mut settings = DebugSettings::load();
    settings.tree.desc = TreeSpecies::Pine.default_desc();
    settings.adjustables.tree_planting_scale.value = 1.5;
    settings.adjustables.tree_planting_apple_density.value = 0.12;
    let opening = opening_tree_description(&settings, layout::DEFAULT_SEED, 0);
    assert_eq!(opening.species, TreeSpecies::AppleTree);
    assert_eq!(
        opening.size,
        TreeSpecies::AppleTree.default_desc().size * 1.5
    );
    assert_eq!(opening.fruit_spawn_probability, 0.12);
}
