# Solid Color Band stem candidates

## Result and acceptance boundary

Three runtime-selectable raster candidates now work for GPU grass and CPU-owned
stem paths: square tubes, tapered square tubes, and crossed ribbons. Each Band
has one flat, lighting-adjusted color (`nointerpolation`); there is no smooth
base-to-tip gradient or per-pixel stem intersection. Hardware rasterization owns
clipping and depth. These measurements use direct geometry, not model-ray pixelization.

The later optional grass model-grid sampling candidate is documented separately
in [hardware-raster model pixelization](grass-raster-model-pixels.md), including its
pixelized silhouettes, runtime switch and **failed mass-grass performance acceptance**.
That switch defaults off; this report's historical timings remain unchanged.

**Crossed ribbons are the performance leader. They are substantially faster than
analytic grass, but are not a universal improvement over original voxel grass.**
Square tubes retain a solid, block-like volume; ribbons intentionally do not.
Tapered tubes produce narrower, pointed-looking silhouettes without sphere caps.
Visual approval and default migration remain pending. Original rendering stays
available and both experimental enable checkboxes default to off.

CPU candidate drawing is faster than the existing block renderer, but this first
explicit-path stream costs more CPU upload/submission time. Do not interpret the
GPU drawing speedup as the same improvement in overall frame rate.

## Ownership and implementation

- GPU grass retains production instance generation, growth, wind, response and
  lighting. No grass path is simulated on CPU or read back from GPU.
- `stem_band_geometry.slang` owns section construction, safe frames and taper.
  `stem_band_mesh.rs` owns reusable eight-vertex band topology: 36 indices for
  closed square tubes, 12 for crossed ribbons. Grass reserves eight Bands.
- Adjacent grass layer centers define midpoint boundaries with shared tangents
  and radii. The same center-frame helper is used by direct and cached variants.
  Existing layer growth/trimming is retained; this is not a new grass simulation.
- GPU pose reuse extends the existing once-per-layer lighting compute pass with
  a 32-byte position/height and final-color/validity record. Frame-slot-owned
  GPU-only buffers grow with population. Disabling reuse restores the direct
  vertex preparation path. Original/analytic modes do not perform this extra work.
  Glass transport has a matching compute specialization.
- `stem_band_paths.rs` converts CPU parent-before-child paths into Band instances.
  Rest arc and a fixed full-length budget determine Band identity, not current
  height or node index. Deformation, branches and partial visible paths retain
  stable palette membership. Lighting anchors are shared within a material Band.
- Actual climbing plants supply their existing CPU nodes and fixed lifetime arc
  budget; motion, terrain collision, pruning and regrowth stay CPU-owned. Their
  leaves keep the existing block renderer. CPU stems have color and shadow passes.
- The CPU flower-like branched fixture is a stress input for this adapter, **not**
  a port of the existing GPU-authored flower/petal renderer.

The shared abstraction is rendering geometry/material semantics, not a mandatory
universal simulation buffer.

## Release measurements

Measured revision: `af13eea9ef7090ceaa14761f23ef901ee6132caa`.
NVIDIA GeForce RTX 3060 Ti, 2560×1440 physical extent, Release binary, native hidden
window, muted audio, automatic present-mode selection. Tests were serial, not
concurrent. Each scene/candidate ran twice in opposite candidate order; each
reported metric aggregates **592** sampled frames. There were **128 successful
runs** across eight grass scenes and six CPU scenes.

Grass uses real production painting: 3×3 gives 4,098 resident instances
(2,267 tall + 1,831 short); 15×15 gives 82,454 (45,518 + 36,936).
Counts and extent match between candidates. Resident population does not mean
every plant is visible in every camera. CPU fixtures deform/upload paths every
frame; they stress the renderer, not thousands of terrain-colliding vine simulations.

### GPU grass drawing, p50 microseconds

Scope: `graphics.flora`. Pose reuse is enabled for the three mesh candidates.

