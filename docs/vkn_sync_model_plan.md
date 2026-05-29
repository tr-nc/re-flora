# vkn managed synchronization plan

## Purpose

Build a clean, robust synchronization model inside `crates/re-flora-vkn` before adding a full profiler. The first objective is not to produce benchmark output; it is to make frame submission, swapchain acquire/present, chunk-build compute submissions, readback jobs, fences, semaphores, queue ownership, and synchronization intent explicit, named, and owned by vkn without slowing normal release builds.

This document tracks the problem, design goals, and staged implementation steps.

## Problem statement

The renderer currently has reasonable wrapper types (`Fence`, `Semaphore`, `Swapchain`, `VulkanContext`), but synchronization is still partly exposed as low-level Vulkan objects and one-off helper calls:

- `Semaphore` and `Fence` expose raw handles through `as_raw()` and `Deref`.
- `Swapchain::acquire_next_image` requires the caller to provide an image-available semaphore.
- `VulkanContext::submit_render_commands` hard-codes one render submit shape.
- `Swapchain::present_after` accepts a specific semaphore, but the dependency is not named or represented as a vkn-level submit/present relationship.
- App code still reasons about images-in-flight, frame fences, and swapchain semaphores directly.
- Chunk builders, terrain rebuilds, acceleration-structure builds, voxelization, and GPU readback paths still own their own job fences and polling/wait behavior outside a vkn-managed job model.
- One-time compute/readback helpers centralize some raw submission mechanics, but they do not expose a stable semantic job lifecycle that a profiler can understand.
- There is no central per-frame or per-GPU-job sync state that can later explain queue lanes, waits, signals, fence latency, readback latency, or frame/job dependencies.

This makes future professional profiling harder. GPU timestamps can measure pass durations, but without managed submit/wait/signal/fence metadata we cannot reliably explain overlap, wait gaps, readback stalls, queue pressure, or critical paths.

## Goals

- Keep synchronization ownership and Vulkan details inside `re-flora-vkn` as much as practical.
- Replace ad-hoc submit/acquire/present calls with semantic vkn APIs.
- Make sync relationships named and inspectable:
  - frame N waits on swapchain image availability
  - render submit signals render finished
  - present waits on render finished
  - image-in-flight waits are tracked by vkn
  - chunk/terrain/voxel/acceleration-structure compute jobs submit with stable names
  - fence-backed job completion and CPU readback waits are tracked by vkn
- Preserve current behavior and frame pacing.
- Avoid performance regressions in normal release builds.
- Keep APIs low-overhead: preallocated per-frame sync objects, no hot-path heap churn, no per-frame string formatting unless diagnostics are enabled.
- Leave an escape hatch for raw Vulkan handles where necessary, but make direct use uncommon and clearly low-level.
- Prepare clean extension points for a later profiler, without implementing the profiler in this phase.
- Cover both swapchain frame sync and off-frame GPU jobs; the profiler foundation should not only understand presentation.

## Non-goals for this phase

- Do not build the full profiler yet.
- Do not build a full render graph yet.
- Do not redesign rendering passes, shader layouts, terrain/water algorithms, or resource lifetimes.
- Do not require timeline semaphores immediately.
- Do not remove every raw `vk::` usage outside vkn in one pass.
- Do not add synchronous GPU query readback or per-frame benchmark logging.

## Design principles

### 1. Managed by default, raw as escape hatch

Normal app/render/chunk-build code should use vkn-owned frame, job, and queue APIs. Raw handles may remain available for low-level code paths, but they should be documented as backend escape hatches rather than the normal sync model.

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

Chunk and readback work should use the same intent-first model:

```rust
let job = gpu_jobs.submit(GpuJobDesc {
    name: "terrain.source_sample",
    queue: QueueLane::General,
    command_buffer: cmdbuf,
    completion: JobCompletion::Fence,
});

if job.is_complete()? {
    let readback = job.finish_readback()?;
}
```

### 3. Zero or near-zero overhead when diagnostics are off

The managed model should primarily reorganize existing sync operations. It should not add extra Vulkan synchronization. It should not allocate or log on every frame or every chunk job by default.

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

Async GPU jobs should have a similarly explicit lifecycle:

1. allocate or reuse job resources
2. record compute/build/copy commands
3. submit with a named job descriptor
4. poll or wait for completion through a vkn token
5. perform readback or publish results only after completion
6. recycle job resources safely

The app/builder layer may decide what work to do and how to use results, but vkn should own the synchronization mechanics.

### 5. Future profiler compatibility

Even before profiling is implemented, APIs should carry stable optional names/IDs for:

