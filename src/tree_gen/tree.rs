use glam::Vec3;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use crate::geom::{Cuboid, RoundCone};

#[derive(Debug, Clone)]
pub struct TreeDesc {
    pub size: f32,
    pub trunk_thickness: f32,
    pub trunk_thickness_min: f32,
    pub spread: f32,
    pub twisted: f32,
    /// The actual leaves size is 2^leaves_size_level
    pub leaves_size_level: u32,
    pub gravity: f32,
    pub iterations: u32,
    pub wide: f32,
    pub seed: u64,
}

impl Default for TreeDesc {
    fn default() -> Self {
        TreeDesc {
            size: 3.0,
            trunk_thickness: 1.45,
            // tested to be the minimum thickness of the trunk, otherwise normal calculation has probability to fail
            trunk_thickness_min: 1.05,
            spread: 0.47,
            twisted: 0.08,
            leaves_size_level: 5,
            gravity: 0.0,
            iterations: 12,
            wide: 0.5,
            seed: 30,
        }
    }
}

#[derive(Debug)]
pub struct Tree {
    desc: TreeDesc,
    trunks: Vec<RoundCone>,
    leaf_positions: Vec<Vec3>,
}

impl Tree {
    pub fn new(desc: TreeDesc) -> Self {
        let (trunks, leaf_positions) = Self::build(&desc);
        Tree {
            desc,
            trunks,
            leaf_positions,
        }
    }

    pub fn desc(&self) -> &TreeDesc {
        &self.desc
    }

    pub fn trunks(&self) -> &[RoundCone] {
        &self.trunks
    }

    pub fn leaves(&self) -> Vec<Cuboid> {
        let mut leaves = Vec::new();
        for pos in &self.leaf_positions {
            let half_size = Vec3::splat(2_u32.pow(self.desc.leaves_size_level) as f32 * 0.5);
            leaves.push(Cuboid::new(*pos, half_size));
        }
        leaves
    }

    /// Build the tree: generate branch primitives and leaf positions.
    fn build(desc: &TreeDesc) -> (Vec<RoundCone>, Vec<Vec3>) {
        let mut rng = StdRng::seed_from_u64(desc.seed);
        let mut trunks = Vec::new();
        let mut leaves = Vec::new();

        // Precompute branch length parameters and trunk thickness
        let size = 150.0 * desc.size / desc.iterations as f32;
        let branch_len_start = size * (1.0 - desc.wide);
        let branch_len_end = size * desc.wide;
        let trunk_thickness = desc.trunk_thickness * desc.size * 6.0;

        // Start recursion from the origin, growing along +Z
        recurse(
            Vec3::ZERO,
            Vec3::Y,
            0,
            desc,
            branch_len_start,
            branch_len_end,
            trunk_thickness,
            &mut trunks,
            &mut leaves,
            &mut rng,
        );

        return (trunks, leaves);

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

            let t_next = (((i + 1) as f32) / iter_f).sqrt();
            let mut thickness_start = (1.0 - t) * trunk_thickness;
            let mut thickness_end = (1.0 - t_next) * trunk_thickness;

            thickness_start = thickness_start.max(desc.trunk_thickness_min);
            thickness_end = thickness_end.max(desc.trunk_thickness_min);

            let end = pos + dir * branch_len;

            // Record this branch segment as a round cone
            trunks.push(RoundCone::new(thickness_start, pos, thickness_end, end));

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
    }
}
