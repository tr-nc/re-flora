# Opening meadow / player grass-brush continuity

## Failure and cause

The fresh-garden recipe introduced in `621b0944` (2026-10-21) painted a
`Species(SHORT_GRASS)` meadow, while the player's ordinary grass brush selected
`GrassMix`. Both used the same density mask, but not the same species mapping.
Replanting an equally dense adjacent area therefore changed its height
population. Occupancy correctly preserves existing roots, so brushing across
the boundary could not turn the original all-short meadow into a mixed meadow.

The native regression, added before the fix, mowed a capsule crossing a chunk
boundary in the actual opening meadow and replanted it through
`apply_surface_flora_regeneration_at`. It failed with:

```text
opening/player grass distribution differs: missing=505 unexpected=505 removed=902
exit=101
```

These are root-and-species identities, not growth, hydration or GPU draw order.

## Fix and ownership

`seed_opening_grass` now fixes its selection to `GrassMix` rather than accepting
an independent startup species choice. Opening and player brush transactions
both delegate to the existing `calculateFloraDensityPlacement` and
`selectPaintedFloraType` shader path. World coordinates remain the authority for
the density and tall/short choice; no CPU copy of the shader algorithm, neighbor
heuristic, edge blur or replacement of existing plants was added.

The opening recipe version is 4. Existing footprints, intentional mowing,
flowers and trees are preserved. Saved plant identities are not migrated or
regenerated on load: old all-short meadows remain as saved. This correction is
for the opening and occupancy grass brush, not a redesign of individually
placed inventory seeds, explicit single-species debug planting or legacy terrain.

## Native regression and validation

```sh
RE_FLORA_GARDEN_GRASS_AUDIT=1 RUST_LOG=info cargo run --release -- \
  --hidden --mute --auto-exit 0.5
# Repeat with --garden-seed 42 for another fresh-garden seed.
```

Require the `[GRASS_DISTRIBUTION_REVIEW]` success marker; do not use a loaded
world or an alternate scene. The check runs after the real opening meadow is
planted, before authored plants and deliberate mowing change available roots.
It exercises two overlapping cross-chunk capsules, mow/replant, and repeated
brush strokes with distinct serials and times. All roots/species in the world
must equal the original set, both grass species must occur in each patch, and
the original flora snapshot and UI selection are restored afterward. GPU work
is opt-in rather than part of the deterministic unit-test suite.

Both default seed 20260519 and seed 42 passed after the fix:

```text
checked_roots=1784 mixed=true mow_replant=identical overlapping_strokes=identical cross_chunk=true untouched_roots=preserved
```

- `cargo fmt --check`, `cargo check`, and `cargo build --release` pass.
- Normal hidden muted Release startup/run/shutdown passes; inspected logs have
  no ERROR, panic or VUID. A hidden `garden-arrival` capture also completed.
- `cargo test`: 1721 passed, 9 failed, 13 ignored in the main binary suite.
  The same full binary suite at baseline `e1b84c7e` in a detached worktree had
  exactly the same results: two GUI loader default assertions and seven
  `tracer::raster_tree` tests fail. No full-suite-green claim.
- No shader source, generated files, saved GUI defaults or asset resources changed.
- This establishes species/root continuity; no performance acceptance or live
  user visual approval is claimed.
