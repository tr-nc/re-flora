# VKN Layout Transition Progress

## Goal

Move image layout/resource-state transition policy toward `re-flora-vkn` so game/render code declares resource use while `vkn` records the needed barriers.

Done means:

- Common operations such as clears, uploads, copies, render-pass attachment use, compute dispatches, graphics sampled reads, and swapchain transfers have a clear `vkn`-owned transition path.
- Explicit transitions remain available for debugging, assertions, and special cases.
- Automatic behavior is configurable and inspectable, not hidden magic.
- Existing rendering behavior and validation checks remain clean.

## Current State

Known from inspection:

- Branch: `agent/vkn-layout-transitions`.
- `crates/re-flora-vkn/src/memory/texture/image.rs` tracks current image layout per array layer and records `vkCmdPipelineBarrier` through `record_transition_barrier`.
- `crates/re-flora-vkn/src/sync/barrier.rs` maps `TextureLayout` to source/destination stage/access masks via `TextureTransition`.
- Some `vkn` helpers already transition internally:
  - `Image::fill_with_raw_u8`
  - `Image::record_clear`
  - `Image::record_copy_to`
  - swapchain blit/readback paths in `crates/re-flora-vkn/src/swapchain.rs`
- Game/render `record_transition(...)` call sites have been migrated out; explicit image transitions now mostly live inside `vkn` helpers such as clears, uploads, copies, swapchain paths, render-target tracking, and pipeline texture-use tracking.
- Descriptor writes are not transitions. `auto_update_descriptor_sets` now writes sampled descriptors with `SHADER_READ_ONLY` and storage-image descriptors with `GENERAL`; actual barriers still come from resource-state tracking. Pipeline-level manual texture writes are tracked as declared texture use, while raw `DescriptorSet::perform_writes` remains an escape hatch.
- Texture-backed render targets update tracked attachment initial/final layouts at render-pass begin/end. Raw swapchain framebuffer paths still use explicit swapchain handling.
- Full correctness requires tracking more than image layout: layout, pipeline stage, access mask, and subresource range.

Constraints:

- Keep changes small and validated.
- Do not hand-edit generated files.
- Run `cargo check` after Rust changes.
- Validate rendering changes with a release hidden run and inspect the latest log.

Assumptions to confirm:

- We want a rendergraph-lite/resource-state tracker first, not a full pass scheduler/rendergraph.
- Linear command order is acceptable for the first version.
- Conservative barriers are acceptable initially if they are correct and easy to inspect.

## Plan / Phases

### Phase 1: Design the resource-state API

- Objective: Define the minimum `vkn` API for declared resource usage, explicit transitions, debug assertions, and configuration.
- Expected output: Short design note or code skeleton for resource states such as sampled read, storage read/write, transfer src/dst, color attachment, depth attachment, and present.
- Dependencies/blockers: Need agreement on naming and how much existing API remains public.
- Status: complete for current scope; added `ResourceState` constructors and a `ResourceStateTracker` policy API with automatic/manual/assert modes.

### Phase 2: Centralize transition recording behind a tracker/encoder

- Objective: Add a `vkn`-owned state tracker that records barriers from old state to requested state.
- Expected output: A linear command encoder or transition tracker that uses existing `TextureTransition`/barrier machinery and keeps explicit transition hooks.
- Dependencies/blockers: Must preserve current `Image` layout tracking or migrate it carefully.
- Status: complete for current scope; `Image` now tracks full `ResourceState` per layer while preserving layout-oriented APIs.

### Phase 3: Move render-pass attachment state updates into `vkn`

- Objective: Make render-target begin/end own attachment state transitions/tracking instead of requiring game code to call `set_layout`.
- Expected output: Render pass/target paths update tracked image state based on attachment initial/final layouts.
- Dependencies/blockers: Raw swapchain framebuffers still have no texture identity, so tracking applies first to texture-backed framebuffers.
- Status: complete for current scope; texture-backed `Framebuffer`s retain attachment `Texture`s and `RenderTarget` updates tracked initial/final attachment layouts at begin/end.

### Phase 4: Add automatic transitions for compute/descriptor texture use

- Objective: Before dispatch, transition bound textures into the state required by their descriptor usage.
- Expected output: Conservative defaults from reflected descriptor type, with optional explicit annotations for sampled vs storage read/write cases.
- Dependencies/blockers: Descriptor metadata alone may not distinguish readonly/writeonly storage images; may need binding annotations or shader reflection support.
- Status: complete for current scope; compute pipelines now retain texture bindings from auto resource binding and transition texture descriptors before direct and indirect dispatch by default. Sampled descriptors use `SHADER_READ_ONLY`; storage-image descriptors remain `GENERAL`. The default-on path can still be disabled per pipeline for cached command buffers whose referenced images are intentionally initialized later.

