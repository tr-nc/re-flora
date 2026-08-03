# Parallel Architecture Workstreams

## Purpose

This is the coordination source for two architecture workstreams that are ready to proceed in
parallel:

1. Deepen DDGI Volume runtime ownership, tracked by GitHub issue
   [#53](https://github.com/tr-nc/re-flora/issues/53).
2. Deepen Vulkan synchronization and runtime GPU resource management through the VKN tickets in
   this document.

The two workstreams have separate ownership boundaries and can start in separate worktrees. They
meet only in the later DDGI synchronization follow-up: DDGI ownership moves first without changing
its current device-wide publication synchronization, while the general VKN work establishes the
completion, residency, descriptor-generation, and resource-state mechanisms that can later replace
that stall safely.

## Workstream A — DDGI Volume runtime ownership

**Tracker:** [GitHub issue #53 — Deepen DDGI Volume runtime ownership](https://github.com/tr-nc/re-flora/issues/53)

**What to build:** Make the DDGI Volume runtime the sole owner of its logical lifecycle and atomic
GPU publication transaction. Terrain and Authored Environment Lighting submit high-level facts;
callers no longer coordinate scheduling, active/staging residency, exact voxel visibility,
validation, descriptor rebinding, captures, or promotion ordering themselves.

**Blocked by:** None — can start immediately.

**Status:** in-progress (terrain/radiance event ownership integrated in `5e7f6847`; concrete
GPU resource/pass ownership remains active)

### Required outcomes

- One canonical typed runtime status is the observation seam for the operator UI, logs, captures,
  deterministic scenarios, and host-only lifecycle tests.
- Geometry, density, radiance, feedback, stale-work rejection, validation, active/staging ownership,
  and capture checkpoint residency belong to the DDGI Volume runtime module.
- Terrain remains authoritative for terrain mutation and visible geometry publication.
- Authored Environment Lighting remains authoritative for normalized live lighting state and stable
  revisions; DDGI owns immutable in-flight snapshots and revision coalescing.
- Direct sun remains immediate and outside the DDGI consumer seam.
- Partial or invalid Irradiance Maps and Visibility Maps never become consumer-visible.
- Preparation, execution, validation, and publication errors fail fast. No recovery state, retry,
  degraded mode, or compatibility layer is introduced.
- Capture v5, accepted logs, external runners, shader behavior, and the complete DDGI correctness
  acceptance remain compatible.
- The existing device-wide publication synchronization remains unchanged in this workstream. Its
  measured replacement belongs to VKN Tickets 07 and 08.

The GitHub issue is the detailed specification and acceptance source of truth. This document records
its coordination boundary so both workstreams can be scheduled without reopening those decisions.

## Workstream B — VKN runtime architecture

The tickets below turn the Vulkan synchronization and runtime GPU resource-management review into
context-sized implementation slices. The intended outcome is a deeper `re-flora-vkn` boundary:
command submission owns residency through completion, runtime resource publication owns retirement,
and command recording owns Buffer and Image state transitions.

These tickets are deliberately smaller than a render graph. Re: Flora keeps its existing linear
rendergraph-lite direction and concrete Vulkan backend.

## Parallel start

For two concurrent implementation agents, start exactly these two assignments:

| Workstream | Starting assignment | Suggested branch | Worktree boundary |
| --- | --- | --- | --- |
| A | GitHub issue #53 — DDGI Volume runtime ownership | `agent/ddgi-runtime` | DDGI runtime, tracer ownership migration, DDGI acceptance |
| B | Ticket 01 — self-resident `surface.build` GPU work | `agent/vkn-submission-lifetime` | `re-flora-vkn` managed jobs plus the representative Surface job |

Ticket 03 is also unblocked, but it should remain queued when only two agents are available. If a
third agent starts it later, it must use a third worktree and an independent branch. Integration and
validation happen in the main worktree after each independently validated worker commit.

## Decisions and guardrails

- Invalid GPU lifecycle use fails fast. Runtime recovery, retry, and degraded operation are not part
  of this work.
- A submitted command buffer and every resource lease needed by its commands remain resident until
  completion.
- Submission completion is the clock used to retire command buffers, descriptor generations,
  Buffers, Images, and dependent Vulkan objects.
- Runtime descriptor publication is generational. Creation-time descriptor initialization may
  remain direct where no prior generation can be in flight.
- Image and Buffer state follow the command-recording lifecycle. Recording abandoned work must not
  mutate committed resource state.
- Do not introduce a full render graph, pass scheduler, fake renderer, public GPU trait, or generic
  adapter without a second real backend.
- Do not introduce a generic transient resource pool without release-mode allocation-churn evidence.
- DDGI synchronization changes remain blocked on the DDGI Volume ownership migration and a measured
  release-mode baseline.
- Each independently validated ticket is committed before the next implementation step begins.

## Shared validation policy

Every Rust or shader ticket follows the repository validation ladder appropriate to its scope:

```bash
cargo fmt --check
cargo check
cargo check --features sync_diagnostics
cargo test
cargo run --release -- --hidden --mute --auto-exit 0.5
cargo run --release -- --tail-latest-log 200
```

The latest hidden-run log must be inspected for concrete Vulkan validation errors, panics, failed
resource operations, and synchronization errors. Performance tickets use matched release-mode app
runs; debug builds and unit tests are not performance evidence.

## VKN dependency frontier

The VKN frontier is:

- Ticket 01 — Make `surface.build` GPU work self-resident and fail fast.
- Ticket 03 — Encode the owned Vulkan resource graph.

Tickets 01 and 03 can proceed independently. Ticket 07 can start only after Workstream A is
complete. All other tickets follow the blocking edges declared below.

---

## Ticket 01 — Make `surface.build` GPU work self-resident and fail fast

**What to build:** Make one representative off-frame Surface build own its submitted command buffer
through GPU completion. Abandoning pending work must fail fast at the managed-job boundary instead
of destroying a fence or command buffer that Vulkan may still be using.

**Blocked by:** None — can start immediately.

**Status:** completed (`5ab1e777`)

- [x] A submitted Surface build remains valid through polling, waiting, completion, and result
      collection without a builder-owned command-buffer sidecar.
- [x] Dropping or otherwise abandoning incomplete work produces a deterministic fail-fast outcome
      and never destroys a fence or frees a command buffer still referenced by pending work.
- [x] Successful completion still returns the same Surface build result and GPU profiling evidence.
- [x] Managed-job diagnostics make submission, completion, and invalid abandonment observable without
      adding waits or allocations to normal diagnostics-disabled runs.
- [x] The shared validation ladder passes, including hidden release execution and log inspection.

---

## Ticket 02 — Migrate remaining off-frame GPU jobs

**What to build:** Move Plain, Contree, scene-acceleration, readback, and remaining off-frame work to
self-resident managed submissions, then remove the superseded borrowed-command-buffer submission
path and duplicated job sidecars.

**Blocked by:** Ticket 01 — Make `surface.build` GPU work self-resident and fail fast.

**Status:** completed (`beab1295`)

- [x] Every migrated job supports its existing submit, poll, wait, complete, discard, and result
      behavior through the managed submission lifecycle.
- [x] Builder job records no longer retain command buffers solely to keep pending submissions alive.
- [x] Stale or discarded build work still completes safely before its logical allocations are
      reclaimed.
- [x] The old borrowed command-buffer submission form is removed once no caller needs it.
- [x] Existing builder tests, readback checks, profiling output, and hidden release validation pass.

---

## Ticket 03 — Encode the owned Vulkan resource graph

**What to build:** Deepen the owned Buffer, Image, Image View, Texture, Framebuffer, and acceleration-
structure resource model so dependent Vulkan objects retain the owners they require and destruction
ordering no longer depends on raw handles or incidental field layout.

**Blocked by:** None — can start immediately.

**Status:** completed (`7cd1a7b2`)

- [x] Owned Image Views cannot outlive the Image allocation on which they depend.
- [x] Owned Framebuffers and acceleration structures retain their attachment or backing-resource
      dependencies until the dependent Vulkan object is destroyed.
- [x] Externally owned swapchain Images remain an explicit ownership mode and are never destroyed by
      the owned-resource path.
- [x] Buffer and Image ownership can be leased by later submission-retirement work without exposing
      allocator internals or raw lifetime responsibilities to callers.
- [x] Repeated creation, cloning, replacement, and destruction pass Vulkan validation in a hidden
      release run.
- [x] No generic GPU backend trait or hypothetical adapter is introduced.

---

## Ticket 04 — Retire runtime Buffer generations at frame completion

**What to build:** Replace the device-wide idle used when the dynamic-fruit instance Buffer grows
with completion-scoped retirement. Rendering may publish the new Buffer immediately, while the
previous generation remains resident until every frame that could reference it has completed.

**Blocked by:**

- Ticket 02 — Migrate remaining off-frame GPU jobs.
- Ticket 03 — Encode the owned Vulkan resource graph.

**Status:** completed (`c807393a`)

- [x] Dynamic-fruit capacity growth no longer calls `device.wait_idle()`.
- [x] The previous instance Buffer stays resident until all frame submissions that may reference it
      have completed.
- [x] Repeated growth across adjacent frames cannot reuse or destroy an in-flight allocation.
- [x] Retirement diagnostics identify the resource generation and the completion event that released
      it without exposing raw Vulkan handles as the public identity.
- [x] Fruit rendering, instance counts, and shadow-change behavior remain unchanged in hidden release
      validation.

---

## Ticket 05 — Publish and retire egui texture descriptor generations

**What to build:** Make egui texture registration, replacement, and removal publish complete
descriptor/resource generations. A previous texture generation remains valid for in-flight frames
and retires only after their completion.

**Blocked by:** Ticket 04 — Retire runtime Buffer generations at frame completion.

**Status:** ready-for-agent (active frontier)

- [ ] Registering a texture publishes one descriptor generation containing the resources required to
      render it.
- [ ] Replacing or removing a texture cannot invalidate a descriptor still referenced by an in-flight
      frame.
- [ ] Texture identity and descriptor identity cannot drift across separate parallel maps.
- [ ] Completed generations are reclaimed without a device-wide idle or an unbounded retention list.
- [ ] The normal egui render path, texture updates, and texture removal pass hidden release validation.

---

## Ticket 06 — Resize extent-dependent resources without a device-wide idle

**What to build:** Make window resize publish a complete new generation of extent-dependent Images,
Framebuffers, and descriptors while prior frames safely finish with the previous generation. Remove
the normal resize path's device-wide idle.

**Blocked by:** Ticket 05 — Publish and retire egui texture descriptor generations.

**Status:** ready-for-agent

- [ ] Normal window resize no longer calls `device.wait_idle()` before replacing extent-dependent GPU
      resources.
- [ ] A frame observes one internally consistent extent generation; it cannot mix old Images with new
      Framebuffers or descriptors.
- [ ] Prior extent generations retire only after the frame submissions that reference them complete.
- [ ] Rapid consecutive resizes and swapchain out-of-date/suboptimal events remain safe and converge
      on the latest window extent.
- [ ] Composition, post-processing, shadows, clouds, egui, swapchain presentation, and screenshot
      paths remain valid after resize.
- [ ] Hidden release logs contain no descriptor, framebuffer, Image lifetime, or synchronization
      validation errors.

---

## Ticket 07 — Measure the DDGI publication stall

**What to build:** Establish an authoritative release-mode baseline for the DDGI Volume publication
stall before changing synchronization. Capture the publication cost and relevant frame-tail behavior
under matched, reproducible conditions.

**Blocked by:** Existing GitHub issue [#53 — Deepen DDGI Volume runtime ownership](https://github.com/tr-nc/re-flora/issues/53).

**Status:** ready-for-agent

- [ ] The benchmark exercises a real DDGI Volume publication after the ownership migration, including
      the current device-wide idle behavior.
- [ ] Repeated release-mode samples report publication stall plus representative frame median and tail
      metrics.
- [ ] Scene, camera, resolution, present mode selection, DDGI spacing, build ancestry, and capture
      conditions are recorded and matched across samples.
- [ ] The benchmark command and raw/summary evidence are durable and can be reused for the replacement
      A/B.
- [ ] No synchronization implementation is changed and no performance conclusion is drawn from debug
      builds or unit tests.

---

## Ticket 08 — Retire DDGI descriptor generations without a device-wide idle

**What to build:** Replace the measured DDGI Volume publication stall with atomic descriptor/resource
generation publication. The last complete DDGI Volume remains visible until the replacement is
complete, and the retired generation remains resident until its final consumer frame completes.

**Blocked by:**

- Ticket 05 — Publish and retire egui texture descriptor generations.
- Ticket 07 — Measure the DDGI publication stall.

**Status:** ready-for-agent

- [ ] DDGI consumer publication no longer requires `device.wait_idle()`.
- [ ] Consumers observe either the previous complete DDGI Volume or the newly published complete
      generation, never a mixture of the two.
- [ ] Partial S0, S1, or feedback work never becomes consumer-visible during publication.
- [ ] Obsolete or replaced generations remain resident until all referencing frame submissions
      complete and then retire deterministically.
- [ ] The complete DDGI correctness acceptance remains green, including geometry edits, density
      rebuilds, radiance changes, convergence, captures, and batch-order invariance.
- [ ] Matched release-mode A/B evidence improves the publication stall without a material frame-tail
      regression.

---

## Ticket 09 — Contract unsafe runtime descriptor mutation

**What to build:** Move remaining runtime descriptor publishers to generation publication and remove
the superseded in-place mutation surface and duplicate resource-residency maps. Preserve a narrow
creation-time initialization path where no prior generation can be in flight.

**Blocked by:**

- Ticket 06 — Resize extent-dependent resources without a device-wide idle.
- Ticket 08 — Retire DDGI descriptor generations without a device-wide idle.

**Status:** ready-for-agent

- [ ] Every descriptor update that can race an in-flight frame uses generation publication and
      completion-scoped retirement.
- [ ] Runtime descriptor objects retain the Buffer, Image, Image View, Sampler, or acceleration-
      structure owners they reference.
- [ ] Duplicate pipeline-side texture residency maps are removed where the descriptor generation now
      owns the same information.
- [ ] Creation-time initialization remains explicit and cannot be mistaken for safe runtime mutation.
- [ ] The superseded runtime in-place descriptor mutation interface is removed once no caller needs it.
- [ ] Descriptor initialization, runtime publication, resize, egui, and DDGI validation all pass.

---

## Ticket 10 — Commit Image state with the command-recording lifecycle

**What to build:** Make one cached Contree command path track Image state inside a recording
transaction. State becomes committed only when the recording lifecycle succeeds, and abandoned or
failed recordings leave committed Image state unchanged.

**Blocked by:**

- Ticket 01 — Make `surface.build` GPU work self-resident and fail fast.
- Ticket 03 — Encode the owned Vulkan resource graph.

**Status:** completed (`c7fdbda5`)

- [x] Recording a transition does not immediately overwrite globally committed Image state.
- [x] Successfully submitted work commits its resulting Image state exactly once in queue order.
- [x] Abandoned, failed, or reset recordings do not leave a false committed layout, stage, or access
      state.
- [x] The cached Contree path no longer disables automatic texture transitions to compensate for
      record-time state mutation.
- [x] Automatic, manual, and assert-only policies remain observable and fail with useful semantic
      diagnostics.
- [x] Transition diagnostics and hidden release validation confirm the same final resource states as
      the preserved behavior.

---

## Ticket 11 — Track Buffer hazards through one Contree build

**What to build:** Extend the command-recording resource-state module through one complete Contree
build so Buffer reads, writes, indirect-command use, transfers, and host visibility declare their
intent and receive the required barriers without broad caller-authored synchronization.

**Blocked by:** Ticket 10 — Commit Image state with the command-recording lifecycle.

**Status:** completed (`583d5fac`)

- [x] The selected Contree build declares every relevant Buffer use at the recording seam.
- [x] The recording module emits the required Buffer dependencies for compute, indirect, transfer,
      and readback transitions in command order.
- [x] Broad manual barriers covered by the declared uses are removed from the migrated path.
- [x] Diagnostics expose semantic Buffer-use transitions without making raw stage/access masks the
      caller's test surface.
- [x] Contree allocation, build output, stale-work discard, CPU-cache readback, and profiling behavior
      remain unchanged.
- [x] Tests and hidden release validation detect missing dependencies and remain free of Vulkan
      synchronization errors.

---

## Ticket 12 — Migrate remaining builder Buffer hazards

**What to build:** Move the remaining Plain, Surface, and Contree builder workloads from broad manual
barriers to declared Buffer use through the command-recording resource-state module.

**Blocked by:** Ticket 11 — Track Buffer hazards through one Contree build.

**Status:** in-progress (`07ec1a7b`; Plain sampling and terrain-smoothing readback paths migrated)

- [ ] Builder compute, indirect, transfer, fill, copy, and host-read operations declare their Buffer
      uses through the shared recording seam.
- [ ] Manual barriers are removed only where the resource-state module owns an equivalent or more
      precise dependency.
- [ ] Cached and one-time command paths preserve their existing recording and completion semantics.
- [ ] Terrain construction, edits, smoothing, flora growth, sampling, readbacks, and allocator
      behavior remain unchanged.
- [ ] Builder tests, release hidden validation, and synchronization diagnostics pass without broad
      fallback barriers masking missing declarations.

---

## Ticket 13 — Migrate tracer Buffer hazards

**What to build:** Move frame-rendering Buffer dependencies to declared use across compute,
indirect dispatch, transfer, graphics, and host-read operations while preserving the existing pass
order and rendergraph-lite architecture.

**Blocked by:** Ticket 11 — Track Buffer hazards through one Contree build.

**Status:** ready-for-agent

- [ ] Tracer Buffer producers and consumers declare their use through the shared recording seam.
- [ ] Compute-to-compute, compute-to-indirect, transfer-to-compute, compute-to-graphics, and
      GPU-to-host dependencies remain correct for the resources that require them.
- [ ] The migration does not introduce pass scheduling, reorder commands, or turn the work into a
      full render graph.
- [ ] Broad manual barriers are removed only after diagnostics demonstrate equivalent declared
      dependencies.
- [ ] Rendered output, screenshots/captures, GPU profiling, and release frame behavior remain
      equivalent under matched conditions.
- [ ] Hidden release logs remain free of synchronization, descriptor, and resource-state errors.

---

## Ticket 14 — Contract the shallow resource-state and barrier paths

**What to build:** Remove the superseded pipeline-local Image trackers and broad manual barrier
surfaces after all migrated callers use the command-recording resource-state module. Retain explicit
barriers only as a narrow, intentional diagnostic or exceptional-operation seam.

**Blocked by:**

- Ticket 12 — Migrate remaining builder Buffer hazards.
- Ticket 13 — Migrate tracer Buffer hazards.

**Status:** ready-for-agent

- [ ] One command-recording module owns committed Image state, Buffer hazards, and barrier emission
      for normal rendering and builder work.
- [ ] Pipeline objects no longer maintain duplicate resource-state trackers or mirrored lifetime
      information.
- [ ] Superseded broad barrier helpers and unsafe state-assumption paths are removed once unused.
- [ ] Remaining explicit barrier/state APIs are narrowly documented, inspectable, and exercised by a
      concrete exceptional use rather than retained for hypothetical compatibility.
- [ ] Tests assert semantic command-recording behavior and diagnostics rather than private pipeline
      fields or raw Vulkan masks.
- [ ] The complete validation ladder passes, including DDGI acceptance where affected and hidden
      release log inspection.

## Completion condition

This ticket set is complete when submission completion is the single retirement clock for runtime
GPU resources, descriptor publication cannot invalidate in-flight work, Buffer and Image hazards are
owned by the command-recording lifecycle, the superseded shallow paths are deleted, and the real
release Vulkan path remains correct and measurable.
