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

**Status:** in-progress (terrain/radiance event ownership integrated in `5e7f6847`, capture
checkpoint configuration/residency and its canonical typed-status observation now belong to
`DdgiRuntime` in `b680f403`/`5bf4b325`, active/staging `DdgiVolumes` ownership moved under the
runtime in `c55c7366`, and the fail-fast physical/logical promotion transaction is centralized in
`49033a6e`; pass recording ownership remains active)

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

**Status:** in-progress (`74ce3a02`, `29e2ce0e`; egui Mesh growth and texture
replacement/removal now publish completion-retired generations, and each live texture is an
explicit texture/descriptor/generation bundle; dedicated lifecycle validation and partial-update
ordering remain)

- [x] Replacing or removing a texture keeps the old texture/descriptor pair resident until frame
      completion instead of dropping either map entry immediately.
- [x] Mesh buffer growth keeps the old vertex/index pair resident until frame completion.
- [x] Completed egui generations use the bounded frame-retirement queue rather than a device-wide
      idle or an unbounded live-resource list.
- [x] Registering a texture publishes one explicit descriptor generation whose owner set is the
      complete renderable resource bundle.
- [ ] Partial texture updates, identity/descriptor pairing, and the full texture lifecycle pass
      dedicated hidden release validation.

---

## Ticket 06 — Resize extent-dependent resources without a device-wide idle

**What to build:** Make window resize publish a complete new generation of extent-dependent Images,
Framebuffers, and descriptors while prior frames safely finish with the previous generation. Remove
the normal resize path's device-wide idle.

**Blocked by:** Ticket 05 — Publish and retire egui texture descriptor generations.

**Status:** in-progress (`768b8c00`, `400e44d6`; resize waits and observes all frame submissions in
queue order, retires the old extent-dependent resource/framebuffer bundle through the completion
clock, and propagates acquire-side suboptimal signals alongside present-side signals)

- [x] Normal window resize no longer calls `device.wait_idle()` before replacing extent-dependent GPU
      resources.
- [x] Prior extent-dependent Images and Framebuffers are retained as one generation until frame
      submissions complete.
- [ ] A frame observes one internally consistent extent generation across descriptors and all passes.
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

**Status:** completed (`8b35ec2c`, `cf134884`, `e3c25906`, `ae6f88ef`; descriptor sets retain the
Buffer/Texture/acceleration-structure owners associated with each written binding, and real staging
publication emits release timing markers for descriptor rebind/resource swap/total cost. The
release-only runner records the exact scene/capture command, raw per-sample logs, and a JSON summary.
Three matched Apple M4 Pro samples are recorded below; the migration A/B remains an explicit Ticket
08 acceptance item.)

- [x] The benchmark exercises a real DDGI Volume publication after the ownership migration using
      `terrain-edits-closed`, which reaches an active/staging `Terrain` promotion and emits
      `[DDGI][PUBLICATION_TIMING]`.
- [x] Three repeated release-mode samples report publication stall plus representative frame median
      and tail metrics: current generation publication total median `0.383 ms` (mean `0.385 ms`,
      p95 `0.383 ms`), frame render median `3865 us` (p95 `6438 us`).
- [x] Scene, camera, resolution, present mode selection, DDGI spacing, build ancestry, and capture
      conditions are recorded and matched across samples: `terrain-edits-closed`, spacing `32`,
      hidden `1280x720` logical / `2560x1440` physical, auto-selected FIFO, Apple M4 Pro, release
      `--perf`, published capture target.
- [x] The benchmark command and raw/summary evidence format are durable and can be reused for the
      replacement A/B (`scripts/benchmark_ddgi_publication.py`).
- [x] No synchronization implementation is changed and no performance conclusion is drawn from debug
      builds or unit tests.

**Evidence:** `target/ddgi-publication-benchmark-20260803/summary.json` contains the three raw logs
and summary. The matched pre-migration `8b35ec2c` worktree, temporarily instrumented only for this
measurement, reported publication total median `0.172 ms` (device-idle median `0.077 ms`) and frame
render median `3720 us` (p95 `6409 us`). The current path removes the device-wide idle, but this
low-load A/B does not show a faster total publication; Ticket 08 therefore keeps its A/B performance
checkbox open.

