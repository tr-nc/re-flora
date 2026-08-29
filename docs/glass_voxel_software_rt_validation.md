# Glass voxel software-RT validation

## Status

The staged experimental implementation is complete through Phase 4 and includes the
distance-invariant secondary-visibility stabilization through commit `8c5f8c96`, raster
silhouette stabilization through commit `ff51a38a`, and raster-disocclusion stabilization
recorded below. Correctness, isolation, Vulkan validation, fallback, bounded-work, targeted
visual regression, and feature-OFF gates pass. The original
25% coverage planning target against feature OFF still does not pass, so this remains an
isolated experiment and is not ship-ready.

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

## Distance anti-aliasing and opaque foreground preservation

The user-authored `moore` snapshot exposed stable spatial moire bands once a visible Glass voxel
projected below one internal render pixel. This was not temporal noise or a cache race: repeated
Release captures were stable. The cache previously enabled its symmetric two-point face prefilter
only above a one-pixel footprint, leaving distant cells to sample sharp secondary visibility with
one center ray. The resulting one-color-per-voxel signal was undersampled by the final raster.

Commit `69ba96a2` removes the projected-footprint cutoff. Every single-face interface now averages
the same two deterministic, symmetric face samples before publishing one geometric normal and one
transport color for the voxel. Both Fresnel branches are still evaluated for each sample; there is
no jitter, temporal accumulation, output blur, or resolution reduction. Edge/corner events retain
their exact event ray because they do not own one rectangular face.

At the fixed 2880x1620 `moore` capture, sampled at the 1440x810 internal pixel cadence, the fraction
of adjacent affected samples with an RGB L1 jump above 72 fell from 9.685% (685 edges) to 3.839%
(199 edges). Mean affected-neighbor RGB L1 delta fell from 17.384 to 7.128. The visible colored
bands disappear while the stored color remains constant inside each Glass voxel.

The retained `r1` snapshot also isolated the opaque amber sentinel in front of Glass. A matched
Glass-on/Glass-off Release comparison found the same 103,184 amber foreground pixels with an empty
mask symmetric difference. Thus Glass does not replace or lower the foreground object's geometry.
The visible two-output-pixel stair steps are the renderer's existing global 0.5x internal render
extent followed by nearest-neighbor upscale; the Glass change neither enlarges nor blurs them.

An RTX 3060 Ti Release A,B,B,A comparison used the fixed 25% Glass workload and a 30-second sample
window. Each pooled side contains 70 post-warm-up samples:

| Metric | Baseline median/p95 | Candidate median/p95 | Delta median/p95 | Gate |
|---|---:|---:|---:|---:|
| `frame.render` | 23.948 / 24.193 ms | 23.800 / 24.088 ms | -0.62% / -0.44% | pass |
| `tracer.render` | 14.932 / 15.087 ms | 14.905 / 15.057 ms | -0.18% / -0.20% | pass |
| `tracer.pass` | 8.856 / 8.900 ms | 8.822 / 8.879 ms | -0.38% / -0.24% | pass |
| `glass.resolve` | 4.614 / 4.748 ms | 4.624 / 4.760 ms | +0.22% / +0.25% | pass |

All existing 5% incremental gates pass. The fixed 25% acceptance capture measured 23.469% coverage,
35,139 foreground pixels, 181,177 screen hits, zero query-budget fallback, zero exhaustion, and zero
non-finite pixels. Evidence is under `target/perf-glass-moire-ab/` and
`target/glass-moore-diagnosis/`.

## Raster silhouette stabilization

The user-authored `test` snapshot isolated two independent failures:

- The amber raster pole leaked into validated Rock terminal pixels. Depth and provenance were
  checked at one integer coordinate, but HDR was then read through linear `SampleLevel`, which
  blended an unvalidated neighboring pole texel. Both accepted terminal paths now use exact
  integer `Load`; raster geometry is accepted only by the ordered projected-segment walk.
- A sharp colored raster silhouette was expanded to the whole nearest Glass cache cell. One
  normal and one complete transport color per voxel cannot itself represent a resolvable
  secondary-visibility edge inside that voxel.

The retained design keeps the complete one-normal/one-color cache for smooth and subpixel cells.
A cell-level pass marks only same-face neighbors whose raster-hit state differs, whose cached HDR
delta is at least 0.25, and whose projected face is at least one internal pixel. A pixel then
qualifies only on the wrong side of the cached raster state and within six internal pixels of an
eight-direction raster provenance boundary. Six is the smallest validated radius: four leaves
eight visible protrusion events in `test`.

