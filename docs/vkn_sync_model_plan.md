# vkn managed synchronization plan

## Purpose

Build a clean, robust synchronization model inside `crates/re-flora-vkn` before adding a full profiler. The first objective is not to produce benchmark output; it is to make frame submission, swapchain acquire/present, fences, semaphores, queue ownership, and synchronization intent explicit, named, and owned by vkn without slowing normal release builds.

This document tracks the problem, design goals, and staged implementation steps.

## Problem statement

The renderer currently has reasonable wrapper types (`Fence`, `Semaphore`, `Swapchain`, `VulkanContext`), but synchronization is still partly exposed as low-level Vulkan objects and one-off helper calls:

- `Semaphore` and `Fence` expose raw handles through `as_raw()` and `Deref`.
- `Swapchain::acquire_next_image` requires the caller to provide an image-available semaphore.
- `VulkanContext::submit_render_commands` hard-codes one render submit shape.
- `Swapchain::present_after` accepts a specific semaphore, but the dependency is not named or represented as a vkn-level submit/present relationship.
- App code still reasons about images-in-flight, frame fences, and swapchain semaphores directly.
- There is no central per-frame sync state that can later explain queue lanes, waits, signals, or frame dependencies.

This makes future professional profiling harder. GPU timestamps can measure pass durations, but without managed submit/wait/signal metadata we cannot reliably explain overlap, wait gaps, or critical paths.

## Goals

- Keep synchronization ownership and Vulkan details inside `re-flora-vkn` as much as practical.
- Replace ad-hoc submit/acquire/present calls with semantic vkn APIs.
- Make sync relationships named and inspectable:
  - frame N waits on swapchain image availability
  - render submit signals render finished
  - present waits on render finished
  - image-in-flight waits are tracked by vkn
- Preserve current behavior and frame pacing.
- Avoid performance regressions in normal release builds.
- Keep APIs low-overhead: preallocated per-frame sync objects, no hot-path heap churn, no per-frame string formatting unless diagnostics are enabled.
- Leave an escape hatch for raw Vulkan handles where necessary, but make direct use uncommon and clearly low-level.
- Prepare clean extension points for a later profiler, without implementing the profiler in this phase.

## Non-goals for this phase

- Do not build the full profiler yet.
- Do not build a full render graph yet.
- Do not redesign rendering passes, shader layouts, terrain/water algorithms, or resource lifetimes.
- Do not require timeline semaphores immediately.
- Do not remove every raw `vk::` usage outside vkn in one pass.
- Do not add synchronous GPU query readback or per-frame benchmark logging.

## Design principles

### 1. Managed by default, raw as escape hatch

Normal app/render code should use vkn-owned frame and queue APIs. Raw handles may remain available for low-level code paths, but they should be documented as backend escape hatches rather than the normal sync model.

### 2. Intent-first API

Callers should describe synchronization intent instead of assembling `vk::SubmitInfo` manually.

Example target shape:

```rust
let frame = frame_scheduler.begin_frame(&mut swapchain)?;

frame.record(|frame, cmdbuf| {
    // app records render work
});

frame_scheduler.submit_and_present(frame)?;
```

Or for lower-level staging:

```rust
queue.submit(SubmitDesc {
    name: "main.render",
    command_buffers: &[cmdbuf],
    waits: &[Wait::swapchain_image_available(frame.image_available())],
    signals: &[Signal::render_finished(frame.render_finished())],
    fence: Some(frame.fence()),
});
```

### 3. Zero or near-zero overhead when diagnostics are off

The managed model should primarily reorganize existing sync operations. It should not add extra Vulkan synchronization. It should not allocate or log on every frame by default.

### 4. Explicit frame lifecycle

A frame should have a clear lifecycle:

1. wait for reusable frame resources
2. acquire swapchain image
3. wait for prior use of that swapchain image if needed
4. reset frame fence and command buffer
5. record commands
6. submit commands
7. present
8. advance frame index

Vkn should own this lifecycle or provide a single high-level abstraction that makes it difficult to misuse.

### 5. Future profiler compatibility

Even before profiling is implemented, APIs should carry stable optional names/IDs for:

- queues
- submits
- waits
- signals
- frame resources
- present operations

When diagnostics/profiling is disabled, names can be static `&'static str` or compiled out of hot paths.

## Current model notes

Relevant current files:

- `crates/re-flora-vkn/src/sync/semaphore.rs`
- `crates/re-flora-vkn/src/sync/fence.rs`
- `crates/re-flora-vkn/src/context/vulkan_context.rs`
- `crates/re-flora-vkn/src/swapchain.rs`
- `src/app/core/mod.rs`

Current top-level frame flow in app code includes:

- wait/reset current frame fence
- acquire swapchain image through `Swapchain::acquire_next_image`
- wait image-in-flight fence from app-owned array
- command buffer begin/end
- submit through `VulkanContext::submit_render_commands`
- present through `Swapchain::present_after`
- advance current frame

This is the first flow to encapsulate.

## Proposed target architecture

### `FrameSync`

A vkn-owned per-frame object containing:

- image-available semaphore
- render-finished semaphore
- in-flight fence
- command buffer reference or command-buffer slot reference, if appropriate
- stable frame-slot index

It replaces scattered app-level frame sync fields.

### `SwapchainFrame`

A short-lived acquired-frame token containing:

- swapchain image index
- frame slot index
- references/handles to required sync objects
- state marker that prevents submit/present before acquire

Target benefit: present cannot happen for a frame that was not acquired, and submit uses the matching sync objects.

### `FrameScheduler` or `SwapchainFrameManager`

A vkn-owned manager for:

- frames-in-flight
- images-in-flight tracking
- acquire
- frame fence wait/reset
- swapchain out-of-date/suboptimal handling
- submit + present glue

Naming is open. `FrameScheduler` is broader and future-friendly; `SwapchainFrameManager` is more precise for the first milestone.

### `QueueSubmitter`

A vkn queue wrapper that accepts a semantic submit descriptor:

```rust
pub struct SubmitDesc<'a> {
    pub name: &'static str,
    pub command_buffers: &'a [&'a CommandBuffer],
    pub waits: &'a [SubmitWait<'a>],
    pub signals: &'a [SubmitSignal<'a>],
    pub fence: Option<&'a Fence>,
}
```

First implementation may still support only the existing general queue. The important part is that raw `vk::SubmitInfo` construction becomes centralized and labeled.

### `SubmitWait` / `SubmitSignal`

Typed sync edges. Initially binary semaphores are enough:

```rust
pub struct SubmitWait<'a> {
    pub name: &'static str,
    pub semaphore: &'a Semaphore,
    pub stage: PipelineWaitStage,
}

pub struct SubmitSignal<'a> {
    pub name: &'static str,
    pub semaphore: &'a Semaphore,
}
```

`PipelineWaitStage` should be a small vkn semantic enum for common cases, translating to Vulkan stage masks internally.

### Present descriptor

Present should also be semantic:

```rust
pub struct PresentDesc<'a> {
    pub name: &'static str,
    pub image_index: u32,
    pub waits: &'a [&'a Semaphore],
}
```

Later this can carry profiler metadata for present waits/vsync behavior.

## Implementation steps

### Step 0: Baseline and audit

- Confirm current branch starts from `main`.
- Record current sync API call sites.
- Run baseline validation before sync changes:
  - `cargo fmt --check`
  - `cargo check`
  - `cargo test`
  - `cargo run --release -- --hidden --auto-exit 0.5`
- Optional: keep a short baseline `--perf` hidden run for regression comparison, but do not treat it as authoritative profiling.

### Step 1: Add labeled submit descriptors inside vkn

Status: done in branch `agent/vkn-profiler` after the initial planning commit.

- Add `SubmitDesc`, `SubmitWait`, `SubmitSignal`, and `PipelineWaitStage` to vkn.
- Implement a general-queue submit method that translates descriptors to `vk::SubmitInfo`.
- Make `VulkanContext::submit_render_commands` call the new descriptor API internally.
- Do not change app behavior yet.

Validation:

- `cargo check`
- hidden release smoke run

Performance risk: very low if descriptor slices are stack-backed and no logging/allocation is added.

### Step 2: Add semantic present descriptor

Status: done in branch `agent/vkn-profiler` after Step 1.

- Add `PresentDesc` to vkn.
- Make `Swapchain::present_after` call `present_desc` internally.
- Keep existing public methods temporarily for compatibility.

Validation:

- `cargo check`
- hidden release smoke run

### Step 3: Encapsulate per-frame sync objects

Status: done in branch `agent/vkn-profiler` after Step 2 for the frame-in-flight bundle. Render-finished semaphores remain per swapchain image until Step 4 moves image-in-flight tracking into vkn.

- Add `FrameSync` in vkn for image-available semaphore, render-finished semaphore, fence, and possibly command buffer slot ownership.
- Move construction of frame semaphores/fences into vkn-owned helper/manager.
- Keep app-facing accessors minimal.

Validation:

- `cargo check`
- hidden release smoke run
- inspect logs for swapchain/acquire/present errors

### Step 4: Move images-in-flight tracking into vkn

Status: done in branch `agent/vkn-profiler` after Step 3. `SwapchainFrameManager` now owns frame slots, per-image render-finished semaphores, image-in-flight fences, and current-frame advancement. Submit/present still remain explicit in the app until Step 5.

