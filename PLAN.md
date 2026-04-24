# Main-Thread Audio Pump Direct RT Plan

## Current State

The first half of the refactor is already done.

Today:

- `PetalSonicEngine` no longer owns an internal producer/render thread.
- The CPAL callback is consumer-only and reads from the ring buffer.
- `re-flora` explicitly calls `pump_audio()` once per frame from the main thread.
- Audio ray tracing is serviced during that frame pump rather than in a separate later update.

However, the ray tracing path is still not truly direct.

The current `re-flora` integration still uses an internal bridge:

- PetalSonic invokes the host-provided batched any-hit callback.
- `AudioRayTracingBackend::trace_any_hit_batch(...)` does not trace immediately.
- Instead, it hashes rays, checks cached results, and queues misses.
- `SpatialSoundManager::pump_audio(...)` then drains those requests and calls the terrain tracer.
- The pump may loop multiple times to converge queued requests.

This means the callback is now frame-synchronous, but it is still not a real in-call terrain trace.


## Problem

The remaining queue/cache bridge exists only because the earlier design had to cross a thread boundary safely.

That thread boundary is now gone.

Keeping the bridge has several downsides:

- the ray callback does not directly represent terrain visibility at the call site
- extra request/cache bookkeeping still exists for correctness rather than performance
- the pump may require multiple sync passes to satisfy one logical audio update
- debug/runtime stats still describe queued/service behavior instead of direct tracing


## Goal

Make the audio ray callback truly synchronous and direct.

Target behavior:

1. `re-flora` installs the real terrain tracing closure for the duration of `pump_audio()`.
2. `PetalSonicEngine::pump_audio()` runs on the main thread.
3. When Steam Audio invokes the batched any-hit callback, the backend immediately calls the installed terrain tracer.
4. Results are returned directly to the same simulation call.
5. No queued requests, replay, cached correctness lookup, or multi-pass service loop remains.


## Non-Goal

This step is not about changing the CPAL callback model.

The CPAL callback should remain unchanged in principle:

- it stays consumer-only
- it reads already-generated stereo frames from the ring buffer
- it never performs heavy terrain tracing work


## Target Runtime Model

After this step, the intended per-frame flow in `re-flora` is:

1. Update gameplay state.
2. Update listener pose and source state.
3. Call `spatial_sound_manager.pump_audio(trace_batch_closure)`.
4. `pump_audio(...)` installs the closure into a scoped direct RT callback slot.
5. `engine.pump_audio()` runs.
6. Steam Audio invokes the any-hit callback as needed.
7. The backend directly calls the active terrain tracing closure and returns the results immediately.
8. The scoped callback is removed before returning.
9. Render and present the frame.


## Recommended Design

Use a scoped callback bridge, not a queue.

Reasoning:

- the backend object still must exist because Steam Audio custom ray callbacks are installed when the scene is created
- but audio pumping is now strictly synchronous on the main thread
- therefore the backend can consult a temporarily-installed callback that is valid only during `pump_audio()`

This preserves the PetalSonic custom-scene integration while removing the request/replay model.


## Detailed Plan

## Phase 1: Add Scoped Direct RT Callback Slot in `re-flora`

In `SpatialSoundManager`, introduce a scoped mechanism for temporarily installing the real batched terrain tracing callback while `pump_audio()` executes.

The callback must support the existing signature shape:

```rust
FnMut(&[AcousticRay], &[f32], &[f32]) -> Result<Vec<bool>>
```

Recommended approach:

- use a thread-local active callback slot
- install it at the start of `SpatialSoundManager::pump_audio(...)`
- remove it on scope exit

Why thread-local:

- `engine.pump_audio()` is synchronous
- no producer thread remains
- the callback only needs to be reachable during the current main-thread pump call


## Phase 2: Make `AudioRayTracingBackend::trace_any_hit_batch(...)` Truly Direct

Replace the existing request/cache logic with immediate tracing.

New behavior:

- if audio ray tracing is disabled, return `vec![false; rays.len()]`
- otherwise, look up the currently installed scoped callback
- invoke it immediately with the provided rays and distance arrays
- return the results directly to Steam Audio

Fallback behavior:

- if no scoped callback is installed, warn once and return `false` for all rays
- if the callback errors or returns the wrong length, warn and return `false` for all rays


## Phase 3: Remove Queue/Cache Correctness Machinery

Delete the now-obsolete synchronization structures from `SpatialSoundManager`.

Remove:

- `AudioRayTracingRequest`
- `AudioRayQueryKey`
- `AudioRayTracingQueryState`
- `mpsc` sender/receiver plumbing
- pending request bookkeeping
- cached last-result lookup used for cross-thread correctness
- `MAX_RT_SYNC_PASSES`
- `service_audio_ray_tracing_requests(...)`

Any cache should only remain if reintroduced later as a measured performance optimization, not as part of correctness.


## Phase 4: Simplify Runtime Stats and Debug Text

Update audio RT instrumentation so it describes the new direct model.

Keep only stats that still make sense, for example:

- callback batch count
- callback ray count
- traced batch count
- traced ray count
- hit count
- direct callback failures or fallback count

Remove queued/service-era terminology such as:

- reused last query results
- missing last query results
- queued requests

Update the debug text in `App::update_audio_ray_tracing()` to match the new meanings.


## Phase 5: Verify Main-Thread Sync Behavior

After the cleanup, verify:

- `pump_audio(...)` performs a single synchronous audio update path
- Steam Audio any-hit callbacks directly execute terrain tracing during the same call
- no queue draining or replay pass remains
- no stale result lookup path remains
- audio still builds and behaves correctly under the current frame loop


## Acceptance Criteria

This step is successful when:

- `AudioRayTracingBackend::trace_any_hit_batch(...)` directly invokes the active terrain tracer callback
- no request queue or result cache is needed for correctness
- `SpatialSoundManager::pump_audio(...)` calls `engine.pump_audio()` exactly once per frame pump
- no multi-pass service loop remains
- debug/runtime stats no longer refer to queued/service semantics
- `cargo check` succeeds in both `re-flora` and `petalsonic`


## Risks

### Scoped callback lifetime mistakes

The active callback will borrow `&mut self.tracer` from `App`, so the scoped installation must be carefully bounded to the `pump_audio(...)` call.

Mitigation:

- use a strict scoped helper
- avoid storing the borrowed callback in long-lived structs

### Error handling in the direct callback path

The terrain tracer closure can fail or return mismatched output lengths.

Mitigation:

- validate result length in the backend
- fall back to `false` results on error
- log a concise warning

### Hidden assumptions in current debug/runtime reporting

Some debug output currently assumes queued/service semantics.

Mitigation:

- update stats and labels in the same pass as the backend cleanup
