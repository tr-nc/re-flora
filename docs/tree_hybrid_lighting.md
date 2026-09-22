# Hybrid lighting for thin raster-tree wood

## Try-out

`R → Debug → Whole Tree Rasterization → Hybrid thin-branch lighting (B, requires raster trees)`.
Enable `Raster whole trees` first. The new saved checkbox defaults off: unchecked retains the
original wood response; checked blends continuously according to rest-occupancy normal confidence.
Wind can be enabled or disabled in either mode. No visible window is launched by validation.

This changes wood shading, not tree voxelisation, silhouette, skinning, collisions, editing,
leaf lighting, terrain shading, or DDGI transport's material model. It cannot recover branches
that voxelisation omitted or guarantee coverage for subpixel geometry.

## Confidence

`src/tracer/voxel_normal.rs` consumes the same radius-two (5×5×5) occupied-neighbour offsets as
the original estimator. It retains the original `normalize(-sum(offset))` normal, including the
upward numeric fallback, but also retains the information that normalization used to discard:

- Direction coherence: `length(sum(offset)) / sum(length(offset))`.
- Tangential support: the smaller eigenvalue of the occupancy covariance projected onto the
  plane perpendicular to the candidate normal. Support must exist in **both** tangent directions;
  a line or its end cannot become reliable merely because its moment is large.
- Confidence: `smoothstep(0.05, 0.35, coherence) * smoothstep(0.2, 1.2, support)`.
  Zero moment produces zero confidence. Support thresholds are in squared voxel units and are
  calibrated to this radius-two estimator, not physical probabilities or general scale invariants.

Axis-aligned and diagonal lines, isolated voxels, and thin-sheet interiors/edges are low-confidence;
broad half-spaces retain full confidence. Intermediate thicknesses receive intermediate values.
Occupancy is discrete, so this is a continuous *response*, not a promise of infinitely smooth
changes when an actual voxel is added/deleted. Confidence is rebuilt with visible terrain/tree
geometry and is invariant under camera movement and wind. The estimator does not depend on
branch radius or material identity, so terrain can adopt the same policy later.

## Lighting and geometry

The original per-fragment sun and DDGI response is weighted by confidence. Low-confidence wood
uses a per-voxel surface average, rather than treating the numeric upward normal as a real surface:

- One sample on an actual triangle of each exposed voxel face; centroids and geometric normals
  come from the resident skinned surface, including when wind is disabled. Hidden faces are not
  sampled. Degenerate triangles contribute nothing.
- DDGI and local-light irradiance are averaged by sampled triangle area. Existing DDGI visibility,
  invalidation/fail-closed behavior, and finite local-light/Glass visibility remain in force.
- Sun visibility is averaged over light-facing exposed samples, weighted by projected area, then
  uses a flora-like 0.75 directional response. Terrain/wood, leaf, and cloud shadows remain active.
  With no light-facing exposed support there is no sun contribution, not an ambient fill.
- Geometric normals choose safe surface receiver offsets; the uncertain occupancy normal does not
  drive those fallback offsets. The average is a stylized small-surface approximation, not exact
  integration of every deformed quad or a new physical scattering model.

The existing compute light cache stores the weighted fallback sun/environment plus hybrid local
light in XYZ and confidence in W. Fully unreliable fragments skip their old DDGI query; transition
fragments blend both responses. No constant ambient term or skipped visibility test is used. The original response remains unchanged for fully reliable cells and when off
(apart from floating-point summation order).

Confidence uses the spare W lane of resident normal records and survives GPU skinning unchanged.
Exposed-face bits occupy bits 17–22 of the cell property word; the low 17 bits still encode the
original oct16 normal plus the nonempty sentinel. No additional persistent GPU buffer is needed.
The lighting compute pipeline is registered as a DDGI consumer for allocation/publication changes
and receives DDGI/visibility resources at initial construction and resize as well.

## Cost and validation policy

The first version prioritizes a correct visual A/B. At most six DDGI/local-light surface samples
are evaluated per weak-normal voxel per frame, **not per fragment**. Fully reliable cells skip that
work. There are no extra occupancy fetches, but rebuilding confidence adds covariance arithmetic.
This is not a zero-cost or dense-forest performance claim; performance acceptance follows visual
approval using release-mode measurements.

Reproducible visual captures (configuration is restored even on failure):

```sh
cargo build --release
python3 scripts/check_raster_tree_static.py --hybrid-lighting --output target/tree-hybrid-visual
python3 scripts/check_raster_tree_static.py --hybrid-lighting --wind --output target/tree-hybrid-wind
# Change the same fixed sun time in both modes:
python3 scripts/check_raster_tree_static.py --hybrid-lighting --time-of-day 0.3 --output target/tree-hybrid-low-sun
```

