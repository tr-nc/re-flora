# Shared model assets

This directory is the **one published model/animation source** for the game and the model preview.
`butterfly.glb` is read directly by Three.js `GLTFLoader` and embedded/read by
`src/model_assets.rs`. Do not export a separate game mesh or a separate table of animation angles.
The old `assets/butterfly/wing-mesh.json` pipeline has been removed.
`leaf.glb` is consumed through the same loader in both runtimes; its single authoring recipe is
`leaf-source.mjs`. The recipe retains the original 32-triangle leaf/short stem, fold and curl.

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

## Publish a leaf shape

Edit `leaf-source.mjs` (including `leafDefaults` for approved width/fold/curl), then run:

```sh
node scripts/publish-leaf-model.mjs
```

Commit the recipe and regenerated `leaf.glb` together. `cargo check` fails if the published asset's
source fingerprint no longer matches the recipe/publisher. Node tests also compare the entire GLB
to a deterministic regeneration. Never hand-edit generated GLB bytes. Games embed these bytes at
build time; no Node/browser/runtime file loading is required in a packaged game.

The browser's shape sliders remain **temporary art experiments**, not automatic edits to committed
assets. The initial approved shape loads from the shared GLB; publishing a changed recipe updates
both consumers. Browser JSON presets and shared-asset publication are deliberately different actions.

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
`leaf_orientation` quaternion directly, with the existing position, physical size, color, lifetime
and alpha. The display-size multiplier is applied only when encoding render instances.
It does not add an Euler offset, face the model toward the camera, reset flight, or resample leaf
motion at butterfly FPS. `LeafFlight` and its angle-dependent falling/rotation equations are unchanged.
Non-falling leaf-colored particles without that physical pose remain on the existing sprite path.

Game lighting/palette/transmission remain the game's leaf lighting, not the preview's studio shader.
The mesh, vertex normals and UVs are shared. Browser eight-neighbor repair is **not** enabled in game;
this A/B compares the old sprite against the new model, not two repair algorithms.

Both models use the existing shared particle-model draw pipeline (some internal binding/type names
still carry the historical `butterfly` prefix). Butterflies cache their N×N tiles; leaves sample a
virtual N×N grid in the fragment shader using the same ray/shading function. Leaves upload one shared
mesh plus per-particle pose, not a 64² allocation for every one of 16K particle slots. Pixel depths are
actual mesh hit depths for world occlusion. The pixel grid stays fixed for an animal's/leaf's world
footprint, not fitted to each rotating silhouette. Large-population performance acceptance remains a
separate Release measurement after visual review.

## Run / verify

```sh
node scripts/serve-model-preview.mjs
# http://127.0.0.1:8765/model-preview/
node --test experiments/model-preview/tests/*.test.mjs
# Playwright required (NODE_PATH may point at an existing installation):
node experiments/model-preview/tests/asset-parity.cjs
cargo test browser_model_pose_parity -- --ignored --nocapture
# Native Vulkan checks, hidden and muted; preserve saved GUI settings:
node scripts/validate-leaf-model.mjs
python3 scripts/validate_butterfly_mesh.py --seconds 12
```

The parity check compares actual Three.js world-space vertices to the game's loader at 102 clip
times including wrap for both models, with tolerance 2e-6 model units. It is an explicit integration check, not part
of ordinary fast Cargo tests. Also run the viewer browser suite and the game's hidden Release smoke
when changing a shared asset or loader.

Validated in this worktree: full Cargo tests, 11 Node tests, browser interaction checks, both-model
pose parity, normal hidden Release startup, live leaf A→B(8/16/64)→A→B switching and display-size
sweeps at 0.25×/1×/2×/4×, and the existing
butterfly 8/22/64px/shadow/transmission sweep. The leaf GPU check used 8 production flight particles;
maximum checked depth difference was below 0.0000005, with no Vulkan validation errors or saved-config
changes. `target/leaf-model-review/` contains the run log, actual game screenshot and native pixel
images. This is correctness/visual evidence, not a large-population performance approval.
