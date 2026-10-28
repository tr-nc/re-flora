# New-tree planting preferences

Open **R → Tree Planting**, or search **new trees** in Debug:

| Control | Range | Default | Effect |
|---|---:|---:|---|
| Plant mature trees instantly (new trees) | Off / On | On | New trees start adult, without a growth clock; Off restores sapling planting |
| Tree scale (new trees) | 0.5–2 | 1 | Species-relative tree structural size for future plantings |
| Apple density (new trees) | 0–1 | 0.1 | Probability of an apple at each eligible fruit attachment |

The checked-in apple density is deliberately lower than the species profile's
0.45 probability (the original next-tree preference shipped at 0.25). Fresh-garden
opening trees now consume the same saved preferences and Apple authoring as Grow.
Trees already present in a running or saved world keep their captured values. Zero produces no apples, one enables all eligible candidates; the slider
is not an exact apple count. Actual counts also depend on the individual tree's
shape, seed and fruit-growth stage. Non-apple species never gain apples.

Scale adjusts the existing tree generator's size (height, branch layout and wood
radii), relative to each species' canonical planting size. It is not a uniform
transform of every rendered organ: leaf spray dimensions and individual apple
size retain their existing independent controls. Changing density does not change
wood/leaves. No shader/instance ABI changes are required.

## Ownership and persistence

- All three controls are declared in `config/gui.toml`, with one generated GUI owner.
  Debug **Save** remembers the planting preset; Search discovers it automatically.
  Older GUI files gain missing controls without being rewritten or changing their
  stored live-tree authoring. Existing saved slider values are preserved.
- The next-tree preview consumes the preset, retaining its seed while adjusting
  values. A successful planting receives that complete mature description once,
  through the same path in finite/shop and unlimited modes. Subsequent plantings
  continue using the preset until changed; controls do not reset after one tree.
- Fresh openings also use this shared recipe, explicitly selecting Apple Tree.
  Layout owns only the four positions and deterministic individual seeds; it does
  not substitute a preset, clamp tree scale or force adult age. Initial trees are
  supplied by the world, without debiting the player's sapling inventory.
- Slider edits refresh only the placement preview. They do **not** request an
  existing-tree rebuild, regenerate existing apples or reset a growth clock.
- Individual trees already save `mature_desc`; growth, save/load and fruit
  publication continue to consume that captured description, not today's sliders.
  No terrain-save schema migration is needed.
- Size starts from the species' canonical planting size, rather than multiplying
  the live editor's size. Loading a previously scaled tree into the editor cannot
  compound its scale on the next planting. The new planting size/density controls
  own these two values for Grow, independently of the existing live tree editor.

Use **Grow → Tree** after setting the preferences, or start a fresh garden using
saved preferences. **Plant mature trees instantly**
is checked by default: the shared finite/unlimited placement path validates the
full adult wood footprint, publishes age 1 and stores no growth clock. Unchecking
starts new trees at age 0.375 with their existing independent growth clock.
This preference is sampled only on placement; toggling does not age, freeze or
restart existing trees, including saplings already growing. Existing saves retain
their clocks. Apples still depend on the captured density and eligible growth stage.

These guarantees concern the two **new-tree** sliders. Existing live authoring,
global Tree Age and Apple Size controls retain their existing scope; they are not
safe substitutes for the new controls if you want to leave old trees alone.

## Fresh-opening settings correction (2026-10-29)

**Impact and cause.** Fresh startup previously called `TreeDesc::default()` in the
layout helper and published every tree at age 1. It bypassed the saved Grow
preferences introduced in `93a47953` (2026-10-26). The reported case reproduced in
native Release: saved density **0.1**, all four opening descriptions **0.45**.
Already-loaded worlds are not regenerated and were not part of that startup bug.

**Ownership.** `vegetation::planting_description` now builds both Grow previews
and opening descriptions from current authoring and planting controls. The layout
helper requires a supplied description and changes only its individual seed.
Both use `plant_tree_with_maturity` for validation, publication and growth-clock
ownership. The opening no longer silently shrinks a configured tree to fit the
old 46-voxel crown cap. An impossible placement is explicitly skipped with a
warning; only typed placement rejections are recoverable this way, while resource
and publication failures still propagate. At enlarged scales/custom authoring,
the default small-tree clearing margins are not guaranteed.

