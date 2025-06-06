use glam::Vec3;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::f32::consts::PI;

// Assuming Cuboid is defined in crate::geom as per the original code.
use crate::geom::{Cuboid, RoundCone};
// Use the provided RoundCone definition.
// use crate::geom::RoundCone; // This is now defined above.

#[derive(Debug, Clone)]
pub struct TreeDesc {
    pub size: f32,
    pub trunk_thickness: f32,
    pub trunk_thickness_min: f32,
    pub spread: f32,
    pub randomness: f32,
    pub vertical_tendency: f32,
    pub branch_angle_min: f32,
    pub branch_angle_max: f32,
    pub branch_probability: f32,
    pub branch_count_min: u32,
    pub branch_count_max: u32,
    pub leaves_size_level: u32,
    pub iterations: u32,
    pub length_variation: f32,
    pub tree_height: f32,
    pub length_dropoff: f32,
    pub thickness_reduction: f32,
    pub seed: u64,

    // Subdivision Parameters
    pub subdivision_threshold: f32,
    pub subdivision_count_min: u32,
    pub subdivision_count_max: u32,
    pub subdivision_randomness: f32,
}
impl Default for TreeDesc {
    fn default() -> Self {
        TreeDesc {
            // Basic Properties
            size: 50.0,                // Tree Size
            trunk_thickness: 0.20,     // Trunk Thickness
            trunk_thickness_min: 1.05, // Min Trunk Thickness

            // Tree Shape
            tree_height: 10.0,         // Tree Height
            spread: 0.00,              // Spread
            vertical_tendency: 0.25,   // Vertical Tendency (up/downward)
            length_variation: 0.15,    // Length Variation
            length_dropoff: 0.66,      // Length Dropoff per Level
            thickness_reduction: 0.70, // Thickness Reduction

            // Branching Control
            branch_probability: 0.57,            // Branch Probability
            branch_count_min: 2,                 // Min Branches
            branch_count_max: 3,                 // Max Branches
            branch_angle_min: 24.0 * PI / 180.0, // Min Branch Angle (24°)
            branch_angle_max: 48.0 * PI / 180.0, // Max Branch Angle (48°)

            // Variation
            randomness: 0.22,     // Randomness
            leaves_size_level: 5, // Leaves Size Level (2^level)

            // Iterations
            iterations: 6, // Iterations

            // Subdivision Parameters
            subdivision_threshold: 10.2,  // Subdivision Threshold
            subdivision_count_min: 2,     // Min Subdivisions
            subdivision_count_max: 7,     // Max Subdivisions
            subdivision_randomness: 0.15, // Subdivision Randomness

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
            if self.branch_angle_min > self.branch_angle_max {
                self.branch_angle_max = self.branch_angle_min;
            }
            if self.branch_count_min > self.branch_count_max {
                self.branch_count_max = self.branch_count_min;
            }
        }

        ui.separator();
        ui.heading("Subdivision");

        changed |= ui
            .add(
                egui::Slider::new(&mut self.subdivision_threshold, 0.1..=20.0)
                    .text("Subdivision Threshold"),
            )
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut self.subdivision_count_min, 1..=10).text("Min Subdivisions"),
            )
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut self.subdivision_count_max, 1..=10).text("Max Subdivisions"),
            )
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut self.subdivision_randomness, 0.0..=1.0)
                    .text("Subdivision Randomness"),
            )
            .changed();

        if changed {
            if self.subdivision_count_min > self.subdivision_count_max {
                self.subdivision_count_max = self.subdivision_count_min;
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
        let mut initial_trunks = Vec::new();
        let mut leaves_positions = Vec::new();

        let base_length = desc.tree_height * desc.size / (desc.iterations as f32);
        let base_thickness = desc.trunk_thickness * desc.size;

        recurse(
            Vec3::ZERO,
            Vec3::Y,
            0,
            desc,
            base_length,
            base_thickness,
            &mut initial_trunks,
            &mut leaves_positions,
            &mut rng,
        );

        let mut trunks = Vec::new();
        for cone in &initial_trunks {
            let subdivided_cones = subdivide_trunk_segment(cone, desc, &mut rng);
            trunks.extend(subdivided_cones);
        }

        let leaves = make_leaves(&leaves_positions, desc.leaves_size_level);

        BuiltObjects { trunks, leaves }
    }
}

