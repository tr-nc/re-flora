# Grow planting catalog

## Player behavior

- **1 / Grow** is the unified planting tool. Hand has been removed; **2** is unassigned. Clicking the active tool again clears selection, with no toolbar slot highlighted. Other numeric bindings are unchanged.
- The right-hand panel has **Type: Flora / Tree**, followed by **Variety**. Its frame stays at half the viewport height for both catalogs; the variety list fills the remaining space and scrolls internally.
- Flora retains exactly `PLAYER_FLORA_PAINT_SELECTIONS`: Grass Mix and the existing model-flower catalog. Left hold paints; right hold removes flora.
- Tree offers **Apple Tree**, **Pine** and **Weeping Willow**. Apple has a low, spreading scaffold crown and real apples; Pine has irregular branch stations and upswept sprays; Willow has pendant leafy shoots. Left click plants one tree; holding does not repeat. The geometry preview and next-placement random seed remain in use. Legacy Conifer remains loadable with both original modes but is not offered for new planting. Test/Garden Tree has been retired; legacy or missing-species saves convert to current apples in place, including replacement of their baked wood. See `docs/evidence/apple-opening-and-test-tree-retirement.md`.
- New Apple Trees use a compact mature garden scale (about 1.5–1.8 player-body heights, with seed variation), shared with the fresh opening. Selecting a different species uses that species' native scale; Pine and Willow do not inherit the Apple's small size. Matching-species tuning and trees already stored in garden saves retain their authored dimensions. See [scale evidence](evidence/apple-tree-player-scale.md).
- The standalone Tree toolbar item and Z shortcut are removed. X / Sprinkler remains separate.
- Type remembers its own variety. Tab cycles only within the active Type. Changing Type or variety cancels the previous continuous stroke; it does not resume from a still-pressed pointer button.
- Selection is session state, not a new Debug setting or saved configuration field.

## Extension boundaries

`src/app/core/plant_catalog.rs` owns the typed catalog and selection:

- `PlantType`: category enumeration and category catalog.
- `PlantVariety`: typed Flora or Tree identity; labels and category identity are independent of toolbar indices.
- `TreeSpecies` (re-exported from `src/tree_gen/species.rs`): persistent tree identity, `PLANTABLE`, label and mature description resolution. Test-only `ALL` includes the supported legacy Conifer, not the retired tree marker. Missing species and `garden_tree` decode as retired metadata for conversion to Apple; new defaults and session selection are Apple Tree.
- `PlantingSelection`: active Type plus remembered flora index and tree species. The flora index remains available to existing scene/benchmark authoring callers inside this shared structure.

To add a tree variety, add its `TreeSpecies` variant in `src/tree_gen/species.rs`, register it in `PLANTABLE` and test `ALL`, and implement its label, mature profile, architecture and fruit policy. Both preview refresh and next placement use this resolver before applying age and seed. Planted trees persist species, individual seed, architecture mode and mature description through existing garden snapshots. New species use the shared bounded axis compiler; Conifer's legacy adapters are retained. This does not guarantee that arbitrary future topology edits preserve branch indices. `docs/conifer_axis_candidate.md` describes the retained legacy Conifer A/B, not the new Pine.

`player_tools.rs` owns pointer routing: Grow/Flora maps to existing continuous flora actions; Grow/Tree maps to discrete `PlantTree`. Grow stays the selected toolbar tool in both cases. Sprinkler remains a placeable. Type/variety changes cancel stale continuous holds.

`ui_style.rs` draws the same variety list component for both categories and returns typed Type/variety selections. `mod.rs` builds entries from the catalog and coordinates events/preview visibility. `input.rs` handles selection side effects and world placement; tree rendering and flora generation backends are unchanged. Existing `planting.rs` substrate/anchor logic is unchanged.

## Validation

- `cargo fmt --check` and `cargo check`: passed.
- Focused catalog, runtime and egui tests passed. Added guards for category-local cycling, remembered selection, unchanged flora catalog, discrete tree actions, hold cancellation, removed tree slot, actual Type/Tree-variety clicks, and disabled UI interaction. Existing short-window test still selects every flower.
- Full `cargo test`: **1394 passed, 5 failed, 5 ignored** in the app suite; the collision bench's 4 tests passed. An initial 200-second attempt timed out on existing climbing tests; the completed rerun took 211.50 seconds.
- All five failures reproduced individually in a detached worktree at pre-change commit `428cb0bd`, using the same test filters. That temporary worktree was removed afterward. Existing failures:
  - `app::core::stone_preview::tests::saved_control_adapter_uses_type_specific_dimensions_and_one_source_spec`
  - `app::gui_config::search_tests::stone_search_checkbox_edit_save_and_reload_are_the_real_saved_fields`
  - `app::gui_config::tests::glass_unrefracted_raster_fallback_defaults_off`
  - `app::gui_config_loader::tests::main_saves_gain_lifecycle_controls_without_changing_authored_display`
  - `app::gui_config_loader::tests::older_saves_get_disabled_supersampling_and_saved_choices_survive_reload`
- Worktree `cargo build --release`: passed; Release binary is ready.
- `cargo run --release -- --hidden --mute --auto-exit 0.5`: passed. Inspected the same worktree's `--tail-latest-log 200`: no ERROR/panic/VUID; normal successful exit. Log: `target/re-flora-logs/re-flora-20261007-002206.608-1861085.log`. One fruit-physics hitch warning reported 62.202 ms; this is startup smoke, not performance acceptance.
- No generated files or `config/gui.toml` changes. No visible game was automatically launched; new menu appearance and actual in-world planting were not manually visually reviewed.

### Fixed panel height follow-up

- Set the full panel to 50% of viewport height, subtracting the frame's actual total margins for the inner content. Disable vertical auto-shrink on the remaining list area.
- `cargo fmt --check`, `cargo check`, all 3 `grow_panel` tests, and `cargo build --release` passed. The height test checks Flora → Tree → Flora at 480, 720 and 1080 viewport heights; the existing short-window catalog click test remains passing.
- Hidden muted Release smoke passed; inspected `--tail-latest-log 200`, no ERROR/panic/VUID and normal successful exit. Log: `target/re-flora-logs/re-flora-20261007-003957.262-1868632.log`; fruit startup hitch warning 24.816 ms. Full suite was not rerun for this follow-up; the baseline failures above remain known.
