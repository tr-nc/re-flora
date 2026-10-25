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

Shovel carry/release is a separate pending checklist item, not implied by this
Plow implementation. There is no subjective manual-drag approval yet.
