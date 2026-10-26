# Per-tree sapling growth

With **Tree Planting → Plant mature trees instantly (new trees)** unchecked,
newly planted catalog trees begin at age **0.375**, grow in 1/32-age stages, and
reach age 1 after **900 simulated seconds**. With the default clock this means
one age stage roughly every 45 seconds. Pausing world updates pauses growth;
there is no offline/wall-clock catch-up. Preview and planted age agree. Mature
canonical descriptions and seeds remain stable: age is applied once, not baked
repeatedly into a shrinking description.

`TreeRecord` owns published age and an optional elapsed-growth clock. At most one
pending tree is republished per half simulated second, with the largest pending
age gap first and stable-ID tie-breaking. Existing transactional tree publication
updates wood, leaves, physics, fruit, shadows and canopy audio together. Mature
Apple trees fruit from age 0.75; smaller saplings do not carry full-sized apples.

Since 2026-10-27 that saved checkbox defaults to **checked**: new trees instead
start at age 1 without a growth clock. The preview uses the same initial age as
placement. Toggling only affects future plantings; existing sapling clocks and
stored mature descriptions are unchanged. See [planting preferences](garden-tree-planting.md).

## Appearance and compatibility

Garden architecture recipes author a slender cylindrical juvenile leader and
interpolate toward their existing mature taper after age 0.5. This belongs to the
axis recipe, not an artificial radius floor in the general compiler or renderer.
Legacy recursive profiles retain their authored-radius contract; mature garden
geometry is unchanged. Positive foliage density now retains at least one leaf
per foliage spray when its small discrete budget would otherwise round to zero.
An explicitly zero density remains bare. Bounded sampling also guarantees that
positive spray does not disappear merely because all random candidates miss.

Snapshots store individual ages and clocks. Missing fields in older saves use
the old saved global age and remain frozen, rather than becoming newborn trees.
Old terrain bytes remain authoritative on restore; the revised young architecture
is used when subsequently rebuilding those recipes. Explicit global-age editing
is a debugging override and freezes automatic clocks for the rebuilt trees.

## Construction safety and bounds

- New trees require finite positions, available monotonic IDs, fewer than 512
  records, and a mature wood footprint inside the world. This is not a reservation
  excluding neighboring plants.
- Before a growth stage, a bounded local atlas read checks actual candidate wood
  cells against occupied terrain. Air and existing tree wood are compatible;
  Dirt is permitted only at/below the planted base. Other material intersections
  pause growth without replacing the construction. The read is capped at 8 MiB;
  oversized/out-of-world growth pauses too. No idle readbacks occur.
- Hand removal of tree wood freezes growing records whose bounds intersect the
  edit. This is deliberately conservative, not an exact per-voxel tree-owner test.
- Failed transactional publication pauses that clock; previous geometry is
  retained by the existing rollback owner. Existing fault-injection unit tests
  cover that owner. No new native GPU fault injection is claimed.
- There is no restart-growth button, root competition, seasonal dormancy, or
  botanical growth simulation. Manual age editing remains an explicit override.

## Native evidence

Apple seed 907, identical camera and base, actual Release application:

| Published age | Simulated seconds after planting | Height (voxels) | Wood sections | Leaves | Fruit |
|---|---:|---:|---:|---:|---:|
| 0.375 | 0 | 9.933 | 48 | 2 | 0 |
| 0.5 | 180 | 18.521 | 118 | 18 | 0 |
| 0.75 | 540 | 33.719 | 242 | 1,253 | 24 |
| 1 | 900 | 39.099 | 250 | 1,628 | 24 |

Captures: `target/garden-sapling-{0.375,0.5,0.75,1}.png`. The early sapling is
intentionally sparse; thin lateral wood can still appear as separated voxel
fragments. These are inspected first-pass screenshots, not player approval of
all ages/species. Unit checks also generate all plantable species at four ages.

Actual save/restart restored age 0.5, elapsed **180.001**, and 18 leaves. The
`cycle` review primed the clock just before its threshold; the normal world tick
published age **0.40625 at 45.000 seconds**. Native wood removal froze age 0.5;
manual age override stayed at 0.75 after additional elapsed time. A Rock voxel in
future wood stopped growth and survived unchanged. Invalid/nonfinite/out-of-world
plant attempts did not mutate the forest.

In two controlled single-tree Release runs, a half-age publication took
**12.465 ms / 13.116 ms including preflight**, and a jump to maturity took
**8.291 ms / 18.905 ms including preflight**. These are individual stage timings,
not a dense-garden benchmark, sustained frame budget, or proof of 60-FPS acceptance.

```sh
for age in 0.375 0.5 0.75 1; do
  env -u WAYLAND_DISPLAY RE_FLORA_SAPLING_REVIEW=$age RUST_LOG=info \
    cargo run --release -- --hidden --mute \
    --terrain-save target/garden-sapling-$age.rflterrain \
    --screenshot garden-arrival target/garden-sapling-$age.png \
    --screenshot-delay 2 --auto-exit 3
done
env -u WAYLAND_DISPLAY RE_FLORA_SAPLING_REVIEW=load RUST_LOG=info \
  cargo run --release -- --hidden --mute \
  --terrain-load target/garden-sapling-0.5.rflterrain --auto-exit 1
# Additional native modes: cycle, edit, blocked, override.
```

Formatting/check, vegetation, tree-generation and terrain-persistence tests,
Release build and ordinary hidden muted smoke passed. Final native logs contained
no ERROR/panic/VUID. No generated source files or external art assets were added.
