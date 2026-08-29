# Glass voxel software-RT validation

## Status

The staged experimental implementation is complete through Phase 4 and includes the
distance-invariant secondary-visibility stabilization through commit
`8c5f8c96`. Correctness, isolation, Vulkan validation, fallback, bounded-work, targeted
visual regression, and feature-OFF gates pass. The original 25% coverage planning target
against feature OFF still does not pass, so this remains an isolated experiment and is not
ship-ready.

The protected architecture and implementation guide remains the source of truth:
`docs/glass_voxel_software_rt_implementation_guide.html`. This report records measured
results; it does not replace or modify that guide.

## Implemented boundary

- Rasterization remains the opaque/raster producer. Glass uses compute/software voxel ray
  tracing only. No Vulkan/DXR hardware ray tracing, BLAS/TLAS, acceleration-structure
  descriptors, or ray queries were introduced.
- `VOXEL_TYPE_SAND` (ID 3) is reinterpreted only under
  `--glass-voxel-test-scene`. The scene is deterministic, persistence is fail-closed, and
  experimental soil bits are canonicalized. No voxel ID, save schema, or stats schema was
  added.
- Feature OFF keeps the ordinary Sand material and its soil, smoothing, footsteps, backpack,
  acoustics, material, and persistence consumers on their original path. Standard and Glass
  primary tracers are separate compile-time shader variants, so Glass output bindings are
  absent from the normal tracer pipeline.
- A shared semantic material policy feeds dense transition DDA, camera-inside handling,
  connected-medium interfaces, Fresnel/Snell/TIR/Beer transport, deterministic top-K paths,
  and finite query/event budgets. CPU double-precision references and GPU captures exercise
  slab, seam, tie, air-gap, opaque-contact, TIR, and absorption cases.
- Hybrid composition preserves first opaque HDR/depth/provenance independently from
  `GlassFront`, validates screen-space reuse, and falls back to voxel/sky radiance. Foreground
  raster objects remain in front of Glass.
- DDGI semantic visibility treats Glass as non-blocking while relocation remains solid.
  Probe transport is straight-through with Fresnel/Beer; local-light finite segments accumulate
  RGB transmittance. Direct sun intentionally skips Glass, with no Glass shadow or caustics.
  Optical revision is part of the immutable DDGI transport snapshot.

## Fixed-scene acceptance

All rows were captured from the clean final binary at 800x500 internal resolution. The final
four coverage workloads reported zero non-finite pixels and zero exhausted pixels.

| Target | Measured | Glass pixels | Foreground | Screen hits | Raster hits | Fallback | Query-budget fallback | DDA median/p95/max | Interfaces max | Active paths max |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 0% | 0.000% | 0 | 0 | 0 | 0 | 0 | 0 | 0/0/0 | 0 | 0 |
| 10% | 8.101% | 32,403 | 6,058 | 24,627 | 17,843 | 1,718 | 0 | 752/883/1,555 | 5 | 3 |
| 25% | 26.073% | 104,294 | 13,273 | 62,982 | 28,461 | 28,039 | 83 | 760/1,247/1,658 | 6 | 3 |
| 50% | 50.478% | 201,913 | 13,273 | 130,945 | 52,156 | 57,695 | 3,023 | 778/1,223/2,313 | 8 | 4 |

At 25%, the exact GPU/CPU transport capture matched: event sequence
`air-glass, glass-air, air-opaque`, terminal opaque, two interfaces, three scene queries, 220
DDA steps, and RGB transmittance `(0.899020, 0.914499, 0.919276)`. Semantic occupancy captured
Glass as non-blocking and Rock as occupied at material revision 1.

## Release performance

Environment: NVIDIA GeForce RTX 3060 Ti, 1600x1000 physical window, 800x500 internal render,
same fixed camera and DDGI-ready scene, 20-second release runs after frame-240 warm-up. Each
coverage uses order-reversed A,B,B,A; raw samples from both A runs and both B runs are pooled.
Times below are B-minus-A deltas in milliseconds (median / p95).

| Measured coverage | `frame.render` | `tracer.pass` | `glass.resolve` | Result |
|---:|---:|---:|---:|---|
| 8.101% | +3.215 / +1.650 | +0.519 / +0.525 | +1.007 / +1.046 | measured |
| 26.073% | +6.255 / +4.505 | +1.724 / +1.736 | +2.330 / +2.380 | planning target missed |
| 50.478% | +9.340 / +8.345 | +3.237 / +3.249 | +4.167 / +4.200 | measured stress case |

The guide labels +1.0 ms median / +1.5 ms p95 at 25% as a first planning target, not a ship
budget. The isolated `glass.resolve` scope alone exceeds both values, and total frame overhead
also exceeds them. This is the remaining release blocker.

## Secondary-visibility stabilization

Three user-authored camera snapshots exposed failures outside the original fixed camera:

- `t1`: at long distance, only 544 of 7,509 visible Glass pixels reused valid opaque
  radiance; 6,965 pixels fell back, making the pane appear gray or black. A deterministic
  projected-segment depth walk raises valid screen hits to 5,609 and reduces fallback to
  1,900. The pane retains refracted scene color instead of collapsing to the simplified
  voxel fallback.
