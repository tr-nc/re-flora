# Shared orthographic tiles: rotating pixels vs screen-grid resampling

This is the cache-compatible visual preview, **not startup baking or an atlas**.
Butterflies, 3D falling leaves, attached apples and fallen apples all use the same
geometry projection and display implementation. Physics, growth, tree wind,
instance poses and mesh shadow casting remain unchanged.

## Try it

**Pixel Models — Global**:

- **Discrete View Count**: shared, default **16**, range 8–512.
- **B: Screen-aligned Pixel Grid (unchecked: A, Rotating Pixels)**: saved boolean,
  default **false**. Switch at runtime; it does not change the generated tile.

A rotates the canonical tile and its square pixels with the object's screen roll.
B resamples that very same tile onto an object-local, screen-aligned N×N grid.
It is not a whole-screen pixelation pass or a globally phase-locked pixel grid.
Resolution remains independent under Butterflies, Falling Leaves and Apple
Appearance (one apple resolution covers attached/fallen states). Object lighting
is always enabled. Old configs gain the A/B setting with A selected.

Both modes are orthographic. The retired perspective tile generator is not the
unchecked comparison: the comparison is **A vs B under the same bake rules**.

## Deep module and data contract

`shader/slang/model_pixel_projection.slang` owns the geometric seam:

- A model adapter supplies triangles, its published physical frame, pivot,
  fixed framing radius, selected canonical view and resolution.
- `sampleOrthographicModelPixel` maps this into a fixed [-1,1] orthographic cube,
  then calls the existing shared center/coverage sampler. No camera distance,
  camera roll, fitted silhouette or A/B flag enters the bake camera.
- `modelOrthographicQuad` places the tile; `modelOrthographicDepth` converts its
  normalized per-pixel depth back to scene depth.

`model_pixel_object.slang` prepares the object light and canonical view basis.
Each view has a deterministic model-local up axis. The published rigid pose
transforms that basis into world space. The same selected direction determines
screen roll via shortest-arc transport, including transport onto the camera plane
for off-axis instances. An apple can rotate through 360 degrees while retaining
one view key, or change view keys as it tumbles; it is not locked to a billboard's
upright orientation. The two transports define this preview's roll convention.

`model_pixel_display.slang` owns both display modes, including depth and opacity.
Neither display path traverses triangles. Materials remain model adapters;
particle/apple callers do not implement separate orthographic projection rules.

GPU resource ownership stays with the renderer's existing frame-slot-safe tile
storage and bounded particle batches. The object buffer remains four float4s:
light, canonical world X + roll cosine, Y + roll sine, and Z. Compute binds it
read/write; graphics has a separate **NonWritable** declaration in
`model_pixel_view_data.slang`, with exactly one declaration per buffer in each
pipeline. Transient draw descriptors bind the same object allocation used by that
batch's compute prepass. No vertex/fragment storage-write feature is required.

This seam is suitable for moving geometry generation to startup later. A future
persistent result must contain material/coverage, normals and canonical depth,
not today's instance-lit RGBA. Its key needs model/variant, view count/index,
resolution, animation state, and a version of these bake rules. Offline loading
can later supply that result without changing placement/display semantics. No
speculative loader/plugin framework or on-disk format is introduced now.

## Visual differences and limits

- The model image has no internal perspective distortion. The **scene camera**
  still places and scales the billboard by its center distance; the world has not
  become an orthographic scene.
- Both modes use the same fixed framing radius and nominal pixel pitch. Authored
  apple geometry, the 64 leaf variants, and sampled butterfly poses fit the fixed
  spherical frame, allowing screen rotation without fitting a new rectangle.
- B uses the source center sample first. Only empty cells gather overlapping
  source cells in a bounded 3×3 neighborhood, choosing nearest canonical depth.
  Positive-area square overlap prevents thin parts disappearing; edge-only
  contact is rejected, so zero roll does not artificially dilate the grid.
- **B can thicken/jitter low-resolution silhouettes** because it conservatively
  resamples already conservative source pixels. This is an artistic comparison,
  not a promise of identical contours or equal occupied-pixel counts.
- Normals/light shading use the real pose. Depth uses the orthographic billboard's
  normalized surface depth, not a flat quad. Both remain approximations of a
  perspective mesh, especially close to the camera or at coarse view counts.
- Butterfly articulation is still continuous/live. An animation-frame cache and
  its additional state dimensions have not been implemented.

## Validation and measurements

```sh
cargo fmt --check
cargo check
cargo test
env -u WAYLAND_DISPLAY node scripts/validate-apple-model.mjs --seconds 12 --stage-one
env -u WAYLAND_DISPLAY node scripts/validate-leaf-model.mjs --seconds 9
env -u WAYLAND_DISPLAY python3 scripts/validate_butterfly_mesh.py --seconds 12
node scripts/benchmark-model-pixels.mjs --seconds 8 --stress-leaves 256 \
  --suite display --output target/model-orthographic-display
env -u WAYLAND_DISPLAY cargo run --release -- --hidden --mute --auto-exit 0.5
```

1104 Rust tests passed, four ignored. Seven new deterministic projection tests
cover pose/scale/translation invariance, full screen roll at one view key, parallel
rays/depth reconstruction, zero-roll resampling, bounded overlap gathering,
authored framing and shared adapter integration. GUI round trips cover the saved
checkbox. The live fixture cycles **both modes at 8/16/37/128/512 views**, independent
leaf/butterfly resolutions (8/64, 16/8, 64/16), apple 8/32/64px, real fruit drops
and native resizes. No saved config changes or Vulkan errors in the final runs.

The old strict continuous-view numerical oracles still run with their internal
zero-view sentinel; they do **not** establish pixel-exact correctness of the new
orthographic modes. New math tests, native live runs and inspected screenshots
are complementary evidence, not a new GPU-vs-CPU orthographic pixel oracle.

RTX 3060 Ti; 2880×1620 output / 1440×810 scene; 256 rotating 16px leaves,
21 16px butterflies and 17 32px apples; 16 views; eight-second Release cases with
nine post-warmup GPU samples each:

| Scene | A GPU p50 | B GPU p50 | Measured cadence |
|---|---:|---:|---:|
| Attached | 7.351 ms | 7.243 ms | ~58 FPS |
| Fallen | 7.305 ms | 7.315 ms | ~58 FPS |

All four benchmark budget/cadence checks passed. These short runs do not establish
that B is faster, and are **not cache-performance measurements**. Screenshots and
logs are under `target/model-orthographic-display/`; live switch validation is
under `target/model-stage-one-review/`. Visual acceptance remains the user's next
step before startup-cache implementation.
