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
fn legacy_willow_has_pendant_shoots_but_current_crowns_ascend() {
    for species in TreeSpecies::PLANTABLE
        .into_iter()
        .chain([TreeSpecies::WeepingWillow])
    {
        for seed in [7, 122, 907] {
            let mut desc = species.mature_desc(&TreeDesc::default());
            desc.branching.seed = seed;
            // The 16-voxel clearance below is specified at the architecture's reference size.
            desc.size = 30.;
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
                assert_eq!(down, 0, "current species twig tips turn upward");
            }
        }
    }
}

#[test]
fn foliage_rules_keep_tip_crowns_and_along_shoot_sprays_distinct() {
    for species in [
        TreeSpecies::AppleTree,
        TreeSpecies::Pine,
        TreeSpecies::WeepingWillow,
        TreeSpecies::LondonPlane,
        TreeSpecies::Ginkgo,
        TreeSpecies::Conifer,
    ] {
        for seed in [7, 122, 907] {
            let mut desc = species.mature_desc(&TreeDesc::default());
            desc.branching.seed = seed;
            // Axis foliage must not accidentally inherit recursive depth/offset semantics.
            desc.branching.iterations = 2;
            desc.leaf_offset = 3;
            for age in [0., 0.45, 1.] {
                let tree = Tree::new(desc.at_age(age));
                let anchors: std::collections::BTreeSet<_> =
                    tree.leaf_branch_indices().iter().copied().collect();
                let last_trunk = tree
                    .trunk_branch_indices()
                    .iter()
                    .copied()
                    .rfind(|&i| tree.axis_segments()[i].order == 0);
                let mut twig_tip = false;
                let mut twig_interior = false;
                for i in anchors {
                    let axis = tree.axis_segments()[i];
                    if axis.order == 0 {
                        assert_eq!(
                            Some(i),
                            last_trunk,
                            "young crowns follow the visible leader"
                        );
                    } else {
                        assert_eq!(axis.order, 2, "leaves attach to shoots, not scaffold wood");
                        if species == TreeSpecies::AppleTree {
                            assert_eq!(axis.u_end, 1., "apple leaf masses stay on twig tips");
                        }
                        twig_tip |= axis.u_end == 1.;
                        twig_interior |= axis.u_end < 1.;
                    }
                }
                if age == 1. {
                    assert!(twig_tip);
                    assert_eq!(twig_interior, species != TreeSpecies::AppleTree);
                }
            }
        }
    }
}

#[test]
fn plane_has_a_clear_trunk_broad_crown_and_its_own_bark_without_apples() {
    assert!(!TreeSpecies::PLANTABLE.contains(&TreeSpecies::WeepingWillow));
    for seed in [0, 7, 122, 907, u64::MAX] {
        let mut desc = TreeSpecies::LondonPlane.default_desc();
        desc.branching.seed = seed;
        assert!(!desc.species.bears_apples());
        let tree = Tree::new(desc.clone());
        assert_eq!(tree.bark_style(), 1);
        let first_lateral_height = tree
            .branches()
            .iter()
            .zip(tree.axis_segments())
            .filter(|(_, m)| m.order == 1)
            .map(|(b, _)| b.start.y)
            .fold(f32::INFINITY, f32::min);
        assert!(
            first_lateral_height > 35. * 0.66,
            "do not grow willow skirts at ground level"
        );
        let mut low = Vec3::splat(f32::INFINITY);
        let mut high = Vec3::splat(f32::NEG_INFINITY);
        for leaf in tree.relative_leaf_placements() {
            low = low.min(leaf.position);
            high = high.max(leaf.position);
        }
        let span = high - low;
        assert!(span.x.max(span.z) > tree.height_voxels() * 0.6);
        assert!(Tree::new(desc.at_age(0.25)).height_voxels() < tree.height_voxels() * 0.5);
    }
    assert_eq!(
        Tree::new(TreeSpecies::AppleTree.default_desc()).bark_style(),
        0
    );
}

