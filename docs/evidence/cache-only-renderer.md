# Cache-only renderer — worker handoff (scope 1)

Branch `agent/cache-only-pixels`, based on `3eeebc34`. Renderer implementation:
`f77d957c`; diagnostic bounds/fixture follow-up: `54633b5d`. Integration, final
aggregate tests and performance acceptance belong to the controller.

## Delivered interface

- All four adapters (particles, attached apples, fallen apples, flowers) always
  prepare object lighting/view data and read shared canonical surfaces. No frame
  adapter accepts a geometry mode, triangle stream, repair graph or reference tile.
- `model_pixel_types`, `model_cache_types`, `model_pixel_cache`, projection and
  display are geometry-free. Only baking imports the conservative geometry
  sampler. Ordinary flower stems still draw real triangles; they are not a pixel
  fallback. Flowers use a compact palette, not a geometry buffer as a palette.
- Conservative center ownership, clipping, supporting depth and 3×3 display
  overlap rules are retained. Dynamic pose, wind, relighting, transmission and
  flower head/whole A/B remain; no flower control semantics were added.
- `ModelPixelFrame`, `PipelineTopology` and ready-slot residency keep their existing
  ownership. Particle publications now contain only instances and draw indices.
  Transitive import tests discover new shader consumers and prohibit geometry
  access; Rust publication guards prohibit the removed streams/mode arguments.
- The obsolete butterfly self-shadow toggle (unused by cached rendering) is
  removed from the declaration and retired on loading old saves. Config diffs are
  exactly that deletion, not runtime settings contamination.

## Validator migration — not equivalent old tests silently passing

The old perspective/continuous native leaf, butterfly and captured-pose frame
routes are **deleted**. `validate-model-coverage-boundary.mjs` and
`validate-model-coverage-poses.mjs` are retired. Their 906 per-instance perspective
poses and old shaded-tile PNG goldens are **not** current acceptance evidence.

Useful independent coverage survives in:

1. `model_pixel_bake_validation.rs`: real baked-surface readback, actual center
   rays and producer-projected vertices, independently checked CPU geometry,
   coverage and nearest depth at two directions of every authored source per bake.
   Captured boundary/frame-identity and tiny-ray precision regression tests moved
   here, without a live render entry. The old bridge-equivalence planner is test-only.
2. `shader/tests/model_pixel_projection_evidence.slang`: standalone execution of
   actual bake clipping/coverage math, including both captured boundary cases.
3. `validate-model-cache.mjs`: exact eight-float comparisons of **all** records
   once per bank rebuild, independently checked bake geometry, and separate cheap
   frame-consumption counters. No consumer calls the generator for validation.
4. Leaf/butterfly runners: actual cached rendering, resolution/pose/A/B or
   transmission/display sweeps, positive consumption and bake geometry evidence.
   Flower phase 5 now checks 8 cache views instead of zero-view rendering.

The independent depth bound remains **0.00002**, coverage epsilon **0.0001 pixel**.
The bake oracle accounts for tile-to-NDC representation error only when checking
vertex identity, not coverage membership. A captured flower edge demonstrated
why reference closest-edge selection must retain float32 tile arithmetic: a
float64 minimizer selected another edge. A deterministic test preserves the
actual float32 contract instead of increasing the depth tolerance.

## Checks and artifacts

All GPU runs used `flock --close /tmp/re-flora-vegi-cache-controls-gpu.lock`.
No visible game, push, release, player-save edit or other-worker source edit.
Evidence root: `target/cache-only-evidence/` in this worker (copy before cleanup).

- `cargo fmt --check`, `cargo check`: passed (`fmt.log`, `check.log`).
- `cargo test model_pixel`: **39 passed, 1 existing ignored**
  (`model-pixel-tests.log`). The ignored browser-fixture experiment is not claimed.
- `cargo test tracer::butterfly_mesh`: **13 passed** (`particle-tests.log`).
- `cargo test retired_butterfly_appearance_settings`: passed, including retired
  self-shadow values (`settings-tests.log`).
- `python3 scripts/run_slang_tests.py`: **25 passed** (`slang-tests.log`).
- Hidden muted Release smoke: passed, successful shutdown, no application/Vulkan
  errors (`smoke.log`). Final smoke also enabled the bake diagnostic and checked
  all four banks. Normal non-diagnostic Release smoke passed at `f77d957c` too.
- `node scripts/validate-model-cache.mjs --seconds 30 --output
  target/cache-only-evidence/cache-matrix`: **13 phases passed**, isolated rebuilds,
  8/16/37/128/512 views, 8–64px, real fruit drops, flower A/B and resize. **14,534,528
  records bit-checked, zero mismatches; 157,288 independently checked occupied
  bake cells; maximum depth error 0.000006706**. Every phase consumed every kind.
  (`cache-matrix/summary.json`, `validation.log`, `scene.png`). The later oracle
  bounds guard only rejects wholly outside degenerate triangles; its final smoke
  and regression test passed, not a rerun of this entire matrix.
- Migrated leaf and butterfly runners, 20 seconds each: passed (`leaf-run.log`,
  `butterfly-run.log`; full artifacts under `target/leaf-model-review/` and
  `target/butterfly-resume/`). Leaf's first Wayland run hit the existing settings
  portal timeout; the unchanged assertion passed when rerun under X11.
- Flower runner, 15 seconds: passed whole/head captures, all eight species, nine
  phases, lifetime and resize (`flower-run.log`, `target/flower-native-review/`).
- Changed JavaScript syntax checks and `uvx ruff check
  scripts/validate_butterfly_mesh.py`: passed (`python-lint.log`). Global `pyright`
  is unavailable; no type-check pass is claimed.

A private target seed initially reused SPIR-V whose dependency manifest pointed
at the seed checkout. `8af1e833` adds the current canonical worktree path to the
shader artifact cache identity. The actual shaders were then rebuilt and native
runs passed; the stale-artifact failed startup is not acceptance evidence.

Generated by `cargo check` (not edited by hand):
`src/auto-generated/gpu_structs.rs`, `src/app/generated/gui_adjustables_gen.rs`.

## Remaining controller acceptance

Full Cargo suite, integrated flower control/geometry/allocation/resize checks and
Release performance comparisons remain pending. This worker makes no new timing
claim, visual approval, maximum-setting memory guarantee or cross-device claim.
The known baseline VKN retired-shader test is not fixed or masked here.
Routine DDGI HISTORY and audio chatter remain the quiet-logs worker's responsibility;
their owned blocks were not changed. Flowers must start from this worker's final
committed tip and continue using the single shared cache/retirement owner.