Qualified pixels are compacted into the cache's now-dead active-slot list and resolved by a
separate dispatch, avoiding divergent full-path work in the ordinary cached resolve. The cache
stores the complete transport plus its unique all-transmitted screen-candidate contribution.
Boundary pixels recompute only that contribution and compose
`complete - cached transmitted + exact transmitted`; cached reflected Fresnel branches and
Beer-Lambert transport remain intact. This is still compute/software DDA and introduces no
hardware ray tracing, acceleration structure, ray query, material ID, stats, or persistence
schema.

The deterministic image analyzer reports:

| Capture | Amber ghost pixels | Colored-edge outward jumps | Jump pixels | Result |
|---|---:|---:|---:|---|
| pre-fix baseline | 192 | 13 | 116 | fail |
| final Release A | 0 | 0 | 0 | pass |
| final Release B | 0 | 0 | 0 | pass |

The two complete PNGs are not byte-identical because the time-delayed captures observe different
DDGI/environment/UI temporal states; the two fixed artifact regions and their integer counters are
identical. Evidence is under `target/glass-test-diagnosis/`.

An RTX 3060 Ti Release A,B,B,A comparison used the fixed 25% Glass workload, a 22-second run,
frame-240 warm-up, and 48/46/46/47 post-warm-up samples in A/B/B/A order:

| Metric | Baseline median/p95 | Candidate median/p95 | Delta median/p95 | 5% incremental gate |
|---|---:|---:|---:|---:|
| `frame.render` | 17.695 / 19.499 ms | 17.971 / 19.458 ms | +1.56% / -0.21% | pass |
| `tracer.render` | 9.394 / 10.498 ms | 9.807 / 10.581 ms | +4.40% / +0.79% | pass |
| `tracer.pass` | 3.650 / 4.689 ms | 3.645 / 4.433 ms | -0.14% / -5.46% | pass |
| `glass.resolve` | 4.515 / 4.994 ms | 4.950 / 5.331 ms | +9.63% / +6.75% | fail |

The whole-frame and whole-tracer gates pass; the aggregate local Glass scope adds 0.435 ms median
and remains above a 5% local-pass target. Evidence is at
`target/perf-glass-test-edge-ab-v20/comparison.json`.

The final 50% hidden Release smoke measured 50.480% coverage and 201,919 Glass pixels, with CPU/GPU
scene-query and transport references matching, zero exhausted pixels, and zero non-finite pixels.
Feature OFF retained a 184-byte placeholder allocation and completed without errors. Logs are
`target/re-flora-logs/re-flora-20260829-154559.769-206548.log` and
`target/re-flora-logs/re-flora-20260829-154542.082-206473.log`.

## Raster disocclusion and large-footprint cache stabilization

The user-authored `border` and `border2` snapshots exposed two manifestations of the same
visibility mismatch:

- raster-only producers such as flora and preview geometry are present in unified opaque HDR,
  depth, and provenance, but do not exist in the voxel `SceneQuery`; and
- one cached primary-transmission result cannot represent screen-space disocclusions across a
  close Glass voxel whose projected face spans several internal pixels.

In `border`, a cached voxel terminal replaced an authoritatively visible raster texel beside the
amber and colored blocks, producing a padding-like strip. In `border2`, cache sharing extended the
same mismatch into a large triangular region that removed grass. Increasing screen-trace distance
or raster hit thickness did not change either artifact. A diagnostic second raster depth peel also
did not address the missing raster/voxel semantic link, so it was reverted rather than adding a
new pass and another set of full-resolution images.

The retained fix has two parts. Terminal resolution now preserves the source unified-opaque texel
when depth proves that it is behind Glass and provenance proves that it came from a raster-only
producer; this is a conservative visibility fallback, not an unvalidated color blend. Cache
classification then uses projected screen footprint as an LOD: subpixel voxels keep one complete
cached transport color, intermediate voxels use the existing raster-boundary correction, and
voxels with a projected face radius of at least three internal pixels recompute only the primary
transmitted branch per pixel. The cached one-normal result, reflected branches, Fresnel, and
Beer-Lambert transport are retained in every tier.

