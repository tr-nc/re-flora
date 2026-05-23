# vkn crate refactor plan

## Goals

- Move the Vulkan wrapper layer out of the game binary into `crates/re-flora-vkn`.
- Keep Vulkan implementation details (`ash`, `ash-window`, raw handles, allocator setup, swapchain present/acquire plumbing) inside the vkn crate where practical.
- Leave game/rendering code depending on semantic vkn APIs instead of raw `ash::vk` calls.
- Preserve current rendering behavior; validate each stage with `cargo check` and hidden app runs.

## Non-goals

- Do not redesign rendering algorithms, shader layouts, or asset formats during this pass.
- Do not chase performance changes unless needed to preserve behavior.

## Trace list

- [x] Create `agent/vkn-crate` branch.
- [x] Extract `src/vkn` into `crates/re-flora-vkn` and keep the app compiling.
- [x] Remove direct app dependency on `gpu-allocator`.
- [ ] Remove direct app dependency on `ash`.
- [ ] Wrap app-level raw swapchain/frame synchronization calls.
- [ ] Replace app/rendering usages of raw `vk::*` enums/flags with vkn semantic types or helpers.
- [ ] Audit direct `ash` imports outside `crates/re-flora-vkn` and either wrap or document deliberate low-level rendering-facing API.
- [ ] Validate with `cargo fmt --check`, `cargo check`, `cargo test`, and `cargo run --release -- --hidden --auto-exit 0.5`.
- [ ] Write final summary in `docs/`.

## Step notes

### Step 1: crate extraction

`crates/re-flora-vkn` now owns the previous vkn module code plus its small shader/resource helper traits. The game crate currently has a transitional root alias so existing `crate::vkn` imports continue to compile while subsequent steps clean public APIs and remove raw Vulkan leakage.

### Step 2: allocator ownership

`re-flora-vkn` now owns GPU allocator construction and memory-location mapping. The game crate creates allocators with `Allocator::new_for_context` and uses `MemoryLocation` from vkn, so it no longer depends directly on `gpu-allocator`.
