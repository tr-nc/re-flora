# Sunflower height relative to the player

Date: 2026-10-26. Player feedback: the original sunflower feels too short; roughly
doubling its height is acceptable. This is a visual tuning change, not a new
species or a physical/botanical simulation.

## Primary-source findings

- [RHS: How to grow Helianthus](https://www.rhs.org.uk/plants/helianthus/growing-guide),
  retrieved in full: annual sunflowers “can range from a mere 30cm (1ft) tall up
  to 3m (10ft) or more.” Its planting guidance describes dwarf plants as
  **30–60 cm**, and giants over **2.4 m** needing substantial support. The guide
  explicitly distinguishes annual and perennial sunflowers; perennial heights
  are not used to set this annual species.
- [NC State Extension: Helianthus annuus](https://plants.ces.ncsu.edu/plants/helianthus-annuus/),
  retrieved in full: the common sunflower has stems that “grow 2 to10 feet tall”
  (approximately **0.61–3.05 m**). Cultivar and growing conditions matter.

There is no universal sunflower-to-person ratio. If **1.70 m is chosen merely as
an illustrative adult reference**, RHS dwarf forms are about 0.18–0.35 people
high, while 3 m forms are about 1.76 people high. This is arithmetic, not a claim
about average human height. A roughly person-height garden sunflower is a
reasonable art choice between these extremes; the game is not selecting a named
cultivar or claiming photorealistic proportions.

Search results also identified RHS individual-species/cultivar detail pages, but
the retrieved extraction did not expose their height fields. Their search-only
numbers are deliberately not used as evidence here. No reference photographs,
external model, texture or image-generation service were used.

## Game measurement and decision

The authoritative player capsule in
`src/app/core/physics/surface_motion.rs` has radius 4 and cylindrical half-height
8 terrain voxels: total height **2 × (4 + 8) = 24 voxels**. Eye height is not body
height and should not substitute for it.

Only `sunflower.stemLayers` in `assets/models/flower-source.mjs` changes:
**68 → 136**. The shared publisher regenerates `assets/models/flowers.json`;
Rust derives its registry and the renderer derives stem bounds/attachments from
that catalog. There is no species-specific rendering patch or global height
slider change. IDs/order, head geometry/texture/palette/tilt, stem width and all
other species remain unchanged. Existing saved sunflowers use the revised model
on loading the rebuilt app; their root, growth and stable species identity remain.

For a mature plant, neutral height sample (`z = 0`), no wind/spawn deformation,
and the checked-in controls (mean 0.6, variance 0.1, stem dimension scale 1,
whole-plant size 0.5, head scale 1.11):

```text
layers          = round(authored layers × 0.6)
world conversion = 10 terrain voxels per model unit × overall size 0.5
stem height      = layers × 0.05 × 5 terrain voxels
head top offset  = max(authored head vertex Y) × 1.11 × 5 = 1.893478 voxels
```

| Reference mature plant | Before | After |
|---|---:|---:|
| Selected stem layers | 41 | 82 |
| Stem height | 10.25 voxels | 20.50 voxels |
| Total height including head | 12.1435 voxels | 22.3935 voxels |
| Fraction of the 24-voxel player | 0.506 | 0.933 |

The **stem doubles**; total height increases about **1.84×**, not exactly 2×,
because the flower head is intentionally not enlarged. This moves the reference
plant from half-person height to nearly person height. The same position-seeded
height variation remains, as do growth/moisture/size controls; actual plants are
not all guaranteed to reach this reference height. Rounding and the existing
minimum-one-layer clamp also mean extremely short variants need not double exactly.

## Validation

- Before/after published-catalog deep comparison: only the sunflower layer count
  changes inside the flower records. Every head vertex, UV, texture, palette,
  socket, species identity/order and other flower record is identical.
- The flower-publication Node test still expected six species and failed before
  this change because it omitted the already-added sunflower. Its explicit stable
  catalog expectation now includes the sunflower and its new 136-layer value.
- 14 shared flower Node tests pass, including deterministic republication,
  geometry/palette separation and preview/native catalog agreement.
- 36 Rust `flora::` tests and 9 surface snapshot tests pass.
- `cargo fmt --check`, `cargo check`, `cargo build --release`, hidden muted native
  sunflower capture and ordinary 0.5-second smoke pass. Native run logs and the
  latest-log tail have no ERROR, panic or VUID diagnostics. No tracked generated
  Rust files or GUI defaults changed.
- Inspected `target/sunflower-height-before.png` and
  `target/sunflower-height-after.png`: same garden-arrival camera and planted root
  fixture, visibly longer stems with intact heads. These are temporary native
  review outputs, not runtime assets or replacements for the historical offline
  report screenshots. Wind/animation timing is not pixel-locked between runs.

Capture command (run before/after rebuilding):

```sh
env -u WAYLAND_DISPLAY RE_FLORA_GARDEN_PLANT_REVIEW=sunflower RUST_LOG=info \
  cargo run --release -- --hidden --mute \
  --screenshot garden-arrival target/sunflower-height-after.png \
  --screenshot-delay 2 --auto-exit 3
```

A user's visible game was left running untouched during validation. No performance
comparison is claimed from concurrent GPU runs, and the user's current process
still has the old embedded assets. Save the garden and restart with
`cargo run --release` to see the new model; there is no asset hot reload.
The old offline garden report remains a record of the initial delivery, not a
screenshot of this subsequent height adjustment.