- `t2`: the previous `1e-5` relative GPU DDA tie band merged distinct grid-plane crossings.
  In a fixed interior `glass-front` ROI, 2,480 output pixels reported a non-primary axis and
  the wrong event was amplified into one-voxel color blocks. ULP-bounded tie classification
  leaves 60 edge/corner output pixels (15 internal samples) and removes the visible outliers.
  Query-budget fallback also falls from 740 pixels to zero in this view.
- `t3`: the upper half of the amber raster sentinel lay on a voxel-DDA miss path, so resolving
  only the final opaque voxel could never see it. Applying the same projected-segment query to
  both opaque and miss terminals restores 14,062 amber pixels in the fixed upper ROI; raster
  screen hits rise from 3,309 to 7,729 and the bar is continuous.

The screen walk is deterministic, bounded to 256 projected pixels, and runs once per cached
visible Glass voxel. It visits the projected segment in path order, accepts raster geometry
at the first depth-interval overlap, and permits a voxel terminal only in the final interval.
There is no jitter or temporal random choice. The one-normal/one-final-color-per-voxel
invariant remains intact.

Two independent Release capture rounds produced identical T1, T2, and T3 counters and image
checks. Every capture reported zero path exhaustion and zero non-finite pixels. Evidence is
under `target/glass-t123-final-regression/`.

The incremental Release A,B,B,A comparison isolates the projected-segment change at
`dce8bb6b` from the ULP DDA baseline at `eac9b447`. Each pooled side has 59 post-warm-up
samples on the RTX 3060 Ti fixed 25% Glass workload:

| Metric | Baseline median/p95 | Candidate median/p95 | Delta median/p95 | Gate |
|---|---:|---:|---:|---:|
| `frame.render` | 18.396 / 18.601 ms | 18.050 / 18.339 ms | -1.88% / -1.41% | pass |
| `tracer.render` | 9.479 / 9.603 ms | 9.352 / 9.480 ms | -1.34% / -1.27% | pass |
| `tracer.pass` | 3.633 / 3.668 ms | 3.636 / 3.673 ms | +0.08% / +0.14% | pass |
| `glass.resolve` | 4.643 / 4.746 ms | 4.520 / 4.634 ms | -2.65% / -2.36% | pass |

All four existing 5% incremental gates pass. The comparison is recorded at
`target/perf-screen-trace-ab/reports/comparison.json`. This does not supersede the overall
feature-OFF planning-budget failure above.

Feature OFF was separately compared against pre-Glass commit
`d820d3d7dbe10cb6cfb61438b0bbfd4381d59956` with the standard `render-steady` A,B,B,A gate.
All 11 configured metrics passed. Representative pooled results:

| Metric | Baseline median/p95 | Candidate median/p95 | Delta median/p95 | Budget |
|---|---:|---:|---:|---:|
| `frame.render` | 1.623 / 2.031 ms | 1.625 / 2.039 ms | +0.12% / +0.35% | 2% |
| `tracer.render` | 0.472 / 0.476 ms | 0.475 / 0.480 ms | +0.64% / +0.84% | 2% |
| `render.trace_record` | 0.241 / 0.267 ms | 0.243 / 0.268 ms | +0.83% / +0.37% | 3% |

## Distance-invariant depth validation

The `r1` and `r2` camera snapshots exposed a fixed-NDC-depth tolerance that changed physical
meaning with camera distance. Commit `8c5f8c96` separates the two required operations:

- front/behind ordering now uses strict four-ULP ordering on the shared R32F projection depth;
- screen-ray hit thickness is reconstructed and compared in linear camera-space world units
  (`2 / 256`), following the camera-space-Z contract used by the referenced screen-space ray
  tracing algorithm.

This is not a larger distance or work budget. The 256 projected-step and eight scene-query caps
are unchanged. Single-variable diagnostics showed that 256 to 1,024 screen steps did not change
`r2`; raising the query cap instead created interface-exhaustion failures.

| Snapshot | Metric | Before | After |
|---|---:|---:|---:|
| `r1` | foreground pixels | 0 | 11,534 |
| `r1` | screen hits | 118,167 | 127,584 |
| `r1` | fallback pixels | 59,590 | 38,639 |
| `r2` | rejected-depth pixels (diagnostic reason breakdown) | 28,127 | 7 |
| `r2` | screen hits | 40,412 | 67,577 |
| `r2` | fallback pixels | 48,049 | 17,802 |

The amber foreground sentinel is complete at `r1`, while `r2` retains the colored raster
objects instead of replacing them with gray simplified voxel fallback. Both final Release runs
reported zero exhaustion and zero non-finite pixels. The fixed 25% acceptance camera also passed
with 15,580 foreground pixels, 80,506 screen hits, zero query-budget fallback, zero exhaustion,
and zero non-finite pixels.

An RTX 3060 Ti Release A,B,B,A comparison used the fixed 25% Glass workload, a 30-second sample
window, and 63 baseline versus 62 candidate post-warm-up samples:

| Metric | Baseline median/p95 | Candidate median/p95 | Delta median/p95 | Gate |
|---|---:|---:|---:|---:|
| `frame.render` | 25.979 / 29.338 ms | 25.780 / 28.668 ms | -0.77% / -2.28% | pass |
| `tracer.render` | 16.321 / 18.212 ms | 15.794 / 19.109 ms | -3.23% / +4.92% | pass |
| `tracer.pass` | 9.657 / 11.683 ms | 8.911 / 12.210 ms | -7.73% / +4.52% | pass |
| `glass.resolve` | 4.765 / 5.478 ms | 4.706 / 5.496 ms | -1.24% / +0.34% | pass |

All existing 5% incremental gates pass. Evidence is under
`target/perf-glass-depth-linear-ab-v4/` and `target/glass-r12-diagnosis/`.

## Memory and lifetime

At 800x500, enabled Glass extent resources total 12,800,000 bytes (12.21 MiB), or 32 bytes per
internal pixel:

| Resource group | Format bytes/pixel | Bytes | Lifetime and alias opportunity |
|---|---:|---:|---|
| `GlassFront` depth + packed event | 4 + 4 | 3,200,000 | Written by Glass tracer, consumed by resolve; dead afterward. Packing/precision reduction is possible after measurement. |
| Unified opaque HDR + depth + provenance | 8 + 4 + 4 | 6,400,000 | Written by composition and consumed by resolve. It overlaps `GlassFront`, so same-frame aliasing is not currently valid. |
| Glass debug counters | 8 | 3,200,000 | Written by resolve and read only by acceptance capture. A non-diagnostic build can remove or lazily allocate it. |

Feature OFF allocates only 2x2 true-2D placeholders for these six images: 128 bytes total. It
does not allocate or clear full-resolution Glass targets, and it does not record the Glass
resolve pass.

## Artifacts and reproducibility

- Final view: `target/glass-final-2872ee63.png`
- Screen-validity/fallback view: `target/glass-fallback-2872ee63.png`
- Screenshot logs: `target/re-flora-logs/re-flora-20260828-033609.382-197937.log` and
  `target/re-flora-logs/re-flora-20260828-033620.142-197998.log`
- Coverage reports: `target/perf/final2-glass{10,25,50}-{a1,b1,b2,a2}.json`
- Feature-OFF comparison: `target/perf/final-feature-off-ab-v2/comparison.json`

Reproduce the primary 25% workload with:

```bash
python scripts/perf_suite.py run glass-coverage-25 \
  --label glass25 \
  --binary target/release/re-flora \
  --output target/perf/glass25.json
```

## Product and instrumentation limits

- There is no direct-sun colored Glass shadow and no caustics.
- DDGI probe rays intentionally use straight-through Glass transport rather than refraction.
- Screen-space reuse is validated and has voxel/sky fallback, but this is not complete
  secondary visibility for arbitrary off-screen raster geometry.
- Top-K and query budgets are deterministic. Budget fallback may produce residual sky energy;
  it is counted, not hidden. Authored scenes currently have zero exhaustion.
- ID 3 remains an experiment-only alias. Glass cannot be saved and must not appear in ordinary
  worlds until a future schema/material decision is made.
- GPU timestamps currently expose the aggregate `glass.resolve` pass. Screen reuse, voxel
  queries, and direct-transmittance work are distinguished by per-pixel counters inside that
  monolithic dispatch, not by separate timestamp scopes. DDGI semantic/revision correctness is
  captured by dedicated tests and readbacks, but full per-frame DDGI Glass segment/recovery
  telemetry remains future instrumentation.
- Results are specific to the tested GPU, driver, resolution, and fixed authored scenes.

## Final validation commands

- `cargo fmt --check`: pass.
- `cargo check`: pass; 102 native Slang shaders precompiled.
- `cargo test -- --skip patt_seam_replay_uses_the_saved_snapshot_and_only_punches_the_roof`:
  pass (4 auxiliary binary tests plus 675 main tests, 1 ignored, and the one documented PATT
  fixture filtered out).
- `python -m unittest discover -s scripts/tests -p 'test_*.py'`: 83 passed.
- Normal feature-OFF hidden release run and Glass 25% resize-lifecycle hidden release run:
  pass, clean shutdown, no Vulkan validation error, device loss, panic, non-finite Glass pixel,
  or Glass exhaustion.
- Post-stabilization `cargo fmt --check` and `cargo check`: pass; 102 native Slang shaders
  precompiled.
- Post-stabilization Rust suite with only the documented dirty-snapshot PATT fixture filtered:
  677 passed, 1 ignored; Python suite: 83 passed.
- Two repeated hidden Release T1/T2/T3 capture rounds: deterministic counters, zero
  exhaustion, zero non-finite pixels.
- Post-stabilization feature-OFF hidden Release smoke: 2x2 Glass placeholders only, clean
  shutdown, and no Glass resolve scope or runtime error.

An additional package-only `cargo test -p re-flora-shader-build` invocation did not reach test
execution because Cargo itself panicked in feature resolution. The authoritative root
`cargo check` and root `cargo test` both compile and exercise the shader-build dependency and
passed; this toolchain issue is not counted as a Glass test failure.
