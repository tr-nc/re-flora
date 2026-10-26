# Garden expansion: aggregate acceptance

2026-10-26; validated runtime code `6c5fe738` on local `main`. No push or packaging. This is a local review build, not a fully green release certification.

## Build and automated tests

- `cargo fmt --check`, `cargo check`, `cargo build --release`, and `git diff --check`: passed.
- Full `cargo test`: **1759 passed / 9 failed / 14 ignored** in the main application; the separate library target passed all **4** tests. Exit 101 is recorded, not hidden.
- The failure-name set exactly matches the nine cases already reproduced on unmodified `9e2b7977` (the earlier baseline transcript contains three targeted runs, not one full rerun):
  - `app::gui_config::tests::sprinkler_sampling_upgrades_old_saves_and_round_trips_independently`
  - `app::gui_config_loader::tests::older_saves_gain_default_apple_size_without_rewriting_the_file`
  - `tracer::raster_tree::tests::cached_corners_do_not_override_overlapping_tree_ownership_order`
  - `tracer::raster_tree::tests::edited_surface_reuses_rest_bindings_without_reusing_faces_or_normals`
  - `tracer::raster_tree::tests::neighboring_blocks_share_posed_corners_and_hits_recover_rest_surface`
  - `tracer::raster_tree::tests::publication_equality_covers_geometry_normals_occupancy_and_skin`
  - `tracer::raster_tree::tests::rest_binding_reuse_requires_same_tree_identity_and_placement`
  - `tracer::raster_tree::tests::smooth_surface_omits_interior_cells_but_tracks_the_full_rest_volume`
  - `tracer::raster_tree::tests::translated_surface_cells_keep_their_extraction_owner_at_float_boundaries`
- Since the failing application target stops Cargo before integration targets, explicitly ran `cargo test --test clipboard_initialization --test display_authorization`: **1 passed / 4 ignored**, not five executed tests. The ignored desktop/clipboard checks remain unexecuted in this aggregate pass.
- Two additional insect-settings migration failures discovered during bee work were **fixed**, not attributed to the baseline. Commit `89d267e3` removes a stale hard-coded FPS label in favor of the declarative schema; all 52 butterfly-filtered tests then passed.

## Final-binary native regression matrix

All runs used `env -u WAYLAND_DISPLAY`, Release mode, `--hidden --mute`, and `RUST_LOG=info`. Final native stdout/run logs were inspected for ERROR, panic and Vulkan VUID messages: none found. The app's `--tail-latest-log 200` helper was also used. These are direct production-API fixtures, not subjective play-throughs.

| Current-binary check | Evidence |
|---|---|
| Grass native acceptance | Bare/flower exclusion, tall/short roots, cross-chunk interior gap, tight edge, final removal, moisture, restore, burial/uncover all true |
| Grass benchmark recheck | 12 one-root refreshes: 1024 roots median/max 0.069/0.084 ms; 65536 roots 0.187/0.241 ms. This later run has no fresh before measurement; retain the original matched before/after comparison, not a new claimed speedup. It times coverage refresh, not the complete edit/frame. |
| Recorded tree audio | Eight derivative assets pass preparation `--check`; six-stage five-canopy native mix review passes. Muted execution is **not listening approval**. |
| Economy old-save reload | 23 coins, 1 apple, 2 harvest receipts; flora/tree/stock invariants checked by the load fixture |
| Fence old-save reload | 3 posts, 203 sampled cells across two non-collinear rope links |
| Plow old-save reload | 110592 soil cells retained |
| Shovel carry-save reload/drop | Terrain 110592 + carried 0 = 110592 |
| Sapling old-save reload | Age 0.5, elapsed 180.001 simulated seconds, 18 leaves |
| Lantern old-save reload | 9847 Rock + 150 Emissive cells, open windows, physical roof ray |
| Bee-house reload | 2 tree/pole houses, 16 residents, 15 visits / 10 returns at review checkpoint |
| Firefly day transition | 24 seeded + 1 ecological birth; all 25 retired at day transition |
| Blue flower | Existing Forget-me-not fixture; one cluster source in the two-cluster budget, direct/DDGI enabled |
| Ordinary startup | Hidden muted 0.5-second smoke passes without review hooks |

Per-feature docs retain the original placement/edit/burial/fallen-fruit/partial-drop/growth-stage tests and screenshots. The old snapshots above were genuinely written before the bee metadata extension and loaded by the final runtime, providing backward-compatibility evidence. GUI defaults/generated files remained unchanged during aggregate runs.

## Acceptance boundary

- No new failures in the executed full-test set; this is **not proof of no bugs**. Nine baseline failures and 18 ignored tests across the main/integration targets remain explicit.
- Plow correctness edits measured 17.035/20.164 ms; a growth stage including preflight measured 18.905 ms. Neither is a 60-FPS acceptance result. Dense combined scenes, maximum insect population and sustained tool dragging are not accepted performance workloads.
- Flower and bee comparisons are presentation-inclusive whole frames, not isolated GPU timings or statistically significant claims.
- Subjective sound, moon/flower/insect appearance, sapling silhouette, collision feel, shovel gestures, economy balance and shop input still await player feedback. No visible game was automatically launched for user review.
- Firefly environmental GI is deliberately omitted; blue flowers use bounded shared cluster lights. No seasonal harvests, all-construction prices, bee breeding/honey or global soil collapse is claimed.
- Commercial wind-audio rights and the original sun texture purchase/rights evidence remain unresolved. Repository licenses do not override third-party rights.

The final self-contained report is `docs/reports/garden-expansion/index.html`; its bundled evidence is intended to remain readable without temporary log paths, this conversation or online services.
