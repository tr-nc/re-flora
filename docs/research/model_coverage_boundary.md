# Strict leaf coverage boundary (scope v3)

Base: `a0b318770d1473974884e0bfe07dee293fc4f230`.

## Reproduction and diagnosis

The unchanged leaf runner passed six times at 5120×2880 on the worker. These
are retained, not evidence of a fix. Checking every pose also exposed the inverse
symptom (GPU addition absent from the CPU plan). A short stream of actual
published poses around the controller's failing flight time reproduced its exact
`instance=1 pixel=11,26` omission. Minimization retains one original variant 18,
its published quaternion/position/size, the fixture camera, and 64px sampling.

Run the deterministic **red on the base implementation** regression:

```sh
flock --close /tmp/re-flora-summer-gpu.lock env -u WAYLAND_DISPLAY \
  CARGO_BUILD_JOBS=4 node scripts/validate-model-coverage-boundary.mjs
```

The renderer-only fixture freezes the captured input, not a favorable replacement
pose. It requires the explicit native leaf review; ordinary play and the original
live A/B runner never enable it. Native failure is `instance=0 pixel=11,26` after
removing the seven irrelevant instances. No numerical assertions are skipped.

Ranked hypotheses and results:

1. **CPU center ownership removes a required repair:** disproved for this case.
   CPU and GPU both miss the center. The actual compute mode 1/3 tile producer
   calls `sampleModelPixelGeometry`, not the CPU repair graph. Mode 0 evaluates
   the graph but the tile producer never reads that result.
2. **Different projected geometry at a conservative cell boundary:** confirmed.
   CPU and GPU clip coordinates for triangle 29 match, but division rounding
   changes bounds and normalized tile coordinates. CPU bounds.x is
   -0.3390507698059082; GPU is -0.3390507996082306 (one ULP).
   CPU projected triangle 29 starts at (11.2487960226,25.6114219607);
   GPU starts at (11.2488756180,25.6114234924). The independent conservative
   edge test, with the unchanged 0.0001px epsilon, gives minimum signed margins
   +0.000034825125 on CPU geometry and -0.000068537943 on GPU geometry.
   The cell is outside even the CPU triangle without the epsilon expansion.
   Thus a CPU reprojection is not the geometry on which the GPU decided coverage;
   this assertion cannot require bit-exact cell membership from those two inputs.
3. **Wrong frame/resources/readback:** disproved by fixed-pose replay, matching
   camera view-projection/inverse bytes, original ray/triangle identity, and the
   targeted producer clip-coordinate capture. It remains red with no pose changes.

Raw captures, commands, every failed probe/build/run, and all initial green runs
are retained under `target/improve-delivery/v3/`. `red-capture-unprobed/` contains
camera/instance/triangle/tile/CPU-plan bytes before shader probes;
`red-capture/` also contains the producer arithmetic probe. Temporary probes are
archived as patches under `probes/`, not shipped. The minimized native red is
`fixture-red/`; CPU and GPU arithmetic comparisons are `cpu-arithmetic.log`,
`producer-arithmetic.txt`, and `coverage-margins.txt`.

## Correction contract

A correct diagnostic must independently classify the **actual projected geometry**,
just as it already uses actual GPU center ownership and independently checks its
rays. Two interface shapes were considered: emulate driver division/reassociation
on the CPU, or read back projected vertices (not coverage decisions) through the
existing native-review adapter. The latter keeps one geometry producer and a
small, explicit diagnostic seam; no GPU-specific arithmetic emulation, tolerance
increase, or production CPU plan is appropriate.

The correction must retain original RGBA/depth preservation, independently verify
projected geometry identity and surface depth, and reject a real missing covered
cell with a negative control. Ordinary production sampling must remain unchanged.
Final acceptance belongs to the controller.

## Implemented correction and proofs

The producer and `projectModelPixelTriangle` share clipping and tile-XY arithmetic
(the depth-preserving seam is detailed below). The native reference dispatch
exports its projected vertices, bounds, triangle
identity and polygon length in a fifth diagnostic slab (normal play still
allocates one slab). The CPU checks this evidence against independent projection,
including source identity, clipping topology, finite coordinates, camera framing
and depth. Its existing 16-machine-epsilon spatial envelope is propagated only
for this projection-identity check. It does **not** enter coverage classification.
The unchanged CPU planner evaluates the observed tile coordinates with the same
0.0001px epsilon as before. CPU-derived vertex depths, not GPU-provided depths,
remain the independent depth oracle. Original center RGBA/depth checks are intact.

