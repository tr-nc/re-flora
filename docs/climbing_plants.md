# Playable climbing vine slice

Implemented on `grepping-plants`, from `80146005`. Session-persistent skeleton;
**plant history is not saved**. Terrain snapshots can save the authored wall, but
loading/replacing terrain clears vines and their render instances. Recreate the
vine explicitly after loading. Existing trees/grass retain their own ownership.

## Try it

1. From this worktree, run `cargo run` when ready for a visible session.
2. Press **R** for Debug. Expand **Climbing Plants**: saved settings, actions and
   live status now share this section. Click **Create vine wall and focus**. This
   modifies real terrain at voxel bounds `(224,192,300)..(288,300,306)` and focuses
   an orbit-edit camera. Do not create/reset over terrain you want to preserve.
   Subsequent **Reset wall and vine...** requires confirmation.
3. Enable attachment markers (yellow attached, red released). Watch stems extend,
   adhere, branch and produce blocky leaves. **Pause vine growth** holds extension
   without changing the saved speed or stopping gravity. **Peel highest attachment**
   releases one wall bond without digging; **Peel all attachments** releases all
   wall bonds, but neither action disconnects the root.
4. Select **3 / Dig**, point at the wall and hold LMB briefly. **Shift + wheel**
   adjusts brush radius. Remove one marked support, then several upper supports.
   Other supports remain; released spans settle rather than regenerating.
5. Edit away from the vine to compare. Refilling does not resurrect old attachments.
   **Focus vine** follows the current skeleton bounds, including after a fall.
6. Click **Disconnect root**. Attached parts remain supported, but extension stops.
   Then **Peel all attachments** (or dig away their supports) to let the entire
   skeleton settle. Reset restores a connected root.
7. For refill testing, let the vine grow, expand **Collision recovery test**, and
   click **Refill terrain through tip**. This inserts real limestone through the tip.
   Dig away upper wall supports afterward; the vine recovers/settles without
   regenerating. The small refill is real terrain and can also be dug away.

Saved declarative controls: enable, pause growth, growth quanta/sec, adhesion spacing
in voxels, attachment markers. Click Debug **Save** to persist settings. Speed zero pauses
extension, not gravity. Disable hides/pauses the vine without deleting its session
history or removing the wall. Enabling for the first time also creates/focuses it.

## CPU profiling

Measured results, remaining export spikes and physics research are recorded in
[research/climbing_vine_tuning.md](research/climbing_vine_tuning.md).

Use the real release app, with the existing deterministic edit/recovery review:

```sh
RE_FLORA_CLIMBING_REVIEW=1 cargo run --release -- --hidden --mute --perf --auto-exit 16
cargo run --release -- --latest-log
```

`[CLIMBING][PERF]` reports export, revalidation, growth, relaxation, render preparation,
voxel query count, nodes, phase and review tick. These opt-in CPU scopes exclude fixture
edits/camera actions and are not GPU/frame-time measurements. Compare matching phases
and ticks (through tick 280), not whole-run averages: faster runs advance farther.
Require both `[CLIMBING][REVIEW] verified ...` and `root_cut=true ...`, plus clean shutdown.

## Model and bounds

- One deterministic seed (`42`), append-only node/anchor IDs, parent edges and fixed
  rest lengths; at most 512 nodes and four independent tips. No random regeneration
  on terrain edits. Branches and leaves follow the stored skeleton.
- 256 terrain voxels/world unit. Each extension is 2 voxels. Default attachment
  spacing is 16 voxels, subsequent spacing varies by ±15% plus one extension's
  quantization. This is deliberately tuned to the small fixture, **not** a claim
  that research's 20–40 cm spacing was implemented literally.
- A contact is one fixed support voxel with matching seed material (limestone in
  the fixture), plus its exposed stem footprint. Burying that footprint releases
  the adhesion without rebinding it elsewhere. Proximity/swept collision and sparse adhesion are separate.
  Flat vertical surface following only: stop at missing support, tops, corners,
  or unavailable data. No snapping to a replacement surface through a wall.
