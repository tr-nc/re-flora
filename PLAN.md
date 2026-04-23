# Main-Thread Audio Pump Refactor Plan

## Background

Today, `PetalSonicEngine` owns an internal producer thread (`render_thread_loop`) that continuously fills an audio ring buffer, while the CPAL audio callback consumes from that buffer on the audio device thread.

In `re-flora`, audio ray tracing cannot safely call into the terrain tracer from that PetalSonic producer thread, so the current integration uses a deferred request/service model:

- PetalSonic's ray tracing callback queues requests.
- `re-flora` services those requests later from the main thread during frame update.
- Cached results are reused to bridge the thread boundary.

This avoids unsafe cross-thread tracer access, but it also means:

- audio ray tracing is not executed in the same call context as audio simulation
- ray tracing results are delayed and can be stale by up to a frame or more
- additional synchronization and caching machinery exists for correctness rather than optimization
- debugging timing is harder because the producer thread and game loop are decoupled


## Problem

The current PetalSonic producer thread introduces a synchronization boundary that does not align well with `re-flora`'s terrain tracer and frame-driven game state updates.

The main issue is not that the CPAL audio callback is asynchronous. That part is expected and correct.

The issue is that audio production currently happens on PetalSonic's own thread instead of on the game side. Because of that:

- listener/source updates happen on the main thread
- audio production happens on a different thread at different times
- ray tracing callbacks occur on that producer thread
- `re-flora` must queue and replay work later on the main thread

This creates a synchronization problem that is structural, not incidental.


## Goal

Remove PetalSonic's internal render thread and switch to a pump-only model where `re-flora` explicitly advances audio production from the main thread once per frame.

The CPAL callback remains unchanged in principle:

- it stays consumer-only
- it reads already-generated stereo frames from the ring buffer
- it never performs heavy work or takes locks on the game side

Audio production becomes app-driven:

- `re-flora` updates listener pose and source state
- `re-flora` calls `pump_audio()`
- `pump_audio()` applies internal watermark policy and generates as many frames as needed


## Why This Refactor

This refactor is intended to make audio simulation happen in the same thread/context as the rest of `re-flora`'s authoritative world state.

Benefits:

- audio ray tracing callbacks can run synchronously on the main thread
- the deferred request/service model can be removed
- listener/source updates affect the same frame's audio pump deterministically
- debugging becomes simpler because production timing is tied to the game frame
- synchronization logic becomes performance-oriented rather than correctness-oriented

Tradeoff:

- audio production becomes sensitive to main-thread stalls
- therefore PetalSonic must keep enough ring buffer headroom via internal watermarks

This tradeoff is acceptable for a real-time exploration game, provided the ring buffer policy keeps enough buffered audio to survive short frame spikes.


## Target Runtime Model

After the refactor, the intended per-frame flow in `re-flora` is:

1. Update gameplay state.
2. Update listener pose.
3. Update source positions / config / playback state.
4. Call `pump_audio()`.
5. Render and present the frame.

Important detail:

- `pump_audio()` does not mean "generate exactly one block".
- It means "inspect ring buffer occupancy and refill toward the internal high watermark".
- Each call may generate zero, one, or multiple chunks.

The CPAL callback still runs independently at device pace and consumes from the ring buffer.


## API Decision

The public pump API should remain simple and return `Result<()>`.

Proposed shape:

```rust
pub fn pump_audio(&mut self) -> Result<()>;
```

Notes:

- internal watermark policy stays inside PetalSonic
- the caller does not provide a target frame count
- diagnostics remain separate from the return type


## High-Level Design

### PetalSonic

PetalSonic will stop spawning an internal producer thread.

It will continue to own:

- the ring buffer
- the CPAL stream
- the mixer/spatial processor
- resampler state
- playback command receiver
- active playback state
- event/timing channels

It will expose a synchronous `pump_audio()` entry point that performs the work formerly driven by `render_thread_loop`.

### re-flora

`re-flora` will call the pump entry once per frame after gameplay/audio state updates.

Because pumping now happens on the main thread, audio ray tracing can be executed synchronously and directly against the terrain tracer, removing the deferred service model.


## Detailed Implementation Plan

## Phase 1: Refactor PetalSonic Engine Ownership

Move producer-side state from the old render-thread context into `PetalSonicEngine` itself.

`PetalSonicEngine` should directly retain whatever is needed for pumping, including:

- ring buffer producer
- resampler
- command receiver
- active playback
- spatial processor
- listener pose
- event sender
- timing sender
- channel/block-size/master-gain configuration