- queues
- submits
- waits
- signals
- frame resources
- present operations
- async compute/build/readback jobs
- job fences and completion polls/waits

When diagnostics/profiling is disabled, names can be static `&'static str` or compiled out of hot paths.

## Current model notes

Relevant current files:

- `crates/re-flora-vkn/src/sync/semaphore.rs`
- `crates/re-flora-vkn/src/sync/fence.rs`
- `crates/re-flora-vkn/src/sync/frame.rs`
- `crates/re-flora-vkn/src/sync/submit.rs`
- `crates/re-flora-vkn/src/sync/present.rs`
- `crates/re-flora-vkn/src/context/vulkan_context.rs`
- `crates/re-flora-vkn/src/command/command_buffer.rs`
- `crates/re-flora-vkn/src/swapchain.rs`
- `src/app/core/mod.rs`
- `src/builder/contree/mod.rs`
- `src/builder/plain/mod.rs`
- `src/builder/surface/mod.rs`
- `src/builder/scene_accel/mod.rs`
- terrain/water GPU sampling and readback call sites under `src/app/core/water` and `src/app/core/terrain_rebuild.rs`

Current top-level frame flow in app code includes:

- wait/reset current frame fence
- acquire swapchain image through `Swapchain::acquire_next_image`
- wait image-in-flight fence from app-owned array
- command buffer begin/end
- submit through `VulkanContext::submit_render_commands`
- present through `Swapchain::present_after`
- advance current frame

This was the first flow to encapsulate. The next sync gap is off-frame GPU work:

- chunk terrain source sampling submits GPU work and waits/polls fences for CPU readback
- collider/source rebuild queues track active jobs and completion outside vkn
- voxelization and builder paths create command buffers and fences for asynchronous compute/build jobs
- acceleration-structure and surface builders wait on job fences directly
- one-time command helpers submit work with optional fences or queue-idle waits

Those jobs are not swapchain frames, but they still contribute to frame time, queue pressure, and user-visible stalls. They need the same semantic sync model before the profiler can explain the whole renderer.

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

### `GpuJobDesc`

A vkn-owned descriptor for non-swapchain GPU work: compute dispatches, terrain/chunk builds, acceleration-structure builds, blits/copies, and CPU readbacks.

Target shape:

```rust
pub struct GpuJobDesc<'a> {
    pub name: &'static str,
    pub queue: QueueLane,
    pub command_buffers: &'a [&'a CommandBuffer],
    pub waits: &'a [SubmitWait<'a>],
    pub signals: &'a [SubmitSignal<'a>],
    pub completion: JobCompletion,
}
```

Initial implementation can be fence-backed and internally reuse `SubmitDesc`. The important part is that callers submit a named job and receive a typed completion token instead of manually owning raw fence behavior.

### `GpuJobToken`

A short-lived or pool-owned token for submitted asynchronous GPU jobs:

- stable job name
- queue lane
- submission generation/index
- completion fence or future timeline value
- optional readback/result metadata supplied by the caller

Target app/builder interactions:

- `is_complete()` for polling without blocking
- `wait()` for explicit blocking paths
- `finish()` / `finish_readback()` for completion-sensitive result publication
- no raw fence access in normal builder/app code

### `GpuJobManager`

A vkn-owned manager for reusable async job synchronization:

- owns or recycles fence-backed job slots
- centralizes submit/poll/wait/reset ordering
- supports existing general-queue jobs first
- can later expose transfer/compute queue lanes if real multi-queue scheduling is introduced
- emits optional diagnostics events for submit, poll, wait, completion, and readback publication

This manager should not decide terrain/chunk algorithms. The app/builder layer remains responsible for choosing work and interpreting outputs; vkn owns the synchronization lifecycle.

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

Status: done in branch `agent/vkn-profiler` after Step 5. `Fence` and `Semaphore` no longer deref to Vulkan handles, their raw handles are crate-visible only, raw acquire/present helpers are no longer public app APIs, and screenshot readback waits use `AcquiredFrame::wait_until_complete` instead of reaching through to the frame fence.

- Remove `Deref<Target = vk::Semaphore>` and `Deref<Target = vk::Fence>` if call sites no longer need it.
- Keep `as_raw()` as crate-visible or clearly documented escape hatch if possible.
- Prefer semantic helpers for common waits, resets, and status checks.

Validation:

- `rg "as_raw\(\).*Semaphore|as_raw\(\).*Fence|vk::Semaphore|vk::Fence" src crates/re-flora-vkn/src`
- `cargo check`

### Step 7: Add optional diagnostics hooks, not profiler output

