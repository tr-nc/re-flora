# Contree GPU build optimization results

Branch: `agent/contree-optimizations`

Status: Phases 1-4 implemented and measured.

## Baseline

Release hidden benchmark evidence collected before the optimization branch:

- Startup contree rebuilds: 50 chunks, `contree_build_and_alloc_total` avg about `828µs`.
- Non-empty startup chunks: 25 chunks, avg about `936µs`.
- Empty startup chunks: 25 chunks, avg about `720µs`.
- 8-chunk tree rebuild: total mesh rebuild about `30.17ms`; surface about `23.85ms`; contree about `5.03ms`; scene texture about `1.20ms`.

## Final result

Final committed Phase 4 validation log:

- `target/re-flora-logs/re-flora-20260521-160303.041-54290.log`

| Metric | Before | After | Change |
| --- | ---: | ---: | ---: |
| 8-chunk rebuild total | `~30.17ms` | `26.36ms` | `~13%` faster |
| 8-chunk contree main-thread time | `~5.03ms` | `1.57ms` | `~69%` faster |
| Non-empty startup contree build avg | `~936µs` | `615.564µs` | `~34%` faster |
| Empty startup contree build avg | `~720µs` | `861ns` skip cost | effectively removed |
| Contree `leaf_write` GPU pass | `~321µs` | `90.73µs` | `~72%` faster |
| Timed contree GPU passes | `~391µs` | `174.821µs` | `~55%` faster |

After these changes, contree is no longer the dominant cost in the tested 8-chunk tree rebuild. Surface generation is now the dominant part: `23.41ms` of the final `26.36ms` rebuild.

## What changed

### Phase 1: skip empty contree builds

If `SurfaceBuildResult::active_voxel_len == 0`, the rebuild now avoids the GPU contree build and only clears stale scene/chunk state.

Preserved behavior:

- previous contree allocation cleanup;
- CPU chunk cache and shared ray-query cache invalidation;
- scene chunk metadata clearing;
- CPU source update with `is_present=false`;
- scene texture entry clearing.

Measured result: empty chunks dropped from about `720µs` each to sub-`2µs` skip work.

### Phase 2: GPU timestamp instrumentation

Added optional per-pass timestamp timing for contree command buffers. This made shader changes evidence-backed instead of guess-based.

Measured hotspot before the sparse path: `leaf_write` dominated contree GPU time at about `321µs`.

### Phase 3: pipeline direct contree waits

Multi-chunk direct rebuilds now submit contree work and wait for the previous chunk while the next surface build is running. Scene texture updates are deferred to the end of the batch.

This reduced visible 8-chunk rebuild contree main-thread time before Phase 4 from about `5.8ms` to about `2.2ms`.

### Phase 4: build contree from active surface bricks

`make_surface.comp` now records the unique set of active `4x4x4` surface bricks:

- `surface_active_brick_flags`
- `surface_active_brick_indices`
- `active_brick_len`

`leaf_write.comp` now dispatches over that compact active-brick list instead of scanning every leaf brick in a `256^3` chunk.

Measured result: `leaf_write` fell from about `321µs` to `90.73µs`.

## Reverted experiment

A follow-up build-serial / mark-buffer attempt to avoid clearing stale sparse leaf nodes was tested and then reverted because it did not improve measured startup contree time.

## Validation commands

Used after implementation phases:

```bash
cargo fmt --check
cargo check
cargo test
source ~/.zshrc
cargo run --release -- --hidden --auto-exit 0.5
```

Performance comparisons use release hidden runs and log-derived counters. Debug builds and unit tests are not performance evidence.