---

## Ticket 08 — Retire DDGI descriptor generations without a device-wide idle

**What to build:** Replace the measured DDGI Volume publication stall with atomic descriptor/resource
generation publication. The last complete DDGI Volume remains visible until the replacement is
complete, and the retired generation remains resident until its final consumer frame completes.

**Blocked by:**

- Ticket 05 — Publish and retire egui texture descriptor generations.
- Ticket 07 — Measure the DDGI publication stall.

**Status:** in-progress (`41d2709e`, `49033a6e`, `cf134884`, `902d7436`; consumer descriptor generations are
published atomically and retired on frame completion, and staging publication no longer calls
`device.wait_idle()`; matched correctness is green, while the measured total-stall improvement is
not yet demonstrated)

- [x] DDGI consumer publication no longer requires `device.wait_idle()`.
- [x] Consumers observe either the previous complete DDGI Volume or the newly published complete
      generation, never a mixture of the two.
- [x] Partial S0, S1, or feedback work never becomes consumer-visible during publication.
- [x] Obsolete or replaced generations remain resident until all referencing frame submissions
      complete and then retire deterministically.
- [x] The DDGI lifecycle acceptance remains green for geometry edits, density preemption/retry,
      radiance coalescing, convergence, and published captures (`scripts/check_ddgi_lifecycle_acceptance.sh`).
- [x] The complete DDGI correctness acceptance remains green across every batch-order invariance
      case. The release transport matrix covered both `forward` and `reverse` donor batches at
      spacings 32 and 16, all sealed/donor/dogleg/portal convergence stages, exact-reference
      correctness, and the terrain-edit runtime matrix. The initial composite run reported one
      failure only because `check_ddgi_runtime_terrain_edits.sh` expected
      `staging_stage=Some(Rebuilding)` while the runtime log contract is `staging_stage=Rebuilding`;
      after `902d7436` corrected that test-only literal, the 29-run runtime matrix exited 0.
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

**Status:** in-progress (`0222d5aa`, `1c7d48bc`, `76397eeb`, `a8e2a009`; Plain's terrain moisture/dry setup is
explicitly creation-time-only, Surface's per-chunk off-frame bindings publish generations retained
by managed-job completion, and the VKN descriptor API now rejects active-generation writes; final
validation and any remaining duplicate residency cleanup remain)

- [x] Every descriptor update currently used by runtime code that can race an in-flight frame uses
      generation publication and completion-scoped retirement; direct writes now require the
      explicit creation-time initialization API.
- [x] Descriptor objects retain the Buffer, Texture (including Image View/Sampler), or acceleration-
      structure owner associated with each written binding.
- [x] Duplicate pipeline-side texture residency maps are removed where the descriptor generation now
      owns the same information; ImageUse declarations now read the active DescriptorSet owners.
- [x] Creation-time initialization remains explicit and cannot be mistaken for safe runtime mutation.
- [x] The superseded runtime in-place descriptor mutation interface is removed once no caller needs it;
      `write_descriptor_set` only accepts a staged generation and initialization uses
      `initialize_descriptor_set`.
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

**Status:** completed (`a477f3d1`; Plain, Surface, and Contree builder Buffer paths now use the
recording seam, including same-state Image write ordering where the former Plain image barriers
were redundant)

- [x] Builder compute, indirect, transfer, fill, copy, and host-read operations declare their Buffer
      uses through the shared recording seam.
- [x] Manual barriers are removed only where the resource-state module owns an equivalent or more
      precise dependency.
- [x] Cached and one-time command paths preserve their existing recording and completion semantics.
- [x] Terrain construction, edits, smoothing, flora growth, sampling, readbacks, and allocator
      behavior remain unchanged.
- [x] Builder tests, release hidden validation, and synchronization diagnostics pass without broad
      fallback barriers masking missing declarations.

---

## Ticket 13 — Migrate tracer Buffer hazards

**What to build:** Move frame-rendering Buffer dependencies to declared use across compute,
indirect dispatch, transfer, graphics, and host-read operations while preserving the existing pass
order and rendergraph-lite architecture.