| Instances / camera | Original voxel | Analytic | Square | Tapered square | Ribbons |
| --- | ---: | ---: | ---: | ---: | ---: |
| 4,098 near | 87 | 1,939 | 110 | 78 | 67 |
| 4,098 mid | 86 | 885 | 83 | 67 | 44 |
| 4,098 far | 29 | 223 | 50 | 54 | 27 |
| 4,098 low | 93 | 958 | 75 | 54 | 46 |
| 82,454 near | 1,404 | 5,455 | 1,135 | 1,205 | 587 |
| 82,454 wide | 565 | 3,081 | 1,200 | 1,143 | 579 |
| 82,454 far | 864 | 4,648 | 954 | 1,014 | 586 |
| 82,454 low | 1,405 | 1,787 | 845 | 1,074 | 488 |

### Account for preparation rather than hiding it

The extra cache is not free. The following p50 values sum `graphics.flora` and
`graphics.flora_lighting_cache` **per sampled frame before taking the percentile**.
The latter is shared flora/leaf lighting preparation, not an isolated grass-only
scope. These are not total frame times and not sums of independent medians.

| Instances / camera | Original voxel | Analytic | Square | Tapered square | Ribbons |
| --- | ---: | ---: | ---: | ---: | ---: |
| 4,098 near | 221 | 2,044 | 232 | 196 | 186 |
| 4,098 mid | 207 | 995 | 216 | 201 | 169 |
| 4,098 far | 157 | 366 | 197 | 199 | 174 |
| 4,098 low | 230 | 1,120 | 226 | 206 | 197 |
| 82,454 near | 1,951 | 6,035 | 1,813 | 1,835 | 1,248 |
| 82,454 wide | 1,158 | 3,627 | 1,829 | 1,783 | 1,269 |
| 82,454 far | 1,412 | 5,197 | 1,586 | 1,644 | 1,214 |
| 82,454 low | 1,950 | 2,333 | 1,481 | 1,799 | 1,207 |

Thus ribbons beat analytic drawing by about 9.3× in the large near scene, but
including preparation gives about 4.8×. Against original voxel rendering, that
same combined scope improves about 1.56×, while large wide **regresses about 9.6%**.
Large-wide combined p95 is 1,478 µs for voxel and 2,346 µs for ribbons. Other scene
work and p95 variability remain significant; do not extrapolate these draw scopes
to whole-game speedups. `frame.render` p50/p95 are preserved in the raw summary.

### CPU-owned stems: drawing, p50 microseconds

Scope: `graphics.cpu_stems`, covering the color draw, not the complete shadow/frame
work. Original blocks receive the same path, population and palette.

| Fixture / resident Band segments at capture | Blocks | Square | Tapered square | Ribbons |
| --- | ---: | ---: | ---: | ---: |
| 256 branched flower-like paths / 10,240 | 284 | 82 | 82 | 59 |
| 1,024 branched flower-like paths / 40,960 | 1,078 | 372 | 354 | 245 |
| 256 vine-like paths / 6,144 | 186 | 48 | 49 | 30 |
| 1,024 vine-like paths / 24,576 | 666 | 213 | 217 | 144 |
| 1,024 vine-like paths, near / 24,576 | 739 | 223 | 225 | 172 |
| 64 growing paths / 640 at capture | 28 | 2 | 2 | 1 |

Growth segment count increases during sampling; the listed count is the common
capture-phase count. Its small, sparse view is a correctness/growth case, not
representative of the large-population rendering cost.

CPU path + submission totals are also measured per frame, not inferred from GPU
scopes. For 1,024 branched paths, p50 is **5.388 ms blocks vs 6.548 ms ribbons**;
for 1,024 vine-like paths it is **3.573 ms vs 4.250 ms**. Path construction itself
is similar; the larger explicit instance stream/submission is the regression.
For branched paths, separate `cpu.upload` p50 is 2.796 ms vs 3.939 ms. These are
unoptimized per-frame stress submissions, not a demonstrated CPU performance win.
Actual climbing motion continues to use its existing 20 Hz protocol.

## Test-fixture clock correction

Early high-population screenshots exposed a test defect: painting stamped births
with real elapsed time while the benchmark rendered with a fixed frame clock.
Some late-painted grass had not appeared at capture, depending on run duration.
Those early quick measurements are exploratory only, not acceptance evidence.

