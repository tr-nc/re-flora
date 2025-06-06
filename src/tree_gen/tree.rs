use glam::Vec3;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::f32::consts::PI;

use crate::geom::{Cuboid, RoundCone};

#[derive(Debug, Clone)]
pub struct TreeDesc {
    pub size: f32,
    pub trunk_thickness: f32,
    /// DO NOT change how the trunk_thickness_min works, it is a restriction!
    pub trunk_thickness_min: f32,
    /// How much branches spread outward (0.0 = straight up, 1.0 = very spread)
    pub spread: f32,
    /// How much randomness in branch directions (0.0 = very regular, 1.0 = chaotic)
    pub randomness: f32,
    /// Overall vertical growth tendency (-1.0 = droopy, 0.0 = neutral, 1.0 = strongly upward)
    pub vertical_tendency: f32,
    /// Minimum angle between branches (in radians)
    pub branch_angle_min: f32,
    /// Maximum angle between branches (in radians)
    pub branch_angle_max: f32,
    /// Probability of branching at each level (0.0 = never, 1.0 = always)
    pub branch_probability: f32,
    /// Minimum number of branches per split
    pub branch_count_min: u32,
    /// Maximum number of branches per split
    pub branch_count_max: u32,
    /// The actual leaves size is 2^leaves_size_level
    pub leaves_size_level: u32,
    /// Number of branching iterations
    pub iterations: u32,
    /// How much the trunk length varies between levels
    pub length_variation: f32,
    /// Total height/length of the tree from root to tip
    pub tree_height: f32,
    /// How much length reduces per level (0.5 = half length each level, 1.0 = no reduction)
    pub length_dropoff: f32,
    /// How much thickness reduces per level (0.5 = half thickness each level)
    pub thickness_reduction: f32,
    /// Seed for randomization
    pub seed: u64,
}

impl Default for TreeDesc {
    fn default() -> Self {
        TreeDesc {
            // Basic Properties (from image)
            size: 50.0,                // Tree Size
            trunk_thickness: 0.20,     // Trunk Thickness
            trunk_thickness_min: 1.05, // Min Trunk Thickness
            iterations: 6,             // Iterations

            // Tree Shape
            tree_height: 10.0,         // Tree Height
            spread: 0.00,              // Spread
            vertical_tendency: 0.10,   // Vertical Tendency (up/downward)
            length_variation: 0.05,    // Length Variation
            length_dropoff: 0.70,      // Length Dropoff per Level
            thickness_reduction: 0.70, // Thickness Reduction

            // Branching Control
            branch_probability: 0.57,            // Branch Probability
            branch_count_min: 2,                 // Min Branches
            branch_count_max: 3,                 // Max Branches
            branch_angle_min: 28.0 * PI / 180.0, // Min Branch Angle (28°)
            branch_angle_max: 37.0 * PI / 180.0, // Max Branch Angle (37°)

            // Variation
            randomness: 0.23,     // Randomness
            leaves_size_level: 5, // Leaves Size Level (2^level)

            // Seed
            seed: 30,
        }
    }
}