**Blocked by:** Ticket 11 — Track Buffer hazards through one Contree build.

**Status:** in-progress (`5fb17dbf`, `f04ef4ca`, `8d09317f`, `48d27b1b`, `b9108858`, `b4ef4d29`,
`4f1d8592`, `14d3b0ef`, `419fff8d`, `1d0b34d5`, `dd0fc745`, `91a7bed8`, `69343901`, `5cdf796d`,
`96acf10d`, `3f90f1b2`, `ebfe4879`, `73fa3aad`, `1248b5b0`, `098cb031`, `0de8c4de`, `29e2ce0e`, `1ec6204f`; DDGI voxel-visibility and
terrain-query one-time paths, CPU-updated tracer/DDGI uniform buffers, CPU-filled tracer graphics
instance buffers, tracer static mesh inputs, flora/wind shader lookup buffers, irradiance-capture
storage writes, DDGI metadata/transient-ray transitions, and Egui mesh buffers now declare Buffer
uses; frame-wide Image tracking remains incomplete)

- [ ] Tracer Buffer producers and consumers declare their use through the shared recording seam.
- [x] DDGI voxel-visibility and terrain-query one-time paths declare HostWrite/ComputeRead,
      ComputeWrite, and HostRead uses.
- [x] CPU-updated tracer uniform and wind buffers declare HostWrite followed by ShaderRead before
      the first frame pass; the declaration covers compute, vertex, and fragment shader consumers.
- [x] CPU-updated DDGI radiance and transport-query uniforms declare HostWrite followed by
      ShaderRead before the frame's first DDGI shader pass.
- [x] CPU-filled tracer graphics instance buffers declare HostWrite followed by VertexRead before
      shadow/main render passes; no barrier is inserted inside a render pass.
- [x] Tracer static graphics mesh inputs declare IndexRead/VertexRead before shadow/main render
      passes; no input declaration is inserted inside a render pass.
- [x] Static flora lookup and wind-volume shader buffers declare ShaderRead before graphics
      passes that consume them.
- [x] Irradiance-capture storage writes declare ComputeWrite before the tracer pass and transition
      to TransferRead through the existing readback helper.
- [x] Irradiance-capture, DDGI trace-stat, and atlas-reduction readbacks declare TransferRead/
      TransferWrite and HostRead through the shared Buffer recording seam.
- [x] DDGI probe metadata and transient ray data declare ComputeWrite/ComputeRead at relocation,
      trace, and filter boundaries, then expose active metadata as ShaderRead before graphics.
- [x] Replaced or removed per-tree leaf/apple instance buffers are retained until frame submission
      completion instead of forcing a device-wide idle wait.
- [x] Wind-volume compute-to-graphics fallback barriers are removed; the tracked compute-image
      transition and graphics ImageUse declarations now own that dependency.
- [x] DDGI exact voxel-visibility metadata declares ShaderRead before tracer and flora consumers,
      covering the one-time pack's HostWrite/ComputeRead publication path.
- [x] Contree node and leaf buffers are leased as ShaderRead before tracer compute, shadow, flora,
      and terrain-query descriptor consumers.
- [x] The terminal DDGI voxel-visibility pack no longer records an unscoped barrier after its last
      command; queue completion and the next declared ShaderRead own the publication boundary.
- [x] DDGI global-sky, relocation, trace, filtering, gutter, and reduction passes rely on declared
      image/buffer transitions rather than intervening unscoped compute-to-compute barriers.
- [ ] Compute-to-compute, compute-to-indirect, transfer-to-compute, compute-to-graphics, and
      GPU-to-host dependencies remain correct for the resources that require them.
- [ ] The migration does not introduce pass scheduling, reorder commands, or turn the work into a
      full render graph.
- [ ] Broad manual barriers are removed only after diagnostics demonstrate equivalent declared
      dependencies.
- [ ] Rendered output, screenshots/captures, GPU profiling, and release frame behavior remain
      equivalent under matched conditions.
- [x] Hidden release logs remain free of synchronization, descriptor, and resource-state errors;
      the post-barrier-removal release run and the `sync_diagnostics` release run both exit cleanly
      with the strict actual-error scan clean (`target/re-flora-logs/re-flora-20260803-224405.505-25540.log`,
      `target/re-flora-logs/re-flora-20260803-224451.054-26210.log`).