- Published terrain bounds select contacts via a chunk index and precise cell
  checks, including footprint cells in neighbouring chunks. Dependency polling catches later decoded-cache publication. Complete
  export dependencies include clearance/search halos and chunk crossings.
  Exact support/material revalidation, not revision change, releases adhesion.
- Queries/commits are synchronous against immutable Contree exports, with current
  dependency **and readiness** checks. A single cached export uses a 16-voxel envelope;
  it is reused only when the complete query bounds are covered and every dependency
  is still current and ready (including known-empty chunks). Reset/load drops it.
  Pending exports freeze simulation; they
  never mean air. Existing active spans use the same current shell data.
- Bounded quasi-static gravity and distance/contact projection. Root connectivity
  is separate from wall adhesion: disconnection removes the root restraint and
  stops all tips; it preserves skeleton and external attachments. Rest lengths
  never change. Movable spans separated by surviving pins solve independently.
  A full distance/contact sweep that leaves every movable position exactly unchanged
  ends iteration early; the 128/512 caps and all final collision/length checks remain.
  This is fixed-point termination, not relaxed tolerances or a new dynamics model.
  Normal playback advances these settling quanta at a fixed 20 Hz in world simulation
  time, preserving the original default 50-ms behavior when World Tick Time is changed.
  Growth has its own clock; pausing growth does not pause settling. Catch-up is bounded
  to eight quanta without accumulating an unbounded debt. The deterministic review
  deliberately retains one growth/settling quantum per ready frame for matched workloads.
  Tests compare exact state against the full-budget solver during growth, release,
  detachment and refill recovery.
- Newly inserted terrain gets bounded outward recovery using the vine's known
  exterior direction, not arbitrary "air" behind a sparse shell. Recovery exits
  pre-existing particle overlaps outward before tangential correction; other
  boxes still block swept movement. Every committed span must pass whole-edge
  collision and 0.002-voxel length-error checks. Buried anchors are released,
  never moved. Unknown/stale data cannot commit recovery.
- Independent resident cuboid mesh and capacity-managed instance buffer; stems and
  leaves use existing lit opaque/shadow pipelines with nonuniform transforms and
  inverse-scale normals. Unchanged instances are not uploaded. No preview/tree
  ownership, per-frame mesh creation, or extra terrain revision authority.

## Validation after review fixes (macOS / Vulkan)

Two red regressions were reproduced before fixing: `unsupported root remains
pinned` and `refill overlap never recovered`. Transcripts are under
`target/climbing-validation/finalize/`, together with final check/test/run logs.

```sh
cd /Users/bytedance/.herdr/worktrees/re-flora/grepping-plants && cargo fmt --check
cd /Users/bytedance/.herdr/worktrees/re-flora/grepping-plants && cargo check
cd /Users/bytedance/.herdr/worktrees/re-flora/grepping-plants && PATH=/opt/homebrew/bin:$PATH cargo test
cd /Users/bytedance/.herdr/worktrees/re-flora/grepping-plants && cargo run --release -- --hidden --mute --auto-exit 0.5
cd /Users/bytedance/.herdr/worktrees/re-flora/grepping-plants && cargo run --release -- --tail-latest-log 30
cd /Users/bytedance/.herdr/worktrees/re-flora/grepping-plants && RE_FLORA_CLIMBING_REVIEW=1 cargo run --release -- --hidden --mute --screenshot player-default target/climbing-fixed --screenshot-delay 1 --screenshot-sequence 4 2 --auto-exit 12
cd /Users/bytedance/.herdr/worktrees/re-flora/grepping-plants && cargo run --release -- --tail-latest-log 200
cd /Users/bytedance/.herdr/worktrees/re-flora/grepping-plants && git diff --check
```

- Formatting/check/diff check pass. Tests: **1078 + 4 passed, 2 ignored**, including
  17 climbing tests. New guardrails cover cut-root growth inhibition with preserved
  support/IDs, complete detachment and floor collision, deterministic refill then
  support removal, a trapped span not freezing another span, buried adhesion,
  cross-chunk exposure indexing, and pending/stale recovery.
