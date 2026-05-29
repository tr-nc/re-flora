# VKN GPU Profiler Plan

## Goal

Build a professional, low-overhead GPU profiler foundation inside `crates/re-flora-vkn`.

The profiler should let the app and renderer attach semantic names to GPU work without exposing Vulkan synchronization, query-pool, barrier, or timestamp details outside vkn. It should eventually explain frame cost across render passes, compute jobs, readbacks, synchronization edges, and resource transitions.

This document tracks the profiler-specific work that follows the managed sync/resource-state cleanup in `docs/vkn_sync_model_plan.md`.

## Non-goals for the first pass

- Do not build a full render graph.
- Do not rewrite chunk builders or tracer scheduling.
- Do not add per-frame logging by default.
- Do not introduce immediate GPU waits for timestamp readback.
- Do not require app/builder code to use raw fences, semaphores, query pools, or Vulkan barrier types.

## Design principles

- **vkn owns Vulkan mechanics**: query pools, timestamp periods, timestamp stages, availability checks, and raw Vulkan conversion stay in `re-flora-vkn`.
- **Runtime opt-in**: normal runs should not allocate profiler query pools or record timestamp commands unless profiling is enabled.
- **No default stalls**: timestamp results are read only after the relevant frame/job completion is known, or through a non-blocking read path.
- **Fixed-capacity hot path**: avoid per-frame heap allocation in normal profiler recording. Overflow should drop extra scopes and count drops, not panic in release.
- **Semantic names**: call sites should use `&'static str` labels such as `tracer.render`, `water.step`, or `terrain.source_readback`.
- **Use existing seams**: build on `SwapchainFrameManager`, `GpuJobToken`, `SubmitDesc`, `PresentDesc`, and `TextureTransition` diagnostics rather than adding new app-side Vulkan knowledge.

## Verification method

For each code phase, validate before committing:

```bash
cargo fmt --check
cargo check
cargo check --features sync_diagnostics
cargo test
cargo run --release -- --hidden --auto-exit 0.5
```

For profiler/perf-facing phases, also run:

```bash
cargo run --release -- --hidden --auto-exit 4 --perf
```

Inspect the latest run log from the same worktree:

```bash
cargo run --release -- --latest-log
cargo run --release -- --tail-latest-log 200
```

Search for concrete failures:

```bash
rg -i "error|panic|failed|validation" <latest-log>
```

Performance validation guidelines:

- Compare release hidden runs, not debug builds.
- Check that profiler-disabled runs do not emit new per-frame logs.
- Check that timestamp result readback does not introduce GPU waits in the frame being measured.
- If a phase changes pacing or GPU work, run `--hidden --auto-exit 4 --perf` before and after and compare `[PERF][FRAME]` lines.

## Current foundation

Already completed on branch `agent/vkn-profiler`:

- Swapchain frame synchronization is managed inside vkn.
- Off-frame GPU jobs use `GpuJobToken` rather than raw fences in app/builder code.
- Submit, present, GPU job, and texture-transition diagnostics hooks exist and are no-op by default.
- Resource barrier/layout/stage/access types are semantic vkn wrappers.
- App-side code no longer uses `vk::ImageLayout`, `vk::AccessFlags`, `vk::PipelineStageFlags`, raw image transition helpers, raw `Fence`, or raw `Semaphore`.
- Remaining app-side `vk::` types are descriptive resource/pipeline fields such as `vk::Format`, usage flags, descriptor types, shader stage flags, load/store ops, and clear/rect structs.

## Phase 1: Timestamp query primitives in vkn

Status: done in branch `agent/vkn-profiler`.

Added the minimal vkn-owned timestamp building blocks.

Tasks:

- Extend `TimestampQueryPool` with non-blocking result retrieval.
- Keep the existing blocking `read_u64` path for tools/tests where appropriate, but do not use it in the live profiler hot path.
- Add a small profiler wrapper that owns per-frame query ranges and timestamp period conversion.
- Use semantic `PipelineStage` for timestamp writes.
- Represent scope metadata with fixed-capacity storage and `&'static str` names.

Implemented API shape:

```rust
GpuProfiler::maybe_new(...)
GpuProfiler::begin_frame(frame_slot, cmdbuf)
GpuProfilerFrame::begin_scope(cmdbuf, "tracer.render", PipelineStage::ALL_COMMANDS)
GpuProfilerFrame::end_scope(cmdbuf, scope, PipelineStage::ALL_COMMANDS)
GpuProfiler::try_collect_frame(frame_slot)
```

Validation:

- `cargo fmt --check`
- `cargo check`
- `cargo check --features sync_diagnostics`
- `cargo test`
- `cargo run --release -- --hidden --auto-exit 0.5`
- inspect hidden-run log for errors, panics, failures, and validation messages

## Phase 2: Frame-slot integration

Status: done in branch `agent/vkn-profiler`.

Tied timestamp result lifetime to existing managed frame completion.

Tasks:

- Associate profiler frame slots with `SwapchainFrameManager` frame slots.
- Reset query ranges at the start of a profiled frame.
- Read frame N results only after the corresponding frame fence/completion is known.
- Skip unavailable results without blocking.
- Expose dropped-scope counts and availability status.

Implementation notes:

- `App` creates `GpuProfiler` only when `--perf` is enabled.
- After `SwapchainFrameManager::begin_frame` returns, the current frame slot's previous fence has already been waited by vkn, so app collection uses `GpuProfiler::try_collect_frame` before resetting the same slot's query range.
- Profiler query reset is recorded at the start of the new command buffer for that frame slot.

Validation:

