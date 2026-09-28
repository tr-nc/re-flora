# Flower-head preview platform and game-owned stems

## Responsibilities

- The browser displays one complete flower head (petals, center, calyx) and its
  postprocessing. It has no stem/leaf meshes, height/bend/leaf controls, stem
  metadata or imports of the removed JS stem/assembly modules.
- `assets/models/flower-head.mjs` extracts the complete head from the legacy
  authoring recipes and normalizes its attachment to the origin. It does not
  assemble a plant. The native publisher emits only these head meshes; the
  legacy recipes are not themselves renderable/published plant assets.
- Rust owns all plant assembly, including the eight species' original stem
  dimensions, complete voxel layers, head placement and bounded wind motion.
  Leaf attachment transforms and the separate leaf/stem triangle accounting
  have been removed. No additional native species were introduced.
- Calyx geometry remains part of the pixelated head even where its legacy
  authoring material is named `stemColor`. The browser labels it "花萼颜色".

## Saved native stem palette

Debug → Flora → Ground Plants → Model Flowers:

- **Stem Bottom Color** (`model_flower_stem_bottom_color`)
- **Stem Tip Color** (`model_flower_stem_tip_color`)

Both use the declarative config and existing save/load traversal. They default
respectively to `#364B00` and `#97C200`; grass and calyx colors are independent.
The shader interpolates at undeformed stem height, so shape, growth and wind
keep the gradient attached to the stalk. Changing these colors updates the
native uniform, not the cached flower-head source or its material.

## Validation

- `cargo fmt --check`, `cargo check`.
- Full `cargo test`: 1,248 main tests + 4 auxiliary tests passed, 4 ignored.
  Includes all declared settings' actual save/reload and renderer-input mapping.
  Homebrew Python selected with `PATH=/opt/homebrew/bin:$PATH` for `tomllib`.
- Flower-model tests verify no stem leaves, one terminal head, full cube layers,
  height/head-scale independence, containment and adjacent-cell wind contact.
- `node --test experiments/model-preview/tests/*.test.mjs`: 28 passed. Published
  native output is canonical and head-only; all ten preview flowers preserve
  complete calyx geometry and source immutability.
- `head-platform-browser.cjs`, `flowers-browser.cjs` and `browser.cjs` passed:
  no assembly controls/imports, reset, GPU disposal, model switching, projections,
  220 head-tile poses, depth occlusion, transparency and mobile layout.
- `node scripts/validate-flower-models.mjs --seconds 10` passed eight-species
  rendering, nine lifecycle/resolution/view phases, deletion/replanting and resize.
  Native screenshot `target/flower-native-review/b.png` shows leafless stalks.
- `RE_FLORA_FLOWER_MODEL_REVIEW=controls cargo run --release -- --hidden --mute
  --auto-exit 12` reached phases 0–20. Phases 19/20 reverse blue/red stem endpoints
  through the real saved-field inputs in memory, without saving the fixture.
  Logs report `assembly=native heads=1 calyx=head leaves=0`, both palette phases,
  `failures=0`, and successful shutdown without Vulkan validation errors.
- Browser screenshot: `target/flower-study/head-platform.png`; canonical page
  refreshed at `http://127.0.0.1:8765/model-preview/?model=gillenia`.

Generated files: `assets/models/flowers.json`, GUI adjustables and GPU structs.
No existing saved settings were changed; config changes only declare the two new
colors. No visible game was launched. These checks are not a large-population
performance benchmark or a substitute for user visual approval.