Status: done in branch `agent/vkn-profiler` after Step 6. `sync_diagnostics` is an opt-in feature with submit/present event shapes and no-op hooks at the descriptor dispatch points. The default build keeps the hooks disabled and does not log, allocate, add timestamp queries, or add GPU waits.

- Add internal no-op hooks or event structs that can record submit names and wait/signal names when a future feature is enabled.
- Keep disabled by default.
- No timestamp queries, no benchmark files, no per-frame logging in this phase.

Validation:

- verify disabled path has no heap allocation in hot frame flow
- release smoke run

### Step 8: Audit off-frame GPU sync call sites

Status: next. This starts the second milestone: managed sync for chunk-build, compute, builder, and readback jobs.

- Inventory all direct `Fence` polling/waiting in app and builder code.
- Classify jobs by behavior:
  - synchronous one-time command with queue idle
  - synchronous one-time command with fence wait
  - asynchronous compute/build job with polling
  - asynchronous job with CPU readback
  - long-lived builder queue job
- Record the current ordering and wait behavior before changing it.
- Identify which jobs can share a generic `GpuJobManager` immediately and which need a narrower adapter first.

Audit result from this pass:

| Area | Current sync shape | Migration target |
| --- | --- | --- |
| `execute_one_time_command` | one-time command submit followed by queue idle | named vkn helper backed by `GpuJobDesc`, preserving queue-idle wait |
| `execute_one_time_command_with_fence` | one-time command submit plus local fence wait | named fence-backed vkn job wait |
| `PlainBuilder::ChunkSolidSampleJob` | async compute/readback job with command buffer + fence in app job struct | replace fence with `GpuJobToken`, keep readback/result fields in builder |
| `SurfaceBuilder::SurfaceBuildJob` | async compute job with fence polling/wait and readback in finish | replace fence with `GpuJobToken` |
| `SceneAccelBuilder::SceneTexUpdateJob` | reused command buffer submitted with new fence, polled/waited by builder | replace fence with `GpuJobToken` |
| `ContreeBuilder::ContreeBuildJob` | reused command buffer submitted with new fence, allocator ownership held by builder | replace fence with `GpuJobToken`, keep allocator rollback in builder |
| `ContreeBuilder::CpuChunkCacheFenceJob` | copy-to-readback command with fence, then CPU decode worker | replace fence with `GpuJobToken`, keep readback buffers and worker handoff in builder |
| direct no-fence builder submits | fire-and-forget command submit | keep on `SubmitDesc`; optionally name through job helper later if profiler needs fire-and-forget events |

Validation:

- `rg "Fence|\.wait\(\)|\.is_signaled\(\)|submit\(" src/builder src/app crates/re-flora-vkn/src`
- no code changes beyond documentation/audit notes in this step

### Step 9: Add a vkn-managed GPU job abstraction

Status: done in branch `agent/vkn-profiler` after the off-frame sync audit. Added fence-backed `GpuJobDesc`, `GpuJobToken`, `GpuJobManager`, `JobCompletion`, and `QueueLane` in vkn. Migration of call sites happens in later steps.

- Add `GpuJobDesc`, `GpuJobToken`, `JobCompletion`, and possibly `QueueLane` under `crates/re-flora-vkn/src/sync` or a sibling job module.
- Implement fence-backed jobs first, internally routing through `SubmitDesc`.
- Provide polling and waiting through token methods:
  - `is_complete()`
  - `wait()`
  - `finish()` or `take_completed()` if ownership needs to move
- Keep raw fence access crate-internal.
- Do not change chunk algorithms or result formats.

Validation:

- `cargo fmt --check`
- `cargo check`
- targeted tests if pure logic is added

Performance risk: low if the first implementation reuses existing fence behavior and avoids per-job allocation beyond existing job creation paths.

### Step 10: Migrate one-time command helpers

Status: done in branch `agent/vkn-profiler` after Step 9. `execute_one_time_command_with_fence` now submits through a fence-backed `GpuJobToken`, while `execute_one_time_command` keeps its queue-idle behavior and uses a named semantic submit descriptor without adding a fence.

- Route `execute_one_time_command` and `execute_one_time_command_with_fence` through the new job abstraction where practical.
- Preserve existing queue-idle behavior for paths that intentionally use it for MoltenVK stability.
- Ensure synchronous readback helpers still block only where they blocked before.
- Add names for common helpers such as transfer copy, terrain source sampling, voxelization, and readback.

Validation:

- `cargo check`
- `cargo test`
- hidden release smoke run
- inspect water/terrain logs for changed fence latency or errors

### Step 11: Migrate chunk/build async jobs

