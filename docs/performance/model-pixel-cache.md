# Permanent shared model surfaces

All 64 falling-leaf variants, attached/fallen apples sharing one shape, and 32
butterfly articulation poses use startup-generated surfaces. Rotating Pixels and
per-object lighting remain permanent. There is no disk cache yet.

The original cache/live comparison and its 128 MiB per-kind fallback have been
**removed**. `model_pixel_cache` is retired from saved settings whether true or
false. **Pixel Models — Global contains only Discrete View Count**, default 16,
range 8–512. Per-object resolutions remain in their own settings.

## Two deep modules

1. `src/tracer/model_pixel_cache.rs`: immutable model sources, view/animation
   keys, current specifications, generation and rebuild policy.
2. [`GpuPagedStorage`](../gpu-paged-storage.md): generic record allocation,
   blocking, shader addressing, synchronization and fence-scoped residency.

Storage never receives a shape count, view count or animation concept. Models
request a number of records with a layout and receive an opaque handle. Physical
storage blocks, page tables and their lifetime are hidden by the storage module.

The shader seam is still **model source + bake specification → unlit surface**:

- `model_pixel_views.slang`: shared actual-N direction distribution and basis.
- `model_pixel_bake.slang`: sole canonical generator, using the existing common
  orthographic geometry/coverage sampler.
- `model_pixel_bake.comp.slang`: startup/rebuild execution, writing through the
  generic storage handle.
- `model_pixel_cache.slang`: stored-surface lookup. It has no live-generation
  return branch or memory-budget fallback.
- Material adapters relight stored data in the instance's real pose; the common
  display reads the transient shaded tile without triangle traversal.

Each stored surface is 32 bytes: model-local physical position + canonical depth,
then model-local normal + material index. Coverage is `depth < 1`; empty records
are initialized too. Instance palette, color, opacity, physical transform,
lighting and external shadows are not baked. No color quantization was introduced.

Compact per-instance RGBA/depth tiles remain transient. Their compute pass now
performs lookup + relighting, not model intersection or coverage generation.
This keeps shading at N×N rather than repeating it per enlarged screen fragment.

The surface format remains version 1. Source assets are immutable within a running
renderer. Offline loading later must validate source identity and generator/format
versions before supplying the same records; no speculative plugin or disk format
was introduced.

## Butterfly pose contract

The nearest of 32 cyclic authored articulation poses is selected without blending
or hysteresis. Published facing, position, scale and flight coupling remain live.
Authored root translation is evaluated at the published continuous phase, applied
as rigid placement with `(1-blend)`, and removed from the canonical shape bank.
Thus intermediate coupling stays continuous and full coupling suppresses duplicate
bob. Physics snapshots are not changed.

Production does not build/upload per-instance butterfly triangles or upload the
leaf triangle bank every frame. The pre-existing continuous numerical diagnostic
path remains test-only. The GPU stored-surface oracle can call the sole generator
to compare records, but never displays that recomputed value or uses it as fallback.

## Rebuilds and capacity

- First render generates the current configuration before consumers.
- One resolution change rebuilds only its affected model data.
- Changing N rebuilds for the actual-N direction distribution.
- Camera, rotation, animation, colors, lighting, spawning and scene/window resize
  do not rebuild stored surfaces.
- Replacement allocations are prepared before recording/publishing their new
  generation. Pointer-reachable buffers receive explicit tracked GPU dependencies.
  Old allocations remain leased until all consuming frame slots complete.

There is **no fixed total/per-kind budget**. Storage's preferred 64 MiB block size
is allocation granularity, not a limit on a logical dataset. It queries real
allocation constraints and uses Vulkan buffer device addresses. Actual exhaustion
or invalid/overflowing requests produce an explicit error, not quality reduction
or a different renderer. Physical capacity is not infinite.

At 16 directions:

| Model data | Current saved resolution | Payload | At 64×64 |
|---|---:|---:|---:|
| 64 leaf variants | 22×22 | 15.125 MiB | 128 MiB |
| One apple shape | 32×32 | 0.5 MiB | 2 MiB |
| 32 butterfly poses | 16×16 | 4 MiB | 64 MiB |

