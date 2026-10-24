use super::{pose::TreePose, Tree, TreeDesc, TreeSpecies, MAX_AXIS_SEGMENTS};
use glam::Vec3;

#[test]
fn garden_species_are_connected_deterministic_and_bounded_across_seeds_and_ages() {
    for species in TreeSpecies::PLANTABLE {
        for seed in [0, 7, 122, 907, u64::MAX] {
            let mut desc = species.mature_desc(&TreeDesc::default());
            desc.branching.seed = seed;
            let mature = Tree::new(desc.clone());
            assert_eq!(mature.branches(), Tree::new(desc.clone()).branches());
            assert!(mature.branches().len() < MAX_AXIS_SEGMENTS);
            assert!(!mature.relative_leaf_placements().is_empty());
            assert!(mature.relative_leaf_placements().len() < 50_000);
            let mut count = 0;
            for age in [0., 0.1, 0.25, 0.5, 0.75, 1.] {
                let tree = Tree::new(desc.at_age(age));
                assert_eq!(tree.branches().len(), mature.branches().len());
                assert!(tree.trunks().len() >= count);
                count = tree.trunks().len();
                let visible = tree.trunk_branch_indices();
                for (i, (branch, meta)) in
                    tree.branches().iter().zip(tree.axis_segments()).enumerate()
                {
                    assert!(branch.start.is_finite() && branch.end.is_finite());
                    assert!(meta.radius_start >= meta.radius_end && meta.radius_end >= 0.);
                    assert_eq!(branch.parent, mature.branches()[i].parent);
                    assert_eq!(meta.axis_id, mature.axis_segments()[i].axis_id);
                    if let Some(parent) = branch.parent {
                        assert!(parent < i);
                        assert_eq!(branch.start, tree.branches()[parent].end);
                        assert!(meta.reveal_age >= tree.axis_segments()[parent].reveal_age);
                        if visible.contains(&i) {
                            assert!(visible.contains(&parent));
                        }
                        if meta.axis_id == tree.axis_segments()[parent].axis_id {
                            assert_eq!(meta.radius_start, tree.axis_segments()[parent].radius_end);
                        } else {
                            assert!(meta.radius_start < tree.axis_segments()[parent].radius_end);
                        }
                    }
                }
                for (leaf, &branch) in tree
                    .relative_leaf_placements()
                    .iter()
                    .zip(tree.leaf_branch_indices())
                {
                    assert!(leaf.position.is_finite());
                    assert!(visible.contains(&branch));
                    assert_eq!(leaf.anchor, tree.branches()[branch].end);
                }
                TreePose::new(tree.branches(), Vec3::ZERO).unwrap();
            }
            assert_eq!(count, mature.branches().len());
            desc.branching.seed = seed.wrapping_add(1);
            assert_ne!(mature.branches(), Tree::new(desc).branches());
        }
    }
}

#[test]
fn willow_has_pendant_shoots_while_apple_and_pine_keep_ascending_crowns() {
    for species in TreeSpecies::PLANTABLE {
        for seed in [7, 122, 907] {
            let mut desc = species.mature_desc(&TreeDesc::default());
            desc.branching.seed = seed;
            let tree = Tree::new(desc);
            let twigs: Vec<_> = tree
                .branches()
                .iter()
                .zip(tree.axis_segments())
                .filter(|(_, meta)| meta.order == 2)
                .map(|(b, _)| b)
                .collect();
            let down = twigs.iter().filter(|b| b.end.y < b.start.y).count();
            if species == TreeSpecies::WeepingWillow {
                assert_eq!(down, twigs.len(), "every free willow shoot descends");
                assert!(twigs.iter().all(|b| b.end.y >= 16.));
            } else {
                assert_eq!(down, 0, "apple/pine twig tips turn upward");
            }
        }
    }
}

#[test]
fn new_profiles_and_legacy_descriptions_round_trip_without_reinterpreting_saved_trees() {
    for species in TreeSpecies::ALL {
        for axis in [false, true] {
            let mut desc = species.mature_desc(&TreeDesc::default());
            desc.axis_model = axis;
            let saved = serde_json::to_vec(&desc).unwrap();
            let restored: TreeDesc = serde_json::from_slice(&saved).unwrap();
            assert_eq!(desc, restored);
            let original = Tree::new(desc.clone());
            assert_eq!(original.branches(), Tree::new(restored).branches());
            assert_eq!(
                !original.axis_segments().is_empty(),
                species.uses_axis_model(axis)
            );
        }
    }
}