- Move direct builder/app ownership of completion fences behind vkn job tokens or small vkn-backed adapters.
- Target call sites include:
  - `src/builder/contree/mod.rs`
  - `src/builder/plain/mod.rs`
  - `src/builder/surface/mod.rs`
  - `src/builder/scene_accel/mod.rs`
  - terrain source/collider/readback queues in app core modules
- Preserve queue scheduling, active/pending queue behavior, and result handoff semantics.
- Keep app/builder code responsible for job payloads and result interpretation; vkn owns submit/poll/wait/reset.

Validation:

- `cargo fmt --check`
- `cargo check`
- `cargo test`
- `cargo run --release -- --hidden --auto-exit 0.5`
- `cargo run --release -- --hidden --auto-exit 4 --perf`
- inspect logs for terrain rebuild, collider build, source readback, voxelization, and shutdown errors

### Step 12: Extend diagnostics hooks to GPU jobs

- Add no-op default hooks for job submit, poll, wait, complete, and readback-finish events.
- Keep `sync_diagnostics` opt-in and allocation/log-free unless a later sink is explicitly enabled.
- Carry stable names and IDs so future profiler output can connect chunk jobs to frame stalls and queue pressure.

Validation:

- `cargo check`
- `cargo check --features sync_diagnostics`
- release smoke run with default features

## Performance guardrails

- No extra queue submissions compared with current frame or job flow.
- No extra semaphores or fences per frame after initialization.
- No unbounded per-job sync allocation in hot chunk/build paths; reuse existing job allocation points or vkn job pools where possible.
- No per-frame or per-job heap allocation in the normal path unless the old path already allocated for that job.
- No per-frame or per-job `String` formatting in the normal path.
- No synchronous GPU waits except the waits already present in the current frame lifecycle or existing compute/readback job lifecycle.
- No immediate query readback.
- No debug/profiling logging unless explicitly enabled.
- Preserve existing present-mode behavior.
- Preserve existing frames-in-flight and swapchain image count behavior.
- Preserve existing chunk queue pacing, active/pending limits, readback timing, and MoltenVK stability workarounds.

## Robustness guardrails

- Treat frame lifecycle as stateful: acquired frames should not be submitted twice or presented before submit.
- Treat GPU job lifecycle as stateful: jobs should not be finished before completion, waited after resource recycling, or reused without reset.
- Handle `OutOfDate` and suboptimal swapchain results without panics where the current code already recovers.
- Keep swapchain resize/recreate paths explicit and tested.
- Make shutdown/drop ordering clear: wait for device idle where needed before destroying swapchain-owned frame resources.
- Prefer typed errors over panics for acquire/present/submit failures that can happen during resize.
- Keep screenshot readback path compatible with frame fence ownership.
- Keep terrain/chunk readback paths compatible with job completion ownership.

## Future profiler handoff

After sync is managed, profiler work can build on the same model:

- submit names become queue timeline events
- wait/signal names become dependency edges
- frame tokens provide frame IDs and image indices
- present descriptors expose present waits
- command-buffer GPU scopes can attach to the submit/frame/job that contains them
- GPU job tokens provide job IDs, queue lanes, names, completion states, and readback points
- chunk/build/readback diagnostics explain queue pressure outside the main render frame

The profiler should then answer:

- CPU frame breakdown
- GPU pass durations
- queue overlap
- semaphore wait chains
- fence wait and poll behavior
- chunk build / compute job contribution to frame stalls
- readback latency and publication delays
- critical path
- p50/p95/p99 frame cost

But this phase ends before implementing those outputs.

## Open questions

- Name: `FrameScheduler`, `SwapchainFrameManager`, or `FrameLoop`?
- Should command buffers be owned by the frame manager immediately, or stay in app-owned `frames_in_flight` for the first pass?
- Should `as_raw()` remain public on sync primitives, or become restricted after migration?
- Should we introduce timeline semaphores later for non-swapchain queue dependencies, or keep binary semaphores until there is a concrete multi-queue need?
- Should present/acquire be modeled as special queue events now, or only once the profiler starts?
- Should the async job manager start as a simple fence-backed pool, or should it immediately reserve room for timeline semaphores?
- Should builder modules own job payload structs while vkn owns only completion tokens, or should vkn provide a generic typed job slot?
- Which current queue-idle one-time helpers are stability requirements and which can safely become fence waits?

## Suggested next milestone

Keep the next code change small:

1. Audit builder/app fence call sites and classify job types.
2. Add a fence-backed `GpuJobDesc` / `GpuJobToken` abstraction inside vkn.
3. Migrate one narrow helper or builder path first, preferably one with clear validation logs.
4. Validate no behavior/performance regression before migrating the broader chunk-build queues.

This extends the managed sync seam from swapchain frames to off-frame GPU jobs while keeping risk low.