#[test]
fn ginkgo_has_a_tapered_upright_crown_fan_sprays_and_no_apples() {
    use crate::util::stable_perpendicular_basis;
    for seed in [0, 7, 122, 907, u64::MAX] {
        let mut desc = TreeSpecies::Ginkgo.default_desc();
        desc.branching.seed = seed;
        assert!(!desc.species.bears_apples());
        assert_eq!(serde_json::to_value(desc.species).unwrap(), "ginkgo");
        let tree = Tree::new(desc.clone());
        assert_eq!(tree.bark_style(), 2);
        assert!((110. ..150.).contains(&tree.height_voxels()));
        let scaffolds: Vec<_> = tree
            .branches()
            .iter()
            .zip(tree.axis_segments())
            .filter(|(_, meta)| meta.order == 1 && meta.u_end == 1.)
            .collect();
        let lower_span = scaffolds
            .iter()
            .filter(|(b, _)| b.start.y < 70.)
            .map(|(b, _)| glam::Vec2::new(b.end.x, b.end.z).length())
            .fold(0., f32::max);
        let upper_span = scaffolds
            .iter()
            .filter(|(b, _)| b.start.y > 90.)
            .map(|(b, _)| glam::Vec2::new(b.end.x, b.end.z).length())
            .fold(0., f32::max);
        assert!(lower_span > upper_span * 1.3, "upper crown should narrow");
        let radius = 2.0_f32.powi(desc.leaves_size_level as i32) * 0.5;
        for (leaf, &branch) in tree
            .relative_leaf_placements()
            .iter()
            .zip(tree.leaf_branch_indices())
        {
            let b = tree.branches()[branch];
            let axis = (b.end - b.start).normalize();
            let (side, vertical) = stable_perpendicular_basis(axis);
            let local =
                leaf.position - leaf.anchor - axis * radius * desc.leaf_spray_tip_offset_ratio;
            let sample = Vec3::new(
                local.dot(axis) / radius,
                local.dot(side) / (radius * desc.leaf_spray_width_ratio),
                local.dot(vertical) / (radius * desc.leaf_spray_thickness_ratio),
            );
            assert!(sample.length_squared() <= 1.00001);
            assert!(sample.y.abs() <= (sample.x + 0.80) * 0.95 + 0.00001);
            assert!(sample.x <= 0.70 + 0.65 * sample.y.abs() + 0.00001);
        }
    }
}

#[test]
fn species_profiles_are_switching_defaults_not_overrides_of_live_authoring() {
    for species in TreeSpecies::PLANTABLE {
        let mut authored = species.mature_desc(&TreeDesc::default());
        authored.axis_model = false;
        authored.size = 23.5;
        authored.trunk_thickness = 0.27;
        authored.branching.seed = 907;
        authored.leaf_density = 0.071;
        authored.leaves_size_level = 3;
        authored.leaf_spray_width_ratio = 0.82;
        authored.leaf_spray_tip_offset_ratio = -0.12;
        authored.fruit_spawn_probability = 0.12;
        authored.enable_subdivision = true;
        let mut expected = authored.clone();
        expected.axis_model = true;
        assert_eq!(species.mature_desc(&authored), expected);
        let saved = serde_json::to_vec(&authored).unwrap();
        let restored: TreeDesc = serde_json::from_slice(&saved).unwrap();
        assert_eq!(species.mature_desc(&restored), expected);
    }
}

#[test]
fn new_apples_are_mature_human_scale_trees_across_individual_seeds() {
    for seed in (0..64).chain([122, 907, u64::MAX]) {
        let mut desc = TreeDesc::default();
        desc.branching.seed = seed;
        let tree = Tree::new(desc.at_age(1.));
        let height = tree.height_voxels();
        // Roughly 1.6–2.0 of the current 24-voxel player body; keep a voxel of
        // tolerance for the randomly sampled leaf envelope, rather than clipping crowns.
        assert!(
            (38.0..=49.0).contains(&height),
            "seed={seed} height={height}"
        );
        assert_eq!(
            tree.trunks().len(),
            tree.branches().len(),
            "not a hidden sapling"
        );
        assert!(!tree.relative_leaf_placements().is_empty());
        assert!(Tree::new(desc.at_age(0.)).height_voxels() < height * 0.5);
    }
}

#[test]
fn species_switches_use_native_sizes_without_resizing_current_authoring_or_retired_saves() {
    for from in TreeSpecies::ALL {
        let mut authored = from.default_desc();
        authored.size = 17.25;
        authored.branching.seed = u64::MAX;
        for to in TreeSpecies::ALL {
            let switched = to.mature_desc(&authored);
            assert_eq!(switched.branching.seed, authored.branching.seed);
            assert_eq!(
                switched.size,
                if from == to {
                    17.25
                } else {
                    to.default_desc().size
                }
            );
            if from != to && to != TreeSpecies::AppleTree {
                assert_eq!(
                    switched.size,
                    if to == TreeSpecies::LondonPlane {
                        20. * 0.66
                    } else {
                        20.
                    },
                    "only London Plane has a reduced non-Apple planting size"
                );
            }
        }
    }
    let mut retired = TreeDesc::legacy_wire_defaults();
    retired.size = 17.25;
    assert_eq!(retired.into_current().size, 17.25);
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