This fixes a diagnostic input-contract error, not a dropped production repair.
Ordinary rendering still uses the same GPU center/coverage algorithm, no CPU
ownership or readback, and no repair fallback. The projected-vertex calculation
is shared rather than copied into the diagnostic adapter. Final publication,
frame-slot safety, compute/draw pairing and the three model consumers are untouched.

Validated before committing the correction:

- `cargo fmt --check`, `CARGO_BUILD_JOBS=4 cargo check`.
- `CARGO_BUILD_JOBS=4 cargo test model_pixel`: 24 passed, 1 existing ignored.
- `CARGO_BUILD_JOBS=4 cargo test butterfly_mesh`: 17 passed, including the captured
  boundary, real covered neighbor, wrong identity/topology/frame/depth rejection.
- `slangc shader/tests/model_pixel_projection_evidence.slang -std 2025 -I shader/slang
  -target executable -o target/improve-delivery/v3/model_pixel_projection_evidence`,
  then execute it: passed. It exercises the actual producer projection/clipping
  helper and boundary coverage predicate. The pinned Slang CPU backend needs
  test-only C++ prelude definitions for `precise` and `SLANG_UNROLL`; its failed
  compiler attempts and diagnosis are retained. Production qualifiers are unchanged.
- The same deterministic native fixture is green (`fixture-green-2/`). Its final
  and center masks match the original red capture exactly; center depths are bit
  identical, and final depth differences are at most 1.1920928955078125e-7.
  The original `production-tile-comparison.txt` compared the unprobed capture,
  whose lighting differed, and did **not** establish color preservation. Review
  subsequently found four changed coverage colors against the retained
  `red-capture/` with bit-identical center RGBA. See the correction below; those
  four differences are a producer regression, not lighting drift.
- Native negative control: actually replace the producer's center-empty, covered
  pixel (12,26) with transparent depth=1. The **unchanged coverage assertion**
  rejects it: `GPU omitted conservative coverage: instance=0 pixel=12,26`.
  See `negative-control/` for patch, build/run logs and nonzero runner exit.
  The injected defect was removed and shaders regenerated before final gates.
- Three consecutive independent, unchanged `validate-leaf-model.mjs --seconds 20`
  runs pass at 5120×2880: `strict-leaf-{1,2,3}/`. Initial B64 checked-hit counts are
  1573, 1581, 1581, respectively; original samples and repairs are nonzero. Every
  attempt is retained. No runner, coverage epsilon, depth threshold, mode,
  camera or saved default was changed.

## Consumer protection and final worker validation

All native invocations below used the same non-nested GPU lock, own `target`,
shared sccache, `CARGO_BUILD_JOBS=4`, X11, Release, hidden and muted mode:

```sh
flock --close /tmp/re-flora-summer-gpu.lock env -u WAYLAND_DISPLAY \
  CARGO_BUILD_JOBS=4 python3 scripts/validate_butterfly_mesh.py --seconds 12
flock --close /tmp/re-flora-summer-gpu.lock env -u WAYLAND_DISPLAY \
  CARGO_BUILD_JOBS=4 node scripts/validate-apple-model.mjs --seconds 12 --stage-one
flock --close /tmp/re-flora-summer-gpu.lock env -u WAYLAND_DISPLAY \
  CARGO_BUILD_JOBS=4 cargo run --release -- --hidden --mute --auto-exit 0.5
```

All passed (`butterfly/`, `model-stage-one/`, `smoke/`). Butterfly retains strict
8/22/64px GPU/CPU depth and coverage checks, 21 models, shadow/transmission toggles.
The ordinary model fixture covers both orthographic display modes, five view
counts, rotating leaves/butterflies with mixed resolutions, attached/fallen apples,
actual drops and five native resizes. It does not enable the continuous native
oracle; it protects the ordinary consumer paths, not an orthographic numerical
oracle or a performance budget. Smoke logs show successful exit and failures=0.

`cargo test captured_leaf_boundary -- --nocapture` also passes independently.
Saved-config SHA256 checks match the initial files. No generated files changed.
Temporary instrumentation and the negative-control mutation are absent from
source; all failed experiments remain in `target/improve-delivery/v3/`. No
unchanged full Cargo suite or DDGI native matrix was repeated. The controller
still owns independent review, final aggregate acceptance and cleanup.

