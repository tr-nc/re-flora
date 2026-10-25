//! Explicit same-machine refactor evidence, not a cross-platform float golden or a benchmark.
//! Run before/after a generator change and compare the TREE_GENERATION_FINGERPRINT lines.
use super::{Tree, TreeDesc, TreeSpecies};
use crate::{branch_skeleton::BranchSegmentRole, geom::RoundCone};
use glam::Vec3;
use std::hash::{Hash, Hasher};

struct Fingerprint(u64);

impl Hasher for Fingerprint {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = (self.0 ^ byte as u64).wrapping_mul(0x100000001b3);
        }
    }
}

impl Fingerprint {
    fn vector(&mut self, value: Vec3) {
        value.to_array().map(f32::to_bits).hash(self);
    }

    fn cones(&mut self, cones: &[RoundCone]) {
        cones.len().hash(self);
        for cone in cones {
            self.vector(cone.center_a());
            self.vector(cone.center_b());
            cone.radius_a().to_bits().hash(self);
            cone.radius_b().to_bits().hash(self);
        }
    }
}

fn capture(label: &str, desc: &TreeDesc, age: f32) {
    // Use the same serialized-description -> age -> Tree interface as snapshot restoration.
    let saved = serde_json::to_vec(desc).unwrap();
    let restored: TreeDesc = serde_json::from_slice(&saved).unwrap();
    let tree = Tree::new(restored.at_age(age));
    let mut hash = Fingerprint(0xcbf29ce484222325);
    saved.hash(&mut hash);
    tree.branches().len().hash(&mut hash);
    for branch in tree.branches() {
        branch.parent.hash(&mut hash);
        hash.vector(branch.start);
        hash.vector(branch.end);
        branch.level.hash(&mut hash);
        matches!(branch.role, BranchSegmentRole::MainAxis).hash(&mut hash);
    }
    tree.axis_segments().len().hash(&mut hash);
    for axis in tree.axis_segments() {
        axis.axis_id.hash(&mut hash);
        axis.order.hash(&mut hash);
        for value in [
            axis.u_start,
            axis.u_end,
            axis.radius_start,
            axis.radius_end,
            axis.reveal_age,
        ] {
            value.to_bits().hash(&mut hash);
        }
    }
    hash.cones(tree.trunks());
    hash.cones(&tree.world_physical_trunks(Vec3::new(0.7, 0.5, 0.8)));
    tree.trunk_branch_indices().hash(&mut hash);
    tree.leaf_branch_indices().hash(&mut hash);
    tree.relative_leaf_placements().len().hash(&mut hash);
    for leaf in tree.relative_leaf_placements() {
        hash.vector(leaf.position);
        hash.vector(leaf.anchor);
    }
    println!(
        "TREE_GENERATION_FINGERPRINT {}",
        serde_json::json!({
            "case": label, "species": desc.species, "seed": desc.branching.seed,
            "axis_model": desc.axis_model, "age": age, "segments": tree.branches().len(),
            "wood": tree.trunks().len(), "leaves": tree.relative_leaf_placements().len(),
            "fingerprint": format!("{:016x}", hash.finish()),
        })
    );
}

#[test]
#[ignore = "explicit same-machine before/after generation capture; not a portable float golden"]
fn capture_tree_generation_compatibility() {
    for species in TreeSpecies::ALL {
        let modes: &[bool] = if species == TreeSpecies::Conifer {
            &[false, true]
        } else {
            &[false]
        };
        for &axis_model in modes {
            for seed in [7, 122, 907] {
                for edited in [false, true] {
                    let mut desc = species.mature_desc(&TreeDesc::default());
                    desc.axis_model = axis_model;
                    desc.branching.seed = seed;
                    if edited {
                        desc.size = 24.75;
                        desc.trunk_thickness = 0.23;
                        desc.leaf_density *= 0.8;
                        desc.enable_subdivision = !desc.enable_subdivision;
                    }
                    for age in [0., 0.25, 0.45, 1.] {
                        capture(if edited { "edited" } else { "profile" }, &desc, age);
                    }
                }
            }
        }
    }
    for index in 0..crate::garden_layout::TREE_CENTERS.len() {
        let desc = crate::garden_layout::tree_descriptor(crate::garden_layout::DEFAULT_SEED, index);
        capture(&format!("opening-{index}"), &desc, 1.);
    }
}
