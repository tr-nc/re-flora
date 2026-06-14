# Terrain build performance notes

This document keeps the useful results from the contree and surface generation optimization passes. Release-mode app runs and logs are the authoritative evidence; debug builds and unit tests are not performance evidence.

## Current result

The tested 8-chunk tree rebuild improved from roughly `30.17ms` before the contree work to `13.67ms` after sparse surface generation and direct active-surface flora dispatch.

| Metric, 8-chunk tree rebuild | Baseline | After contree | After surface/flora sparse path |
| --- | ---: | ---: | ---: |
| Total rebuild | `~30.17ms` | `26.36ms` | `13.67ms` |
| Surface total | `~23.85ms` | `23.41ms` | `10.16ms` |
| Contree total/main-thread time | `~5.03ms` | `1.57ms` | `1.63ms` |
| Scene texture total | `~1.20ms` | `1.27ms` | `1.76ms` |

Representative final log:

```text
target/verdarium-logs/verdarium-20260521-174411.058-38643.log
```

## Contree changes kept

- Empty chunks skip GPU contree builds and only clear stale scene/chunk state.
- Contree pass timing was added so shader changes could be measured.
- Multi-chunk rebuilds submit contree work and wait for the previous chunk while the next surface build is running.
- `make_surface` records active `4x4x4` surface bricks; `leaf_write` dispatches over that compact active-brick list instead of scanning every leaf brick in a `256^3` chunk.

Important measured wins:

| Metric | Before | After | Change |
| --- | ---: | ---: | ---: |
| 8-chunk contree main-thread time | `~5.03ms` | `1.57ms` | `~69%` faster |
| Non-empty startup contree build avg | `~936µs` | `615.564µs` | `~34%` faster |
| Empty startup contree build avg | `~720µs` | `861ns` skip cost | effectively removed |
| Contree `leaf_write` GPU pass | `~321µs` | `90.73µs` | `~72%` faster |

A build-serial / mark-buffer experiment to avoid clearing stale sparse leaf nodes was reverted because it did not improve measured startup contree time.

## Surface/flora changes kept

The surface builder now avoids the two old full-volume bottlenecks for normal non-preserve-flora rebuilds:

1. It builds surface data only for compacted solid `8^3` workgroups.
2. It builds normal flora directly from active surface bricks in the same surface command buffer.

Current algorithm outline:

```text
prepare_sparse_surface_dispatch
  -> compact solid 8^3 workgroups
  -> write indirect dispatch

make_surface_sparse
  -> map dispatch id through compact list
  -> preload selected 8x8x8 block + halo
  -> emit packed surface data
  -> emit active surface brick metadata

prepare_active_surface_flora_dispatch
  -> derive indirect flora dispatch from active_brick_len

active_surface_to_flora_instances
  -> scan only 64 voxels per active surface brick
  -> place flora directly from packed surface data
```

The occupancy-based flora path remains for preserve-flora edits, trimming, growth, and instance-to-occupancy flows.

Important measured milestones:

| Metric, 8-chunk rebuild | After contree | After direct flora | After same-cmdbuf flora | After sparse surface |
| --- | ---: | ---: | ---: | ---: |
| Total rebuild | `26.36ms` | `17.76ms` | `15.16ms` | `13.67ms` |
| Surface total | `23.41ms` | `14.74ms` | `12.52ms` | `10.16ms` |

Sparse upper chunks dropped from full-volume `make_surface` around `0.67ms` to sparse `make_surface_sparse` around `0.012ms` to `0.014ms`.

## Remaining useful follow-ups

- Full surface batching could reduce per-chunk submit/wait serialization, but requires per-job scratch resources and safe contree consumption of those resources.
- `surface_active_brick_flags` clear is still a fixed cost on sparse chunks. A tested epoch/tag replacement regressed performance, so any new approach needs measurement.
- Solid-workgroup flags are conservative after removals. If edit-heavy workloads become important, add a more precise clear/update path.

## Validation used for these passes

```bash
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --auto-exit 0.5
```

Performance comparisons used release hidden runs and log-derived counters.
