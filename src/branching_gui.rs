use crate::branch_skeleton::BranchingDesc;

#[derive(Debug, Clone)]
pub struct BranchingGuiSpec {
    pub iterations_min: u32,
    pub iterations_max: u32,
    pub initial_length_min: f32,
    pub initial_length_max: f32,
    pub length_dropoff_min: f32,
    pub length_dropoff_max: f32,
    pub segment_length_variation_min: f32,
    pub segment_length_variation_max: f32,
    pub spread_min: f32,
    pub spread_max: f32,
    pub randomness_min: f32,
    pub randomness_max: f32,
    pub vertical_tendency_min: f32,
    pub vertical_tendency_max: f32,
    pub branch_count_min_min: u32,
    pub branch_count_min_max: u32,
    pub branch_count_max_min: u32,
    pub branch_count_max_max: u32,
    pub branch_angle_min_degrees_max: f32,
    pub branch_angle_max_degrees_max: f32,
    pub seed_base: u64,
    pub seed_offset_max: Option<u64>,
    pub seed_label: &'static str,
}

impl Default for BranchingGuiSpec {
    fn default() -> Self {
        Self {
            iterations_min: 1,
            iterations_max: 12,
            initial_length_min: 0.1,
            initial_length_max: 120.0,
            length_dropoff_min: 0.1,
            length_dropoff_max: 1.0,
            segment_length_variation_min: 0.0,
            segment_length_variation_max: 1.0,
            spread_min: 0.0,
            spread_max: 2.0,
            randomness_min: 0.0,
            randomness_max: 1.0,
            vertical_tendency_min: -1.0,
            vertical_tendency_max: 1.0,
            branch_count_min_min: 0,
            branch_count_min_max: 8,
            branch_count_max_min: 0,
            branch_count_max_max: 8,
            branch_angle_min_degrees_max: 90.0,
            branch_angle_max_degrees_max: 120.0,
            seed_base: 0,
            seed_offset_max: None,
            seed_label: "Seed",
        }
    }
}

pub fn edit_branching_desc(
    ui: &mut egui::Ui,
    desc: &mut BranchingDesc,
    spec: &BranchingGuiSpec,
) -> bool {
    let mut changed = false;

    ui.heading("L-System / Branching");
    changed |= ui
        .add(
            egui::Slider::new(
                &mut desc.iterations,
                spec.iterations_min..=spec.iterations_max,
            )
            .text("Iterations"),
        )
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(
                &mut desc.initial_length,
                spec.initial_length_min..=spec.initial_length_max,
            )
            .text("Initial Length"),
        )
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(
                &mut desc.length_dropoff,
                spec.length_dropoff_min..=spec.length_dropoff_max,
            )
            .text("Length Dropoff"),
        )
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(
                &mut desc.segment_length_variation,
                spec.segment_length_variation_min..=spec.segment_length_variation_max,
            )
            .text("Segment Length Variation"),
        )
        .changed();

    ui.separator();
    ui.heading("Axis Shape");
    changed |= ui
        .add(egui::Slider::new(&mut desc.spread, spec.spread_min..=spec.spread_max).text("Spread"))
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(
                &mut desc.randomness,
                spec.randomness_min..=spec.randomness_max,
            )
            .text("Randomness"),
        )
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(
                &mut desc.vertical_tendency,
                spec.vertical_tendency_min..=spec.vertical_tendency_max,
            )
            .text("Vertical Tendency"),
        )
        .changed();

    ui.separator();
    ui.heading("Branch Window");
    changed |= ui
        .add(
            egui::Slider::new(&mut desc.branch_start_fraction, 0.0..=1.0)
                .text("Branch Start Fraction"),
        )
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(&mut desc.branch_end_fraction, 0.0..=1.0).text("Branch End Fraction"),
        )
        .changed();
    changed |= ui
        .checkbox(&mut desc.continue_main_axis, "Continue Main Axis")
        .changed();

    ui.separator();
    ui.heading("Lateral Branches");
    changed |= ui
        .add(egui::Slider::new(&mut desc.branch_probability, 0.0..=1.0).text("Branch Probability"))
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(
                &mut desc.branch_count_min,
                spec.branch_count_min_min..=spec.branch_count_min_max,
            )
            .text("Min Branches"),
        )
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(
                &mut desc.branch_count_max,
                spec.branch_count_max_min..=spec.branch_count_max_max,
            )
            .text("Max Branches"),
        )
        .changed();

    let mut angle_min_deg = desc.branch_angle_min.to_degrees();
    let mut angle_max_deg = desc.branch_angle_max.to_degrees();
    changed |= ui
        .add(
            egui::Slider::new(&mut angle_min_deg, 0.0..=spec.branch_angle_min_degrees_max)
                .text("Min Branch Angle (deg)"),
        )
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(&mut angle_max_deg, 0.0..=spec.branch_angle_max_degrees_max)
                .text("Max Branch Angle (deg)"),
        )
        .changed();

    ui.separator();
    ui.heading("Seed");
    let mut seed_offset = desc.seed.saturating_sub(spec.seed_base);
    let seed_changed = ui
        .add(
            egui::DragValue::new(&mut seed_offset)
                .speed(1.0)
                .prefix(format!("{}: ", spec.seed_label)),
        )
        .changed();
    if seed_changed {
        if let Some(max) = spec.seed_offset_max {
            seed_offset = seed_offset.min(max);
        }
        desc.seed = spec.seed_base.wrapping_add(seed_offset);
        changed = true;
    }

    if changed {
        desc.branch_angle_min = angle_min_deg.to_radians();
        desc.branch_angle_max = angle_max_deg.to_radians();
        desc.normalize();
    }

    changed
}