**Behavioral evidence.** `node scripts/validate-opening-tree-settings.mjs` runs
real hidden/muted Release processes under an isolated `RE_FLORA_ROOT`, copying
config rather than modifying the user's files. It verifies four published trees
and their actual saved descriptions/ages/clocks for scale/density/maturity:
`1 / 0.1 / adult`, `0.5 / 0 / adult`, `2 / 1 / adult`, and `1.5 / 0.2 / sapling`.
A fifth process reloads the saplings with settings `0.5 / 0 / adult`; saved tree
identities, descriptions, ages and clocks remain identical. Logs contain no
ERROR/VUID/panic or skipped-tree diagnostics and shutdown reports `failures=0`.
Evidence: `target/opening-tree-settings/run-EuNtB4/`; the initial red/green density
comparison is in `target/opening-tree-settings/{before,after}.log`.

Pure regressions compare the real opening recipe with Grow across seeds and
non-default authoring/scale/density, including a non-Apple editor selection.
57 vegetation tests pass. `cargo fmt --check`, `cargo check`, `cargo build --release`
and the standard hidden/muted smoke pass. Full Rust tests: **1,734 passed,
9 failed, 13 ignored**, plus 4 passing collision tests; the failures are the same
previously documented two config-default and seven raster-tree assertions.
No saved config or generated files changed. No shader or save-format change.

## Instant-maturity follow-up (2026-10-27)

- Old GUI files gain the default-on checkbox without a file rewrite; explicitly
  saved Off survives Save/reload. No global tree-age setting is repurposed.
- The native fixture now verifies the first free tree is immediately adult with
  no clock (no growth advance used to make it adult). It unchecks before planting
  the purchased tree and checks age 0.375, clock 0 and exactly one stock debit.
  Rechecking leaves its snapshot unchanged; it still matures normally afterward.
- A separate Release process reloads both trees and verifies the same owned
  descriptions and rendering. All accepted native logs have no ERROR/panic/VUID.
- 54 vegetation tests (including preview-age invalidation), the new preference migration test and exhaustive generic
  settings save/reload pass. Inspected `target/instant-tree-control.png`: one
  searchable checkbox in Tree Planting, checked by default.
- Check, formatting, Release build and ordinary hidden/muted smoke pass. No full
  repository test rerun, subjective approval or dense-scene performance claim.

## Size/density validation

- Real egui slider press/release tests change both values through search results,
  verify no live-tree edit signal or authoring mutation, and save/reload each value.
- An old GUI document without this section loads with defaults, unchanged stored
  tree size/density and unchanged file bytes. Saving non-default values round-trips.
- Pure tests cover independent existing/new mature descriptions across growth ages,
  species-relative size, repeated application after restore without compounding,
  non-apple species, zero density and deterministic thinning of stable fruit IDs.
- 53 vegetation tests, 9 GUI search tests and the exhaustive declarative-setting
  persistence test pass; dedicated planting/loader tests pass as well.
- Native seed fixture uses the real preview and placement paths: one free tree at
  scale 1 / density 1, then one purchased sapling at scale 1.5 / density 0.1. Later
  preset changes to scale 0.5 / density 0 do not change either existing tree.
  The second sapling still grows to its captured 1.5-sized mature form, and the
  first tree's canopy publication generation remains unchanged.
- The saved world is reloaded by another Release process. The two mature trees
  retain sizes/densities and **67 / 11 fruit specifications** respectively. They
  have different seeds/scales, so those counts are not an isolated density A/B.
  Fixed-candidate thinning is covered separately by the deterministic test.
- `cargo fmt --check`, `cargo check`, `cargo build --release`, ordinary hidden muted
  startup and per-worktree run-log inspection pass. No ERROR/panic/VUID diagnostics
  in accepted native runs. Generated GUI Rust fields come from `cargo check`;
  no shader-derived GPU structs change.
- Inspected `target/tree-planting-controls.png` and `target/tree-planting-settings.png`.
  These are temporary native review captures, not imported runtime assets. Manual
  artistic approval and dense-forest performance are not claimed; the full suite's
  previously recorded baseline failures were not reclassified as fixed.

Reproduction (from the project worktree):

```sh
env -u WAYLAND_DISPLAY RE_FLORA_TREE_PLANTING_REVIEW=seed RUST_LOG=info \
  cargo run --release -- --flat-terrain --hidden --mute \
  --terrain-save target/tree-planting-settings.rflterrain --auto-exit 1

env -u WAYLAND_DISPLAY RE_FLORA_TREE_PLANTING_REVIEW=load RUST_LOG=info \
  cargo run --release -- --hidden --mute \
  --terrain-load target/tree-planting-settings.rflterrain --auto-exit 0.5
```

The explicit `seed` fixture clears its initial world before creating a controlled
baseline. Ordinary gameplay never clears or reauthors the garden when these
sliders change. Do not use fixture environment variables for normal play.
