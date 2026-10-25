# Conifer axis-model candidate

> Historical candidate: new planting now offers **Apple Tree / Pine / Weeping Willow**.
> Legacy Conifer saves and their A/B generator remain supported; the old Conifer is no longer a
> Grow catalog item. See [current species validation](evidence/garden-tree-species.md).
> The Garden Tree preservation statements below describe that historical candidate. Test/Garden
> Tree was subsequently retired and now migrates to Apple; see the
> [retirement evidence](evidence/apple-opening-and-test-tree-retirement.md).

## Try-out

1. Select **1 / Grow → Type: Tree → Conifer** and left-click plantable terrain.
2. **R / Debug → Flora → Tree Architecture → Conifer: axis model (A/B)** switches at runtime:
   - unchecked: original recursive generator using the Conifer's comparison preset;
   - checked (default): continuous trunk with layered, tapered crown.
3. Switching replaces planted Conifer geometry and refreshes the preview while preserving IDs, seeds and mature authoring. Garden Trees are not changed by this A/B. The checkbox is saved through `SavedControls`, not App-only state.

The candidate is a pine/fir-style architectural silhouette, not a botanical simulation of a particular species. It still uses existing voxel leaves, bark rendering, canopy audio and lighting. Needle geometry, cones, dedicated foliage colors, competition for light and species-specific lifecycle are not included. Conifers do not spawn apples. Visual approval and performance acceptance remain separate follow-ups; no visible game was launched automatically.

## Model

- `src/tree_gen/species.rs` owns the serializable species enum and mature preset resolver. The Grow catalog re-exports that identity instead of defining a second tree enum.
- `TreeDesc` retains `species`, `axis_model`, authored parameters and the existing seed. Existing saves without the new fields default to Garden Tree and its original generator.
- `src/tree_gen/axis.rs` compiles a complete trunk and lateral axes into the existing parent-before-child `BranchSegment` interface. A Conifer has one continuous trunk, 9 whorls of 4 main branches, and feathered twig axes. The current profile produces 552 segments; the compiler has an explicit 4096-segment cap.
- Axis metadata separately retains axis identity, branch order, local normalized arc positions, explicit endpoint radii and causal age reveal times. Same-axis continuation does not increase branch order. Branch sites are parent-segment endpoints, as required by existing pose and skin bindings.
- Trunk height is 170 voxels at size 30. Crown width decreases with attachment height independently of trunk segment length. Seed varies lean, phase, limb length and twig length while retaining architecture. Radius taper is continuous along an axis; new side axes start thinner than their parent.
- Age scales the same mature individual and reveals connected segments: a child cannot appear before its parent. Foliage is attached to visible twig endpoints and the visible leader tip, rather than one global recursive depth.
- Axis wind joints use authored radius-relative compliance/frequency; the existing Garden Tree wind path is unchanged. This does not claim deformation invariance under arbitrary resampling of an axis.
- The saved-tree reconstruction budget uses the bounded axis compiler for axis Conifers, not the recursive generator's exponential estimate. Existing finite-value and description validation remains in place.

## Preview age correction

Previously the next-placement description was already interpolated by global age, then `TreePlacementService` applied age again when planting. The next-placement description is now always mature; the preview and world publication each apply age once. Species selection and A/B preview refresh preserve the next individual seed; successful planting advances the next seed as before.

## Validation

Implementation step 1 committed as `83f44547` after `cargo check`, 30 tree-generation/pose/skin tests, saved custom-field round-trip, snapshot tests, Release build and hidden muted startup.

Integrated validation:

- `cargo fmt --check`, `cargo check`: passed.
- 30 `tree_gen` tests, 2 catalog tests, 3 Grow GUI tests, 3 snapshot tests, 35 vegetation publication tests, saved custom-field round-trip and tree GUI config round-trip: passed.
- Added a real egui checkbox click/save/reload test. It verifies the saved A/B field changes without modifying Garden Tree authoring.
- Full `cargo test`: app suite **1401 passed, 5 failed, 5 ignored**; collision bench **4 passed**. The five failures are the same previously reproduced pre-change baseline failures documented in `docs/grow_planting_catalog.md`; no new failures. Full-suite log: `/tmp/conifer-all-tests.log`.
- Release build passed. Both normal hidden muted startup and the extended `--raster-tree-smoke` acceptance passed.
- Extended smoke plants a half-age Conifer next to a Garden Tree through the actual publication path, checks age is applied once, toggles both A/B modes on the planted tree, verifies original Garden authoring/Conifer identity and seed, rebuilds mature age, restores a mixed-species snapshot, and validates GPU surface, poses and lighting with wind enabled.
- Verbose hidden acceptance: `RUST_LOG=re_flora=info cargo run --release -- --hidden --mute --raster-tree-smoke --auto-exit 120`. Log: `target/re-flora-logs/re-flora-20261007-013437.772-1914295.log`. Inspect per-worktree logs using `--latest-log` / `--tail-latest-log`.
- Concrete smoke evidence: `[TREE][CONIFER_SMOKE] passed mixed_species=true persistent_seed=true age_once=true AB_planted_geometry=true snapshot_restore=true axis_segments=552 leaf_instances=7699`; `[TREE][RASTER_SMOKE] passed ... conifer=true`; shutdown `failures=0`. No ERROR/panic/VUID observed. There were fruit-physics hitch warnings during startup/publication; these runs are correctness checks, not performance acceptance.
- No shader, generated-file or `config/gui.toml` changes. No manual in-world visual review or release performance benchmark is claimed.
