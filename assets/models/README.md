# Shared model assets

This directory is the **one published model/animation source** for the game and the model preview.
`butterfly.glb` is read directly by Three.js `GLTFLoader` and embedded/read by
`src/model_assets.rs`. Do not export a separate game mesh or a separate table of animation angles.
The old `assets/butterfly/wing-mesh.json` pipeline has been removed.

Authoring projects may live elsewhere. The approved butterfly authoring project and provenance
remain under `experiments/butterfly-method-comparison/blender-v5/`; publish its GLB here after an
intentional art change. The old experiment GLBs are historical outputs, not active game/preview
inputs. Updating this canonical file updates both consumers (rebuild the game; reload the browser).

The shared loader supports opaque triangle meshes and node-transform animation (linear/step TRS,
quaternion slerp). Unsupported skinning/morph/cubic animation fails explicitly rather than producing
a different approximation. New assets should extend this loader once, not add a handwritten runtime
version of their geometry/animation.

Consumers adapt **world placement, lighting, palette and display sampling**, not the authored mesh
or local animation. Butterfly flight coupling still owns world attitude and suppresses authored
root bob to avoid double world displacement. Its stroke integrals are now derived from the same GLB
rotation channel. Browser lights are not a substitute for in-game environment lighting.

## Run / verify

```sh
node scripts/serve-model-preview.mjs
# http://127.0.0.1:8765/model-preview/
node --test experiments/model-preview/tests/*.test.mjs
# Playwright required (NODE_PATH may point at an existing installation):
node experiments/model-preview/tests/asset-parity.cjs
cargo test browser_model_pose_parity -- --ignored --nocapture
```

The parity check compares actual Three.js world-space vertices to the game's loader at 102 clip
times including wrap, with tolerance 2e-6 model units. It is an explicit integration check, not part
of ordinary fast Cargo tests. Also run the viewer browser suite and the game's hidden Release smoke
when changing a shared asset or loader.