Two independent delayed Release captures for both snapshots preserve continuous grass/wall
occlusion in `border2` and clean raster silhouettes in `border`. Evidence is under
`target/glass-border-final/` as `border{,2}-lod3-{a,b}.png`. The fix adds no raster pass, image,
material ID, stats field, persistence schema, or hardware-RT dependency. At the 800x450 internal
snapshot extent, Glass resources remain 23.99 MiB.

An RTX 3060 Ti Release A,B,B,A comparison against the pre-fix shader used the fixed 25% Glass
workload. All incremental 5% gates pass:

| Metric | Baseline median/p95 | Candidate median/p95 | Delta median/p95 | Gate |
|---|---:|---:|---:|---:|
| `frame.render` | 18.576 / 19.615 ms | 18.599 / 19.735 ms | +0.12% / +0.61% | pass |
| `tracer.render` | 9.753 / 10.249 ms | 9.731 / 10.103 ms | -0.23% / -1.42% | pass |
| `tracer.pass` | 3.631 / 3.668 ms | 3.624 / 3.672 ms | -0.18% / +0.13% | pass |
| `glass.resolve` | 4.917 / 5.367 ms | 4.923 / 5.142 ms | +0.11% / -4.19% | pass |

Evidence is at `target/perf-glass-border-lod3/comparison.json`.

Feature OFF was separately exercised with the standard `render-steady` Release A,B,B,A gate.
All eleven metrics pass; `frame.render` is unchanged at 1.631 ms median (+0.00%, +1.74% p95),
and `tracer.render` changes from 0.475 to 0.476 ms median (+0.21%, +0.00% p95). The default
no-Glass graph still records no Glass resolve work, so merging this guarded experiment does not
move ordinary worlds onto the Glass architecture. Evidence is at
`target/perf-feature-off-glass-lod3/comparison.json`.

## Feature-OFF pipeline isolation

The earlier feature-OFF gate proved that the observed frame time was stable, but inspection of
the compiled SPIR-V found that a runtime `glass_experiment_enabled == false` branch was not a
strong enough architectural boundary. Several standard modules still contained Glass traversal,
transport, and resource code even though those branches were never taken. Representative module
sizes and structured branch counts against pre-Glass commit `d820d3d7` were:

| Standard module | Pre-Glass bytes / branches | Runtime-guarded bytes / branches | Specialized OFF bytes / branches |
|---|---:|---:|---:|
| primary tracer | 451,664 / 1,686 | 555,344 / 2,226 | 451,792 / 1,686 |
| shadow tracer | 23,152 / 39 | 40,464 / 129 | 23,152 / 39 |
| composition | 28,740 / 44 | 31,068 / 49 | 28,740 / 44 |
| DDGI probe trace | 111,652 / 371 | 178,808 / 698 | 112,056 / 371 |
| flora lighting cache | 69,080 / 148 | 92,684 / 262 | 69,216 / 148 |
| local-light diagnostic | 35,780 / 105 | 81,540 / 333 | 35,780 / 105 |

The retained implementation compiles paired standard and Glass shader entry points from the same
source, with the Glass transport included only behind a compile-time define. Startup selects
exactly one variant for the primary and shadow tracers, composition, DDGI trace/relocation/voxel
visibility, flora and tree lighting caches, and local-light diagnostics. Feature OFF does not
create the Glass resolve shader module or compute pipeline and no longer creates a duplicate
primary tracer pipeline or duplicate DDGI descriptor generation. The 184-byte shared-resource
placeholder remains solely to keep the common resource bundle structurally valid; it creates no
full-resolution image and records no Glass work.

An uncontaminated RTX 3060 Ti `render-steady` Release A,B,B,A comparison used the pre-Glass
`d820d3d7` binary as A and the specialized feature-OFF binary as B. All eleven configured median
gates pass with 705 baseline and 704 candidate post-warm-up samples:

| Metric | Baseline median/p95 | Candidate median/p95 | Delta median/p95 | Gate |
|---|---:|---:|---:|---:|
| `frame.render` | 1.624 / 2.053 ms | 1.627 / 2.068 ms | +0.18% / +0.71% | pass |
| `frame.cpu_total` | 2.602 / 3.127 ms | 2.609 / 3.161 ms | +0.27% / +1.08% | pass |
| `tracer.render` | 0.471 / 0.484 ms | 0.475 / 0.489 ms | +0.85% / +1.03% | pass |
| `render.trace_record` | 0.244 / 0.276 ms | 0.244 / 0.277 ms | +0.00% / +0.36% | pass |
| `composition.pass` | 0.034 / 0.035 ms | 0.034 / 0.035 ms | +0.00% / +0.00% | pass |

