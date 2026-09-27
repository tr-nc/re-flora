# Pixel models: stage-one lighting and discrete-view preview

Initial implementation: `cb46ae90`; permanent quantization/count slider: `9fe259dc`.
This remains live tile rendering, **not an atlas cache**. The subsequent
[shared orthographic A/B preview](model-pixel-orthographic-preview.md) replaces
camera-dependent tile projection and adds rotating/screen-aligned pixel display.

## Try it

**Pixel Models — Global** contains:

- **Discrete View Count** — integer slider, **8–512**, default **16**.
- **B: Screen-aligned Pixel Grid (unchecked: A, Rotating Pixels)** — saved A/B,
  default A; both modes use fixed orthographic tiles.

View quantization and per-object lighting are always enabled. The lighting
checkbox has been removed, including its app/render/GPU option field. Changes to
the global count take effect immediately. Old saved view and lighting checkboxes
are retired regardless of their values; a missing count becomes 16, and an
existing count is preserved. This single global policy applies to butterflies,
3D falling leaves, and attached/fallen apples. Apples now always use the pixel
pipeline; the former **Flora → Apple Appearance (A/B)** model checkbox and voxel
render path have been retired. **Flora → Apple Appearance** retains resolution.
Butterfly resolution remains under **Butterflies**, and falling-leaf resolution
under **Falling Leaves**. The original leaf sprite comparison is unchanged.

Normal visible try-out command, when requested: `cargo run --release`.

## Shared rendering policy

### One spatial lighting query per object

A GPU prepass samples the object's center once for external shadow visibility
(terrain, leaf and cloud shadow maps) and environment lighting. A single query
can internally use several texture/probe taps; this is one **spatial sampling
point**, not a promise of one texture fetch. Diffuse irradiance uses a fixed
world-up representative normal. The existing legacy-ambient option for particles
is respected.

The shared result is stored in frame-local object data. Each occupied texel still
uses its own material and real-pose normal for cheap directional sunlight and
leaf/wing transmission calculations. No per-texel external-shadow/environment
queries run in this mode. Butterfly per-texel triangle self-shadow queries are
also omitted. Thus object lighting does not make every pixel the same color.

Expected approximation: a shadow boundary crossing an object is not represented
spatially. The whole object shares the center's shadow visibility; crossing a
boundary can change the whole object together. No large-object exception or
multi-point fallback is introduced.

### Adjustable discrete directions, without blending

`src/tracer/model_pixel_views.rs` defines the Fibonacci golden-angle azimuths.
For each requested count N, the GPU distributes N model-local directions over the
whole sphere using `y = 1 - 2 * (i + 0.5) / N`. It does not take a prefix of a fixed
512-direction sphere, which would incorrectly cluster smaller counts into a cap.
An immutable 8 KiB azimuth table avoids per-frame trigonometry and GPU resource
replacement when dragging the slider. Every integer count in 8–512 is supported,
not just powers of two. A future atlas key must include the count as well as the
view index: changing N redistributes directions.

The nearest direction is selected once per object. No view interpolation,
temporal blend, or hysteresis is applied. Angular jumps while moving the camera,
rotating an object, or changing N are intentional.

The sampler applies a rigid view correction around the object center. It changes
the visible sampled geometry, not simulation pose, collision or tree wind.
Butterflies carry their published flight orientation separately from their
articulated wing geometry. Leaves use their published orientation; apples use
attached-tree or fallen-body axes.

Only the **view direction** is quantized. The current preview uses fixed
orthographic tile projection; screen roll and scene-distance scaling happen at
display time. Geometry animation remains continuous. See the orthographic report
for the A/B resampling rules. An animation-frame cache is still future work. Lighting uses the corresponding surface in
the real physical pose, rather than treating the view correction as a physical
rotation of the object.

The existing per-texel depth path remains. In discrete mode this is the depth of
the view-corrected sampled geometry, hence an intentional approximate silhouette/
occlusion. No normal atlas, depth atlas, flat-depth option, or persistent tile
cache has been added. Tile geometry is still sampled every frame.

## Validation

```sh
cargo fmt --check
cargo check
cargo test
node scripts/validate-apple-model.mjs --seconds 12 --stage-one
node scripts/validate-leaf-model.mjs --seconds 9
python3 scripts/validate_butterfly_mesh.py --seconds 12
cargo run --release -- --hidden --mute --auto-exit 0.5
```

- Current Rust validation: 1104 passed / 4 ignored in the main test target, plus
  the build tests. Declarative round trips cover the saved slider and display A/B.
  A layout test keeps resolutions object-specific.