### Phase 5: Add graphics sampled texture transition hooks

- Objective: Before render-pass begin, let graphics pipelines transition sampled textures declared through descriptors.
- Expected output: Conservative graphics pipeline texture transition helper that can be called outside render passes.
- Dependencies/blockers: Graphics barriers must be recorded before render-pass begin; draw-time recording is too late for arbitrary image barriers.
- Status: complete for current scope; graphics pipelines retain auto-bound and pipeline-written texture descriptors and expose configurable `record_texture_transitions` for call sites to run before render-pass begin. Tracer graphics passes use it for flora/leaves/particles and leaf-shadow rendering. Graphics auto texture transitions are enabled by default, but the explicit pre-render-pass hook remains necessary because Vulkan image barriers cannot be recorded inside arbitrary render-pass draw helpers.

### Phase 6: Migrate game-code explicit transitions gradually

- Objective: Replace repeated manual transitions with declared usage through the new `vkn` API while keeping debug assertions available.
- Expected output: Smaller call sites in `src/builder/*` and `src/tracer/*`, with behavior unchanged.
- Dependencies/blockers: Phases 2-4 should exist first.
- Status: complete for current scope; game-code `record_transition(...)` calls in `src/builder/*` and `src/tracer/mod.rs` have been migrated to vkn-owned compute texture transitions, graphics texture transitions, or render-target attachment tracking. Pipeline-level manual texture descriptor writes now update tracked texture use; raw descriptor-set writes remain manual/escape-hatch territory.

### Phase 7: Diagnostics and strict modes

- Objective: Make automatic transitions easy to debug and configurable.
- Expected output: Optional transition logging, validation assertions, manual-only mode, conservative/precise policy switches, and clear panic/error messages for unknown states.
- Dependencies/blockers: Need the tracker API to stabilize first.
- Status: complete for current scope; compute and graphics pipelines expose policy setters/getters for automatic/manual/assert tracking, auto-transition enabled/getter hooks, and tracked-binding counts. The `sync_diagnostics` feature now exposes optional texture-transition trace logging and the existing transition diagnostics sink for deeper inspection.

## Verification Method

Baseline checks after implementation phases:

```bash
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --auto-exit 0.5
cargo run --release -- --tail-latest-log 200
```

Acceptance criteria:

- Hidden release run completes without Vulkan validation errors, panics, or rendering errors in the latest log.
- Existing explicit-transition call sites can be migrated incrementally; no large all-at-once rewrite is required.
- Automatic transitions produce equivalent final layouts/states for migrated paths.
- Debug/strict mode can detect missing or unexpected state assumptions.
- Descriptor image layout values match the actual tracked image state at use sites.

Additional useful checks once diagnostics exist:

- Compare transition logs before/after migration for representative frames.
- Count removed game-code transition calls and verify no lost barriers in migrated paths.
- Exercise texture upload, clear, storage image compute, sampled texture rendering, render-pass attachment, shadow/depth, and swapchain blit/readback paths.

If verification is not yet possible, the missing piece is the tracker/encoder implementation and diagnostics output.

## Progress Log

