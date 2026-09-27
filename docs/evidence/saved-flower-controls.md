# Saved flower controls — worker handoff (scope 1)

Worker `flowers`, branch `agent/flower-cache-controls`, based on cache-only tip
`4f52cf3f`. No live-generation path restored, other-worker edits, pushes, releases,
visible game or pi/workflow configuration changes.

## Behavior and ownership

- Saved Debug → Flora → Model Flowers controls now include complete-head size
  (0.25–4), plant height / stems and attachments (0.25–4), and independent flower
  directions (8–512, default 16). Existing 8–64px resolution controls both bake and
  display. Existing overall size and head/whole A/B keep their saved meanings.
- Older saves receive missing Flora declarations through the standard loader;
  no custom save hook or App-only setting. A real old-file/save/reload test caught
  and fixed the missing-declaration crash before delivery.
- `src/flora/models.rs` consumes published `heads[].anchor`, root-relative. Stem
  positions use vertical scaling and inverse-transpose normals. Every triangle
  in a complete head uses `height(anchor) + head_scale * (point - anchor)`,
  including calyx/center. Default framing is preserved exactly. Worker bounds
  refit to transformed vertices; the controller follow-up below supersedes that
  framing policy to avoid a discontinuity at the default slider values.
- `ModelPixelFrame`'s cache owns immutable transformed source generations. Native
  mesh stems read the **same triangle buffer** as whole/head surface baking;
  parts use the same ranges and bounds. Growth, overall scale and wind remain a
  shared rigid runtime pose, not cache invalidators. Culling includes arbitrary
  rigid orientation, displayed quads and spawn translation/overshoot.
- Per-kind view counts drive both baking and nearest-view selection, from the
  published cache entries. The shared azimuth prefix is sized to the largest
  **current** bank, not 512. Geometry/resolution/direction changes invalidate only
  flowers; overall size, A/B, growth, display, lighting and resize reuse surfaces.
- Existing GpuPagedStorage/ready-slot/transient-descriptor ownership is retained.
  Flower relighting tiles also shrink to their current rounded demand. No second
  flower cache, retirement queue or procedural recipe.

Two narrowly necessary VKN lifetime fixes accompany the feature:

1. Idle/unused transient descriptor tails are trimmed when their slot is ready,
   so old bake sources/direction buffers are not pinned forever. A deterministic
   two-slot test proves pending owners survive and unused owners retire.
2. PipelineLayout retains its empty padding layout, absent from reflection. Moving
   bake inputs to one transient set exposed a reproducible deleted-layout Vulkan
   error at pipeline creation. The same hidden Release repro passes after the
   ownership fix. This is separate from the known retired-shader unit-test issue.

## Executed evidence

All GPU runs were hidden/muted Release, serialized with
`flock --close /tmp/re-flora-vegi-cache-controls-gpu.lock`. X11 was selected with
`env -u WAYLAND_DISPLAY` (the new runner does the equivalent). Logs were inspected,
including `cargo run --release -- --tail-latest-log` from this worktree.

Evidence root: **`target/flower-controls-evidence/` (about 42 MiB)** in this worker.
Copy before cleanup; full copies of the new sweep and native flower captures are
already there. Initial failed layout reproductions are retained and are NOT passes.

- `cargo fmt --check`, `cargo check`, `git diff --check`: passed.
- `cargo test model_pixel`: **43 passed, 1 existing ignored**. Includes transitive
  cache-only guards, per-kind invalidation, demand sizing, exact native/bake source
  agreement, growth/wind/spawn bounds, and ready-slot tile growth/shrink.
- `cargo test flora::models`: **3 passed**, including all eight species × 16
  independent head/height combinations, authored anchors, colors, normals and bounds.
- `cargo test app::gui_config`: **55 passed**, including actual saved-field round
  trips, old overall-size/A-B compatibility and missing-control defaults.
