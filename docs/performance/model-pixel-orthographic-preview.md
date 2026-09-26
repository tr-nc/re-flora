# Shared orthographic tiles with rotating pixels

The user selected **A: Rotating Pixels** after the `e10f9105` A/B experiment.
It is now the only production display mode. B's screen-grid resampling, overlap
gather, checkbox and app/render/GPU option field have been removed. Old saved
`model_pixel_screen_grid` values are retired, whether true or false.

This is still a cache-compatible live preview, **not startup baking or an atlas**.
Butterflies, 3D falling leaves, attached apples and fallen apples all use the same
geometry projection and display implementation. Physics, growth, tree wind,
instance poses and mesh shadow casting remain unchanged.

## Controls

**Pixel Models — Global** contains only **Discrete View Count**: shared, default
**16**, range 8–512. Canonical square pixels rotate with the object's screen roll.
Per-object lighting is permanent. Resolution stays under Butterflies, Falling
Leaves and Apple Appearance (one apple resolution covers attached/fallen states).
This is not a whole-screen pixelation pass or a globally aligned pixel grid.

## Deep module and data contract

`shader/slang/model_pixel_projection.slang` owns the geometric seam:

- A model adapter supplies triangles, its published physical frame, pivot,
  fixed framing radius, selected canonical view and resolution.
- `sampleOrthographicModelPixel` maps this into a fixed [-1,1] orthographic cube,
  then calls the existing shared center/coverage sampler. No camera distance,
  camera roll or fitted silhouette enters the bake camera.
- `modelOrthographicQuad` rotates and places the tile;
  `modelOrthographicDepth` converts normalized per-pixel depth to scene depth.

`model_pixel_object.slang` prepares object light and the canonical view basis.
Each view has a deterministic model-local up axis. The published rigid pose
transforms that basis into world space. The same selected direction determines
screen roll via shortest-arc transport, including transport onto the camera plane
for off-axis instances. An apple can rotate through 360 degrees while retaining
one view key, or change view keys as it tumbles. It is not locked upright.

`model_pixel_display.slang` owns tile lookup, opacity and depth reconstruction.
It neither resamples a rotated image onto a new grid nor traverses triangles.
Materials remain model adapters; callers do not duplicate projection rules.

GPU resources use existing frame-slot-safe tile storage and bounded particle
batches. The object buffer remains four float4s: light, canonical world X + roll
cosine, Y + roll sine, and Z. Compute binds it read/write; vertex consumers use the
**NonWritable** declaration in `model_pixel_view_data.slang`. Transient draw
descriptors bind the same object allocation used by that batch's compute prepass.
No vertex/fragment storage-write feature is required.

This seam supports moving geometry generation to startup later. A future
persistent result must contain material/coverage, normals and canonical depth,
not today's instance-lit RGBA. Its key needs model/variant, view count/index,
resolution, animation state and bake-rule version. Offline loading can later
supply that result without changing placement/display semantics. No on-disk
format or speculative loader framework has been introduced.

## Intentional approximations

- The model image has no internal perspective distortion. The scene camera still
  places/scales the billboard by center distance; the world remains perspective.
- Fixed framing does not fit each pose. Authored apples, all 64 leaf variants and
  sampled butterfly poses fit the fixed spherical frame.
- Normals/light shading use the real pose. Depth uses the orthographic surface,
  not a flat quad. Both approximate a perspective mesh, especially close up or
  at coarse view counts. Geometry coverage remains conservative.
- Butterfly articulation remains continuous/live. Animation-frame caching is
  still future work.

## Validation

```sh
cargo fmt --check
cargo check
cargo test
env -u WAYLAND_DISPLAY node scripts/validate-apple-model.mjs --seconds 12 --stage-one
env -u WAYLAND_DISPLAY node scripts/validate-leaf-model.mjs --seconds 9
env -u WAYLAND_DISPLAY python3 scripts/validate_butterfly_mesh.py --seconds 12
env -u WAYLAND_DISPLAY cargo run --release -- --hidden --mute --auto-exit 0.5
```

1102 Rust tests passed, four ignored. The two B-only overlap/resampling tests were
removed. Remaining projection tests cover pose/scale/translation invariance, full
roll at one view key, parallel rays/depth reconstruction, authored framing and
shared adapter integration. Saved-setting migration tests cover both values of
the retired checkbox. The live fixture cycles 8/16/37/128/512 views, independent
leaf/butterfly resolutions (8/64, 16/8, 64/16), apple 8/32/64px, real fruit drops and
native resizes. Final runs had no saved config changes or Vulkan errors.

The old continuous-view numerical oracles retain their internal zero-view
sentinel. They are not a pixel-exact GPU/CPU oracle for the orthographic mode.

The rotating-only `--suite apples --stress-leaves 256 --seconds 8` run is saved in
`target/model-rotating-only/`. Its default-32px GPU/cadence checks passed. The
fallen 64px case measured GPU p50 8.814 ms but only ~45 FPS actual cadence; the
script's default-resolution acceptance must not be read as a 64px cadence pass
or as evidence of a speedup from deleting B.

## Historical experiment

At `e10f9105`, 256 rotating 16px leaves + 21 16px butterflies + 17 32px apples,
16 views, RTX 3060 Ti, 2880×1620 output / 1440×810 scene:

| Scene | A GPU p50 | B GPU p50 | Measured cadence |
|---|---:|---:|---:|
| Attached | 7.351 ms | 7.243 ms | ~58 FPS |
| Fallen | 7.305 ms | 7.315 ms | ~58 FPS |

These were eight-second Release cases, nine post-warmup samples each, not cache
measurements. Historical artifacts: `target/model-orthographic-display/`.
The retired `--suite display` requires that older checkout. Current benchmarks
use `--suite apples` or `--suite stage-one`; both use rotating pixels only.
