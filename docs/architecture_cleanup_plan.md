# Architecture Cleanup Plan

## Context

This branch is for low-risk architecture cleanup. The goal is to reduce god files and implicit module coupling without changing runtime behavior, rendering algorithms, shader layouts, or performance-sensitive code paths unless a later focused change requires it.

Keep every step small and validate after each meaningful move.

Default validation ladder:

```bash
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --auto-exit 0.5
```

For generated files, do not hand-edit generated output. If a build/check regenerates files, include only generated diffs that follow from source/config changes.

## Cleanup order

User-requested order:

1. Extract CLI/options from `main.rs`.
2. Extract inventory/tool/backpack state from `App`.
3. Narrow wildcard public exports gradually.
4. Continue `re-flora-vkn` boundary cleanup incrementally.
5. Split app water orchestration into `app/water/*`.
6. Move terrain visual rebuild state machine out of `app/core/mod.rs`.

## Progress

Completed in this branch:

1. `extract cli options`
   - moved CLI option types, parser, help text, and render flags into `src/cli.rs`.
   - kept crate-root re-exports for compatibility.
2. `extract player tool state`
   - introduced `src/app/core/player_tools.rs` and moved item selection, active voxel, tool timers, backpack counters, edit-loop sound state, and backpack panel target state under `PlayerToolState`.
3. `narrow simple public exports`
   - replaced selected wildcard module re-exports in simple leaf modules with explicit type/constant exports.
4. `add semantic buffer usage helpers`
   - added semantic `BufferUsage` constructors/helpers in `re-flora-vkn` and used them in egui mesh buffers.
5. `split water simulation runtime`
   - converted `src/app/core/water.rs` into `src/app/core/water/mod.rs` plus `src/app/core/water/runtime.rs` for `AsyncWaterSim` and its worker thread.
6. `move terrain rebuild pipeline`
   - moved deferred/synchronous visible terrain rebuild state and methods from `src/app/core/mod.rs` into `src/app/core/terrain_rebuild.rs`.


## 1. Extract CLI/options from `main.rs`

### Goal

Make `main.rs` mostly application bootstrap and move argument parsing/help/log helper types into focused modules.

### Likely files

- `src/main.rs`
- new `src/cli.rs` or `src/app/options.rs`
- possibly new `src/logging.rs` if run-log helpers are large enough

### Target shape

- `main.rs` keeps module declarations, logger setup call, `EventLoop` creation, and `AppController` launch.
- `AppOptions`, `PresentModePreference`, and `WaterProfilePreference` move out of `main.rs`.
- CLI parsing and help text move with `AppOptions`.
- Preserve current CLI behavior exactly, including failure messages and supported hidden-run helpers.

### Risks

- Public paths currently referenced as `crate::AppOptions`, `crate::WaterProfilePreference`, etc.
- Keep compatibility by re-exporting these types from crate root during the first move if needed.

### Validation

Run at least:

```bash
cargo fmt --check
cargo check
cargo test
cargo run --release -- --help
```

Prefer also a hidden run after the move.

## 2. Extract inventory/tool/backpack state from `App`

### Goal

Reduce the size of `App` and isolate player tool state from rendering, world, and water state.

### Likely files

- `src/app/core/mod.rs`
- `src/app/core/input.rs`
- new `src/app/player_tools.rs` or `src/app/core/player_tools.rs`

### Candidate state

- selected item panel slot
- active voxel type
- mouse/tool held flags
- tool cooldown timestamps
- backpack dirt/sand/wood/rock counts
- terrain edit loop sound id/mute state if it remains tool-specific
- backpack summary panel screen position

### Target shape

Introduce a small owned struct such as:

```rust
struct PlayerToolState { /* fields moved from App */ }
```

Start with data movement only. Keep behavior and method call order unchanged. If method extraction gets too large, first move only fields plus trivial accessors.

### Risks

- `input.rs`, UI drawing, and terrain edit actions all touch this state.
- Avoid changing tool semantics while moving fields.

### Validation

Run:

```bash
cargo fmt --check
cargo check
cargo test
```

For behavior, use a hidden run if possible.

## 3. Narrow wildcard public exports gradually

### Goal

Make module boundaries more explicit and reduce accidental coupling caused by `pub use *`.

### Likely files

