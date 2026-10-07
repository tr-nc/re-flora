# Butterfly flower visiting — implementation and validation

## Step 1: authoritative flower contacts

- Flower contact points are ray-tested against the same transformed authored head triangles used by native/cached rendering, not roots or stem socket planes.
- A bounded (512 heads) GPU pass calls the production `flowerPlantPose`, including growth, spawn, height variation and vegetation response. Identity is `(world/mode epoch, authored response_id, head)`, independent of draw order.
- Readback uses existing acquired-frame fences, with no additional query submission/wait. Per-slot resources and metadata remain paired; older generations are discarded. World replacement invalidates the publication.
- This commit supplies infrastructure only; behavior and the saved A/B control follow in the next validated step. No saved config or model asset was changed.

Validation in `new-bird`:

- `cargo fmt --check`, `cargo check`, `cargo build --release`: passed.
- `cargo test every_authored_flower_has_a_real_upward_contact`: 1 passed, all authored flowers at 0.25/1/4 head scales.
- `cargo test abi_and_epochs`: 1 passed.
- Release hidden muted smoke (`--auto-exit 0.5`): passed.
- Native flower fixture: `RUST_LOG=re_flora=info,re_flora_vkn=warn RE_FLORA_FLOWER_CONTACT_REVIEW=1 RE_FLORA_FLOWER_MODEL_REVIEW=a cargo run --release -- --hidden --mute --auto-exit 3` (with GPU lock and Wayland unset): 6 valid real flower contacts, 288-byte publication, `frame_fence=true query_wait=false authored_surface=true`; shutdown `failures=0`, no ERROR/panic/VUID.
- An initial zero-size Vulkan copy caused by argument order was caught by the native log inspection and corrected before commit; the successful validation above is the corrected run.
- Generated tracked files: unchanged. Interim unused APIs will be consumed by the behavior step. These runs establish resource/contact correctness, not visual or performance acceptance.

## Step 2: visiting loop and saved runtime A/B

- Added `src/particles/butterfly_behavior.rs`: search → travel → approach → dwell/feed → takeoff, with recovery for deleted/stale hosts, sustained wind and stalled progress. Decisions are timed, not reselected every frame. Per-individual species weights are art direction; bounded recent-visit/failed-approach memory discourages immediate repeats.
- Goal steering layers onto existing 120 Hz integration, inertia, wingbeat phase and hard world/terrain guards. The original controller is retained when unchecked. Approach progressively uses precision steering rather than random bursts; wind correction is air-speed limited, not a teleport or unlimited counterforce.
- Rest uses a raised-wing authored animation frame and its measured lowest-vertex clearance. The actual rendered attachment follows the current GPU flower pose (native and cached mesh path), not delayed CPU readback; CPU pose still supports intent. Missing/currently invalid host slots are hidden instead of leaving an unanchored resting butterfly. Detachment invalidates held presentation immediately.
- Contact observation age/velocity use the simulation clock separately from render animation time; frozen capture timelines must not falsely expire real contacts.
- Debug Panel: **Flower-visiting behavior (A/B)**, search radius, dwell and individual preference under Flower visiting; visit wind limit grouped with Wind response. All are saved-field-bound controls using the existing Save/Search mechanism. Older settings files default to unchecked. The checked-in candidate intentionally enables only the new `[butterfly_flight.foraging]` section; all pre-existing saved values are unchanged.

Correctness validation:

- `cargo fmt --check`, `cargo check`, `cargo build --release`: passed (133 shaders). Tracked generated files unchanged.
- Butterfly-focused tests: 48 passed; separate integrated foraging test passed with the saved 0.25× speed and 4× vertical strength, both calm and moderate wind, integrated displacement bounds and immediate runtime A/B detachment. Renderer tests cover fixed rest wings, stable host mapping and invalid host suppression. Save/reload/UI tests cover the checkbox and settings.
- Initial complete `cargo test`: one new ABI expectation was corrected (flower part 80 → 112 bytes); **four unrelated failures** were reproduced individually on untouched baseline `f21ddff5` in a temporary detached worktree, subsequently removed. They are `saved_control_adapter_uses_type_specific_dimensions_and_one_source_spec`, `stone_search_checkbox_edit_save_and_reload_are_the_real_saved_fields`, `glass_unrefracted_raster_fallback_defaults_off`, and `older_saves_get_disabled_supersampling_and_saved_choices_survive_reload`. Existing Stone/Glass/supersampling saved-default mismatches were not changed.
- Complete suite excluding exactly those four verified baseline failures: **1430 app tests + 4 collision-bench tests passed, 5 ignored**. The unfiltered suite is not claimed green. Logs: `target/butterfly-foraging-step2-tests-with-baseline-exclusions.log` and `target/butterfly-foraging-baseline-tests.log`.
- Native Release fixtures (`RE_FLORA_BUTTERFLY_FORAGING_REVIEW=visits/wind/ab/lifetime/off`, hidden, muted): actual production planting/contact pipeline, two real emitter handles, in-memory saved-field inputs only, no settings/world writes. Fixture speed is 1× and dwell 1 s for bounded native cycle coverage, **not** an ordinary visual/perf claim about saved 0.25× / 4 s values.
  - Visits, 20 s: 10 landings, 10 departures, 8 transitions to a different host; complete-loop marker by ~5 s, no stalls/lost targets.
  - Corrected wind run, 22 s: 6-unit, 4 s gust through the production field; two visit interruptions, no invalid resource use. Strong residual transported wind prevents immediate new visits in this interval, so post-gust reacquisition is not claimed by this fixture. An initial fixture-only world/voxel origin conversion error produced no wind and was fixed before these results.
  - Runtime A/B, 20 s: true → false at 12 s → true at 14 s; visit counters stop while unchecked and resume afterwards. No handle/age reset or stale attachment.
  - Occupied host deletion, 20 s: one real brush edit removes the occupied flower; both visitors release it (`lost=2`) and subsequently visit different flowers. Draw-slot compaction does not transfer their identity to a replacement plant.
  - Original mode, 20 s: choices/landings/departures stay zero; no contact dispatch after diagnostic seeding.
- All successful native fixtures and the final 0.5 s Release smoke: shutdown `failures=0`, no ERROR/panic/VUID. Latest logs were inspected using this worktree's log helpers. Final post-clock-change 8 s visiting rerun also reaches complete-cycle/distinct-host PASS.
- These establish behavioral/resource safety. Subjective visual approval, dense-world/stress performance acceptance and any ecological nectar/pollination effects remain separate; no visible game was automatically launched.
