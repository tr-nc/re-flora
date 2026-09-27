# Shared model-surface cache (cache-only renderer)

The shared cache restored from `f7b7e036` is adapted to the
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
  rebuilds only its bank. Flower directions and authored head/height controls
  rebuild only flowers; the global view count applies to the other three banks.
- Each 32-byte surface stores model-local position + canonical depth and normal +
  material key. Colors/lighting/transforms remain per-instance. Flower material
  keys address a separate compact authored palette, not a geometry buffer. Empty
  cells are initialized too. Source identities are separate from bake mesh ranges.
- Per-instance RGBA/depth tiles still run lookup + relighting each frame; they no
  longer perform intersection or coverage generation. Display stays shared.
- Butterfly articulation uses the restored 32-frame bank. Published root position,
  continuous root motion, orientation and flight-coupling blend are not quantized.
- Continuous/zero-view rendering is removed, including diagnostic frame routes.
  Requests always clamp to 8–512. Frame consumers cannot import the generator,
  triangle types or geometry descriptors. New consumers use `model_pixel_cache`
  and the existing `ModelPixelFrame` publication seam; there is no live fallback.
- The obsolete butterfly triangle self-shadow checkbox is removed and retired
  from saved files; the cache path already used object lighting. Transmission,
  instance pose, root motion, wind and both pixel display modes remain dynamic.

## Owners and lifetime

`ModelPixelFrame` owns `ModelPixelCache` and prepares it once in the acquired,
completed frame slot before any consumer. `PipelineTopology` owns the bake pipeline;
its source/direction buffers are immutable within each published generation and
bound through ready-slot descriptors. They do not participate in DDGI/extent generations. Dynamic consumer descriptors still follow
existing topology publication and compute/draw pairing.

Restored `re-flora-vkn::GpuPagedStorage` owns buffer-device-address paging and
residency. Every pointer-reachable buffer is explicitly declared for compute use
and retained until each consuming frame slot completes. It has 64 MiB allocation
granularity, not a total-cache cap. Allocation failures are explicit and clean up
unpublished buffers. Replacements are allocated before any generation is published.

The four bank keys contain kind, resolution and their actual view count. The
flower key also contains head/height geometry controls. Authored assets are
compiled once; the cache publishes immutable transformed source generations for
configuration changes, sharing them with native stems. The direction prefix is
allocated for the largest current bank, never the slider maximum. Format version is 3. This is an
in-memory GPU cache, not a disk cache. Maximum settings can require GiB of memory;
there is no truncation, silent resolution reduction or uncached fallback.

## Reproduction

```sh
cargo fmt --check
cargo check
cargo test model_pixel
cargo test tracer::butterfly_mesh
cargo test -p re-flora-vkn memory::paged_storage
flock --close /tmp/re-flora-vegi-cache-controls-gpu.lock env -u WAYLAND_DISPLAY \
  cargo run --release -- --hidden --mute --auto-exit 0.5
flock --close /tmp/re-flora-vegi-cache-controls-gpu.lock \
  node scripts/validate-model-cache.mjs --seconds 30
```

`validate-model-cache.mjs` checks every baked record's eight float bit patterns
against the generator **once per bank rebuild**. A separate bake-only diagnostic
exports actual center rays and projected vertices at the first/last canonical
directions of every source. An independent CPU oracle verifies identity,
coverage, nearest depth and center preservation; it never reads a GPU coverage
mask as its expected result. Depth tolerance remains 0.00002 and coverage epsilon
0.0001 pixel. The reference preserves float32 closest-edge selection, rather than
using a float64 minimizer that can choose a different edge after tile rounding.

Runtime lookup only counts consumed records in explicit review mode; it does not
resample geometry. All four banks must have consumers in every phase, and every
rebuild must finish both bake validators. The fixture checks isolated bank
rebuilds, 8/16/37/128/512 views, complete-head/whole-plant A/B, growth/size/light/
display-only reuse, real fruit drops and post-consumer resize. Large-view cases use
8px intentionally. Config and player saves are untouched. This is correctness
evidence, not timing or a maximum-memory test.

See [cache-only worker evidence and validator migrations](../evidence/cache-only-renderer.md).

## Current saved controls and acceptance

**Debug → Flora → Ground Plants → Model Flowers (A/B)** applies to all eight
species, in both complete-head and whole-plant modes:

- Complete Head Size: 0.25–4×, around each published attachment including calyx/center.
- Plant Height (Stems / Attachments): 0.25–4×, independent of head size.
- Pixel Resolution: 8–64px, default 32.
- View Count: 8–512, default 16, independent of apples/leaves/butterflies.

All are saved through the standard config system and apply live. The old overall
size and head/whole toggle retain their meaning; old saves acquire new defaults.
Small changes around 1× preserve framing continuously rather than jumping pixel
density. High settings can take time/memory to rebuild; there is no maximum-slider
preallocation, quality downgrade or live fallback.

[Integrated acceptance](../evidence/cache-controls-delivery/README.md), code
`2fb853df`: 1,246 main tests + 4 root tests, 25 Slang CPU tests; 18 native control
phases, the 13-phase four-bank matrix, all migrated native validators and normal
hidden Release runs passed. 128 plants at 32px cost 0.048–0.050 ms in the tile pass;
1,024 plants cost 0.226 ms (actual cadence about 58 FPS). The older timings below
remain historical evidence, not claims for the current code.

## Historical cache-restoration acceptance

The results below apply to the earlier restoration, not a performance acceptance
for cache-only retirement. Its continuous validators are now retired/migrated.

Final validated code: **`43a5683a`** (cache restoration `44a1ab6e`, storage `89330a88`).
35,131,392 leaf, 18,340,416 apple, 8,140,608 butterfly and 12,240,192 flower surface
checks, **zero mismatches**. Every one of the 13 phases has samples in all four
kinds after excluding prior-phase frame-slot readbacks. 64px leaves explicitly
exercise a 128 MiB bank across multiple GPU pages. All expected rebuild sets and
post-consumer resize passed. Logs: `target/cache-restore/cache-review-final/`.

The main Cargo suite passed **1,238 tests, 4 ignored**, plus 4 other root tests.
Original native flower A/B/lifetime/resize, attached/fallen apple, strict continuous
leaf and strict continuous butterfly validators all passed unchanged. No Vulkan
errors or GUI config changes. Twelve CLI help/error-recovery checks passed too.

## Release measurements

RTX 3060 Ti, 2880×1620 output, 1440×810 scene, automatic present mode; fixed 16
views. No GPU oracle during timing. Same 32px flower heads and planted populations,
not a quality reduction or added visibility culling. Original diagnosis is
[recorded separately](model-flowers-diagnosis.md).

| Wild Geranium case | Previous tile GPU p50 | Restored tile GPU p50 | Actual restored FPS |
|---|---:|---:|---:|
| 32 plants, facing plot | 4.681 ms | 0.039 ms | 58.3 |
| 128 plants, facing plot | 17.886 ms | 0.066 ms | 58.4 |
| 128 plants, looking away | 19.340 ms | 0.066 ms | 58.1 |
| 1,024 plants, facing plot | not measured | 0.383 ms | 58.1 |

The 1,024-plant run submitted 2,048 head tiles and built the same four shared banks
once, not 1,024 caches. Whole-frame GPU p50/p95 was 7.596/9.224 ms. The earlier
independent offscreen red/green run measured 0.075 ms and passed the reproducer.

With 256 rotating 16px leaves, 21 butterflies and 32px apples, shared particle tile
work was 0.042 ms; attached/fallen apple tile work was 0.030/0.027 ms. The existing
benchmark's default-32px GPU and baseline-normalized cadence gates passed. Actual
cadence stays near 58 FPS; reciprocal GPU time is not claimed as observed FPS.

These are short steady-state measurements, not startup-latency, maximum-population
or every-device acceptance. Chunk-only flower culling is unchanged: offscreen tiles
can still do cheap lookup/relighting. High resolution/view-count changes bake
synchronously and can hitch; they can also request GiB of GPU memory.

[Machine-readable results and validation hashes](../evidence/model-cache-restored/summary.json)
include the complete matrices. [Native all-consumer diagnostic capture](../evidence/model-cache-restored/cache-scene.png)
is evidence of the actual renderer, not final artistic approval. Full raw artifacts:
`target/cache-restore/`.

Known unrelated baseline test issue: the broader VKN suite still expects retired
`shader/particles/particle_lod_textured.vert`; the unchanged assertion fails before
reflection (50 passed, 1 failed at the storage-restoration step). The four restored
storage tests pass. No missing-artifact check or geometry-oracle tolerance was relaxed.