## Bounded review correction: supporting materials

Starting head: `830fd24a6e577cea176619898700e9a3aeff6ca0`. All earlier evidence
remains intact; new evidence is under `v3/correction/`.

The review's RGB counterexample reproduces without a build (`reviewed-red.log`):
center RGBA/depth is byte-identical, but coverage colors at (24,30), (28,31),
(41,32), (42,32) differ. Ranked hypotheses were depth-rounding changing the
supporting triangle, changed shading inputs for the same triangle, and wrong
readback identity. A temporary same-dispatch probe ran the original coverage
function and the refactored function with identical mesh, pose, camera, center
hit and lighting. `probe-red-comparison.log` proves the first hypothesis:

| Pixel | Original triangle | Refactored triangle |
|---|---:|---:|
| 24,30 | 9 | 6 |
| 28,31 | 13 | 10 |
| 41,32 | 27 | 26 |
| 42,32 | 26 | 27 |

Depth candidates differ by one ULP; that changes the winning material centroid.
Original shading in the same dispatch exactly matches all four original capture
RGBs; refactored shading matches the actual new final tile. Thus lighting and
readback identity cannot explain the differences. The probe is archived, not
shipped.

The fixed-pose native runner now checks two coverage-material equivalences from
the original producer: (41,32)/(43,31) share triangle 27, and (42,32)/(41,30)
share triangle 26. All witnesses must be center-empty, actually covered and
nonblack. Comparing final RGB bytes between identical material centroids avoids
a lighting-dependent color golden. Before the fix, the exact existing fixture
command exits 1 with **both** equivalences false (`regression-red/` and
`regression-red-console.log`), while the coverage/depth oracle still passes.
`CARGO_BUILD_JOBS=4 cargo check` passes (`regression-check.log`). This regression
is committed before changing producer arithmetic (`6ebd30d0`).

### Depth-preserving correction

The faulty refactor materialized normalized/saturated depths into the projected
polygon, removing the original per-fan division and `precise` XY dataflow from
the coverage loop. That is not numerically neutral for nearest-support selection.
The corrected seam shares homogeneous clipping (`clipModelPixelTriangle`) and
XY conversion (`modelPixelTileXY`), while keeping the producer's original per-fan
float3 division, precise XY locals, barycentrics, depth dot/saturation and strict
`depth >= hit.depth` ordering. Diagnostic vertices remain observations, not
coverage decisions. No epsilon, depth threshold, ordering/tie policy or center
shader changed.

A second same-input native probe (`probe-green-comparison.log`) now has identical
supporting triangles, barycentrics, positions, depths and RGBA for all four
counterexamples. Both probes are archived in `correction/`; temporary source
instrumentation and the legacy-function copy have been removed.

After removing probes, the deterministic runner passes with **both** material
equivalences true (`fixture-green/`). The actual final tile and center tile are
also **entirely byte-identical** to `red-capture/pixels.bin` tiles 1 and 257:

- final SHA256: `1ccae727d361c350cfa1fd26121b1e40ae7b5c561176f4f52c8848ee5dafcc1f`
- center SHA256: `92aa081f646f813eb2c95a58fc33872284038c76569162805cfed389c2882462`

`compare-restored.mjs` / `restored-output-comparison.log` check every float lane:
zero changed RGB pixels, zero changed depths, not merely the four reported
pixels or a coverage-mask comparison. This supersedes the earlier color
preservation conclusion. All consumers use the restored shared coverage loop;
the refactor's depth dataflow change was not inherently diagnostic-only.
This is a proof for the captured input, not a universal cross-driver bit-exact
rendering claim. A separate offline comparison found different optimized SPIR-V
for the original/corrected apple shaders (`spirv/`); compilation alone is not
claimed as output equivalence evidence.

The fixture now logs and requires actual `resolution=64` from instance metadata,
not the configured 16px setting. `cargo fmt --check`, `CARGO_BUILD_JOBS=4 cargo
check`, `cargo test model_pixel` (24 passed, 1 existing ignored), `cargo test
butterfly_mesh` (17 passed), and the explicit Slang projection executable all
pass; logs are in `correction/`. The original strict leaf/butterfly/apple runners
remain unchanged. New final native gates follow this focused correction.
