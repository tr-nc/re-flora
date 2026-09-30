# Shared model assets

This directory is the **one published model/animation source** for models shared by the game and preview.
Flowers now share `parametric-flower.mjs`, `palette-mask.mjs` and the six presets in `flower-source.mjs`
between the game and web. Extra flower studies and bee/bird models remain web-only; they are not
parallel game implementations or automatically published assets.
`butterfly.glb` is read directly by Three.js `GLTFLoader` and embedded/read by
`src/model_assets.rs`. Do not export a separate game mesh or a separate table of animation angles.
The old `assets/butterfly/wing-mesh.json` pipeline has been removed.
`leaf.glb` is consumed through the same loader in both runtimes; its single authoring recipe is
`leaf-source.mjs`. The recipe retains the original 32-triangle leaf/short stem, fold and curl.
`leaf-variants.glb` is a deterministic **derived game-only shape bank**, not another authoring source: 64 meshes from the same recipe, variant 0 identical to `leaf.glb`. Every falling model leaf chooses one variant at spawn and keeps it for life.

Apples use `apple-source.mjs` as the single geometry recipe for the HTML preview and the generated game render mesh `apple-preview.json` (`node scripts/publish-apple-model.mjs`). Attached and dropped apples always use the shared pixel pipeline; the old voxel-rendering checkbox, meshes and color pipeline have been removed. Shadow passes use the same authored apple geometry. **Flora → Apple Appearance → Pixels per Apple** controls the independent 8–64 resolution (default 32) for both life states. Attached fruit uses the tree's published wind/attachment pose; dropped fruit uses its rigid-body pose. The canonical voxel collision volume, growth and drop physics are retained. Old saved copies of the retired checkbox are removed on load/next save. The HTML green-apple preset and color pickers do not affect the game.

Authoring projects may live elsewhere. The approved butterfly authoring project and provenance
remain under `experiments/butterfly-method-comparison/blender-v5/`; publish its GLB here after an
intentional art change. The old experiment GLBs are historical outputs, not active game/preview
inputs. Updating this canonical file updates both consumers (rebuild the game; reload the browser).

The shared loader supports opaque triangle meshes and node-transform animation (linear/step TRS,
quaternion slerp). Unsupported skinning/morph/cubic animation fails explicitly rather than producing
a different approximation. New assets should extend this loader once, not add a handwritten runtime
version of their geometry/animation. Butterfly flight coupling additionally requires its existing
one-second, uniformly keyed wing stroke; a changed contract must be adapted explicitly, not silently
approximated.

Consumers adapt **world placement, lighting, palette and display sampling**, not the authored mesh
or local animation. Butterfly flight coupling still owns world attitude and suppresses authored
root bob to avoid double world displacement. Its stroke integrals are now derived from the same GLB
rotation channel. Browser lights are not a substitute for in-game environment lighting.

## Publish the flower bank

The six published flowers are data presets in `flower-source.mjs` (Corn Poppy and Bellflower remain retired).
Both the native publisher and web studio call the one `parametric-flower.mjs` generator; it also
publishes the head's `socket_normal` for the analytic stem's closed attachment-plane cut. Source
hits and world-direction texel display points must remain on the inward side, rather than relying
on a thin flower center to hide an overflowing rounded stem end. See [socket evidence](../../docs/evidence/stem-flower-socket.md).
The old
radial/tulip/whole-plant geometry recipes have been removed. `flower-head.mjs` preserves the complete
attachment-local head, not one tile per petal. All preset geometry, UVs and palette-weight decoding
are shared; runtime placement/lighting/sampling remain consumer-owned.
See [native unification evidence](../../docs/evidence/unified-native-flowers.md).
The flower web studio displays complete heads and their postprocessing; it does not generate or assemble stems.
Game assembly lives in `src/flora/models.rs` and `src/flora/models/column.rs`. Neither consumer
renders stem leaves; published meshes contain only petals, centers and calyces.
`node scripts/publish-flower-models.mjs` writes `flowers.json`, embedded by `src/flora/models.rs`.
Each catalog entry owns its stable `id`, English `displayName` and integer `stemLayers`; `build.rs` generates the Rust count/registry from the published data, and the native stem and Grow list follow that registry. Commit source and derived output together. `cargo check` verifies both sources and the
publisher fingerprint. Numbers publish at nine decimal places to remove cross-platform
last-bit math noise; Node tests compare deterministic head output and Rust tests verify
native assembly, complete layers, wind bounds and independent height/head-size controls. The complete head retains its authored
geometry (including calyx), never one tile per petal. White geranium, Gillenia, the four-/five-petal
studies and custom flower remain browser-only presets, not added to the native species bank.
`palette-mask.mjs` resolves four-slot weights in Linear-sRGB to the exact sRGB8 atlas used by the web.
The published bank includes that generated atlas and vertex UVs; native caches store interpolated
UNORM12 UVs in the existing surface record and filter decoded linear texels at runtime. Do not
hand-edit the atlas/mesh JSON; change the shared preset/generator and republish.

