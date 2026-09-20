# Digging regression: native tree rebuild stalls

## Status

The user rejected the lighting candidate for severe digging stalls and intermittent black patches.
Performance is the acceptance priority; lower flicker alone is not acceptance. This change fixes one
reproduced native-world stall, **not** the un-reproduced black patches or all digging performance.

Preserved and pushed annotated tags, with remote peeled SHAs verified:
- `checkpoint/terrain-edit-lighting-start-ad7e4bd6` → `ad7e4bd6d8e5aa20b112e23d177f7f8e9caf8869` (original pre-lighting checkpoint).
- `checkpoint/ddgi-before-history-candidate-06d8257b` → `06d8257ba7fa02ac97ef2c1dd4cbb21fcfacdf95` (before the latest history experiments).

No reset, release, visible launch, or subagent was used. Neither DDGI experiment was enabled by this fix.

## User evidence and reproduction

Preserved the actual run log in `target/dig-regression/user-session.log` (original
`re-flora-20260920-012156.434-685667.log`). It shows 3840×2160 on RTX 3060 Ti, 32-voxel probe spacing,
all 179 field-latch messages with both history experiments **false**, and runtime raster-tree mode B.
Many digging publications trigger ~75–78 ms tree compilation with unchanged mesh counts.
The saved GUI had mode B off: simply copying saved settings did **not** reproduce the live session.
Do not assume which build profile the user launched from these logs alone.

The old sustained-lighting fixture comparison did not reproduce this stall. That negative result
and its logs are retained in `target/dig-regression/comparison.json`; it is not acceptance evidence.

A red-capable native-world replay uses the real brush API via `--water-edit-soak`, explicitly enables
raster trees, leaves normal vegetation/rendering present, and uses no screenshots or RFIRR capture:

```sh
cargo build --release
python3 scripts/check_tree_terrain_edit_perf.py target/tree-edit-perf-01
```

Run from the tested worktree; output must be fresh. The script saves binary/source/config provenance,
locks GPU testing, restores exact GUI/camera bytes, and exits 1 on unwanted tree rebuilds. It tests
three edits, not a continuous 150-stroke user cave. Timing uses logged CPU frames from the first edit
through one second after the last, excluding the initial tree compile. GPU scope samples alone missed
the CPU stall. No hardware-specific timing threshold is hidden in the deterministic rebuild gate.

## Root cause and fix

`sync_static_raster_trees` used global visible-terrain revision as the sole cache validity criterion.
Every unrelated edit therefore waited for the device, read back the tree atlas region, regenerated
mesh/skin/attachments/BVH data, and uploaded it again. Compilation timing excludes the preceding
`wait_idle`; merely optimizing a DDGI shader would not remove this work.

The compiled tree cache now owns:
- the last validated terrain revision;
- the canonical tree-registry revision (placement/replacement/removal, including empty-shell transactions);
- exact atlas read bounds, using the same padded/clamped bounds as the mesher;
- sticky invalidation until a complete replacement upload succeeds.

Visible terrain publication forwards its actual affected bounds. Disjoint edits advance validity
without changing geometry. Intersections, meshing-halo changes, or canonical tree changes still rebuild.
Invalidated surfaces retain their old identity until replacement, preserving physics publication
semantics. Wind-only pose changes do not invalidate rest geometry. This is not a general exemption
for terrain edits, a guessed radius, or a change to DDGI lighting/history policy.

## Release measurements

Same machine, native hidden/muted world, raster trees enabled, three actual brush edits:

| version | during-edit tree rebuilds | maximum logged edit frame |
|---|---:|---:|
| pre-candidate `06d8257b`, first native run | 3 | 96.72 ms |
| current candidate `1c0af07e`, before fix | 3 | 102.18 ms |
| pre-candidate, durable regression script | 3 | 95.49 ms |
| fixed, durable regression script | 0 | 20.33 ms |
| fixed, repeat | 0 | 19.86 ms |

The initial tree compile is retained. A pre-fix raster-off control reached 20.44 ms in the edit window.
Thus rolling back **only** the recent history candidates does not remove this stall. The original
`ad7e4bd6` checkpoint was tagged but not benchmarked here; no performance claim is made for it.
The fix is independent of the lighting candidates and can be retained if lighting is later rolled back.

Evidence: `target/dig-regression/{native-comparison.json,after/,confirmed-previous/,confirmed-current/,current-repeat/}`.
Pre-commit source patch/new cache source are preserved in `after/`; the earlier commit ID in those
reports denotes the base, not a claim of a clean build. The durable script went red on the previous
worktree and green twice on the fix. Initial startup, actual tree-changing edits, and other stall sources
remain separate performance concerns.

## Validation and remaining issue

- `cargo fmt --check`, `cargo check`, `cargo test`: 1041 main + 4 library passed, 2 ignored.
- Six new Rust guards cover spatial/halo invalidation, sticky dirtiness, publication identity, and
  canonical record changes; four Python analyzer tests cover missing evidence and midnight windows.
- Hidden/muted release tree smoke with resize passed: A/B switching, stiffness sweep, wind poses,
  17 matched edit rays, actual removal of 3 wood voxels, age rebuild, removal, and replacement.
- `cargo run --release -- --hidden --mute --auto-exit 0.5` and canonical log-tail inspection passed.
- No generated files changed; GUI/camera were restored, including the comparison worktree.

**Black patches are open.** No matching saved cave or black-patch frame is available, and this replay
is not a black-patch test. Do not claim that avoiding tree rebuilds fixes the patches, nor restore a
fixed-color fallback without agreement. Next evidence needed is a screenshot plus preferably the
saved terrain/camera at the failing location, so old/new lighting can be compared on that exact case.