Current payload total: **19.625 MiB**, independent of instance count. Benchmark
16px leaves/butterflies + 32px apples use **12.5 MiB**, or **14 MiB** with 64px apples.
Address tables/alignment and temporarily retained generations add storage. The
block count and buffer bytes are reported in `MODEL_CACHE_BUILD`.

At 32 directions, 64px leaf data is 256 MiB and occupies four blocks; it no longer
switches to live generation. The maximum allowed view/resolution combination can
request several GiB: allocation success depends on actual available resources.
Startup and settings changes can hitch; generation is not progressive/background.

## Validation

```sh
cargo fmt --check
cargo check
cargo test
cargo test -p re-flora-vkn
env -u WAYLAND_DISPLAY node scripts/validate-apple-model.mjs --cache
env -u WAYLAND_DISPLAY node scripts/validate-leaf-model.mjs --seconds 9
env -u WAYLAND_DISPLAY python3 scripts/validate_butterfly_mesh.py --seconds 12
env -u WAYLAND_DISPLAY cargo run --release -- --hidden --mute --auto-exit 0.5
cargo run --release -- --tail-latest-log 200
```

Main tests: **1106 passed / 4 ignored**. Vulkan library: **45 passed**, including
storage planning, boundaries, overflow/device limits and retirement. Saved-setting
migration removes both former checkbox values. Generated files were regenerated
with `cargo check`.

The GPU oracle compares all eight float bit patterns of stored and regenerated
surfaces, including empty cells; no epsilon is used. The 16-second review checked
34,996,224 leaf, 17,508,096 apple and 12,676,608 butterfly records with **zero
mismatches**. It exercised 8/16/37/128/512 views, independent resolutions, actual
fruit drops, native resizes, isolated rebuilds, 256 MiB/four-block storage and
replacement. A broader combination also used 296 MiB/five-block leaf data.

The 1,100-leaf stress run checked 585,164,800 leaf records, including 4,505,600 per
64px frame across the existing 64 MiB transient-tile batching boundary. All kinds
had zero mismatches. Artifacts:

- `target/model-cache-review/validation.log`, `scene.png`
- `target/model-cache-review/paged-cross-batch.log`

Both old strict continuous GPU/CPU coverage/depth/RGBA diagnostics passed. These
tests do not replace artistic review of 32-frame butterfly articulation. No
visible game was automatically launched. Vulkan errors and saved-config changes
were absent. Runtime coverage is Linux / RTX 3060 Ti, not Windows/MoltenVK.

## Release measurements

```sh
node scripts/benchmark-model-pixels.mjs --seconds 8 --stress-leaves 256 \
  --suite cache --output target/model-paged-bench
```

`cache` is now an alias of `apples`, not a live/cached A/B. Both run 8/32/64px
apples; `stage-one` varies direction count with caching always active. Historical
live comparisons require the older revision.

RTX 3060 Ti; 2880×1620 output / 1440×810 scene; 256 rotating 16px leaves + 21 16px
butterflies + 17 apples; 16 views; nine post-warmup GPU samples per case. Every
kind generated once, without an oracle or regeneration during steady state.

| Scene / apple pixels | Whole GPU p50 | Apple lookup/shading | Particle lookup/shading |
|---|---:|---:|---:|
| Attached / 32 | 5.408 ms | 0.029 ms | 0.031 ms |
| Fallen / 32 | 5.340 ms | 0.026 ms | 0.032 ms |
| Attached / 64 | 6.169 ms | 0.035 ms | 0.032 ms |
| Fallen / 64 | 5.313 ms | 0.030 ms | 0.031 ms |

Across all six cases, GPU p95 was 5.351–7.600 ms and actual cadence **58.1–58.5 FPS**.
The default-32px GPU/cadence checks passed. These short runs establish usable
Release behavior, not a controlled speedup from paging: scene/GPU timings vary.
Artifacts: `target/model-paged-bench/{summary.json,*.log,*.png}`.

For context, the previous live/cached A/B at `f44c9340` measured 32px apple stages
around 1.4 → 0.03 ms and particle stages 0.39 → 0.035 ms. Those were cache-reuse
savings, not measurements of this storage refactor. Startup bake latency and
allocation-exhaustion behavior were not benchmarked or deliberately forced.
