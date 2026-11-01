use super::*;
use crate::tree_gen::{Tree, TreeForm, TreeSpecies};

#[test]
fn veteran_ginkgo_is_broad_dense_irregular_and_branch_bound() {
    for seed in [0, 7, 122, 907, u64::MAX] {
        let mut desc = TreeSpecies::Ginkgo.default_desc();
        desc.branching.seed = seed;
        desc.form = TreeForm::Veteran;
        desc.size = 48.;
        desc.trunk_thickness = 0.48;
        desc.leaf_density = 0.22;
        desc.leaf_spray_width_ratio = 1.2;
        desc.leaf_spray_thickness_ratio = 0.58;
        let tree = Tree::new(desc.clone());
        let width = crate::garden_layout::tree_radius_voxels(&desc) * 2.;
        assert!(
            width > tree.height_voxels() * 0.9,
            "broad veteran crown: seed={seed} width={width} height={}",
            tree.height_voxels()
        );
        assert!(tree.relative_leaf_placements().len() > 100_000);
        assert!(tree.trunks()[0].radius_a() > 30.);
        let roots: Vec<_> = tree
            .branches()
            .iter()
            .zip(tree.axis_segments())
            .filter(|(_, m)| m.order == 0)
            .map(|(b, _)| b.end)
            .collect();
        assert!(roots
            .windows(3)
            .any(|p| (p[1] - p[0]).cross(p[2] - p[1]).length() > 1.));
        for (i, (branch, axis)) in tree.branches().iter().zip(tree.axis_segments()).enumerate() {
            if let Some(parent) = branch.parent {
                assert!(parent < i);
                assert_eq!(branch.start, tree.branches()[parent].end);
                assert!(axis.reveal_age >= tree.axis_segments()[parent].reveal_age);
            }
        }
        for (leaf, &branch) in tree
            .relative_leaf_placements()
            .iter()
            .zip(tree.leaf_branch_indices())
        {
            assert_eq!(leaf.anchor, tree.branches()[branch].end);
        }
        let serialized = serde_json::to_string(&desc).unwrap();
        assert!(serialized.contains("\"form\":\"veteran\""));
        let restored: TreeDesc = serde_json::from_str(&serialized).unwrap();
        assert_eq!(desc, restored);
        for age in [0.25, 0.65, 1.] {
            let young = Tree::new(desc.at_age(age));
            assert_eq!(young.branches().len(), tree.branches().len());
            for (a, b) in young.branches().iter().zip(tree.branches()) {
                assert_eq!(a.parent, b.parent);
                assert_eq!(a.role, b.role);
                assert_eq!(a.level, b.level);
            }
        }
    }
}

#[test]
fn old_ginkgo_descriptions_keep_their_previous_geometry() {
    let desc = TreeSpecies::Ginkgo.default_desc();
    let json = serde_json::to_string(&desc).unwrap();
    assert!(!json.contains("\"form\""));
    let restored: TreeDesc = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.form, TreeForm::Standard);
    assert_eq!(Tree::new(desc).branches(), Tree::new(restored).branches());
}