To add a flower, give its recipe an `id`, `displayName` and `stemLayers`, publish the bank, and update `FLORA_SPECIES_COUNT` in `shader/slang/flora_types.slang`. To retire one, delete its recipe, add its stable key to `RETIRED` in `src/builder/surface/snapshot.rs`, publish, and update that same shader count. `build.rs` derives the Rust registry and count; Grow choices, stem assembly and cache shape counts follow automatically. A flower within the current single-whorl radial family is a preset operation; unsupported layered/fused/bilateral structures are explicitly accepted limitations, not a reason to add species-specific geometry. A future topology extension needs separate design and visual review. Validate historical save schemas and shader-generated layouts with `cargo check`, tests and a hidden Release run. Older research/evidence captures may still mention retired flowers; they are not live assets.

All six are plantable from **item slot 2 / Grow → the right-side Plant Brush panel**. For visual debugging, **Debug Panel → Terrain & Plants → Plant all flowers & grasses around me** places the six model flowers, Tall Grass and Short Grass in separate positions around the walking player's feet (or the edit-camera focus). It excludes the session-only climbing vine. The one-shot action changes terrain flora and persists only when the terrain snapshot is saved.
The list scrolls on short windows; the status-only backpack is hidden while Grow is active
so it cannot cover plant choices. Existing species
keys and identities are preserved when loading older gardens; model flowers occupy 4–9. **Debug → Pixel Models — Global** owns the flower pixel resolution and static view count, while **Debug → Flora → Ground Plants → Model Flowers** retains head scale, stem height and overall size controls. The former height slider is now **Stem Height Multiplier Mean**
(the saved `model_flower_height_scale` ID/value is preserved). **Stem Height Multiplier
Variance** defaults to `0.01` and **Stem Voxel Edge Scale** defaults to `1`.

Forget-me-not now uses its own five-petal preset; Cosmos retains eight petals. Sharing a **generator**
does not mean these different shapes can share a geometry bake. Each of the six current presets owns
a distinct cache family (12 whole/head sources rather than the previous 10). The cache can still share
truly identical future geometry, checked byte-for-byte including UVs, independently of palette.
The native pixel caches use separate view sliders: dynamic leaves/apples/butterflies default to 32;
static flowers default to 256. The 32-byte surface ABI is unchanged; cache format version is now 7.

Each species owns a base **integer layer count**, not a target height. A position/species
seed supplies a fixed standard-normal sample `z` per plant (independent of time, draw slot
and size controls). With mean `m` and variance `v`:

```text
maximum layers = max(1, round(base layers × (m + 3 × sqrt(v))))
plant layers   = clamp(round(base layers × (m + sqrt(v) × z)), 1, maximum layers)
voxel edge     = 0.05 × voxel edge scale
stem height    = plant layers × voxel edge
```

