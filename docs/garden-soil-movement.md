# Conservative garden soil movement

## Plow

Choose **Plow** and drag on exposed Dirt. The brush cuts up to three local layers
and settles the same parcels into two side banks. Brush half-width is bounded to
2–6 voxels. Existing Edit/Till/Soil tools retain their separate meanings; Plow is
not a material-conversion or unlimited-supply brush.

`src/soil_movement.rs` plans against bounded, authoritative 3D bytes. It removes
only exposed Dirt, uses the existing `terrain_deposition` planner for both banks,
and preserves the complete multiset of soil moisture parcels. Grass appearance is
re-derived at the new location. A full accepted plan uploads both debit and credit
in **one atlas replacement**; only then does normal visible-terrain publication
update meshes, roots, lighting, collision/query caches and revisions.

If either bank cannot accept every parcel, the whole speculative CPU plan is
discarded—even if its other bank succeeded. There is no intermediate world hole,
held plow inventory, partial GPU commit or separate add operation to cancel. Air,
rock, occupied space and world edges cannot create or consume soil. Fast pointer
jumps over 32 voxels begin a new dab instead of reading an unbounded world region.
Stationary holds do not repeatedly excavate the same point. Unlimited materials
has no effect on this tool's conservation contract.

### Support/collapse decision

Treat existing garden Dirt as cohesive: no new whole-world slope solver, cave
collapse, or detached-soil-to-visual-particle conversion is invoked by Plow.
Only the deliberately moved parcels settle, through the existing local supported
column/repose solver. Unsupported drop targets and obstructed banks fail safely.
This is a bounded gameplay choice, **not** a geotechnical simulation or a claim
that real unsupported excavations are safe.

Research context: [OSHA soil classification, Appendix A](https://www.osha.gov/laws-regs/regulations/standardnumber/1926/1926subpartpappa)
distinguishes cohesive clay-rich soil from granular soil without cohesive
strength. The primary-source search index returned these definitions; direct
retrieval was blocked by HTTP 403, so a full document review is not claimed.
The game deliberately chooses a single cohesive Dirt material rather than
introducing realistic strength classes or applying excavation safety slopes.

### Validation

- Three deterministic planner tests: visible cut + banks with exact quantity and
  moisture-histogram conservation; rollback when the second side is blocked;
  unsupported/out-of-bounds, invalid-coordinate and non-Dirt cases.
- Production pointer mapping/hold tests and text-toolbar tests pass.
- Native Release fixture moved **1,200 + 874 = 2,074** parcels. Actual terrain
  counts before and after: **110,592 → 110,592**. Real save/reload retained 110,592.
- Inspected `target/garden-plow.png` (uniform dry-soil fixture). An earlier
  alternating-moisture fixture visually obscured the furrow; moisture conservation
  remains covered by the deterministic histogram test, not inferred from this image.
- Two native edits read **95,744 bytes each**, and took **17.035 / 20.164 ms** for
  the synchronous application path including readback/planning/upload/publication.
  This is correctness-fixture timing, not a sustained-drag benchmark or a claim
  that the tool meets a 60-FPS budget. Further input/performance/art polish remains
  appropriate; no global readback or background idle scan was added.
- Formatting, `cargo check`, Release build and ordinary hidden muted smoke pass.
  Final fixture/load/smoke logs have no ERROR, panic or VUID.

```sh
RE_FLORA_SOIL_MOVE_REVIEW=1 cargo run --release -- --hidden --mute \
  --terrain-save target/garden-plow.rflterrain \
  --screenshot garden-arrival target/garden-plow.png --screenshot-delay 2 --auto-exit 3
RE_FLORA_SOIL_MOVE_REVIEW=load cargo run --release -- --hidden --mute \
  --terrain-load target/garden-plow.rflterrain --auto-exit 1
```

## Shovel

**Shovel** is distinct from the existing general-purpose **Edit** brush. Press
left mouse to scoop only Dirt (bounded 2–6-voxel radius), keep holding to carry,
and release elsewhere to settle it through the same finite-volume deposition
engine used by Soil. One press never continuously digs. A loaded shovel cannot
scoop again. A simple 19-block brown clump and `Shovel · N soil` status display
represent the carried amount; this is not a finished hand/shovel animation.

- `CarriedSoil` is the sole owner of removed parcels, including moisture bytes.
  Credit follows the successful atlas removal and precedes any fallible terrain
  publication. Drop debit is the exact successful GPU write count, likewise before
  publication. No debug unlimited-resource flag enters either operation.
- Blocked/partial drops retain every unplaced parcel. Click/hold/release again to
  retry. Changing tools, losing focus, opening UI or canceling a pick discards the
  gesture, not its soil. It remains on the shovel rather than silently becoming
  generic backpack stock. Saving stores it with the world; loading starts with
  no active gesture. Old snapshots default to an empty shovel.
- A quick release before the asynchronous scoop answer preserves its captured
  release ray. The drop is queried against the **new** world revision after the
  scoop commits. Stale/expired picks cannot spend resources or repeat an action.
- The same cohesive-terrain policy described above applies: the excavated cavity
  remains supported rather than triggering a global cave-in. Only dropped soil
  settles. This is deliberately not a soil-strength or landslide simulation.

### Shovel validation

- Planner/input tests cover Dirt-only extraction, non-soil preservation,
  cancellable planning, press/hold ownership, quick release, stale/canceled picks,
  retry, partial debit, JSON ownership and old-save defaults.
- Actual Release scoop: **688** parcels; **109,904 terrain + 688 held = 110,592**.
  A canceled gesture and invalid release retain all 688.
- Saved the world while loaded, restarted from that file, and deposited all 688:
  **110,592 terrain + 0 held**. Inspected `target/garden-shovel-held.png` and
  `target/garden-shovel-drop.png` after reframing the controlled bed fixture.
- Actual GPU partial drop into a one-cell legal cavity: **1 placed / 687 retained**;
  including that separate cavity, the total remains exactly **110,592**.
- Separate native four-moisture fixture retains histogram
  **[27,648, 27,648, 27,648, 27,648]** after scoop and deposition.
- Native runs used the existing `Infinite Materials=true` debug setting and still
  conserved finite carried soil. No inventory minting is hidden by debug supply.
- Formatting/check/Release build, input/persistence/deposition-related tests and
  ordinary hidden muted smoke pass. Inspected final logs: no ERROR, panic or VUID.
  Automated input-state and native geometry/persistence checks are not subjective
  manual gesture approval. Drop cost inherits the existing Soil sampling mode;
  no sustained interaction performance acceptance is claimed.

```sh
RE_FLORA_SHOVEL_REVIEW=hold cargo run --release -- --hidden --mute \
  --terrain-save target/garden-shovel-held.rflterrain \
  --screenshot garden-arrival target/garden-shovel-held.png --screenshot-delay 2 --auto-exit 3
RE_FLORA_SHOVEL_REVIEW=load-drop cargo run --release -- --hidden --mute \
  --terrain-load target/garden-shovel-held.rflterrain \
  --screenshot garden-arrival target/garden-shovel-drop.png --screenshot-delay 2 --auto-exit 3
RE_FLORA_SHOVEL_REVIEW=partial cargo run --release -- --hidden --mute --auto-exit 1
RE_FLORA_SHOVEL_REVIEW=moisture cargo run --release -- --hidden --mute --auto-exit 1
```

All review hooks are unsaved fixtures. Generated geometry is original Rust code;
no external image/model or AI generation service was used.