/// **UPDATED FUNCTION**
/// Subdivides a single RoundCone into multiple, smaller, slightly perturbed cones.
fn subdivide_trunk_segment(cone: &RoundCone, desc: &TreeDesc, rng: &mut StdRng) -> Vec<RoundCone> {
    // Use accessor methods from the provided RoundCone definition
    let axis = cone.center_b() - cone.center_a();
    let length = axis.length();

    if length <= desc.subdivision_threshold || desc.subdivision_count_max <= 1 {
        return vec![cone.clone()];
    }

    let num_segments = if desc.subdivision_count_min >= desc.subdivision_count_max {
        desc.subdivision_count_min
    } else {
        rng.gen_range(desc.subdivision_count_min..=desc.subdivision_count_max)
    };

    if num_segments <= 1 {
        return vec![cone.clone()];
    }

    let mut subdivided_trunks = Vec::with_capacity(num_segments as usize);
    // Use accessor methods to get initial state
    let mut current_pos = cone.center_a();
    let mut current_radius = cone.radius_a();

    let segment_vec = axis / num_segments as f32;

    let up = if axis.normalize_or_zero().y.abs() < 0.9 {
        Vec3::Y
    } else {
        Vec3::X
    };
    let perp1 = axis.cross(up).normalize_or_zero();
    let perp2 = axis.cross(perp1).normalize_or_zero();

    for i in 1..=num_segments {
        // Interpolate radius manually.
        let end_radius = cone.radius_b();
        let t_radius = 1.0 / (num_segments - i + 1) as f32;
        let next_radius = current_radius * (1.0 - t_radius) + end_radius * t_radius;

        let mut next_pos;

        if i == num_segments {
            // Ensure the last segment ends exactly at the original endpoint.
            next_pos = cone.center_b();
        } else {
            // Use accessor to get the starting position for interpolation.
            next_pos = cone.center_a() + segment_vec * i as f32;

            if desc.subdivision_randomness > 0.0 {
                let random_angle = rng.gen_range(0.0..=2.0 * PI);
                let random_dir_perp = perp1 * random_angle.cos() + perp2 * random_angle.sin();
                let displacement_magnitude =
                    segment_vec.length() * desc.subdivision_randomness * rng.gen_range(0.5..=1.0);
                next_pos += random_dir_perp * displacement_magnitude;
            }
        }

        // Create the new segment using the RoundCone constructor.
        subdivided_trunks.push(RoundCone::new(
            current_radius,
            current_pos,
            next_radius,
            next_pos,
        ));

        current_pos = next_pos;
        current_radius = next_radius;
    }

    subdivided_trunks
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
        leaves.push(pos);
        return;
    }

    let length_variation = if desc.length_variation > 0.0 {
        rng.gen_range(1.0 - desc.length_variation..=1.0 + desc.length_variation)
    } else {
        1.0
    };
    let segment_length = length * length_variation;
    let thickness_start = thickness.max(desc.trunk_thickness_min);
    let natural_thickness_end = if desc.thickness_reduction > 0.0 {
        thickness * desc.thickness_reduction
    } else {
        thickness * 0.1_f32.powf((level + 1) as f32)
    };
    let thickness_end = natural_thickness_end.max(desc.trunk_thickness_min);

    let level_factor = (level as f32) / (desc.iterations as f32);
    let vertical_influence = desc.vertical_tendency * level_factor;
    let adjusted_dir = (dir + Vec3::new(0.0, vertical_influence, 0.0)).normalize_or_zero();

    let end_pos = pos + adjusted_dir * segment_length;

    // This call is compatible with the new RoundCone::new signature.
    trunks.push(RoundCone::new(thickness_start, pos, thickness_end, end_pos));

    let should_branch =
        level < desc.iterations - 1 && (level == 0 || rng.gen::<f32>() < desc.branch_probability);

    if should_branch {
        let branch_count = if desc.branch_count_min == desc.branch_count_max {
            desc.branch_count_min
        } else {
            rng.gen_range(desc.branch_count_min..=desc.branch_count_max)
        };

        for i in 0..branch_count {
            let new_dir =
                calculate_branch_direction(adjusted_dir, i, branch_count, level, desc, rng);

            if new_dir != Vec3::ZERO {
                recurse(
                    end_pos,
                    new_dir,
                    level + 1,
                    desc,
                    length * desc.length_dropoff,
                    thickness_end,
                    trunks,
                    leaves,
                    rng,
                );
            }
        }
    } else {
        let new_dir = add_direction_variation(adjusted_dir, desc.randomness * 0.2, rng);
        recurse(
            end_pos,
            new_dir,
            level + 1,
            desc,
            length * desc.length_dropoff,
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
    let golden_angle = 2.4;
    let around_angle = if total_branches > 1 {
        (branch_index as f32) * (2.0 * PI) / (total_branches as f32) + (level as f32) * golden_angle
    } else {
        rng.gen::<f32>() * 2.0 * PI
    };
    let away_angle =
        rng.gen_range(desc.branch_angle_min..=desc.branch_angle_max) * (1.0 + desc.spread);

    let up = if parent_dir.y.abs() < 0.9 {
        Vec3::Y
    } else {
        Vec3::X
    };
    let right = parent_dir.cross(up).normalize_or_zero();
    let forward = parent_dir.cross(right).normalize_or_zero();

    let branch_dir = {
        let rotated_perp = right * around_angle.cos() + forward * around_angle.sin();
        let base_dir = parent_dir * away_angle.cos() + rotated_perp * away_angle.sin();
        base_dir.normalize_or_zero()
    };

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
