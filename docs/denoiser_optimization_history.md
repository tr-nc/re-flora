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

## Fresh-sample guard

`--fresh-samples` forces temporal reset every frame while retaining the spatial filter. Its first
committed measurement is the guard baseline for subsequent iterations; every later optimization must
report both normal-history and fresh-sample results here.

| Guard mode | Artifact | Mean luma delta | Mean p95 | Mean p99 | Noticeable ratio | Max transition mean |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| History control | `guard-history.toml` | 0.022459 | 0.000000 | 1.000000 | 0.000101 | 0.108173 |
| Fresh baseline | `fresh-baseline.toml` | 0.128273 | 1.000000 | 2.000000 | 0.001667 | 0.201563 |

The history control remains within the measured run-to-run spread of `7677e9d1`, so the reset push
constant and report-mode plumbing do not alter normal denoising. The fresh row becomes the
spatial-only regression baseline for the next optimization.
