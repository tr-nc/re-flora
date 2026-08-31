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
