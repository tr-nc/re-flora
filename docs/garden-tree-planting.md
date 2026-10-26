# Next-tree size and apple density

Open **R → Tree Planting**, or search **new trees** in Debug:

| Control | Range | Default | Effect |
|---|---:|---:|---|
| Tree scale (new trees) | 0.5–2 | 1 | Species-relative tree structural size for future plantings |
| Apple density (new trees) | 0–1 | 0.25 | Probability of an apple at each eligible fruit attachment |

The new apple density default is deliberately lower than the old 0.45 planting
probability. Existing trees, including the initial garden trees, keep their old
values. Zero produces no apples, one enables all eligible candidates; the slider
is not an exact apple count. Actual counts also depend on the individual tree's
shape, seed and fruit-growth stage. Pine and Willow never gain apples.

Scale adjusts the existing tree generator's size (height, branch layout and wood
radii), relative to each species' canonical planting size. It is not a uniform
transform of every rendered organ: leaf spray dimensions and individual apple
size retain their existing independent controls. Changing density does not change
wood/leaves. No shader/instance ABI changes are required.

## Ownership and persistence

- Both controls are declared in `config/gui.toml`, with one generated GUI owner.
  Debug **Save** remembers the planting preset; Search discovers it automatically.
  Older GUI files gain missing controls without being rewritten or changing their
  stored live-tree authoring. Existing saved slider values are preserved.
- The next-tree preview consumes the preset, retaining its seed while adjusting
  values. A successful planting receives that complete mature description once,
  through the same path in finite/shop and unlimited modes. Subsequent plantings
  continue using the preset until changed; controls do not reset after one tree.
- Slider edits refresh only the placement preview. They do **not** request an
  existing-tree rebuild, regenerate existing apples or reset a growth clock.
- Individual trees already save `mature_desc`; growth, save/load and fruit
  publication continue to consume that captured description, not today's sliders.
  No terrain-save schema migration is needed.
- Size starts from the species' canonical planting size, rather than multiplying
  the live editor's size. Loading a previously scaled tree into the editor cannot
  compound its scale on the next planting. The new planting size/density controls
  own these two values for Grow, independently of the existing live tree editor.

Use **Grow → Tree** after setting the sliders. New trees still begin as saplings;
apples appear only at the existing eligible growth stages, not on a just-planted
sapling. Settings changes do not freeze normal growth/wind on old trees.

These guarantees concern the two **new-tree** sliders. Existing live authoring,
global Tree Age and Apple Size controls retain their existing scope; they are not
safe substitutes for the new controls if you want to leave old trees alone.

## Validation

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
