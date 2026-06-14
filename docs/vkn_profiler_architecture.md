# VKN sync and GPU profiler architecture

This document summarizes the current `verdarium-vkn` synchronization/profiler foundation. It replaces the older phase-by-phase sync and profiler plans.

## Current foundation

`verdarium-vkn` owns the Vulkan synchronization mechanics for normal rendering and off-frame GPU work:

- swapchain frame acquire/submit/present lifecycle;
- frame semaphores, fences, and images-in-flight tracking;
- semantic submit and present descriptors;
- fence-backed off-frame GPU jobs through `GpuJobToken`;
- semantic pipeline stages, memory access flags, texture layouts, and texture transitions;
- optional diagnostics hooks for submits, presents, GPU jobs, and texture transitions.

App and builder code should not assemble raw Vulkan sync objects or barriers directly. Remaining app-side `vk::` usage is mostly descriptive resource/pipeline data such as formats, usage flags, descriptor types, and render-pass settings.

## Managed frame lifecycle

`SwapchainFrameManager` owns the per-frame path:

```text
wait reusable frame slot
acquire swapchain image
wait prior use of that image
reset frame fence and command buffer
record app commands
submit render work
present
advance frame slot
```

The app still decides what to render, but vkn owns the acquire/submit/present synchronization sequence.

## Managed GPU jobs

Off-frame builder/readback work uses named GPU job tokens instead of direct fence ownership.

Migrated representative jobs include:

- plain chunk solid sampling;
- surface builds;
- scene texture updates;
- contree builds;
- contree CPU-cache readbacks;
- one-time command helpers where practical.

Builder modules still own job payloads and result interpretation. Vkn owns submit, poll, wait, and completion mechanics.

## Semantic resource state

Texture/resource transitions now flow through vkn-owned semantic state:

- `TextureLayout`
- `PipelineStage`
- `MemoryAccess`
- `ResourceState`
- `TextureTransition`

The central image transition path emits optional diagnostics events, giving future profiler views resource-state timelines without reintroducing app-side Vulkan layout/access knowledge.

## GPU profiler

The GPU profiler is runtime opt-in and currently enabled by `--perf`.

Implemented primitives:

- `GpuProfiler` for frame-slot timestamp scopes;
- non-blocking timestamp result collection after frame-slot completion;
- fixed-capacity frame scopes with dropped-scope counts;
- `GpuJobProfiler` for representative off-frame job scopes;
- concise GUI/log presentation of collected scope durations.

Current high-value scopes include:

- `frame.render`
- `tracer.render`
- split tracer/postprocess graphics scopes
- `egui.render`
- representative `surface.build` GPU job scopes

Perf logs use stable labels such as:

```text
[PERF][GPU_FRAME_SCOPE]
[PERF][GPU_JOB_SCOPE]
```

Profiler-disabled runs should not allocate app profiler state, record timestamp queries, or emit per-frame profiler logs.

## Validation ladder

Use the standard project validation when touching sync/profiler code:

```bash
cargo fmt --check
cargo check
cargo check --features sync_diagnostics
cargo test
cargo run --release -- --hidden --auto-exit 0.5
cargo run --release -- --hidden --auto-exit 4 --perf
cargo run --release -- --tail-latest-log 200
```

Search the latest log for concrete failures:

```bash
rg -i "error|panic|failed|validation" <latest-log>
```

## Guardrails

- No timestamp readback waits in the frame being measured.
- No extra queue submissions, fences, semaphores, or layout transitions just for diagnostics.
- No per-frame string formatting or heap allocation unless profiling/diagnostics are explicitly enabled.
- Scope overflow should drop extra scopes and count drops, not panic in release.
- Keep names semantic and stable: `tracer.render`, `surface.build`, `terrain.source_readback`, etc.
- Expand profiling incrementally; avoid turning this into a full render graph until a concrete need appears.

## Useful next steps

- Add p50/p95/p99 summaries for frame scopes and representative GPU jobs.
- Expand GPU job profiling beyond `surface.build` only after confirming current logs are useful.
- Add transition-count/timeline summaries from the existing texture transition diagnostics sink when needed.
- Continue replacing raw descriptive `vk::` usage with small semantic vkn wrappers only where it reduces coupling without broad churn.
