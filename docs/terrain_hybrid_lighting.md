# Hybrid lighting for ordinary thin terrain voxels

## Try-out

Run `cargo run --release`, then **R → Debug → Lighting Diagnostics → Hybrid
thin-voxel terrain lighting (B)**. The saved checkbox defaults **off**:
unchecked is the original terrain response, checked enables the candidate.
It is independent of raster-tree lighting, whose approved default is now on.

This is the current **Contree primary-surface terrain shading** path, not hardware
ray tracing, the path-tracing reference, or DDGI transport. It applies to ordinary
materials, including isolated world voxels, rods and single-voxel sheets. It does
not change voxel geometry, collisions, material colors, moisture or emission.

The general rule is reusable: average received irradiance over exposed surface
area, not over only the light-facing projected area. The stylized choice to give
a weak-normal voxel one surface-average response is not exact per-face shading.
It avoids trusting a fabricated upward occupancy normal; geometric face normals
are still necessary for irradiation and safe visibility receivers.

## Implementation and ownership

- `surface_normal_policy.slang` owns the radius-two estimator's confidence
  thresholds. `build.rs` generates the CPU constants used by raster trees from
  that source; terrain uses the same policy through `occupancy_normal.slang`.
- Surface extraction retains its packed 5×5×5 occupancy rows and original normal
  moment. The row accumulator also calculates covariance and distance support,
  without extra occupancy fetches. Isolated cells, lines and thin sheets have low
  confidence; broad half-spaces retain full confidence. Confidence is geometric,
  not a probability, and calibrated to this stencil in voxel units.
- The existing 32-bit Contree surface record carries six exposed-face bits in
  bits 24–29 and six-bit uncertainty in bits 4–7/30–31. Material bits 0–3 and oct16
  normal bits 8–23 are unchanged. Atlas moisture/state uses a **different format**
  and is not repurposed. Quantization error is at most 1/126 in confidence.
- Metadata follows the same Visible Terrain Publication as the normal, including
  the existing normal halo and rebuild rules. Shading does not resample mutable
  atlas occupancy or maintain an independent revision/cache owner.
- `MarchingResult.surface_data` carries the published record to the consumer.
  Zero/legacy metadata means reliable original shading. Non-Contree hits, notably
  opaque DDA events behind experimental Glass, conservatively keep their original
  response instead of reading an invalid leaf address. This is not full Glass
  pipeline adoption.
- `tracer.slang::thinTerrainLighting` samples actual exposed face centres. Sun,
  local lights and DDGI retain their existing visibility paths; sun energy and
  diffuse irradiance use the shared `surface_irradiance.slang` area reducer already
  used by raster trees. There is no minimum ambient, brightness clamp, or tip gain.
- Confidence blends original and averaged responses. Fully unreliable hits skip
  the old smooth-normal DDGI/direct calculation; fully reliable hits skip the
  fallback. Path-tracing reference and DDGI diagnostic views bypass the new blend.
  Emission and material/preview application remain outside the lighting blend.

No new persistent GPU buffers, descriptor bindings, or per-setting save hooks are
required. Unlike the tree cache, this first terrain adapter samples **per primary
hit**, not once per voxel. At most six exposed-face DDGI/local-light samples are
added on weak-normal hits. Surface metadata calculation runs at rebuild time in
both A and B; the on/off frame measurements below do not measure that common
construction overhead or the feature-off shader overhead against an older binary.

## Reproducible evidence

```sh
cargo build --release
python3 scripts/check_terrain_hybrid_lighting.py --output target/terrain-hybrid/visual
python3 scripts/check_terrain_hybrid_lighting.py --irradiance --output target/terrain-hybrid/linear
python3 scripts/analyze_terrain_hybrid_lighting.py target/terrain-hybrid/linear
python3 scripts/check_terrain_hybrid_lighting.py --benchmark --output target/terrain-hybrid/perf
```

The helpers serialize GUI changes and GPU runs with the existing GPU lock, restore
configuration on success/failure, and reject missing captures or fatal log errors.
Irradiance capture is a separate one-shot run; it must not preempt a delayed PNG
capture. `--irradiance` captures the first **published** field, not a claim of full
convergence or the exact field shown in the later PNG. The radiometric guard tests
direct light, independently of DDGI convergence.

The `thin-voxels` environment fixture contains nine identified isolated rock cells,
two rods, a thin sheet and a thick control block. It uses normal terrain publication,
not replacement raster geometry. Direct CLI access:

```sh
cargo run --release -- --hidden --mute --environment-lighting-test-scene thin-voxels --screenshot environment-test-scene target/thin.png --screenshot-delay 4 --auto-exit 7
```