- 2026-06-01: Confirmed current branch is `agent/vkn-layout-transitions` with a clean working tree.
- 2026-06-01: Inspected current transition ownership. Decision: pursue a `vkn` resource-state tracker/rendergraph-lite direction rather than a full rendergraph initially.
- 2026-06-01: Created this progress tracker under `docs/` because existing project planning and architecture notes live there.
- 2026-06-01: Added `ResourceStateTracker` with automatic/manual/assert policies, moved raw image-barrier recording into the tracker module, and migrated `Image` tracking from layout-only to full `ResourceState` per layer while keeping existing layout APIs.
- 2026-06-01: Made texture-backed `Framebuffer`s retain attachment textures and updated `RenderTarget` to track render-pass attachment initial/final layouts in `vkn`; raw swapchain framebuffers remain unchanged.
- 2026-06-01: Validated the first implementation slice with `cargo fmt`, `cargo check`, `cargo test`, and a release hidden run; latest-log inspection found only the pre-existing multiple-butterfly-atlas warning.
- 2026-06-01: Added compute-pipeline automatic image transition plumbing for auto-bound texture descriptors. The first attempt used shader-read-only for sampled descriptors, but validation exposed that current descriptor writes still advertise `GENERAL`; adjusted compute texture states to match `GENERAL` and left automatic compute texture transitions opt-in/disabled by default until descriptor layout metadata and startup initialization paths are made precise. Manual descriptor writes remain a known gap.
- 2026-06-01: Revalidated after making compute automatic texture transitions opt-in: `cargo fmt`, `cargo check`, `cargo test`, and release hidden run all passed; latest-log scan found only the known butterfly-atlas warning.
- 2026-06-01: Enabled opt-in automatic compute texture transitions for the wind-volume pipeline and removed its matching manual pre-dispatch `GENERAL` transition as the first game-code migration candidate.
- 2026-06-01: Removed tracer render-pass post-end `set_layout(GENERAL)` calls for graphics output/depth and shadow depth textures; texture-backed `RenderTarget` final-layout tracking now owns these assumptions.
- 2026-06-01: Enabled opt-in automatic compute texture transitions for surface flora occupancy edit passes and removed the shared manual `occupancy_data -> GENERAL` pre-dispatch transition.
- 2026-06-01: Tried flipping compute automatic texture transitions to default-on, but release hidden validation exposed startup descriptor/layout mismatches on resources still in `UNDEFINED`; reverted to opt-in default while retaining per-pipeline enable/disable and a simple tracked-binding-count inspection helper.
- 2026-06-01: Opted plain-builder texture-using compute pipelines into automatic texture transitions and removed the manual `chunk_atlas -> GENERAL` transition from the recorded chunk-init command buffer.
- 2026-06-01: Opted remaining tracer texture-using compute pipelines into automatic texture transitions and removed manual `record_transition(GENERAL)` setup from tracer, shadow/VSM, denoiser, composition, lens-flare, and post-processing passes.
- 2026-06-01: Added graphics-pipeline texture transition tracking for auto-bound descriptors and call it before tracer graphics render passes begin, covering flora/leaves/particles and leaf-shadow sampled texture use.
- 2026-06-01: Made auto descriptor writes and auto texture transition states agree on layout precision: sampled image descriptors now use `SHADER_READ_ONLY`, while storage images stay in `GENERAL`.
- 2026-06-01: Updated compute and graphics automatic texture transitions to cover all array layers for auto-bound textures instead of only layer 0.
- 2026-06-01: Flipped compute automatic texture transitions to default-on, removed redundant per-pipeline opt-in calls, and kept an explicit opt-out for the cached contree leaf-write command buffer that reads a surface image initialized by later per-chunk surface builds.
- 2026-06-01: Moved `SceneAccelBuilder::clear_tex` before recording its cached update command buffer, matching the plain-builder pattern where images are physically initialized before automatic-transition recording mutates their tracked state.
- 2026-06-01: Revalidated default-on compute transitions with `cargo fmt --check`, `cargo check`, `cargo test`, and a release hidden run; latest-log scan shows only the known hidden-monitor and butterfly-atlas warnings.
- 2026-06-01: Enabled graphics texture transitions by default, removed redundant graphics per-pipeline opt-ins, and made pipeline-level manual texture descriptor writes update tracked texture-use state.
- 2026-06-01: Added pipeline resource-state policy getters/setters and feature-gated texture-transition trace logging for `sync_diagnostics`.
- 2026-06-01: Marked the rendergraph-lite transition migration phases complete for the current scope after full validation passed.

## Open Questions / Risks

- Should the first API evolve from `ResourceStateTracker` into a command encoder, or stay as a barrier/resource-state utility?
- How should automatic behavior be configured: global context setting, per-command-buffer setting, or per-operation policy?
- How precise do we need to be for storage images: read-only, write-only, or read/write? Current automatic transitions conservatively use read/write for storage images because Vulkan storage image descriptors require `GENERAL` layout.
- Array-layer ranges are now handled for auto-bound textures; mip ranges and image-view subresource subsets are still not represented.
- Render-pass attachment tracking now works only for texture-backed framebuffers; raw image-view framebuffers such as swapchain targets still need explicit swapchain handling.
- Overly conservative barriers may be correct but could hurt performance; precise barriers may require more metadata.
- Existing `Image::set_layout` is an escape hatch and can hide bugs; migration should replace it with explicit tracker assumptions/assertions where possible.
- Pipeline-level manual texture descriptor writes are tracked for automatic compute/graphics transitions; raw `DescriptorSet::perform_writes` calls still bypass pipeline tracking and should remain an explicit escape hatch.
