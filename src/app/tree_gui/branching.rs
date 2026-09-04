use crate::branch_skeleton::BranchingDesc;

pub(super) fn edit_branching_desc(ui: &mut egui::Ui, desc: &mut BranchingDesc) -> bool {
    let mut changed = false;

    ui.heading("L-System / Branching");
    changed |= ui
        .add(egui::Slider::new(&mut desc.iterations, 1..=12).text("Iterations"))
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut desc.initial_length, 0.1..=120.0).text("Initial Length"))
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut desc.length_dropoff, 0.1..=1.0).text("Length Dropoff"))
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(&mut desc.segment_length_variation, 0.0..=1.0)
                .text("Segment Length Variation"),
        )
        .changed();

    ui.separator();
    ui.heading("Axis Shape");
    changed |= ui
        .add(egui::Slider::new(&mut desc.spread, 0.0..=2.0).text("Spread"))
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut desc.randomness, 0.0..=1.0).text("Randomness"))
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(&mut desc.vertical_tendency, -1.0..=1.0)
                .text("Axis Vertical Tendency"),
        )
        .on_hover_text(
            "Progressively pulls each parent axis toward global up (positive) or down (negative). \
             Unlike branch angle, this changes the axis that later branches grow from.",
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
        .add(egui::Slider::new(&mut desc.branch_count_min, 0..=8).text("Min Branches"))
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut desc.branch_count_max, 0..=8).text("Max Branches"))
        .changed();

    let (mut angle_mean_deg, mut angle_variation_deg) = branch_angle_mean_variation_degrees(desc);
    let angle_mean_changed = ui
        .add(egui::Slider::new(&mut angle_mean_deg, 0.0..=120.0).text("Branch Angle Mean (deg)"))
        .on_hover_text(
            "Mean local angle between a lateral branch and its current parent axis, before Spread \
             is applied.",
        )
        .changed();
    let max_variation = angle_mean_deg.min(120.0 - angle_mean_deg);
    angle_variation_deg = angle_variation_deg.min(max_variation);
    let angle_variation_changed = ui
        .add(
            egui::Slider::new(&mut angle_variation_deg, 0.0..=max_variation)
                .text("Branch Angle Variation (± deg)"),
        )
        .on_hover_text(
            "Each lateral branch samples uniformly from mean minus this value through mean plus \
             this value.",
        )
        .changed();
    changed |= angle_mean_changed || angle_variation_changed;

    ui.separator();
    ui.heading("Seed");
    changed |= ui
        .add(
            egui::DragValue::new(&mut desc.seed)
                .speed(1.0)
                .prefix("Seed: "),
        )
        .changed();

    if changed {
        set_branch_angle_mean_variation_degrees(desc, angle_mean_deg, angle_variation_deg);
        desc.normalize();
    }

    changed
}

fn branch_angle_mean_variation_degrees(desc: &BranchingDesc) -> (f32, f32) {
    let min = desc.branch_angle_min.to_degrees();
    let max = desc.branch_angle_max.to_degrees();
    ((min + max) * 0.5, (max - min).abs() * 0.5)
}

fn set_branch_angle_mean_variation_degrees(
    desc: &mut BranchingDesc,
    mean_degrees: f32,
    variation_degrees: f32,
) {
    let variation_degrees = variation_degrees.max(0.0);
    desc.branch_angle_min = (mean_degrees - variation_degrees).to_radians();
    desc.branch_angle_max = (mean_degrees + variation_degrees).to_radians();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_angle_mean_variation_round_trips() {
        let mut desc = crate::tree_gen::TreeDesc::default().branching;
        desc.branch_angle_min = 24.0_f32.to_radians();
        desc.branch_angle_max = 48.0_f32.to_radians();

        let (mean, variation) = branch_angle_mean_variation_degrees(&desc);
        assert!((mean - 36.0).abs() < 0.001);
        assert!((variation - 12.0).abs() < 0.001);

        set_branch_angle_mean_variation_degrees(&mut desc, 50.0, 10.0);
        assert!((desc.branch_angle_min.to_degrees() - 40.0).abs() < 0.001);
        assert!((desc.branch_angle_max.to_degrees() - 60.0).abs() < 0.001);
    }
}
