// treegen.rs

use glam::Vec3;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

/// Descriptor matching the parameters from the Python version.
#[derive(Debug, Clone)]
pub struct TreeDesc {
    pub size: f32,
    pub trunkthickness: f32,
    pub spread: f32,
    pub twisted: f32,
    pub leaves: f32,
    pub gravity: f32,
    pub iterations: u32,
    pub wide: f32,
    pub seed: u64,
}

/// A round cone connecting two spheres, approximating a branch segment.
#[derive(Debug, Clone)]
pub struct RoundCone {
    pub sphere_a_radius: f32,
    pub sphere_b_radius: f32,
    pub sphere_a_center: Vec3,
    pub sphere_b_center: Vec3,
}

/// The generated tree, containing branch primitives and leaf spawn positions.
#[derive(Debug)]
pub struct Tree {
    pub desc: TreeDesc,
    pub trunks: Vec<RoundCone>,
    pub leaf_positions: Vec<Vec3>,
}

impl Tree {
    /// Construct a new Tree from the given descriptor.
    pub fn new(desc: TreeDesc) -> Self {
        let (trunks, leaf_positions) = build(&desc);
        Tree {
            desc,
            trunks,
            leaf_positions,
        }
    }
}

/// Build the tree: generate branch primitives and leaf positions.
pub fn build(desc: &TreeDesc) -> (Vec<RoundCone>, Vec<Vec3>) {
    let mut rng = StdRng::seed_from_u64(desc.seed);
    let mut trunks = Vec::new();
    let mut leaves = Vec::new();

    // Precompute branch length parameters and trunk thickness
    let size = 150.0 * desc.size / desc.iterations as f32;
    let branch_len_start = size * (1.0 - desc.wide);
    let branch_len_end = size * desc.wide;
    let trunk_thickness = desc.trunkthickness * desc.size * 6.0;

    // Recursive branch generation
    fn recurse(
        pos: Vec3,
        dir: Vec3,
        i: u32,
        desc: &TreeDesc,
        branch_len_start: f32,
        branch_len_end: f32,
        trunk_thickness: f32,
        trunks: &mut Vec<RoundCone>,
        leaves: &mut Vec<Vec3>,
        rng: &mut StdRng,
    ) {
        let iter_f = desc.iterations as f32;
        let t = ((i as f32) / iter_f).sqrt();
        let branch_len = branch_len_start + t * (branch_len_end - branch_len_start);

        let thickness_start = (1.0 - t) * trunk_thickness;
        let t_next = (((i + 1) as f32) / iter_f).sqrt();
        let thickness_end = (1.0 - t_next) * trunk_thickness;

        let end = pos + dir * branch_len;

        // Record this branch segment as a round cone
        trunks.push(RoundCone {
            sphere_a_radius: thickness_start,
            sphere_b_radius: thickness_end,
            sphere_a_center: pos,
            sphere_b_center: end,
        });

        if i < desc.iterations - 1 {
            // Decide branching
            let mut b = 1;
            let mut var = (i as f32) * 0.2 * desc.twisted;
            let branch_prob = t;
            if rng.random::<f32>() < branch_prob {
                b = 2;
                var = 2.0 * desc.spread * t;
            }
            for _ in 0..b {
                let dx = dir.x + rng.random_range(-var..=var);
                let dy = dir.y + rng.random_range(-var..=var);
                let dz = dir.z + rng.random_range(-var..=var);
                let new_dir = Vec3::new(dx, dy, dz).normalize_or_zero();
                recurse(
                    end,
                    new_dir,
                    i + 1,
                    desc,
                    branch_len_start,
                    branch_len_end,
                    trunk_thickness,
                    trunks,
                    leaves,
                    rng,
                );
            }
        } else {
            // Leaf spawn point at the midpoint of the last segment
            leaves.push((pos + end) * 0.5);
        }
    }

    // Start recursion from the origin, growing along +Z
    recurse(
        Vec3::ZERO,
        Vec3::Z,
        0,
        desc,
        branch_len_start,
        branch_len_end,
        trunk_thickness,
        &mut trunks,
        &mut leaves,
        &mut rng,
    );

    (trunks, leaves)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tree_generation_basic() {
        let desc = TreeDesc {
            size: 1.6,
            trunkthickness: 1.0,
            spread: 0.5,
            twisted: 0.5,
            leaves: 1.0,
            gravity: 0.0,
            iterations: 12,
            wide: 0.5,
            seed: 42,
        };
        let tree = Tree::new(desc.clone());
        // We expect at least one branch segment and at least one leaf position
        assert!(!tree.trunks.is_empty(), "No trunk segments generated");
        assert!(
            !tree.leaf_positions.is_empty(),
            "No leaf positions generated"
        );
        // Re-generating with the same seed should give the same number of primitives
        let tree2 = Tree::new(desc);
        assert_eq!(
            tree.trunks.len(),
            tree2.trunks.len(),
            "Trunk count differs for same seed"
        );
        assert_eq!(
            tree.leaf_positions.len(),
            tree2.leaf_positions.len(),
            "Leaf count differs for same seed"
        );

        // print the generated tree for debugging
        println!("Generated tree: {:#?}", tree);
    }
}
