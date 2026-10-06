# Model pixel pre-cache

Current controls are saved and searchable under **Model Pixel Pre-cache**:

- Flower heads: use model pre-cache; pre-cache resolution (N × N).
- Butterflies: use model pre-cache; pre-cache resolution (N × N).
- Apples: use model pre-cache; pre-cache resolution (N × N), shared by attached and fallen fruit.

Each switch is independent. Unchecked renders the original native triangles;
checked renders pre-baked pixel cells, not merely a quantized triangle mesh.
Resolution is an integer **8–32**. The existing **Model direction count** is shared
by both rendering modes and all three banks (8–512, including 128 and 256).
Save uses the existing unified settings document. Disabled preferences persist.
Old resolution IDs retain their ownership and values within the supported range;
legacy values above 32 clamp to 32. Loading migrates memory only, never saves.

Falling leaves no longer have a model renderer, model checkbox, model-size slider
or model cache. They use the ordinary voxel/square particle paths. Flight,
wind, source-voxel orientation, transfer, regrowth and physical sizes remain intact.
Historical leaf GLB fixtures remain test/research assets, not game model sources.

## Implementation

`src/tracer/model_surface_cache.rs` owns asynchronous CPU baking and immutable
GPU banks. A balanced triangle BVH samples the nearest authored surface for each
canonical direction and pixel center; coverage, source position, interpolated
normal, material and UV are cached. No view is baked per animal or per frame.
Butterflies reuse 32 articulated source poses with fixed 3.4-unit framing.
Only flower heads are baked; stem rendering is unchanged.

`shader/slang/model_surface_cache.slang` reconstructs constant-color pixel cells,
with per-cell surface depth, live materials/lighting and published rigid poses.
Finite-view orientation, continuous roll and head pivots use the existing shared
bank. UI and the independent scene-pixel slider are unchanged.

Banks rebuild independently on their resolution/direction/shape dependencies.
Native rendering remains visible while a changed bank is being prepared.
Acquired frame slots retain all buffers until their fences complete; a replacement
cannot overwrite submitted data. Obsolete CPU jobs are cancelled. Baking or upload
failure is logged and falls back to native rendering rather than hanging loading.

## Validation

- Pure Rust tests cover nearest BVH intersections, missed rays, source coverage,
  entry bounds, finite samples, cancellation, fixed butterfly framing, independent
  cache keys and CPU/Slang-compatible basis/layout sizes.
- GUI tests cover three independent saved switches/resolutions, legacy leaf
  retirement, bounded migration, same-document save/reload, searchable single
  ownership and no implicit save.
- `scripts/validate-model-precache.sh` captures on/off and individual modes and
  exercises live switches, 8/16/24/32 resolutions and native window resizes under
  Vulkan validation. It verifies the actual cached/native object draw paths and
  that GUI/camera files were not saved.

Native screenshots establish visible candidates and correctness only. Final art,
motion, performance budgets and other GPUs/platforms still require review.
