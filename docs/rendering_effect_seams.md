# Rendering effect seams

Procedural clouds are retired: no cloud shaders, pipelines, allocations, dispatches,
clears, histories, render flags, uniforms, or live controls remain. The real shared
interfaces introduced before retirement continue to serve terrain, flora, soil,
sky, god rays, lens flare, and glass. There is no disabled implementation or effect
registry waiting to be enabled.

## Shared frame data

`shader/slang/tracer_types.slang::U_CameraInfo` is the single camera snapshot layout.
`camera_info` and `camera_info_prev_frame` are separate **resource roles**, not
separate shader types. `TracerUniformResources` allocates both from the canonical
tracer layout; `BufferUpdater` uploads generated `CameraInfo` values. No optional
effect supplies the ABI or owns frame camera history. Reflection tests verify both
roles, all seven offsets, and the 400-byte layout for tracer/god rays/lens flare.

## Daylight query and receiver policies

`tracer_shadowing.slang::sampleDirectSunShadow` owns semantic source descriptors,
filtering, readiness masks, and terrain × leaf composition. Ordinary consumers pass
a receiver, GUI policy, and shadow camera, never a list of effect maps.
`flora_shadow.slang` adds stylized normal weighting and lighting, not a second
sampling implementation. The PCSS receiver-plane path shares the leaf policy.

- `voxelSunShadowReceiver`: canonical voxel-face-offset terrain/leaf samples, stable
  across pixels. Continuous hit positions still serve local lights and DDGI, but
  there is no unused atmospheric receiver field in the shadow query.
- `surfaceSunShadowReceiver`: an already exposed face/triangle point receives only
  the normal offset, **not** a second half-voxel displacement.
- `pointSunShadowReceiver`: already positioned models/flora/soil. `withLeafReceiver`
  retains grass's rest-anchored leaf sampling while terrain follows its posed point.

Leaf depth gating, 3×3 maximum-opacity filtering, UV rejection, minimum
transmittance, and terrain VSM remain independent policies; never precombine their
maps. CPU Slang tests exercise actual constructors and the shared per-tap depth
gate. `terrain_moisture_sun.slang` owns soil's exposure plan: night/backface rejection
and exact terrain-ray fallback outside/unready VSM coverage. Exhaustive mask/coverage
tests cover its decisions; `terrain_moisture_dry.slang` performs the ray/map queries.
DDGI transport and the path-tracing reference remain independent of visual shadows.

The host's `Tracer::direct_sun_shadow_resources` is an opaque `ResourceContainer`.
The soil builder resolves reflected dependencies from it plus geometry providers.
These fixed shadow resources survive render resize: creation-time soil descriptors
must not reference extent-recreated resources without coordinated publication.

## Sky and retained lifecycle

`composition_sky.slang::sampleScreenSky` owns camera-screen sky evaluation.
Scene/glass-scene composition does not enumerate effect textures.
`computeSkyWithSunAndStars` is the directional query used for actual reflected
rays; never reuse a camera-screen sample for a reflected direction. The active
voxel-glass path is `glass_resolve.slang`, not the legacy terrarium helper.

`PipelineTopology` retains fence-safe descriptor generations and extent retirement.
God-ray/lens-flare histories still reset on resize and camera cut. Direct-sun terrain
and leaf history still reset on local invalidation/light-space changes. Cloud
retirement removes its entire lifecycle rather than substituting neutral clears.

## Concrete reintroduction sites

A future effect implementation owns its private resources, shaders, passes, history
copies/clears, and invalidation policy in a feature module (the architecture-step
commit `9fb15163` demonstrates that ownership, not a ready-to-reenable feature).
Register its entry points in `crates/re-flora-shader-build/src/lib.rs`; compose its
resource provider and passes at `src/tracer/{resources,extent_dependent_resources,
pipeline_builder,mod}.rs`. The topology must own descriptor publication/retirement;
the tracer schedules complete phases, not individual private texture operations.

Add daylight attenuation once in `tracer_shadowing.slang` and the daylight provider;
add screen layers once in `composition_sky.slang`. Receiver policies must explicitly
select the right sample domain; existing point, voxel and exposed-surface policies
are not interchangeable. No material or gameplay consumer should acquire effect
texture arguments. Authored controls belong in declarative GUI config (regenerate
with `cargo check`), not duplicate application state. Restore source-aware diagnostics
only through an explicit capture schema change, not a recycled lane.

## Compatibility boundaries

- Schema-v1 GUI loading removes the exact 20 retired IDs before live adjustables
  exist; it removes the obsolete group while preserving unrelated saved settings.
  Migration is covered through load → expanded draw → sync → save → reload.
- `--no-clouds` is an explicit deprecated no-op; canonical scripts omit it.
- RFIRR v11 retains the 284-byte header/evidence and five float4 planes. Its fifth
  plane is `(terrain, leaf, integrated_weight, combined)`. Point samples have zero
  weight; integrated samples preserve independently averaged sources/products.
  See [capture contract](ddgi_transport_acceptance.md). v1–v10 decoding and historical
  cloud metrics remain at the decoder boundary; production runners accept only v11.
- Historical docs/captures, vendor point-cloud references, and the stock asset-pack
  `Clouds.png` are not procedural-cloud implementations and remain intact.
