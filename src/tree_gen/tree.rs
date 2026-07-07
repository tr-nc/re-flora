use crate::branch_skeleton::{generate_branch_skeleton_with_rng, BranchingDesc};
use crate::geom::RoundCone;
use glam::Vec3;
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
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
    pub first_branch_level: u32,
    pub continue_main_axis: bool,
    pub leaves_size_level: u32,
    pub leaf_offset: u32,
    pub iterations: u32,
    pub segment_length_variation: f32,
    pub tree_height: f32,
    pub length_dropoff: f32,
    pub thickness_reduction: f32,
    pub seed: u64,
    pub enable_subdivision: bool,
    pub subdivision_count_min: u32,
    pub subdivision_count_max: u32,
    pub subdivision_randomness: f32,
    pub subdivision_randomness_progression: f32,
}

impl Default for TreeDesc {
    fn default() -> Self {
        TreeDesc {
            // Basic Properties
            size: 30.0,
            trunk_thickness: 0.40,
            trunk_thickness_min: 1.05,
            iterations: 7,

            // Tree Shape
            tree_height: 6.0,
            spread: 0.0,
            vertical_tendency: 0.47,
            segment_length_variation: 0.12,
            length_dropoff: 0.78,
            thickness_reduction: 0.61,

            // Branching Control
            branch_probability: 0.82,
            branch_count_min: 2,
            branch_count_max: 3,
            first_branch_level: 1,
            continue_main_axis: false,
            branch_angle_min: 24.0 * PI / 180.0,
            branch_angle_max: 48.0 * PI / 180.0,

            // Subdivision
            enable_subdivision: true,
            subdivision_count_min: 6,
            subdivision_count_max: 9,
            subdivision_randomness: 2.6,
            subdivision_randomness_progression: 3.0,

            // Variation
            randomness: 0.33,
            leaves_size_level: 5,
            leaf_offset: 1,

            // Seed
            seed: 122,
        }
    }
}

impl TreeDesc {
    #[allow(dead_code)]
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
        changed |= ui
            .add(
                egui::Slider::new(&mut self.first_branch_level, 0..=self.iterations.max(1))
                    .text("First Branch Level"),
            )
            .changed();
        changed |= ui
            .checkbox(&mut self.continue_main_axis, "Continue Main Axis")
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

        // nEW: subdivision toggle
        changed |= ui
            .checkbox(&mut self.enable_subdivision, "Enable Subdivision")
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
                egui::Slider::new(&mut self.subdivision_randomness, 0.0..=10.0)
                    .text("Subdivision Randomness"),
            )
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut self.subdivision_randomness_progression, 0.1..=3.0)
                    .text("Subdivision Randomness Progression"),
            )
            .changed();

        if changed && self.subdivision_count_min > self.subdivision_count_max {
            self.subdivision_count_max = self.subdivision_count_min;
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

    fn branching_desc(desc: &TreeDesc, initial_length: f32) -> BranchingDesc {
        BranchingDesc {
            seed: desc.seed,
            iterations: desc.iterations,
            initial_length,
            length_dropoff: desc.length_dropoff,
            spread: desc.spread,
            randomness: desc.randomness,
            vertical_tendency: desc.vertical_tendency,
            branch_angle_min: desc.branch_angle_min,
            branch_angle_max: desc.branch_angle_max,
            branch_probability: desc.branch_probability,
            branch_count_min: desc.branch_count_min,
            branch_count_max: desc.branch_count_max,
            segment_length_variation: desc.segment_length_variation,
            first_branch_level: desc.first_branch_level,
            continue_main_axis: desc.continue_main_axis,
        }
    }

    fn thickness_at_level(desc: &TreeDesc, base_thickness: f32, level: u32) -> f32 {
        let mut thickness = base_thickness;
        for current_level in 0..level {
            thickness = if desc.thickness_reduction > 0.0 {
                thickness * desc.thickness_reduction
            } else {
                thickness * 0.1_f32.powf((current_level + 1) as f32)
            };
        }
        thickness
    }

    fn build(desc: &TreeDesc) -> BuiltObjects {
        let mut rng = StdRng::seed_from_u64(desc.seed);
        let base_length = Self::initial_segment_length(desc);
        let base_thickness = desc.trunk_thickness * desc.size;
        let branching_desc = Self::branching_desc(desc, base_length);
        let skeleton = generate_branch_skeleton_with_rng(&branching_desc, &mut rng);

        let leaf_level = desc.iterations.saturating_sub(desc.leaf_offset);
        let leaf_positions = skeleton
            .nodes
            .iter()
            .filter(|node| node.level == leaf_level)
            .map(|node| node.pos)
            .collect();

        let mut trunks = Vec::new();
        for segment in &skeleton.segments {
            let thickness_start = Self::thickness_at_level(desc, base_thickness, segment.level);
            let thickness_end = Self::thickness_at_level(desc, base_thickness, segment.level + 1);
            let cone = RoundCone::new(
                thickness_start.max(desc.trunk_thickness_min),
                segment.start,
                thickness_end.max(desc.trunk_thickness_min),
                segment.end,
            );
            // subdivision now respects the toggle
            let subdivided_cones = subdivide_trunk_segment(&cone, desc, segment.level, &mut rng);
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
fn subdivide_trunk_segment(
    cone: &RoundCone,
    desc: &TreeDesc,
    level: u32,
    rng: &mut StdRng,
) -> Vec<RoundCone> {
    // early-out if subdivision is disabled
    if !desc.enable_subdivision {
        return vec![cone.clone()];
    }

    let axis = cone.center_b() - cone.center_a();

    // do not subdivide if the segment is too short or if subdivision is effectively disabled.
    if desc.subdivision_count_max <= 1 {
        return vec![cone.clone()];
    }

    // 0.0 at root, 1.0 at the deepest level
    let t = (level as f32) / (desc.iterations as f32).max(1.0);

    // Shape the curve with an exponent:
    //  - 1.0 => roughly linear
    //  - >1.0 => more weight toward the tip
    //  - <1.0 => more weight toward the base
    let curve_exp = desc.subdivision_randomness_progression.max(0.01);
    let mut weight = t.powf(curve_exp);

    // Ensure the base still has some randomness:
    // min_root_factor: 0.0 = allow root to be ~0, 0.2 = root is at least 20% of max.
    let min_root_factor = 0.2;
    let min_weight = min_root_factor;
    // Remap [0..1] into [min_weight..1]
    weight = min_weight + (1.0 - min_weight) * weight;

    // Final randomness scale for this level
    let iteration_randomness = desc.subdivision_randomness * weight;

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

    let root_radius = desc.trunk_thickness * desc.size;

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

                // 0 at root, → 1 as radius gets small
                let radius_ratio = (segment_start_radius / root_radius).clamp(0.0, 1.0);
                let tip_bias = 1.0 - radius_ratio; // 0 at base, 1 at tip-ish

                let displacement_magnitude = segment_start_radius
                    * iteration_randomness
                    * tip_bias
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