- Introduce `SwapchainFrameManager` or equivalent.
- It owns `images_in_flight: Vec<Option<Fence>>` and frame-slot advancement.
- `begin_frame` handles:
  - current frame fence wait
  - acquire image
  - image-in-flight wait
  - fence reset
- App receives an acquired-frame token with image index and command buffer/frame slot references.

Validation:

- `cargo check`
- `cargo test`
- hidden release smoke run
- resize/manual window smoke if practical

Performance risk: must not add additional waits beyond the current sequence. The manager should reproduce the existing ordering first.

### Step 5: Move submit/present lifecycle into vkn

Status: done in branch `agent/vkn-profiler` after Step 4. The app still records commands and handles presentation outcomes, but render submit, present, and frame advancement now go through `SwapchainFrameManager::submit_and_present`.

- Add `submit_and_present` or equivalent on the frame manager.
- App stops manually calling:
  - `sync.fence.wait()`
  - `swapchain.acquire_next_image(...)`
  - `image_in_flight_fence.wait()`
  - `sync.fence.reset()`
  - `vulkan_ctx.submit_render_commands(...)`
  - `swapchain.present_after(...)`
  - manual current-frame advancement
- App still records commands and chooses render content.

Validation:

- `cargo fmt --check`
- `cargo check`
- `cargo test`
- `cargo run --release -- --hidden --auto-exit 0.5`
- `cargo run --release -- --hidden --auto-exit 4 --perf`

### Step 6: Make raw sync access uncommon

- Remove `Deref<Target = vk::Semaphore>` and `Deref<Target = vk::Fence>` if call sites no longer need it.
- Keep `as_raw()` as crate-visible or clearly documented escape hatch if possible.
- Prefer semantic helpers for common waits, resets, and status checks.

Validation:

- `rg "as_raw\(\).*Semaphore|as_raw\(\).*Fence|vk::Semaphore|vk::Fence" src crates/re-flora-vkn/src`
- `cargo check`

### Step 7: Add optional diagnostics hooks, not profiler output

- Add internal no-op hooks or event structs that can record submit names and wait/signal names when a future feature is enabled.
- Keep disabled by default.
- No timestamp queries, no benchmark files, no per-frame logging in this phase.

Validation:

- verify disabled path has no heap allocation in hot frame flow
- release smoke run

## Performance guardrails

- No extra queue submissions compared with current frame flow.
- No extra semaphores or fences per frame after initialization.
- No per-frame heap allocation in the normal path.
- No per-frame `String` formatting in the normal path.
- No synchronous GPU waits except the waits already present in the current frame lifecycle.
- No immediate query readback.
- No debug/profiling logging unless explicitly enabled.
- Preserve existing present-mode behavior.
- Preserve existing frames-in-flight and swapchain image count behavior.

## Robustness guardrails

- Treat frame lifecycle as stateful: acquired frames should not be submitted twice or presented before submit.
- Handle `OutOfDate` and suboptimal swapchain results without panics where the current code already recovers.
- Keep swapchain resize/recreate paths explicit and tested.
- Make shutdown/drop ordering clear: wait for device idle where needed before destroying swapchain-owned frame resources.
- Prefer typed errors over panics for acquire/present/submit failures that can happen during resize.
- Keep screenshot readback path compatible with frame fence ownership.

## Future profiler handoff

After sync is managed, profiler work can build on the same model:

- submit names become queue timeline events
- wait/signal names become dependency edges
- frame tokens provide frame IDs and image indices
- present descriptors expose present waits
- command-buffer GPU scopes can attach to the submit/frame that contains them

The profiler should then answer:

- CPU frame breakdown
- GPU pass durations
- queue overlap
- semaphore wait chains
- critical path
- p50/p95/p99 frame cost

But this phase ends before implementing those outputs.

## Open questions

- Name: `FrameScheduler`, `SwapchainFrameManager`, or `FrameLoop`?
- Should command buffers be owned by the frame manager immediately, or stay in app-owned `frames_in_flight` for the first pass?
- Should `as_raw()` remain public on sync primitives, or become restricted after migration?
- Should we introduce timeline semaphores later for non-swapchain queue dependencies, or keep binary semaphores until there is a concrete multi-queue need?
- Should present/acquire be modeled as special queue events now, or only once the profiler starts?

## Suggested first milestone

Keep the first code change small:

1. Add semantic submit/present descriptors.
2. Route existing `submit_render_commands` and `present_after` through them.
3. Do not change app frame lifecycle yet.
4. Validate no behavior/performance regression.

This creates the API seam needed for managed sync while keeping risk low.
