# vkn crate refactor summary

## Result

The Vulkan wrapper layer has been extracted from the game crate into `crates/re-flora-vkn`.
The game crate no longer depends directly on `ash`, `ash-window`, `gpu-allocator`, `shaderc`, or runtime `spirv-reflect`.

## What moved behind vkn

- Vulkan context, instance, device, surface, queue, swapchain, render pass, framebuffer, descriptor, pipeline, shader, sync, memory, texture, and RTX helpers now live in `re-flora-vkn`.
- GPU allocator creation is `Allocator::new_for_context`.
- Memory placement uses vkn's `MemoryLocation` instead of `gpu_allocator` types in game code.
- Present mode conversion, swapchain acquire/present error mapping, render submission, swapchain screenshot readback, and readback color conversion are vkn-owned.
- Frame sync and off-frame GPU jobs use vkn-managed frame/job tokens rather than app-owned raw fences or semaphores.
- Texture layouts, resource transitions, pipeline stages, and memory access flags use semantic vkn types at app/builder/tracer boundaries.
- Timestamp query pools and the current GPU profiler primitives are vkn-owned.
- Buffer fill, fence wait/poll, common render command buffer operations, and framebuffer creation from textures are vkn-owned helpers.
- The old transitional `crate::vkn` alias was removed; game modules import `re_flora_vkn` directly.

See also: `docs/vkn_profiler_architecture.md`.

## Remaining low-level surface

Some rendering modules still pass `re_flora_vkn::vk` descriptor constants into vkn descriptor structs, such as image formats, usage flags, descriptor types, shader stages, load/store ops, and clear/rect structs. These are descriptive render-resource or pipeline settings, not direct `ash` calls, raw backend handle operations, or synchronization/resource-state control.

No direct `ash::` usage remains outside `crates/re-flora-vkn`.

## Rendering portability note

On macOS/MoltenVK, `readonly uniform image*` GLSL declarations still map to storage-image descriptors and count against the small `maxPerStageDescriptorStorageImages` limit. Prefer sampled image/sampler declarations for read-only exact texel lookups:

```glsl
layout(set = ..., binding = ...) uniform usampler2D tex;
uint value = texelFetch(tex, uv, 0).r;
```

Keep true read-write or write-only outputs as storage images. Any texture sampled in a shader needs `SAMPLED` image usage in addition to any `STORAGE` or transfer usage required by other passes.

This rule fixed the earlier MoltenVK descriptor-limit errors in tracer/denoiser/VSM-adjacent paths.

## Recommended follow-up

Keep the refactor incremental. Do not do a broad rendering rewrite just to remove every remaining `vk::` reference. Instead, gradually narrow the public surface of `re-flora-vkn` as nearby rendering code is touched:

- Prefer small semantic vkn-owned descriptor types over passing raw `vk::*` constants from game code.
- Prioritize high-churn or high-risk areas first: render targets, pipeline setup, barriers/layout transitions, buffer/image usage roles, and render pass attachment descriptions.
- Good candidate wrappers include `ColorFormat`, `DepthFormat`, `BufferRole`, `ImageRole`, `RenderTargetDesc`, `PipelineDesc`, and `BarrierDesc`.
- Keep declarative resource setup behavior unchanged while moving Vulkan-specific enum/flag mapping into vkn.
- Treat new game-crate uses of raw `vk::` as suspicious unless they are temporarily needed for an explicit vkn API gap.

The intended direction is a stricter boundary: gameplay and renderer orchestration describe what they need; `re-flora-vkn` translates those descriptions into Vulkan details.

## Validation from the crate extraction pass

Completed successfully at the end of the extraction pass:

- `cargo fmt --check`
- `cargo check`
- `cargo test` — 55 tests passed
- `cargo run --release -- --hidden --auto-exit 0.5`

Hidden release run exited successfully and saved log:

`target/re-flora-logs/re-flora-20260523-151046.567-72342.log`
