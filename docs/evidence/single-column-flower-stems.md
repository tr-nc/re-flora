# Native single-column flower stems

## Delivered scope

The game's existing eight model flowers (species 4–11) now use the selected
single-column topology: no forks, one complete terminal flower, one cube per
horizontal stalk layer. White geranium and Gillenia remain browser-only.

- `assets/models/flower-topology.mjs` and `flower-stem.mjs` are shared by the
  preview and native publisher. `flowers.json` is regenerated, not hand-edited.
- A cell edge is `0.05` authoring units, or half a grass voxel at full growth and
  overall scale 1. Global size and growth retain their existing scaling behavior.
- Height settings rebuild whole layers, not stretched cuboids. The Rust generator
  is tested against the published JS geometry. Leaves retain their geometry and
  move with their main-stem attachments.
- The calyx stays in the complete head range with the petals and center. Stem
  color does not imply stem ownership. No new A/B control is introduced.
- GPU wind uses the existing vegetation response with bounded horizontal
  attachment translation instead of whole-plant tilt. Cell Y intervals stay
  disjoint; adjacent footprints overlap throughout the supported height range.
  Culling includes the maximum quarter-height displacement.
- Moving native cubes retain **all six faces**: wind can expose cap regions that
  were internal at rest. Internal faces are hidden within the opaque cell volumes.
  The static browser mesh can omit those faces without changing external bounds.
- Current immutable cache generations own the changing source ranges and draw
  counts. Vertex/index capacity covers the maximum saved height; draw recording
  checks that capacity before submitting.

## Validation

```sh
cargo fmt --check
cargo check
PATH=/opt/homebrew/bin:$PATH cargo test
node --test experiments/model-preview/tests/*.test.mjs
node experiments/model-preview/tests/stems-browser.cjs
node experiments/model-preview/tests/flowers-browser.cjs
node scripts/validate-flower-models.mjs --seconds 10
RE_FLORA_FLOWER_MODEL_REVIEW=controls cargo run --release -- --hidden --mute --auto-exit 8
cargo run --release -- --tail-latest-log 100
```

The browser scripts use the installed Playwright via `NODE_PATH` and macOS Chrome
via `CHROME_EXECUTABLE`. Homebrew Python is selected for `tomllib`, required by an
existing lighting acceptance test.

- Rust main suite: 1,248 passed, 4 ignored; collision benchmark logic: 4 passed.
- Node suite: 30 passed, including canonical native publication, exact single-layer
  cross-sections, complete moving-cell caps and native-source immutability.
- Both browser flower suites passed for ten preview flowers; 220 head-tile poses,
  eight depth-occlusion fixtures, reset, resource lifetime and mobile layout checks.
- Native validator passed eight-species coverage, nine lifecycle/view/resolution
  phases, removal/replanting and swapchain resize, with clean Vulkan diagnostics.
  Its legacy `a`/`b` capture labels now both use the fixed selected renderer.
- Native controls run reached phases 0–17, including height/head scales 0.25–4,
  resolutions 8/32/64, view counts 8/16/37/512 and overall scale 2. Shutdown logged
  `failures=0` and `Application exited successfully`.
- The native validator verified `config/gui.toml` was not modified.

Artifacts are local to this worktree: `target/flower-native-review/summary.json`,
`b.png`, the sibling validator logs, and the app logs under `target/re-flora-logs`.

These are correctness/integration checks, not large-population performance
acceptance or user visual approval. No visible game session was launched.