**Deferred boundary:** Egui's dynamic Mesh buffers now declare HostWrite/IndexRead/VertexRead before
the GUI render pass and retain replaced generations through completion (`74ce3a02`, `91a7bed8`,
`29e2ce0e`).
Frame-wide Image tracking now enters one recording transaction for normal, loading, and off-frame
paths; rapid-resize and mixed-generation overlap evidence remains open under Tickets 06 and 14.

---

## Ticket 14 — Contract the shallow resource-state and barrier paths

**What to build:** Remove the superseded pipeline-local Image trackers and broad manual barrier
surfaces after all migrated callers use the command-recording resource-state module. Retain explicit
barriers only as a narrow, intentional diagnostic or exceptional-operation seam.

**Blocked by:**

- Ticket 12 — Migrate remaining builder Buffer hazards.
- Ticket 13 — Migrate tracer Buffer hazards.

**Status:** completed (`7ef223bb`, `a477f3d1`, `ebfe4879`, `0de8c4de`, `5c5ce3a2`, `704cf9eb`,
`83fe98de`, `01a7a2d7`, `42420042`, `49fdd00e`, `6902c0b1`, `a8e2a009`, `f9b6d4f5`, `3809c27b`, `1ec6204f`;
pipeline-local Image trackers removed, ImageUse declarations now route through CommandBuffer, and
tracer image-copy history paths no longer add redundant compute/transfer fallback barriers; remaining
work in the broader synchronization workstream is focused on resize/overlap evidence and the
exceptional swapchain barrier boundary)

- [x] One command-recording module owns committed Image state, Buffer hazards, and barrier emission
      for normal rendering and builder work; frame/loading recordings now use the same Image+Buffer
      transaction, and render-pass attachment bookkeeping no longer mutates Image state directly;
      the superseded Buffer-only transaction entry point is deleted.
- [x] Pipeline objects no longer maintain duplicate resource-state trackers or mirrored lifetime
      information; descriptor owners now drive image-use declarations directly.
- [x] Superseded broad barrier helpers and unsafe state-assumption paths are removed once unused;
      the unused public `ResourceStateTracker` policy/assumption API is deleted.
- [x] Tracer wind-volume and DDGI image-only pass boundaries now use declared ImageUse transitions
      (including same-state write ordering) instead of broad compute-to-compute fallbacks.
- [x] Image history copies use their source/destination/final-layout transitions as the dependency
      seam; the surrounding broad compute/transfer fallback barriers are removed.
- [x] Tracer output-to-graphics and render-target-to-composition dependencies use declared Image
      transitions and recording-transaction attachment state; their global fallback barriers are
      removed.
- [x] The remaining tracer compute-only pass boundaries use declared Image/Buffer uses; redundant
      frame-wide compute-to-compute barriers are removed from the migrated path.
- [x] Shadow render/compute boundaries rely on RenderTarget and pipeline Image transitions; the
      one retained compute-to-graphics barrier is documented as the MoltenVK VSM diagnostic seam.
- [x] Remaining explicit barrier/state APIs are narrowly documented, inspectable, and exercised by
      concrete swapchain and one-time copy operations rather than retained for hypothetical
      compatibility.
- [x] Tests assert semantic command-recording behavior and descriptor-owner ImageUse mapping rather
      than private pipeline fields or raw Vulkan masks.
- [x] The complete validation ladder passes, including DDGI acceptance where affected and hidden
      release log inspection. The final checks covered `cargo fmt --check`, `cargo check`,
      `cargo check --features sync_diagnostics`, `cargo test`, DDGI lifecycle acceptance, and both
      normal and `sync_diagnostics` hidden release runs with clean strict actual-error scans.

## Completion condition

This ticket set is complete when submission completion is the single retirement clock for runtime
GPU resources, descriptor publication cannot invalidate in-flight work, Buffer and Image hazards are
owned by the command-recording lifecycle, the superseded shallow paths are deleted, and the real
release Vulkan path remains correct and measurable.