- `cargo test render_frame_input`: **1 passed**, including all flower controls.
- `cargo test -p re-flora-vkn memory::paged_storage`: **4 passed**;
  `cargo test -p re-flora-vkn idle_bakes_and_shrinking`: **1 passed**.
- `python3 scripts/run_slang_tests.py`: **25 CPU tests passed**.
- New JavaScript syntax/help/invalid-argument checks passed (usage error exits 2).
- Final ordinary `cargo run --release -- --hidden --mute --auto-exit 0.5`: passed,
  successful shutdown, failures=0, no application/Vulkan errors
  (`final-smoke.log`, `final-smoke-tail.log`).
- `node scripts/validate-flower-controls.mjs --seconds 30`: **18 phases passed**
  (`control-sweep/summary.json`, log and screenshot). All eight species, both A/B
  modes, independent head/height edits and opposite slider extremes, live 8/32/64px,
  8/16/37/512 directions, old overall scale/growth and post-consumer resize.
  **4,853,824 baked records bit-checked, zero mismatches; 139,285 independent
  occupied-cell checks; maximum depth error 0.000010669** (bound 0.00002).
- Measured flower payloads grow/shrink through **11,010,048 → 688,128 → 22,020,096
  → 1,591,296 → 344,064 → 22,020,096 → 11,010,048 bytes**. Resident surface bytes
  equal current active bytes after completed slots in every phase. Exact page
  alignment/table overhead is asserted, not a slider-maximum reservation.
  Shared directions allocate **256 → 8192 → 592 → 256 bytes** for current maximum
  counts **16 → 512 → 37 → 16**. Other banks remain at 16 in this fixture.
- `node scripts/validate-model-cache.mjs --seconds 30 --output
  target/flower-controls-evidence/cache-matrix`: **13 phases passed**, all consumers,
  fruit drops and resize. **14,534,528 records**, zero mismatches; **157,288**
  independent occupied cells; max depth error **0.000006706**. The existing fixture
  now explicitly sets both global and flower counts when sweeping all banks.
- `node scripts/validate-flower-models.mjs --seconds 20`: A/B captures plus all nine
  existing lifetime/growth/resize phases passed (`native-flowers/`).

The new sweep initially assumed multiple native frame slots; inspection showed
production `MAX_FRAMES_IN_FLIGHT=1`. Its final summary explicitly records
`readySlotCount: 1`, `sawPendingRetirement: false`. The native run proves immediate
replacement after that fence is ready; **overlapping multi-slot GPU execution is
not claimed**. Pending-slot retention is covered by deterministic production
storage/descriptor tests, not fabricated native telemetry. Residency byte metrics
count retained buffer payload/page tables, not driver allocator heap reservations.

## Controller framing follow-up

Integration review reproduced a 21–29% whole-plant frame-radius jump when a shape
slider moved by only 0.0001 from its default. A new all-species regression failed
before correction. Complete-head frames now follow their authored attachment
transform exactly; whole frames vertically transport the authored center and
refit the enclosing radius while preserving its authored relative margin. This
keeps old saves/defaults exact and the neighborhood continuous, with no new
geometry or lifetime owner. All shape/lighting/cache tests and the integrated
18-phase GPU controls sweep pass. Two lighting wiring guards were also migrated
from the deleted per-texel branch to the actual cached object-lighting producer
and both apple consumers, not removed.

## Original worker acceptance handoff

Integration into `vegi`, full Cargo suite, aggregate final controls/cache/log checks,
visual acceptance and authoritative Release performance comparisons. No maximum
simultaneous resolution/views memory stress, cross-device results or performance
budget acceptance is claimed. The old ignored browser repair fixture was not run.
The known baseline VKN retired-shader test is neither masked nor fixed here.

Generated output changed only **`src/app/generated/gui_adjustables_gen.rs`**, from
`cargo check`. `config/gui.toml` differs only by the three intended declarations;
all native runners verified it unchanged. No DDGI HISTORY or audio-owned blocks
were edited.