Unit tests cover degenerate neighbourhoods, broad/oblique half-spaces, axis symmetry, intermediate
confidence, rebuilding after edits, packed face/normal identity, and GPU rest-record metadata.
The saved field participates in the generic save/reload tests. The hidden raster-tree smoke reads
back actual GPU confidence and light-cache outputs, checks finite/nonnegative values and A/B
round trips, and retains the existing pose, mesh, BVH, edit, growth/removal/replacement and resize
checks. Python tests ensure lighting A/B keeps raster geometry in both modes and restores files
on capture failure.

## Validation — 2026-09-22, Apple M4 Pro / MoltenVK

- `cargo fmt --check`, `cargo check`, `PATH=/opt/homebrew/bin:$PATH cargo test`: passed
  (1065 binary + 4 library tests, 2 ignored). The first test run used macOS Python 3.9 and failed
  the pre-existing lighting analyzer test because `tomllib` was unavailable; the full rerun used
  Homebrew Python 3.14. No production workaround was added.
- Three new capture tests and five existing benchmark/CLI tests passed. The new Python test file
  passes Ruff; the existing capture script retains its three pre-existing lint findings
  (EXE001/I001/FURB167), with no additional findings when those are excluded.
- `cargo run --release -- --hidden --mute --auto-exit 0.5`: passed.
- `cargo run --release -- --hidden --mute --raster-tree-smoke --resize-lifecycle-test`: passed.
  Actual GPU light-cache confidence round trips were checked in static and skinned modes. A live
  point light was added and removed (GPU source counts 0→1→0); 17 exact CPU/GPU rays matched,
  a real edit removed three wood voxels, and age/removal/replacement and resize passed. Final log:
  `target/re-flora-logs/re-flora-20260922-131739.635-33076.log`.
- The new GPU-cache assertion initially caught missing frame-snapshot→uniform wiring, while CPU
  metadata itself was correct. The wiring and frame-input sentinel test were fixed; the complete
  smoke then passed. This is a real red/green check, not merely a successful launch.
- Mature tree confidence counts: **2 fallback / 1861 transition / 2090 fully reliable**;
  age 0.5: **4 / 372 / 289**. GPU skinning preserved confidence exactly.
- Twelve actual captures: `target/tree-hybrid-{visual,wind,night}/{A,B}-{wood,canopy}.png`.
  Inspected daytime A/B wood, B wind wood/canopy, and nighttime A/B wood: thin upper branches
  acquire softer, flatter illumination while the thick trunk keeps its major directional shading;
  tree/ground shadows remain, and the fallback does not introduce a night-time ambient glow.
  This is a candidate for user visual review, not a claim of approved appearance or exhaustive
  view/Glass validation.
- Final default/smoke/capture/performance logs contain no ERROR, panic, or VUID messages, and exit
  with `failures=0`. The worktree's `--latest-log` / `--tail-latest-log` helpers were inspected.
  This does not imply an external Vulkan validation layer was enabled. GUI/camera files were
  restored; only the intended new GUI declaration remains. Generated changes are limited to
  `src/app/generated/gui_adjustables_gen.rs` and `src/auto-generated/gpu_structs.rs`.

### Bounded release cost observation, not acceptance

Fixed 2560×1440 single-tree camera, static wood, no flora/particles/clouds/god rays/lens flare,
fixed time 0.47. Three alternating-order runs per mode, 12 seconds each, discard the first three
seconds after the first frame log. Use only the fixed every-30-frame cadence: ordinary PERF frame
logging also includes extra slow frames and would bias the distribution. Normal automatic present
mode, no freezing of the environment update pipeline. Logs/summary:
`target/tree-hybrid-perf/`; local diagnostic driver: `target/measure_tree_hybrid.py`.

| Recorded metric | Original | Hybrid |
| --- | ---: | ---: |
| Periodic steady samples | 76 | 79 |
| Frame median / p95 | 10.97 / 18.38 ms | 9.71 / 16.49 ms |
| Main-frame GPU median | 7.939 ms | 7.577 ms |
| Tree-lighting compute GPU median / p95 | 0.046 / 0.069 ms | 0.256 / 0.633 ms |

The tree-lighting compute adds approximately **0.21 ms median** in this scene. Do not interpret
noisy lower total-frame numbers as a speedup or extrapolate this single-tree/no-local-light result
to a forest or many lights. An earlier eight-second diagnostic lacked enough periodic steady
samples and was superseded by these longer runs. Visual approval and subsequent performance
acceptance remain separate stages.