Scripted painting now passes its fixed clock explicitly through
`apply_surface_flora_regeneration_at`; ordinary player painting keeps real time.
The fixture pins spawn duration to 0.28 seconds and warms up 111 frames after the
last paint. A unit test checks the clock and interactive-mode fallback.

A same-binary fixed-clock pose-reuse on/off check reduced wide-scene screenshot
normalized RMSE to 0.000030 / 0.000040 / 0.000052 for square/taper/ribbons. Images
are **not byte-identical**; this is not a depth-buffer equivalence claim. Cached
and direct paths share tested geometry construction and preserve visible style.
The final two-repeat tables above use the corrected clock and committed source.

## Validation and artifacts

Passed:

- `cargo fmt --check`, `cargo check`, `cargo build --release`.
- `cargo test`: 1,362 passed, 4 ignored, no failures.
- `python3 scripts/run_slang_tests.py`: all 33 Slang CPU tests passed, including
  midpoint continuity, degenerate frames, taper and Band-coordinate guardrails.
- Native hidden/muted smoke plus log inspection. Ordinary startup logged an
  existing fruit-physics hitch warning; no rendering error or failed exit.
- Native Glass scene smoke: `SHUTDOWN phase=complete failures=0`.
- All 128 benchmark runs: normal shutdown, no rendering/validation errors, no
  dropped GPU scopes, matching populations/extent, unchanged saved config hashes.
- Final actual wall-vine run with tapered bands: `verified prune=true wait=true
  regrow=true root_recovery=true finite=true nodes=24`; nonzero CPU stem GPU scopes;
  `SHUTDOWN phase=complete failures=0`; both saved configs unchanged.

Reproduce:

```sh
cargo build --release
node scripts/validate-stem-band-candidates.mjs --suite all
# Short exploratory pass:
node scripts/validate-stem-band-candidates.mjs --quick
# Diagnostic direct vertex preparation (still runtime-switchable in Debug):
RE_FLORA_GRASS_BAND_POSE_REUSE=0 node scripts/validate-stem-band-candidates.mjs --quick --suite gpu
```

Local evidence under `target/stem-band-trials/`:

- `full-all/{runs.json,summary.json,path-total.json}`: raw per-frame samples,
  percentiles, revision, config hashes and diagnostic paired totals.
- `full-all/<scene>-<candidate>-<repeat>/`: all 128 captured images; adjacent logs.
- `near-candidates.png`: original, analytic, square, taper, ribbons contact sheet.
- `pose-{on,off}-fixed-clock/`: diagnostic reuse comparisons.
- `final-vine-taper.log`, `glass-cache-smoke.log`, `final-*-tests.log`: native/unit evidence.

The earlier `validate-stem-sampling.mjs` remains stale; it was not repaired or
counted as passing by this experiment.

## Try-out

Use ordinary `cargo run --release`, or seed a small grass patch without timing UI:

```sh
RE_FLORA_GRASS_STEM_TRYOUT=1 cargo run --release -- --authored-flora-bench
```

In Debug's stem concern group:

1. **Experimental grass stem rendering**: unchecked = original; checked = candidate.
2. **Stem candidate (grass and CPU paths)**: 0 analytic grass reference, 1 square,
   2 tapered square, 3 crossed ribbons. Prefer 2 for a narrower solid silhouette;
   compare 3 for speed/thin-plane appearance. CPU experiments use modes 1–3.
3. **Grass: reuse GPU-prepared band pose**: compare preparation strategies at runtime.
4. **Experimental color-band stems (CPU climbing paths)**: switch actual climbing
   stems while keeping their simulation and leaves unchanged.

No visible game was automatically launched. User GUI/camera edits were preserved
and excluded from commits; commits are local, with no push or official packaging.

## Remaining work after visual review

- Choose solid tubes vs thin ribbons based on appearance, not solely draw time.
- CPU submission/instance payload optimization is needed before claiming a whole
  CPU-path performance win; do not move CPU collision simulation onto GPU merely
  to disguise this upload cost.
- Mesh candidates currently retain the same full band topology at both grass LODs.
  A measured, style-preserving distant LOD is a natural next performance step.
- Model pixelization and conversion of other actual flower renderers are separate
  follow-ups. This experiment does not claim universal plant migration, visual
  identity, complete dynamic depth equivalence, or default release acceptance.
