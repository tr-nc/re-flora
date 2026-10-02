# Restaurant scene: raster / true-voxel A/B

## Try it

Run `cargo run --release -- --rooftop-poc`. Press **R** to open Debug Panel,
search **restaurant** in the top Search field (or expand **Terrain**, the displayed
name of the stored `Voxel` section), and enable
**Restaurant scene: true voxels (A/B; off = original models)**.
Unchecked retains the authored raster scene; checked stamps all 317 boxes into the
normal atlas → Surface → Contree → scene-acceleration pipeline and removes the
static scene renderer. The checkbox uses declarative GUI configuration and Save.
It does nothing outside the rooftop scenario. The scenario remains unsaved.

Backup tag: `backup/pre-voxel-scene-20260214` (`4da8b50e`).

## Geometry and material constraints

- Complete neighbourhood: restaurant shell, interior furniture/kitchen, glazing,
  awnings, sign, benches, street markings, lamps, planters and eight street trees.
- One-voxel resolution. Fractional details are conservatively rounded to enclosing
  cells. Original overlay order is retained when batching adjacent materials.
- Translate by `(180, 24, 160)` voxels, without scaling/cropping, into a rooftop-only
  `1024 × 512 × 1024` domain. Camera, soil bounds and fixtures translate together.
  Ordinary garden dimensions, saved terrain format and defaults remain unchanged.
- Existing IDs 2–11 are unchanged, including dirt, sand, both woods and every
  existing rooftop palette entry. Unused IDs become dedicated glass (1), asphalt
  (12), painted metal (13), terracotta (14), canopy green (15).
- The type mask remains `0x0f`: **16 encodable IDs including empty**. Edit counters
  and per-type removal budgets cover all 16, including the final four lanes.
- Dedicated glass uses the existing dielectric transport, refraction/reflection,
  optical shadow and resolve code. Enabling its pipelines does **not** reinterpret
  sand. The older sand-as-glass test scenarios retain their separate material mode.
- Limited pigments approximate the original per-box RGB colors; they are not a
  pixel-identical color reconstruction. Voxel lighting/reflections are deliberately
  real engine shading, not the original raster colors pasted onto proxy models.

## Interaction / ownership

The plantable roof starts empty. Existing soil placement, gradual removal,
smoothing, Grow, inventory rules and tool input remain intact. Existing fixed-shell
triangle collision is retained in both modes to preserve character and mower
interaction; B additionally publishes ordinary voxel geometry/collider updates.
Only rendering ownership switches, not the fixed physical support.

Fixed non-soil atlas cells carry ownership in bit 7, previously reserved for those
types. Dirt/sand retain all state bits and are never classified as fixed cells.
Every shared primitive writer (including tree growth and carving) refuses to
replace owned cells. Only the explicit scene-authoring fill policy can write or
clear them for A/B switching. Thus immutability is enforced below UI picking, not
by withholding an edit button alone. Both scene modes retain bounded roof editing.

Switching stamps/clears only the fixed boxes, rebuilds visible voxel geometry while
preserving flora, and leaves editable soil untouched. It is a synchronous setup
operation, not an every-frame geometry upload. Larger-domain clipping is used by
soil/flora edits, planting, wind, particles, moisture scheduling and water sampling.

## Validation

Commands and logs are local to this worktree (`target/voxel-scene-*.log`):

```sh
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --mute --auto-exit 0.5
RE_FLORA_ROOFTOP_VALIDATE=1 RE_FLORA_ROOFTOP_VOXELS=1 \
  cargo run --release -- --rooftop-poc --hidden --mute --auto-exit 0.5
RE_FLORA_ROOFTOP_VALIDATE=1 \
  cargo run --release -- --rooftop-poc --hidden --mute --auto-exit 0.5
RE_FLORA_MOWER_SURFACE_VALIDATE=1 RE_FLORA_ROOFTOP_VOXELS=1 \
  cargo run --release -- --rooftop-poc --hidden --mute --auto-exit 2
```

- Final `cargo test`: **1,293 app tests + 4 library tests passed; 4 ignored**.
  Three Python capture/decoder contract tests also passed.
- Tests cover full-domain geometry, non-overlap with editable soil, material IDs,
  ownership exclusion for every possible soil byte, counter ABI and expanded
  moisture scheduling. A CPU collision test covers the complete scene before/after
  translation, including a physics step.
- Actual Release fixtures verify first/second soil dabs, gradual removal, smooth
  floor clipping, Grow, fixed roof grounding, production RMB placement and unchanged
  backpack. Their edit center exceeds the original 512-voxel X/Z bounds.
- A/B round trips from **both starting modes** compare all **22,643,712 editable
  atlas bytes** exactly, preserve planted flora, inspect glass occupancy and reject
  ordinary empty/cherry-wood writes over owned glass. Initial mode is restored.
- Real mower fixtures pass in A and B: bare roof and road placement/driving,
  grounding, stationary hold/heading, resumed motion and release destruction.
  The existing fixture also now waits for the loading overlay to release its test
  pixel before invoking production pointer controls; it does not bypass UI ownership.
- Normal startup and successful fixtures finish with `failures=0`, without
  ERROR/panic/Vulkan validation diagnostics. Early failed experiments remain in
  diagnostic logs, not counted as passing validation.
- RFIRR v11 fixture regenerated from the Rust producer's actual serialized output:
  only its eight-byte compiled lighting-model identity changes, not format/payload.

Visual comparison was inspected at the same opening camera:
`target/rooftop-original.png` and `target/rooftop-voxels.png`.
The layout, storefront openings, streets and trees reproduce the original;
voxel materials, edge quantization and glass reflections visibly differ.

## Costs / boundaries

This is a visual experiment, **not performance acceptance**. No performance budget
is asserted or silently waived. Both rooftop A/B modes allocate the enlarged domain
for a fair runtime switch: atlas 512 MiB, Contree pools approximately 431 MiB, and
DDGI grid 33 × 17 × 33 at spacing 32 (approximately 150 MiB allocated resources).
Visibility traversal safety limits cover the larger domain. Switches temporarily
stall for authoring/publication; memory and frame-time optimization are separate
follow-up work after visual review. No visible game is launched automatically.