The old `RenderThreadContext` becomes unnecessary once synchronous pumping is in place.


## Phase 2: Remove Internal Render Thread

Change `start()` so it still:

- initializes the output device
- creates the stream config
- allocates the ring buffer
- builds and starts the CPAL output stream

But it should no longer:

- spawn `petalsonic-render`
- run `render_thread_loop`
- manage render-thread shutdown/join logic

Cleanup items:

- remove `render_thread`
- remove `render_shutdown`
- remove `render_thread_loop`
- remove `RenderThreadContext`


## Phase 3: Add `pump_audio()` with Internal Watermark Policy

Implement:

```rust
pub fn pump_audio(&mut self) -> Result<()>;
```

Responsibilities of `pump_audio()`:

1. Update listener pose in the spatial processor.
2. Process pending playback commands.
3. Inspect ring buffer occupancy.
4. If occupancy is below the low watermark, generate enough audio to move toward the high watermark.
5. Emit timing events.
6. Emit playback events such as `SourceCompleted` / `SourceLooped`.

The refill policy should preserve the current intent:

- low watermark triggers refill
- high watermark is the target occupancy
- refill chunk bounds work per pump pass

The exact chunk count per frame may vary.


## Phase 4: Reuse Existing Generation Path

Keep the existing sample generation path as intact as possible.

In particular, reuse:

- `process_playback_commands(...)`
- `generate_samples(...)`
- existing resampling and event emission behavior
- existing mixer/spatial processor flow

The safest implementation is a structural refactor rather than rewriting the audio algorithm.


## Phase 5: Wire `re-flora` to Pump Once Per Frame

In the main frame loop, call the new pump function after audio-relevant state has been updated.

Intended location in `re-flora`:

- after tree audio/source/listener updates
- before render/present

Target order:

1. gameplay updates
2. listener/source updates
3. `pump_audio()`
4. graphics work


## Phase 6: Remove Deferred Audio RT Servicing

Once audio generation runs on the main thread, remove the queue/service synchronization path from `SpatialSoundManager`.

Remove or simplify:

- queued audio ray tracing requests
- later servicing in `update_audio_ray_tracing()`
- request receiver plumbing
- pending request bookkeeping used only for cross-thread correctness

Replace it with direct synchronous tracing during pump-driven audio generation.

Any remaining result cache should only stay if it is a proven performance optimization.


## Phase 7: Update Debugging and Timing Instrumentation

After the refactor, the useful timing points are:

- CPAL callback consumes samples
- main thread calls `pump_audio()`
- synchronous ray tracing executes during pump

The current request/service timing logs become obsolete and should be removed or replaced with pump-centric instrumentation.


## Risks

### Main-thread stalls can starve audio

This is the main cost of removing the internal producer thread.

Mitigation:

- keep a healthy high watermark
- avoid too-small refill thresholds
- allow one pump call to generate multiple chunks if occupancy is low

### Refactor scope inside PetalSonicEngine

This is a structural change, not a small patch.

Mitigation:

- preserve existing generation helpers
- migrate ownership carefully
- keep the CPAL callback path minimal and unchanged in behavior

### Removing the deferred RT path may expose hidden assumptions

The current service model and caches may be doing more than synchronization.

Mitigation:

- remove only the correctness-related parts first
- keep any performance cache only if profiling justifies it


## Acceptance Criteria

The refactor is successful when:

- PetalSonic no longer spawns or manages an internal producer/render thread
- CPAL callback still consumes from the ring buffer without heavy work
- `re-flora` pumps audio explicitly once per frame
- listener/source updates can be followed immediately by `pump_audio()`
- audio ray tracing callbacks execute synchronously on the main thread
- the deferred request/service synchronization path is removed
- normal gameplay does not show frequent ring buffer underruns


## Suggested Implementation Order

1. Refactor PetalSonic state ownership for synchronous pumping.
2. Add `pump_audio() -> Result<()>`.
3. Remove internal render thread creation and shutdown logic.
4. Wire `re-flora` to call `pump_audio()` once per frame.
5. Simplify audio ray tracing to direct synchronous tracing.
6. Remove obsolete timing/service code.
7. Validate underrun behavior and responsiveness in-game.


## Summary

The refactor replaces PetalSonic's autonomous producer thread with an explicit main-thread pump model.

This is the right direction for `re-flora` because it aligns audio generation with the game's authoritative state updates and removes the need for delayed cross-thread ray tracing service.

The CPAL callback remains device-driven and consumer-only, while production pacing moves to a simple per-frame `pump_audio() -> Result<()>` call with internal watermark management.