Variance is **not standard deviation**. Rounding, the one-layer minimum and upper
three-standard-deviation safety cap mean the final discrete/clamped heights are only
approximately Gaussian. At variance zero, same-species plants have equal layer counts.
Reducing voxel size never adds layers to compensate; the stem becomes shorter. Mean
changes layer counts, never stretches cubes. Existing global size/growth still apply to
the whole plant. The head follows the actual tip without being resized by these controls.

GPU instances select a prefix of a bounded, closed-cell source mesh and re-bend its cell
centers without stretching. At most 301 cells per plant are allocated under supported
settings. Cache generations carry the matching distribution and draw counts; large
variance increases vertex work, not per-instance flower-head baking.

**Stem Bottom Color** and **Stem Tip Color** are declarative saved
settings, independent of grass and calyx colors. The native stem interpolates between them
along its undeformed height, so wind and overall scaling do not slide the gradient. There
is no whole-plant pixelization switch. The opt-in **Pixel Sampling — Flower Stems** experiment adds a separate
original/continuous/direction/surface comparison; unchecked preserves the voxel-stem rules below.
See [stem sampling evidence](../../docs/evidence/stem-sampling-experiment.md).

- Exactly one cube occupies each horizontal layer of the stalk. Its baseline edge is `0.05`
  (half a grass edge at full growth and overall scale 1). Adjacent cells share positive face
  area; no lateral filling or branches are generated. Global size/growth still scale the plant.
- Native cells retain all six faces, including caps hidden at rest: wind may expose new
  parts of those caps. Internal faces stay occluded inside the opaque cell volumes.
- Mean/variance edits regenerate the bounded layer bank rather than stretch cubes. The CPU source generation
  owns the resulting triangle ranges and draw counts; bounded index capacity covers the full
  saved height range. There are no stem leaflets or leaf-attachment transforms.
- Wind uses the existing vegetation response, but translates whole cells in XZ
  instead of tilting the entire stack. Smoothstep displacement is bounded to one quarter of
  stem height so the saved height range retains overlapping cell footprints. The terminal head
  follows the same tip displacement. Culling includes that bound; this is not a new solver.
- **The calyx belongs to the complete flower object**, together with petals and center, and is
  pixelated with that head. It is never constrained by the stalk's one-voxel-per-layer rule.
`flower_model.slang` adapts pose/materials to the existing sampling/projection/display modules;
`ModelPixelFrame` owns compute/draw pairing and frame-slot storage, and `PipelineTopology`
owns descriptor retirement. Native voxel stems and depth-bearing heads share the game depth
attachment and environment lighting. These flowers add no dedicated collision or shadow-caster
pass. Browser shape/color/weight-map edits remain temporary until intentionally copied to the shared preset and published.
The shared model deliberately supports a single radial whorl with identical petals and a simple
center. Tulips are cup approximations; layered roses, fused bell corollas and orchid-specific organ
roles are not supported. Limits are documented in the generator comments; do not resurrect old
species branches to disguise them.

See [the study and native screenshots](../../docs/research/stylized-flower-model-study.md)
and `node scripts/validate-flower-models.mjs --help` for the bounded real-game checks.
Large-population Release performance acceptance remains separate from this visual candidate.

## Publish a leaf shape

Edit `leaf-source.mjs` (including `leafDefaults` for approved width/length/fold/curl), then run:

```sh
node scripts/publish-leaf-model.mjs
```

Commit the recipe and both regenerated GLBs together. `cargo check` fails if either published asset's
source fingerprint no longer matches the recipe/publisher. Node tests compare both complete GLBs
to deterministic regeneration. Never hand-edit generated GLB bytes. Games embed these bytes at
build time; no Node/browser/runtime file loading is required in a packaged game.

The browser's shape sliders remain **temporary art experiments**, not automatic edits to committed
assets. The initial approved shape loads from `leaf.glb`; publishing a changed recipe updates
it and the game shape bank. The bank spans width 0.78–1.22, length 0.82–1.16, widest-point position
0.32–0.70, fold 0.12–0.52, and curl -0.45–0.90 (ranges are sampled, not endpoints).
A deterministic per-life seed from the particle slot, generation and spawn seed chooses the bank
index; recycling a slot does not force its next leaf to retain the old shape. Browser color presets change only the four preview color pickers; they do not publish a game palette or modify geometry. Curl rotates the midrib's tangent, shortening its Y projection while preserving its arc length; the length slider scales that arc separately.

