use crate::branching_gui::{edit_branching_desc, BranchingGuiSpec};
use crate::tree_gen::TreeDesc;

pub(super) fn edit_tree_desc(
    ui: &mut egui::Ui,
    tree: &mut TreeDesc,
    render_leaves: Option<&mut bool>,
) -> bool {
    let mut changed = false;

    ui.heading("Tree Renderer");
    changed |= ui
        .add(
            egui::Slider::new(&mut tree.size, 0.1..=50.0)
                .text("Tree Size")
                .logarithmic(true),
        )
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut tree.trunk_thickness, 0.01..=5.0).text("Trunk Thickness"))
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(&mut tree.thickness_reduction, 0.0..=1.0).text("Thickness Reduction"),
        )
        .changed();

    ui.separator();
    changed |= edit_branching_desc(ui, &mut tree.branching, &BranchingGuiSpec::default());

    ui.separator();
    ui.heading("Subdivision");
    changed |= ui
        .checkbox(&mut tree.enable_subdivision, "Enable Subdivision")
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut tree.subdivision_count_min, 1..=10).text("Min Subdivisions"))
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut tree.subdivision_count_max, 1..=10).text("Max Subdivisions"))
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(&mut tree.subdivision_randomness, 0.0..=10.0)
                .text("Subdivision Randomness"),
        )
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(&mut tree.subdivision_randomness_progression, 0.1..=3.0)
                .text("Subdivision Randomness Progression"),
        )
        .changed();

    if changed && tree.subdivision_count_min > tree.subdivision_count_max {
        tree.subdivision_count_max = tree.subdivision_count_min;
    }

    ui.separator();
    ui.heading("Leaves");
    if let Some(render_leaves) = render_leaves {
        ui.checkbox(render_leaves, "Render Leaves");
    }
    changed |= ui
        .add(
            egui::Slider::new(&mut tree.leaves_size_level, 0..=8)
                .text("Leaves Size Level (2^level)"),
        )
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(&mut tree.leaf_offset, 0..=tree.branching.iterations.max(1))
                .text("Leaf Offset (levels from end)"),
        )
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut tree.leaf_density, 0.005..=0.2).text("Leaf Density"))
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(&mut tree.leaf_spray_width_ratio, 0.1..=1.5).text("Leaf Spray Width"),
        )
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(&mut tree.leaf_spray_thickness_ratio, 0.05..=1.0)
                .text("Leaf Spray Thickness"),
        )
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(&mut tree.leaf_spray_tip_offset_ratio, -0.5..=1.0)
                .text("Leaf Spray Tip Offset"),
        )
        .changed();

    ui.separator();
    ui.heading("Fruit");
    changed |= ui
        .add(
            egui::Slider::new(&mut tree.fruit_spawn_probability, 0.0..=1.0)
                .text("Fruit Spawn Probability"),
        )
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(&mut tree.fruit_side_offset_voxels, 0.0..=16.0)
                .text("Fruit Side Offset (voxels)"),
        )
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(&mut tree.fruit_side_offset_variance_voxels, 0.0..=16.0)
                .text("Fruit Side Offset Variation"),
        )
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(&mut tree.fruit_down_offset_voxels, 0.0..=24.0)
                .text("Fruit Down Offset From Branch"),
        )
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(&mut tree.fruit_down_offset_variance_voxels, 0.0..=16.0)
                .text("Fruit Down Offset Variation (±)"),
        )
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(&mut tree.fruit_swing_length_voxels, 0.0..=12.0)
                .text("Fruit Pivot Offset From Center"),
        )
        .on_hover_text(
            "Distance from the fruit center to its fixed attachment pivot; 2 voxels places the \
             default apple pivot at its top center.",
        )
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(&mut tree.fruit_swing_max_angle_degrees, 0.0..=85.0)
                .text("Fruit Max Swing Angle (deg)"),
        )
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut tree.fruit_swing_speed, 0.0..=8.0).text("Fruit Swing Speed"))
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(&mut tree.fruit_swing_speed_variation, 0.0..=1.0)
                .text("Fruit Swing Speed Variation"),
        )
        .changed();
    changed |= ui
        .add(
            egui::Slider::new(&mut tree.fruit_swing_min_response, 0.0..=1.0)
                .text("Fruit Minimum Wind Response"),
        )
        .changed();

    changed
}

#[cfg(test)]
mod tests {
    use super::edit_tree_desc;
    use crate::tree_gen::TreeDesc;

    #[test]
    fn untouched_tree_editor_preserves_the_description_and_reports_no_change() {
        let before = TreeDesc::default();
        let mut edited = before.clone();
        let mut render_leaves = true;
        let mut changed = true;

        egui::__run_test_ui(|ui| {
            changed = edit_tree_desc(ui, &mut edited, Some(&mut render_leaves));
        });

        assert!(!changed);
        assert_eq!(edited, before);
        assert!(render_leaves);
    }
}