Evidence is at `target/perf/glass-feature-off-specialized-abba/comparison.json`. A clean matched
25% Glass Release pair also keeps all four incremental medians within the existing 5% gate:
`frame.render` +1.07%, `glass.resolve` +0.12%, `tracer.pass` -0.25%, and
`tracer.render` -0.57%. Evidence is at
`target/perf/glass-specialization-coverage25-clean-pair.json`.

## Memory and lifetime

At 800x500, enabled Glass extent resources total 26,431,492 bytes (25.21 MiB):

| Resource group | Format bytes/pixel | Bytes | Lifetime and alias opportunity |
|---|---:|---:|---|
| `GlassFront` depth + packed event | 4 + 4 | 3,200,000 | Written by Glass tracer, consumed by resolve; dead afterward. Packing/precision reduction is possible after measurement. |
| Unified opaque HDR + depth + provenance | 8 + 4 + 4 | 6,400,000 | Written by composition and consumed by resolve. It overlaps `GlassFront`, so same-frame aliasing is not currently valid. |
| Glass debug counters | 8 | 3,200,000 | Written by resolve and read only by acceptance capture. A non-diagnostic build can remove or lazily allocate it. |
| Cell metadata, two transport values, and reused active list | n/a | 13,631,492 | Experiment-only fixed-capacity cache. The second float4 adds 4 MiB for exact primary-transmission replacement; the active list is reused for compacted boundary pixels after cell classification. |

Feature OFF allocates 2x2 true-2D placeholders for the six images plus one-entry cache buffers:
184 bytes total. It does not allocate or clear full-resolution Glass targets, and it does not
record the Glass resolve pass.

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
- `cargo check`: pass; 110 native Slang shaders precompiled.
- `cargo test -- --skip patt_seam_replay_uses_the_saved_snapshot_and_only_punches_the_roof`:
  pass (4 auxiliary binary tests plus 677 main tests, 1 ignored, and the one documented PATT
  fixture filtered out).
- `python -m unittest discover -s scripts/tests -p 'test_*.py'`: 83 passed.
- Normal feature-OFF hidden release run and Glass 25% resize-lifecycle hidden release run:
  pass, clean shutdown, no Vulkan validation error, device loss, panic, non-finite Glass pixel,
  or Glass exhaustion.
- Post-stabilization `cargo fmt --check` and `cargo check`: pass; 110 native Slang shaders
  precompiled.
- Post-stabilization Rust suite with only the documented dirty-snapshot PATT fixture filtered:
  677 passed, 1 ignored; Python suite: 83 passed.
- Two repeated hidden Release T1/T2/T3 capture rounds: deterministic counters, zero
  exhaustion, zero non-finite pixels.
- Post-stabilization feature-OFF hidden Release smoke: 2x2 Glass placeholders only, clean
  shutdown, and no Glass resolve scope or runtime error.
- Raster-disocclusion stabilization: `cargo fmt --check`, `cargo check`, 677 focused Rust
  tests plus four auxiliary binary tests, and all 83 Python tests pass. The documented PATT
  dirty-snapshot fixture was the only filtered Rust test; one unrelated test remains ignored.
- Final feature-OFF hidden Release smoke retained the 184-byte 2x2 placeholders. The final
  50% Glass hidden Release smoke measured 50.480% coverage and 201,919 Glass pixels with zero
  exhausted and zero non-finite pixels; both runs shut down cleanly without a Vulkan validation
  error, device loss, or panic.
- Compile-time feature-OFF specialization: standard shader branch counts match the pre-Glass
  modules, all eleven `render-steady` Release A,B,B,A gates pass, and the Glass resolve module and
  pipeline are not created. Repeated specialized Release captures at
  `target/glass-border-specialized/` retain both raster-disocclusion fixes.

An additional package-only `cargo test -p re-flora-shader-build` invocation did not reach test
execution because Cargo itself panicked in feature resolution. The authoritative root
`cargo check` and root `cargo test` both compile and exercise the shader-build dependency and
passed; this toolchain issue is not counted as a Glass test failure.
