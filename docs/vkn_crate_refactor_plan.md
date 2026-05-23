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
- [x] Remove direct app dependency on `ash`.
- [x] Wrap app-level raw swapchain/frame synchronization calls.
- [ ] Replace app/rendering usages of raw `vk::*` enums/flags with vkn semantic types or helpers.
- [x] Audit direct `ash` imports outside `crates/re-flora-vkn` and remove them.
- [x] Move query pool, buffer fill, and fence polling/waiting raw calls behind vkn wrappers.
- [x] Audit remaining `re_flora_vkn::vk` uses outside the vkn crate and convert high-value raw command/handle cases to semantic helpers.
- [ ] Validate with `cargo fmt --check`, `cargo check`, `cargo test`, and `cargo run --release -- --hidden --auto-exit 0.5`.
- [ ] Write final summary in `docs/`.

## Step notes

### Step 1: crate extraction

`crates/re-flora-vkn` now owns the previous vkn module code plus its small shader/resource helper traits. The game crate currently has a transitional root alias so existing `crate::vkn` imports continue to compile while subsequent steps clean public APIs and remove raw Vulkan leakage.

### Step 2: allocator ownership

`re-flora-vkn` now owns GPU allocator construction and memory-location mapping. The game crate creates allocators with `Allocator::new_for_context` and uses `MemoryLocation` from vkn, so it no longer depends directly on `gpu-allocator`.

### Step 3: ash dependency removal

The game crate no longer depends on `ash` directly. Low-level Vulkan symbols that still appear outside vkn come through the vkn crate export and are the next cleanup target for semantic wrappers.

### Step 4: app frame abstraction

The top-level app no longer handles raw swapchain result codes, raw submit infos, raw fences, or raw screenshot image transitions. Vkn now owns present-mode conversion, frame acquire/present error mapping, render command submission, swapchain readback recording, and color readback conversion.

### Step 5: crate boundary cleanup

The transitional `crate::vkn` alias was removed. The derive macro now generates implementations against `re_flora_vkn` directly, and game modules import vkn APIs through the new crate name.

### Step 6: raw call wrappers

Timestamp query pools, buffer fill commands, and fence wait/poll operations moved behind vkn-owned wrappers. Builder modules no longer create/destroy query pools or issue raw fill/fence calls directly.

### Step 7: render command wrappers

Egui and tracer draw paths no longer call raw command-buffer/device methods or raw image-view handles from the game crate. Command-buffer helpers and framebuffer-from-textures keep the raw Vulkan handles inside vkn. Remaining `vk` references outside vkn are descriptor-style constants passed into vkn-owned descriptor structs (formats, usage flags, barriers, render pass settings), not direct backend calls.
