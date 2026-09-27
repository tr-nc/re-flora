# Shared model-surface cache (restored)

The shared cache removed in `6bae0d62` is restored from `f7b7e036`, adapted to the
current `ModelPixelFrame` ownership boundary and current coverage algorithms.
**All production model-pixel consumers use it**: all 64 model-leaf variants,
attached/fallen apples sharing one shape, 32 articulated butterfly frames, and
all eight flowers (8 whole-plant + 13 complete-head sources). Ordinary voxel/tree
rendering and non-model sprite paths are unchanged; retired tree experiments are
not restored.

## Contract

- Immutable sources are baked at startup/first render for the requested resolutions
  and actual view count. Normal rendering reads shared surfaces, never re-traverses
  triangles per instance and never silently falls back to live geometry.
- Camera motion, instance count, growth/scale, root wind, lighting, flower A/B and
  screen-grid/rotating display do not rebuild surfaces. Changing one resolution
  rebuilds only its bank; changing view count rebuilds all four banks.
- Each 32-byte surface stores model-local position + canonical depth and normal +
  material key. Colors/lighting/transforms remain per-instance. Flower material
  keys address the immutable authored palette. Empty cells are initialized too.
- Per-instance RGBA/depth tiles still run lookup + relighting each frame; they no
  longer perform intersection or coverage generation. Display stays shared.
- Butterfly articulation uses the restored 32-frame bank. Published root position,
  continuous root motion, orientation and flight-coupling blend are not quantized.
- Zero-view continuous geometry exists only for explicit numerical diagnostics,
  not as a player option or resource-pressure fallback. No saved cache-off switch.

## Owners and lifetime

`ModelPixelFrame` owns `ModelPixelCache` and prepares it once in the acquired,
completed frame slot before any consumer. `PipelineTopology` owns the bake pipeline;
only immutable source/direction buffers are bound to it. Its resources do not
participate in DDGI/extent generations. Dynamic consumer descriptors still follow
existing topology publication and compute/draw pairing.

Restored `re-flora-vkn::GpuPagedStorage` owns buffer-device-address paging and
residency. Every pointer-reachable buffer is explicitly declared for compute use
and retained until each consuming frame slot completes. It has 64 MiB allocation
granularity, not a total-cache cap. Allocation failures are explicit and clean up
unpublished buffers. Replacements are allocated before any generation is published.

The four bank keys contain kind, resolution and view count. Sources are immutable
compiled assets for the renderer's lifetime; format version is 2. This is an
in-memory GPU cache, not a disk cache. Maximum settings can require GiB of memory;
there is no truncation, silent resolution reduction or uncached fallback.

## Reproduction

```sh
cargo fmt --check
cargo check
cargo test model_pixel
cargo test tracer::butterfly_mesh
cargo test -p re-flora-vkn memory::paged_storage
cargo run --release -- --hidden --mute --auto-exit 0.5
node scripts/validate-model-cache.mjs
node scripts/diagnose-flower-performance.mjs --suite offscreen --check-offscreen
```

`validate-model-cache.mjs` checks every consumed surface's eight float bit patterns
against the same canonical generator on the GPU. All four consumers must have
samples in every phase. It checks independent bank rebuilds, 8/16/37/128/512 views,
complete-head/whole-plant A/B, growth/size/light/display-only reuse, actual fruit
drops and resize after cached submissions. Large-view cases use 8px explicitly;
this is a bounded correctness fixture, not a maximum-memory benchmark. It leaves
`config/gui.toml` unchanged and never reads/writes player saves. The env-only GPU
oracle adds expensive live reference work and must not be used for timing.

First all-consumer run: 14,909,440 leaf, 20,463,104 apple, 9,149,952 butterfly and
13,608,448 flower surface checks, **zero mismatches**. All 13 phases and expected
rebuild sets passed. Logs/images: `target/model-cache-review/`.

First fixed-scene Release regression loop: 128 unseen Wild Geranium plants, 32px
per complete head, still 256 submitted tiles: median flower tile work **0.075 ms**
versus the diagnosed 18–19 ms. Actual median cadence returned to about 58 FPS.
This is not a maximum-population acceptance or startup-latency claim. Full before/
after evidence and other native regressions are recorded separately.

Known unrelated baseline test issue: the broader VKN suite still expects retired
`shader/particles/particle_lod_textured.vert`; the unchanged assertion fails before
reflection (50 passed, 1 failed at the storage-restoration step). The four restored
storage tests pass. No missing-artifact check or geometry-oracle tolerance was relaxed.
