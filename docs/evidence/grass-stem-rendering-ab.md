# Ordinary grass: analytic stem A/B

## Candidate

Implementation: `8966a94c` (local). Debug → **Pixel Sampling — Grass** →
**Experimental grass stem rendering (tall and short)**. Unchecked is the original;
checked replaces, rather than overlays, both ordinary grass meshes. Default is off;
the declarative checkbox is saved and searchable. Old GUI saves get the missing
checkbox without changing their other values.

Grass and flowers share rounded-cone intersection, model-space pixel sampling,
perspective depth remapping and conservative proxy projection. Grass has no heads,
branches, sockets or head-cache allocation. Six proxy vertices replace the old
per-voxel cube/billboard vertices. The grass adapter uses the existing per-layer
pose, growth trimming, rest bend, wind, grass palette, stress tint and environment
lighting cache. It connects the live voxel centers with constant-radius rounded
segments (radius 0.5 voxel); this changes square cross sections and stepped bends
into rounded continuous coverage. Flower taper/radius/color controls do not change
grass thickness or colors. Sampling resolution is shared with flower stems.

This is a visual candidate, **not performance acceptance or a default migration**.
The baseline's far per-voxel billboards are retained only in A; B currently uses the
same analytic chain at all distances. B still evaluates each layer's pose/shading
in each proxy vertex and retains the lighting compute cache: fewer vertices do not
mean an equally large reduction in total vertex work or cache cost.

## Reproduce

```sh
cargo build --release
node scripts/validate-grass-stem-ab.mjs --help
node scripts/validate-grass-stem-ab.mjs
```

The helper runs the Release binary sequentially, hidden/muted, using
`RE_FLORA_GRASS_STEM_REVIEW=<near|mid|far>-<both|tall|short|curved>-<a|b>`
with `--windowed --perf --authored-flora-bench`. The isolated opt-in fixture paints
nine deterministic mature grass patches through the production painting path,
then fixes camera, growth potential, inertial response and sampling resolution
(45). Normalized world-pose/shader time advances at 1/60 per fixture frame.
There are 120 warmup frames and 300 sample frames; the first four sample frames
are excluded for timestamp readback latency. Two repeats use A/B then B/A order.
Each table entry aggregates 592 timestamp samples. GUI is hidden in this fixture.
Optional `RE_FLORA_GRASS_STEM_CAPTURE=<directory>` schedules a screenshot at
fixture frame 120, subject to the existing scene/lighting readiness gate.
No fixture or helper writes GUI config or camera snapshots.

Manual try-out (no perf overlay, movement and the checkbox remain interactive):

```sh
RE_FLORA_GRASS_STEM_TRYOUT=1 \
RE_FLORA_DEBUG_PANEL_REVIEW=1 \
RE_FLORA_DEBUG_SEARCH_REVIEW='Experimental grass' \
cargo run --release -- --authored-flora-bench
```

This only seeds the patch and camera for ten frames, then relinquishes control;
it does not force benchmark tuning, fixed time, or a rendering mode afterward.

## Measured result

GPU: **NVIDIA GeForce RTX 3060 Ti**. Screenshot/swapchain size: **2560×1440**.
All cases contained 2,267 tall and 1,831 short instances; the tall/short cases select
which species draws, not a different population. Only grass draw cost is reported
as `graphics.flora`; environment-cache compute is a separate scope. Whole-frame
GPU time is `frame.render`, not CPU wall time or GPU+present. Units below: **ms**.

| Case | Grass A p50 | Grass B p50 | Grass A p95 | Grass B p95 | Frame GPU A p50 | Frame GPU B p50 |
|---|---:|---:|---:|---:|---:|---:|
| Near, both | 0.103 | 38.410 | 0.111 | 38.924 | 6.101 | 43.446 |
| Mid, both | 0.089 | 0.939 | 0.095 | 0.952 | 6.194 | 6.340 |
| Far, both | 0.036 | 0.272 | 0.041 | 0.281 | 6.205 | 6.200 |
| Mid, tall | 0.067 | 0.737 | 0.073 | 0.768 | 6.120 | 6.173 |
| Mid, short | 0.033 | 0.268 | 0.037 | 0.276 | 6.146 | 6.231 |
| Near, strongly curved | 0.102 | 45.294 | 0.110 | 45.562 | 6.105 | 50.248 |

**A wins grass draw time in every tested case.** Far whole-frame times are
indistinguishable at this noise level; that is not evidence that B is faster.
GPU scopes may overlap with other work and must not be summed to predict frame
cost. CPU frame p50 was about 17 ms in the non-pathological cases (present/wait
included), and about 49–56 ms for B's extreme near cases. Those CPU/present
figures are not a GPU performance advantage.

Source inspection gives a concrete next hypothesis: a proxy crossing the near
plane conservatively becomes full-screen. Thousands of overlapping analytic
proxies with explicit fragment depth and several segment tests can therefore
produce enormous near-camera fragment cost. The mid/far slowdown also exists
without that extreme case. No pipeline-statistics/overdraw counter experiment
has yet isolated how much of each slowdown is attributable to near-plane bounds,
intersection work, depth rejection, or repeated vertex pose work.

After visual approval, investigate near-plane clipped/tighter proxies first,
then cheaper chain intersection, per-instance pose preparation, and screen-size
LOD. These are optimization opportunities, **not measured future wins**. The old
path also has optimization opportunities and is already very cheap in this scene.

## Validation and artifacts

- `cargo fmt --check`, `cargo check`, full `cargo test` passed for the rendering
  step: 1,355 app tests + 4 auxiliary tests, 4 ignored. Subsequent fixture test
  validates A/B pairing and case parsing; final full suite passed: 1,356 app
  tests + 4 auxiliary tests, 4 ignored (`target/grass-ab-final-tests.log`).
  Final interactive-fixture hidden Release smoke passed with `failures=0`.
- All 31 Slang CPU tests passed, including empty/one-layer/full-height/bent grass,
  conservative bounds, intersections and zero-scale proxy rejection.
- Both A and B hidden muted Release smoke runs exited with `failures=0`, no
  Vulkan validation/panic errors. The first Wayland B smoke emitted a desktop
  color-scheme portal timeout; the X11 retry and all 24 measured runs had no ERROR,
  panic or VUID and exited with `failures=0`.
- `target/grass-stem-ab/summary.json`: p50/p95 and sample counts.
- `target/grass-stem-ab/<case>-{1,2}.log`: full per-frame GPU/CPU logs.
- `target/grass-stem-ab/<case>.png`: twelve A/B screenshots. Near/mid/far, short
  and strongly curved B screenshots were inspected. B has rounder silhouettes
  and smoother connected bends; apparent density changes as square coverage
  becomes round. Static captures are not a temporal-stability or user approval
  test. Visual acceptance, large-population scaling, cross-GPU performance,
  close-camera clipping appearance and animation stability remain open.
- Generated GUI fields changed through builds; no new shader-derived GPU ABI
  fields. Existing user `config/gui.toml` tuning and camera snapshots are preserved
  and excluded from commits.