- Toolchain prerequisite retained: `/usr/bin/python3` lacks `tomllib`; the initial
  implementation's plain `cargo test` failed the existing lighting analyzer
  subprocess. Installed Homebrew Python 3.14.7 resolves it. Final full tests used
  the explicit PATH above; no repository workaround or other validation blocker.
- Review mode runs fixed quanta on ready frames and executes real world edits.
  Final sequence: growth to 66 nodes, attachments **7→6**; actual refill at
  `(234,297,306)..(237,302,308)`, confirmed overlapping current terrain; upper-wall
  removal **13→2**, explicitly retaining the refill. Then:
  `stable_ids=true unaffected_supports=true collision_clear=true refill_retained=true finite=true`,
  126 nodes, max length error **0.001128 voxels**, max motion **9.6069 voxels**.
  Cutting the root and removing remaining supports produced **2→0** attachments,
  stopped growth and **2.3969 voxels** of root drop. No scripted anchor deletion.
- Final smoke: `target/re-flora-logs/re-flora-20260922-134836.377-62847.log`.
  Final review: `target/re-flora-logs/re-flora-20260922-134842.174-64934.log`.
  Built-in log helpers inspected both. Successful exits, `failures=0`, no
  ERROR/panic/VUID. A 15.805 ms fruit-physics hitch was logged at fixture creation;
  this warning also existed before integration. No performance acceptance claimed.
- Four local 5120×2880 captures: `target/climbing-fixed.000000.png` through
  `.000003.png`; final image inspected. They cover growth, refill/support removal,
  settling, then root disconnection/complete detachment. Earlier limestone captures
  remain under `target/climbing-limestone.*.png`. Artifacts are not committed assets.

## Deliberate limitations / remaining review

Not arbitrary click-to-plant: the discoverable Debug entry is the first playable
scope. No moving supports, general corner traversal, arbitrary stem cuts, physical
fracture, wilting, mature reattachment, multiple plants, or saved plant history.
Root disconnection affects the single connected skeleton; it is not an arbitrary
edge-cutting system. Leaves are visual followers, not colliders. No new dynamic-vine DDGI/local-light integration claimed.

Recovery is bounded to six voxels per candidate with finite projection iterations.
A genuinely impossible span (e.g. a perfectly taut span pinned through newly solid
terrain), deeply buried geometry, or a blocked outward exit remains unchanged,
without freezing independently movable spans. Cutting the root/removing the pins
can make recovery feasible. CPU data is a surface shell, not an interior-air oracle. Manual brush interaction,
real snapshot-load lifecycle and broader terrain shapes still need user review
(their wiring/pure policies are covered, not all end-to-end scenarios).
No visible app was launched. Screenshots show a candidate, not user-approved visual
quality. No performance optimization or performance acceptance performed.

## Files and commits

- `3efb63c7`: core `src/climbing_plants.rs`, registered in `src/main.rs`.
- `b5d0bd4f`: resident block rendering in `src/tracer/{mod.rs,dynamic_fruit_resources.rs}`
  and shared `shader/slang/dynamic_fruit{,_shadow}.vert.slang` transforms.
- `84064973`: App runtime `src/app/core/climbing_plants.rs`, Debug/world-clock wiring
  in `src/app/core/mod.rs`, publication/reset observer in `visible_terrain.rs`,
  declarative `config/gui.toml`, core guardrails and renderer upload guard.
- `ebb3bd86`: limestone fixture (avoids automatic stucco grass), collision verification,
  accurate attachment-marker label.
- `07a37d64`: root connectivity, explicit Debug disconnection action, pure and
  hidden-runtime detachment checks (`src/climbing_plants.rs`, App runtime/UI).
- `70d603b4`: per-span distance/contact solver, bounded refill recovery, exposed
  contact indexing, Debug refill action and real refill/removal review sequence
  (same three Rust files). No shaders/config/generated changes in either fix.
- This note is updated separately after validation.

Only tracked generated change: `src/app/generated/gui_adjustables_gen.rs`, produced
by `cargo check` from GUI declarations; never hand-edited. No incidental app-save
config diffs, main/other-worker changes, push, release, or nested delegation.