## In-game falling leaf A/B

Debug → **Falling Leaves**:

- `Shared 3D Leaf (B; Unchecked = Original Sprite)` — default off. A keeps the existing sprite;
  B displays the shared curved mesh. This is a render-only switch, not a flight-mode switch.
- `Falling Leaf Display Size` — saved **0.25×–4×, default 1×** multiplier, applied to both A/B
  displays. It does not change aerodynamic size, falling speed, rotation, collision or lifetime;
  butterflies and non-falling leaf-colored debris are unaffected.
- `Pixels per Falling Leaf` — separate saved control, **8–64, default 16**, matching the current
  butterfly control's range/default, without sharing its live value.

The source leaf lies in local XY with +Z as its leaf normal. The renderer uses the existing published
`leaf_orientation` quaternion and the stable per-life shape seed, with the existing position, physical size, color, lifetime
and alpha. The display-size multiplier is applied only when encoding render instances.
It does not add an Euler offset, face the model toward the camera, reset flight, or resample leaf
motion at butterfly FPS. `LeafFlight` and its angle-dependent falling/rotation equations are unchanged.
Non-falling leaf-colored particles without that physical pose remain on the existing sprite path.

Game lighting/palette/transmission remain the game's leaf lighting, not the preview's studio shader.
The mesh recipe, vertex normals and UVs are shared. **Conservative projected coverage and the
resulting eight-neighbor connectivity contract apply to the shared game models**; the HTML preview offers a coverage checkbox for visual A/B against its old bridge-only mode. There is no game repair toggle. The existing leaf A/B
still compares the original sprite against the shared model, not two repair algorithms.

Butterflies, falling leaves, apples and model flowers all call `sampleModelPixelGeometry` in
`shader/slang/model_pixel_surface.slang` from compute, then use the same
`model_pixel_display.slang` screen-fragment lookup. The shared
`model_pixel_projection.slang` now supplies fixed orthographic framing; the global
A/B chooses rotating pixels or screen-aligned resampling of the same tile. See
[`model-pixel-orthographic-preview.md`](../../docs/performance/model-pixel-orthographic-preview.md).
Adapters own only mesh ranges, poses and
material shading; they do not own coverage or screen-fragment ray loops. Some particle binding/type
names retain the historical `butterfly` prefix. Leaves upload one shared 64-shape bank plus per-particle
pose/shape index. Visible particle tiles are packed at their own resolution into at-most-64-MiB batches,
not preallocated at 64² for every one of 16K slots. Frame-slot-owned buffers are retired after unused
batches/trees disappear; there is no permanent cache for every previously created tree. Pixel depths are
actual mesh hit depths for original samples; additions use the nearest supporting projected
geometry's depth for world occlusion. The pixel grid stays fixed for an animal's/leaf's world
footprint, not fitted to each rotating silhouette. Large-population performance acceptance remains a
separate Release measurement after visual review.

## Fixed geometric coverage and connectivity

`src/tracer/model_pixel_repair.rs` mirrors the preview's geometry-supported conservative footprint
and eight-neighbor shortest-bridge/pruning stages. A leaf is one repair group; butterfly wings remain
separate. Diagonal contact needs no bridge, but a projected silhouette can still gain pixels even
when its original mask was connected. Fully unsampled geometry is recovered when its opaque projected
triangles cover the tile. Conservative coverage can thicken silhouettes; it does not guarantee a
connection through occluders.

The production GPU generator projects the current pose and classifies centers after the camera is
final. All models use the same nearest-hit, clipping, conservative cell-overlap and depth rules.
Existing center hits always win. Unoccupied covered cells use their supporting triangle's material
and projected depth. The final conservative-coverage pass supersedes every visible interpolated
bridge expression in the legacy planner; the common GPU implementation computes that final result
without evaluating the dead intermediate graph. Equivalence tests and native CPU/GPU checks retain
the full planner as the oracle. Lighting is evaluated afresh on the GPU.
There is **no production GPU image readback** and no flight/pose/timing modification.

