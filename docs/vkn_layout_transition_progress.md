# VKN Layout Transition Progress

## Goal

Move image layout/resource-state transition policy toward `re-flora-vkn` so game/render code declares resource use while `vkn` records the needed barriers.

Done means:

- Common operations such as clears, uploads, copies, render-pass attachment use, compute dispatches, sampled reads, and swapchain transfers have a clear `vkn`-owned transition path.
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
- Game/render code still explicitly calls transitions in several places, for example:
  - `src/builder/surface/mod.rs`
  - `src/builder/plain/mod.rs`
  - `src/tracer/mod.rs`
- Descriptor writes are not transitions. `auto_update_descriptor_sets` currently writes texture descriptors with `TextureLayout::GENERAL`, but that only sets descriptor metadata.
- Render pass code currently has a note that callers are responsible for final layout tracking after `record_end`; some callers use `Image::set_layout` after render passes.
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
- Status: not started.

### Phase 2: Centralize transition recording behind a tracker/encoder

- Objective: Add a `vkn`-owned state tracker that records barriers from old state to requested state.
- Expected output: A linear command encoder or transition tracker that uses existing `TextureTransition`/barrier machinery and keeps explicit transition hooks.
- Dependencies/blockers: Must preserve current `Image` layout tracking or migrate it carefully.
- Status: not started.

### Phase 3: Move render-pass attachment state updates into `vkn`

- Objective: Make render-target begin/end own attachment state transitions/tracking instead of requiring game code to call `set_layout`.
- Expected output: Render pass/target paths update tracked image state based on attachment initial/final layouts.
- Dependencies/blockers: `Framebuffer` currently stores raw image views, so attachment texture identity may need to be retained for texture-backed framebuffers.
- Status: not started.

### Phase 4: Add automatic transitions for compute/descriptor texture use

- Objective: Before dispatch, transition bound textures into the state required by their descriptor usage.
- Expected output: Conservative defaults from reflected descriptor type, with optional explicit annotations for sampled vs storage read/write cases.
- Dependencies/blockers: Descriptor metadata alone may not distinguish readonly/writeonly storage images; may need binding annotations or shader reflection support.
- Status: not started.

### Phase 5: Migrate game-code explicit transitions gradually

- Objective: Replace repeated manual transitions with declared usage through the new `vkn` API while keeping debug assertions available.
- Expected output: Smaller call sites in `src/builder/*` and `src/tracer/*`, with behavior unchanged.
- Dependencies/blockers: Phases 2-4 should exist first.
- Status: not started.

### Phase 6: Diagnostics and strict modes

- Objective: Make automatic transitions easy to debug and configurable.
- Expected output: Optional transition logging, validation assertions, manual-only mode, conservative/precise policy switches, and clear panic/error messages for unknown states.
- Dependencies/blockers: Need the tracker API to stabilize first.
- Status: not started.

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

## Open Questions / Risks

- Should the first API be called a command encoder, state tracker, resource barrier manager, or rendergraph-lite?
- How should automatic behavior be configured: global context setting, per-command-buffer setting, or per-operation policy?
- How precise do we need to be for storage images: read-only, write-only, or read/write?
- How do we represent subresource ranges beyond current one-layer tracking?
- Render-pass attachment tracking may require storing texture identity in `Framebuffer`/`RenderTarget`, not only raw image views.
- Overly conservative barriers may be correct but could hurt performance; precise barriers may require more metadata.
- Existing `Image::set_layout` is an escape hatch and can hide bugs; migration should replace it with explicit tracker assumptions/assertions where possible.