- `src/builder/mod.rs`
- `src/tracer/mod.rs`
- `src/util/mod.rs`
- `crates/re-flora-vkn/src/lib.rs`

### Policy

Do not do a broad mechanical rewrite across the entire codebase. Instead:

- Narrow exports only in modules touched by previous steps.
- Prefer explicit `pub use module::{TypeA, TypeB};` over `pub use module::*;`.
- Keep crate-root compatibility re-exports temporarily if that avoids churn.
- Remove compatibility re-exports only after call sites are already explicit.

### Risks

- Large compile-error bursts if public exports are narrowed too aggressively.
- Re-export churn can obscure real refactor diffs.

### Validation

Run `cargo check` after every export narrowing batch.

## 4. Continue `re-flora-vkn` boundary cleanup incrementally

### Goal

Continue the direction described by `docs/vkn_crate_refactor_plan.md` and `docs/vkn_crate_refactor_summary.md`: game/rendering code should describe rendering intent, while `re-flora-vkn` translates that intent into Vulkan details.

### Policy

Do not rewrite all remaining `re_flora_vkn::vk` usage at once. When touching nearby rendering/resource setup code, add small semantic wrappers in `re-flora-vkn`.

Good candidates:

- image and buffer usage roles
- color/depth format wrappers
- render target descriptors
- pipeline descriptors
- barrier/transition descriptors

### Non-goals

- No rendering algorithm redesign.
- No shader layout changes.
- No performance claims without release-mode measurements.

### Validation

Run:

```bash
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --auto-exit 0.5
```

Inspect the latest log when render/bootstrap paths are touched.

## 5. Split app water orchestration into `app/water/*`

### Goal

Separate the app-level water integration code from the solver crate and reduce `src/app/core/water.rs` size.

### Current layers

- `crates/re-flora-water`: solver/domain crate.
- `src/app/core/water.rs`: app integration, async sim thread, GUI config sync, terrain SDF source/collider/cache worker queues, soak validation.

### Target shape

Possible module split:

- `src/app/water/mod.rs`
- `src/app/water/runtime.rs` — `AsyncWaterSim` and water sim thread command/snapshot handling
- `src/app/water/gui.rs` — GUI config sync helpers
- `src/app/water/terrain_pipeline.rs` — SDF source refresh, collider rebuild, water cache rebuild workers/queues
- `src/app/water/soak.rs` — water edit soak validation helpers

The exact split should follow dependencies found during implementation. Prefer movement-only chunks.

### Risks

- Water code crosses many systems: app state, terrain chunks, CPU solid voxel store, worker channels, GUI config, and release-mode performance validation.
- Keep `crates/re-flora-water` unchanged unless the app split reveals a clear API boundary issue.

### Validation

Run full validation, including hidden release run and latest log inspection.

## 6. Move terrain visual rebuild state machine out of `app/core/mod.rs`

### Goal

Align code structure with `docs/terrain_visual_rebuild_pipeline.md`: visible terrain rebuilds are primarily synchronous; non-visual terrain SDF/collider/cache work remains queued. The older deferred visual rebuild state machine should not dominate `app/core/mod.rs`.

### Likely files

- `src/app/core/mod.rs`
- `src/app/core/vegetation.rs`
- `src/app/world_ops.rs`
- new `src/app/terrain_rebuild.rs` or `src/app/core/terrain_rebuild.rs`

### Target shape

- Move `ChunkRebuildRequest`, `TerrainChunkRebuildInFlight`, `TerrainChunkRebuildStage`, and related methods out of `core/mod.rs`.
- Keep existing behavior first.
- After movement, clarify naming/comments so synchronous visible rebuild remains the preferred path.
- Do not delete deferred visual rebuild code until call sites and behavior are fully understood.

### Risks

- Vegetation/tree replacement paths still use deferred rebuild calls.
- Terrain rebuild interacts with surface, contree, scene texture updates, flora preservation, and water/collider follow-up scheduling.

### Validation

Run full validation and inspect logs for terrain rebuild errors.

## General rules for this branch

- Prefer movement-only commits before behavior changes.
- Avoid unrelated cleanup.
- Keep generated files untouched unless regenerated by `cargo check` from source/config changes.
- For performance-sensitive changes, release hidden app runs and logs are authoritative.
- If a step becomes larger than expected, stop after the first compiling extraction and document follow-up work here.