- View-set tests check all slider counts for finite unit directions and balanced
  latitudes; selected small, odd and maximum counts additionally check uniqueness,
  sphere coverage and rigid handedness. Migration tests cover both retired checkbox
  values, missing counts and preservation of an existing count. Render-input tests
  check the shared view count.
- Stage-one live fixture cycles both orthographic display modes at 8/16/37/128/512
  views with fixed per-object lighting, rendering 64 rotating
  leaves, 21 animated butterflies, attached/fallen pixel apples at 8/32/64px, actual
  fruit drops and native resize publication. No saved config changes or Vulkan
  errors. This is runtime correctness/inspection evidence, not a pixel-exact
  numerical oracle for the deliberately quantized view.
- Continuous-view leaf/butterfly numerical oracles remain strict. They explicitly
  disable shared lighting and use an internal zero-view sentinel for continuous
  reference projection. Zero is not a GUI value: normal inputs clamp to 8–512. Regression work found one-ULP color
  differences between separately compiled shading paths; both now use one shader
  shading site. Intermittent boundary/depth disagreements also prompted explicit
  `precise` projection arithmetic, preventing unrelated material changes from
  altering contraction at coverage boundaries. No tolerance was increased.
  Six consecutive leaf fixture runs passed afterwards, followed by the butterfly
  fixture. Depth failures now retain a reproducible geometry capture when reached.
- Scene images were inspected, including the coarse 8-view and dense 512-view
  endpoints. Artistic acceptance of
  angular jumps and the shared-light approximation was followed by the user's
  decision to make both policies permanent.

Artifacts: `target/model-stage-one-review/`, `target/model-stage-one/`.

## Release measurements

The current `--suite stage-one` sweeps **8/16/128/512 views × attached/fallen apples**,
with per-object lighting fixed on and default 16 views as the cadence reference.
All eight cases passed the GPU/cadence checks (`target/model-global/summary.json`).
The final hidden Release smoke confirmed `single_light=true views=16`.
A first validation attempt encountered a Wayland settings-portal timeout; the
full native suite passed under X11 (`env -u WAYLAND_DISPLAY`).

Historical slider experiment: **8/128/512 views × both lighting modes ×
attached/fallen apples**. Results for that implementation are in
`target/model-view-count/summary.json`. All twelve cases passed the reference-scene
GPU/cadence checks; measured cadence was about 58 FPS, with GPU p95 at most
13.115 ms. Increasing from 8 to 512 views increased particle tile p50 from
0.482 to 0.529 ms with shared lighting in the attached scene. This is still live
sampling, not a cache-performance result. Whole-frame results varied, so they
should not be read as a monotonic view-count cost curve.

**Historical table below:** fixed-128 toggle experiment before quantization was
made permanent. Its continuous-view/off rows require the older revision.

```sh
cargo build --release
node scripts/benchmark-model-pixels.mjs --seconds 8 --stress-leaves 256 \
  --suite stage-one --output target/model-global
```

Same reference conditions as [the tile report](model-pixel-tiles.md): RTX 3060 Ti,
2880×1620 output, 1440×810 scene, 256 rotating 16px leaves + 21 16px butterflies +
17 32px apples. Eight-second runs, nine post-warmup GPU log samples per case.
Object preparation is included in the corresponding tile-generation scope.

| Attached apples | Particle tiles p50 | Apple tiles p50 | Whole GPU frame p50 |
|---|---:|---:|---:|
| Both off | 0.626 ms | 1.495 ms | 7.546 ms |
| One light only | 0.482 ms | 1.500 ms | 7.232 ms |
| Discrete views only | 0.638 ms | 1.510 ms | 7.437 ms |
| Both on | 0.493 ms | 1.511 ms | 7.272 ms |

| Fallen apples | Particle tiles p50 | Apple tiles p50 | Whole GPU frame p50 |
|---|---:|---:|---:|
| Both off | 0.622 ms | 1.474 ms | 7.488 ms |
| One light only | 0.480 ms | 1.483 ms | 7.190 ms |
| Discrete views only | 0.630 ms | 1.489 ms | 7.497 ms |
| Both on | 0.494 ms | 1.491 ms | 7.359 ms |

Measured frame cadence remains about 58 FPS in this presentation environment.
Do not attribute every change in total frame time to these scopes: other passes
vary, and medians of separate scopes are not additive.

Single-object lighting saves roughly 0.14 ms (~23%) of particle tile generation
in this fixture; apples show no material reduction. Discrete views add a small
live-preview cost. These results do **not** support claiming that lighting alone
was the apple bottleneck or that stage-two cache performance has been measured.
Caching can still remove the repeated geometry/coverage work that stage one
intentionally retains.
