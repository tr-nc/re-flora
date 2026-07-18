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

## Rejected compact normal estimator

A measured experiment changed the occupancy-weighted normal neighborhood from 5×5×5 to 3×3×3. It reduced `surface.make_sparse` by 18.14% and `surface.build` by 13.45%, but it also produced visibly less smooth terrain shading. The changed normals could additionally alter natural flora placement through the surface-flatness policy. The compact estimator was therefore rejected rather than retaining a visual and gameplay tradeoff for a construction-only optimization.

## Packed shared occupancy rows

The retained estimator preserves the smooth 5×5×5 neighborhood and its exact integer normal sum. Each 12-voxel X row in the workgroup halo is stored as an occupancy bit mask, while the central 8-voxel type rows use packed four-bit voxel types. For each surface voxel, 25 shared row loads and population counts reproduce the same X, Y, and Z moments as visiting all 125 voxels individually. Shared surface source data falls from 6,912 bytes to 832 bytes per workgroup.

The five-bit X-moment identity and all Y/Z population contributions were exhaustively checked for every row pattern and offset combination. Matching ordered active-voxel, active-brick, and solid-workgroup signatures passed the release benchmark comparison. Because the resulting integer sum, normalization, and Oct16 packing path are unchanged, terrain normals and flora placement retain the smooth reference behavior.

### RTX 3060 Ti, packed rows versus smooth reference

| Metric | Baseline median | Candidate median | Median delta | p95 delta |
|---|---:|---:|---:|---:|
| `surface.build` | 732.0 µs | 597.0 µs | -18.44% | -23.87% |
| `surface.make_sparse` | 540.0 µs | 411.5 µs | -23.80% | -29.16% |
| `tree.replace_deferred_total` | 12.775 ms | 12.475 ms | -2.35% | -5.47% |

The optimized sparse SPIR-V grew from 10,692 to 12,368 bytes. Fixed-camera repeat RMSE was 0.00946 for the reference and 0.00779 for packed rows; cross-build RMSE ranged from 0.00595 to 0.01070, within same-build render variation. Report: `target/perf/surface-packed-smooth-ab/`; local screenshots: `/tmp/surface-{smooth,packed}-{1,2}.png`.
