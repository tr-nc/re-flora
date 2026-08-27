# Glass voxel software-RT validation

## Status

The staged experimental implementation is complete through Phase 4 at commit
`2872ee635f0dc85fe0496de3beb94d7440075d12`. Correctness, isolation, Vulkan validation,
fallback, bounded-work, and feature-OFF regression gates pass. The 25% coverage planning
performance target does not pass, so this remains an isolated experiment and is not
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

Feature OFF was separately compared against pre-Glass commit
`d820d3d7dbe10cb6cfb61438b0bbfd4381d59956` with the standard `render-steady` A,B,B,A gate.
All 11 configured metrics passed. Representative pooled results:

| Metric | Baseline median/p95 | Candidate median/p95 | Delta median/p95 | Budget |
|---|---:|---:|---:|---:|
| `frame.render` | 1.623 / 2.031 ms | 1.625 / 2.039 ms | +0.12% / +0.35% | 2% |
| `tracer.render` | 0.472 / 0.476 ms | 0.475 / 0.480 ms | +0.64% / +0.84% | 2% |
| `render.trace_record` | 0.241 / 0.267 ms | 0.243 / 0.268 ms | +0.83% / +0.37% | 3% |

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

An additional package-only `cargo test -p re-flora-shader-build` invocation did not reach test
execution because Cargo itself panicked in feature resolution. The authoritative root
`cargo check` and root `cargo test` both compile and exercise the shader-build dependency and
passed; this toolchain issue is not counted as a Glass test failure.
