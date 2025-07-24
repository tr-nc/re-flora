use crate::geom::RoundCone;
use glam::Vec3;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::f32::consts::PI;

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
    pub leaf_offset: u32,
    pub iterations: u32,
    pub segment_length_variation: f32,
    pub tree_height: f32,
    pub length_dropoff: f32,
    pub thickness_reduction: f32,
    pub seed: u64,
    pub enable_subdivision: bool,
    pub subdivision_threshold: f32,
    pub subdivision_count_min: u32,
    pub subdivision_count_max: u32,
    pub subdivision_randomness: f32,
    pub subdivision_randomness_progression: f32,
}

impl Default for TreeDesc {
    fn default() -> Self {
        TreeDesc {
            size: 30.0,
            trunk_thickness: 0.40,
            trunk_thickness_min: 1.05,
            iterations: 7,
            tree_height: 6.0,
            spread: 0.35,
            vertical_tendency: -0.45,
            segment_length_variation: 0.15,
            length_dropoff: 0.65,
            thickness_reduction: 0.60,
            branch_probability: 0.70,
            branch_count_min: 2,
            branch_count_max: 3,
            branch_angle_min: 24.0 * PI / 180.0,
            branch_angle_max: 48.0 * PI / 180.0,
            enable_subdivision: true,
            subdivision_threshold: 6.5,
            subdivision_count_min: 6,
            subdivision_count_max: 9,
            subdivision_randomness: 0.27,
            subdivision_randomness_progression: 1.5,
            randomness: 0.35,
            leaves_size_level: 5,
            leaf_offset: 1,
            seed: 122,
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
            .add(
                egui::Slider::new(&mut self.segment_length_variation, 0.0..=1.0)
                    .text("Segment Length Variation"),
            )
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

        // NEW: subdivision toggle
        changed |= ui
            .checkbox(&mut self.enable_subdivision, "Enable Subdivision")
            .changed();

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
        changed |= ui
            .add(
                egui::Slider::new(&mut self.subdivision_randomness_progression, 0.1..=3.0)
                    .text("Subdivision Randomness Progression"),
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
                egui::Slider::new(&mut self.leaf_offset, 0..=self.iterations.max(1))
                    .text("Leaf Offset (levels from end)"),
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
    leaf_positions: Vec<Vec3>,
}

#[derive(Debug)]
pub struct Tree {
    built_objects: BuiltObjects,
}

impl Tree {
    pub fn new(desc: TreeDesc) -> Self {
        let built_objects = Self::build(&desc);
        Tree { built_objects }
    }

    pub fn trunks(&self) -> &[RoundCone] {
        &self.built_objects.trunks
    }

    /// Obtain the leaf positions relative to the tree position.
    pub fn relative_leaf_positions(&self) -> &[Vec3] {
        &self.built_objects.leaf_positions
    }

    fn initial_segment_length(desc: &TreeDesc) -> f32 {
        let d = desc.length_dropoff;
        // if d is almost 1.0, fall back to the old average-per-level method
        if (1.0 - d).abs() < 1e-5 {
            return desc.tree_height * desc.size / (desc.iterations as f32).max(1.0);
        }
        let iterations_f = desc.iterations as f32;
        let numerator = desc.tree_height * desc.size * (1.0 - d);
        let denominator = 1.0 - d.powf(iterations_f);
        numerator / denominator
    }

    fn build(desc: &TreeDesc) -> BuiltObjects {
        let mut rng = StdRng::seed_from_u64(desc.seed);
        let mut initial_trunks = Vec::new();
        let mut leaf_positions = Vec::new();

        let base_length = Self::initial_segment_length(desc);
        let base_thickness = desc.trunk_thickness * desc.size;

        recurse(
            Vec3::ZERO,
            Vec3::Y,
            0,
            desc,
            base_length,
            base_thickness,
            &mut initial_trunks,
            &mut leaf_positions,
            &mut rng,
        );

        let mut trunks = Vec::new();
        for (cone, level) in &initial_trunks {
            // subdivision now respects the toggle
            let subdivided_cones = subdivide_trunk_segment(cone, desc, *level, &mut rng);
            trunks.extend(subdivided_cones);
        }

        BuiltObjects {
            trunks,
            leaf_positions,
        }
    }
}

/// Subdivides a single RoundCone into multiple, smaller, slightly perturbed cones.
/// Respects the `enable_subdivision` toggle.
fn subdivide_trunk_segment(cone: &RoundCone, desc: &TreeDesc, level: u32, rng: &mut StdRng) -> Vec<RoundCone> {
    // NEW: early-out if subdivision is disabled
    if !desc.enable_subdivision {
        return vec![cone.clone()];
    }

    let axis = cone.center_b() - cone.center_a();
    let length = axis.length();

    // Do not subdivide if the segment is too short or if subdivision is effectively disabled.
    if length <= desc.subdivision_threshold || desc.subdivision_count_max <= 1 {
        return vec![cone.clone()];
    }

    // Calculate iteration-based randomness progression, similar to thickness_reduction
    let level_factor = (level as f32) / (desc.iterations as f32).max(1.0);
    let iteration_randomness = if desc.subdivision_randomness_progression > 0.0 {
        desc.subdivision_randomness * desc.subdivision_randomness_progression.powf(level_factor)
    } else {
        desc.subdivision_randomness
    };

    let num_segments = if desc.subdivision_count_min >= desc.subdivision_count_max {
        desc.subdivision_count_min
    } else {
        rng.random_range(desc.subdivision_count_min..=desc.subdivision_count_max)
    };

    if num_segments <= 1 {
        return vec![cone.clone()];
    }

    let mut subdivided_trunks = Vec::with_capacity(num_segments as usize);
    let mut current_pos = cone.center_a();
    let segment_vec = axis / num_segments as f32;

    let up = if axis.normalize_or_zero().y.abs() < 0.9 {
        Vec3::Y
    } else {
        Vec3::X
    };
    let perp1 = axis.cross(up).normalize_or_zero();
    let perp2 = axis.cross(perp1).normalize_or_zero();

    for i in 1..=num_segments {
        let start_t = (i - 1) as f32 / num_segments as f32;
        let end_t = i as f32 / num_segments as f32;
        let segment_start_radius = cone.radius_a() * (1.0 - start_t) + cone.radius_b() * start_t;
        let segment_end_radius = cone.radius_a() * (1.0 - end_t) + cone.radius_b() * end_t;

        let mut next_pos;

        if i == num_segments {
            next_pos = cone.center_b();
        } else {
            next_pos = current_pos + segment_vec;
            if iteration_randomness > 0.0 {
                let random_angle = rng.random_range(0.0..2.0 * PI);
                let random_dir_perp = perp1 * random_angle.cos() + perp2 * random_angle.sin();
                let displacement_magnitude = segment_start_radius
                    * iteration_randomness
                    * rng.random_range(0.5..=1.0);
                next_pos += random_dir_perp * displacement_magnitude;
            }
        }

        subdivided_trunks.push(RoundCone::new(
            segment_start_radius.max(desc.trunk_thickness_min),
            current_pos,
            segment_end_radius.max(desc.trunk_thickness_min),
            next_pos,
        ));

        current_pos = next_pos;
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
    trunks: &mut Vec<(RoundCone, u32)>,
    leaf_positions: &mut Vec<Vec3>,
    rng: &mut StdRng,
) {
    if level >= desc.iterations {
        return;
    }

    // Leaf placement: place leaves at (iterations - leaf_offset) levels from the end
    if level == desc.iterations.saturating_sub(desc.leaf_offset) {
        leaf_positions.push(pos);
    }

    let length_variation_factor = {
        let random_factor = rng.random_range(-1.0..=1.0); // Always generate a random value
        1.0 + random_factor * desc.segment_length_variation // Scale by variation amount
    };

    let segment_length = length * length_variation_factor;
    let thickness_start = thickness;
    let natural_thickness_end = if desc.thickness_reduction > 0.0 {
        thickness * desc.thickness_reduction
    } else {
        thickness * 0.1_f32.powf((level + 1) as f32)
    };
    let thickness_end = natural_thickness_end;

    let level_factor = (level as f32) / (desc.iterations as f32);
    let vertical_influence = desc.vertical_tendency * level_factor;
    let adjusted_dir = (dir + Vec3::new(0.0, vertical_influence, 0.0)).normalize_or_zero();

    let end_pos = pos + adjusted_dir * segment_length;

    trunks.push((
        RoundCone::new(
            thickness_start.max(desc.trunk_thickness_min),
            pos,
            thickness_end.max(desc.trunk_thickness_min),
            end_pos,
        ),
        level,
    ));

    let should_branch = level < desc.iterations - 1
        && (level == 0 || rng.random::<f32>() < desc.branch_probability);

    if should_branch {
        let branch_count = if desc.branch_count_min == desc.branch_count_max {
            desc.branch_count_min
        } else {
            rng.random_range(desc.branch_count_min..=desc.branch_count_max)
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
                    leaf_positions,
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
            leaf_positions,
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
        rng.random::<f32>() * 2.0 * PI
    };
    let away_angle =
        rng.random_range(desc.branch_angle_min..=desc.branch_angle_max) * (1.0 + desc.spread);

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
    let rand_x = rng.random_range(-variation..=variation);
    let rand_y = rng.random_range(-variation..=variation);
    let rand_z = rng.random_range(-variation..=variation);
    (dir + Vec3::new(rand_x, rand_y, rand_z)).normalize_or_zero()
}
