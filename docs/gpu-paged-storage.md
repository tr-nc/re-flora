# Generic GPU paged storage

Implemented in `crates/re-flora-vkn/src/memory/paged_storage.rs`, paired with
`shader/slang/gpu_paged_storage.slang`. First consumer: shared model surfaces.

## Interface and scope

This module stores **records**, not models. It knows record count, stride,
alignment, device allocation constraints and submission lifetime. It knows
nothing about shapes, view directions, animation frames, materials or rebuild
policy. Those remain in `src/tracer/model_pixel_cache.rs`.

```text
GpuPagedStorage::new(context, allocator)
allocate(record_count, Layout) -> Result<GpuStorageAllocation>
begin_frame(completed_slot)
use_in_frame(slot, command_buffer, allocation, BufferUse)

allocation.handle() -> opaque shader handle
GPU: gpuStorageLoad<T>(handle, index) / gpuStorageStore<T>(handle, index, value)
```

`allocate` returns uninitialized storage. Initialize every record before reading.
Shader element type must fit the declared stride/alignment, and indices must be
less than the handle's logical length. Allocations and command buffers must use
the same device. This is structured-buffer-style storage, not an image allocator,
texture atlas, vertex/index allocator, global eviction cache or an offline loader.

## Hidden implementation

- A logical allocation is split into blocks with a preferred **64 MiB payload**.
  This is an allocation granularity, **not a total-size or per-kind budget**.
  The last block allocates only its actual records plus alignment allowance.
- `VkPhysicalDeviceMaintenance3Properties::maxMemoryAllocationSize` supplies the
  real per-allocation device constraint. Arithmetic, host page-table allocation,
  alignment and individual record size are checked before GPU allocation.
- Records never straddle blocks. Power-of-two records per block permit shift/mask
  addressing. Other strides work too; record stride is not fixed to model texels.
- A private page table contains aligned Vulkan buffer device addresses. The
  24-byte shader handle contains page-table address, logical length, stride and
  page shift; its CPU fields are opaque. Callers never calculate a block index,
  inspect its buffers or manage descriptor arrays.
- Payload access uses buffer device addresses, not a giant storage-buffer
  descriptor range. The renderer already requires `bufferDeviceAddress` and
  `shaderInt64`; this does not add descriptor-indexing requirements.
- `use_in_frame` explicitly registers the table and every underlying block with
  the command buffer's resource-state tracker. Pointer-reachable allocations
  cannot be discovered by shader descriptor reflection. The table is read-only
  even when a shader writes the payload.
- The same operation retains an allocation lease in the frame slot. The caller
  must invoke `begin_frame` **only after that slot's submission fence completes**.
  Replacing or dropping a caller's handle cannot free data still consumed by an
  older submission. Repeated uses within a slot share one residency lease.

There is no model-specific capacity ceiling, truncation, automatic quality
reduction, eviction or live-generation fallback. Capacity is still finite:
physical GPU/host memory, legal device allocations and address representation
remain real limits. Even the page table must fit a legal device allocation.
`Buffer::try_new_sized` propagates actual allocation/binding errors and cleans up
unpublished buffers. The model consumer allocates all replacements before
recording their generation or publishing new keys; allocation failure is explicit.
It tells the user to free GPU memory or reduce requested inputs, rather than
silently selecting a different renderer.

`resident_bytes` reports buffer sizes including alignment allowance and the page
table, not the Vulkan allocator's larger backing heaps or driver-reserved memory.

## First consumer

The model module owns immutable source IDs, 64 leaf variants, one apple shape,
32 butterfly articulation poses, view selection, resolutions and invalidation.
It asks storage for N surface records and passes an opaque handle to the generator.
The same handle reaches the lookup shader through per-frame metadata. Startup
and settings changes run the sole generator; ordinary frames only read stored
surfaces and relight them.

The old `model_pixel_cache` setting is retired for both saved boolean values.
There is no normal per-instance geometry-generation branch and no 128 MiB budget.
The internal `RE_FLORA_MODEL_CACHE_REVIEW` oracle invokes the generator only to
compare stored bits, never as displayed data or as a resource-pressure fallback.
The older continuous numerical diagnostics remain test-only, not selectable
rendering alternatives.

Other prospective consumers (immutable geometry or precomputed tables) can use
this storage without adding concepts to it. No other renderer was migrated
speculatively. An offline surface loader would eventually add a staging-upload
operation at this storage seam; there is no speculative plugin/file framework.

## Validation

- `cargo check` and `cargo fmt --check` passed; generated shader/GUI structs were
  regenerated, not hand-edited.
- `cargo test`: **1106 passed, 4 ignored**.
- `cargo test -p re-flora-vkn`: **45 passed**, including four storage tests for
  multi-GiB planning, block boundaries/tails, errors/overflow and frame residency.
  The first full library run found a pre-existing stale particle shader name;
  `34ef7d91` corrects only that test's path to the already-current shader.
- `node scripts/validate-apple-model.mjs --cache` under hidden muted Release:
  bit-identical stored/generator surface records for all three kinds. It verifies
  reuse, isolated rebuilds, 256 MiB leaf storage across four blocks, 128 MiB
  butterfly storage across two blocks, replacement, fruit drops and native resize.
  The broader sweep also exercised 296 MiB leaf storage across five blocks.
- A 1,100-leaf / 64px run exercised both storage blocks and the existing transient
  RGBA-tile batch boundary. No Vulkan errors or stored-bit mismatches.
- The original strict native leaf/butterfly GPU/CPU oracles passed, as did a final
  hidden muted Release smoke. No visible game was launched automatically.

Runtime validation was on Linux / RTX 3060 Ti. Maximum 512-view/64px configurations
would request several GiB and were checked arithmetically, not physically allocated
to exhaustion. Windows and MoltenVK were not exercised. No claim of infinite
physical capacity or benchmark-optimal block size is made.

See [model cache results](performance/model-pixel-cache.md) for commands, artifacts
and measured Release cadence/GPU timings.
