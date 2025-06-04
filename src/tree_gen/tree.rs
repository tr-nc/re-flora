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
            trunk_thickness: 0.6,
            trunk_thickness_min: 1.05,
            spread: 0.47,
            twisted: 0.08,
            leaves_size_level: 5,
            gravity: 0.0, // Default gravity is 0.0 (no effect)
            iterations: 12,
            wide: 0.5,
            seed: 30,
        }
    }
}

impl TreeDesc {
    pub fn edit_by_gui(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;

        changed |= ui
            .add(
                egui::Slider::new(&mut self.size, 0.1..=50.0)
                    .text("Tree Size")
                    .logarithmic(true),
            )
            .changed();

        changed |= ui
            .add(egui::Slider::new(&mut self.trunk_thickness, 0.01..=5.0).text("Trunk Thickness"))
            .changed();

        changed |= ui
            .add(
                egui::Slider::new(&mut self.trunk_thickness_min, 0.001..=2.0)
                    .text("Min Trunk Thickness"),
            )
            .changed();

        changed |= ui
            .add(egui::Slider::new(&mut self.spread, 0.0..=1.0).text("Spread"))
            .changed();

        changed |= ui
            .add(egui::Slider::new(&mut self.spread, 0.0..=1.0).text("Spread"))
            .changed();

        changed |= ui
            .add(egui::Slider::new(&mut self.twisted, 0.0..=1.0).text("Twisted"))
            .changed();

        changed |= ui
            .add(
                egui::Slider::new(&mut self.leaves_size_level, 0..=8)
                    .text("Leaves Size Level (2^level)"),
            )
            .changed();
        // Note: 2^8 = 256. Adjust max level as needed.

        changed |= ui
            .add(egui::Slider::new(&mut self.gravity, -2.0..=2.0).text("Gravity"))
            .changed();

        changed |= ui
            .add(egui::Slider::new(&mut self.iterations, 1..=12).text("Iterations"))
            .changed();
        // Iterations can heavily impact performance, so keep the max reasonable.

        changed |= ui
            .add(egui::Slider::new(&mut self.wide, 0.0..=5.0).text("Wide"))
            .changed();

        changed |= ui
            .add(
                egui::DragValue::new(&mut self.seed)
                    .speed(1.0) // Controls how fast the value changes when dragging
                    .range(0..=u64::MAX) // Optional: clamp to a specific range
                    .prefix("Seed: "),
            )
            .changed();

        return changed;
    }
}

#[derive(Debug)]
struct BuiltObjects {
    trunks: Vec<RoundCone>,
    leaves: Vec<Cuboid>,
}

#[derive(Debug)]
pub struct Tree {
    desc: TreeDesc,
    built_objects: BuiltObjects,
}

impl Tree {
    pub fn new(desc: TreeDesc) -> Self {
        let built_objects = Self::build(&desc);
        Tree {
            desc,
            built_objects,
        }
    }

    pub fn desc(&self) -> &TreeDesc {
        &self.desc
    }

    pub fn trunks(&self) -> &[RoundCone] {
        &self.built_objects.trunks
    }

    pub fn leaves(&self) -> &[Cuboid] {
        &self.built_objects.leaves
    }

    /// Build the tree: generate branch primitives and leaf positions.
    fn build(desc: &TreeDesc) -> BuiltObjects {
        let mut rng = StdRng::seed_from_u64(desc.seed);
        let mut trunks = Vec::new();
        let mut leaves_positions = Vec::new();

        // Precompute branch length parameters and trunk thickness
        let size = 150.0 * desc.size / desc.iterations as f32;
        let branch_len_start = size * (1.0 - desc.wide);
        let branch_len_end = size * desc.wide;
        let trunk_thickness = desc.trunk_thickness * desc.size * 6.0;

        // Start recursion from the origin, growing along +Y (assuming Y is up)
        recurse(
            Vec3::ZERO,
            Vec3::Y, // Initial growth direction (upwards)
            0,
            desc,
            branch_len_start,
            branch_len_end,
            trunk_thickness,
            &mut trunks,
            &mut leaves_positions,
            &mut rng,
        );

        let leaves = make_leaves(&leaves_positions, desc.leaves_size_level);

        return BuiltObjects { trunks, leaves };

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
            // t ranges from 0 (trunk base) to almost 1 (branch tips)
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
                let mut b = 1; // Number of child branches
                let mut var = (i as f32) * 0.2 * desc.twisted; // Variance for direction change
                let branch_prob = t; // Probability of branching increases with t
                if rng.gen::<f32>() < branch_prob {
                    // Assuming rng.random() was a stand-in for rng.gen()
                    b = 2;
                    var = 2.0 * desc.spread * t;
                }
                for _ in 0..b {
                    // Calculate random components for the new direction
                    let rand_dx = rng.gen_range(-var..=var); // Assuming rng.random_range was rng.gen_range
                    let rand_dy = rng.gen_range(-var..=var);
                    let rand_dz = rng.gen_range(-var..=var);

                    // Initial new direction components based on current direction and randomization
                    let next_dir_x_unnormalized = dir.x + rand_dx;
                    let mut next_dir_y_unnormalized = dir.y + rand_dy;
                    let next_dir_z_unnormalized = dir.z + rand_dz;

                    // --- MODIFICATION START: Apply gravity ---
                    // desc.gravity is expected to be in [0, 1].
                    // 't' scales the effect, so outer/later branches (larger 't') are more affected.
                    // The main trunk (i=0, t=0) is not affected by this pull.
                    let downward_pull = desc.gravity * t;
                    next_dir_y_unnormalized -= downward_pull; // Reduce y-component to simulate droop
                                                              // --- MODIFICATION END ---

                    let new_dir = Vec3::new(
                        next_dir_x_unnormalized,
                        next_dir_y_unnormalized,
                        next_dir_z_unnormalized,
                    )
                    .normalize_or_zero();

                    // If new_dir becomes zero (e.g., due to strong gravity exactly countering upward growth),
                    // the recursion for this branch effectively stops as future segments will have zero length.
                    // This might be desired or could be handled by ensuring new_dir is never zero if problematic.
                    if new_dir != Vec3::ZERO {
                        // Added a check to prevent recursion with zero direction
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
                }
            } else {
                // Leaf spawn point at the midpoint of the last segment
                leaves.push((pos + end) * 0.5);
            }
        }

        fn make_leaves(leaves_positions: &[Vec3], leaves_size_level: u32) -> Vec<Cuboid> {
            let mut leaves = Vec::new();
            for pos in leaves_positions {
                let half_size = Vec3::splat(2_u32.pow(leaves_size_level) as f32 * 0.5);
                leaves.push(Cuboid::new(*pos, half_size));
            }
            leaves
        }
    }
}
