use super::*;

#[test]
fn opening_ginkgo_capture_the_same_saved_recipe_as_grow() {
    let mut settings = DebugSettings::load();
    settings.tree.desc = TreeSpecies::Ginkgo.default_desc();
    settings.adjustables.tree_size_variation.value = 0.;
    settings.tree.desc.leaf_density = 0.07;
    settings.tree.desc.leaf_spray_width_ratio = 1.2;
    for (scale, mature) in [(0.5, false), (1., true), (2., false)] {
        settings.adjustables.tree_planting_scale.value = scale;
        settings.adjustables.tree_planting_instant_mature.value = mature;
        for seed in [layout::DEFAULT_SEED, 0, u32::MAX] {
            for index in 0..layout::TREE_CENTERS.len() {
                let opening = opening_tree_description(&settings, seed, index);
                let expected = super::super::vegetation::planting_description(
                    &settings,
                    TreeSpecies::Ginkgo,
                    layout::tree_individual(seed, index),
                );
                assert_eq!(opening, expected);
                assert_eq!(
                    opening.size,
                    TreeSpecies::Ginkgo.default_desc().size * scale
                );
                assert_eq!(opening.fruit_spawn_probability, 0.);
                assert_eq!(opening.leaf_density, 0.07);
                assert_eq!(opening.leaf_spray_width_ratio, 1.2);
                let serialized = serde_json::to_string(&opening).unwrap();
                assert!(!serialized.contains("\"form\""));
                let restored: TreeDesc = serde_json::from_str(&serialized).unwrap();
                assert_eq!(opening, restored);
            }
        }
    }
}

#[test]
fn opening_and_later_planting_share_random_and_fixed_individuals() {
    let mut settings = DebugSettings::load();
    for enabled in [true, false] {
        settings.adjustables.tree_randomize_new.value = enabled;
        settings.adjustables.tree_size_variation.value = 30.;
        settings.adjustables.tree_hue_mean.value = -30.;
        settings.adjustables.tree_hue_variation.value = 30.;
        let first = opening_tree_description(&settings, 20260519, 0);
        for i in 0..layout::TREE_CENTERS.len() {
            let opening = opening_tree_description(&settings, 20260519, i);
            let next = super::super::vegetation::planting_description(
                &settings,
                TreeSpecies::Ginkgo,
                layout::tree_individual(20260519, i),
            );
            assert_eq!(opening, next);
            if !enabled {
                assert_eq!(opening, first);
            } else if i != 0 {
                assert_ne!(opening, first);
            }
        }
    }
}

#[test]
fn opening_roots_are_supported_across_sloped_soil_including_seed_seven() {
    let settings = DebugSettings::load();
    let size = Vec2::splat(4.);
    for seed in [layout::DEFAULT_SEED, 7, 0, u32::MAX, 122, 907] {
        let ground = |p: Vec2| (layout::height_voxels(p / size, seed) as f32 + 1.) / 256. - 1e-7;
        for (i, center) in layout::TREE_CENTERS.iter().enumerate() {
            let desc = opening_tree_description(&settings, seed, i);
            for age in [super::super::vegetation::SAPLING_AGE, 1.] {
                let tree = crate::tree_gen::Tree::new(desc.at_age(age));
                let position = opening_tree_position(&tree, *center * size, ground);
                let root = &tree.world_trunks(position)[0];
                let base = (position.y * 256.).floor() as u32;
                let min = root.aabb().min().floor().as_uvec3();
                let max = root.aabb().max().ceil().as_uvec3();
                for z in min.z..=max.z {
                    for x in min.x..=max.x {
                        let point = (Vec2::new(x as f32, z as f32) + Vec2::splat(0.5)) / 256.;
                        let surface = layout::height_voxels(point / size, seed);
                        assert!(
                            surface <= base,
                            "uphill soil would reject the tree: seed={seed} tree={i} age={age}"
                        );
                    }
                }
                assert!(
                    (position.y - ground(*center * size)) * 256. < root.radius_a(),
                    "do not float the root above its soil"
                );
            }
        }
    }
}

#[test]
fn opening_keeps_ginkgo_when_the_authoring_editor_contains_another_tree() {
    let mut settings = DebugSettings::load();
    settings.tree.desc = TreeSpecies::Pine.default_desc();
    settings.adjustables.tree_size_variation.value = 0.;
    settings.adjustables.tree_planting_scale.value = 1.5;
    let opening = opening_tree_description(&settings, layout::DEFAULT_SEED, 0);
    assert_eq!(opening.species, TreeSpecies::Ginkgo);
    assert_eq!(opening.size, TreeSpecies::Ginkgo.default_desc().size * 1.5);
    assert_eq!(opening.fruit_spawn_probability, 0.);
}
