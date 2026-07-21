# Terrain Denoiser Optimization History

All measurements use the release renderer through `scripts/denoiser_bench.py`, a hidden native
window, the `player-default` camera, 90 warmup frames, and 64 captured frames. The physical render
extent on the benchmark machine is 2560x1440. Lower is better for every metric.

## Version history

| Version | Commit / artifact | Mean luma delta | Mean p95 | Mean p99 | Noticeable ratio | Max transition mean | Decision |
| --- | --- | ---: | ---: | ---: | ---: | ---: | --- |
| Main baseline | `a0ce1dc7`, `baseline.toml` | 0.093050 | 1.000000 | 2.000000 | 0.000461 | 0.184364 | Baseline |
| Valid history, alpha 0.10 | `candidate-history.toml` | 0.035231 | 0.000000 | 1.000000 | 0.000111 | 0.101518 | Superseded by lower alpha |
| Valid history, alpha 0.10 repeat | `candidate-history-repeat.toml` | 0.035384 | 0.000000 | 1.000000 | 0.000117 | 0.127118 | Confirms stable mean/tail ratio |
| Valid history, alpha 0.06 | `7677e9d1`, `candidate-alpha-006.toml` | 0.021984 | 0.000000 | 1.000000 | 0.000091 | 0.091737 | Kept |
| Fixed policy, one GUI control | `gui-history.toml` | 0.020351 | 0.000000 | 1.000000 | 0.000073 | 0.091413 | Kept; equivalent output |
| Packed hit marker | `packed-hit-history.toml` | 0.020432 | 0.000000 | 1.000000 | 0.000075 | 0.091324 | Kept; removes invalid ninth storage image |

## Changes and conclusions

### `a0ce1dc7`: measurable baseline

Added continuous presented-frame readback and adjacent-frame luma metrics. This changed no denoiser
behavior. It established that sparse flashes were hidden by the small global mean, so p95/p99 and the
ratio of pixels changing by at least 8/255 are required acceptance metrics.

### `7677e9d1`: deterministic, surface-valid history

- Moved history length from an in-place `R8` image into the W component of the copied position
  history. This removes cross-workgroup read/write races without adding a storage image.
- Rejected out-of-bounds reprojection taps before texture access.
- Required the current and previous voxel IDs to match before accepting history.
- Reduced temporal alpha from 0.28 to 0.06 after measuring 0.10 and 0.06.

Compared with main, the kept version reduces mean luma delta by 76.37%, noticeable-pixel ratio by
80.18%, and maximum transition mean by 50.24%. Exact voxel validation bounds the lower alpha's ghost
risk by discarding history immediately when reprojection lands on a different surface voxel.

### Fixed policy: one denoiser control

The validated position, spatial, and iteration settings are now named constants in Rust instead of
GUI inputs. `Temporal Responsiveness` is the only remaining denoiser control, with a focused
0.02-0.30 range and a default of 0.06. The fixed policy is position similarity 0.8, color phi 0.75,
normal power 20, position phi 0.05, depth falloff 0.0-0.5, stable-history fraction 0.05, changing
luminance phi enabled, spatial denoising enabled, and three A-Trous iterations.

This removes ten stale tuning controls while preserving the shader resource layout. Against the
history control, mean delta improved 9.38% and noticeable ratio improved 27.09%; fresh-sample mean
changed by -0.01%. Those small differences are normal stochastic run variation, so this is treated
as behavior-equivalent parameter consolidation rather than a new quality claim.

### Packed hit marker: eight tracer storage images

The tracer now writes its hit/miss marker into the position texture's W component. Temporal resolve
consumes that marker and replaces it with history length; spatial resolve then treats a nonzero
history length as a hit. This removes the standalone full-resolution `R8` hit texture and reduces
the tracer compute stage from nine storage images to the device limit of eight.

The release hidden logs for both benchmark modes contain no validation errors after this change.
History mean moved +0.40% and noticeable ratio +1.46%, while fresh mean moved -0.88%; these changes
are within the observed stochastic spread, and p95/p99 are unchanged.

## Fresh-sample guard

`--fresh-samples` forces temporal reset every frame while retaining the spatial filter. Its first
committed measurement is the guard baseline for subsequent iterations; every later optimization must
report both normal-history and fresh-sample results here.

| Guard mode | Artifact | Mean luma delta | Mean p95 | Mean p99 | Noticeable ratio | Max transition mean |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| History control | `guard-history.toml` | 0.022459 | 0.000000 | 1.000000 | 0.000101 | 0.108173 |
| Fresh baseline | `fresh-baseline.toml` | 0.128273 | 1.000000 | 2.000000 | 0.001667 | 0.201563 |
| Fixed-policy history | `gui-history.toml` | 0.020351 | 0.000000 | 1.000000 | 0.000073 | 0.091413 |
| Fixed-policy fresh | `gui-fresh.toml` | 0.128261 | 1.000000 | 2.000000 | 0.001666 | 0.200251 |
| Packed-hit history | `packed-hit-history.toml` | 0.020432 | 0.000000 | 1.000000 | 0.000075 | 0.091324 |
| Packed-hit fresh | `packed-hit-fresh.toml` | 0.127133 | 1.000000 | 2.000000 | 0.001656 | 0.147243 |

The history control remains within the measured run-to-run spread of `7677e9d1`, so the reset push
constant and report-mode plumbing do not alter normal denoising. The fresh row becomes the
spatial-only regression baseline for the next optimization.

## Spatial-detail guard

Starting with `detail-guard-history.toml` and `detail-guard-fresh.toml`, reports also measure the
mean horizontal/vertical luma gradient of the frame averaged across all 64 samples. This largely
averages away sample noise before measuring stable structure. Higher is sharper: the packed-hit
baseline is 1.928250 in history mode and 1.882909 in fresh mode. Spatial-filter changes must report
this metric alongside flicker; a lower temporal delta alone is not sufficient if it erases stable
detail.
