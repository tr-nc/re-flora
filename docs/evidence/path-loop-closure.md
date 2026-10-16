# Return-to-start construction loops

## Player contract

Pavement slabs/bricks and all three low-wall materials can close the **current held
stroke**. Draw a sufficiently long loop, return near the original press position,
then release when the closure marker and `Release to close the loop` hint appear.
Moving away before release removes the proposal without rebuilding published terrain.
Dirt vegetation-clearing paths, short clicks and active paving backdrag do not snap.

Only the unfinished tail is fitted. The tool does not move the original start or old
courses, connect separate gestures, override protected terrain, fill the interior,
create editable spline objects or add whole-wall Undo. Tight/unsupported/obstructed
joins can still be rejected normally. Closed masonry preserves its ordinary head/bed
gaps; this is not a new watertight-container or soil-simulation feature.

For a raised planting bed: use **V**, enable low walls in **R → Pavement**, choose Stone
or Brick, draw the rim and release to close. Use the existing Soil and Grow tools inside.
No new saved setting, shader, file-format migration or generated GPU structure is needed.

## Ownership and invariants

- `CourseContinuity` retains the **first actually published course** as well as the latest.
  Its existing cloned retraction checkpoints restore both facts together.
- `ActivePavement` detects a return near the user's original press, not an arbitrary older
  path. Capture radius is derived from width/pitch; minimum built length prevents clicks
  from closing. Existing backdrag has priority.
- `path_band/closure.rs` fits a bounded tangent-constrained cubic/straight approach from
  the last published exit to the first published entry. Independent handles and a bounded
  approach-length search allow both circular returns and corner starts. It cannot shortcut
  an unbounded unpublished backlog. Arc-length stations divide only this local connector.
- Every section must produce convex, non-overlapping band cells and valid whole material
  pieces. The final exit is the exact original entry, not merely a nearby endpoint.
  The material adapter supplies the published-coverage predicate; no seam overlap exception
  or selective deletion of an inner tile is introduced.
- A whole closed bond has even half-cell parity. The final wall head joint reuses node zero,
  including its deterministic stone-gap identity, so alternating layers match across the seam.
- The normal candidate preflight still checks support, staged/published overlap, world bounds
  and protected atlas voxels. A snapped candidate remains unpublished until release. Neither
  the connector nor the prior open tail is emitted while the user is considering that snap.
- Finishing a compiled connector is idempotent. Existing terrain/flora publication and save
  ownership remain authoritative; no retained loop object is serialized.

The recorded native corner fixture exercises a particularly important distinction: matching
only the terminal tangent is insufficient. A wide connector can sweep through the first
course *before* arriving at that tangent. The planner considers straight approaches and tests
actual footprints instead of exempting the first course from overlap protection.

## Validation

- `cargo fmt --check`, `cargo check`, full `cargo test`: passed; main target **1638 passed,
  7 ignored**; library 4 passed, display-authorization target 1 passed/1 ignored.
- Four deterministic closure tests cover clockwise/counterclockwise circles, sparse/dense
  input, stone/brick paving with smoothing on/off, all wall materials, multiple widths,
  corner starts, sloped ground, reversible preview, even bonds and identical seam planes.
  `fixtures/loop-corner.json` records actual native terrain-ray input, including small XZ
  offsets and discrete terrain heights; it is not a hand-smoothed replacement.
- Existing pavement (64 tests at that stage) and path-band (15) checks passed, including
  backdrag, rejection recovery, stationary input and continuous publication.
- `scripts/validate-path-loops.sh`: passed in hidden muted Release mode. Production placement
  and atlas readback verified Earth/Stone/Brick rings (28/32/28 courses), a rectangle corner
  start, exact continuous sections, even bond parity, every row/layer's material, byte-identical
  atlas while snapped, unchanged interior soil columns, and muted sound on release. Saved
  terrain was reloaded in another process and witness voxels/interiors compared.
- `scripts/validate-low-walls.sh`: passed unchanged for the existing six open-wall fixtures,
  upper-obstacle rejection/recovery, deep bedrock protection, no internal half-brick gaps,
  same/fresh-gesture retrace, audio lifecycle, save/reload and Dig.
- Final `cargo build --release` and normal `--hidden --mute --auto-exit 0.5` startup passed;
  the run log was inspected. No GUI configuration or generated-file diff.

Artifacts (not committed): `target/path-loops/rings.png`, `rings.rflterrain`, `witness.json`,
`seed.log`, `reload.log`; existing wall regression artifacts under `target/low-walls/`.
The native screenshot was inspected: continuous soil/stone/brick rims, with the enclosed
areas left empty. Construction geometry is not a guarantee of a perfect mathematical circle:
only the unpublished tail can move, and corner-start joins may round outward.

Native fixtures drive the real candidate/publication seam, not OS mouse events. Physical
pointer feel and manual Soil/Grow inside the bed remain user try-out checks. Paving closure
has deterministic planner/material coverage; the native loop fixture specifically covers
walls. This is correctness/visual evidence, not a new performance benchmark or acceptance.
