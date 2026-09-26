# Rendering effect seams

This is the behavior-preserving architecture step, before cloud retirement. Clouds
remain hard-disabled by launch/frame input, but their implementation is still built.

## Shared frame data

`shader/slang/tracer_types.slang::U_CameraInfo` is the single camera snapshot layout.
`camera_info` and `camera_info_prev_frame` are different **resource roles**, not different
shader types. `TracerUniformResources` allocates both from the canonical tracer layout;
`BufferUpdater` uploads the same generated `CameraInfo` representation. Temporal effects
consume these snapshots and must not supply the layout or own the frame camera history.
The VKN reflection test checks the actual uniform types, all seven offsets, and 400-byte
layout for the tracer, god rays, lens flare, and clouds.

## Daylight query

`shader/slang/tracer_shadowing.slang::sampleDirectSunShadow` owns source bindings,
filtering, availability masks, and transmittance composition. Callers supply a receiver,
GUI policy, and the shadow camera, not a list of terrain/leaf/atmosphere maps.
`flora_shadow.slang` adds stylized normal weighting and lighting, not a second shadow
sampling implementation.

Receiver constructors encode **different real policies**:

- `voxelSurfaceSunShadowReceiver`: voxel-face-offset terrain/leaf samples, continuous
  surface-offset atmospheric sample. Used by smooth terrain and raster-tree shading.
- `surfaceSunShadowReceiver`: a point on an exposed face/triangle, with just the normal
  offset. Used by terrain and raster-tree surface integrals. Never treat this as a voxel
  center and offset it twice.
- `pointSunShadowReceiver`: already positioned point receivers, including soil and
  models. `withLeafReceiver` retains grass's rest-anchored leaf sample without freezing
  its terrain/atmosphere sample.

Leaf depth gating, 3x3 maximum-opacity filtering, UV rejection, minimum transmittance,
VSM, and the PCSS receiver-plane implementation remain independent policies. Do not
replace them with one precombined shadow texture. Soil retains its terrain-ray fallback
outside available VSM coverage.

On the host, `Tracer::direct_sun_shadow_resources` is an opaque `ResourceContainer`.
The soil builder resolves its complete reflected query through this provider and its
own geometry providers. Adding/removing a daylight source changes the query module and
provider composition, not the builder or its app caller. Source-mask/capture diagnostics
are explicit source-aware protocols, separate from ordinary daylight consumers.

## Sky query

`composition_sky.slang::sampleScreenSky` owns the view-dependent sky layers.
`composition_scene.slang` and the terrarium's refracted scene query use this interface
without forwarding an effect texture. `computeSkyWithSunAndStars` remains the directional
analytic query; reflected directions must not reuse a camera-screen cloud sample.
The existing unused cloud-reflection helper is deliberately left for retirement.

## Effect implementation and lifecycle

`src/tracer/clouds.rs` owns cloud allocation formats/extents, shader loading, passes,
clear values, temporal push constants, copies, and validity. `CloudRuntime` records
complete shadow/screen phases, including history storage and the neutral disabled path.
The tracer only schedules those phases and notifies invalidation on resize, camera cut,
light-space change, or disabled shadows.

The existing topology still owns descriptor generations and fence-safe retirement.
Cloud resources join it as nested `ResourceContainer`s; `CloudPasses::pipelines` registers
the group for extent publication. Private screen textures are recreated with the render
extent; fixed-resolution shadow textures survive resize, with history invalidated as
before. No universal effect registry or alternative camera owner is introduced.

## Locality when retiring or returning a feature

Implementation changes concentrate in the feature module/shaders, the native shader
build list, and explicit composition sites: resource/topology registration, frame-phase
scheduling, the daylight query/provider, and the screen-sky query. Feature configuration
and any source-aware diagnostic serialization still require deliberate compatibility
work. Terrain, flora, particle/model, glass-scene, and soil consumers do not enumerate
cloud resources and therefore do not need another round of signature/binding edits.
