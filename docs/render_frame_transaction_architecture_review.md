# Linear Render Frame Transaction Architecture Review

Date: 2026-08-30

Reviewed HEAD: `4ce1321fea3aa588982ba997c490e2001c0b3e0e`

Candidate strength entering review: Worth exploring

Decision: Reject at the depth gate; retain the explicit shadow/moisture/scene seam

## Scope

This review asks whether the application frame loop should be replaced by one Tracer-owned linear
render transaction. An acceptable change must hide repeated ordering or lifecycle policy from more
than one caller while preserving the real external terrain-moisture pass, profiling scopes, and
release benchmark metrics. A parameter bundle, callback-shaped render graph, typestate wrapper, or
file-only move is not sufficient.

## Current ownership and evidence

`Tracer::record_shadow_prepass` and `Tracer::record_trace_after_shadow_prepass` have one production
caller in `src/app/core/mod.rs`. Their split was introduced deliberately so the application can run
terrain-moisture drying between shadow publication and scene tracing:

1. Tracer records current direct-sun shadow data.
2. Terrain moisture reads the just-published direct-sun availability mask and records its own GPU
   work through the terrain owner.
3. Tracer records terrain, raster graphics, composition, and post-processing.

This is not duplicated orchestration. The App owns terrain moisture, while Tracer owns rendering on
both sides of that external seam. Moving moisture into Tracer would reverse the intended ownership;
adding a moisture trait would create a hypothetical adapter with one production implementation.

The two Tracer phases already hide their internal pass ordering. The caller supplies their distinct
inputs and keeps the existing release profiling scopes `tracer.shadow_prepass` and `tracer.render`.
The large `Tracer::update_buffers` input surface is a separate concern: wrapping those values in a
single struct would move fields without hiding policy or reducing the knowledge required to produce
them.

## Interface designs considered

### 1. Tracer-owned callback transaction

```rust
tracer.record_frame(
    shadow_inputs,
    scene_inputs,
    |shadow_observation| terrain_moisture.record_dry(shadow_observation),
)?;
```

This is the strongest candidate because it can hide profiler borrowing and assert the phase order
while retaining one explicit external seam. It still requires the only caller to construct both
existing input sets, and the callback makes Tracer responsible for scheduling a foreign domain.

Deletion test: deleting the method restores the same short sequence to its only caller. No policy
becomes duplicated, so the additional module does not create enough leverage.

### 2. Typestate phase token

```rust
let shadow = tracer.record_shadow(frame, shadow_inputs)?;
terrain_moisture.record_dry(shadow.direct_sun_visibility());
tracer.record_scene(shadow, scene_inputs)?;
```

The token would make ordering statically explicit, but the caller would still know and invoke every
phase. It adds types and lifetime coupling without moving implementation knowledge behind a smaller
interface. The existing command-buffer order already enforces the required dependency.

Deletion test: deleting the token removes ceremony and restores the current two semantic methods.
The proposed interface is shallower than the implementation it exposes.

### 3. Frame transaction builder

```rust
let mut frame = tracer.begin_frame(frame_inputs)?;
frame.record_shadow(shadow_inputs)?;
frame.record_external(|shadow| terrain_moisture.record_dry(shadow));
frame.record_scene(scene_inputs)?;
frame.finish()?;
```

This mirrors the implementation as public phases and is effectively a linear render graph. It
increases the number of ordering states and makes cancellation/error behavior part of a new
interface, without a second caller or a second interposed pass to justify that flexibility.

Deletion test and YAGNI both fail.

### 4. Single parameter object only

```rust
tracer.record_frame(RenderFrameInputs { /* existing values */ })?;
```

This reduces the textual function signature but does not reduce the facts the App must author or
the domains that own them. It is a parameter move, not a deep module, and is rejected.

## Depth-gate decision

All designs fail to provide enough depth at the current call graph. The existing split represents a
real in-process seam rather than accidental orchestration, has one caller, and already delegates the
large internal pass sequences to Tracer. The local `gpu_profiler.take()` choreography is a Rust
borrowing inconvenience, not evidence for a new architectural owner.

No runtime code change is justified. The current two-phase interface should remain explicit.

## Reopen conditions

Reconsider the callback transaction only if at least one of these becomes true:

- a second production caller must reproduce the same shadow/external/scene ordering;
- a second real external pass must be inserted at the same boundary;
- repeated ordering or completion bugs demonstrate that command-buffer order is insufficient; or
- a profiling owner emerges that can hide multiple real scopes behind a smaller interface without
  taking ownership of terrain moisture.

Any future implementation must preserve the direct-sun moisture dependency, the existing release
benchmark metric names, the Tracer/terrain ownership boundary, and the ability to attribute shadow
and scene recording separately. It must pass focused moisture and direct-sun tests, the hidden
Vulkan smoke run, and the existing release render benchmarks before replacing this decision.
