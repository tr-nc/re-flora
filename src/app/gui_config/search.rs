//! Presentation-only search, shared by declarative settings and custom panel sections.
//! Queries are session UI state, never part of the saved game/settings document.

#[derive(Clone, Default, Debug)]
pub(crate) struct SearchFilter {
    terms: Vec<String>,
}

fn normalize(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect()
}

impl SearchFilter {
    pub(crate) fn new(query: &str) -> Self {
        Self {
            terms: normalize(query)
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
        }
    }

    pub(crate) fn is_active(&self) -> bool {
        !self.terms.is_empty()
    }

    /// All query words must occur somewhere in the supplied visible labels/path/keywords.
    pub(crate) fn matches<'a>(&self, fields: impl IntoIterator<Item = &'a str>) -> bool {
        if !self.is_active() {
            return true;
        }
        let fields: Vec<_> = fields.into_iter().map(normalize).collect();
        self.terms
            .iter()
            .all(|term| fields.iter().any(|field| field.contains(term)))
    }

    /// Legacy/custom tool groups can opt in without changing their state ownership.
    /// Search shows matching groups directly; normal collapse state is not touched.
    pub(crate) fn section<R>(
        &self,
        ui: &mut egui::Ui,
        title: &str,
        keywords: &[&str],
        body: impl FnOnce(&mut egui::Ui) -> R,
    ) -> Option<R> {
        if !self.is_active() {
            return ui.collapsing(title, body).body_returned;
        }
        if !self.matches(std::iter::once(title).chain(keywords.iter().copied())) {
            return None;
        }
        Some(
            ui.push_id(("debug_search_group", title), |ui| {
                crate::app::ui_text::section(ui, title);
                body(ui)
            })
            .inner,
        )
    }
}

#[derive(Default)]
pub(super) struct SearchState {
    pub(super) query: String,
}

impl SearchState {
    pub(super) fn toolbar(&mut self, ui: &mut egui::Ui) -> (SearchFilter, bool) {
        let previous = self.query.clone();
        ui.horizontal(|ui| {
            crate::app::ui_text::label(ui, "Search");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let id = ui.make_persistent_id("debug_panel_search");
                let was_focused = ui.memory(|memory| memory.has_focus(id) || memory.had_focus_last_frame(id));
                // egui clears focus at pass start on Escape; retain the previous owner's intent.
                let clear_requested = was_focused && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
                let focus_requested = ui.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::F));
                if clear_requested { self.query.clear(); }
                // Lay out Clear first so the input uses the actual remaining width,
                // including theme padding and spacing, without expanding its parent.
                let clear_clicked = ui.add_enabled(!self.query.is_empty(), egui::Button::new("Clear")).clicked();
                if clear_clicked { self.query.clear(); }
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .id(id)
                        .hint_text("Names or groups... (Ctrl+F)")
                        .desired_width(ui.available_width()),
                ).on_hover_text("Not saved: this is a session-only panel filter. All words must match; case and punctuation are ignored. Ctrl+F focuses search; Escape clears it. Legacy custom tools match by group.");
                if focus_requested || clear_requested || clear_clicked { response.request_focus(); }
            });
        });
        (SearchFilter::new(&self.query), self.query != previous)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_match_across_fields_and_ignore_case_punctuation_and_whitespace() {
        let query = SearchFilter::new("  TRUE_voxel  RESTAURANT ");
        assert!(query.matches(["Restaurant scene: true voxels", "rooftop_voxel_scene"]));
        assert!(!query.matches(["Restaurant", "Original models"]));
        assert!(SearchFilter::new("voxel scene").matches(["rooftop_voxel_scene"]));
        assert!(SearchFilter::new("树木").matches(["树木颜色"]));
        assert!(!SearchFilter::new("not here").matches(["not", "absent"]));
        assert!(!SearchFilter::new("  _ / ").is_active());
    }

    #[test]
    fn search_uses_visible_words_without_hidden_synonyms() {
        assert!(SearchFilter::new("tree wind").matches(["Tree", "Wind response"]));
        assert!(!SearchFilter::new("树 风").matches(["Tree", "Wind response"]));
        assert!(!SearchFilter::new("colour").matches(["Grass color"]));
        assert!(!SearchFilter::new("worldtick").matches(["World tick"]));
    }

    #[test]
    fn toolbar_focus_typing_and_escape_report_query_changes() {
        let context = egui::Context::default();
        let mut state = SearchState::default();
        let mut draw = |events, modifiers| {
            let mut result = (SearchFilter::default(), false);
            let _ = context.run_ui(
                egui::RawInput {
                    events,
                    modifiers,
                    ..Default::default()
                },
                |ui| {
                    result = state.toolbar(ui);
                },
            );
            result
        };
        draw(vec![], egui::Modifiers::NONE);
        draw(
            vec![egui::Event::Key {
                key: egui::Key::F,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }],
            egui::Modifiers::COMMAND,
        );
        let (filter, changed) = draw(
            vec![egui::Event::Text("glass".to_owned())],
            egui::Modifiers::NONE,
        );
        assert!(changed);
        assert!(filter.matches(["Glass"]));
        assert!(!filter.matches(["Dirt"]));
        let (filter, changed) = draw(
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            egui::Modifiers::NONE,
        );
        assert!(changed);
        assert!(!filter.is_active());
    }

    #[test]
    fn custom_sections_hide_unrelated_content_and_open_matches_without_changing_collapse_state() {
        let context = egui::Context::default();
        let filter = SearchFilter::new("probe spacing");
        let mut drew = false;
        let _ = context.run_ui(Default::default(), |ui| {
            assert!(filter
                .section(ui, "Environment Probes", &["Spacing"], |_| {
                    drew = true;
                })
                .is_some());
            assert!(filter
                .section(ui, "Camera Snapshots", &[], |_| panic!(
                    "unmatched group rendered"
                ))
                .is_none());
        });
        assert!(drew);
        let _ = context.run_ui(Default::default(), |ui| {
            assert!(
                SearchFilter::default()
                    .section(ui, "Environment Probes", &[], |_| {})
                    .is_none(),
                "search changed the normally collapsed group"
            );
        });
    }
}
