# Playable climbing vine slice

Implemented on `grepping-plants`, from `80146005`. Session-persistent skeleton;
**plant history is not saved**. Terrain snapshots can save the authored wall, but
loading/replacing terrain clears vines and their render instances. Recreate the
vine explicitly after loading. Existing trees/grass retain their own ownership.

## Try it

1. From this worktree, run `cargo run` when ready for a visible session.
2. Press **R** for Debug. Expand **Climbing vine actions**, then click
   **Create/reset climbing vine wall and focus**. This modifies real terrain at
   voxel bounds `(224,192,300)..(288,300,306)` and focuses an orbit-edit camera.
   Do not reset over terrain you want to preserve. The wall crosses chunk boundaries.
3. Under **Climbing Plants**, optionally enable attachment markers (yellow attached,
   red released). Watch stems extend, adhere, branch, and produce blocky leaves.
4. Select **3 / Dig**, point at the wall and hold LMB briefly. **Shift + wheel**
   adjusts brush radius. Remove one marked support, then several upper supports.
   Other supports remain; released spans settle rather than regenerating.
5. Edit away from the vine to compare. Refilling does not resurrect old attachments.
   Reset/focus actions remain available in Debug.

Saved declarative controls: enable, growth quanta/sec, adhesion spacing in voxels,
attachment markers. Click Debug **Save** to persist settings. Speed zero pauses
extension, not gravity. Disable hides/pauses the vine without deleting its session
history or removing the wall. Enabling for the first time also creates/focuses it.

## Model and bounds

- One deterministic seed (`42`), append-only node/anchor IDs, parent edges and fixed
  rest lengths; at most 512 nodes and four independent tips. No random regeneration
  on terrain edits. Branches and leaves follow the stored skeleton.
- 256 terrain voxels/world unit. Each extension is 2 voxels. Default attachment
  spacing is 16 voxels, subsequent spacing varies by ±15% plus one extension's
  quantization. This is deliberately tuned to the small fixture, **not** a claim
  that research's 20–40 cm spacing was implemented literally.
- A contact is one fixed support voxel with matching seed material (limestone in
  the fixture). Proximity/swept collision and sparse adhesion are separate.
  Flat vertical surface following only: stop at missing support, tops, corners,
  or unavailable data. No snapping to a replacement surface through a wall.
- Published terrain bounds select contacts via a chunk index and precise cell
  checks. Dependency polling catches later decoded-cache publication. Complete
  export dependencies include clearance/search halos and chunk crossings.
  Exact support/material revalidation, not revision change, releases adhesion.
- Queries/commits are synchronous against immutable Contree exports, with current
  dependency **and readiness** checks. Pending exports freeze simulation; they
  never mean air. Existing active spans use the same current shell data.
- Bounded quasi-static gravity and distance projection, with fixed root and surviving
  anchors. Rest lengths never change. Candidate movement and whole final edges are
  checked against radius-expanded voxel boxes; failed collision/length solves are
  rejected atomically. This is a small flexible-chain solver, not a botany/XPBD engine.
- Independent resident cuboid mesh and capacity-managed instance buffer; stems and
  leaves use existing lit opaque/shadow pipelines with nonuniform transforms and
  inverse-scale normals. Unchanged instances are not uploaded. No preview/tree
  ownership, per-frame mesh creation, or extra terrain revision authority.

## Validation (macOS / Vulkan)

All commands ran from this worker only. Reproduce with:

```sh
cd /Users/bytedance/.herdr/worktrees/re-flora/grepping-plants && cargo fmt --check
cd /Users/bytedance/.herdr/worktrees/re-flora/grepping-plants && cargo check
cd /Users/bytedance/.herdr/worktrees/re-flora/grepping-plants && PATH=/opt/homebrew/bin:$PATH cargo test
cd /Users/bytedance/.herdr/worktrees/re-flora/grepping-plants && cargo run --release -- --hidden --mute --auto-exit 0.5
cd /Users/bytedance/.herdr/worktrees/re-flora/grepping-plants && cargo run --release -- --tail-latest-log 30
cd /Users/bytedance/.herdr/worktrees/re-flora/grepping-plants && RE_FLORA_CLIMBING_REVIEW=1 cargo run --release -- --hidden --mute --screenshot player-default target/climbing-limestone --screenshot-delay 0.5 --screenshot-sequence 4 2 --auto-exit 9
cd /Users/bytedance/.herdr/worktrees/re-flora/grepping-plants && cargo run --release -- --tail-latest-log 200
```

- Formatting/check pass. Tests: **1071 + 4 passed, 2 ignored**. Pure tests cover
  determinism, spacing, finite lengths, stable topology, local/unrelated edits,
  one/several anchor releases, material replacement, no resurrection, pending/stale
  queries, cross-chunk dependencies, thin-wall sweeps, support gaps, taut spans,
  free-tip settling, and world-replacement clearing. Existing Contree tests cover
  unfinished rebuilds even when old caches/revisions remain available.
- Plain `cargo test` initially failed the existing lighting analyzer subprocess:
  `/usr/bin/python3` lacks `tomllib` (`ModuleNotFoundError`). Using installed
  Homebrew Python 3.14.7 resolves it; no repository workaround added.
- Review mode runs one fixed growth/relaxation quantum per ready render frame;
  it does **real WorldEditTransaction edits**, never scripted anchor deletion.
  Counts: 66 nodes / seven attachments; first edit **7→6**; growth to 114 nodes /
  twelve attachments; upper-wall edit **12→2**. Post-edit verification:
  `stable_ids=true unaffected_supports=true collision_clear=true finite=true`,
  max edge-length error **0.000180 voxels**, max movement **9.6033 voxels**.
- Final smoke log: `target/re-flora-logs/re-flora-20260922-132758.157-73629.log`.
  Final review log: `target/re-flora-logs/re-flora-20260922-132807.165-76271.log`.
  Both exited successfully, `failures=0`, no ERROR/panic/VUID found. Hidden-monitor
  fallback warning remains. Review logged a 14.997 ms fruit-physics hitch during
  fixture creation; this warning also occurred in the pre-integration smoke
  (96.306 ms). These observations are **not** a performance comparison/acceptance.
- Four inspected/captured 2560×1440 images:
  `target/climbing-limestone.000000.png` through `.000003.png` (early growth through
  removed upper wall and displaced vine). Logs/check/test transcripts also copied
  under `target/climbing-validation/`. Artifacts are local, not committed assets.

## Deliberate limitations / remaining review

Not arbitrary click-to-plant: the discoverable Debug entry is the first playable
scope. No moving supports, general corner traversal, root/stem cutting, physical
fracture, wilting, mature reattachment, multiple plants, or saved plant history.
The root remains pinned even if its wall contact disappears. Leaves are visual
followers, not colliders. No new dynamic-vine DDGI/local-light integration claimed.

The conservative solver can freeze a span if collision/length constraints cannot
be satisfied; inserting solid terrain through an existing stem is not depenetrated.
CPU data is a surface shell, not an interior-air oracle. Manual brush interaction,
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
- This note is committed separately after validation.

Only tracked generated change: `src/app/generated/gui_adjustables_gen.rs`, produced
by `cargo check` from GUI declarations; never hand-edited. No incidental app-save
config diffs, main/other-worker changes, push, release, or nested delegation.
