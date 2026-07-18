# Surface-construction hot-path optimization

This report records measured experiments on the sparse surface extraction path. Release-mode RTX 3060 Ti runs are authoritative; all A/B comparisons use `surface-rebuild` in `A,B,B,A` order.

## Existing timing attribution

The surface builder already timestamps these GPU passes independently:

- surface image clear;
- active-brick flag clear;
- sparse dispatch preparation;
- sparse surface extraction;
- optional flora dispatch preparation;
- optional active-surface-to-flora conversion.

`[PERF][SURFACE_BUILD_PASS_TIMING]` exposes each pass and `[PERF][GPU_JOB_SCOPE]` exposes the full `surface.build` job. The benchmark suite also rejects construction comparisons unless ordered chunk signatures match for active voxels, active bricks, and solid workgroups.

## Workgroup aggregation

The retained optimization replaces global atomics per active voxel with workgroup-local aggregation:

- active voxels increment one shared counter and one global counter per workgroup;
- active voxels set one of eight shared 4×4×4 local-brick bits;
- invocation zero reserves the complete active-brick output range once and publishes each active local brick once;
- all invocations remain live through the final workgroup barrier.

The optimization is implemented in the native Slang source of truth. Active voxel, active brick, and solid workgroup signatures matched exactly across all order-reversed runs. The legacy comparison source used for these measurements was removed after the native-only transition.

### RTX 3060 Ti, native Slang workgroup aggregation

| Metric | Baseline median | Candidate median | Median delta | p95 delta |
|---|---:|---:|---:|---:|
| `surface.build` | 709.5 µs | 700.0 µs | -1.34% | -4.71% |
| `surface.make_sparse` | 516.5 µs | 510.0 µs | -1.26% | -5.19% |
| `tree.replace_deferred_total` | 13.905 ms | 13.355 ms | -3.96% | +0.38% |

Report: `target/perf/slang-surface-aggregation-ab/` (local benchmark artifact, not tracked).

## Integer normal accumulation

The smooth 5×5×5 estimator now sums integer voxel offsets into `int3` and converts once before normalization. Every component is an exact integer in a small bounded range, so this is mathematically equivalent to adding the same integer offsets as floats and leaves packed normals and appearance unchanged.

Native Slang A/B results with matching workload signatures:

| Metric | Baseline median | Candidate median | Median delta | p95 delta |
|---|---:|---:|---:|---:|
| `surface.build` | 738.0 µs | 722.0 µs | -2.17% | -6.26% |
| `surface.make_sparse` | 538.0 µs | 530.5 µs | -1.39% | -7.02% |
| `tree.replace_deferred_total` | 13.290 ms | 12.775 ms | -3.88% | -2.81% |

Report: `target/perf/slang-surface-integer-ab/` (local benchmark artifact, not tracked).

## Compact normal estimator

The retained native Slang compile-time policy changes the occupancy-weighted normal neighborhood from 5×5×5 to 3×3×3. Both estimators remain named in `surface_normal_policy.slang`, so restoring the smooth reference is a one-line policy change. Geometry, active-voxel counts, and active-brick counts do not change; only packed normals do.

### RTX 3060 Ti, compact versus smooth normals

| Metric | Baseline median | Candidate median | Median delta | p95 delta |
|---|---:|---:|---:|---:|
| `surface.build` | 732.5 µs | 634.0 µs | -13.45% | -17.31% |
| `surface.make_sparse` | 554.0 µs | 453.5 µs | -18.14% | -21.61% |
| `tree.replace_deferred_total` | 14.325 ms | 13.840 ms | -3.39% | -3.19% |

The optimized sparse SPIR-V shrank from 9,392 to 9,376 bytes. Fixed-camera screenshot RMSE was 0.00342 for a smooth same-build repeat, 0.00365 for a compact same-build repeat, and 0.00585–0.00598 across estimators. Side-by-side inspection found the intended slightly more local terrain shading, with no silhouette, geometry, depth, or material changes. Reports: `target/perf/surface-compact-normal-ab/`; local screenshots: `/tmp/surface-{smooth,compact}-{1,2}.png`.
