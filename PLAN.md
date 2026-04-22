# Audio Acoustics Plan

## Goal

Refactor the current direct-only spatial audio occlusion path to use Steam Audio's custom batched ray tracing callbacks backed by re-flora's existing GPU ray tracer, then extend that same path to real reflections.

## Principles

- Do not build or maintain a separate acoustic mesh.
- Use the existing GPU ray-traceable world representation as the source of truth.
- Prefer batched callbacks over single-ray callbacks.
- Preserve current direct-path behavior before enabling reflections.
- Keep acoustics ownership inside `petalsonic`, not in app-side polling glue.

## Phase 1: Direct Callback Refactor

### Objectives

- Replace app-side direct occlusion polling with Steam Audio custom ray tracing.
- Preserve current direct occlusion behavior and tuning.
- Keep reflections disabled.

### Work

- Add a ray tracing backend interface in `petalsonic` for acoustics queries.
- Switch `SpatialProcessor` from the default ray tracer to `CustomRayTracer`.
- Construct the Steam Audio scene with `CustomRayTracingCallbacks`.
- Configure simulation with:
  - `with_custom_ray_tracer(ray_batch_size)`
  - `with_direct(...)`
- Implement `BatchedAnyHitCallback` using re-flora's GPU terrain ray batch query path.
- Thread the callback backend from `re-flora` into `petalsonic`.
- Remove `OcclusionRefreshRequested` as the primary direct-occlusion path.
- Remove app-side `update_audio_ray_tracing()` as the source of truth for acoustics.

### Validation

- Direct occlusion sounds equivalent to the current implementation.
- Audio no longer depends on app-side occlusion refresh polling.
- Batched callback traffic is stable and does not cause audio stalls.

## Phase 2: Closest-Hit Callback Support

### Objectives

- Extend the callback path from boolean occlusion to full closest-hit data.
- Supply the data needed for real reflections.

### Work

- Extend re-flora's GPU ray query path to return:
  - hit distance
  - hit normal
  - material index
- Implement `BatchedClosestHitCallback` on top of that query path.
- Define a clean material mapping from re-flora voxel/material types to Steam Audio acoustic material indices.
- Verify whether `audionimbus` custom ray tracer handling is fully sufficient for material-driven reflections and transmission.
- Patch `audionimbus` locally if raw material forwarding needs improvement.

### Validation

- Closest-hit callback returns stable normals and distances.
- Material indices are deterministic and acoustically sensible.
- Direct simulation still behaves correctly after switching to closest-hit capable queries.

## Phase 3: Real Reflections in PetalSonic

### Objectives

- Enable true Steam Audio reflections using the same callback-backed scene.
- Keep the render thread focused on mixing, not heavy simulation.

### Work

- Enable reflections in the simulator with `with_reflections(...)`.
- Add `ReflectionEffect` support to `petalsonic`'s spatial effects graph.
- Add a dedicated acoustics simulation thread in `petalsonic`.
- Run:
  - `run_direct()`
  - `run_reflections()`
- Fetch simulation outputs off the audio thread and publish snapshots for the render thread.
- Apply reflected output into the ambisonics path before decode to stereo.

### Validation

- Reflections respond to terrain and large structures.
- No heavy simulation work occurs on the audio callback thread.
- Render thread consumes cached acoustics state without blocking.

## Phase 4: Cleanup and Integration

### Objectives

- Remove obsolete host-side acoustics glue.
- Expose enough diagnostics to tune and debug the new system.

### Work

- Delete or retire legacy direct override plumbing that only existed for host-side occlusion polling.
- Keep manual overrides only if still useful for debugging.
- Update debug UI and logging to show:
  - callback batch counts
  - simulation cadence
  - direct/reflections enabled state
  - acoustics update age
- Rename old audio ray-tracing terminology to acoustics where appropriate.

### Validation

- No duplicate acoustics systems remain.
- Debug output reflects the callback-backed simulation path.
- Direct and reflections can be toggled and verified independently.

## Phase 5: Performance and Quality Tuning

### Objectives

- Make the new system practical for the real game workload.
- Tune ray budgets and threading without regressing audio stability.

### Work

- Tune custom ray tracer batch size.
- Tune direct and reflections simulation budgets.
- Profile GPU ray query cost under representative gameplay scenes.
- Profile render-thread and simulation-thread interaction.
- Adjust material presets and transmission values by terrain/voxel type.

### Validation

- No audio dropouts under normal gameplay load.
- Reflection quality scales acceptably with cost.
- Acoustic material choices produce believable results.

## Open Questions

- What is the cleanest thread-safe wrapper around re-flora's tracer for callback use?
- Does `audionimbus` need a small local patch to improve custom-hit material handling?
- Which direct simulation features should remain enabled during Phase 1 for strict parity with today's behavior?

## Recommended Execution Order

1. Finish Phase 1 completely before touching reflections.
2. Complete Phase 2 before enabling `with_reflections(...)`.
3. Land Phase 3 only after direct-path parity is confirmed.
4. Finish cleanup before deep performance tuning.
