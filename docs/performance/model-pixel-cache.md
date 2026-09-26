# Shared startup model-surface cache

Implemented after acceptance of orthographic **Rotating Pixels**. This is a real
persistent GPU cache, not a view-snapping preview. It covers all 64 falling-leaf
variants, attached/fallen apples sharing one shape, and 32 butterfly animation
poses. There is no disk cache yet.

## Try / compare

**Pixel Models — Global → Shared Startup Cache (unchecked: Live Generation)**

- Unchecked: regenerate canonical surfaces per instance, every frame.
- Checked: read the startup-generated shared surfaces instead.
- Both use the same generator, projection, nearest view/frame selection,
  per-object lighting, per-pixel normal shading, rotating pixels and depth display.
- The comparison is saved; missing old settings default to unchecked. This is
  **not** the retired screen-grid B mode. View count remains 8–512, default 16.
- Resolutions remain in the individual object settings. One apple setting applies
  to both attached and fallen fruit.

Both modes prebuild the current configuration at first render, before consumers,
so switching the checkbox is immediate and does not trigger another bake. Only
current resolutions/view count are generated, not every possible setting.

## Deep module / generation seam

`src/tracer/model_pixel_cache.rs` owns immutable source preparation, animation
frame selection, per-kind specifications, allocation limits, GPU generation,
publication and frame-slot retirement. Callers request a frame configuration and
bind its resources; they do not implement an atlas allocator or a generator.

The shader half consists of:

- `model_pixel_views.slang`: one actual-N direction distribution and canonical
  basis for both startup generation and runtime nearest-view selection.
- `model_pixel_bake.slang`: **immutable model source + bake specification → unlit
  surface tile**. It calls the existing shared orthographic geometry/coverage
  sampler. Model source IDs resolve to geometry ranges and bank/shape keys.
- `model_pixel_bake.comp.slang`: startup/rebuild scheduling of that generator.
- `model_pixel_cache.slang`: validated bank lookup or the same live generator.
- Existing material adapters relight the result in the instance's real pose.
  Existing `model_pixel_display.slang` displays it without triangle traversal.

Each surface texel is 32 bytes:

1. model-local physical sample position (xyz), normalized orthographic depth (w);
2. model-local normal (xyz), material index (w).

Coverage is `depth < 1`. Empty cells are initialized too. Material IDs retain leaf
stem tinting and apple skin/stem/leaf colors. Instance palette/color, fading,
lighting, external shadows and physical transform are **not baked**. There is no
color quantization or final scene-lit color in the persistent banks.

The existing compact per-instance RGBA/depth tiles remain transient. In cached
mode, their compute pass does lookup + relighting, not triangle sampling or
coverage generation. This deliberately keeps expensive shading at N×N instead
of repeating it for every enlarged screen fragment.

The internal baked-result format is version 1. Sources are immutable compiled
assets for the lifetime of this renderer, so runtime keys only need kind,
resolution and actual view count; shape/animation frame and view index address
within a bank. A future offline loader must additionally validate source content
identity, format and generator version before supplying this same result. No
speculative plugin or file-format framework was added.

## Butterfly articulation versus published flight

The nearest of **32 cyclic authored poses** is selected without interpolation or
hysteresis. This is a new rendering approximation, shared by both sides of the
cache comparison; unchecked is not the old continuous-articulation renderer.

Only articulated shape is discretized. Published facing, position, scale and
flight coupling are preserved. The authored root translation is evaluated at the
published continuous phase and applied as rigid placement, scaled by `(1-blend)`.
It is removed from the canonical shape bank. Thus full flight coupling still
suppresses duplicate bob, intermediate blends remain continuous, and root motion
does not require an additional cache dimension. Physics snapshots are untouched.

Production no longer builds/uploads per-instance butterfly triangles or uploads
the leaf triangle bank every frame. The legacy geometry path remains solely for
the existing continuous numerical diagnostics.

## Rebuilds, bounds and safety

- Changing one object's resolution rebuilds only its bank.
- Changing view count rebuilds affected banks for the new actual-N distribution.
- Camera motion, object rotation, animation, lighting, spawning and the checkbox
  do not rebuild banks. Animation frames and shape variants share ready data.
- Scene/window resize does not alter bake inputs.
- New banks are written before same-command-buffer consumers. Reflected buffer
  uses provide compute-write/read dependencies. Frame slots retain old allocations
  until their fences complete; an in-flight descriptor is never edited in place.
- Generation metadata has a separate frame-owned buffer, so a full 128 MiB bank
  does not exceed the portable storage-buffer limit just to hold a header.