Actual Linux / RTX 3060 Ti evidence is under `target/terrain-hybrid/`:

- Inspected `visual-final/{A,B}.png`: isolated voxels are no longer uniformly
  brightened by the upward fallback; rod/sheet edge discontinuities are reduced.
  The broad block retains its main volume. Terrain visual approval remains with
  the user; approval of the earlier tree correction is not approval of this adapter.
- `linear-final/{A,B}.rfirr`: 960×540 raw lighting. After dividing out recorded
  visibility, all nine isolated cells have a B/A solar ratio of **0.4034155**, as
  predicted by exposed cube area and the actual sun direction. There are 107–133
  samples per cell in each mode. The thick control has 90 samples and ratio **1.0**.
  See `linear-result.json`. Passing A as the B input produces RED/exit 1
  (`linear-red.json`); this is not just a successful-startup check.
- In hybrid captures the component shadow planes represent the projected-area
  weighted surface visibility blended with the original response. The world
  plane's exact single-ray visibility remains the original diagnostic; it is not
  relabeled as the surface integral. The analyzer uses the evaluated component
  visibility plane, not that single ray.

## Release cost observation (not acceptance)

Same binary, automatic present mode, fixed camera/sun, no flora/particles/clouds/
god rays/lens flare. Each scenario used A,B,B,A, **30 seconds per run**, discarding
frames below 600. Only the GPU/CPU frame-scope samples are consumed, not additional
slow-frame messages. Surface workloads and final extents were checked unchanged:
**2880×1620 swapchain, 1440×810 rendered scene**. Workload and sample requirements
are owned by `config/perf_scenarios.toml`; raw reports include every sample.

| Scene / metric | Off A median / p95 | On B median / p95 | Median change |
| --- | ---: | ---: | ---: |
| Player camera — main-frame GPU | 3.110 / 3.551 ms | 3.306 / 3.845 ms | +0.196 ms (+6.3%) |
| Player camera — terrain pass | 1.203 / 1.208 ms | 1.393 / 1.402 ms | +0.190 ms (+15.8%) |
| Thin fixture — main-frame GPU | 2.708 / 3.131 ms | 3.052 / 3.368 ms | +0.344 ms (+12.7%) |
| Thin fixture — terrain pass | 0.857 / 0.862 ms | 1.202 / 1.209 ms | +0.345 ms (+40.3%) |

Player results: `perf-final/comparison.json`, 313 A / 309 B samples. Thin results:
`perf-thin-repeat/comparison.json`, 354 A / 328 B samples. CPU frame medians were
5.317→5.466 ms (player) and 4.812→5.177 ms (thin).

The first 15-second diagnostic had too few periodic samples and was rejected.
The first full thin series had a noisy final A run (large p95 spikes), so a complete
A,B,B,A thin series was repeated; the original logs remain in `perf-final/`.
Its terrain-pass median delta was also +0.345 ms. Do not interpret that first
series' inverted p95 as a speedup. The repeat has consistent p95 direction.

This is a measurable cost, not a zero-cost or performance-acceptance claim. It
excludes a dense forest, many local lights, other GPUs, higher internal resolution,
and construction-cost comparisons against the pre-metadata version. A sparse
per-voxel cache could be evaluated later if those workloads require it; it is not
implemented or assumed here. The terrain candidate remains opt-in.

## Validation

- `cargo fmt --check`, `cargo check`, `cargo test`: passed (1068 binary + 4 library
  tests, 2 ignored diagnostics). Generated changes come from the GUI declaration
  and shader uniform reflection; generated files were not hand-edited.
- All **20** Slang CPU tests passed. New tests exercise degenerate/broad/intermediate
  normals, original oct16 normal identity, all confidence/face/material combinations
  and legacy metadata. The shared reducer's existing energy/shadow/night tests pass.
- Hybrid capture/helper tests and perf-suite tests passed. Ruff and Pyright passed
  for the new Python files (`uvx pyright` because the global executable was absent).
- Release hidden/muted default run passed. Raster-tree smoke + resize passed with
  terrain A/B round trips, ordinary terrain/tree rendering, local lights, edits,
  age and replacement. `terrain-edits` with B enabled completed close/reopen-skylight
  publication and density rebuild, ending at terrain revision 4.
- Latest-log/tail helpers were inspected. Final default/smoke/edit logs ended with
  `failures=0`, no ERROR, panic or VUID messages. This is not a claim that an external
  Vulkan validation layer was active. Saved GUI/camera files were restored.