Only explicit native review runs allocate separate center-only reference tiles and exact GPU-ray
captures (never displayed). They compare original RGBA/depth bit-for-bit, reconstruct the full CPU
coverage/bridge oracle from independently validated center ownership, verify coverage depth, and
sample rotating poses periodically. CPU intersection tests use the captured GPU rays, separately
checking their transform against the camera/pose, rather than guessing unprojection cancellation.
Conservative coverage may create a visible component that had no center samples; this is expected,
not a new island bug. Diagnostic floating-point allowances never expand rendered geometry.
Different camera/lighting adapters mean game images are not promised byte-identical to browser images.

The checked-in user GUI values are currently model B, 4x size and 22 pixels; the initial control
contracts above remain independent of these saved choices.

## Run / verify

```sh
node scripts/serve-model-preview.mjs
# http://127.0.0.1:8765/model-preview/
node --test experiments/model-preview/tests/*.test.mjs
# Playwright required (NODE_PATH may point at an existing installation):
node experiments/model-preview/tests/asset-parity.cjs
cargo test browser_model_pose_parity -- --ignored --nocapture
node experiments/model-preview/tests/repair-parity.cjs
cargo test browser_repair_plan_parity -- --ignored --nocapture
# Native Vulkan checks, hidden and muted; preserve saved GUI settings:
node scripts/validate-leaf-model.mjs
python3 scripts/validate_butterfly_mesh.py --seconds 12
node scripts/validate-apple-model.mjs --seconds 12
node scripts/validate-flower-models.mjs
node scripts/validate-flower-snapshots.mjs
# Explicit Release performance matrix; temporarily edits/restores GUI config.
# Close other game instances first. See --help for dimensions and acceptance.
cargo build --release
node scripts/benchmark-model-pixels.mjs --seconds 8
```

Current convergence architecture, measured before/after results and scope limitations:
[`docs/performance/model-pixel-tiles.md`](../../docs/performance/model-pixel-tiles.md).

The repair oracle compares 140 WebGL/JS masks across both models, views, resolutions and animation
phases against the native planner's final additions, including entirely missed features.
The asset parity check compares actual Three.js world-space vertices to the game's loader at 102 clip
times including wrap for both models, with tolerance 2e-6 model units. It is an explicit integration check, not part
of ordinary fast Cargo tests. Also run the viewer browser suite and the game's hidden Release smoke
when changing a shared asset or loader.

Validated in this worktree: full Cargo tests, 16 Node tests, browser interaction checks, both-model
pose parity, normal hidden Release startup, live leaf A→B(8/16/64)→A→B switching with 8 distinct
stable shape variants, and display-size
sweeps at 0.25×/1×/2×/4×, and the existing
butterfly 8/22/64px/shadow/transmission sweep. The leaf GPU check used 8 production flight particles;
maximum checked depth difference was below 0.0000005, with no Vulkan validation errors or saved-config
changes. The earlier connectivity-only regression counted 85 leaf and 20 butterfly additions;
those historical counts no longer apply after general projected coverage. Current GPU review checks
preserve original RGBA/depth bit-for-bit and validate model-shaded coverage seeds and bridge colors. `target/leaf-model-review/` contains the run log,
actual game screenshot and native pixel images. This is correctness/visual evidence, not a
large-population performance approval.

A short normal-production Release fixture (8 live particles, saved 22px/4x leaf settings, no GPU
review readback) logged frame-time median/p95 of 10.95/13.22 ms before and 8.54/9.04 ms after, using
16/18 logged samples after frame 100. Logs: `target/connectivity-baseline.log` and
`target/connectivity-fixed.log`. These separate short runs are only a smoke comparison, **not**
evidence of a speedup or a representative-population performance acceptance.