impl TreeDesc {
    pub fn edit_by_gui(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;

        ui.heading("Basic Properties");
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
            .add(egui::Slider::new(&mut self.iterations, 1..=12).text("Iterations"))
            .changed();

        ui.separator();
        ui.heading("Tree Shape");

        changed |= ui
            .add(egui::Slider::new(&mut self.tree_height, 0.5..=50.0).text("Tree Height"))
            .changed();

        changed |= ui
            .add(egui::Slider::new(&mut self.spread, 0.0..=2.0).text("Spread"))
            .changed();

        changed |= ui
            .add(
                egui::Slider::new(&mut self.vertical_tendency, -1.0..=1.0)
                    .text("Vertical Tendency (upward/downward)"),
            )
            .changed();

        changed |= ui
            .add(egui::Slider::new(&mut self.length_variation, 0.0..=1.0).text("Length Variation"))
            .changed();

        changed |= ui
            .add(
                egui::Slider::new(&mut self.length_dropoff, 0.1..=1.0)
                    .text("Length Dropoff per Level"),
            )
            .changed();

        changed |= ui
            .add(
                egui::Slider::new(&mut self.thickness_reduction, 0.0..=1.0)
                    .text("Thickness Reduction"),
            )
            .changed();

        ui.separator();
        ui.heading("Branching Control");

        changed |= ui
            .add(
                egui::Slider::new(&mut self.branch_probability, 0.0..=1.0)
                    .text("Branch Probability"),
            )
            .changed();

        changed |= ui
            .add(egui::Slider::new(&mut self.branch_count_min, 1..=5).text("Min Branches"))
            .changed();

        changed |= ui
            .add(egui::Slider::new(&mut self.branch_count_max, 1..=8).text("Max Branches"))
            .changed();

        let mut angle_min_deg = self.branch_angle_min.to_degrees();
        let mut angle_max_deg = self.branch_angle_max.to_degrees();

        changed |= ui
            .add(egui::Slider::new(&mut angle_min_deg, 0.0..=90.0).text("Min Branch Angle (deg)"))
            .changed();

        changed |= ui
            .add(egui::Slider::new(&mut angle_max_deg, 0.0..=120.0).text("Max Branch Angle (deg)"))
            .changed();

        if changed {
            self.branch_angle_min = angle_min_deg.to_radians();
            self.branch_angle_max = angle_max_deg.to_radians();
            // Ensure min <= max
            if self.branch_angle_min > self.branch_angle_max {
                self.branch_angle_max = self.branch_angle_min;
            }
            if self.branch_count_min > self.branch_count_max {
                self.branch_count_max = self.branch_count_min;
            }
        }

        ui.separator();
        ui.heading("Variation");

        changed |= ui
            .add(egui::Slider::new(&mut self.randomness, 0.0..=1.0).text("Randomness"))
            .changed();

        changed |= ui
            .add(
                egui::Slider::new(&mut self.leaves_size_level, 0..=8)
                    .text("Leaves Size Level (2^level)"),
            )
            .changed();

        changed |= ui
            .add(
                egui::DragValue::new(&mut self.seed)
                    .speed(1.0)
                    .prefix("Seed: "),
            )
            .changed();

        changed
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

    fn build(desc: &TreeDesc) -> BuiltObjects {
        let mut rng = StdRng::seed_from_u64(desc.seed);
        let mut trunks = Vec::new();
        let mut leaves_positions = Vec::new();

        // Calculate base segment length based on total tree height and iterations
        let base_length = desc.tree_height * desc.size / (desc.iterations as f32);
        let base_thickness = desc.trunk_thickness * desc.size;

        // Start the recursion
        recurse(
            Vec3::ZERO,
            Vec3::Y,
            0,
            desc,
            base_length,
            base_thickness,
            &mut trunks,
            &mut leaves_positions,
            &mut rng,
        );

        let leaves = make_leaves(&leaves_positions, desc.leaves_size_level);

        BuiltObjects { trunks, leaves }
    }
}

fn recurse(
    pos: Vec3,
    dir: Vec3,
    level: u32,
    desc: &TreeDesc,
    length: f32,
    thickness: f32,
    trunks: &mut Vec<RoundCone>,
    leaves: &mut Vec<Vec3>,
    rng: &mut StdRng,
) {
    if level >= desc.iterations {
        // Add leaf at the tip
        leaves.push(pos);
        return;
    }

    // Calculate length for this segment with some variation
    let length_variation = if desc.length_variation > 0.0 {
        rng.gen_range(1.0 - desc.length_variation..=1.0 + desc.length_variation)
    } else {
        1.0
    };

    let segment_length = length * length_variation;

    // Calculate thickness for this segment
    let thickness_start = thickness.max(desc.trunk_thickness_min);

    // Apply thickness reduction, but ensure smooth transitions
    let natural_thickness_end = if desc.thickness_reduction > 0.0 {
        thickness * desc.thickness_reduction
    } else {
        // When thickness_reduction is 0, gradually taper to very thin
        thickness * 0.1_f32.powf((level + 1) as f32)
    };

    let thickness_end = natural_thickness_end.max(desc.trunk_thickness_min);

    // Apply vertical tendency to direction
    // Positive values bias upward, negative values create droopy branches
    let level_factor = (level as f32) / (desc.iterations as f32);
    let vertical_influence = desc.vertical_tendency * level_factor;
    let adjusted_dir = (dir + Vec3::new(0.0, vertical_influence, 0.0)).normalize_or_zero();

    let end_pos = pos + adjusted_dir * segment_length;

    // Add trunk segment
    trunks.push(RoundCone::new(thickness_start, pos, thickness_end, end_pos));

    // Decide if we should branch
    let should_branch =
        level < desc.iterations - 1 && (level == 0 || rng.gen::<f32>() < desc.branch_probability);

    if should_branch {
        // Determine number of branches
        let branch_count = if desc.branch_count_min == desc.branch_count_max {
            desc.branch_count_min
        } else {
            rng.gen_range(desc.branch_count_min..=desc.branch_count_max)
        };

        // Create branches
        for i in 0..branch_count {
            let new_dir =
                calculate_branch_direction(adjusted_dir, i, branch_count, level, desc, rng);

            if new_dir != Vec3::ZERO {
                recurse(
                    end_pos,
                    new_dir,
                    level + 1,
                    desc,
                    length * desc.length_dropoff, // Use configurable dropoff
                    thickness_end,
                    trunks,
                    leaves,
                    rng,
                );
            }
        }
    } else {
        // No branching, continue straight with some variation
        let new_dir = add_direction_variation(adjusted_dir, desc.randomness * 0.2, rng);

        recurse(
            end_pos,
            new_dir,
            level + 1,
            desc,
            length * desc.length_dropoff, // Use same dropoff for consistency
            thickness_end,
            trunks,
            leaves,
            rng,
        );
    }
}

fn calculate_branch_direction(
    parent_dir: Vec3,
    branch_index: u32,
    total_branches: u32,
    level: u32,
    desc: &TreeDesc,
    rng: &mut StdRng,
) -> Vec3 {
    // Create a more natural branching pattern
    let golden_angle = 2.4; // Approximately golden angle for natural spiral

    // Base angle around the parent direction
    let around_angle = if total_branches > 1 {
        (branch_index as f32) * (2.0 * PI) / (total_branches as f32) + (level as f32) * golden_angle
    } else {
        rng.gen::<f32>() * 2.0 * PI
    };

    // Angle away from parent direction
    let away_angle =
        rng.gen_range(desc.branch_angle_min..=desc.branch_angle_max) * (1.0 + desc.spread);

    // Create perpendicular vectors to parent direction
    let up = if parent_dir.y.abs() < 0.9 {
        Vec3::Y
    } else {
        Vec3::X
    };

    let right = parent_dir.cross(up).normalize_or_zero();
    let forward = parent_dir.cross(right).normalize_or_zero();

    // Create branch direction
    let branch_dir = {
        // Base direction rotated around parent
        let rotated_perp = right * around_angle.cos() + forward * around_angle.sin();

        // Blend between parent direction and perpendicular
        let base_dir = parent_dir * away_angle.cos() + rotated_perp * away_angle.sin();

        base_dir.normalize_or_zero()
    };

    // Add randomness
    add_direction_variation(branch_dir, desc.randomness, rng)
}

fn add_direction_variation(dir: Vec3, variation: f32, rng: &mut StdRng) -> Vec3 {
    if variation <= 0.0 {
        return dir;
    }

    let rand_x = rng.gen_range(-variation..=variation);
    let rand_y = rng.gen_range(-variation..=variation);
    let rand_z = rng.gen_range(-variation..=variation);

    (dir + Vec3::new(rand_x, rand_y, rand_z)).normalize_or_zero()
}

fn make_leaves(leaves_positions: &[Vec3], leaves_size_level: u32) -> Vec<Cuboid> {
    let mut leaves = Vec::new();
    let leaf_actual_size = 2_u32.pow(leaves_size_level) as f32;
    for pos in leaves_positions {
        leaves.push(Cuboid::new(*pos, Vec3::splat(leaf_actual_size * 0.5)));
    }
    leaves
}