- `cargo fmt --check`
- `cargo check`
- `cargo run --release -- --hidden --auto-exit 0.5`
- `cargo run --release -- --hidden --auto-exit 4 --perf`
- inspect hidden-run logs for errors, panics, failures, and validation messages

## Phase 3: First render scopes

Status: done in branch `agent/vkn-profiler`.

Recorded a small number of high-value GPU scopes in the main render path.

Implemented scopes:

- `frame.render`
- `tracer.render`
- `egui.render`

Tasks:

- Keep call sites semantic and short.
- Avoid wrapping every small command in the first pass.
- Surface results in the existing frame timing panel only when profiling/perf is enabled.

Implementation notes:

- `GpuProfiler` now also exposes direct `begin_scope` / `end_scope` methods for app integration without holding a long mutable frame borrow.
- Loading and normal render paths both record `frame.render`; normal render also records `tracer.render`, and both paths record `egui.render`.
- The timing panel shows up to 12 collected GPU scopes with microsecond durations and dropped-scope count.

Validation:

- `cargo fmt --check`
- `cargo check`
- `cargo check --features sync_diagnostics`
- `cargo test`
- `cargo run --release -- --hidden --auto-exit 0.5`
- `cargo run --release -- --hidden --auto-exit 4 --perf`
- inspect hidden-run logs for errors, panics, failures, and validation messages

## Phase 4: GPU job scopes

Status: done in branch `agent/vkn-profiler` for the first representative job.

Attached timestamp scopes to off-frame GPU jobs such as chunk builds, compute jobs, and readbacks.

Tasks:

- Connect scopes to `GpuJobToken`/job names and queue lanes.
- Collect job timestamp results after job completion, never by waiting solely for profiler data.
- Start with one or two representative jobs before migrating all builders.

Implementation notes:

- Added `GpuJobProfiler`, `GpuJobScopeToken`, and `GpuJobScopeResult` in vkn.
- The first app integration is `surface.build`, enabled only under `--perf` via `SurfaceBuilder::enable_gpu_job_profiling`.
- Scope results are collected in `finish_build_surface`, after the corresponding `GpuJobToken` is already complete.
- Job scope results log as `[PERF][GPU_JOB_SCOPE]` and keep queue/name metadata for future aggregation.

Validation:

- `cargo fmt --check`
- `cargo check`
- `cargo check --features sync_diagnostics`
- `cargo test`
- `cargo run --release -- --hidden --auto-exit 0.5`
- `cargo run --release -- --hidden --auto-exit 4 --perf`
- inspect hidden-run logs for errors, panics, failures, and validation messages

## Phase 5: Transition/barrier diagnostics sink

Status: done in branch `agent/vkn-profiler`.

Turned the existing texture-transition diagnostics hook into optional profiler data.

Tasks:

- Keep default hook no-op.
- When enabled, record transition events with old/new layout, stage, access, aspect, and layer range.
- Correlate transitions with frame/job context if available.
- Avoid per-transition string formatting in the hot path.

Implementation notes:

- `TextureTransitionDiagnostics` is now a stable public event shape in `re_flora_vkn::sync::diagnostics`.
- Added optional `set_texture_transition_diagnostics_sink` registration behind the existing diagnostics seam.
- Default builds still compile to a no-op sink path and return `false` for sink registration.
- `sync_diagnostics` builds forward transition events to the registered function pointer without formatting or allocation in the hook.

Validation:

- `cargo fmt --check`
- `cargo check`
- `cargo check --features sync_diagnostics`
- `cargo test`
- `cargo run --release -- --hidden --auto-exit 0.5`
- `cargo run --release -- --hidden --auto-exit 4 --perf`
- inspect hidden-run logs for errors, panics, failures, and validation messages

## Phase 6: Profiler presentation

Status: done in branch `agent/vkn-profiler` for the first presentation pass.

Exposed collected data in a concise agent-friendly form.

Tasks:

- Extend the existing frame timing GUI/log path with GPU scope durations.
- Keep UI plain text and lightweight.
- Prefer microseconds and stable labels.
- Eventually support p50/p95/p99 summaries, but only after raw frame/job scopes are reliable.

Implementation notes:

- The frame timing GUI shows GPU frame scopes as plain-text microsecond rows.
- `--perf` logs `[PERF][GPU_FRAME_SCOPE]` periodically with stable scope labels and dropped-scope count.
- GPU job scopes log as `[PERF][GPU_JOB_SCOPE]` when their jobs finish.
- Disabled runs do not allocate the app GPU profiler or surface GPU job profiler.

Validation:

- `cargo fmt --check`
- `cargo check`
- `cargo check --features sync_diagnostics`
- `cargo test`
- `cargo run --release -- --hidden --auto-exit 0.5`
- `cargo run --release -- --hidden --auto-exit 4 --perf`
- inspect hidden-run logs for errors, panics, failures, and validation messages

## Open questions

- Should profiling be enabled by existing `--perf`, or by a separate `--gpu-profiler` flag with `--perf` implying it later?
- What fixed scope capacity is enough for the first pass: 64, 128, or 256 scopes per frame?
- Should scope overflow be visible in the GUI/log immediately, or only in debug diagnostics?
- Should GPU job profiling share the frame profiler query pool or use separate per-job pools/ranges?
- How should transition diagnostics be correlated with command buffers before a full render graph exists?

## Immediate next step

The first profiler primitive is complete through initial presentation. Keep future work incremental:

1. Add aggregation summaries such as p50/p95/p99 for frame scopes and representative GPU jobs.
2. Expand GPU job profiling beyond `surface.build` only after measuring the current log usefulness.
3. Add a richer transition diagnostics sink only when a concrete profiler view needs transition counts or state-change timelines.