There is a **128 MiB per-bank limit** (three banks). Over-budget combinations keep
the exact live generator for that kind and emit `MODEL_CACHE_FALLBACK`, rather
than displaying stale data, lowering resolution, dropping shapes or allocating
several gigabytes. The checkbox remains requested-on; consult these explicit logs
for per-kind fallback. Returning to a supported setting restores caching.

For 16 views and 16px leaves/butterflies + 32px apples, banks total **12.5 MiB**;
64px apples raise this to **14 MiB**. The currently saved 22px leaf setting instead
uses **19.625 MiB** total at 16 views. Even 64px leaves at 16 views fit exactly in
one 128 MiB bank. Maximum live-bank storage is 384 MiB; temporarily retained old
generations add memory according to the number of in-flight frame slots. Startup
and setting changes may hitch; this implementation is synchronous GPU generation,
not a background/progressive baker.

## Correctness validation

```sh
cargo fmt --check
cargo check
cargo test
env -u WAYLAND_DISPLAY node scripts/validate-apple-model.mjs --cache
env -u WAYLAND_DISPLAY node scripts/validate-leaf-model.mjs --seconds 9
env -u WAYLAND_DISPLAY python3 scripts/validate_butterfly_mesh.py --seconds 12
env -u WAYLAND_DISPLAY cargo run --release -- --hidden --mute --auto-exit 0.5
cargo run --release -- --tail-latest-log 200
```

**1106 tests passed / 4 ignored.** New tests cover canonical sources, bounds,
cyclic frame selection, budget boundaries, continuous published root/coupling,
shared adapter use, GUI migration and persistence. Shader-derived files were
regenerated with `cargo check`, not hand-edited.

`--cache` enables an internal GPU oracle that independently regenerates the live
surface at each consumed cache address and compares **all eight float bit patterns**,
including empty cells. No epsilon/tolerance is used. Counters are read only after
frame completion. The final 16-second run checked 20,684,800 leaf, 12,844,928 apple
and 6,015,744 butterfly texels with **zero mismatches**. It also verified:

- both modes at 8/16/37/128/512 directions and independent resolutions;
- attached fruit, actual drops and five native resize generations;
- checkbox-only reuse, isolated per-kind rebuilds, budget fallback and recovery;
- no Vulkan errors or saved config changes.

An additional 1,100-leaf run exercised 64px output across the existing 64 MiB
transient-tile batch boundary. It checked 338,905,600 leaf texels, including
4,505,600 per frame in 64px cached cases, with zero mismatches in all kinds.
Artifacts: `target/model-cache-review/{validation.log,scene.png,cross-batch.log}`.

The existing strict continuous GPU/CPU coverage/depth and RGBA diagnostics also
passed unchanged. Neither those oracles nor the new surface-bit oracle constitute
artistic approval of the 32-frame butterfly approximation. Screenshots were
inspected, but the visible game was not automatically launched.

## Release performance

```sh
node scripts/benchmark-model-pixels.mjs --seconds 8 --stress-leaves 256 \
  --suite cache --output target/model-cache-bench
```

RTX 3060 Ti; 2880×1620 output / 1440×810 scene; 256 rotating 16px leaves + 21 16px
butterflies + 17 apples; 16 views; nine post-warmup GPU samples per case. No GPU
oracle, readback, fallback or repeated cache builds in these eight runs.

| Scene / apple pixels | Live GPU p50 | Cached GPU p50 | Apple tile live → cached | Particle tiles live → cached |
|---|---:|---:|---:|---:|
| Attached / 32 | 7.194 ms | 6.128 ms | 1.424 → 0.032 ms | 0.389 → 0.035 ms |
| Fallen / 32 | 7.149 ms | 6.371 ms | 1.402 → 0.028 ms | 0.390 → 0.035 ms |
| Attached / 64 | 8.897 ms | 6.285 ms | 3.084 → 0.039 ms | 0.390 → 0.035 ms |
| Fallen / 64 | 8.785 ms | 6.142 ms | 3.019 → 0.032 ms | 0.388 → 0.035 ms |

Actual measured cadence remained **58.0–58.6 FPS**; do not turn reciprocal GPU
milliseconds into claimed FPS. Default-32px GPU/cadence acceptance passed.
Screenshots, logs and `summary.json` are in `target/model-cache-bench/`.

These are short steady-state measurements, not startup-latency measurements.
`MODEL_CACHE_BUILD record_us` measures CPU command recording, **not GPU bake time**.
The `models.cache.bake` GPU scope exists, but normal periodic logging missed the
first generation in this run. No startup GPU-time or offline-loading speed claim
is made. The cache still spends per-instance lighting/shading/display time.
